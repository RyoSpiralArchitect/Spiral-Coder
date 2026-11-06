use super::*;

/// Trim output that exceeds `max_chars`, appending a truncation notice.
/// This is the biggest single token-saver: a `cargo build` or `ls -R` can
/// return tens of thousands of characters; we only need the key lines.
pub(crate) fn truncate_output(s: &str, max_chars: usize) -> String {
    let s = s.trim_end();
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    let truncated: String = s.chars().take(max_chars).collect();
    let total_lines = s.lines().count();
    format!("{truncated}\n[…truncated — {total_lines} lines total, first {max_chars} chars shown]")
}

/// Tail-biased truncation: errors appear at the END of compiler output, not the beginning.
/// E.g. `cargo build` prints 100+ "Compiling …" lines before the actual error.
/// Showing the last `max_chars` puts the real error in view.
pub(crate) fn truncate_output_tail(s: &str, max_chars: usize) -> String {
    let s = s.trim_end();
    let char_count = s.chars().count();
    if char_count <= max_chars {
        return s.to_string();
    }
    let skip = char_count - max_chars;
    let tail: String = s.chars().skip(skip).collect();
    let total_lines = s.lines().count();
    let shown_lines = tail.lines().count();
    let hidden = total_lines.saturating_sub(shown_lines);
    format!("[…{hidden} earlier lines omitted — showing last {max_chars} chars]\n{tail}")
}

/// Line- and char-bounded preview truncation (used for approval prompts).
pub(crate) fn truncate_preview(s: &str, max_chars: usize, max_lines: usize) -> String {
    let mut out = String::new();
    let mut lines = 0usize;
    for line in s.lines() {
        if lines >= max_lines {
            out.push_str("\n...truncated...\n");
            break;
        }
        if out.len() + line.len() + 1 > max_chars {
            out.push_str("\n...truncated...\n");
            break;
        }
        out.push_str(line);
        out.push('\n');
        lines += 1;
    }
    out.trim_end().to_string()
}

pub(crate) fn compact_one_line(s: &str, max_chars: usize) -> String {
    let flat = s
        .replace("\r\n", "\n")
        .lines()
        .collect::<Vec<_>>()
        .join(" ");
    let collapsed = flat.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut out = String::new();
    let mut n = 0usize;
    for ch in collapsed.chars() {
        if n >= max_chars {
            out.push('…');
            break;
        }
        out.push(ch);
        n += 1;
    }
    if out.is_empty() {
        "-".to_string()
    } else {
        out
    }
}

pub(crate) fn compact_multiline_block(s: &str) -> String {
    s.replace("\r\n", "\n")
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) fn approx_tokens_text(s: &str) -> usize {
    // Rough heuristic:
    // - ASCII-ish text: ~4 chars/token
    // - Non-ASCII (CJK etc): closer to 1 char/token
    let mut ascii = 0usize;
    let mut non_ascii = 0usize;
    for ch in s.chars() {
        if ch.is_ascii() {
            ascii += 1;
        } else {
            non_ascii += 1;
        }
    }
    (ascii + 3) / 4 + non_ascii
}

pub(crate) fn approx_tokens_messages(messages: &[serde_json::Value]) -> usize {
    messages
        .iter()
        .map(|m| {
            m.get("content")
                .and_then(|c| c.as_str())
                .map(approx_tokens_text)
                .unwrap_or(0)
        })
        .sum()
}

pub(crate) fn simple_before_after(old: &str, new: &str) -> String {
    if old == new {
        return "(no changes)".to_string();
    }
    let old_p = truncate_preview(old, 1400, 60);
    let new_p = truncate_preview(new, 1400, 60);
    format!("--- before ---\n{old_p}\n\n--- after ---\n{new_p}\n")
        .trim_end()
        .to_string()
}

/// Extract a bounded digest of causes and locations before output shortening.
/// Returns None when no clear error lines are found.
pub(crate) fn extract_error_digest(stdout: &str, stderr: &str) -> Option<String> {
    failure_diagnostics::error_digest(stdout, stderr)
}

