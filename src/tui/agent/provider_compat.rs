use super::task_harness::{ArtifactMode, TaskHarness};
use super::*;

fn mistral_name_embedded_blocks_and_tool(
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

fn inline_block_from_name(name: &str) -> Option<String> {
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

fn markdownish_block_fields(raw: &str) -> Option<String> {
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

fn markdownish_mistral_blocks_from_name(name: &str) -> Vec<String> {
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

fn mistral_nested_tool_payload(tc: &ToolCallData) -> Option<(Vec<String>, ToolCallData)> {
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

fn split_concatenated_json_values(raw: &str) -> Vec<serde_json::Value> {
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

pub(super) fn normalize_mistral_tool_call(
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

pub(super) fn build_mistral_think_only_hint(root_user_text: &str, think: &ThinkBlock) -> String {
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

pub(super) fn mistral_observation_tool(name: &str) -> bool {
    matches!(
        name,
        "read_file" | "search_files" | "list_dir" | "glob" | "done"
    )
}

fn consecutive_missing_plan_blocks_for_diagnostic_tools(messages: &[serde_json::Value]) -> usize {
    let mut count = 0usize;
    let mut idx = messages.len();
    while idx >= 2 {
        let tool_msg = &messages[idx - 1];
        let assistant_msg = &messages[idx - 2];
        if !is_missing_plan_governor_block(tool_msg) {
            break;
        }
        let Some(name) = assistant_blocked_tool_name(assistant_msg) else {
            break;
        };
        if !is_diagnostic_tool_name(name.as_str()) {
            break;
        }
        count = count.saturating_add(1);
        idx -= 2;
    }
    count
}

fn consecutive_missing_think_blocks_for_diagnostic_tools(messages: &[serde_json::Value]) -> usize {
    let mut count = 0usize;
    let mut idx = messages.len();
    while idx >= 2 {
        let tool_msg = &messages[idx - 1];
        let assistant_msg = &messages[idx - 2];
        if !is_missing_think_governor_block(tool_msg) {
            break;
        }
        let Some(name) = assistant_blocked_tool_name(assistant_msg) else {
            break;
        };
        if !is_diagnostic_tool_name(name.as_str()) {
            break;
        }
        count = count.saturating_add(1);
        idx -= 2;
    }
    count
}

fn synthetic_action_plan_goal(root_user_text: &str) -> String {
    let goal = compact_one_line(root_user_text.trim(), 220);
    if goal.is_empty() {
        "Complete the requested task safely and verify the result.".to_string()
    } else {
        goal
    }
}

fn synthetic_action_plan_first_step(tc: &ToolCallData, task_harness: TaskHarness) -> String {
    match task_harness.artifact_mode {
        ArtifactMode::NewFiles => {
            if let Some(command) =
                canonicalize_tool_call_command(tc.name.as_str(), tc.arguments.as_str())
            {
                return format!("{command} to create the requested file now");
            }
            return "create the requested file now with write_file or a minimal exec".to_string();
        }
        ArtifactMode::NewRepo => {
            if let Some(command) =
                canonicalize_tool_call_command(tc.name.as_str(), tc.arguments.as_str())
            {
                return format!("{command} to create the requested repo/project artifact now");
            }
            return "create the requested repo/project artifact now with write_file or exec"
                .to_string();
        }
        ArtifactMode::ObserveOnly | ArtifactMode::ExistingFiles => {}
    }

    if let Some(command) = canonicalize_tool_call_command(tc.name.as_str(), tc.arguments.as_str()) {
        match tc.name.as_str() {
            "read_file" | "search_files" | "list_dir" | "glob" => {
                return format!(
                    "{command} to inspect the current target and confirm the relevant context"
                );
            }
            _ => {
                return format!("{command} to gather the smallest safe evidence first");
            }
        }
    }

    match tc.name.as_str() {
        "read_file" => "read the current target file and confirm the edit context".to_string(),
        "search_files" => {
            "search for the current target and confirm the strongest match".to_string()
        }
        "list_dir" => "list the relevant directory and confirm the likely target files".to_string(),
        "glob" => "glob for the likely target files before editing".to_string(),
        _ => format!("use {} to gather the smallest safe evidence first", tc.name),
    }
}

fn synthetic_action_plan_verify_step(
    required_verification: VerificationLevel,
    test_cmd: Option<&str>,
) -> String {
    match required_verification {
        VerificationLevel::Behavioral => {
            let command = test_cmd
                .map(|cmd| compact_one_line(cmd, 160))
                .filter(|cmd| !cmd.is_empty())
                .unwrap_or_else(|| "run the targeted behavioral verification command".to_string());
            format!("{command} and confirm the requested behavior now passes")
        }
        VerificationLevel::Build => {
            let command = verification_examples(VerificationLevel::Build);
            if command.trim().is_empty() {
                "run a real build/check/lint verification command and confirm it passes".to_string()
            } else {
                format!("{command} and confirm the requested scope still builds cleanly")
            }
        }
    }
}

fn synthetic_action_plan_acceptance(
    required_verification: VerificationLevel,
    task_harness: TaskHarness,
) -> Vec<String> {
    if matches!(
        task_harness.artifact_mode,
        ArtifactMode::NewFiles | ArtifactMode::NewRepo
    ) {
        return match required_verification {
            VerificationLevel::Behavioral => vec![
                "the requested artifact is created and confirmed by a passing behavioral verification command"
                    .to_string(),
                "the final result cites the exact passing behavioral verification command"
                    .to_string(),
            ],
            VerificationLevel::Build => vec![
                "the requested artifact is created and confirmed by a passing build/check/lint verification command"
                    .to_string(),
                "the final result cites the exact passing build/check/lint verification command"
                    .to_string(),
            ],
        };
    }

    match required_verification {
        VerificationLevel::Behavioral => vec![
            "the requested change is implemented and confirmed by a passing behavioral verification command"
                .to_string(),
            "the final result cites the exact passing behavioral verification command".to_string(),
        ],
        VerificationLevel::Build => vec![
            "the requested change is implemented and confirmed by a passing build/check/lint verification command"
                .to_string(),
            "the final result cites the exact passing build/check/lint verification command"
                .to_string(),
        ],
    }
}

fn synthetic_action_plan_risks(
    required_verification: VerificationLevel,
    task_harness: TaskHarness,
) -> String {
    if matches!(
        task_harness.artifact_mode,
        ArtifactMode::NewFiles | ArtifactMode::NewRepo
    ) {
        return match required_verification {
            VerificationLevel::Behavioral => {
                "wrong path or incomplete artifact; the chosen test may miss the requested contents"
                    .to_string()
            }
            VerificationLevel::Build => {
                "wrong path or incomplete artifact; build-only verification may miss the requested contents"
                    .to_string()
            }
        };
    }

    match required_verification {
        VerificationLevel::Behavioral => {
            "wrong file or speculative fix; the chosen test may miss the requested behavior"
                .to_string()
        }
        VerificationLevel::Build => {
            "wrong file or speculative fix; build-only verification may miss the requested behavior"
                .to_string()
        }
    }
}

pub(super) fn synthetic_action_plan(
    root_user_text: &str,
    tc: &ToolCallData,
    task_harness: TaskHarness,
    required_verification: VerificationLevel,
    test_cmd: Option<&str>,
) -> PlanBlock {
    let steps = match task_harness.artifact_mode {
        ArtifactMode::NewFiles => vec![
            synthetic_action_plan_first_step(tc, task_harness),
            synthetic_action_plan_verify_step(required_verification, test_cmd),
            "call done with the created file path and the verified outcome".to_string(),
        ],
        ArtifactMode::NewRepo => vec![
            synthetic_action_plan_first_step(tc, task_harness),
            "create any minimal companion files or directories required by the request".to_string(),
            synthetic_action_plan_verify_step(required_verification, test_cmd),
            "call done with the created repo/project path and the verified outcome".to_string(),
        ],
        ArtifactMode::ObserveOnly | ArtifactMode::ExistingFiles => vec![
            synthetic_action_plan_first_step(tc, task_harness),
            "read the strongest candidate file and confirm the exact edit context".to_string(),
            "apply the smallest evidence-backed code change in the confirmed target".to_string(),
            synthetic_action_plan_verify_step(required_verification, test_cmd),
            "call done with the verified outcome and any remaining gaps".to_string(),
        ],
    };

    PlanBlock {
        goal: synthetic_action_plan_goal(root_user_text),
        steps,
        acceptance_criteria: synthetic_action_plan_acceptance(required_verification, task_harness),
        risks: synthetic_action_plan_risks(required_verification, task_harness),
        assumptions:
            "the current repo state matches the task and the configured verification command is relevant"
                .to_string(),
    }
}

fn supports_action_task_rescues(provider: &ProviderKind) -> bool {
    matches!(
        provider,
        ProviderKind::OpenAiCompatible | ProviderKind::Mistral
    )
}

fn extract_loose_json_string_field(raw: &str, field: &str) -> Option<(String, bool)> {
    let needle = format!("\"{field}\"");
    let start = raw.find(needle.as_str())?;
    let after_key = &raw[start + needle.len()..];
    let colon = after_key.find(':')?;
    let mut tail = after_key[colon + 1..].trim_start();
    if !tail.starts_with('"') {
        return None;
    }
    tail = &tail[1..];

    let mut out = String::new();
    let mut chars = tail.chars();
    let mut escaped = false;
    while let Some(ch) = chars.next() {
        if escaped {
            match ch {
                '"' => out.push('"'),
                '\\' => out.push('\\'),
                '/' => out.push('/'),
                'n' => out.push('\n'),
                'r' => out.push('\r'),
                't' => out.push('\t'),
                'b' => out.push('\u{0008}'),
                'f' => out.push('\u{000C}'),
                'u' => {
                    let mut hex = String::new();
                    for _ in 0..4 {
                        let Some(h) = chars.next() else {
                            return Some((out, false));
                        };
                        hex.push(h);
                    }
                    if let Ok(value) = u16::from_str_radix(hex.as_str(), 16) {
                        if let Some(decoded) = char::from_u32(value as u32) {
                            out.push(decoded);
                        }
                    }
                }
                other => out.push(other),
            }
            escaped = false;
            continue;
        }
        match ch {
            '\\' => escaped = true,
            '"' => return Some((out, true)),
            other => out.push(other),
        }
    }

    Some((out, false))
}

fn extract_first_quoted_text(text: &str) -> Option<String> {
    let start = text.find('"')?;
    let mut out = String::new();
    let mut chars = text[start + 1..].chars();
    let mut escaped = false;
    while let Some(ch) = chars.next() {
        if escaped {
            out.push(match ch {
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                other => other,
            });
            escaped = false;
            continue;
        }
        match ch {
            '\\' => escaped = true,
            '"' => return Some(out),
            other => out.push(other),
        }
    }
    None
}

fn recent_assertion_string_mismatch(messages: &[serde_json::Value]) -> Option<(String, String)> {
    for msg in messages.iter().rev() {
        let Some(content) = msg.get("content").and_then(|value| value.as_str()) else {
            continue;
        };
        let mut left: Option<String> = None;
        let mut right: Option<String> = None;
        for line in content.lines() {
            let trimmed = line.trim();
            if left.is_none() && trimmed.starts_with("left: ") {
                left = extract_first_quoted_text(trimmed);
            }
            if right.is_none() && trimmed.starts_with("right: ") {
                right = extract_first_quoted_text(trimmed);
            }
        }
        if let (Some(left), Some(right)) = (left, right) {
            return Some((left, right));
        }
    }
    None
}

fn mismatch_segment(left: &str, right: &str) -> Option<(String, String)> {
    if left == right {
        return None;
    }

    let left_chars: Vec<char> = left.chars().collect();
    let right_chars: Vec<char> = right.chars().collect();
    let mut prefix = 0usize;
    while prefix < left_chars.len()
        && prefix < right_chars.len()
        && left_chars[prefix] == right_chars[prefix]
    {
        prefix += 1;
    }

    let mut suffix = 0usize;
    while suffix + prefix < left_chars.len()
        && suffix + prefix < right_chars.len()
        && left_chars[left_chars.len() - 1 - suffix] == right_chars[right_chars.len() - 1 - suffix]
    {
        suffix += 1;
    }

    let left_mid = left_chars[prefix..left_chars.len().saturating_sub(suffix)]
        .iter()
        .collect::<String>();
    let right_mid = right_chars[prefix..right_chars.len().saturating_sub(suffix)]
        .iter()
        .collect::<String>();
    if left_mid.is_empty() && right_mid.is_empty() {
        None
    } else {
        Some((left_mid, right_mid))
    }
}

pub(super) fn repair_truncated_patch_tool_call_from_recent_mismatch(
    messages: &[serde_json::Value],
    tc: &ToolCallData,
    root_read_only: bool,
    goal_wants_actions: bool,
    provider: ProviderKind,
    observations: &ObservationEvidence,
) -> Option<(ToolCallData, String, String)> {
    if root_read_only || !goal_wants_actions || !tc.can_rewrite() {
        return None;
    }
    if !matches!(provider, ProviderKind::OpenAiCompatible) || tc.name != "patch_file" {
        return None;
    }

    let parsed = serde_json::from_str::<serde_json::Value>(&tc.arguments).ok();
    let parsed_path = parsed
        .as_ref()
        .and_then(|value| value.get("path").and_then(|v| v.as_str()))
        .map(str::to_string);
    let parsed_search = parsed
        .as_ref()
        .and_then(|value| value.get("search").and_then(|v| v.as_str()))
        .map(str::to_string);
    let parsed_replace = parsed
        .as_ref()
        .and_then(|value| value.get("replace").and_then(|v| v.as_str()))
        .map(str::to_string);
    if parsed_path.as_deref().unwrap_or("").trim().len() > 0
        && parsed_search.as_deref().unwrap_or("").trim().len() > 0
        && parsed_replace.as_deref().unwrap_or("").trim().len() > 0
    {
        return None;
    }

    let path = parsed_path.or_else(|| {
        extract_loose_json_string_field(tc.arguments.as_str(), "path").map(|(value, _)| value)
    })?;
    let search = parsed_search.or_else(|| {
        extract_loose_json_string_field(tc.arguments.as_str(), "search").map(|(value, _)| value)
    })?;
    if path.trim().is_empty()
        || search.trim().is_empty()
        || !observation_supports_target_path(path.as_str(), observations)
    {
        return None;
    }

    let replace_state = parsed_replace
        .map(|value| (value, true))
        .or_else(|| extract_loose_json_string_field(tc.arguments.as_str(), "replace"));
    if let Some((replace, closed)) = replace_state.as_ref() {
        if *closed && !replace.trim().is_empty() {
            return None;
        }
    }

    let (left, right) = recent_assertion_string_mismatch(messages)?;
    let repaired_replace = if search.contains(left.as_str()) {
        search.replacen(left.as_str(), right.as_str(), 1)
    } else if let Some((old, new)) = mismatch_segment(left.as_str(), right.as_str()) {
        if old.is_empty() || old.len() > 8 || search.matches(old.as_str()).count() != 1 {
            return None;
        }
        search.replacen(old.as_str(), new.as_str(), 1)
    } else {
        return None;
    };
    if repaired_replace == search {
        return None;
    }

    let rewritten = ToolCallData {
        thought_signature: None,
        id: tc.id.clone(),
        name: tc.name.clone(),
        arguments: json!({
            "path": path,
            "search": search,
            "replace": repaired_replace,
        })
        .to_string(),
    };
    let original = canonicalize_tool_call_command(tc.name.as_str(), tc.arguments.as_str())
        .unwrap_or_else(|| blocked_tool_call_signature(tc.name.as_str(), tc.arguments.as_str()));
    let coerced =
        canonicalize_tool_call_command(rewritten.name.as_str(), rewritten.arguments.as_str())
            .unwrap_or_else(|| {
                blocked_tool_call_signature(rewritten.name.as_str(), rewritten.arguments.as_str())
            });
    if original == coerced {
        None
    } else {
        Some((rewritten, original, coerced))
    }
}

pub(super) fn rescue_missing_plan_for_tool_turn(
    messages: &[serde_json::Value],
    tc: &ToolCallData,
    root_user_text: &str,
    task_harness: TaskHarness,
    root_read_only: bool,
    goal_wants_actions: bool,
    provider: ProviderKind,
    required_verification: VerificationLevel,
    test_cmd: Option<&str>,
) -> Option<PlanBlock> {
    if root_read_only || !goal_wants_actions {
        return None;
    }
    if !supports_action_task_rescues(&provider) {
        return None;
    }
    let artifact_creation_turn = matches!(
        task_harness.artifact_mode,
        ArtifactMode::NewFiles | ArtifactMode::NewRepo
    ) && matches!(tc.name.as_str(), "write_file" | "exec");
    if !is_diagnostic_tool_name(tc.name.as_str()) && !artifact_creation_turn {
        return None;
    }

    let prior_blocks = consecutive_missing_plan_blocks_for_tool(messages, tc);
    let prior_diagnostic_blocks = consecutive_missing_plan_blocks_for_diagnostic_tools(messages);
    if artifact_creation_turn {
        if prior_blocks.saturating_add(1) < 2 {
            return None;
        }
    } else if matches!(provider, ProviderKind::Mistral) {
        if prior_blocks.saturating_add(1) < 2 && prior_diagnostic_blocks.saturating_add(1) < 2 {
            return None;
        }
    } else if prior_blocks.saturating_add(1) < 2 && prior_diagnostic_blocks.saturating_add(1) < 2 {
        return None;
    }

    Some(synthetic_action_plan(
        root_user_text,
        tc,
        task_harness,
        required_verification,
        test_cmd,
    ))
}

pub(super) fn rescue_read_only_missing_plan_for_tool_turn(
    messages: &[serde_json::Value],
    tc: &ToolCallData,
    root_user_text: &str,
    root_read_only: bool,
    provider: ProviderKind,
) -> Option<PlanBlock> {
    if !root_read_only || !mistral_observation_tool(tc.name.as_str()) {
        return None;
    }
    let prior_blocks = consecutive_missing_plan_blocks_for_tool(messages, tc);
    let prior_observation_blocks = consecutive_missing_plan_blocks_for_observation(messages);
    if prior_blocks.saturating_add(1) < 2 && prior_observation_blocks.saturating_add(1) < 3 {
        return None;
    }
    if matches!(provider, ProviderKind::Mistral) {
        if let Some(repaired) =
            repair_mistral_plan_for_tool_turn(None, None, tc, root_user_text, root_read_only)
        {
            return Some(repaired);
        }
    }
    Some(synthetic_read_only_observation_plan(root_user_text))
}

pub(super) fn rescue_read_only_missing_think_for_tool_turn(
    messages: &[serde_json::Value],
    tc: &ToolCallData,
    plan: &PlanBlock,
    root_user_text: &str,
    root_read_only: bool,
    provider: ProviderKind,
    evidence: &ObservationEvidence,
) -> Option<ThinkBlock> {
    if !root_read_only || !mistral_observation_tool(tc.name.as_str()) {
        return None;
    }
    if tc.name == "read_file" {
        return Some(compat_synthetic_think(tc, plan));
    }
    if tc.name == "done"
        && build_read_only_completion_hint(
            root_user_text,
            plan,
            evidence,
            messages,
            &WorkingMemory::default(),
        )
        .is_some()
    {
        return Some(compat_synthetic_think(tc, plan));
    }
    if matches!(provider, ProviderKind::Mistral) {
        return Some(compat_synthetic_think(tc, plan));
    }
    let prior_blocks = consecutive_missing_think_blocks_for_tool(messages, tc);
    let prior_observation_blocks = consecutive_missing_think_blocks_for_observation(messages);
    if prior_blocks.saturating_add(1) < 2 && prior_observation_blocks.saturating_add(1) < 2 {
        return None;
    }
    Some(compat_synthetic_think(tc, plan))
}

pub(super) fn rescue_missing_think_for_tool_turn(
    messages: &[serde_json::Value],
    tc: &ToolCallData,
    plan: &PlanBlock,
    root_read_only: bool,
    goal_wants_actions: bool,
    provider: ProviderKind,
    force: bool,
) -> Option<ThinkBlock> {
    if root_read_only || !goal_wants_actions {
        return None;
    }
    if !supports_action_task_rescues(&provider) {
        return None;
    }
    if matches!(
        tc.name.as_str(),
        "exec" | "write_file" | "patch_file" | "apply_diff" | "done"
    ) {
        return Some(compat_synthetic_think(tc, plan));
    }
    if tc.name == "read_file" {
        return Some(compat_synthetic_think(tc, plan));
    }
    if !is_diagnostic_tool_name(tc.name.as_str()) {
        return None;
    }
    if matches!(provider, ProviderKind::Mistral) {
        return Some(compat_synthetic_think(tc, plan));
    }
    if force {
        return Some(compat_synthetic_think(tc, plan));
    }

    let prior_blocks = consecutive_missing_think_blocks_for_tool(messages, tc);
    let prior_diagnostic_blocks = consecutive_missing_think_blocks_for_diagnostic_tools(messages);
    if prior_blocks.saturating_add(1) < 2 && prior_diagnostic_blocks.saturating_add(1) < 2 {
        return None;
    }

    Some(compat_synthetic_think(tc, plan))
}

pub(super) fn rescue_missing_reflection_for_tool_turn(
    reason: &str,
    tc: &ToolCallData,
    root_read_only: bool,
    goal_wants_actions: bool,
    provider: ProviderKind,
    mem: &FailureMemory,
    file_tool_consec_failures: usize,
) -> Option<ReflectionBlock> {
    if root_read_only || !goal_wants_actions {
        return None;
    }
    if !supports_action_task_rescues(&provider) {
        return None;
    }

    let reason_low = reason.to_ascii_lowercase();
    let last_outcome = if mem.consecutive_failures > 0
        || reason_low.contains("failure")
        || reason_low.contains("error")
        || reason_low.contains("wrong results")
    {
        "failure"
    } else {
        "partial"
    };
    let repeated_failure = mem.repeated_failure_or_stall() || file_tool_consec_failures >= 2;
    let strategy_change = if repeated_failure {
        StrategyChange::Abandon
    } else {
        StrategyChange::Adjust
    };
    let wrong_assumption = match tc.name.as_str() {
        "exec" => "verification happened before the fix was confirmed",
        "patch_file" | "apply_diff" | "write_file" => {
            "the intended edit was not yet precise enough"
        }
        "read_file" => "the current evidence was not specific enough",
        "search_files" | "list_dir" | "glob" => "discovery was still too broad",
        _ => "the previous action did not match the shortest safe next step",
    }
    .to_string();
    let next_minimal_action = match tc.name.as_str() {
        "exec" => "inspect the failing source before rerunning tests".to_string(),
        "patch_file" | "apply_diff" => serde_json::from_str::<serde_json::Value>(&tc.arguments)
            .ok()
            .and_then(|v| {
                v.get("path")
                    .and_then(|x| x.as_str())
                    .map(|path| format!("patch {path} with the smallest confirmed fix"))
            })
            .unwrap_or_else(|| "apply the smallest confirmed code fix".to_string()),
        "write_file" => serde_json::from_str::<serde_json::Value>(&tc.arguments)
            .ok()
            .and_then(|v| {
                v.get("path")
                    .and_then(|x| x.as_str())
                    .map(|path| format!("rewrite {path} only if replacement is necessary"))
            })
            .unwrap_or_else(|| "rewrite only the confirmed target file".to_string()),
        "read_file" => serde_json::from_str::<serde_json::Value>(&tc.arguments)
            .ok()
            .and_then(|v| {
                v.get("path")
                    .and_then(|x| x.as_str())
                    .map(|path| format!("read {path} and inspect the failing logic"))
            })
            .unwrap_or_else(|| "read the strongest failing code path".to_string()),
        "search_files" => "search for the exact failing symbol or assertion".to_string(),
        "list_dir" => "read the most likely source file instead of listing again".to_string(),
        "glob" => "inspect the strongest matching file instead of globbing again".to_string(),
        _ => format!("change approach before rerunning {}", tc.name),
    };

    Some(ReflectionBlock {
        last_outcome: last_outcome.to_string(),
        goal_delta: GoalDelta::Same,
        wrong_assumption,
        strategy_change,
        next_minimal_action,
    })
}

fn synthetic_impact_progress(plan: &PlanBlock) -> String {
    let chosen_idx = plan
        .steps
        .iter()
        .position(|step| {
            let low = step.to_ascii_lowercase();
            [
                "patch",
                "edit",
                "write",
                "apply",
                "fix",
                "change",
                "update",
                "implement",
            ]
            .iter()
            .any(|term| low.contains(term))
        })
        .unwrap_or(0);
    let label = plan
        .steps
        .get(chosen_idx)
        .cloned()
        .unwrap_or_else(|| "the current plan".to_string());
    format!("step {} moved because {}", chosen_idx + 1, label)
}

fn synthetic_impact_changed(reason: &str) -> String {
    let detail = reason
        .split_once(':')
        .map(|(_, rest)| compact_one_line(rest.trim(), 120))
        .unwrap_or_default();
    if detail.is_empty() {
        "applied the confirmed smallest fix".to_string()
    } else if reason.to_ascii_lowercase().contains("command") {
        format!("applied the confirmed mutation command: {detail}")
    } else {
        format!("applied the confirmed smallest fix in {detail}")
    }
}

fn synthetic_impact_remaining_gap(tc: &ToolCallData) -> String {
    match tc.name.as_str() {
        "exec" => parse_exec_command_from_args(&tc.arguments)
            .map(|command| {
                format!("still need to run {command} and confirm the acceptance criteria")
            })
            .unwrap_or_else(|| {
                "still need the next verification run and final confirmation".to_string()
            }),
        _ => format!(
            "still need the next planned action ({}) and final verification",
            tc.name
        ),
    }
}

pub(super) fn rescue_missing_impact_for_tool_turn(
    reason: &str,
    tc: &ToolCallData,
    root_read_only: bool,
    goal_wants_actions: bool,
    provider: ProviderKind,
    plan: Option<&PlanBlock>,
    pending: Option<impact_recovery::UnreviewedMutation>,
) -> Option<ImpactBlock> {
    if root_read_only || !goal_wants_actions {
        return None;
    }
    if !supports_action_task_rescues(&provider) {
        return None;
    }
    let pending = pending?;
    let plan = plan?;

    Some(ImpactBlock {
        changed: if reason.contains(':') {
            synthetic_impact_changed(reason)
        } else {
            format!(
                "recorded workspace mutation at step {} still requires acceptance verification",
                pending.step
            )
        },
        progress: synthetic_impact_progress(plan),
        remaining_gap: synthetic_impact_remaining_gap(tc),
    })
}

pub(super) fn rescue_missing_evidence_for_tool_turn(
    tc: &ToolCallData,
    root_read_only: bool,
    goal_wants_actions: bool,
    provider: ProviderKind,
    observations: &ObservationEvidence,
) -> Option<EvidenceBlock> {
    if root_read_only || !goal_wants_actions {
        return None;
    }
    if !supports_action_task_rescues(&provider) {
        return None;
    }
    if !mutation_tool_requires_evidence(tc) {
        return None;
    }
    let target_path = mutation_target_path(tc)?;
    if !observation_supports_target_path(target_path.as_str(), observations) {
        return None;
    }

    Some(EvidenceBlock {
        target_files: vec![target_path.clone()],
        target_symbols: vec!["confirmed edit region".to_string()],
        evidence: format!(
            "read_file(path={target_path}) already confirmed the current code at the mutation target"
        ),
        open_questions: "none".to_string(),
        next_probe: format!("patch {target_path} with the smallest confirmed fix"),
    })
}

pub(super) fn repair_mistral_plan_for_tool_turn(
    parsed_plan: Option<&PlanBlock>,
    active_plan: Option<&PlanBlock>,
    tc: &ToolCallData,
    root_user_text: &str,
    root_read_only: bool,
) -> Option<PlanBlock> {
    if !root_read_only || !mistral_observation_tool(tc.name.as_str()) {
        return None;
    }
    let fallback = synthetic_read_only_observation_plan(root_user_text);
    let mut repaired = active_plan.cloned().unwrap_or_else(|| fallback.clone());
    if let Some(parsed) = parsed_plan {
        if !parsed.goal.trim().is_empty() {
            repaired.goal = parsed.goal.clone();
        }
        if !parsed.steps.is_empty() {
            repaired.steps = parsed.steps.clone();
        }
        if !parsed.acceptance_criteria.is_empty() {
            repaired.acceptance_criteria = parsed.acceptance_criteria.clone();
        }
        if !parsed.risks.trim().is_empty() {
            repaired.risks = parsed.risks.clone();
        }
        if !parsed.assumptions.trim().is_empty() {
            repaired.assumptions = parsed.assumptions.clone();
        }
    }
    if repaired.steps.len() < plan_field_min_items("steps").unwrap_or(2) {
        repaired.steps = fallback.steps;
    }
    if repaired.acceptance_criteria.len() < plan_field_min_items("acceptance").unwrap_or(1) {
        repaired.acceptance_criteria = fallback.acceptance_criteria;
    }
    if repaired.risks.trim().is_empty() {
        repaired.risks = fallback.risks;
    }
    if repaired.assumptions.trim().is_empty() {
        repaired.assumptions = fallback.assumptions;
    }
    Some(repaired)
}

pub(super) fn compat_synthetic_think(tc: &ToolCallData, plan: &PlanBlock) -> ThinkBlock {
    let next = match tc.name.as_str() {
        "exec" => parse_exec_command_from_args(&tc.arguments)
            .unwrap_or_else(|| "run the command".to_string()),
        "read_file" => serde_json::from_str::<serde_json::Value>(&tc.arguments)
            .ok()
            .and_then(|v| {
                v.get("path")
                    .and_then(|x| x.as_str())
                    .map(|path| format!("read {path}"))
            })
            .unwrap_or_else(|| "read the target file".to_string()),
        "search_files" => serde_json::from_str::<serde_json::Value>(&tc.arguments)
            .ok()
            .and_then(|v| {
                let pattern = v
                    .get("pattern")
                    .and_then(|x| x.as_str())
                    .unwrap_or("pattern");
                let dir = v.get("dir").and_then(|x| x.as_str()).unwrap_or(".");
                Some(format!("search {pattern} in {dir}"))
            })
            .unwrap_or_else(|| "search the codebase".to_string()),
        "list_dir" => serde_json::from_str::<serde_json::Value>(&tc.arguments)
            .ok()
            .and_then(|v| {
                v.get("dir")
                    .and_then(|x| x.as_str())
                    .map(|dir| format!("list {dir}"))
            })
            .unwrap_or_else(|| "list the directory".to_string()),
        "glob" => serde_json::from_str::<serde_json::Value>(&tc.arguments)
            .ok()
            .and_then(|v| {
                let pattern = v.get("pattern").and_then(|x| x.as_str()).unwrap_or("*");
                let dir = v.get("dir").and_then(|x| x.as_str()).unwrap_or(".");
                Some(format!("glob {pattern} in {dir}"))
            })
            .unwrap_or_else(|| "glob for candidate files".to_string()),
        "write_file" => serde_json::from_str::<serde_json::Value>(&tc.arguments)
            .ok()
            .and_then(|v| {
                v.get("path")
                    .and_then(|x| x.as_str())
                    .map(|path| format!("write {path}"))
            })
            .unwrap_or_else(|| "write the requested file".to_string()),
        "patch_file" => serde_json::from_str::<serde_json::Value>(&tc.arguments)
            .ok()
            .and_then(|v| {
                v.get("path")
                    .and_then(|x| x.as_str())
                    .map(|path| format!("patch {path}"))
            })
            .unwrap_or_else(|| "patch the target file".to_string()),
        "done" => "call done immediately".to_string(),
        _ => format!("run {}", tc.name),
    };

    ThinkBlock {
        goal: compact_one_line(plan.goal.as_str(), 120),
        step: 1,
        tool: tc.name.clone(),
        risk: "wrong target or overly broad action".to_string(),
        doubt: "may need one narrower follow-up".to_string(),
        next,
        verify: "confirm the tool output matches the request".to_string(),
    }
}

pub(super) fn select_think_for_tool_turn<'a>(
    parsed: Option<&'a ThinkBlock>,
    compat_last: Option<&'a ThinkBlock>,
    compat_synth: Option<&'a ThinkBlock>,
    plan: &PlanBlock,
    tc: &ToolCallData,
    provider: ProviderKind,
) -> (Option<&'a ThinkBlock>, bool) {
    if matches!(provider, ProviderKind::Mistral) {
        if let Some(think) = parsed.filter(|think| validate_think(think, plan, tc).is_ok()) {
            return (Some(think), false);
        }
        if let Some(think) = compat_synth.filter(|think| validate_think(think, plan, tc).is_ok()) {
            return (Some(think), true);
        }
        if let Some(think) = compat_last.filter(|think| validate_think(think, plan, tc).is_ok()) {
            return (Some(think), false);
        }
        return (
            parsed.or(compat_synth).or(compat_last),
            compat_synth.is_some(),
        );
    }

    if let Some(think) = parsed.filter(|think| validate_think(think, plan, tc).is_ok()) {
        return (Some(think), false);
    }
    if let Some(think) = compat_last.filter(|think| validate_think(think, plan, tc).is_ok()) {
        return (Some(think), false);
    }
    if let Some(think) = compat_synth.filter(|think| validate_think(think, plan, tc).is_ok()) {
        return (Some(think), true);
    }

    let fallback = parsed.or(compat_last).or(compat_synth);
    let used_synth = fallback
        .zip(compat_synth)
        .map(|(selected, synth)| std::ptr::eq(selected, synth))
        .unwrap_or(false);
    (fallback, used_synth)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn rescue_missing_plan_for_tool_turn_after_repeated_blocks_for_mistral_actions() {
        let tc = ToolCallData {
            thought_signature: None,
            id: "call_2".to_string(),
            name: "read_file".to_string(),
            arguments: json!({"path":"Cargo.toml"}).to_string(),
        };
        let messages = vec![
            json!({
                "role": "assistant",
                "content": "",
                "tool_calls": [{
                    "id": "call_1",
                    "type": "function",
                    "function": {
                        "name": "read_file",
                        "arguments": json!({"path":"Cargo.toml"}).to_string()
                    }
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_1",
                "content": "GOVERNOR BLOCKED\n\n[Plan Gate] Missing valid <plan>.\n\ntool:\nread_file\narguments:\n{\"path\":\"Cargo.toml\"}"
            }),
        ];

        let rescued = rescue_missing_plan_for_tool_turn(
            &messages,
            &tc,
            "Fix the failing test with the smallest code change. Run tests before you finish.",
            TaskHarness::infer(
                "Fix the failing test with the smallest code change. Run tests before you finish.",
                false,
            ),
            false,
            true,
            ProviderKind::Mistral,
            VerificationLevel::Behavioral,
            Some("cargo test 2>&1"),
        )
        .expect("synthetic action plan");

        assert!(rescued.goal.contains("Fix the failing test"));
        assert!(rescued
            .steps
            .iter()
            .any(|step| step.contains("read_file(path=Cargo.toml)")));
        assert!(rescued
            .steps
            .iter()
            .any(|step| step.contains("cargo test 2>&1")));
    }

    #[test]
    fn rescue_missing_plan_for_tool_turn_after_single_mixed_diagnostic_block_for_mistral() {
        let tc = ToolCallData {
            thought_signature: None,
            id: "call_2".to_string(),
            name: "read_file".to_string(),
            arguments: json!({"path":"Cargo.toml"}).to_string(),
        };
        let messages = vec![
            json!({
                "role": "assistant",
                "content": "",
                "tool_calls": [{
                    "id": "call_1",
                    "type": "function",
                    "function": {
                        "name": "read_file",
                        "arguments": json!({"path":"src/lib.rs"}).to_string()
                    }
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_1",
                "content": "GOVERNOR BLOCKED\n\n[Plan Gate] Missing valid <plan>.\n\ntool:\nread_file\narguments:\n{\"path\":\"src/lib.rs\"}"
            }),
        ];

        let rescued = rescue_missing_plan_for_tool_turn(
            &messages,
            &tc,
            "Fix the failing test with the smallest code change. Run tests before you finish.",
            TaskHarness::infer(
                "Fix the failing test with the smallest code change. Run tests before you finish.",
                false,
            ),
            false,
            true,
            ProviderKind::Mistral,
            VerificationLevel::Behavioral,
            Some("cargo test 2>&1"),
        )
        .expect("synthetic action plan after mixed diagnostic miss");

        assert!(rescued
            .steps
            .iter()
            .any(|step| step.contains("read_file(path=Cargo.toml)")));
    }

    #[test]
    fn rescue_missing_plan_for_tool_turn_after_single_diagnostic_block_for_openai_actions() {
        let tc = ToolCallData {
            thought_signature: None,
            id: "call_2".to_string(),
            name: "read_file".to_string(),
            arguments: json!({"path":"src/observer/repo_rules.rs"}).to_string(),
        };
        let messages = vec![
            json!({
                "role": "assistant",
                "content": "",
                "tool_calls": [{
                    "id": "call_1",
                    "type": "function",
                    "function": {
                        "name": "list_dir",
                        "arguments": json!({"dir":"src"}).to_string()
                    }
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_1",
                "content": "GOVERNOR BLOCKED\n\n[Plan Gate] Missing valid <plan>.\n\ntool:\nlist_dir\narguments:\n{\"dir\":\"src\"}"
            }),
        ];

        let rescued = rescue_missing_plan_for_tool_turn(
            &messages,
            &tc,
            "Fix the existing observer rule module with the smallest safe code change.",
            TaskHarness::infer(
                "Fix the existing observer rule module with the smallest safe code change.",
                false,
            ),
            false,
            true,
            ProviderKind::OpenAiCompatible,
            VerificationLevel::Behavioral,
            Some("cargo test 2>&1"),
        )
        .expect("synthetic action plan for openai after one diagnostic miss");

        assert!(rescued
            .steps
            .iter()
            .any(|step| step.contains("read_file(path=src/observer/repo_rules.rs)")));
    }

    #[test]
    fn rescue_missing_think_for_tool_turn_after_repeated_blocks_for_mistral_actions() {
        let tc = ToolCallData {
            thought_signature: None,
            id: "call_2".to_string(),
            name: "read_file".to_string(),
            arguments: json!({"path":"Cargo.toml"}).to_string(),
        };
        let messages = vec![
            json!({
                "role": "assistant",
                "content": "",
                "tool_calls": [{
                    "id": "call_1",
                    "type": "function",
                    "function": {
                        "name": "read_file",
                        "arguments": json!({"path":"Cargo.toml"}).to_string()
                    }
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_1",
                "content": "GOVERNOR BLOCKED\n\n[Think Gate] Missing <think>.\n\ntool:\nread_file\narguments:\n{\"path\":\"Cargo.toml\"}"
            }),
        ];
        let plan = PlanBlock {
            goal: "Fix the failing test with the smallest code change.".to_string(),
            steps: vec![
                "read_file(path=Cargo.toml) or src/lib.rs to confirm the failing logic".to_string(),
                "patch the smallest confirmed bug".to_string(),
                "run cargo test 2>&1".to_string(),
            ],
            acceptance_criteria: vec![
                "the requested change is implemented and confirmed by a passing behavioral verification command".to_string(),
            ],
            risks: "wrong target or speculative fix".to_string(),
            assumptions: "the failing test still reflects the requested bug".to_string(),
        };

        let rescued = rescue_missing_think_for_tool_turn(
            &messages,
            &tc,
            &plan,
            false,
            true,
            ProviderKind::Mistral,
            false,
        )
        .expect("synthetic think");

        validate_think(&rescued, &plan, &tc).expect("valid synthetic think");
        assert_eq!(rescued.tool, "read_file");
        assert!(rescued.next.contains("read Cargo.toml"));
    }

    #[test]
    fn rescue_missing_think_for_tool_turn_is_immediate_for_openai_read_file() {
        let tc = ToolCallData {
            thought_signature: None,
            id: "call_2".to_string(),
            name: "read_file".to_string(),
            arguments: json!({"path":"src/observer/repo_rules.rs"}).to_string(),
        };
        let plan = PlanBlock {
            goal: "Fix the observer rule change with the smallest code patch.".to_string(),
            steps: vec![
                "read_file(path=src/observer/repo_rules.rs) to confirm the rule list".to_string(),
                "patch the smallest confirmed rule entry".to_string(),
                "run cargo test 2>&1".to_string(),
            ],
            acceptance_criteria: vec![
                "the requested change is implemented and confirmed by a passing behavioral verification command".to_string(),
            ],
            risks: "wrong target or speculative fix".to_string(),
            assumptions: "the failing test still reflects the requested bug".to_string(),
        };

        let rescued = rescue_missing_think_for_tool_turn(
            &[],
            &tc,
            &plan,
            false,
            true,
            ProviderKind::OpenAiCompatible,
            false,
        )
        .expect("immediate synthetic think for read_file");

        validate_think(&rescued, &plan, &tc).expect("valid synthetic think");
        assert_eq!(rescued.tool, "read_file");
    }
}
