//! Shared prose-context detection for final-report clauses and explicit checks.
//!
//! Tracks fenced code blocks, inline quotes, and example/negation sentences so
//! that demonstration or quoted wording is not treated as an authored obligation.

/// Returns true when a sentence begins with an example or negation signal.
///
/// This is intentionally conservative: it matches clear markers such as
/// "for example", "e.g.", "not", "don't", "do not", "never" and "no" so that
/// clauses following them are not promoted into report obligations or exact
/// native checks.
pub(super) fn is_example_or_negation_sentence(sentence: &str) -> bool {
    let first = sentence.split_whitespace().next().unwrap_or("");
    sentence.starts_with("for example")
        || sentence.starts_with("e.g.")
        || sentence.starts_with("for instance")
        || first == "not"
        || first == "not:"
        || first == "don't"
        || sentence.starts_with("do not")
        || first == "never"
        || first == "no"
}

/// Returns true if `line` begins or ends a Markdown/InfoString code fence.
pub(super) fn is_fence_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("```") || trimmed.starts_with("~~~")
}

/// Returns the three-character fence marker for a fence line.
///
/// Panics if called on a non-fence line; callers should guard with
/// `is_fence_line` first.
pub(super) fn fence_marker(line: &str) -> &str {
    let trimmed = line.trim_start();
    &trimmed[..3]
}

/// Returns true when `start` is inside an inline backtick or double-quote span
/// that is still open at that byte position.
pub(super) fn is_inside_inline_quote(line: &str, start: usize) -> bool {
    let prefix = &line[..start];
    let mut quote = None;
    for ch in prefix.chars() {
        if matches!(ch, '`' | '"') {
            if quote == Some(ch) {
                quote = None;
            } else if quote.is_none() {
                quote = Some(ch);
            }
        }
    }
    quote.is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_example_and_negation_openings() {
        for sentence in [
            "for example, use this.",
            "e.g. run exactly: cargo test",
            "for instance, final report must state PASS.",
            "not a real requirement",
            "not: final answer must include x",
            "don't run exactly: cargo test",
            "do not run exactly: cargo test",
            "never run exactly: cargo test",
            "no final report is needed",
        ] {
            assert!(
                is_example_or_negation_sentence(sentence),
                "expected example/negation: {sentence}"
            );
        }
        assert!(!is_example_or_negation_sentence("run exactly: cargo test"));
        assert!(!is_example_or_negation_sentence(
            "final report must state PASS."
        ));
    }

    #[test]
    fn detects_fence_lines() {
        assert!(is_fence_line("```text"));
        assert!(is_fence_line("   ~~~rust"));
        assert!(!is_fence_line("not a fence"));
        assert_eq!(fence_marker("```text"), "```");
        assert_eq!(fence_marker("   ~~~"), "~~~");
    }

    #[test]
    fn detects_open_inline_quotes() {
        assert!(is_inside_inline_quote(
            "`final report must state PASS.` x",
            10
        ));
        assert!(is_inside_inline_quote("\"final report must\" x", 5));
        assert!(!is_inside_inline_quote("`closed` final report must x", 15));
        assert!(!is_inside_inline_quote("final report must x", 5));
    }

    #[test]
    fn recognizes_example_variants_with_punctuation() {
        assert!(is_example_or_negation_sentence(
            "e.g., run exactly: cargo test"
        ));
        assert!(is_example_or_negation_sentence(
            "for example: final report must state PASS."
        ));
    }
}
