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
    summary.match_indices(row).any(|(start, _)| {
        let before = &summary[..start];
        let after = summary[start + row.len()..].trim_start_matches([' ', '\t']);
        let line_prefix = before.rsplit('\n').next().unwrap_or("").trim();
        let starts_field = ["", "-", "*"].contains(&line_prefix)
            || (before.ends_with(char::is_whitespace)
                && before.trim_end().ends_with(['.', '!', '?', ';', ',']));
        let ends_field = after.is_empty()
            || after.starts_with(['\n', '\r'])
            || (after.starts_with(['.', '!', '?', ';', ','])
                && (after.len() == 1 || after[1..].starts_with(char::is_whitespace)));
        starts_field && ends_field
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
