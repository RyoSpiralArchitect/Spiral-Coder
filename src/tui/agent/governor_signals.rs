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

/// Iteration relief granted to governor-blocked turns: a rejected call executed
/// nothing, so it cannot be the run's productive work.
pub(super) const GOVERNOR_BLOCK_ITER_RELIEF: usize = 4;

pub(super) fn count_governor_blocked_tool_turns(messages: &[serde_json::Value]) -> usize {
    crate::task_origin::current_task_messages(messages)
        .iter()
        .filter(|msg| {
            msg.get("role").and_then(|v| v.as_str()) == Some("tool")
                && msg
                    .get("content")
                    .and_then(|v| v.as_str())
                    .is_some_and(|content| content.contains("GOVERNOR BLOCKED"))
        })
        .count()
}

fn governor_block_signature(content: &str) -> &str {
    content
        .split("GOVERNOR BLOCKED")
        .nth(1)
        .unwrap_or("")
        .lines()
        .find(|line| !line.trim().is_empty())
        .map(|line| line.trim())
        .unwrap_or("")
}

/// Blocked turns earn back at most one iteration each, and only once a later
/// tool result proves the block was recovered from. A block with no successful
/// later tool turn is still the run's open wound — extending the budget there
/// cannot help — and identical repeats of the same gate reason earn only once.
pub(super) fn governor_block_iter_relief(messages: &[serde_json::Value]) -> usize {
    let task = crate::task_origin::current_task_messages(messages);
    let is_block = |msg: &serde_json::Value| {
        msg.get("role").and_then(|v| v.as_str()) == Some("tool")
            && msg
                .get("content")
                .and_then(|v| v.as_str())
                .is_some_and(|content| content.contains("GOVERNOR BLOCKED"))
    };
    let mut signatures = std::collections::BTreeSet::new();
    for (index, msg) in task.iter().enumerate() {
        if !is_block(msg) {
            continue;
        }
        let recovered = task[index + 1..].iter().any(|later| {
            later.get("role").and_then(|v| v.as_str()) == Some("tool") && !is_block(later)
        });
        if recovered {
            let content = msg["content"].as_str().unwrap_or("");
            signatures.insert(governor_block_signature(content).to_string());
        }
    }
    signatures.len().min(GOVERNOR_BLOCK_ITER_RELIEF)
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

pub(crate) fn strip_matching_quotes(s: &str) -> &str {
    let bytes = s.as_bytes();
    if bytes.len() >= 2
        && ((bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[bytes.len() - 1] == b'\''))
    {
        &s[1..s.len() - 1]
    } else {
        s
    }
}

pub(crate) fn split_top_level_args(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut depth = 0usize;
    let mut in_quote: Option<char> = None;
    let mut escaped = false;

    for ch in s.chars() {
        if escaped {
            cur.push(ch);
            escaped = false;
            continue;
        }
        match in_quote {
            Some(q) => {
                cur.push(ch);
                if ch == '\\' {
                    escaped = true;
                } else if ch == q {
                    in_quote = None;
                }
            }
            None => match ch {
                '"' | '\'' => {
                    in_quote = Some(ch);
                    cur.push(ch);
                }
                '(' | '[' | '{' => {
                    depth = depth.saturating_add(1);
                    cur.push(ch);
                }
                ')' | ']' | '}' => {
                    depth = depth.saturating_sub(1);
                    cur.push(ch);
                }
                ',' if depth == 0 => {
                    let part = cur.trim();
                    if !part.is_empty() {
                        out.push(part.to_string());
                    }
                    cur.clear();
                }
                _ => cur.push(ch),
            },
        }
    }

    let tail = cur.trim();
    if !tail.is_empty() {
        out.push(tail.to_string());
    }
    out
}

pub(crate) fn canonicalize_arg_value(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => compact_one_line(s.trim(), 160),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Null => "null".to_string(),
        _ => compact_one_line(&value.to_string(), 160),
    }
}

pub(crate) fn canonicalize_named_command(name: &str, args: &[(String, String)]) -> Option<String> {
    let normalized_name = name
        .trim()
        .trim_matches('<')
        .trim_matches('>')
        .trim()
        .to_ascii_lowercase();
    if normalized_name.is_empty() {
        return None;
    }
    let mut normalized_args = args
        .iter()
        .filter_map(|(key, value)| {
            let key = key.trim().to_ascii_lowercase();
            let value = compact_one_line(strip_matching_quotes(value.trim()).trim(), 160);
            if key.is_empty() || value.is_empty() || value == "-" {
                None
            } else {
                Some((key, value))
            }
        })
        .collect::<Vec<_>>();
    normalized_args.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    let joined = normalized_args
        .into_iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join(", ");
    Some(format!("{normalized_name}({joined})"))
}

pub(crate) fn canonicalize_tool_call_command(name: &str, arguments: &str) -> Option<String> {
    if name.trim().eq_ignore_ascii_case("exec") {
        return parse_exec_command_from_args(arguments).map(|cmd| compact_one_line(&cmd, 200));
    }
    let value = serde_json::from_str::<serde_json::Value>(arguments).ok()?;
    let obj = value.as_object()?;
    let args = obj
        .iter()
        .map(|(key, value)| (key.clone(), canonicalize_arg_value(value)))
        .collect::<Vec<_>>();
    canonicalize_named_command(name, &args)
}

