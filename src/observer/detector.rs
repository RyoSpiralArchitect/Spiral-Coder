use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileScanTool {
    SearchFiles,
    Glob,
    ListDir,
}

#[derive(Debug, Clone)]
pub enum Event {
    CommandExecuted {
        command: String,
    },
    CommandSucceeded {
        command: String,
    },
    AutomaticTestFailed {
        path: String,
        diagnostic: String,
    },
    AutomaticTestPassed {
        path: String,
    },
    CommandTiming {
        command: String,
        duration_ms: u64,
    },
    CommandFailure {
        command: String,
        stderr: String,
    },
    SandboxBreach {
        command: String,
        detail: String,
    },
    DangerousCommand {
        command: String,
        reason: String,
    },
    LargeDiffApplied {
        path: String,
        diff_chars: usize,
        hunks: usize,
    },
    LargeCommandOutput {
        command: String,
        lines_total: Option<usize>,
        truncated_to_chars: Option<usize>,
    },
    FileScan {
        tool: FileScanTool,
        pattern: String,
        dir: String,
    },
    FileWritten {
        path: String,
    },
    FileEditUnconfirmed {
        path: String,
    },
    LoopDetected {
        reason: String,
    },
}

#[derive(Debug, Clone)]
struct ToolCallMeta {
    name: String,
    args: Option<Value>,
}

fn normalize_command_key(cmd: &str) -> String {
    cmd.trim()
        .to_ascii_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn dangerous_command_reason(command: &str) -> Option<String> {
    let cmd = command.trim();
    if cmd.is_empty() {
        return None;
    }
    let low = cmd.to_ascii_lowercase();

    let has_pipe_to_shell = low.contains("| sh")
        || low.contains("| bash")
        || low.contains("| zsh")
        || low.contains("| pwsh")
        || low.contains("| powershell")
        || low.contains("| iex")
        || low.contains("| invoke-expression");
    let has_remote_fetch = low.contains("curl ")
        || low.contains("curl\t")
        || low.contains("wget ")
        || low.contains("iwr ")
        || low.contains("invoke-webrequest")
        || low.contains("invoke-restmethod");
    if has_pipe_to_shell && has_remote_fetch {
        return Some("security:rce(remote script piped to shell)".to_string());
    }

    if low.contains("invoke-expression")
        || low.contains(" iex ")
        || low.starts_with("iex ")
        || low.contains("| iex")
    {
        return Some("security:rce(invoke-expression)".to_string());
    }

    // Obvious destructive operations (high severity).
    if low.contains("rm -rf /")
        || low.contains("rm -rf /*")
        || low.contains("rm -rf ~")
        || low.contains("rm -rf $home")
        || low.contains("rm -rf $env:home")
        || low.contains("rm -rf $env:userprofile")
        || low.contains("rm -rf .git")
        || low.contains("del /s /q")
        || low.contains("format ")
        || low.contains("mkfs")
        || low.contains("dd if=")
    {
        return Some("security:destructive(filesystem wipe)".to_string());
    }

    // Risky git operations (reliability / data loss).
    if low.contains("git push") && (low.contains("--force") || low.contains("--force-with-lease")) {
        return Some("reliability:git(force push)".to_string());
    }
    if low.contains("git reset") && low.contains("--hard") {
        return Some("reliability:git(reset --hard)".to_string());
    }
    if low.contains("git clean") && (low.contains("-fdx") || low.contains("-xdf")) {
        return Some("reliability:git(clean -fdx)".to_string());
    }

    // Overly permissive permissions (security).
    if low.contains("chmod 777") {
        return Some("security:permissions(chmod 777)".to_string());
    }

    None
}

fn parse_truncated_marker_line(line: &str) -> Option<(usize, usize)> {
    let low = line.to_ascii_lowercase();
    if !low.contains("truncated") || !low.contains("lines total") {
        return None;
    }
    let mut nums: Vec<usize> = Vec::new();
    let mut cur = String::new();
    for ch in low.chars() {
        if ch.is_ascii_digit() {
            cur.push(ch);
        } else if !cur.is_empty() {
            if let Ok(v) = cur.parse::<usize>() {
                nums.push(v);
                if nums.len() >= 2 {
                    break;
                }
            }
            cur.clear();
        }
    }
    if nums.len() < 2 && !cur.is_empty() {
        if let Ok(v) = cur.parse::<usize>() {
            nums.push(v);
        }
    }
    if nums.len() >= 2 {
        Some((nums[0], nums[1]))
    } else {
        None
    }
}

fn extract_truncated_marker(text: &str) -> Option<(usize, usize)> {
    for line in text.lines() {
        if let Some(info) = parse_truncated_marker_line(line.trim()) {
            return Some(info);
        }
    }
    None
}

fn parse_exit_code(text: &str) -> Option<i32> {
    fn parse_int_after_marker(s: &str, marker: &str) -> Option<i32> {
        let idx = s.find(marker)?;
        let rest = s[idx + marker.len()..].trim_start();
        if rest.is_empty() {
            return None;
        }
        let mut out = String::new();
        for (i, ch) in rest.chars().enumerate() {
            if i == 0 && ch == '-' {
                out.push(ch);
                continue;
            }
            if ch.is_ascii_digit() {
                out.push(ch);
            } else {
                break;
            }
        }
        if out.is_empty() || out == "-" {
            None
        } else {
            out.parse::<i32>().ok()
        }
    }

    parse_int_after_marker(text, "exit_code:")
        .or_else(|| parse_int_after_marker(text, "exit_code="))
        .or_else(|| parse_int_after_marker(text, "exit:"))
        .or_else(|| parse_int_after_marker(text, "exit="))
}

fn parse_duration_ms(text: &str) -> Option<u64> {
    fn parse_u64_after_marker(s: &str, marker: &str) -> Option<u64> {
        let low = s.to_ascii_lowercase();
        let idx = low.find(marker)?;
        let rest = s[idx + marker.len()..].trim_start();
        if rest.is_empty() {
            return None;
        }
        let mut out = String::new();
        for ch in rest.chars() {
            if ch.is_ascii_digit() {
                out.push(ch);
            } else {
                break;
            }
        }
        if out.is_empty() {
            None
        } else {
            out.parse::<u64>().ok()
        }
    }

    parse_u64_after_marker(text, "duration_ms:")
        .or_else(|| parse_u64_after_marker(text, "duration_ms="))
}

fn extract_first_stderr_line(text: &str) -> String {
    for line in text.lines() {
        let t = line.trim_start();
        if t.len() < 7 {
            continue;
        }
        if t.get(..7)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("stderr:"))
        {
            return t[7..].trim().to_string();
        }
    }
    String::new()
}

