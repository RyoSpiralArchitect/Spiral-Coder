use super::*;

pub(crate) fn latest_goal_check_missing(messages: &[Value]) -> Vec<String> {
    for msg in messages.iter().rev() {
        if msg.get("role").and_then(|v| v.as_str()) != Some("user") {
            continue;
        }
        let content = msg.get("content").and_then(|v| v.as_str()).unwrap_or("");
        if !content.starts_with("[goal_check]") {
            continue;
        }
        for line in content.lines() {
            let trimmed = line.trim();
            let Some(rest) = trimmed.strip_prefix("Missing:") else {
                continue;
            };
            return rest
                .split(',')
                .map(str::trim)
                .filter(|entry| !entry.is_empty())
                .map(ToString::to_string)
                .collect();
        }
    }
    Vec::new()
}

#[derive(Debug, Clone, Default)]
pub(crate) struct RepoScaffoldStatus {
    repo_root: String,
    has_git: bool,
    has_gitignore: bool,
    has_readme: bool,
    required_files: Vec<String>,
    present_files: BTreeSet<String>,
}

pub(crate) fn repo_scaffold_status(
    messages: &[Value],
    test_cmd: &str,
    tool_root: Option<&str>,
) -> Option<RepoScaffoldStatus> {
    let repo_root = repo_root_from_test_cmd(test_cmd)?;
    let required_files = required_repo_files_from_test_cmd(test_cmd, repo_root.as_str());
    let mut pending_by_id: BTreeMap<String, (String, String)> = BTreeMap::new();
    let mut status = RepoScaffoldStatus {
        repo_root: repo_root.clone(),
        required_files,
        ..RepoScaffoldStatus::default()
    };

    for msg in messages {
        match msg.get("role").and_then(|v| v.as_str()).unwrap_or("") {
            "assistant" => {
                let Some(tool_calls) = msg.get("tool_calls").and_then(|v| v.as_array()) else {
                    continue;
                };
                for tc in tool_calls {
                    let id = tc.get("id").and_then(|v| v.as_str()).unwrap_or("").trim();
                    if id.is_empty() {
                        continue;
                    }
                    let Some(function) = tc.get("function") else {
                        continue;
                    };
                    let name = function
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .trim();
                    let arguments = function
                        .get("arguments")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .trim();
                    match name {
                        "write_file" => {
                            let path =
                                serde_json::from_str::<Value>(arguments)
                                    .ok()
                                    .and_then(|value| {
                                        value
                                            .get("path")
                                            .and_then(|v| v.as_str())
                                            .map(str::to_string)
                                    });
                            if let Some(path) = path {
                                pending_by_id.insert(id.to_string(), (name.to_string(), path));
                            }
                        }
                        "exec" => {
                            let command =
                                serde_json::from_str::<Value>(arguments)
                                    .ok()
                                    .and_then(|value| {
                                        value
                                            .get("command")
                                            .and_then(|v| v.as_str())
                                            .map(str::to_string)
                                    });
                            if let Some(command) = command {
                                pending_by_id.insert(id.to_string(), (name.to_string(), command));
                            }
                        }
                        _ => {}
                    }
                }
            }
            "tool" => {
                let id = msg
                    .get("tool_call_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim();
                if id.is_empty() {
                    continue;
                }
                let content = msg.get("content").and_then(|v| v.as_str()).unwrap_or("");
                let Some((kind, payload)) = pending_by_id.get(id) else {
                    continue;
                };
                match kind.as_str() {
                    "write_file" if content.trim_start().starts_with("OK: wrote '") => {
                        if payload.starts_with(&format!("{repo_root}/")) {
                            status.present_files.insert(payload.clone());
                        }
                        if payload.ends_with("/README.md") {
                            status.has_readme = true;
                        }
                        if payload.ends_with("/.gitignore") {
                            status.has_gitignore = true;
                        }
                    }
                    "exec" if content.trim_start().starts_with("OK (exit_code: 0)") => {
                        if payload.trim() == format!("git init {}", repo_root) {
                            status.has_git = true;
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    if let Some(root) = tool_root {
        let repo_path = resolve_repo_scaffold_path(root, repo_root.as_str());
        status.has_git |= repo_path.join(".git").exists();
        status.has_gitignore |= repo_path.join(".gitignore").exists();
        status.has_readme |= repo_path.join("README.md").exists();
        for required_path in status.required_files.clone() {
            if resolve_repo_file_path(root, required_path.as_str()).exists() {
                if required_path.ends_with("/README.md") {
                    status.has_readme = true;
                }
                if required_path.ends_with("/.gitignore") {
                    status.has_gitignore = true;
                }
                status.present_files.insert(required_path);
            }
        }
    }

    Some(status)
}

pub(crate) fn write_file_looks_malformed(tc: &ToolCallData) -> bool {
    let parsed = serde_json::from_str::<Value>(tc.arguments.as_str()).ok();
    let path = parsed
        .as_ref()
        .and_then(|value| value.get("path").and_then(|v| v.as_str()))
        .map(str::trim)
        .unwrap_or("");
    let content = parsed
        .as_ref()
        .and_then(|value| value.get("content").and_then(|v| v.as_str()))
        .map(str::trim)
        .unwrap_or("");
    parsed.is_none()
        || path.is_empty()
        || content.is_empty()
        || path == ".git"
        || path.ends_with("/.git")
}

pub(crate) fn has_repo_scaffold_progress(status: &RepoScaffoldStatus) -> bool {
    status.has_git || status.has_gitignore || status.has_readme || !status.present_files.is_empty()
}

pub(crate) fn next_repo_scaffold_tool_call(
    tool_call_id: &str,
    status: &RepoScaffoldStatus,
) -> Option<ToolCallData> {
    if !status.has_git {
        return Some(ToolCallData {
            thought_signature: None,
            id: tool_call_id.to_string(),
            name: "exec".to_string(),
            arguments: serde_json::json!({
                "command": format!("git init {}", status.repo_root)
            })
            .to_string(),
        });
    }
    if let Some(target) = preferred_missing_repo_required_file(status) {
        let content = if target.ends_with("/.gitignore") {
            default_repo_gitignore().to_string()
        } else {
            scaffold_repo_file_content(target)
        };
        return Some(ToolCallData {
            thought_signature: None,
            id: tool_call_id.to_string(),
            name: "write_file".to_string(),
            arguments: serde_json::json!({
                "path": target,
                "content": content,
            })
            .to_string(),
        });
    }
    if !status.has_gitignore {
        return Some(ToolCallData {
            thought_signature: None,
            id: tool_call_id.to_string(),
            name: "write_file".to_string(),
            arguments: serde_json::json!({
                "path": format!("{}/.gitignore", status.repo_root),
                "content": default_repo_gitignore(),
            })
            .to_string(),
        });
    }
    if !status.has_readme {
        let readme_path = format!("{}/README.md", status.repo_root);
        let readme_content = scaffold_repo_file_content(readme_path.as_str());
        return Some(ToolCallData {
            thought_signature: None,
            id: tool_call_id.to_string(),
            name: "write_file".to_string(),
            arguments: serde_json::json!({
                "path": readme_path,
                "content": readme_content,
            })
            .to_string(),
        });
    }
    None
}

pub(crate) fn preferred_missing_repo_required_file(status: &RepoScaffoldStatus) -> Option<&str> {
    if let Some(gitignore) = status
        .required_files
        .iter()
        .find(|path| path.ends_with("/.gitignore") && !status.present_files.contains(*path))
    {
        return Some(gitignore.as_str());
    }

    status
        .required_files
        .iter()
        .find(|path| !status.present_files.contains(*path))
        .map(String::as_str)
}

pub(crate) fn text_contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

pub(crate) fn prompt_mentions_path_literal(text: &str) -> bool {
    text.split_whitespace().any(|token| {
        let trimmed = token.trim_matches(|c: char| {
            c.is_ascii_punctuation()
                || matches!(
                    c,
                    '「' | '」' | '『' | '』' | '（' | '）' | '`' | '"' | '\''
                )
        });
        let has_path_sep = trimmed.contains('/') || trimmed.contains('\\');
        let has_extension = trimmed
            .split('/')
            .next_back()
            .is_some_and(|segment| segment.contains('.'));
        has_path_sep || has_extension
    })
}
