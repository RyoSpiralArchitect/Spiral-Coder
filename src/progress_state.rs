use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::agent_session::{
    session_bridge_from_messages, SessionAcceptedStrategy, SessionBridge, SessionDeadEnd,
};

fn now_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn compact_one_line(text: &str, max_chars: usize) -> String {
    let compact = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = compact.trim();
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_string();
    }
    let mut out = String::new();
    for ch in trimmed.chars().take(max_chars.saturating_sub(1)) {
        out.push(ch);
    }
    out.push('…');
    out
}

fn normalize_key(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_ascii_lowercase()
}

fn keyword_tokens(text: &str) -> std::collections::BTreeSet<String> {
    text.split(|ch: char| !ch.is_ascii_alphanumeric())
        .map(|part| part.trim().to_ascii_lowercase())
        .filter(|part| part.len() >= 3)
        .collect()
}

fn token_overlap_score(
    a: &std::collections::BTreeSet<String>,
    b: &std::collections::BTreeSet<String>,
) -> f32 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let overlap = a.intersection(b).count() as f32;
    let denom = a.len().min(b.len()).max(1) as f32;
    (overlap / denom).clamp(0.0, 1.0)
}

fn extract_tag_block<'a>(text: &'a str, tag: &str) -> Option<&'a str> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = text.find(&open)?;
    let rest = &text[start + open.len()..];
    let end = rest.find(&close)?;
    Some(rest[..end].trim())
}

fn latest_plan_goal(messages: &[Value]) -> Option<String> {
    for msg in messages.iter().rev() {
        if msg.get("role").and_then(|v| v.as_str()) != Some("assistant") {
            continue;
        }
        let content = msg.get("content").and_then(|v| v.as_str()).unwrap_or("");
        let Some(plan_body) = extract_tag_block(content, "plan") else {
            continue;
        };
        for line in plan_body.lines() {
            let Some((key, value)) = line.split_once(':') else {
                continue;
            };
            if key.trim().eq_ignore_ascii_case("goal") {
                let goal = compact_one_line(value.trim(), 200);
                if !goal.is_empty() {
                    return Some(goal);
                }
            }
        }
    }
    None
}

