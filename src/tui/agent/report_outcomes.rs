//! Bounded before/after report requirements derived from native check receipts.
//! Rows describe recorded status, never semantic test coverage or task success.
use serde_json::Value;

pub(super) fn requested(root: &str) -> bool {
    super::final_instructions::instructions(root)
        .iter()
        .any(|clause| {
            let clause = clause.to_ascii_lowercase();
            clause.contains("before")
                && clause.contains("after")
                && ["test", "check", "verification"]
                    .iter()
                    .any(|s| clause.contains(s))
                && ["outcome", "result"].iter().any(|s| clause.contains(s))
        })
}

/// Accept a canonical receipt field as a line or a sentence in a paragraph.
/// Formatting does not change the evidence requirement; labels, status and
/// command must still match, with boundaries excluding qualified/negated fields.
pub(super) fn states_row(summary: &str, row: &str) -> bool {
    let Some((field, command)) = row.split_once(" — ") else {
        return false;
    };
    let Some((label, status)) = field.split_once(": ") else {
        return false;
    };
    let marker = format!("{label}:");
    summary.match_indices(&marker).any(|(start, _)| {
        let before = &summary[..start];
        let tail = summary[start + marker.len()..].trim_start_matches([' ', '\t']);
        let Some(tail) = tail.strip_prefix(status) else {
            return false;
        };
        let Some(tail) = tail.trim_start_matches([' ', '\t']).strip_prefix('—') else {
            return false;
        };
        // Normalize only presentation whitespace around label/status/separator.
        // The complete quoted command, including internal spaces, stays exact.
        let Some(tail) = tail.trim_start_matches([' ', '\t']).strip_prefix(command) else {
            return false;
        };
        let after = tail.trim_start_matches([' ', '\t']);
        let line_prefix = before.rsplit('\n').next().unwrap_or("").trim();
        let starts_field = ["", "-", "*"].contains(&line_prefix)
            || (before.ends_with(char::is_whitespace)
                && (before.trim_end().ends_with(['.', '!', '?', ';', ','])
                    || follows_report_label(before, row)));
        let ends_field = after.is_empty()
            || after.starts_with(['\n', '\r'])
            || (after.starts_with(['.', '!', '?', ';', ','])
                && (after.len() == 1 || after[1..].starts_with(char::is_whitespace)));
        starts_field && ends_field
    })
}

/// A colon can introduce an explicit report field, but an arbitrary prefix
/// such as "Do not claim:" or "Example:" cannot assert it. Recognize bounded
/// report labels and the retained live verification sentence, keeping exact
/// authored outcomes/commands; never populate missing rows from native proof.
fn follows_report_label(before: &str, row: &str) -> bool {
    let mut labels = vec![
        "Recorded outcomes:".to_string(),
        "Verification results:".to_string(),
        "Verified with existing tests before and after:".to_string(),
    ];
    if let Some(command) = row
        .split_once(" — `")
        .and_then(|(_, rest)| rest.strip_suffix('`'))
    {
        labels.push(format!("Verified with {command} before and after edits:"));
    }
    labels.iter().any(|label| {
        before.trim_end().strip_suffix(label).is_some_and(|prefix| {
            prefix.trim().is_empty()
                || prefix.ends_with('\n')
                || (prefix.ends_with(char::is_whitespace)
                    && prefix.trim_end().ends_with(['.', '!', '?', ';']))
        })
    })
}

