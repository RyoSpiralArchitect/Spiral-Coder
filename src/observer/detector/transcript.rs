use super::*;

pub fn detect_events_from_transcript(transcript: &str) -> Vec<Event> {
    let mut events: Vec<Event> = Vec::new();

    let mut last_fail_key = String::new();
    let mut same_fail_repeats: u32 = 0;
    let mut loop_emitted = false;

    let normalized = transcript.replace("\r\n", "\n");
    let lines: Vec<&str> = normalized.lines().collect();
    let mut i = 0usize;
    while i < lines.len() {
        let line = lines[i].trim_end();

        // File tool markers as shown in the web UI.
        if let Some(rest) = line.strip_prefix("🔍 search_files:") {
            let tail = rest.trim();
            let (pattern, dir) = match tail.rsplit_once("(dir=") {
                Some((a, b)) => (a.trim(), b.trim().trim_end_matches(')').trim()),
                None => (tail, "."),
            };
            if !pattern.is_empty() {
                events.push(Event::FileScan {
                    tool: FileScanTool::SearchFiles,
                    pattern: pattern.to_string(),
                    dir: dir.to_string(),
                });
            }
            i += 1;
            continue;
        }
        if let Some(rest) = line.strip_prefix("❖ glob:") {
            let tail = rest.trim();
            let (pattern, dir) = match tail.rsplit_once("(dir=") {
                Some((a, b)) => (a.trim(), b.trim().trim_end_matches(')').trim()),
                None => (tail, "."),
            };
            if !pattern.is_empty() {
                events.push(Event::FileScan {
                    tool: FileScanTool::Glob,
                    pattern: pattern.to_string(),
                    dir: dir.to_string(),
                });
            }
            i += 1;
            continue;
        }
        if let Some(rest) = line.strip_prefix("📁 list_dir:") {
            let dir = rest.trim();
            events.push(Event::FileScan {
                tool: FileScanTool::ListDir,
                pattern: String::new(),
                dir: if dir.is_empty() {
                    ".".to_string()
                } else {
                    dir.to_string()
                },
            });
            i += 1;
            continue;
        }

        if let Some(rest) = line.strip_prefix("✎ write_file:") {
            let p = rest.trim();
            if !p.is_empty() {
                record_edit(
                    &mut events,
                    "write_file",
                    p,
                    lines.get(i + 1).copied().unwrap_or(""),
                );
            }
            i += 1;
            continue;
        }
        if let Some(rest) = line.strip_prefix("✎ patch_file:") {
            let p = rest.trim();
            if !p.is_empty() {
                record_edit(
                    &mut events,
                    "patch_file",
                    p,
                    lines.get(i + 1).copied().unwrap_or(""),
                );
            }
            i += 1;
            continue;
        }
        if let Some(rest) = line.strip_prefix("⟁ apply_diff:") {
            let tail = rest.trim();
            if !tail.is_empty() {
                let (path, meta) = match tail.rsplit_once('(') {
                    Some((a, b)) if b.contains("chars") || b.contains("hunks") => (a.trim(), b),
                    _ => (tail, ""),
                };
                if !path.is_empty() {
                    record_edit(
                        &mut events,
                        "apply_diff",
                        path,
                        lines.get(i + 1).copied().unwrap_or(""),
                    );
                }

                let meta_low = meta.to_ascii_lowercase();
                let mut diff_chars: Option<usize> = None;
                let mut hunks: Option<usize> = None;
                for tok in meta_low.split(|c: char| !c.is_ascii_digit()) {
                    if tok.is_empty() {
                        continue;
                    }
                    if diff_chars.is_none() {
                        diff_chars = tok.parse::<usize>().ok();
                        continue;
                    }
                    if hunks.is_none() {
                        hunks = tok.parse::<usize>().ok();
                        break;
                    }
                }
                if let (Some(dc), Some(hk)) = (diff_chars, hunks) {
                    if should_flag_large_diff(dc, hk)
                        && crate::execution_evidence::file_edit_succeeded(
                            "apply_diff",
                            lines.get(i + 1).copied().unwrap_or(""),
                        )
                    {
                        events.push(Event::LargeDiffApplied {
                            path: path.to_string(),
                            diff_chars: dc,
                            hunks: hk,
                        });
                    }
                }
            }
            i += 1;
            continue;
        }

        if !line.starts_with("```") {
            i += 1;
            continue;
        }

        // Fence block.
        let mut fence_lines: Vec<&str> = Vec::new();
        i += 1;
        while i < lines.len() {
            let l = lines[i];
            if l.trim_end() == "```" {
                break;
            }
            fence_lines.push(l);
            i += 1;
        }

        // Skip closing fence if present.
        if i < lines.len() && lines[i].trim_end() == "```" {
            i += 1;
        }

        // Parse command from first prompt line.
        let mut command: Option<String> = None;
        for l in &fence_lines {
            let t = l.trim_start();
            if let Some(rest) = t.strip_prefix("$ ") {
                command = Some(rest.trim().to_string());
                break;
            }
            if let Some(rest) = t.strip_prefix("PS> ") {
                command = Some(rest.trim().to_string());
                break;
            }
        }
        let Some(cmd) = command else {
            continue;
        };
        if cmd.trim().is_empty() {
            continue;
        }

        events.push(Event::CommandExecuted {
            command: cmd.clone(),
        });

        if let Some(reason) = dangerous_command_reason(&cmd) {
            events.push(Event::DangerousCommand {
                command: cmd.clone(),
                reason,
            });
        }

        // Try to parse exit code from the next line ("exit: N") or any fence line.
        let mut exit_code: Option<i32> = None;
        if i < lines.len() {
            if let Some(code) = parse_exit_code(lines[i]) {
                exit_code = Some(code);
            }
        }
        if exit_code.is_none() {
            for l in &fence_lines {
                if let Some(code) = parse_exit_code(l) {
                    exit_code = Some(code);
                    break;
                }
            }
        }

        // Try to parse duration from next lines ("duration_ms: N") or any fence line.
        let mut duration_ms: Option<u64> = None;
        for off in 0..3 {
            if i + off < lines.len() {
                if let Some(ms) = parse_duration_ms(lines[i + off]) {
                    duration_ms = Some(ms);
                    break;
                }
            }
        }
        if duration_ms.is_none() {
            for l in &fence_lines {
                if let Some(ms) = parse_duration_ms(l) {
                    duration_ms = Some(ms);
                    break;
                }
            }
        }
        if let Some(ms) = duration_ms {
            events.push(Event::CommandTiming {
                command: cmd.clone(),
                duration_ms: ms,
            });
        }

        let stderr = fence_lines
            .iter()
            .find_map(|l| {
                let t = l.trim_start();
                if t.len() >= 7
                    && t.get(..7)
                        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("stderr:"))
                {
                    Some(t[7..].trim().to_string())
                } else {
                    None
                }
            })
            .unwrap_or_default();

        let has_breach_marker = fence_lines.iter().any(|l| l.contains("SANDBOX_BREACH:"))
            || fence_lines.iter().any(|l| l.contains("SANDBOX BREACH"));
        let suspicious_marker = fence_lines
            .iter()
            .find(|l| l.contains("SUSPICIOUS_SUCCESS:"))
            .map(|l| l.trim().to_string());

        for l in &fence_lines {
            if let Some((lines_total, truncated_to_chars)) = parse_truncated_marker_line(l.trim()) {
                events.push(Event::LargeCommandOutput {
                    command: cmd.clone(),
                    lines_total: Some(lines_total),
                    truncated_to_chars: Some(truncated_to_chars),
                });
                break;
            }
        }

        let failed = match exit_code {
            Some(code) => code != 0,
            None => false,
        } || has_breach_marker
            || suspicious_marker.is_some();

        if has_breach_marker {
            let detail = fence_lines
                .iter()
                .find(|l| l.contains("SANDBOX_BREACH:") || l.contains("SANDBOX BREACH"))
                .map(|l| l.trim().to_string())
                .unwrap_or_else(|| "SANDBOX_BREACH".to_string());
            events.push(Event::SandboxBreach {
                command: cmd.clone(),
                detail,
            });
        }

        // Only the UI result metadata outside the output fence is a receipt.
        // An exit: 0 line printed by the command is not successful execution.
        if !failed
            && lines
                .get(i)
                .and_then(|line| line.trim().strip_prefix("exit: "))
                .and_then(|value| value.parse::<i32>().ok())
                == Some(0)
        {
            events.push(Event::CommandSucceeded {
                command: cmd.clone(),
            });
        }
        if failed {
            let stderr2 = if let Some(s) = suspicious_marker {
                s
            } else if stderr.trim().is_empty() {
                "(no stderr)".to_string()
            } else {
                stderr
            };
            events.push(Event::CommandFailure {
                command: cmd.clone(),
                stderr: stderr2,
            });

            let key = normalize_command_key(&cmd);
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
                    reason: format!("same failing command repeated {} times", same_fail_repeats),
                });
            }
        }
    }

    events
}

fn record_edit(events: &mut Vec<Event>, tool: &str, path: &str, result: &str) {
    if crate::execution_evidence::file_edit_succeeded(tool, result) {
        events.push(Event::FileWritten {
            path: path.to_string(),
        });
    } else {
        events.push(Event::FileEditUnconfirmed {
            path: path.to_string(),
        });
    }
}
