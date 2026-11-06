use super::*;

pub(super) fn normalize_for_signature_with_limit(s: &str, max_len: usize) -> String {
    // Keep this tiny: lowercased + digits collapsed removes most "At line:123" noise.
    let mut out = String::with_capacity(s.len().min(max_len));
    for ch in s.chars() {
        if ch.is_ascii_digit() {
            out.push('#');
        } else {
            out.push(ch.to_ascii_lowercase());
        }
        if out.len() >= max_len {
            break;
        }
    }
    out
}

pub(super) fn normalize_for_signature(s: &str) -> String {
    normalize_for_signature_with_limit(s, 160)
}

pub(super) fn text_contains_any(haystack: &str, terms: &[String]) -> bool {
    terms
        .iter()
        .any(|term| !term.is_empty() && haystack.contains(term))
}

pub(super) fn command_sig(command: &str) -> String {
    command_sig_with_limit(command, 160)
}

pub(super) fn command_sig_with_limit(command: &str, max_len: usize) -> String {
    // Single line, trimmed, collapsed whitespace.
    let normalized = command.replace("\r\n", "\n");
    let one = normalized
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .trim();
    let collapsed = one.split_whitespace().collect::<Vec<_>>().join(" ");
    normalize_for_signature_with_limit(&collapsed, max_len)
}

pub(super) fn blocked_tool_call_signature(name: &str, arguments: &str) -> String {
    canonicalize_tool_call_command(name, arguments)
        .unwrap_or_else(|| format!("{name}:{}", normalize_for_signature(arguments)))
}

pub(super) fn assistant_blocked_tool_call_signature(msg: &serde_json::Value) -> Option<String> {
    let tool_calls = msg.get("tool_calls")?.as_array()?;
    let tc = tool_calls.first()?;
    let name = tc
        .get("function")
        .and_then(|f| f.get("name"))
        .and_then(|v| v.as_str())?
        .trim();
    let arguments = tc
        .get("function")
        .and_then(|f| f.get("arguments"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    if name.is_empty() {
        None
    } else {
        Some(blocked_tool_call_signature(name, arguments))
    }
}

pub(super) fn assistant_blocked_tool_name(msg: &serde_json::Value) -> Option<String> {
    let tool_calls = msg.get("tool_calls")?.as_array()?;
    let tc = tool_calls.first()?;
    tc.get("function")
        .and_then(|f| f.get("name"))
        .and_then(|v| v.as_str())
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
}

pub(super) fn is_missing_plan_governor_block(msg: &serde_json::Value) -> bool {
    msg.get("role").and_then(|v| v.as_str()) == Some("tool")
        && msg
            .get("content")
            .and_then(|v| v.as_str())
            .map(|content| {
                content.contains("GOVERNOR BLOCKED") && content.contains("Missing valid <plan>")
            })
            .unwrap_or(false)
}

pub(super) fn is_missing_think_governor_block(msg: &serde_json::Value) -> bool {
    msg.get("role").and_then(|v| v.as_str()) == Some("tool")
        && msg
            .get("content")
            .and_then(|v| v.as_str())
            .map(|content| {
                content.contains("GOVERNOR BLOCKED") && content.contains("Missing <think>")
            })
            .unwrap_or(false)
}

pub(super) fn is_missing_gate_governor_block(msg: &serde_json::Value) -> bool {
    is_missing_plan_governor_block(msg) || is_missing_think_governor_block(msg)
}

pub(super) fn consecutive_missing_plan_blocks_for_tool(
    messages: &[serde_json::Value],
    tc: &ToolCallData,
) -> usize {
    let target_sig = blocked_tool_call_signature(&tc.name, &tc.arguments);
    let mut count = 0usize;
    let mut idx = messages.len();
    while idx >= 2 {
        let tool_msg = &messages[idx - 1];
        let assistant_msg = &messages[idx - 2];
        if !is_missing_plan_governor_block(tool_msg) {
            break;
        }
        let Some(sig) = assistant_blocked_tool_call_signature(assistant_msg) else {
            break;
        };
        if sig != target_sig {
            break;
        }
        count = count.saturating_add(1);
        idx -= 2;
    }
    count
}

pub(super) fn consecutive_missing_plan_blocks_for_observation(
    messages: &[serde_json::Value],
) -> usize {
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
        if !mistral_observation_tool(name.as_str()) {
            break;
        }
        count = count.saturating_add(1);
        idx -= 2;
    }
    count
}

pub(super) fn consecutive_missing_gate_blocks_for_observation(
    messages: &[serde_json::Value],
) -> usize {
    let mut count = 0usize;
    let mut idx = messages.len();
    while idx >= 2 {
        let tool_msg = &messages[idx - 1];
        let assistant_msg = &messages[idx - 2];
        if !is_missing_gate_governor_block(tool_msg) {
            break;
        }
        let Some(name) = assistant_blocked_tool_name(assistant_msg) else {
            break;
        };
        if !mistral_observation_tool(name.as_str()) {
            break;
        }
        count = count.saturating_add(1);
        idx -= 2;
    }
    count
}

pub(super) fn consecutive_missing_think_blocks_for_tool(
    messages: &[serde_json::Value],
    tc: &ToolCallData,
) -> usize {
    let target_sig = blocked_tool_call_signature(&tc.name, &tc.arguments);
    let mut count = 0usize;
    let mut idx = messages.len();
    while idx >= 2 {
        let tool_msg = &messages[idx - 1];
        let assistant_msg = &messages[idx - 2];
        if !is_missing_think_governor_block(tool_msg) {
            break;
        }
        let Some(sig) = assistant_blocked_tool_call_signature(assistant_msg) else {
            break;
        };
        if sig != target_sig {
            break;
        }
        count = count.saturating_add(1);
        idx -= 2;
    }
    count
}

pub(super) fn consecutive_missing_think_blocks_for_observation(
    messages: &[serde_json::Value],
) -> usize {
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
        if !mistral_observation_tool(name.as_str()) {
            break;
        }
        count = count.saturating_add(1);
        idx -= 2;
    }
    count
}