pub(super) fn required_rows(root: &str, messages: &[Value], test_cmd: Option<&str>) -> Vec<String> {
    if !requested(root) {
        return vec![];
    }
    let evidence = super::evidence_packet::project(messages, test_cmd);
    let context =
        super::exec_verification::ExecVerificationContext::from_messages(test_cmd, messages);
    let mutations: Vec<_> = evidence
        .actions
        .iter()
        .filter(|a| {
            crate::execution_evidence::file_edit_succeeded(
                &a.tool,
                a.result.as_deref().unwrap_or(""),
            ) || (a.tool == "exec"
                && a.executed == Some(true)
                && a.arguments["command"]
                    .as_str()
                    .is_some_and(|c| context.classify(c).kind == super::ExecKind::Action))
        })
        .filter_map(|a| a.result_step)
        .collect();
    let first = mutations.iter().min().copied();
    let last = mutations.iter().max().copied();
    let mut commands = super::task_harness::benchmark_proof::required_commands(root);
    if commands.is_empty() {
        commands.extend(test_cmd.filter(|c| !c.trim().is_empty()).map(str::to_owned));
    }
    if commands.is_empty() {
        commands.extend(
            evidence
                .actions
                .iter()
                .filter(|a| a.tool == "exec")
                .filter_map(|a| a.arguments["command"].as_str())
                .filter(|c| context.classify(c).kind == super::ExecKind::Verify)
                .map(str::to_owned),
        );
    }
    commands.sort();
    commands.dedup();
    let mut rows = Vec::new();
    for command in commands {
        let checks: Vec<_> = evidence
            .actions
            .iter()
            .filter(|a| {
                a.tool == "exec"
                    && a.executed == Some(true)
                    && a.arguments["command"].as_str() == Some(command.as_str())
            })
            .collect();
        let before = checks
            .iter()
            .rev()
            .find(|a| a.result_step.is_some_and(|s| first.is_none_or(|f| s < f)));
        let after = checks
            .iter()
            .rev()
            .find(|a| a.result_step.is_some_and(|s| last.is_some_and(|l| s > l)));
        let status = |result: Option<&&super::evidence_packet::ActionEvidence>, absent| match result
            .and_then(|a| a.succeeded)
        {
            Some(true) => "PASSED",
            Some(false) => "FAILED",
            None => absent,
        };
        rows.push(format!(
            "Before: {} — `{command}`",
            status(before, "NOT RECORDED")
        ));
        rows.push(format!(
            "After: {} — `{command}`",
            status(after, "NOT VERIFIED")
        ));
    }
    if rows.is_empty() {
        rows.extend(["Before: NOT RECORDED".into(), "After: NOT VERIFIED".into()]);
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    const ROOT: &str = "Fix greet.py. Finish with an authored report naming the changed file and the actual before/after test outcomes; disclose any remaining work.";
    const CHECK: &str = "python3 -S -m unittest -v test_greet.py";
    fn exchange(m: &mut Vec<Value>, tool: &str, args: Value, result: &str) {
        let id = format!("call-{}", m.len());
        m.extend([json!({"role":"assistant","tool_calls":[{"id":id,"function":{"name":tool,"arguments":args.to_string()}}]}), json!({"role":"tool","tool_call_id":id,"content":result})]);
    }
    #[test]
    fn report_spacing_is_flexible_only_outside_the_exact_command() {
        let row = "After: PASSED — `check 'two  spaces'`";
        for text in [
            "- After:  PASSED — `check 'two  spaces'`",
            "After:\tPASSED\t—\t`check 'two  spaces'`",
            "Verification results: After:  PASSED — `check 'two  spaces'`.",
        ] {
            assert!(states_row(text, row), "{text}");
        }
        for text in [
            "After:  PASSED — `check 'two spaces'`",
            "After:\nPASSED — `check 'two  spaces'`",
            "After:  NOT PASSED — `check 'two  spaces'`",
            "Not After:  PASSED — `check 'two  spaces'`",
            "After:  PASSED — `check 'two  spaces'` was not recorded",
            "Example: After:  PASSED — `check 'two  spaces'`",
        ] {
            assert!(!states_row(text, row), "{text}");
        }
    }

    #[test]
    fn accepted_live_handoff_must_report_both_recorded_outcomes() {
        let mut messages = vec![json!({"role":"user","content":ROOT})];
        exchange(
            &mut messages,
            "exec",
            json!({"command":CHECK}),
            "⚠ LOGIC ERROR: assertion failed\n\nFAILED (exit_code: 1)",
        );
        exchange(
            &mut messages,
            "patch_file",
            json!({"path":"greet.py"}),
            "OK: patched 'greet.py'",
        );
        exchange(
            &mut messages,
            "exec",
            json!({"command":CHECK}),
            "OK (exit_code: 0)",
        );
        let old = "Fixed greet.py to return 'Hello, <name>!' for all names including empty string.";
        let error = super::super::final_handoff::validate_authored_done_summary(
            ROOT,
            old,
            old,
            &messages,
            Some(CHECK),
        )
        .unwrap_err();
        assert!(error.contains("Before: FAILED"));
        assert!(error.contains("After: PASSED"));
        let rows = required_rows(ROOT, &messages, Some(CHECK));
        let repaired = format!("{old}\n{}", rows.join("\n"));
        assert!(
            super::super::final_handoff::validate_authored_done_summary(
                ROOT,
                &repaired,
                &repaired,
                &messages,
                Some(CHECK)
            )
            .is_ok(),
            "{:?}",
            super::super::final_handoff::validate_authored_done_summary(
                ROOT,
                &repaired,
                &repaired,
                &messages,
                Some(CHECK)
            )
        );
        // Retained r2 OpenAI report: the receipts were correct but a line-only
        // matcher rejected the paragraph repeatedly until the shared cap ran out.
        let paragraph = format!(
            "Changed greet.py to fix punctuation from '?' to '!'. {}. {}. No remaining work.",
            rows[0], rows[1]
        );
        assert!(super::super::final_handoff::validate_authored_done_summary(
            ROOT,
            &paragraph,
            &paragraph,
            &messages,
            Some(CHECK)
        )
        .is_ok());
        assert!(states_row(&format!("- {}", rows[0]), &rows[0]));
        assert!(states_row(&format!("{}, {}.", rows[0], rows[1]), &rows[1]));
        // Retained format-r1 mini wrote complete exact fields after a colon,
        // then spent five further requests on an unchanged report refusal.
        let inline = format!("Fixed greet.py. Verified with {CHECK} before and after edits: {}, {}. No further work.", rows[0], rows[1]);
        assert!(super::super::final_handoff::validate_authored_done_summary(
            ROOT,
            &inline,
            &inline,
            &messages,
            Some(CHECK)
        )
        .is_ok());
        assert!(states_row(
            &format!("Recorded outcomes: {}", rows[0]),
            &rows[0]
        ));
        let retained = format!("Fixed greet.py. Verified with existing tests before and after: {}, {}. No remaining work.", rows[0], rows[1]);
        assert!(super::super::final_handoff::validate_authored_done_summary(
            ROOT,
            &retained,
            &retained,
            &messages,
            Some(CHECK)
        )
        .is_ok());
        for prefix in [
            "Not:",
            "Do not claim:",
            "Example:",
            "Not Recorded outcomes:",
            "Example: Recorded outcomes:",
            "Not Verified with existing tests before and after:",
            "Example: Verified with existing tests before and after:",
        ] {
            assert!(!states_row(&format!("{prefix} {}", rows[0]), &rows[0]));
        }
        assert!(!states_row(
            &format!(
                "Not Verified with {CHECK} before and after edits: {}",
                rows[0]
            ),
            &rows[0]
        ));
        assert!(!states_row(
            &inline.replace(CHECK, "different check"),
            &rows[0]
        ));
        assert!(!states_row(
            &format!("Recorded outcomes: {} was not recorded", rows[0]),
            &rows[0]
        ));
        assert!(!states_row(&format!("Not {}", rows[0]), &rows[0]));
        assert!(!states_row(
            &format!("{} was not recorded", rows[0]),
            &rows[0]
        ));
        assert!(!states_row(
            &paragraph.replace("FAILED", "PASSED"),
            &rows[0]
        ));
        // Fields outside the authored summary cannot fill missing outcome rows.
        assert!(super::super::final_handoff::validate_authored_done_summary(
            ROOT,
            old,
            &repaired,
            &messages,
            Some(CHECK)
        )
        .is_err());
        messages = serde_json::from_str(&serde_json::to_string(&messages).unwrap()).unwrap();
        assert_eq!(required_rows(ROOT, &messages, Some(CHECK)), rows);
        exchange(
            &mut messages,
            "patch_file",
            json!({"path":"greet.py"}),
            "OK: patched 'greet.py'",
        );
        let current = required_rows(ROOT, &messages, Some(CHECK));
        assert!(current[0].contains("Before: FAILED"));
        assert!(current[1].contains("After: NOT VERIFIED"));
        assert!(super::super::final_handoff::validate_authored_done_summary(
            ROOT,
            &repaired,
            &repaired,
            &messages,
            Some(CHECK)
        )
        .is_err());
        let partial = format!("greet.py remains incomplete.\n{}", current.join("\n"));
        assert!(super::super::final_handoff::validate_authored_done_summary(
            ROOT,
            &partial,
            &partial,
            &messages,
            Some(CHECK)
        )
        .is_ok());
    }
    #[test]
    fn unknown_and_rejected_results_do_not_become_reported_success() {
        let mut messages = vec![json!({"role":"user","content":ROOT})];
        exchange(
            &mut messages,
            "exec",
            json!({"command":CHECK}),
            "GOVERNOR BLOCKED\nOK (exit_code: 0)",
        );
        assert!(required_rows(ROOT, &messages, Some(CHECK))[0].contains("NOT RECORDED"));
        exchange(
            &mut messages,
            "patch_file",
            json!({"path":"greet.py"}),
            "OK: patched 'greet.py'",
        );
        exchange(
            &mut messages,
            "exec",
            json!({"command":CHECK}),
            "FAILED (exit_code: 1)\nstdout:\nOK (exit_code: 0)",
        );
        assert!(required_rows(ROOT, &messages, Some(CHECK))[1].contains("After: FAILED"));
        assert!(required_rows("Explain this code", &messages, Some(CHECK)).is_empty());
        assert!(!requested(&format!("```text\n{ROOT}\n```")));
    }
}
