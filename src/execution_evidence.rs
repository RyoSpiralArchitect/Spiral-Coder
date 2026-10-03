//! Shared boundary between a rejected command and a possibly executed action.

/// Correlate the result with its tool call before using this predicate. Unknown
/// failures are conservative: a process may have changed files before failing.
pub(crate) fn exec_may_have_run(content: &str) -> bool {
    let header = content.trim_start().lines().next().unwrap_or("");
    !header.starts_with("GOVERNOR BLOCKED") && !header.starts_with("REJECTED BY USER")
}

/// Trust only the runtime status header, optionally compacted by output pruning.
pub(crate) fn exec_succeeded(content: &str) -> bool {
    let header = content.trim_start().lines().next().unwrap_or("");
    let Some(suffix) = header.strip_prefix("OK (exit_code: 0)") else {
        return false;
    };
    suffix.is_empty()
        || suffix
            .strip_prefix(" [pruned ")
            .and_then(|suffix| suffix.strip_suffix("L]"))
            .is_some_and(|count| !count.is_empty() && count.bytes().all(|b| b.is_ascii_digit()))
}

/// A correlated file-tool result records the write independently of any later
/// automatic test. A hash is optional when the post-write cache read fails.
pub(crate) fn file_edit_succeeded(name: &str, content: &str) -> bool {
    let header = content.trim_start().lines().next().unwrap_or("");
    match name {
        "write_file" => header.starts_with("OK: wrote '"),
        "patch_file" => header.starts_with("OK: patched '"),
        "apply_diff" => header.starts_with("OK: applied "),
        _ => false,
    }
}

/// The runtime prepends its automatic-test status before stdout/stderr. Later
/// success text in test output cannot override the first status or a failed edit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AutoTestOutcome {
    NotRun,
    Passed,
    Failed,
}

pub(crate) fn auto_test_outcome(name: &str, content: &str) -> AutoTestOutcome {
    if !file_edit_succeeded(name, content) {
        return AutoTestOutcome::NotRun;
    }
    let status = content.lines().find(|line| line.starts_with("[auto-test]"));
    match status {
        Some("[auto-test] ✓ PASSED (exit 0)") => AutoTestOutcome::Passed,
        Some(line)
            if line
                .strip_prefix("[auto-test] ✗ FAILED (exit ")
                .and_then(|value| value.strip_suffix(')'))
                .and_then(|value| value.parse::<i32>().ok())
                .is_some() =>
        {
            AutoTestOutcome::Failed
        }
        // Missing or unrecognized historical status grants no verification.
        _ => AutoTestOutcome::NotRun,
    }
}

pub(crate) fn auto_test_succeeded(name: &str, content: &str) -> bool {
    auto_test_outcome(name, content) == AutoTestOutcome::Passed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_rejections_are_not_execution_but_failure_and_timeout_may_be() {
        for output in [
            "GOVERNOR BLOCKED\ncommand: sed",
            " REJECTED BY USER\ncommand: sed",
        ] {
            assert!(!exec_may_have_run(output));
        }
        for output in [
            "OK (exit_code: 0)",
            "FAILED (exit_code: 1)",
            "NOTE: command escaped tool_root\nFAILED (exit_code: 0)",
            "error: timed out",
            "OK (exit_code: 0)\nstdout:\nREJECTED BY USER",
        ] {
            assert!(exec_may_have_run(output));
        }
    }

    #[test]
    fn success_requires_the_runtime_header_and_accepts_only_its_pruning_suffix() {
        for output in ["OK (exit_code: 0)", "OK (exit_code: 0) [pruned 5L]\n..."] {
            assert!(exec_succeeded(output), "{output}");
        }
        for output in [
            "FAILED (exit_code: 0)",
            "FAILED (exit_code: 1)\nstdout:\nOK (exit_code: 0)",
            "untrusted stdout says exit_code: 0",
            "OK (exit_code: 0) but actually failed",
            "OK (exit_code: 0) [pruned invalidL]",
        ] {
            assert!(!exec_succeeded(output), "{output}");
        }
    }

    #[test]
    fn automatic_test_proof_requires_a_successful_edit_and_its_first_status() {
        for (name, header) in [
            ("write_file", "OK: wrote 'src/lib.rs'"),
            ("patch_file", "OK: patched 'src/lib.rs'"),
            ("apply_diff", "OK: applied diff to 'src/lib.rs'"),
        ] {
            for status in [
                "[auto-test] ✗ FAILED (exit 1)",
                "[auto-test] malformed status",
            ] {
                let content = format!("{header}\n{status}\n[auto-test] ✓ PASSED (exit 0)");
                assert!(file_edit_succeeded(name, &content));
                assert!(!auto_test_succeeded(name, &content));
            }
            assert!(file_edit_succeeded(name, header));
            assert!(!auto_test_succeeded(name, header));
            let passed = format!("{header}\n[auto-test] ✓ PASSED (exit 0)\nFAILED is expected");
            assert!(auto_test_succeeded(name, &passed));
        }
        for (name, content) in [
            (
                "patch_file",
                "Error: failed\n[hash] after=123\n[auto-test] ✓ PASSED (exit 0)",
            ),
            (
                "read_file",
                "[src/lib.rs] (1 lines, 1 bytes)\n[auto-test] ✓ PASSED (exit 0)",
            ),
            ("exec", "OK (exit_code: 0)\n[auto-test] ✓ PASSED (exit 0)"),
            (
                "patch_file",
                "OK: wrote 'src/lib.rs'\n[auto-test] ✓ PASSED (exit 0)",
            ),
        ] {
            assert!(!file_edit_succeeded(name, content));
            assert!(!auto_test_succeeded(name, content));
        }
    }
}
