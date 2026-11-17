//! Separate a failed assertion's quoted message from the runner's own output.
//!
//! An assertion message quotes the program under test: its stderr, a parse
//! error it reported, or a file it was asked to read. Those words describe the
//! behavior being checked, not the test runner's environment, path or syntax.

/// Whether the output contains a failed-assertion message, and the output with
/// every such message removed.
pub(super) fn without_assertion_messages(text: &str) -> (bool, String) {
    let mut found = false;
    let mut quoting = false;
    let mut rest = String::with_capacity(text.len());
    for line in text.lines() {
        if starts_message(line) {
            found = true;
            quoting = true;
            continue;
        }
        if quoting && !ends_message(line) {
            continue;
        }
        quoting = false;
        rest.push_str(line);
        rest.push('\n');
    }
    (found, rest)
}

/// Python/Node `AssertionError: …`, pytest `E   AssertionError: …`, and Rust
/// ``assertion `left == right` failed: …`` / `assertion failed: …`.
fn starts_message(line: &str) -> bool {
    let low = line.trim_start().to_ascii_lowercase();
    let low = low
        .strip_prefix("e ")
        .map(str::trim_start)
        .unwrap_or(low.as_str());
    low.starts_with("assertionerror")
        || low.starts_with("assertion `left")
        || low.starts_with("assertion failed")
}

/// A message ends at a blank line or at the runner's next report structure.
fn ends_message(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.is_empty()
        || [
            "---",
            "===",
            "Ran ",
            "FAILED",
            "test result:",
            "note: run with",
            "thread '",
        ]
        .iter()
        .any(|prefix| trimmed.starts_with(prefix))
}

#[cfg(test)]
mod tests {
    use super::super::{build_failed_tool_output, classify_error, ErrorClass};
    use super::*;

    // Sealed S20/S21 visible-check shape: the candidate printed this error and the
    // protected test quoted its stderr in the assertion message.
    const S21_VISIBLE_FAILURE: &str = "test_status_summarizes_completed_and_interrupted_records (scripts.status_visible.StatusVisible.test_status_summarizes_completed_and_interrupted_records) ... FAIL

======================================================================
FAIL: test_status_summarizes_completed_and_interrupted_records (scripts.status_visible.StatusVisible.test_status_summarizes_completed_and_interrupted_records)
----------------------------------------------------------------------
Traceback (most recent call last):
  File \"/workspace/scripts/status_visible.py\", line 28, in test_status_summarizes_completed_and_interrupted_records
    self.assertEqual(result.returncode, 0, f'{result.stderr}\\n{context}')
AssertionError: 1 != 0 : Error: No such file or directory (os error 2)
state=stopped; manifest={\"completed\": false}; expected={\"status\": \"stopped\", \"stop_reason\": \"interrupted\", \"version\": 1}

----------------------------------------------------------------------
Ran 1 test in 10.232s

FAILED (failures=1)
";

    #[test]
    fn a_quoted_missing_file_in_a_failed_assertion_is_a_logic_failure() {
        assert_eq!(classify_error(S21_VISIBLE_FAILURE, ""), ErrorClass::Logic);
        let rendered = build_failed_tool_output("", S21_VISIBLE_FAILURE, 1);
        assert!(rendered.starts_with("⚠ LOGIC ERROR"), "{rendered}");

        // The same shape produced by a real unittest run.
        let child = std::process::Command::new("python3")
            .args([
                "-S",
                "-c",
                "import unittest\n\
                 class StatusVisible(unittest.TestCase):\n\
                 \x20   def test_status(self):\n\
                 \x20       self.assertEqual(1, 0, 'Error: No such file or directory (os error 2)\\nstate=stopped')\n\
                 unittest.main()\n",
            ])
            .output()
            .unwrap();
        assert!(!child.status.success());
        let (stdout, stderr) = (
            String::from_utf8_lossy(&child.stdout),
            String::from_utf8_lossy(&child.stderr),
        );
        assert_eq!(classify_error(&stderr, &stdout), ErrorClass::Logic);
    }

    #[test]
    fn quoted_syntax_network_and_rust_assertion_messages_are_logic_failures() {
        for output in [
            "Traceback (most recent call last):\n  File \"t.py\", line 3, in test\nAssertionError: 'parse error at line 1' != ''\n",
            "AssertionError: 'request timed out' not found in 'ok'\n\nFAILED (failures=1)\n",
            "thread 'status' panicked at tests/coupled_status.rs:12:5:\nassertion `left == right` failed: Error: No such file or directory (os error 2)\n  left: 1\n right: 0\nnote: run with `RUST_BACKTRACE=1` environment variable to display a backtrace\n",
            "E       AssertionError: Error: No such file or directory\nE       assert 1 == 0\n",
        ] {
            assert_eq!(classify_error(output, ""), ErrorClass::Logic, "{output}");
        }
    }

    #[test]
    fn genuine_runner_path_syntax_and_environment_failures_keep_their_class() {
        let missing = std::process::Command::new("python3")
            .args(["-S", "definitely_missing_script_for_classification.py"])
            .output()
            .unwrap();
        assert_eq!(
            classify_error(&String::from_utf8_lossy(&missing.stderr), ""),
            ErrorClass::Path
        );
        for (output, class) in [
            (
                "ERROR: test_fixture (t.T.test_fixture)\nTraceback (most recent call last):\nFileNotFoundError: [Errno 2] No such file or directory: 'fixture.json'\n",
                ErrorClass::Path,
            ),
            (
                "FileNotFoundError: [Errno 2] No such file or directory: 'x'\n\nDuring handling of the above exception, another exception occurred:\n\nAssertionError: setup failed\n",
                ErrorClass::Path,
            ),
            (
                "  File \"x.py\", line 1\n    def (\n        ^\nSyntaxError: invalid syntax\n",
                ErrorClass::Syntax,
            ),
            ("bash: cargo: command not found\n", ErrorClass::Environment),
        ] {
            assert_eq!(classify_error(output, ""), class, "{output}");
        }
    }

    #[test]
    fn assertion_messages_end_at_blank_lines_and_report_structure() {
        let (found, rest) = without_assertion_messages(S21_VISIBLE_FAILURE);
        assert!(found);
        assert!(!rest.contains("No such file"));
        assert!(rest.contains("Ran 1 test") && rest.contains("FAILED (failures=1)"));
        let (found, rest) = without_assertion_messages("no assertion here\n");
        assert!(!found);
        assert_eq!(rest, "no assertion here\n");
    }
}