fn extract_sandbox_breach_detail(text: &str) -> Option<String> {
    let mut lines = text.lines();
    while let Some(line) = lines.next() {
        if line.contains("SANDBOX BREACH") {
            let mut out = vec![line.trim().to_string()];
            if let Some(next) = lines.next() {
                let t = next.trim();
                if !t.is_empty() {
                    out.push(t.to_string());
                }
            }
            return Some(out.join("\n"));
        }
        if line
            .to_ascii_lowercase()
            .contains("cwd_after escaped tool_root")
        {
            return Some(line.trim().to_string());
        }
    }
    None
}

fn diff_hunk_count(diff: &str) -> usize {
    diff.lines()
        .filter(|l| l.trim_start().starts_with("@@"))
        .count()
}

fn should_flag_large_diff(diff_chars: usize, hunks: usize) -> bool {
    // Large apply_diff payloads are harder to review and more likely to be wrong.
    diff_chars >= 12_000 || hunks >= 10
}

fn is_tool_error(tool_name: &str, content: &str) -> bool {
    if tool_name == "exec" && crate::execution_evidence::exec_succeeded(content) {
        return false;
    }
    let t = content.trim_start();
    if t.is_empty() {
        return false;
    }
    let low = t.to_ascii_lowercase();
    if low.starts_with("error:") || low.starts_with("error ") || low.starts_with("error\n") {
        return true;
    }
    if low.starts_with("error reading")
        || low.starts_with("error writing")
        || low.starts_with("error patching")
        || low.starts_with("error applying")
        || low.starts_with("error listing")
        || low.starts_with("error searching")
        || low.starts_with("error glob")
    {
        return true;
    }

    // Server file tools return "ERROR: ..." (uppercase).
    if t.starts_with("ERROR") {
        return true;
    }

    if tool_name == "exec" {
        let header = t.lines().next().unwrap_or("");
        if header.to_ascii_lowercase().starts_with("failed (exit_code") {
            return true;
        }
        if let Some(code) = parse_exit_code(header) {
            return code != 0;
        }
    }

    false
}

