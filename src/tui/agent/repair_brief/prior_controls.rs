//! Historical controls from the last failed verification sweep. Both roles can
//! use the earlier results to plan a repair. Only new native results supply proof.
use super::super::{
    acceptance_report::CurrentEvidence, check_catalog::Check, evidence_packet::ExecutionPacket,
};
use crate::execution_evidence::{auto_test_outcome, AutoTestOutcome};
use serde::Serialize;
use serde_json::Value;
use std::path::Path;

#[derive(Debug, Serialize)]
pub(super) struct PriorControls {
    failed_check: String,
    failed_call_id: String,
    subsequent_edit_call_id: String,
    passed_before_failure: Vec<String>,
    guidance: &'static str,
}

fn label(check: &Check) -> String {
    if check.command.len() <= 160 {
        check.command.clone()
    } else {
        format!("@{}", check.id)
    }
}

pub(super) fn project(
    messages: &[Value],
    evidence: &ExecutionPacket,
    test_cmd: Option<&str>,
    tool_root: Option<&str>,
    sources: &[String],
) -> Option<PriorControls> {
    let workspace = Path::new(tool_root?);
    let mutation = evidence.mutation_step?;
    let edit = evidence
        .actions
        .iter()
        .find(|a| a.result_step == Some(mutation))?;
    let path = edit.arguments["path"].as_str()?.trim_start_matches("./");
    if edit.executed != Some(true)
        || !sources.iter().any(|source| source == path)
        || auto_test_outcome(&edit.tool, edit.result.as_deref()?) != AutoTestOutcome::Failed
    {
        return None;
    }

    // A later declared rerun, including one at a different cwd, supersedes
    // the earlier sweep. Never choose a favorable older failure or pass.
    let (failed, check) = evidence
        .actions
        .iter()
        .filter_map(|action| {
            if action.tool != "exec" || action.executed != Some(true) {
                return None;
            }
            let check = evidence
                .available_checks
                .iter()
                .find(|c| action.arguments["command"] == c.command)?;
            Some((action, check))
        })
        .max_by_key(|(action, _)| action.result_step)?;
    let step = failed.result_step?;
    if step >= mutation
        || failed.succeeded != Some(false)
        || failed.exec_cwd.as_deref().map(Path::new) != Some(workspace)
        || !evidence.pending_exact_checks.contains(&check.command)
    {
        return None;
    }

    // Reuse the same workspace- and mutation-aware proof calculation that
    // governed the session just before the failure was recorded.
    let scope = crate::task_origin::current_task_messages(messages);
    let (index, row) = scope
        .iter()
        .enumerate()
        .filter(|(_, m)| m["role"] == "tool")
        .nth(step.checked_sub(1)?)?;
    if row["tool_call_id"] != failed.call_id {
        return None;
    }
    let before = CurrentEvidence::from_native(&scope[..index], test_cmd, tool_root);
    let mut passed = Vec::new();
    for previous in &before.citable_checks {
        if previous.command != check.command
            && evidence.pending_exact_checks.contains(&previous.command)
            && evidence
                .available_checks
                .iter()
                .find(|c| c.command == previous.command)
                == Some(previous)
        {
            passed.push(label(previous));
        }
    }
    passed.truncate(8);
    (!passed.is_empty()).then(|| PriorControls {
        failed_check: label(check),
        failed_call_id: failed.call_id.clone(),
        subsequent_edit_call_id: edit.call_id.clone(),
        passed_before_failure: passed,
        guidance: "The listed checks passed at the workspace before the failed check. A later source edit failed its automatic test. Compare that edit with the earlier working behavior when narrowing the repair. Rerun the required checks after the last edit for current proof.",
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::agent::task_obligations;
    use serde_json::json;

    fn exchange(messages: &mut Vec<Value>, id: &str, tool: &str, args: Value, out: &str) {
        messages.extend([
            json!({"role":"assistant","tool_calls":[{"id":id,"function":{"name":tool,"arguments":args.to_string()}}]}),
            json!({"role":"tool","tool_call_id":id,"content":out}),
        ]);
    }

    #[test]
    fn failed_source_edit_carries_only_prior_workspace_controls_without_renewing_them() {
        let commands = ["verify visible", "verify paths", "verify unrelated"];
        let root = format!(
            "<task_requirements>\n{}\n</task_requirements>",
            json!({
                "version":1,"requirements":[{"id":"fix","description":"Repair the reader",
                "artifacts":[{"path":"reader.rs","kind":"source"}],
                "checks":commands.iter().map(|command| json!({"command":command})).collect::<Vec<_>>()}]
            })
        );
        let mut messages = vec![json!({"role":"user","content":root})];
        let view = |messages: &[Value]| {
            json!(task_obligations::project_advisory(
                messages,
                None,
                Some("/work")
            ))
        };
        exchange(
            &mut messages,
            "pass",
            "exec",
            json!({"command":commands[0]}),
            "OK (exit_code: 0)\ncwd: /work",
        );
        exchange(
            &mut messages,
            "other",
            "exec",
            json!({"command":commands[2]}),
            "OK (exit_code: 0)\ncwd: /other",
        );
        exchange(
            &mut messages,
            "fail",
            "exec",
            json!({"command":commands[1]}),
            "FAILED (exit_code: 1)\ncwd: /work",
        );
        let after_check = messages.clone();
        let edit = "OK: patched 'reader.rs'\n[auto-test] ✗ FAILED (exit 101)";
        exchange(
            &mut messages,
            "edit",
            "patch_file",
            json!({"path":"reader.rs"}),
            edit,
        );
        let result = view(&messages);
        let controls = &result["repair_brief"]["prior_controls"];
        assert_eq!(controls["failed_call_id"], "fail");
        assert_eq!(controls["subsequent_edit_call_id"], "edit");
        assert_eq!(controls["passed_before_failure"], json!([commands[0]]));
        assert!(result["citable_checks"].is_null());
        assert_eq!(result["pending_exact_checks"].as_array().unwrap().len(), 3);

        exchange(
            &mut messages,
            "blocked",
            "exec",
            json!({"command":commands[1]}),
            "GOVERNOR BLOCKED",
        );
        assert_eq!(view(&messages)["repair_brief"]["prior_controls"], *controls);
        exchange(
            &mut messages,
            "rerun",
            "exec",
            json!({"command":commands[1]}),
            "OK (exit_code: 0)\ncwd: /work",
        );
        assert!(view(&messages)["repair_brief"]["prior_controls"].is_null());

        for (path, output) in [
            ("docs.md", edit),
            (
                "reader.rs",
                "OK: patched 'reader.rs'\n[auto-test] ✓ PASSED (exit 0)",
            ),
        ] {
            let mut scenario = after_check.clone();
            exchange(
                &mut scenario,
                "different",
                "patch_file",
                json!({"path":path}),
                output,
            );
            assert!(view(&scenario)["repair_brief"]["prior_controls"].is_null());
        }
        let mut different_cwd = after_check;
        exchange(
            &mut different_cwd,
            "later",
            "exec",
            json!({"command":commands[1]}),
            "FAILED (exit_code: 1)\ncwd: /other",
        );
        exchange(
            &mut different_cwd,
            "edit",
            "patch_file",
            json!({"path":"reader.rs"}),
            edit,
        );
        assert!(view(&different_cwd)["repair_brief"]["prior_controls"].is_null());
    }
}
