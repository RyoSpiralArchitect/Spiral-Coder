use super::*;

pub(crate) fn mistral_name_embedded_blocks_and_tool(
    tc: &ToolCallData,
) -> Option<(Vec<String>, Option<ToolCallData>)> {
    let raw = tc.name.trim();
    let low = raw.to_ascii_lowercase();
    if !(low.starts_with("plan>")
        || low.contains("<plan>")
        || low.contains("<think>")
        || low.contains("</plan>")
        || low.contains("</think>"))
    {
        return None;
    }

    let mut blocks = Vec::new();
    let mut rest = raw;
    let mut tool_from_think: Option<String> = None;

    if low.starts_with("plan>")
        && !low.contains("<think>")
        && !low.contains("<goal>")
        && !low.contains("<steps>")
        && !low.contains("<acceptance")
        && !low.contains("<risks>")
        && !low.contains("<assumptions>")
    {
        if let Some(block) = inline_block_from_name(raw) {
            if parse_plan_block(&block).is_some() {
                blocks.push(block);
                if let Some(plan_end) = low.find("</plan>") {
                    rest = &raw[plan_end + "</plan>".len()..];
                } else {
                    rest = "";
                }
            }
        }
    }

    if blocks.is_empty() {
        if let Some(after_plan_prefix) = raw.strip_prefix("plan>") {
            let after_low = after_plan_prefix.to_ascii_lowercase();
            if let Some(end) = after_low.find("</plan>") {
                let plan_chunk = &after_plan_prefix[..end + "</plan>".len()];
                let block = format!("<plan>{plan_chunk}");
                if parse_plan_block(&block).is_some() {
                    blocks.push(block);
                    rest = &after_plan_prefix[end + "</plan>".len()..];
                }
            }
        }
    }

    if blocks.is_empty() {
        if let Some(plan_body) = extract_tag_block(raw, "plan") {
            let block = format!("<plan>\n{plan_body}\n</plan>");
            if parse_plan_block(&block).is_some() {
                blocks.push(block);
                if let Some(plan_end) = low.find("</plan>") {
                    rest = &raw[plan_end + "</plan>".len()..];
                }
            }
        }
    }

    let think_search = if rest == raw { raw } else { rest };
    if let Some(think_body) = extract_tag_block(think_search, "think") {
        let block = format!("<think>\n{think_body}\n</think>");
        if let Some(think) = parse_think_block(&block) {
            if !think.tool.trim().is_empty() {
                tool_from_think = Some(think.tool.clone());
            }
            blocks.push(block);
            if let Some(think_end) = think_search.to_ascii_lowercase().find("</think>") {
                rest = &think_search[think_end + "</think>".len()..];
            }
        }
    }

    if blocks.is_empty() {
        return None;
    }

    if let Some((mut prelude, normalized)) = mistral_nested_tool_payload(tc) {
        blocks.append(&mut prelude);
        return Some((blocks, Some(normalized)));
    }

    let normalized = tool_from_think
        .map(|tool| ToolCallData {
            thought_signature: None,
            id: tc.id.clone(),
            name: tool,
            arguments: tc.arguments.clone(),
        })
        .or_else(|| {
            known_runtime_tool_name_from_text(rest).map(|tool| ToolCallData {
                thought_signature: None,
                id: tc.id.clone(),
                name: tool,
                arguments: tc.arguments.clone(),
            })
        });

    Some((blocks, normalized))
}

pub(crate) fn inline_block_from_name(name: &str) -> Option<String> {
    let trimmed = name.trim();
    let (tag, rest, fields, close_tag) = if let Some(rest) = trimmed.strip_prefix("plan>") {
        (
            "plan",
            rest.trim(),
            [
                "goal:",
                "steps:",
                "acceptance:",
                "acceptance_criteria:",
                "risks:",
                "assumptions:",
            ]
            .as_slice(),
            "</plan>",
        )
    } else if let Some(rest) = trimmed.strip_prefix("think>") {
        (
            "think",
            rest.trim(),
            [
                "goal:", "step:", "tool:", "risk:", "doubt:", "next:", "verify:",
            ]
            .as_slice(),
            "</think>",
        )
    } else {
        return None;
    };

    let mut body = rest.to_string();
    for field in fields.iter().skip(1) {
        body = body.replace(field, &format!("\n{field}"));
    }
    body = body.replace(close_tag, &format!("\n{close_tag}"));
    let body = body.trim();
    if body.is_empty() {
        return None;
    }
    Some(format!("<{tag}>\n{body}"))
}

pub(crate) fn markdownish_block_fields(raw: &str) -> Option<String> {
    let mut body = raw
        .replace("**•**", "\n")
        .replace('•', "\n")
        .replace("**", "")
        .replace('`', "")
        .replace("</thinking>", "")
        .replace("</plan>", "")
        .replace("</think>", "");
    body = compact_multiline_block(body.as_str());
    let body = body.trim();
    if body.is_empty() {
        None
    } else {
        Some(body.to_string())
    }
}

