//! Bounded explicit final-report clauses, shared by prompting and validation.

pub(super) fn instructions(root: &str) -> Vec<&str> {
    let mut clauses = Vec::new();
    for line in root.lines() {
        let lower = line.to_ascii_lowercase();
        let mut starts = Vec::new();
        for marker in [
            "final answer must ",
            "final report must ",
            "final response must ",
            "final summary must ",
        ] {
            for (start, _) in lower.match_indices(marker) {
                let tail = &lower[start + marker.len()..];
                let verb = tail.split_whitespace().next().unwrap_or("");
                if [
                    "include",
                    "name",
                    "state",
                    "list",
                    "describe",
                    "summarize",
                    "explain",
                    "disclose",
                    "mention",
                    "identify",
                ]
                .contains(&verb)
                {
                    starts.push(start + marker.len() + verb.len());
                }
            }
        }
        starts.sort_unstable();
        for start in starts {
            let tail = &line[start..];
            let mut quote = None;
            let end = tail
                .char_indices()
                .find_map(|(index, ch)| {
                    if matches!(ch, '`' | '"') {
                        if quote == Some(ch) {
                            quote = None;
                        } else if quote.is_none() {
                            quote = Some(ch);
                        }
                    }
                    (quote.is_none()
                        && matches!(ch, '.' | '!' | '?')
                        && tail[index + ch.len_utf8()..].starts_with(char::is_whitespace))
                    .then_some(index + ch.len_utf8())
                })
                .unwrap_or(tail.len());
            clauses.push(&tail[..end]);
        }
    }
    clauses
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_explicit_report_variants_without_consuming_other_instructions() {
        for report in ["answer", "report", "response", "summary"] {
            for verb in [
                "include",
                "name",
                "state",
                "list",
                "describe",
                "summarize",
                "explain",
                "disclose",
                "mention",
                "identify",
            ] {
                let root = format!("Your final {report} must {verb} the changed files and actual test outcomes. Preserve the fixtures.");
                assert_eq!(
                    instructions(&root),
                    [" the changed files and actual test outcomes."]
                );
            }
        }
        assert!(
            instructions("The tool generates a final report. No final answer is needed.")
                .is_empty()
        );
        assert!(instructions("The final report mustache is a test string.").is_empty());
    }

    #[test]
    fn preserves_order_case_and_quoted_sentence_boundaries() {
        assert_eq!(instructions("最終: Your FINAL REPORT MUST NAME `Phase 1. Complete` and a.rs. Preserve fixtures. Final answer must include b.rs."),
            [" `Phase 1. Complete` and a.rs.", " b.rs."]);
    }
}
