//! Keep bounded failure causes through both result shortening and later pruning.
use super::{compact_one_line, extract_error_digest, interesting_failure_line};

pub(super) fn automatic_test_failure_diagnostics(content: &str) -> Vec<String> {
    let mut lines = content.lines();
    let Some(status) = lines.find(|line| line.starts_with("[auto-test]")) else {
        return Vec::new();
    };
    if !status.starts_with("[auto-test] ✗ FAILED (exit ") {
        return Vec::new();
    }
    let output = lines.collect::<Vec<_>>().join("\n");
    let diagnostics = if let Some(digest) = extract_error_digest("", &output) {
        digest.lines().skip(1).take(4).map(str::to_string).collect()
    } else {
        vec![interesting_failure_line("", &output)]
    };
    diagnostics
        .iter()
        .map(|line| compact_one_line(line, 220))
        .filter(|line| !line.is_empty() && !line.starts_with("[auto-test]"))
        .collect()
}
