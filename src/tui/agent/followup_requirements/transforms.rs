use super::*;

pub(crate) fn synthesize_followup_patch(
    requirement: &FollowupRequirement,
    body: &str,
) -> Option<ToolCallData> {
    if body.contains(requirement.missing_literal.as_str()) {
        return None;
    }
    let replace = match requirement.kind {
        FollowupKind::Docs => {
            transform_markdown_followup(body, requirement.missing_literal.as_str())
        }
        FollowupKind::TuiReplay => {
            transform_tui_replay_followup(body, requirement.missing_literal.as_str())
        }
        FollowupKind::RuntimeEval => {
            transform_runtime_eval_followup(body, requirement.missing_literal.as_str())
        }
    }?;
    Some(ToolCallData {
        thought_signature: None,
        id: "synthetic_followup_patch".to_string(),
        name: "patch_file".to_string(),
        arguments: json!({
            "path": requirement.file_path,
            "search": body,
            "replace": replace,
        })
        .to_string(),
    })
}

pub(crate) fn transform_markdown_followup(body: &str, missing_literal: &str) -> Option<String> {
    let mut lines = body.lines().map(str::to_string).collect::<Vec<_>>();
    let bullet = format!("- `{missing_literal}`");
    let insert_at = lines
        .iter()
        .enumerate()
        .rev()
        .find_map(|(idx, line)| {
            let trimmed = line.trim();
            (trimmed.starts_with("- `") && trimmed.ends_with('`')).then_some(idx + 1)
        })
        .unwrap_or(lines.len());
    lines.insert(insert_at, bullet);
    let mut out = lines.join("\n");
    if body.ends_with('\n') {
        out.push('\n');
    }
    Some(out)
}

pub(crate) fn transform_tui_replay_followup(body: &str, missing_literal: &str) -> Option<String> {
    transform_json_paths_followup(body, missing_literal)
}

pub(crate) fn transform_runtime_eval_followup(body: &str, missing_literal: &str) -> Option<String> {
    transform_json_paths_followup(body, missing_literal)
}

pub(crate) fn transform_json_paths_followup(body: &str, missing_literal: &str) -> Option<String> {
    let mut value = serde_json::from_str::<Value>(body).ok()?;
    let paths = value.get_mut("paths")?.as_array_mut()?;
    if paths
        .iter()
        .any(|entry| entry.as_str() == Some(missing_literal))
    {
        return None;
    }
    paths.push(Value::String(missing_literal.to_string()));
    let mut out = serde_json::to_string_pretty(&value).ok()?;
    if body.ends_with('\n') {
        out.push('\n');
    }
    Some(out)
}

pub(crate) fn docs_followup_path(root_user_text: &str) -> Option<String> {
    path_literals_in_text(root_user_text)
        .into_iter()
        .find(|path| path.starts_with("docs/") && path.ends_with(".md"))
}

pub(crate) fn tui_replay_followup_path(root_user_text: &str) -> Option<String> {
    path_literals_in_text(root_user_text)
        .into_iter()
        .find(|path| {
            path.ends_with(".spiral-coder/tui_replay.json") || path.ends_with("tui_replay.json")
        })
}

pub(crate) fn runtime_eval_followup_path(root_user_text: &str) -> Option<String> {
    path_literals_in_text(root_user_text)
        .into_iter()
        .find(|path| {
            path.ends_with(".spiral-coder/runtime_eval.json") || path.ends_with("runtime_eval.json")
        })
}

pub(crate) fn target_src_literal(root_user_text: &str) -> Option<String> {
    path_literals_in_text(root_user_text)
        .into_iter()
        .find(|path| path.starts_with("src/") && path.ends_with(".rs"))
}
