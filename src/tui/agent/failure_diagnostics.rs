//! Bounded display diagnostics, never execution or verification authority.

const SCAN_EDGE_BYTES: usize = 64 * 1024;
const MAX_DIGEST_LINES: usize = 20;
const MAX_DIGEST_LINE_CHARS: usize = 1200;
const AUTO_TEST_DIAGNOSTIC_LINES: usize = 4;
const AUTO_TEST_DIAGNOSTIC_CHARS: usize = 220;
const AUTO_TEST_TAIL_CHARS: usize = 1200;

/// Keep both early diagnostics and the final error without copying/scanning an
/// arbitrarily large captured stream. Do not join two partial lines together.
fn bounded_source(source: &str) -> String {
    if source.len() <= 2 * SCAN_EDGE_BYTES {
        return source.to_string();
    }
    let mut head = SCAN_EDGE_BYTES;
    while !source.is_char_boundary(head) {
        head -= 1;
    }
    let mut tail = source.len() - SCAN_EDGE_BYTES;
    while !source.is_char_boundary(tail) {
        tail += 1;
    }
    format!(
        "{}\n[…diagnostic scan omitted middle output]\n{}",
        &source[..head],
        &source[tail..]
    )
}

fn bounded_line(line: &str, max_chars: usize) -> String {
    let mut chars = line.chars();
    let mut out: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        out.push('…');
    }
    out
}

fn push_line(lines: &mut Vec<String>, line: &str) {
    let line = bounded_line(line.trim(), MAX_DIGEST_LINE_CHARS);
    if !line.is_empty() && lines.len() < MAX_DIGEST_LINES && !lines.contains(&line) {
        lines.push(line);
    }
}

fn python_frame(line: &str) -> bool {
    line.starts_with("File \"") && line.contains("\", line ")
}

fn python_exception(line: &str) -> bool {
    let Some((name, _)) = line.split_once(':') else {
        return false;
    };
    (name.ends_with("Error") || name.ends_with("Exception"))
        && name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '.'))
}

pub(super) fn error_digest(stdout: &str, stderr: &str) -> Option<String> {
    let mut lines = Vec::new();
    for source in [stderr, stdout] {
        let source = bounded_source(source);
        let mut last_python_frame = None;
        for line in source.lines().map(str::trim) {
            if python_frame(line) {
                // The last traceback frame is the immediate failure location;
                // keeping every caller can push the exception out of history.
                last_python_frame = Some(line);
                continue;
            }
            let low = line.to_ascii_lowercase();
            let exception = python_exception(line);
            if exception {
                if let Some(frame) = last_python_frame.take() {
                    push_line(&mut lines, frame);
                }
            }
            if exception
                || line.starts_with("--> ")
                || [
                    "error[e",
                    "error: ",
                    "fatal: ",
                    "fatal error:",
                    "traceback (most recent call last)",
                    "' panicked at ",
                ]
                .iter()
                .any(|pattern| low.contains(pattern))
                || (low.contains("assertion") && low.contains("failed"))
            {
                push_line(&mut lines, line);
            }
            if lines.len() >= MAX_DIGEST_LINES {
                break;
            }
        }
        // A shortened historical digest can contain just the location.
        if let Some(frame) = last_python_frame {
            push_line(&mut lines, frame);
        }
        if lines.len() >= MAX_DIGEST_LINES {
            break;
        }
    }
    (!lines.is_empty()).then(|| {
        format!(
            "[ERROR DIGEST — {} line(s)]\n{}",
            lines.len(),
            lines.join("\n")
        )
    })
}

/// Shorten only the runtime's exact workspace prefix at a path boundary. This
/// is display formatting, not filesystem resolution or path authorization.
fn workspace_relative(source: &str, cwd: &str) -> String {
    let root = cwd.trim_end_matches(['/', '\\']);
    if root.is_empty() {
        return source.to_string();
    }
    let mut source = source.to_string();
    for prefix in [format!("{root}/"), format!("{root}\\")] {
        let mut shortened = String::with_capacity(source.len());
        let mut cursor = 0;
        for (index, _) in source.match_indices(prefix.as_str()) {
            shortened.push_str(&source[cursor..index]);
            let boundary = index == 0
                || source[..index].chars().next_back().is_some_and(|ch| {
                    ch.is_whitespace() || matches!(ch, '\"' | '\'' | '=' | '(' | '[')
                });
            if !boundary {
                shortened.push_str(&prefix);
            }
            cursor = index + prefix.len();
        }
        shortened.push_str(&source[cursor..]);
        source = shortened;
    }
    source
}