/// Returns true if a tool result can safely be pruned (= it was a success).
/// Failures are never pruned because they are critical context for recovery.
pub(crate) fn is_prunable_tool_result(content: &str) -> bool {
    let c = content.trim_start();
    // exec success
    c.starts_with("OK (exit_code: 0)")
        // write_file / patch_file success
        || c.starts_with("OK: wrote '")
        || c.starts_with("OK: patched '")
        || c.starts_with("OK: applied ")
        // read_file success — header starts with "[path] (N lines"
        || (c.starts_with('[') && c.contains("] (") && c.contains(" lines,"))
        || c.starts_with("[search_files:")
        || c.starts_with("[list_dir:")
        || c.starts_with("[glob")
}

pub(crate) fn push_unique_tool_line(
    out: &mut Vec<String>,
    seen: &mut std::collections::HashSet<String>,
    line: &str,
) {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return;
    }
    let compact = success_output::compact_passed_test_row(trimmed, 220)
        .unwrap_or_else(|| compact_one_line(trimmed, 220));
    if seen.insert(compact.clone()) {
        out.push(compact);
    }
}

pub(crate) fn compact_success_tool_result_for_history(tool_name: &str, content: &str) -> String {
    if matches!(tool_name, "read_file") {
        return content.to_string();
    }
    if content.len() <= SUCCESS_TOOL_HISTORY_MAX_CHARS
        && content.lines().count() <= SUCCESS_TOOL_HISTORY_MAX_LINES
    {
        return content.to_string();
    }

    let lines: Vec<&str> = content.lines().collect();
    if lines.is_empty() {
        return content.to_string();
    }

    let mut kept = Vec::new();
    let mut seen = std::collections::HashSet::new();
    push_unique_tool_line(&mut kept, &mut seen, lines[0]);

    // The first runtime status is authoritative, even when later stdout prints
    // another status. Preserve it verbatim before applying the digest budget.
    if crate::execution_evidence::file_edit_succeeded(tool_name, content) {
        if let Some(status) = lines.iter().find(|line| line.starts_with("[auto-test]")) {
            seen.insert((*status).to_string());
            kept.push((*status).to_string());
            for line in tool_result_history::automatic_test_failure_diagnostics(content) {
                push_unique_tool_line(&mut kept, &mut seen, &line);
            }
        }
    }

    if tool_name == "exec" {
        for line in lines.iter().skip(1).take(3) {
            push_unique_tool_line(&mut kept, &mut seen, line);
        }
    }

    let marker_patterns = [
        "[auto-test]",
        "[hash]",
        "PASSED (exit 0)",
        "FAILED (exit ",
        "✓ auto-verify",
        "✗ auto-verify",
        "test result:",
        "Finished ",
        "running ",
        "cwd:",
        "cwd_after:",
    ];
    for line in &lines {
        let trimmed = line.trim();
        if marker_patterns.iter().any(|pat| trimmed.contains(pat)) {
            push_unique_tool_line(&mut kept, &mut seen, trimmed);
        }
        if kept.len() >= SUCCESS_TOOL_HISTORY_MAX_LINES {
            break;
        }
    }

    let fill_from = match tool_name {
        "search_files" | "list_dir" | "glob" => 1,
        "write_file" | "patch_file" | "apply_diff" => 1,
        _ => 2,
    };
    for line in lines.iter().skip(fill_from) {
        push_unique_tool_line(&mut kept, &mut seen, line);
        if kept.len() >= SUCCESS_TOOL_HISTORY_MAX_LINES {
            break;
        }
    }

    let kept_lines = kept.len();
    let total_lines = lines.len();
    if kept_lines < total_lines {
        kept.insert(
            1.min(kept.len()),
            format!(
                "[history digest — kept {}/{} lines, {} chars]",
                kept_lines,
                total_lines,
                content.len()
            ),
        );
    }
    kept.join("\n")
}

