use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LastReflectionSummary {
    pub last_outcome: Option<String>,
    pub goal_delta: Option<String>,
    pub wrong_assumption: Option<String>,
    pub strategy_change: Option<String>,
    pub next_minimal_action: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionVerificationMemory {
    pub command: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionAcceptedStrategy {
    pub wrong_assumption: String,
    pub next_minimal_action: String,
    #[serde(default)]
    pub matched_command: String,
    #[serde(default)]
    pub count: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionDeadEnd {
    pub command: String,
    pub reason: String,
    #[serde(default)]
    pub count: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionBridge {
    #[serde(default)]
    pub last_good_verification: Option<SessionVerificationMemory>,
    #[serde(default)]
    pub accepted_strategies: Vec<SessionAcceptedStrategy>,
    #[serde(default)]
    pub repeated_dead_ends: Vec<SessionDeadEnd>,
}

impl SessionBridge {
    pub fn is_empty(&self) -> bool {
        self.last_good_verification.is_none()
            && self.accepted_strategies.is_empty()
            && self.repeated_dead_ends.is_empty()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub struct ObservationReadCache {
    pub command: String,
    pub path: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub struct ObservationSearchCache {
    pub command: String,
    pub pattern: String,
    pub hit_count: usize,
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub struct ObservationResolutionCache {
    pub query: String,
    pub canonical_path: String,
    pub source: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub struct ObservationCache {
    pub reads: Vec<ObservationReadCache>,
    pub searches: Vec<ObservationSearchCache>,
    #[serde(default)]
    pub resolutions: Vec<ObservationResolutionCache>,
}

fn extract_tag_block<'a>(text: &'a str, tag: &str) -> Option<&'a str> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = text.find(&open)?;
    let rest = &text[start + open.len()..];
    let end = rest.find(&close)?;
    Some(rest[..end].trim())
}

fn parse_reflection_summary(text: &str) -> Option<LastReflectionSummary> {
    let body = extract_tag_block(text, "reflect")?;
    let mut s = LastReflectionSummary::default();

    for line in body.lines() {
        let (k, v) = match line.split_once(':') {
            Some((k, v)) => (k.trim().to_ascii_lowercase(), v.trim().to_string()),
            None => continue,
        };
        if v.is_empty() {
            continue;
        }
        match k.as_str() {
            "last_outcome" => s.last_outcome = Some(v),
            "goal_delta" => s.goal_delta = Some(v.to_ascii_lowercase()),
            "wrong_assumption" => s.wrong_assumption = Some(v),
            "strategy_change" => s.strategy_change = Some(v.to_ascii_lowercase()),
            "next_minimal_action" => s.next_minimal_action = Some(v),
            _ => {}
        }
    }

    if s.last_outcome.is_none()
        && s.goal_delta.is_none()
        && s.wrong_assumption.is_none()
        && s.strategy_change.is_none()
        && s.next_minimal_action.is_none()
    {
        None
    } else {
        Some(s)
    }
}

pub fn last_reflection_summary_from_messages(
    messages: &[serde_json::Value],
) -> Option<LastReflectionSummary> {
    for msg in messages.iter().rev() {
        if msg.get("role").and_then(|r| r.as_str()) != Some("assistant") {
            continue;
        }
        let content = msg.get("content").and_then(|c| c.as_str()).unwrap_or("");
        if let Some(s) = parse_reflection_summary(content) {
            return Some(s);
        }
    }
    None
}

pub fn recent_reflection_summaries_from_messages(
    messages: &[serde_json::Value],
    max: usize,
) -> Vec<LastReflectionSummary> {
    let max = max.max(1).min(12);
    let mut out: Vec<LastReflectionSummary> = Vec::new();
    for msg in messages.iter().rev() {
        if out.len() >= max {
            break;
        }
        if msg.get("role").and_then(|r| r.as_str()) != Some("assistant") {
            continue;
        }
        let content = msg.get("content").and_then(|c| c.as_str()).unwrap_or("");
        if let Some(s) = parse_reflection_summary(content) {
            out.push(s);
        }
    }
    out.reverse(); // chronological
    out
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

#[derive(Debug, Clone)]
struct ReflectionSummaryEvent {
    message_index: usize,
    summary: LastReflectionSummary,
}

#[derive(Debug, Clone)]
struct PendingToolCall {
    name: String,
    signature: String,
    command: Option<String>,
    path: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SessionToolKind {
    Observation,
    Mutation,
    Verification,
}

#[derive(Debug, Clone)]
struct SessionToolEvent {
    message_index: usize,
    kind: SessionToolKind,
    signature: String,
    tool_name: String,
    path: Option<String>,
}

fn reflection_summary_events(messages: &[serde_json::Value]) -> Vec<ReflectionSummaryEvent> {
    let mut out = Vec::new();
    for (idx, msg) in messages.iter().enumerate() {
        if msg.get("role").and_then(|r| r.as_str()) != Some("assistant") {
            continue;
        }
        let content = msg.get("content").and_then(|c| c.as_str()).unwrap_or("");
        let Some(summary) = parse_reflection_summary(content) else {
            continue;
        };
        out.push(ReflectionSummaryEvent {
            message_index: idx,
            summary,
        });
    }
    out
}

fn json_arg_string(arguments: &str, key: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(arguments)
        .ok()?
        .get(key)
        .and_then(|value| value.as_str())
        .map(|value| compact_one_line(value.trim(), 180))
        .filter(|value| !value.is_empty())
}

fn parse_exec_command_from_args(arguments: &str) -> Option<String> {
    json_arg_string(arguments, "command")
}

fn tool_call_signature(name: &str, arguments: &str) -> String {
    match name {
        "exec" => parse_exec_command_from_args(arguments)
            .unwrap_or_else(|| format!("exec({})", compact_one_line(arguments, 120))),
        "read_file" | "write_file" | "patch_file" | "apply_diff" => {
            let path = json_arg_string(arguments, "path").unwrap_or_else(|| "?".to_string());
            format!("{name}(path={path})")
        }
        "list_dir" => {
            let dir = json_arg_string(arguments, "dir").unwrap_or_else(|| ".".to_string());
            format!("list_dir(dir={dir})")
        }
        "glob" => {
            let pattern = json_arg_string(arguments, "pattern").unwrap_or_else(|| "*".to_string());
            format!("glob(pattern={pattern})")
        }
        "search_files" => {
            let pattern = json_arg_string(arguments, "pattern").unwrap_or_else(|| "?".to_string());
            let dir = json_arg_string(arguments, "dir").unwrap_or_else(|| ".".to_string());
            format!("search_files(pattern={pattern}, dir={dir})")
        }
        _ => format!("{name}({})", compact_one_line(arguments, 120)),
    }
}

fn parse_exec_exit_code(content: &str) -> Option<i32> {
    let marker = "exit_code:";
    let start = content.find(marker)?;
    let tail = &content[start + marker.len()..];
    let digits: String = tail
        .trim_start()
        .chars()
        .take_while(|ch| ch.is_ascii_digit() || *ch == '-')
        .collect();
    digits.parse::<i32>().ok()
}

fn looks_verification_command(command: &str) -> bool {
    let low = command.to_ascii_lowercase();
    [
        " test",
        "test ",
        "cargo test",
        "pytest",
        "pnpm test",
        "npm test",
        "bun test",
        "go test",
        "cargo check",
        "cargo clippy",
        "cargo build",
        "verify",
        "lint",
    ]
    .iter()
    .any(|needle| low.contains(needle))
}

fn successful_session_tool_event(
    message_index: usize,
    pending: PendingToolCall,
    content: &str,
) -> Option<SessionToolEvent> {
    let trimmed = content.trim_start();
    let kind = match pending.name.as_str() {
        "read_file" | "search_files" | "list_dir" | "glob" => {
            if trimmed.starts_with('[') && !trimmed.starts_with("[RESULT") {
                SessionToolKind::Observation
            } else {
                return None;
            }
        }
        "write_file" if trimmed.starts_with("OK: wrote '") => SessionToolKind::Mutation,
        "patch_file" if trimmed.starts_with("OK: patched '") => SessionToolKind::Mutation,
        "apply_diff" if trimmed.starts_with("OK: applied ") => SessionToolKind::Mutation,
        "exec" => {
            let command = pending.command.as_deref()?;
            if parse_exec_exit_code(content) != Some(0) {
                return None;
            }
            if looks_verification_command(command) {
                SessionToolKind::Verification
            } else {
                SessionToolKind::Mutation
            }
        }
        _ => return None,
    };
    Some(SessionToolEvent {
        message_index,
        kind,
        signature: pending.signature,
        tool_name: pending.name,
        path: pending.path,
    })
}

fn session_tool_events(messages: &[serde_json::Value]) -> Vec<SessionToolEvent> {
    let mut pending: BTreeMap<String, PendingToolCall> = BTreeMap::new();
    let mut out = Vec::new();
    for (idx, msg) in messages.iter().enumerate() {
        match msg.get("role").and_then(|v| v.as_str()).unwrap_or("") {
            "assistant" => {
                let Some(tool_calls) = msg.get("tool_calls").and_then(|value| value.as_array())
                else {
                    continue;
                };
                for tool_call in tool_calls {
                    let id = tool_call
                        .get("id")
                        .and_then(|value| value.as_str())
                        .unwrap_or("")
                        .trim();
                    if id.is_empty() {
                        continue;
                    }
                    let Some(function) = tool_call.get("function") else {
                        continue;
                    };
                    let name = function
                        .get("name")
                        .and_then(|value| value.as_str())
                        .unwrap_or("")
                        .trim()
                        .to_string();
                    let arguments = function
                        .get("arguments")
                        .and_then(|value| value.as_str())
                        .unwrap_or("")
                        .trim()
                        .to_string();
                    pending.insert(
                        id.to_string(),
                        PendingToolCall {
                            signature: tool_call_signature(name.as_str(), arguments.as_str()),
                            command: parse_exec_command_from_args(arguments.as_str()),
                            path: json_arg_string(arguments.as_str(), "path")
                                .or_else(|| json_arg_string(arguments.as_str(), "dir")),
                            name,
                        },
                    );
                }
            }
            "tool" => {
                let id = msg
                    .get("tool_call_id")
                    .and_then(|value| value.as_str())
                    .unwrap_or("")
                    .trim();
                let Some(pending_call) = pending.remove(id) else {
                    continue;
                };
                let content = msg
                    .get("content")
                    .and_then(|value| value.as_str())
                    .unwrap_or("");
                if let Some(event) = successful_session_tool_event(idx, pending_call, content) {
                    out.push(event);
                }
            }
            _ => {}
        }
    }
    out
}

fn action_matches_tool_event(action: &str, event: &SessionToolEvent) -> bool {
    let action_low = normalize_key(action);
    let signature_low = normalize_key(event.signature.as_str());
    if !event.tool_name.is_empty() && action_low.contains(event.tool_name.as_str()) {
        return true;
    }
    if let Some(path) = event.path.as_deref() {
        let path_low = normalize_key(path);
        if !path_low.is_empty()
            && (action_low.contains(path_low.as_str()) || signature_low.contains(path_low.as_str()))
        {
            return true;
        }
    }
    token_overlap_score(
        &keyword_tokens(action),
        &keyword_tokens(event.signature.as_str()),
    ) >= 0.45
}

pub fn session_bridge_from_messages(messages: &[serde_json::Value]) -> Option<SessionBridge> {
    let tool_events = session_tool_events(messages);
    let last_good_verification = tool_events.iter().rev().find_map(|event| {
        (event.kind == SessionToolKind::Verification).then(|| SessionVerificationMemory {
            command: event.signature.clone(),
        })
    });

    let mut accepted_by_action: BTreeMap<String, SessionAcceptedStrategy> = BTreeMap::new();
    for reflection in reflection_summary_events(messages) {
        let Some(next_action) = reflection.summary.next_minimal_action.as_deref() else {
            continue;
        };
        if next_action.trim().is_empty() {
            continue;
        }
        let strategy_change = reflection
            .summary
            .strategy_change
            .as_deref()
            .unwrap_or("")
            .to_ascii_lowercase();
        if strategy_change == "keep" {
            continue;
        }
        let Some(matched_event) = tool_events.iter().find(|event| {
            event.message_index > reflection.message_index
                && action_matches_tool_event(next_action, event)
        }) else {
            continue;
        };
        let key = normalize_key(next_action);
        let entry = accepted_by_action
            .entry(key)
            .or_insert_with(|| SessionAcceptedStrategy {
                wrong_assumption: reflection
                    .summary
                    .wrong_assumption
                    .as_deref()
                    .map(|value| compact_one_line(value, 160))
                    .unwrap_or_else(|| "previous strategy underfit the real target".to_string()),
                next_minimal_action: compact_one_line(next_action, 160),
                matched_command: compact_one_line(matched_event.signature.as_str(), 180),
                count: 0,
            });
        entry.count = entry.count.saturating_add(1);
    }
    let mut accepted_strategies = accepted_by_action.into_values().collect::<Vec<_>>();
    accepted_strategies.sort_by_key(|entry| {
        (
            std::cmp::Reverse(entry.count),
            std::cmp::Reverse(entry.next_minimal_action.len()),
        )
    });
    accepted_strategies.truncate(3);

    let mut dead_end_counts: BTreeMap<String, u32> = BTreeMap::new();
    for event in &tool_events {
        if event.kind != SessionToolKind::Observation {
            continue;
        }
        *dead_end_counts.entry(event.signature.clone()).or_insert(0) += 1;
    }
    let mut repeated_dead_ends = dead_end_counts
        .into_iter()
        .filter(|(_, count)| *count >= 2)
        .map(|(command, count)| SessionDeadEnd {
            command: compact_one_line(command.as_str(), 180),
            reason: "repeat observation without enough new progress".to_string(),
            count,
        })
        .collect::<Vec<_>>();
    repeated_dead_ends.sort_by_key(|entry| std::cmp::Reverse(entry.count));
    repeated_dead_ends.truncate(3);

    let bridge = SessionBridge {
        last_good_verification,
        accepted_strategies,
        repeated_dead_ends,
    };
    (!bridge.is_empty()).then_some(bridge)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSession {
    pub version: u32,
    pub created_at_ms: u128,
    pub updated_at_ms: u128,

    pub tool_root: Option<String>,
    pub checkpoint: Option<String>,
    pub cur_cwd: Option<String>,

    #[serde(default)]
    pub last_reflection: Option<LastReflectionSummary>,

    #[serde(default)]
    pub recent_reflections: Vec<LastReflectionSummary>,

    #[serde(default)]
    pub observation_cache: Option<ObservationCache>,

    #[serde(default)]
    pub session_bridge: Option<SessionBridge>,

    /// OpenAI-compatible message array (includes tool_calls + tool_call_id).
    pub messages: Vec<serde_json::Value>,
}

impl AgentSession {
    pub const VERSION: u32 = 1;

    #[allow(dead_code)]
    pub fn new(
        tool_root: Option<String>,
        checkpoint: Option<String>,
        cur_cwd: Option<String>,
        observation_cache: Option<ObservationCache>,
        messages: Vec<serde_json::Value>,
    ) -> Self {
        let now = now_ms();
        let last_reflection = last_reflection_summary_from_messages(&messages);
        let recent_reflections = recent_reflection_summaries_from_messages(&messages, 3);
        let session_bridge = session_bridge_from_messages(&messages);
        Self {
            version: Self::VERSION,
            created_at_ms: now,
            updated_at_ms: now,
            tool_root,
            checkpoint,
            cur_cwd,
            last_reflection,
            recent_reflections,
            observation_cache,
            session_bridge,
            messages,
        }
    }

    #[allow(dead_code)]
    pub fn touch(&mut self) {
        self.updated_at_ms = now_ms();
    }

    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read session file: {}", path.display()))?;
        let sess: AgentSession = serde_json::from_str(&text)
            .with_context(|| format!("failed to parse session JSON: {}", path.display()))?;
        if sess.version != Self::VERSION {
            anyhow::bail!(
                "unsupported session version {} (expected {})",
                sess.version,
                Self::VERSION
            );
        }
        Ok(sess)
    }

    #[allow(dead_code)]
    pub fn save_atomic(path: &Path, sess: &AgentSession) -> Result<()> {
        let json = serde_json::to_string_pretty(sess).context("failed to serialize session")?;
        save_text_atomic(path, &json)
    }

    /// Repairs common session corruption patterns so the agent can resume.
    /// Returns a short warning string if the message list was modified.
    pub fn repair_for_resume(&mut self) -> Option<String> {
        let mut pending_ids: Vec<String> = Vec::new();
        let mut pending_started_at: Option<usize> = None;
        let mut trim_from: Option<usize> = None;
        let mut reason: Option<String> = None;

        for (idx, msg) in self.messages.iter().enumerate() {
            let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("");
            match role {
                "assistant" => {
                    let tool_calls = msg.get("tool_calls");
                    let has_tool_calls = tool_calls
                        .and_then(|tc| tc.as_array())
                        .map(|a| !a.is_empty())
                        .unwrap_or(false);
                    if has_tool_calls {
                        if !pending_ids.is_empty() {
                            trim_from = pending_started_at.or(Some(idx));
                            reason = Some(
                                "found a new assistant tool_call before the previous tool_call completed"
                                    .to_string(),
                            );
                            break;
                        }
                        let ids: Vec<String> = tool_calls
                            .and_then(|tc| tc.as_array())
                            .into_iter()
                            .flatten()
                            .filter_map(|tc| {
                                tc.get("id").and_then(|v| v.as_str()).map(|s| s.to_string())
                            })
                            .collect();
                        if !ids.is_empty() {
                            pending_ids = ids;
                            pending_started_at = Some(idx);
                        }
                    } else if !pending_ids.is_empty() {
                        trim_from = pending_started_at;
                        reason = Some(
                            "found a non-tool assistant message while tool results were still pending"
                                .to_string(),
                        );
                        break;
                    }
                }
                "tool" => {
                    let Some(id) = msg.get("tool_call_id").and_then(|v| v.as_str()) else {
                        trim_from = Some(idx);
                        reason = Some("tool message missing tool_call_id".to_string());
                        break;
                    };
                    if pending_ids.is_empty() {
                        trim_from = Some(idx);
                        reason = Some(
                            "tool result appeared without a preceding assistant tool_call"
                                .to_string(),
                        );
                        break;
                    }
                    if let Some(pos) = pending_ids.iter().position(|p| p == id) {
                        pending_ids.remove(pos);
                        if pending_ids.is_empty() {
                            pending_started_at = None;
                        }
                    } else {
                        trim_from = Some(idx);
                        reason = Some(
                            "tool result tool_call_id did not match the pending tool_call"
                                .to_string(),
                        );
                        break;
                    }
                }
                _ => {
                    if !pending_ids.is_empty() {
                        trim_from = pending_started_at;
                        reason = Some(format!(
                            "found a '{role}' message while tool results were still pending"
                        ));
                        break;
                    }
                }
            }
        }

        if trim_from.is_none() && !pending_ids.is_empty() {
            trim_from = pending_started_at;
            reason = Some("session ended mid tool_call (missing tool results)".to_string());
        }

        let Some(from) = trim_from else {
            return None;
        };

        if from >= self.messages.len() {
            return None;
        }

        let old_len = self.messages.len();
        self.messages.truncate(from);
        let trimmed = old_len - from;
        Some(format!(
            "repaired session: truncated {trimmed} message(s) from index {from} ({})",
            reason.unwrap_or_else(|| "unknown reason".to_string())
        ))
    }
}

fn save_text_atomic(path: &Path, text: &str) -> Result<()> {
    let parent0 = path.parent().unwrap_or_else(|| Path::new("."));
    // For "session.json", `parent()` can be empty ("") which should behave like ".".
    let parent = if parent0.as_os_str().is_empty() {
        Path::new(".")
    } else {
        parent0
    };
    std::fs::create_dir_all(parent)
        .with_context(|| format!("failed to create session directory: {}", parent.display()))?;

    // Best-effort atomic write: write to a temp file in the same directory, then rename.
    let mut tmp = tempfile::NamedTempFile::new_in(parent)
        .with_context(|| format!("failed to create temp file under {}", parent.display()))?;
    use std::io::Write;
    tmp.write_all(text.as_bytes())
        .context("failed to write session temp file")?;
    tmp.flush().ok();

    let tmp_path: PathBuf = tmp.path().to_path_buf();
    match tmp.persist(path) {
        Ok(_) => Ok(()),
        Err(e) => {
            // If persist failed, ensure we don't leave an orphan.
            let _ = std::fs::remove_file(&tmp_path);
            Err(anyhow!("failed to persist session file: {}", e.error))
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct SaveKey {
    messages_len: usize,
    checkpoint: Option<String>,
    cur_cwd: Option<String>,
    observation_cache_hash: u64,
}

#[derive(Serialize)]
struct AgentSessionSnapshot<'a> {
    version: u32,
    created_at_ms: u128,
    updated_at_ms: u128,
    tool_root: Option<&'a str>,
    checkpoint: Option<&'a str>,
    cur_cwd: Option<&'a str>,
    last_reflection: Option<LastReflectionSummary>,
    recent_reflections: Vec<LastReflectionSummary>,
    observation_cache: Option<&'a ObservationCache>,
    session_bridge: Option<SessionBridge>,
    messages: &'a [serde_json::Value],
}

/// Best-effort session auto-saver for long CLI runs.
///
/// Writes an OpenAI-compatible message array (including tool_calls + tool_call_id)
/// to a JSON file atomically, so the agent can resume after crashes or interruptions.
pub struct SessionAutoSaver {
    path: PathBuf,
    created_at_ms: u128,
    observation_cache: Mutex<Option<ObservationCache>>,
    progress_context: Mutex<Option<crate::progress_state::ProgressSaveContext>>,
    last_saved: Mutex<SaveKey>,
    warned: AtomicBool,
}

impl SessionAutoSaver {
    pub fn new(path: PathBuf, existing: Option<&AgentSession>) -> Self {
        let created_at_ms = existing.map(|s| s.created_at_ms).unwrap_or_else(now_ms);
        Self {
            path,
            created_at_ms,
            observation_cache: Mutex::new(existing.and_then(|s| s.observation_cache.clone())),
            progress_context: Mutex::new(None),
            last_saved: Mutex::new(SaveKey::default()),
            warned: AtomicBool::new(false),
        }
    }

    pub fn set_observation_cache(&self, cache: Option<ObservationCache>) {
        let mut slot = self
            .observation_cache
            .lock()
            .expect("SessionAutoSaver observation_cache poisoned");
        *slot = cache;
    }

    pub fn set_progress_context(
        &self,
        context: Option<crate::progress_state::ProgressSaveContext>,
    ) {
        let mut slot = self
            .progress_context
            .lock()
            .expect("SessionAutoSaver progress_context poisoned");
        *slot = context;
    }

    pub fn save_or_error(
        &self,
        tool_root: Option<&str>,
        checkpoint: Option<&str>,
        cur_cwd: Option<&str>,
        messages: &[serde_json::Value],
    ) -> Result<()> {
        let _ = self.save_inner(tool_root, checkpoint, cur_cwd, messages, false)?;
        Ok(())
    }

    /// Returns a warning message only once (for UI display) when autosave fails.
    pub fn save_best_effort(
        &self,
        tool_root: Option<&str>,
        checkpoint: Option<&str>,
        cur_cwd: Option<&str>,
        messages: &[serde_json::Value],
    ) -> Option<String> {
        match self.save_inner(tool_root, checkpoint, cur_cwd, messages, true) {
            Ok(_) => None,
            Err(e) => {
                if !self.warned.swap(true, Ordering::Relaxed) {
                    Some(format!("{e:#}"))
                } else {
                    None
                }
            }
        }
    }

    fn save_inner(
        &self,
        tool_root: Option<&str>,
        checkpoint: Option<&str>,
        cur_cwd: Option<&str>,
        messages: &[serde_json::Value],
        skip_if_unchanged: bool,
    ) -> Result<bool> {
        let observation_cache = self
            .observation_cache
            .lock()
            .expect("SessionAutoSaver observation_cache poisoned")
            .clone();
        let progress_context = self
            .progress_context
            .lock()
            .expect("SessionAutoSaver progress_context poisoned")
            .clone();
        let mut observation_hasher = std::collections::hash_map::DefaultHasher::new();
        observation_cache.hash(&mut observation_hasher);
        let key = SaveKey {
            messages_len: messages.len(),
            checkpoint: checkpoint.map(|s| s.to_string()),
            cur_cwd: cur_cwd.map(|s| s.to_string()),
            observation_cache_hash: observation_hasher.finish(),
        };
        {
            let last = self
                .last_saved
                .lock()
                .expect("SessionAutoSaver last_saved poisoned");
            // Never overwrite a newer save with an older snapshot (e.g., Ctrl+C in the CLI main loop
            // while the agent task has already autosaved progress).
            if key.messages_len < last.messages_len {
                return Ok(false);
            }
            if skip_if_unchanged && *last == key {
                return Ok(false);
            }
        }

        let session_bridge = session_bridge_from_messages(messages);
        let snap = AgentSessionSnapshot {
            version: AgentSession::VERSION,
            created_at_ms: self.created_at_ms,
            updated_at_ms: now_ms(),
            tool_root,
            checkpoint,
            cur_cwd,
            last_reflection: last_reflection_summary_from_messages(messages),
            recent_reflections: recent_reflection_summaries_from_messages(messages, 3),
            observation_cache: observation_cache.as_ref(),
            session_bridge: session_bridge.clone(),
            messages,
        };
        let json = serde_json::to_string_pretty(&snap).context("failed to serialize session")?;
        save_text_atomic(&self.path, &json)?;

        if let (Some(root), Some(context)) = (tool_root, progress_context.as_ref()) {
            let progress = crate::progress_state::RepoProgressState::derive_with_bridge(
                context,
                messages,
                session_bridge.as_ref(),
            );
            let progress_path = crate::progress_state::path_for_root(root);
            progress.save_atomic(&progress_path)?;
        }

        let mut last = self
            .last_saved
            .lock()
            .expect("SessionAutoSaver last_saved poisoned");
        *last = key;

        Ok(true)
    }
}

fn now_ms() -> u128 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_path(prefix: &str, ext: &str) -> PathBuf {
        let n = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        PathBuf::from(format!("{prefix}-{n}.{ext}"))
    }

    #[test]
    fn save_atomic_supports_parentless_paths() {
        // This must work for paths like "session.json" where Path::parent() is empty ("").
        let path = unique_path("spiral-coder-session-test", "json");
        let sess = AgentSession::new(
            None,
            None,
            None,
            None,
            vec![json!({"role":"user","content":"hi"})],
        );
        AgentSession::save_atomic(&path, &sess).expect("save_atomic");
        let text = std::fs::read_to_string(&path).expect("read");
        assert!(text.contains("\"messages\""));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn session_roundtrip_preserves_observation_cache() {
        let path = unique_path("spiral-coder-session-obs", "json");
        let sess = AgentSession::new(
            Some("/tmp/demo".to_string()),
            Some("abc123".to_string()),
            Some("src".to_string()),
            Some(ObservationCache {
                reads: vec![ObservationReadCache {
                    command: "read_file(path=src/main.rs)".to_string(),
                    path: "src/main.rs".to_string(),
                }],
                searches: vec![ObservationSearchCache {
                    command: "search_files(pattern=reflect, dir=src)".to_string(),
                    pattern: "reflect".to_string(),
                    hit_count: 3,
                    paths: vec!["src/tui/agent.rs".to_string()],
                }],
                resolutions: vec![ObservationResolutionCache {
                    query: "tui/events.rs".to_string(),
                    canonical_path: "src/tui/events.rs".to_string(),
                    source: "repo_map:read_file".to_string(),
                }],
            }),
            vec![json!({"role":"user","content":"hi"})],
        );
        AgentSession::save_atomic(&path, &sess).expect("save_atomic");
        let loaded = AgentSession::load(&path).expect("load");
        assert_eq!(
            loaded
                .observation_cache
                .as_ref()
                .and_then(|cache| cache.reads.first())
                .map(|read| read.path.as_str()),
            Some("src/main.rs")
        );
        assert_eq!(
            loaded
                .observation_cache
                .as_ref()
                .and_then(|cache| cache.resolutions.first())
                .map(|resolution| resolution.canonical_path.as_str()),
            Some("src/tui/events.rs")
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn autosaver_rewrites_when_observation_cache_changes() {
        let path = unique_path("spiral-coder-session-autosave", "json");
        let existing = AgentSession::new(
            None,
            None,
            None,
            Some(ObservationCache::default()),
            vec![json!({"role":"user","content":"hi"})],
        );
        let saver = SessionAutoSaver::new(path.clone(), Some(&existing));
        saver
            .save_or_error(None, None, None, &existing.messages)
            .expect("save first");
        saver.set_observation_cache(Some(ObservationCache {
            reads: vec![ObservationReadCache {
                command: "read_file(path=README.md)".to_string(),
                path: "README.md".to_string(),
            }],
            searches: Vec::new(),
            resolutions: Vec::new(),
        }));
        assert!(saver
            .save_best_effort(None, None, None, &existing.messages)
            .is_none());
        let loaded = AgentSession::load(&path).expect("load");
        assert_eq!(
            loaded
                .observation_cache
                .as_ref()
                .map(|cache| cache.reads.len()),
            Some(1)
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn autosaver_writes_repo_progress_when_context_is_set() {
        let dir = tempfile::tempdir().expect("tempdir");
        let session_path = dir.path().join("session.json");
        let tool_root = dir.path().join("tool_root");
        std::fs::create_dir_all(&tool_root).expect("tool_root");
        let existing = AgentSession::new(
            Some(tool_root.to_string_lossy().to_string()),
            None,
            None,
            None,
            vec![
                json!({"role":"user","content":"Fix the failing test with the smallest code change."}),
                json!({"role":"assistant","content":"<plan>\ngoal: fix src/lib.rs\nsteps: 1) patch 2) verify\nacceptance: 1) cargo test passes\nrisks: stale read\nassumptions: src/lib.rs is wrong\n</plan>"}),
                json!({
                    "role":"assistant",
                    "tool_calls": [{
                        "id":"call_patch",
                        "type":"function",
                        "function":{"name":"patch_file","arguments":"{\"path\":\"src/lib.rs\",\"search\":\"bug\",\"replace\":\"fix\"}"}
                    }]
                }),
                json!({"role":"tool","tool_call_id":"call_patch","content":"OK: patched 'src/lib.rs'"}),
            ],
        );
        let saver = SessionAutoSaver::new(session_path.clone(), Some(&existing));
        saver.set_progress_context(Some(crate::progress_state::ProgressSaveContext::new(
            "Fix the failing test with the smallest code change.",
            "fix_existing_files",
            "modify_existing",
        )));
        saver
            .save_or_error(
                Some(tool_root.to_string_lossy().as_ref()),
                None,
                None,
                &existing.messages,
            )
            .expect("save");

        let progress_path =
            crate::progress_state::path_for_root(tool_root.to_string_lossy().as_ref());
        let progress =
            crate::progress_state::RepoProgressState::load(&progress_path).expect("progress");
        assert_eq!(progress.lane, "fix_existing_files");
        assert_eq!(progress.completed_artifacts.len(), 1);
        assert_eq!(progress.completed_artifacts[0].path, "src/lib.rs");
    }

    #[test]
    fn session_bridge_derives_verification_strategy_and_dead_end() {
        let messages = vec![
            json!({"role":"user","content":"Fix the failing test."}),
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_read_1",
                    "type": "function",
                    "function": {"name":"read_file","arguments":"{\"path\":\"src/lib.rs\"}"}
                }]
            }),
            json!({
                "role":"tool",
                "tool_call_id":"call_read_1",
                "content":"[src/lib.rs] (4 lines, 40 bytes)\npub fn greet(name: &str) -> String {"
            }),
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_read_2",
                    "type": "function",
                    "function": {"name":"read_file","arguments":"{\"path\":\"src/lib.rs\"}"}
                }]
            }),
            json!({
                "role":"tool",
                "tool_call_id":"call_read_2",
                "content":"[src/lib.rs] (4 lines, 40 bytes)\npub fn greet(name: &str) -> String {"
            }),
            json!({
                "role":"assistant",
                "content":"<reflect>\nlast_outcome: partial\ngoal_delta: same\nwrong_assumption: reading again would help\nstrategy_change: adjust\nnext_minimal_action: patch src/lib.rs with the smallest fix\n</reflect>"
            }),
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_patch",
                    "type": "function",
                    "function": {"name":"patch_file","arguments":"{\"path\":\"src/lib.rs\",\"search\":\"?\",\"replace\":\"!\"}"}
                }]
            }),
            json!({
                "role":"tool",
                "tool_call_id":"call_patch",
                "content":"OK: patched 'src/lib.rs'"
            }),
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_test",
                    "type": "function",
                    "function": {"name":"exec","arguments":"{\"command\":\"cargo test 2>&1\"}"}
                }]
            }),
            json!({
                "role":"tool",
                "tool_call_id":"call_test",
                "content":"OK (exit_code: 0)\nstdout:\n1 passed\n"
            }),
        ];

        let bridge = session_bridge_from_messages(&messages).expect("session bridge");

        assert_eq!(
            bridge
                .last_good_verification
                .as_ref()
                .map(|verification| verification.command.as_str()),
            Some("cargo test 2>&1")
        );
        assert_eq!(bridge.accepted_strategies.len(), 1);
        assert_eq!(
            bridge.accepted_strategies[0].matched_command,
            "patch_file(path=src/lib.rs)"
        );
        assert_eq!(bridge.repeated_dead_ends.len(), 1);
        assert_eq!(bridge.repeated_dead_ends[0].count, 2);
    }

    #[test]
    fn session_roundtrip_preserves_session_bridge() {
        let path = unique_path("spiral-coder-session-bridge", "json");
        let sess = AgentSession::new(
            Some("/tmp/demo".to_string()),
            None,
            None,
            None,
            vec![
                json!({"role":"user","content":"Fix the failing test."}),
                json!({
                    "role": "assistant",
                    "tool_calls": [{
                        "id": "call_test",
                        "type": "function",
                        "function": {"name":"exec","arguments":"{\"command\":\"cargo test 2>&1\"}"}
                    }]
                }),
                json!({
                    "role":"tool",
                    "tool_call_id":"call_test",
                    "content":"OK (exit_code: 0)\nstdout:\n1 passed\n"
                }),
            ],
        );
        AgentSession::save_atomic(&path, &sess).expect("save_atomic");
        let loaded = AgentSession::load(&path).expect("load");

        assert_eq!(
            loaded
                .session_bridge
                .as_ref()
                .and_then(|bridge| bridge.last_good_verification.as_ref())
                .map(|verification| verification.command.as_str()),
            Some("cargo test 2>&1")
        );
        let _ = std::fs::remove_file(&path);
    }
}