pub(crate) fn parse_named_command_signature(
    command: &str,
) -> Option<(String, std::collections::BTreeMap<String, String>)> {
    let trimmed = command.trim();
    let open_idx = trimmed.find('(')?;
    if !trimmed.ends_with(')') {
        return None;
    }
    let name = trimmed[..open_idx].trim().to_ascii_lowercase();
    if name.is_empty() {
        return None;
    }
    let inner = &trimmed[open_idx + 1..trimmed.len() - 1];
    let mut args = std::collections::BTreeMap::new();
    for part in split_top_level_args(inner) {
        let (key, value) = part.split_once('=')?;
        let key = key.trim().to_ascii_lowercase();
        let value = compact_one_line(strip_matching_quotes(value.trim()).trim(), 160);
        if !key.is_empty() && !value.is_empty() && value != "-" {
            args.insert(key, value);
        }
    }
    Some((name, args))
}

pub(crate) fn canonicalize_evidence_command(command: &str) -> String {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if let Some(open_idx) = trimmed.find('(') {
        if trimmed.ends_with(')') {
            let name = trimmed[..open_idx].trim();
            if !name.contains(char::is_whitespace) {
                let inner = &trimmed[open_idx + 1..trimmed.len() - 1];
                let args = split_top_level_args(inner)
                    .into_iter()
                    .filter_map(|part| {
                        let (key, value) = part.split_once('=')?;
                        Some((key.trim().to_string(), value.trim().to_string()))
                    })
                    .collect::<Vec<_>>();
                if let Some(sig) = canonicalize_named_command(name, &args) {
                    return sig;
                }
            }
        }
    }

    let trimmed = trimmed
        .split_once(" (")
        .map(|(head, _)| head.trim())
        .unwrap_or(trimmed);
    let low = trimmed.to_ascii_lowercase();
    if low.starts_with("read_file of ") || low.starts_with("read file of ") {
        let path = trimmed
            .split_once(" of ")
            .map(|(_, rhs)| {
                rhs.trim()
                    .trim_matches('`')
                    .trim_matches('"')
                    .trim_matches('\'')
            })
            .unwrap_or("")
            .trim();
        if !path.is_empty() {
            if let Some(sig) =
                canonicalize_named_command("read_file", &[("path".to_string(), path.to_string())])
            {
                return sig;
            }
        }
    }
    if let Some(rest) = trimmed.strip_prefix("read_file ") {
        if let Some(sig) = canonicalize_named_command(
            "read_file",
            &[(
                "path".to_string(),
                rest.trim()
                    .trim_matches('`')
                    .trim_matches('"')
                    .trim_matches('\'')
                    .to_string(),
            )],
        ) {
            return sig;
        }
    }
    if let Some(rest) = trimmed.strip_prefix("list_dir ") {
        if let Some(sig) =
            canonicalize_named_command("list_dir", &[("path".to_string(), rest.trim().to_string())])
        {
            return sig;
        }
    }
    if let Some(rest) = trimmed.strip_prefix("glob ") {
        if let Some(sig) =
            canonicalize_named_command("glob", &[("pattern".to_string(), rest.trim().to_string())])
        {
            return sig;
        }
    }
    if low.starts_with("search_files for ") || low.starts_with("search files for ") {
        let normalized = trimmed
            .replace("search files for ", "search_files for ")
            .replace(" in `", " in ")
            .replace(" in \"", " in ")
            .replace(" in '", " in ");
        let rest = normalized
            .strip_prefix("search_files for ")
            .unwrap_or(normalized.as_str());
        if let Some((pattern, dir)) = rest.rsplit_once(" in ") {
            let pattern = pattern
                .trim()
                .trim_matches('`')
                .trim_matches('"')
                .trim_matches('\'')
                .trim();
            let dir = dir
                .trim()
                .trim_matches('`')
                .trim_matches('"')
                .trim_matches('\'')
                .trim_end_matches('/')
                .trim();
            if let Some(sig) = canonicalize_named_command(
                "search_files",
                &[
                    ("pattern".to_string(), pattern.to_string()),
                    ("dir".to_string(), dir.to_string()),
                ],
            ) {
                return sig;
            }
        }
    }
    if low.starts_with("search_files with pattern ")
        || low.starts_with("search files with pattern ")
    {
        let normalized = trimmed
            .replace("search files with pattern ", "search_files with pattern ")
            .replace(" in `", " in ")
            .replace(" in \"", " in ")
            .replace(" in '", " in ");
        let rest = normalized
            .strip_prefix("search_files with pattern ")
            .unwrap_or(normalized.as_str());
        if let Some((pattern, dir)) = rest.rsplit_once(" in ") {
            let pattern = pattern
                .trim()
                .trim_matches('`')
                .trim_matches('"')
                .trim_matches('\'')
                .trim();
            let dir = dir
                .trim()
                .trim_matches('`')
                .trim_matches('"')
                .trim_matches('\'')
                .trim_end_matches('/')
                .trim();
            if let Some(sig) = canonicalize_named_command(
                "search_files",
                &[
                    ("pattern".to_string(), pattern.to_string()),
                    ("dir".to_string(), dir.to_string()),
                ],
            ) {
                return sig;
            }
        }
    }
    if let Some(rest) = trimmed.strip_prefix("search_files ") {
        if let Some((pattern, dir)) = rest.rsplit_once(" in ") {
            if let Some(sig) = canonicalize_named_command(
                "search_files",
                &[
                    ("pattern".to_string(), pattern.trim().to_string()),
                    ("dir".to_string(), dir.trim().to_string()),
                ],
            ) {
                return sig;
            }
        }
    }
    if low.starts_with("grep ") {
        let pattern = trimmed
            .split('"')
            .nth(1)
            .or_else(|| trimmed.split('\'').nth(1))
            .map(|s| s.trim().to_string());
        let path = trimmed
            .split_whitespace()
            .last()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        if let (Some(pattern), Some(path)) = (pattern, path) {
            if let Some(sig) = canonicalize_named_command(
                "search_files",
                &[("pattern".to_string(), pattern), ("dir".to_string(), path)],
            ) {
                return sig;
            }
        }
    }
    normalize_memory_entry(trimmed)
}