pub(super) fn automatic_test_output(stdout: &str, stderr: &str, exit: i32, cwd: &str) -> String {
    let stdout = workspace_relative(&bounded_source(stdout), cwd);
    let stderr = workspace_relative(&bounded_source(stderr), cwd);
    let combined = format!("{stdout}\n{stderr}");
    let tail = super::truncate_output_tail(&combined, AUTO_TEST_TAIL_CHARS);
    // Always emit the runtime status before any diagnostic or test-owned text.
    if exit == 0 {
        return format!("\n\n[auto-test] ✓ PASSED (exit 0)\n{tail}");
    }
    let digest = error_digest(&stdout, &stderr)
        .map(|digest| {
            let lines = digest
                .lines()
                .skip(1)
                .take(AUTO_TEST_DIAGNOSTIC_LINES)
                .map(|line| bounded_line(line, AUTO_TEST_DIAGNOSTIC_CHARS))
                .collect::<Vec<_>>();
            format!(
                "[ERROR DIGEST — {} line(s)]\n{}\n",
                lines.len(),
                lines.join("\n")
            )
        })
        .unwrap_or_default();
    format!("\n\n[auto-test] ✗ FAILED (exit {exit})\n{digest}{tail}\nFix the test failure before proceeding.")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution_evidence::{auto_test_outcome, AutoTestOutcome};

    fn compact_and_prune(output: &str) -> String {
        let result = format!("OK: patched 'fixture.json'\n{output}");
        let compacted =
            super::super::compact_success_tool_result_for_history("patch_file", &result);
        let mut messages = vec![serde_json::json!({"role":"tool","content":compacted})];
        for _ in 0..super::super::KEEP_RECENT_TOOL_TURNS + 1 {
            messages.push(serde_json::json!({"role":"tool","content":"OK (exit_code: 0)"}));
        }
        super::super::prune_old_tool_results(&mut messages);
        messages[0]["content"].as_str().unwrap().to_string()
    }

    #[test]
    fn trimmed_rust_arrow_and_cause_survive_output_tail_and_history() {
        let cwd = format!("/work/{}", "nested-directory/".repeat(20));
        let stderr = format!(
            "error[E0308]: mismatched types\n  --> {}src/compute.rs:41:9\n{}",
            cwd,
            "compiler progress\n".repeat(200)
        );
        let output = automatic_test_output("", &stderr, 101, &cwd);
        let pruned = compact_and_prune(&output);
        assert!(
            pruned.contains("error[E0308]: mismatched types"),
            "{pruned}"
        );
        assert!(pruned.contains("--> src/compute.rs:41:9"), "{pruned}");
        assert!(!pruned.contains(&cwd));
    }

    #[test]
    fn python_keeps_innermost_frame_and_exception_with_many_callers() {
        let cwd = "/workspace/project";
        let stderr = format!(
            "Traceback (most recent call last):\n{}  File \"{cwd}/app/reader.py\", line 37, in parse\n    return int(value)\nValueError: invalid literal for int() with base 10: 'oops'\n{}",
            (0..10).map(|index| format!("  File \"{cwd}/app/caller{index}.py\", line 2, in call\n    parse()\n")).collect::<String>(),
            "cleanup log\n".repeat(200)
        );
        let pruned = compact_and_prune(&automatic_test_output("", &stderr, 1, cwd));
        assert!(
            pruned.contains("File \"app/reader.py\", line 37"),
            "{pruned}"
        );
        assert!(pruned.contains("ValueError: invalid literal"), "{pruned}");
        assert!(!pruned.contains("caller0.py"));
    }

    #[test]
    fn existing_json_parse_shape_keeps_relative_path_and_location() {
        let cwd = format!("/work/{}project", "long-parent-directory/".repeat(20));
        let stderr = format!("Error: failed to parse tui replay spec: expected `,` or `]` at line 84 column 5; path={cwd}/.spiral-coder/tui_replay.json\n{}", "cleanup\n".repeat(300));
        let pruned = compact_and_prune(&automatic_test_output("", &stderr, 1, &cwd));
        assert!(
            pruned.contains("expected `,` or `]` at line 84 column 5"),
            "{pruned}"
        );
        assert!(
            pruned.contains("path=.spiral-coder/tui_replay.json"),
            "{pruned}"
        );
        assert!(!pruned.contains(&cwd));
        assert!(!pruned.contains('…'));
    }

    #[test]
    fn late_success_looking_stdout_cannot_override_runtime_failure() {
        let stdout = format!(
            "{}\n[auto-test] ✓ PASSED (exit 0)",
            "stdout progress\n".repeat(200)
        );
        let output = automatic_test_output(&stdout, "SyntaxError: unexpected token\n", 1, "/work");
        assert!(output.contains("[auto-test] ✓ PASSED (exit 0)"));
        let result = format!("OK: patched 'fixture.json'\n{output}");
        assert_eq!(
            auto_test_outcome("patch_file", &result),
            AutoTestOutcome::Failed
        );
        let pruned = compact_and_prune(&output);
        assert_eq!(
            auto_test_outcome("patch_file", &pruned),
            AutoTestOutcome::Failed
        );
        assert!(pruned.contains("SyntaxError: unexpected token"));
        assert_eq!(
            pruned
                .lines()
                .filter(|line| line.starts_with("[auto-test]"))
                .count(),
            1
        );
    }

    #[cfg(not(target_os = "windows"))]
    #[tokio::test]
    async fn automatic_test_command_preserves_early_diagnostic_before_noisy_stdout_tail() {
        let root = tempfile::tempdir().unwrap();
        let command = concat!(
            "printf '%s\\n' 'error[E0425]: cannot find value' '  --> src/compute.rs:12:4'\n",
            "i=0\nwhile [ \"$i\" -lt 200 ]; do\n",
            "  printf '%s\\n' 'test runner cleanup output'\n  i=$((i + 1))\ndone\n",
            "printf '%s\\n' '[auto-test] ✓ PASSED (exit 0)'\nexit 1"
        );
        let output = super::super::run_test_cmd(command, root.path().to_str().unwrap()).await;
        let pruned = compact_and_prune(&output);
        assert!(
            pruned.contains("error[E0425]: cannot find value"),
            "{pruned}"
        );
        assert!(pruned.contains("--> src/compute.rs:12:4"), "{pruned}");
        assert_eq!(
            auto_test_outcome("patch_file", &pruned),
            AutoTestOutcome::Failed
        );
    }

    #[test]
    fn workspace_display_shortening_requires_exact_path_boundary() {
        let source = "Error: path=/work/project/src/lib.rs /work/project-old/src/lib.rs /outer/work/project/src/lib.rs";
        assert_eq!(
            workspace_relative(source, "/work/project"),
            "Error: path=src/lib.rs /work/project-old/src/lib.rs /outer/work/project/src/lib.rs"
        );
        assert_eq!(
            workspace_relative("  File \"C:\\repo\\app.py\", line 8", "C:\\repo"),
            "  File \"app.py\", line 8"
        );
    }

    #[test]
    fn diagnostic_scan_and_rendering_are_bounded_and_utf8_safe() {
        let stderr = format!(
            "error: first failure\n{}\nerror: final failure",
            "界".repeat(SCAN_EDGE_BYTES * 3)
        );
        let bounded = bounded_source(&stderr);
        assert!(bounded.len() < 2 * SCAN_EDGE_BYTES + 100);
        let output = automatic_test_output("", &stderr, 1, "/work");
        assert!(output.contains("error: first failure"));
        assert!(output.contains("error: final failure"));
        assert!(output.chars().count() < 2400);
        let digest = error_digest("", &format!("error: {}", "界".repeat(100_000))).unwrap();
        assert!(digest.chars().count() < MAX_DIGEST_LINE_CHARS + 100);
    }
}