fn save_text_atomic(path: &Path, text: &str) -> Result<()> {
    let parent0 = path.parent().unwrap_or_else(|| Path::new("."));
    let parent = if parent0.as_os_str().is_empty() {
        Path::new(".")
    } else {
        parent0
    };
    std::fs::create_dir_all(parent)
        .with_context(|| format!("failed to create progress directory: {}", parent.display()))?;

    let mut tmp = tempfile::NamedTempFile::new_in(parent)
        .with_context(|| format!("failed to create temp file under {}", parent.display()))?;
    use std::io::Write;
    tmp.write_all(text.as_bytes())
        .context("failed to write progress temp file")?;
    tmp.flush().ok();

    let tmp_path = tmp.path().to_path_buf();
    match tmp.persist(path) {
        Ok(_) => Ok(()),
        Err(e) => {
            let _ = std::fs::remove_file(&tmp_path);
            Err(anyhow::anyhow!(
                "failed to persist progress file: {}",
                e.error
            ))
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProgressArtifact {
    pub path: String,
    pub source: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProgressVerification {
    pub command: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProgressSaveContext {
    pub task_summary: String,
    pub lane: String,
    pub artifact_mode: String,
}

impl ProgressSaveContext {
    pub fn new(task_summary: &str, lane: &str, artifact_mode: &str) -> Self {
        Self {
            task_summary: compact_one_line(task_summary, 220),
            lane: lane.trim().to_string(),
            artifact_mode: artifact_mode.trim().to_string(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepoProgressState {
    pub version: u32,
    pub updated_at_ms: u128,
    #[serde(default)]
    pub task_summary: String,
    #[serde(default)]
    pub current_objective: String,
    #[serde(default)]
    pub lane: String,
    #[serde(default)]
    pub artifact_mode: String,
    #[serde(default)]
    pub completed_artifacts: Vec<ProgressArtifact>,
    #[serde(default)]
    pub verified_commands: Vec<ProgressVerification>,
    #[serde(default)]
    pub accepted_strategies: Vec<SessionAcceptedStrategy>,
    #[serde(default)]
    pub repeated_dead_ends: Vec<SessionDeadEnd>,
}

impl RepoProgressState {
    pub const VERSION: u32 = 1;

    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read progress file: {}", path.display()))?;
        let state: Self = serde_json::from_str(&text)
            .with_context(|| format!("failed to parse progress JSON: {}", path.display()))?;
        if state.version != Self::VERSION {
            anyhow::bail!(
                "unsupported progress version {} (expected {})",
                state.version,
                Self::VERSION
            );
        }
        Ok(state)
    }

    pub fn save_atomic(&self, path: &Path) -> Result<()> {
        let text =
            serde_json::to_string_pretty(self).context("failed to serialize repo progress")?;
        save_text_atomic(path, &text)
    }

    pub fn derive(context: &ProgressSaveContext, messages: &[Value]) -> Self {
        let bridge_owned = session_bridge_from_messages(messages);
        Self::derive_with_bridge(context, messages, bridge_owned.as_ref())
    }

    pub fn derive_with_bridge(
        context: &ProgressSaveContext,
        messages: &[Value],
        bridge: Option<&SessionBridge>,
    ) -> Self {
        let bridge = bridge.cloned().unwrap_or_default();
        let task_summary = compact_one_line(context.task_summary.as_str(), 220);
        let current_objective = latest_plan_goal(messages)
            .unwrap_or_else(|| compact_one_line(context.task_summary.as_str(), 200));
        let completed_artifacts = completed_artifacts_from_messages(messages);
        let verified_commands = verified_commands_from_messages(messages, &bridge);
        Self {
            version: Self::VERSION,
            updated_at_ms: now_ms(),
            task_summary,
            current_objective,
            lane: context.lane.clone(),
            artifact_mode: context.artifact_mode.clone(),
            completed_artifacts,
            verified_commands,
            accepted_strategies: bridge.accepted_strategies.into_iter().take(3).collect(),
            repeated_dead_ends: bridge.repeated_dead_ends.into_iter().take(3).collect(),
        }
    }

    pub fn has_details(&self) -> bool {
        !(self.completed_artifacts.is_empty()
            && self.verified_commands.is_empty()
            && self.accepted_strategies.is_empty()
            && self.repeated_dead_ends.is_empty())
    }

    pub fn task_matches(&self, task_summary: &str) -> bool {
        let current = keyword_tokens(task_summary);
        let saved = keyword_tokens(self.task_summary.as_str());
        token_overlap_score(&current, &saved) >= 0.35
    }
}

pub fn path_for_root(root: &str) -> PathBuf {
    Path::new(root).join(".spiral-coder/progress.json")
}

#[derive(Debug, Clone)]
struct PendingToolCall {
    tool_name: String,
    signature: String,
    path: Option<String>,
    command: Option<String>,
}

#[derive(Debug, Clone)]
struct ToolEvent {
    tool_name: String,
    signature: String,
    path: Option<String>,
}

fn json_arg_string(arguments: &str, key: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(arguments)
        .ok()?
        .get(key)
        .and_then(|v| v.as_str())
        .map(|v| v.to_string())
}

fn canonicalize_tool_signature(name: &str, arguments: &str) -> String {
    match name {
        "exec" => json_arg_string(arguments, "command")
            .map(|s| compact_one_line(s.as_str(), 220))
            .unwrap_or_else(|| format!("exec({})", compact_one_line(arguments, 160))),
        "write_file" | "patch_file" | "apply_diff" | "read_file" => {
            let path = json_arg_string(arguments, "path").unwrap_or_default();
            if path.is_empty() {
                format!("{name}({})", compact_one_line(arguments, 160))
            } else {
                format!("{name}(path={})", compact_one_line(path.as_str(), 180))
            }
        }
        _ => format!("{name}({})", compact_one_line(arguments, 160)),
    }
}

fn pending_tool_calls(messages: &[Value]) -> Vec<ToolEvent> {
    let mut by_id: BTreeMap<String, PendingToolCall> = BTreeMap::new();
    let mut completed = Vec::new();
    for msg in messages {
        match msg.get("role").and_then(|v| v.as_str()) {
            Some("assistant") => {
                let Some(tool_calls) = msg.get("tool_calls").and_then(|v| v.as_array()) else {
                    continue;
                };
                for tool_call in tool_calls {
                    let Some(id) = tool_call.get("id").and_then(|v| v.as_str()) else {
                        continue;
                    };
                    let function = tool_call.get("function").unwrap_or(&Value::Null);
                    let name = function
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let arguments = function
                        .get("arguments")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    by_id.insert(
                        id.to_string(),
                        PendingToolCall {
                            tool_name: name.clone(),
                            signature: canonicalize_tool_signature(name.as_str(), &arguments),
                            path: json_arg_string(&arguments, "path"),
                            command: json_arg_string(&arguments, "command"),
                        },
                    );
                }
            }
            Some("tool") => {
                let Some(id) = msg.get("tool_call_id").and_then(|v| v.as_str()) else {
                    continue;
                };
                let Some(pending) = by_id.remove(id) else {
                    continue;
                };
                let content = msg.get("content").and_then(|v| v.as_str()).unwrap_or("");
                if tool_result_success(content) {
                    completed.push(ToolEvent {
                        tool_name: pending.tool_name,
                        signature: pending.command.unwrap_or_else(|| pending.signature.clone()),
                        path: pending.path,
                    });
                }
            }
            _ => {}
        }
    }
    completed
}

fn tool_result_success(content: &str) -> bool {
    let low = content.to_ascii_lowercase();
    low.starts_with("ok:")
        || low.starts_with("ok (exit_code: 0)")
        || low.contains("[auto-test] ✓ passed")
}

fn completed_artifacts_from_messages(messages: &[Value]) -> Vec<ProgressArtifact> {
    let mut seen = BTreeMap::<String, ProgressArtifact>::new();
    for event in pending_tool_calls(messages) {
        if !matches!(
            event.tool_name.as_str(),
            "write_file" | "patch_file" | "apply_diff"
        ) {
            continue;
        }
        let Some(path) = event.path.as_deref() else {
            continue;
        };
        seen.entry(normalize_key(path))
            .or_insert_with(|| ProgressArtifact {
                path: compact_one_line(path, 180),
                source: event.tool_name.clone(),
            });
    }
    seen.into_values().collect()
}

fn verified_commands_from_messages(
    messages: &[Value],
    bridge: &SessionBridge,
) -> Vec<ProgressVerification> {
    let mut commands = Vec::<String>::new();
    if let Some(verification) = bridge.last_good_verification.as_ref() {
        commands.push(compact_one_line(verification.command.as_str(), 220));
    }
    for event in pending_tool_calls(messages) {
        if event.tool_name != "exec" {
            continue;
        }
        if !looks_like_verification_command(event.signature.as_str()) {
            continue;
        }
        commands.push(compact_one_line(event.signature.as_str(), 220));
    }
    let mut seen = BTreeMap::<String, ProgressVerification>::new();
    for command in commands {
        if command.is_empty() {
            continue;
        }
        seen.entry(normalize_key(command.as_str()))
            .or_insert_with(|| ProgressVerification { command });
    }
    seen.into_values().collect()
}

fn looks_like_verification_command(command: &str) -> bool {
    let low = command.to_ascii_lowercase();
    [
        "cargo test",
        "pytest",
        "python -m unittest",
        "python3 -m unittest",
        "npm test",
        "pnpm test",
        "yarn test",
        "cargo check",
        "cargo build",
        "lint",
        "verify",
    ]
    .iter()
    .any(|needle| low.contains(needle))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repo_progress_derives_artifacts_and_verification() {
        let context = ProgressSaveContext::new(
            "Fix the failing test with the smallest code change.",
            "fix_existing_files",
            "modify_existing",
        );
        let messages = vec![
            serde_json::json!({"role":"user","content":"Fix the failing test with the smallest code change."}),
            serde_json::json!({"role":"assistant","content":"<plan>\ngoal: fix the failing test safely\nsteps: 1) inspect 2) patch 3) verify\nacceptance: 1) tests pass\nrisks: stale read\nassumptions: src/lib.rs is the bug\n</plan>"}),
            serde_json::json!({"role":"assistant","tool_calls":[
                {"id":"call_patch","type":"function","function":{"name":"patch_file","arguments":"{\"path\":\"src/lib.rs\",\"search\":\"bug\",\"replace\":\"fix\"}"}}
            ]}),
            serde_json::json!({"role":"tool","tool_call_id":"call_patch","content":"OK: patched 'src/lib.rs'\n[auto-test] ✗ FAILED (exit 1)"}),
            serde_json::json!({"role":"assistant","tool_calls":[
                {"id":"call_exec","type":"function","function":{"name":"exec","arguments":"{\"command\":\"cargo test 2>&1\"}"}}
            ]}),
            serde_json::json!({"role":"tool","tool_call_id":"call_exec","content":"OK (exit_code: 0)\nall good"}),
            serde_json::json!({"role":"assistant","content":"<reflect>\nlast_outcome: success\ngoal_delta: closer\nwrong_assumption: rereading would help\nstrategy_change: adjust\nnext_minimal_action: patch src/lib.rs directly\n</reflect>"}),
        ];

        let progress = RepoProgressState::derive(&context, &messages);

        assert_eq!(progress.current_objective, "fix the failing test safely");
        assert_eq!(progress.completed_artifacts.len(), 1);
        assert_eq!(progress.completed_artifacts[0].path, "src/lib.rs");
        assert_eq!(progress.verified_commands.len(), 1);
        assert_eq!(progress.verified_commands[0].command, "cargo test 2>&1");
    }

    #[test]
    fn repo_progress_matches_related_task_summaries() {
        let progress = RepoProgressState {
            version: RepoProgressState::VERSION,
            updated_at_ms: 1,
            task_summary: "Fix the failing Rust test with the smallest code change.".to_string(),
            current_objective: "fix failing rust test".to_string(),
            lane: "fix_existing_files".to_string(),
            artifact_mode: "modify_existing".to_string(),
            completed_artifacts: vec![],
            verified_commands: vec![],
            accepted_strategies: vec![],
            repeated_dead_ends: vec![],
        };

        assert!(progress.task_matches("Fix the failing test with the smallest safe Rust change."));
        assert!(!progress.task_matches("Create a new git repo for a pygame maze game."));
    }
}