fn args_string_arg(args: &Value, key: &str) -> Option<String> {
    Some(args.get(key)?.as_str()?.to_string())
}

fn tool_command_from_args(args: Option<&Value>) -> Option<String> {
    let a = args?;
    args_string_arg(a, "command")
        .or_else(|| args_string_arg(a, "cmd"))
        .or_else(|| args_string_arg(a, "script"))
}

fn tool_path_from_args(args: Option<&Value>) -> Option<String> {
    let a = args?;
    args_string_arg(a, "path")
        .or_else(|| args_string_arg(a, "file_path"))
        .or_else(|| args_string_arg(a, "filename"))
}

fn args_search_pattern(args: Option<&Value>) -> Option<String> {
    let a = args?;
    args_string_arg(a, "pattern").or_else(|| args_string_arg(a, "query"))
}

fn args_dir(args: Option<&Value>) -> Option<String> {
    let a = args?;
    args_string_arg(a, "dir").or_else(|| args_string_arg(a, "root"))
}

fn index_tool_calls(messages: &[Value]) -> HashMap<String, ToolCallMeta> {
    let mut out: HashMap<String, ToolCallMeta> = HashMap::new();
    for msg in messages {
        if msg.get("role").and_then(|r| r.as_str()) != Some("assistant") {
            continue;
        }
        let Some(tcs) = msg.get("tool_calls").and_then(|v| v.as_array()) else {
            continue;
        };
        for tc in tcs {
            let Some(id) = tc.get("id").and_then(|x| x.as_str()).map(|s| s.to_string()) else {
                continue;
            };
            let fn_obj = tc.get("function");
            let name = fn_obj
                .and_then(|f| f.get("name"))
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            let arg_str = fn_obj
                .and_then(|f| f.get("arguments"))
                .and_then(|x| x.as_str())
                .unwrap_or("");
            let args = serde_json::from_str::<Value>(arg_str).ok();
            out.insert(id, ToolCallMeta { name, args });
        }
    }
    out
}