pub(crate) fn markdownish_mistral_blocks_from_name(name: &str) -> Vec<String> {
    let trimmed = name.trim();
    let low = trimmed.to_ascii_lowercase();
    if !low.starts_with("plan") {
        return Vec::new();
    }
    let looks_markdownish = trimmed.contains("**")
        || trimmed.contains('•')
        || low.contains("**goal")
        || low.contains("**steps")
        || low.contains("**acceptance")
        || low.contains("<thinking>");
    if !looks_markdownish {
        return Vec::new();
    }

    let plan_raw = if let Some((plan, _think)) = trimmed.split_once("<thinking>") {
        plan
    } else {
        trimmed
    };
    let mut blocks = Vec::new();
    let mut plan_body = plan_raw.trim().to_string();
    if plan_body.to_ascii_lowercase().starts_with("plan") {
        plan_body = plan_body[4..]
            .trim_start_matches(|c: char| c == ':' || c == '>' || c.is_whitespace())
            .to_string();
    }
    if let Some(body) = markdownish_block_fields(plan_body.as_str()) {
        if parse_plan_block(format!("<plan>\n{body}\n</plan>").as_str()).is_some() {
            blocks.push(format!("<plan>\n{body}\n</plan>"));
        }
    }

    blocks
}

pub(crate) fn mistral_nested_tool_payload(
    tc: &ToolCallData,
) -> Option<(Vec<String>, ToolCallData)> {
    let value: serde_json::Value = serde_json::from_str(&tc.arguments).ok()?;
    let obj = value.as_object()?;
    let mut prelude = Vec::new();
    if let Some(think) = obj.get("think").and_then(|v| v.as_str()) {
        let think = think.trim();
        if !think.is_empty() {
            prelude.push(format!("<think>\n{think}\n</think>"));
        }
    }
    let tool_name = obj
        .get("tool")
        .and_then(|v| v.as_str())
        .or_else(|| {
            obj.get("function")
                .and_then(|v| v.get("name"))
                .and_then(|v| v.as_str())
        })?
        .trim();
    let tool_args = obj
        .get("arguments")
        .or_else(|| obj.get("function").and_then(|v| v.get("parameters")))?;
    if tool_name.is_empty() {
        return None;
    }
    Some((
        prelude,
        ToolCallData {
            thought_signature: None,
            id: tc.id.clone(),
            name: tool_name.to_string(),
            arguments: tool_args.to_string(),
        },
    ))
}

pub(crate) fn split_concatenated_json_values(raw: &str) -> Vec<serde_json::Value> {
    let mut out = Vec::new();
    let de = serde_json::Deserializer::from_str(raw).into_iter::<serde_json::Value>();
    for value in de {
        match value {
            Ok(v) => out.push(v),
            Err(_) => return Vec::new(),
        }
    }
    out
}

pub(crate) fn normalize_mistral_tool_call(
    tc: &ToolCallData,
) -> (Vec<String>, Option<ToolCallData>) {
    if let Some(block) = pseudo_tool_call_to_block_text(tc) {
        return (vec![block], None);
    }
    if let Some((blocks, normalized)) = mistral_name_embedded_blocks_and_tool(tc) {
        return (blocks, normalized);
    }
    let markdownish_blocks = markdownish_mistral_blocks_from_name(&tc.name);
    if !markdownish_blocks.is_empty() {
        if let Some((mut prelude, normalized)) = mistral_nested_tool_payload(tc) {
            let mut blocks = markdownish_blocks;
            blocks.append(&mut prelude);
            return (blocks, Some(normalized));
        }
        return (markdownish_blocks, None);
    }
    if let Some(block) = inline_block_from_name(&tc.name) {
        if let Some((mut prelude, normalized)) = mistral_nested_tool_payload(tc) {
            prelude.insert(0, block);
            return (prelude, Some(normalized));
        }
        return (vec![block], None);
    }

    let mut prelude = Vec::new();
    let values = split_concatenated_json_values(&tc.arguments);
    if values.len() > 1 {
        for value in values.iter().take(values.len().saturating_sub(1)) {
            if let Some(block) = pseudo_block_text_from_named_args("plan", value)
                .or_else(|| pseudo_block_text_from_named_args("think", value))
            {
                prelude.push(block);
            }
        }

        if let Some(last) = values.last() {
            return (
                prelude,
                Some(ToolCallData {
                    thought_signature: None,
                    id: tc.id.clone(),
                    name: tc.name.trim_matches(|c| c == '<' || c == '>').to_string(),
                    arguments: last.to_string(),
                }),
            );
        }
    }

    (
        prelude,
        Some(ToolCallData {
            thought_signature: None,
            id: tc.id.clone(),
            name: tc.name.trim_matches(|c| c == '<' || c == '>').to_string(),
            arguments: tc.arguments.clone(),
        }),
    )
}

pub(crate) fn build_mistral_think_only_hint(root_user_text: &str, think: &ThinkBlock) -> String {
    let pattern = preferred_read_only_search_pattern(root_user_text);
    let dir = preferred_read_only_search_dir(root_user_text);
    let read_path = preferred_read_only_read_path_hint(root_user_text)
        .unwrap_or("<matching file from the latest successful search>");
    let tool = think.tool.trim();
    let suggested = match tool {
        "search_files" => format!("search_files(pattern=\"{pattern}\", dir=\"{dir}\")"),
        "list_dir" => format!("list_dir(dir=\"{dir}\")"),
        "glob" => format!("glob(pattern=\"*{pattern}*\", dir=\"{dir}\")"),
        "read_file" => format!("read_file(path=\"{read_path}\")"),
        _ => format!("search_files(pattern=\"{pattern}\", dir=\"{dir}\")"),
    };

    format!(
        "[Mistral compatibility]\n\
`think` is not a real tool call.\n\
Next assistant turn:\n\
1) emit the <think> block as plain text\n\
2) call ONE real tool immediately after it\n\
Suggested real tool for this task: {suggested}\n\
Do NOT emit a `think` tool_call again."
    )
}

pub(crate) fn mistral_observation_tool(name: &str) -> bool {
    matches!(
        name,
        "read_file" | "search_files" | "list_dir" | "glob" | "done"
    )
}