/// Collapse tool result messages older than KEEP_RECENT_TOOL_TURNS to a
/// compact summary, preserving the first automatic-test status if present.
/// Each collapsed result saves ~200-2000 tokens.
/// Only the content field is modified; tool_call_id stays intact.
pub(crate) fn prune_old_tool_results(messages: &mut Vec<serde_json::Value>) {
    let tool_indices: Vec<usize> = messages
        .iter()
        .enumerate()
        .filter(|(_, m)| m["role"].as_str() == Some("tool"))
        .map(|(i, _)| i)
        .collect();

    if tool_indices.len() <= KEEP_RECENT_TOOL_TURNS {
        return;
    }

    let confirming_read = deferred_edit::diagnosis::retained_read(messages);
    let prune_count = tool_indices.len() - KEEP_RECENT_TOOL_TURNS;
    for &idx in tool_indices.iter().take(prune_count) {
        if confirming_read
            .as_deref()
            .is_some_and(|id| messages[idx]["tool_call_id"] == id)
        {
            continue;
        }
        if let Some(content) = messages[idx]["content"].as_str() {
            // Keep execution failures, and retain the automatic-test cause when
            // shortening an otherwise successful edit. Do not repeatedly grow
            // the header of an already bounded multi-line failure receipt.
            if !is_prunable_tool_result(content) {
                continue;
            }
            if content
                .lines()
                .next()
                .is_some_and(|line| line.contains(" [pruned "))
            {
                continue;
            }
            let line_count = content.lines().count();
            if line_count > 2 {
                let first = content.lines().next().unwrap_or("[done]").to_string();
                let mut summary = format!("{first} [pruned {line_count}L]");
                if ["write_file", "patch_file", "apply_diff"]
                    .iter()
                    .any(|name| crate::execution_evidence::file_edit_succeeded(name, content))
                {
                    if let Some(status) =
                        content.lines().find(|line| line.starts_with("[auto-test]"))
                    {
                        summary.push('\n');
                        summary.push_str(status);
                        for diagnostic in
                            tool_result_history::automatic_test_failure_diagnostics(content)
                        {
                            summary.push('\n');
                            summary.push_str(&diagnostic);
                        }
                    }
                }
                messages[idx]["content"] = serde_json::Value::String(summary);
            }
        }
    }
}

pub(crate) fn assistant_message_is_compactable(msg: &serde_json::Value) -> bool {
    if msg.get("role").and_then(|v| v.as_str()) != Some("assistant") {
        return false;
    }
    let content = msg.get("content").and_then(|v| v.as_str()).unwrap_or("");
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return false;
    }
    if trimmed.starts_with("[DONE]")
        || trimmed.contains("[error]")
        || trimmed.contains("GOVERNOR BLOCKED")
    {
        return false;
    }
    msg.get("tool_calls")
        .and_then(|v| v.as_array())
        .map(|items| !items.is_empty())
        .unwrap_or(false)
        || parse_plan_block(trimmed).is_some()
        || parse_think_block(trimmed).is_some()
        || parse_reflection_block(trimmed).is_some()
        || parse_impact_block(trimmed).is_some()
        || parse_evidence_block(trimmed).is_some()
}

pub(crate) fn summarize_assistant_message(msg: &serde_json::Value) -> Option<String> {
    let content = msg.get("content").and_then(|v| v.as_str()).unwrap_or("");
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut parts = Vec::new();
    if let Some(plan) = parse_plan_block(trimmed) {
        parts.push(format!(
            "plan goal={} steps={} acceptance={}",
            compact_one_line(plan.goal.as_str(), 90),
            plan.steps.len(),
            plan.acceptance_criteria.len()
        ));
    }
    if let Some(think) = parse_think_block(trimmed) {
        parts.push(format!(
            "think step={} tool={} next={}",
            think.step,
            compact_one_line(think.tool.as_str(), 24),
            compact_one_line(think.next.as_str(), 90)
        ));
    }
    if let Some(reflect) = parse_reflection_block(trimmed) {
        parts.push(format!(
            "reflect delta={} strategy={} next={}",
            reflect.goal_delta.as_str(),
            reflect.strategy_change.as_str(),
            compact_one_line(reflect.next_minimal_action.as_str(), 90)
        ));
    }
    if let Some(impact) = parse_impact_block(trimmed) {
        parts.push(format!(
            "impact progress={} gap={}",
            compact_one_line(impact.progress.as_str(), 90),
            compact_one_line(impact.remaining_gap.as_str(), 90)
        ));
    }
    if let Some(evidence) = parse_evidence_block(trimmed) {
        parts.push(format!(
            "evidence files={} next_probe={}",
            evidence.target_files.len(),
            compact_one_line(evidence.next_probe.as_str(), 90)
        ));
    }
    if let Some(tool_calls) = msg.get("tool_calls").and_then(|v| v.as_array()) {
        let names = tool_calls
            .iter()
            .filter_map(|tc| {
                tc.get("function")
                    .and_then(|f| f.get("name"))
                    .and_then(|v| v.as_str())
                    .map(str::trim)
                    .filter(|name| !name.is_empty())
                    .map(ToString::to_string)
            })
            .collect::<Vec<_>>();
        if !names.is_empty() {
            parts.push(format!("tools={}", names.join(",")));
        }
    }
    if parts.is_empty() {
        parts.push(compact_one_line(trimmed, 140));
    }
    Some(format!(
        "[assistant-summary] {} [compacted]",
        parts.join(" | ")
    ))
}

