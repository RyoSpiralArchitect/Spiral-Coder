//! Quote complete scalar assertions from a single native unittest failure digest.
use serde::Serialize;
use std::collections::BTreeSet;

#[derive(Debug, Serialize)]
pub(super) struct ReportedSubtests {
    method: String,
    cases: Vec<Case>,
}

#[derive(Debug, Serialize)]
struct Case {
    parameters: String,
    assertion: String,
}

impl ReportedSubtests {
    pub(super) fn from_digest(output: &str, method: &str, failures: usize) -> Option<Self> {
        if failures < 2 {
            return None;
        }
        // The native digest precedes stdout/stderr. Repeated headers in either
        // captured stream do not supply another set of observations.
        let mut lines = output
            .lines()
            .take_while(|line| !matches!(*line, "stdout:" | "stderr:"));
        let count = lines
            .find_map(|line| {
                line.strip_prefix("[ERROR DIGEST — ")?
                    .strip_suffix(" line(s)]")
            })?
            .parse::<usize>()
            .ok()?;
        let short_method = method.rsplit_once('.')?.1;
        let header = format!("FAIL: {short_method} ({method}) ");
        let mut pending = None;
        let mut labels = BTreeSet::new();
        let mut cases = Vec::new();
        for _ in 0..count {
            let line = lines.next()?;
            if let Some(label) = line.strip_prefix(&header) {
                if pending.is_some()
                    || !label.starts_with('(')
                    || !label.ends_with(')')
                    || label.len() > 256
                    || !labels.insert(label)
                {
                    return None;
                }
                pending = Some(label);
            } else if let Some(assertion) = line.strip_prefix("AssertionError: ") {
                let parameters = pending.take()?;
                if !scalar_assertion(assertion) {
                    return None;
                }
                cases.push(Case {
                    parameters: parameters.to_owned(),
                    assertion: assertion.to_owned(),
                });
            } else if !line.starts_with("File \"") {
                return None;
            }
        }
        (pending.is_none() && cases.len() == failures).then(|| Self {
            method: method.to_owned(),
            cases,
        })
    }
}

fn scalar_assertion(assertion: &str) -> bool {
    if assertion.len() > 256 || assertion.ends_with('…') {
        return false;
    }
    let comparison = assertion
        .split_once(" : ")
        .map_or(assertion, |(text, _)| text);
    let mut words = comparison.split_ascii_whitespace();
    let scalar = |s: &str| matches!(s, "None" | "True" | "False") || s.parse::<i128>().is_ok();
    words.next().is_some_and(scalar)
        && matches!(words.next(), Some("==" | "!=" | "<" | "<=" | ">" | ">="))
        && words.next().is_some_and(scalar)
        && words.next().is_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ambiguous_or_clipped_rows_cannot_replace_the_focused_probe() {
        let clean = "[ERROR DIGEST — 4 line(s)]\nFAIL: test_v (m.T.test_v) (n=1)\nAssertionError: -1 != 2\nFAIL: test_v (m.T.test_v) (n=2)\nAssertionError: False == True";
        assert!(ReportedSubtests::from_digest(clean, "m.T.test_v", 2).is_some());
        for text in [
            clean.replace("(n=2)", "(n=1)"),
            clean.replace("-1 != 2", "very long dict with operands omitted"),
            clean.replace("-1 != 2", "0 == 0 : clipped…"),
            clean.replace("AssertionError: -1 != 2\n", ""),
            clean.replace("m.T.test_v) (n=2)", "m.T.other) (n=2)"),
            format!("stderr:\n{clean}"),
        ] {
            assert!(
                ReportedSubtests::from_digest(&text, "m.T.test_v", 2).is_none(),
                "{text}"
            );
        }
        assert!(ReportedSubtests::from_digest(clean, "m.T.test_v", 3).is_none());
    }
}
