//! Bounded explicit final-report clauses, shared by prompting and validation.

pub(super) fn instructions(root: &str) -> Vec<&str> {
    let mut clauses = Vec::new();
    let mut fence = None;
    for line in root.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            let marker = &trimmed[..3];
            if fence == Some(marker) {
                fence = None;
            } else if fence.is_none() {
                fence = Some(marker);
            }
            continue;
        }
        if fence.is_some() {
            continue;
        }
        let lower = line.to_ascii_lowercase();
        let mut starts = Vec::new();
        for marker in [
            "final answer must ",
            "final report must ",
            "final response must ",
            "final summary must ",
        ] {
            for (start, _) in lower.match_indices(marker) {
                if is_quoted_or_example_context(line, start) {
                    continue;
                }
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

fn is_quoted_or_example_context(line: &str, start: usize) -> bool {
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
    if quote.is_some() {
        return true;
    }
    let lower_prefix = prefix.to_ascii_lowercase();
    let sentence_start = lower_prefix
        .rfind(['.', '!', '?'])
        .map(|i| i + 1)
        .unwrap_or(0);
    let sentence = lower_prefix[sentence_start..].trim_start();
    is_example_or_negation_sentence(sentence)
}

fn is_example_or_negation_sentence(sentence: &str) -> bool {
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

    #[test]
    fn ignores_clauses_in_fences_quoted_blocks_and_examples() {
        assert!(instructions(
            "```text\nFinal report must state PASS.\n```\nYour final answer must include real.rs."
        )
        .contains(&" real.rs."));
        assert!(instructions("`Final report must state PASS.`").is_empty());
        assert_eq!(
            instructions("`Final report must state PASS.` Your final answer must include real.rs."),
            [" real.rs."]
        );
        assert!(instructions("For example, final report must state PASS.").is_empty());
        assert!(instructions("Not: final report must state PASS.").is_empty());
        assert_eq!(
            instructions(
                "Don't write: final answer must include fake.rs. Your final report must include real.rs."
            ),
            [" real.rs."]
        );
    }
}