pub(crate) fn prune_old_assistant_messages(messages: &mut Vec<serde_json::Value>) {
    let assistant_indices: Vec<usize> = messages
        .iter()
        .enumerate()
        .filter(|(_, msg)| msg.get("role").and_then(|v| v.as_str()) == Some("assistant"))
        .map(|(idx, _)| idx)
        .collect();

    if assistant_indices.len() <= KEEP_RECENT_ASSISTANT_TURNS {
        return;
    }

    strategy_memory::retain_for_resume(messages);
    let prune_count = assistant_indices.len() - KEEP_RECENT_ASSISTANT_TURNS;
    for &idx in assistant_indices.iter().take(prune_count) {
        if crate::streaming::provider_metadata::has_signed_call(&messages[idx])
            || !assistant_message_is_compactable(&messages[idx])
        {
            continue;
        }
        let Some(summary) = summarize_assistant_message(&messages[idx]) else {
            continue;
        };
        messages[idx]["content"] = serde_json::Value::String(summary);
    }
}

/// Build the tool result string for a failed command with structured
/// diagnosis guidance.  Forces the model to reason about the error rather
/// than blindly retrying or continuing.
pub(crate) fn build_failed_tool_output(stdout: &str, stderr: &str, exit_code: i32) -> String {
    let class = classify_error(stderr, stdout);

    let specific_hint = specific_recovery_hint(stderr, stdout);
    let class_hint = error_class_hint(&class);

    let mut hint_prefix = String::new();
    if !specific_hint.is_empty() {
        hint_prefix.push_str(specific_hint);
        hint_prefix.push_str("\n\n");
    }
    if !class_hint.is_empty() {
        hint_prefix.push_str(class_hint);
        hint_prefix.push_str("\n\n");
    }
    let mut out = format!(
        "{hint_prefix}FAILED (exit_code: {exit_code})\n\
         \n\
         ⚠ STOP — diagnosis required before your next action:\n\
         1. Quote the exact line causing the error.\n\
         2. Identify the root cause in one sentence.\n\
         3. Fix the underlying cause with the smallest targeted edit or command. Keep required verification commands unchanged.\n\
         Do NOT continue the original plan until the fix succeeds.\n"
    );

    // Error digest first: compact list of all error lines so the model sees
    // ALL errors even when stdout is long (e.g. cargo build with many "Compiling" lines).
    if let Some(digest) = extract_error_digest(stdout, stderr) {
        out.push_str(&format!("\n{digest}\n"));
    }

    // Tail-biased: errors appear at the END of compiler output, not the beginning.
    // Showing the last N chars ensures the real errors are in view.
    let stdout_t = truncate_output_tail(stdout, MAX_STDOUT_CHARS);
    let stderr_t = failure_diagnostics::failure_excerpt(stderr.trim_end(), MAX_STDERR_CHARS);
    if !stdout_t.is_empty() {
        out.push_str(&format!("\nstdout (tail):\n{stdout_t}\n"));
    }
    if !stderr_t.is_empty() {
        out.push_str(&format!("\nstderr:\n{stderr_t}\n"));
    }
    out
}