pub fn detect_events(messages: &[Value]) -> Vec<Event> {
    let mut tool_calls = HashMap::new();
    let mut events: Vec<Event> = Vec::new();

    let mut last_fail_key = String::new();
    let mut same_fail_repeats: u32 = 0;
    let mut loop_emitted = false;

    for msg in messages {
        if msg.get("role").and_then(|r| r.as_str()) == Some("assistant") {
            tool_calls.extend(index_tool_calls(std::slice::from_ref(msg)));
        }
        if msg.get("role").and_then(|r| r.as_str()) != Some("tool") {
            continue;
        }

        let tool_call_id = msg
            .get("tool_call_id")
            .and_then(|x| x.as_str())
            .unwrap_or("");
        let content = msg.get("content").and_then(|c| c.as_str()).unwrap_or("");

        let meta = tool_calls.remove(tool_call_id);
        let meta = meta.as_ref();
        let tool_name = meta.map(|m| m.name.as_str()).unwrap_or("");

        if tool_name == "exec" {
            if !crate::execution_evidence::exec_may_have_run(content) {
                continue;
            }
            let command = tool_command_from_args(meta.and_then(|m| m.args.as_ref()))
                .unwrap_or_else(|| "(unknown command)".to_string());
            events.push(Event::CommandExecuted {
                command: command.clone(),
            });
            if crate::execution_evidence::exec_succeeded(content) {
                events.push(Event::CommandSucceeded {
                    command: command.clone(),
                });
            }

            if let Some(ms) = parse_duration_ms(content) {
                events.push(Event::CommandTiming {
                    command: command.clone(),
                    duration_ms: ms,
                });
            }

            if let Some(reason) = dangerous_command_reason(&command) {
                events.push(Event::DangerousCommand {
                    command: command.clone(),
                    reason,
                });
            }

            if let Some(detail) = extract_sandbox_breach_detail(content) {
                events.push(Event::SandboxBreach {
                    command: command.clone(),
                    detail,
                });
            }

            if let Some((lines_total, truncated_to_chars)) = extract_truncated_marker(content) {
                events.push(Event::LargeCommandOutput {
                    command: command.clone(),
                    lines_total: Some(lines_total),
                    truncated_to_chars: Some(truncated_to_chars),
                });
            } else if content.to_ascii_lowercase().contains("output truncated") {
                events.push(Event::LargeCommandOutput {
                    command: command.clone(),
                    lines_total: None,
                    truncated_to_chars: None,
                });
            }

            if is_tool_error("exec", content) {
                let stderr = {
                    let s = extract_first_stderr_line(content);
                    if s.trim().is_empty() {
                        let first = content.lines().next().unwrap_or("").trim().to_string();
                        if first.is_empty() {
                            "(no stderr)".to_string()
                        } else {
                            first
                        }
                    } else {
                        s
                    }
                };
                events.push(Event::CommandFailure {
                    command: command.clone(),
                    stderr,
                });

                let key = normalize_command_key(&command);
                if !key.is_empty() && key == last_fail_key {
                    same_fail_repeats = same_fail_repeats.saturating_add(1);
                } else {
                    last_fail_key = key;
                    same_fail_repeats = 1;
                    loop_emitted = false;
                }
                if same_fail_repeats >= 3 && !loop_emitted {
                    loop_emitted = true;
                    events.push(Event::LoopDetected {
                        reason: format!(
                            "same failing command repeated {} times",
                            same_fail_repeats
                        ),
                    });
                }
            }

            continue;
        }

        if tool_name == "search_files" {
            let pattern = args_search_pattern(meta.and_then(|m| m.args.as_ref()))
                .unwrap_or_else(|| "".to_string());
            let dir =
                args_dir(meta.and_then(|m| m.args.as_ref())).unwrap_or_else(|| "".to_string());
            if !pattern.trim().is_empty() {
                events.push(Event::FileScan {
                    tool: FileScanTool::SearchFiles,
                    pattern,
                    dir,
                });
            }
        }
        if tool_name == "glob" {
            let pattern = args_search_pattern(meta.and_then(|m| m.args.as_ref()))
                .unwrap_or_else(|| "".to_string());
            let dir =
                args_dir(meta.and_then(|m| m.args.as_ref())).unwrap_or_else(|| "".to_string());
            if !pattern.trim().is_empty() {
                events.push(Event::FileScan {
                    tool: FileScanTool::Glob,
                    pattern,
                    dir,
                });
            }
        }
        if tool_name == "list_dir" {
            let dir =
                args_dir(meta.and_then(|m| m.args.as_ref())).unwrap_or_else(|| "".to_string());
            events.push(Event::FileScan {
                tool: FileScanTool::ListDir,
                pattern: String::new(),
                dir,
            });
        }

        if matches!(tool_name, "write_file" | "patch_file" | "apply_diff") {
            if !crate::execution_evidence::file_edit_succeeded(tool_name, content) {
                continue;
            }
            let Some(path) = tool_path_from_args(meta.and_then(|m| m.args.as_ref())) else {
                continue;
            };
            if path.trim().is_empty() {
                continue;
            }
            events.push(Event::FileWritten { path: path.clone() });
            match crate::execution_evidence::auto_test_outcome(tool_name, content) {
                crate::execution_evidence::AutoTestOutcome::Failed => {
                    events.push(Event::AutomaticTestFailed {
                        path,
                        diagnostic: content
                            .lines()
                            .skip_while(|line| !line.starts_with("[auto-test]"))
                            .take(6)
                            .collect::<Vec<_>>()
                            .join("\n"),
                    })
                }
                crate::execution_evidence::AutoTestOutcome::Passed => {
                    events.push(Event::AutomaticTestPassed { path })
                }
                crate::execution_evidence::AutoTestOutcome::NotRun => {}
            }

            if tool_name == "apply_diff" {
                if let Some(args) = meta.and_then(|m| m.args.as_ref()) {
                    let diff = args.get("diff").and_then(|d| d.as_str()).unwrap_or("");
                    let diff_chars = diff.len();
                    let hunks = diff_hunk_count(diff);
                    if should_flag_large_diff(diff_chars, hunks) {
                        events.push(Event::LargeDiffApplied {
                            path: tool_path_from_args(Some(args))
                                .unwrap_or_else(|| "(unknown path)".to_string()),
                            diff_chars,
                            hunks,
                        });
                    }
                }
            }
        }
    }

    events
}

mod transcript;
pub use transcript::detect_events_from_transcript;
