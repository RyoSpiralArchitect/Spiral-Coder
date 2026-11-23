//! Quote complete scalar assertions from a single native unittest failure digest.
use super::{correlated_results, exec_cwd, exec_may_have_run, exec_outcome, relative_path};
use crate::tui::agent::{parse_exec_command_from_args, requirement_proof::Contract};
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::Path;

#[derive(Debug, Serialize)]
pub(super) struct ReportedSubtests {
    method: String,
    cases: Vec<Case>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
struct Case {
    parameters: String,
    assertion: String,
}

#[derive(Debug, Serialize)]
pub(super) struct UnchangedAfterPatch {
    before_check_call_id: String,
    patch_call_id: String,
    path: String,
    repeated_assertions: usize,
}

impl ReportedSubtests {
    /// Compare complete labeled failures around a single landed source patch.
    /// Other processes and edits are barriers to this narrowly attributed observation.
    pub(super) fn unchanged_after_patch(
        &self,
        messages: &[Value],
        check: &str,
        failure_path: &str,
        root: &Path,
        current_id: &str,
        requirements: &Contract,
    ) -> Option<UnchangedAfterPatch> {
        let mut last = [None, None, None];
        for (call, result) in correlated_results(messages) {
            if matches!(
                call.name.as_str(),
                "exec" | "write_file" | "patch_file" | "apply_diff"
            ) && result["content"]
                .as_str()
                .is_some_and(|s| exec_may_have_run(s) && !s.starts_with("Awaiting approval"))
            {
                last.rotate_left(1);
                last[2] = Some((call, result));
            }
        }
        let [Some((before, old)), Some((patch, edit)), Some((current, _))] = last else {
            return None;
        };
        if current.name != "exec"
            || current.id != current_id
            || before.name != "exec"
            || patch.name != "patch_file"
        {
            return None;
        }
        let output = old["content"].as_str()?;
        let command = parse_exec_command_from_args(&before.arguments)?;
        if command != check
            || exec_cwd(output).map(Path::new) != Some(root)
            || exec_outcome(output) != Some(false)
            || crate::execution_evidence::file_edit_outcome(&patch.name, edit["content"].as_str()?)
                != Some(crate::execution_evidence::FileEditOutcome::Changed)
        {
            return None;
        }
        let args: Value = serde_json::from_str(&patch.arguments).ok()?;
        let path = relative_path(args["path"].as_str()?, root.to_str())?;
        if !requirements
            .requirements
            .iter()
            .any(|r| r.has_source(&path))
            || path == failure_path
        {
            return None;
        }
        let super::unittest_comparison::Diagnosis::Subtests(previous) =
            super::unittest_comparison::project(check, failure_path, output, root)?
        else {
            return None;
        };
        (self.method == previous.method
            && self.cases.len() == previous.cases.len()
            && self.cases.iter().all(|case| previous.cases.contains(case)))
        .then(|| UnchangedAfterPatch {
            before_check_call_id: before.id,
            patch_call_id: patch.id,
            path,
            repeated_assertions: self.cases.len(),
        })
    }

    pub(super) fn from_digest(output: &str, method: &str, failures: usize) -> Option<Self> {
        if failures < 2 {
            return None;
        }
        let lines = crate::tui::agent::failure_diagnostics::retained_digest_lines(output)?;
        let short_method = method.rsplit_once('.')?.1;
        let header = format!("FAIL: {short_method} ({method}) ");
        let mut pending = None;
        let mut labels = BTreeSet::new();
        let mut cases = Vec::new();
        for line in lines {
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