pub(crate) fn build_ok_tool_output(stdout: &str) -> String {
    let stdout_t = success_output::stdout_preview(stdout, MAX_STDOUT_CHARS);
    if stdout_t.is_empty() {
        "OK (exit_code: 0)".to_string()
    } else {
        format!("OK (exit_code: 0)\nstdout:\n{stdout_t}")
    }
}

pub(crate) fn wrap_exec_with_pwd(cmd: &str) -> String {
    let raw = cmd.trim();
    if raw.is_empty() {
        return String::new();
    }

    if cfg!(target_os = "windows") {
        // Emit marker even on failures to keep recovery loops from losing cwd.
        // NOTE: if this wrapped script triggers bash->PowerShell translation, the translator MUST
        // preserve standalone `}` lines (see exec.rs).
        return [
            "$ErrorActionPreference = 'Stop'",
            "try {",
            raw,
            "} finally {",
            &format!("Write-Output (\"`n{}\" + (Get-Location).Path)", PWD_MARKER),
            "}",
        ]
        .join("\n");
    }

    // POSIX: keep behavior simple while preserving the wrapped command's exit status.
    format!(
        "{{\n{raw}\n__spiral_coder_status=$?\nprintf '\\n%s\\n' \"{PWD_MARKER}$(pwd)\"\nif [ \"$__spiral_coder_status\" -ne 0 ]; then\n  sh -c \"exit $__spiral_coder_status\"\nelse\n  true\nfi\n}}"
    )
}

pub(crate) fn strip_pwd_marker(stdout_raw: &str) -> (String, Option<String>) {
    let raw = stdout_raw.replace("\r\n", "\n");
    if raw.is_empty() {
        return (String::new(), None);
    }
    let mut kept: Vec<&str> = Vec::new();
    let mut pwd: Option<String> = None;
    for ln in raw.split('\n') {
        if let Some(rest) = ln.strip_prefix(PWD_MARKER) {
            let p = rest.trim();
            if !p.is_empty() {
                pwd = Some(p.to_string());
            }
            continue;
        }
        kept.push(ln);
    }
    (kept.join("\n").trim_end().to_string(), pwd)
}

pub(crate) fn normalize_path_sep(s: &str) -> String {
    s.replace('\\', "/")
}

pub(crate) fn is_within_root(path: &str, root: &str) -> bool {
    let p = normalize_path_sep(path)
        .replace('\u{0}', "")
        .trim()
        .trim_end_matches('/')
        .to_string();
    let r = normalize_path_sep(root)
        .replace('\u{0}', "")
        .trim()
        .trim_end_matches('/')
        .to_string();
    if p.is_empty() || r.is_empty() {
        return false;
    }
    if cfg!(target_os = "windows") {
        let p = p.to_ascii_lowercase();
        let r = r.to_ascii_lowercase();
        p == r || p.starts_with(&(r + "/"))
    } else {
        p == r || p.starts_with(&(r + "/"))
    }
}

pub(crate) fn absolutize_path(path: &str) -> Option<String> {
    let p = path.trim();
    if p.is_empty() {
        return None;
    }
    let pb = std::path::PathBuf::from(p);
    let abs = if pb.is_absolute() {
        pb
    } else {
        let cwd = std::env::current_dir().ok()?;
        cwd.join(pb)
    };
    Some(abs.to_string_lossy().into_owned())
}

pub(crate) fn inject_cwd(tool_output: &str, cwd_line: &str, note: Option<&str>) -> String {
    let t = tool_output.trim_end_matches('\n');
    if t.is_empty() {
        return String::new();
    }
    let (first, rest) = match t.split_once('\n') {
        Some((a, b)) => (a, b),
        None => (t, ""),
    };

    let mut out = String::new();
    out.push_str(first);
    out.push('\n');
    out.push_str(cwd_line);
    if let Some(n) = note {
        if !n.trim().is_empty() {
            out.push('\n');
            out.push_str(n.trim_end());
        }
    }
    if !rest.trim().is_empty() {
        out.push('\n');
        out.push_str(rest);
    }
    out
}
