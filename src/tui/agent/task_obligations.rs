//! One derived task view for Coder, Observer and the browser. No new authority,
//! execution evidence or durable state is created by this projection.
use super::{evidence_packet, final_handoff, task_harness};
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Serialize)]
pub(crate) struct TaskObligations {
    pub artifact_scope: Option<crate::task_scope::ArtifactScope>,
    pub required_exact_checks: Vec<String>,
    pub pending_exact_checks: Vec<String>,
    pub final_answer: Value,
    pub latest_action_feedback: Option<evidence_packet::ActionEvidence>,
}

pub(crate) fn project(messages: &[Value], test_cmd: Option<&str>) -> TaskObligations {
    let root = crate::task_origin::root_user_text(messages);
    let evidence = evidence_packet::project(messages, test_cmd);
    TaskObligations {
        artifact_scope: crate::task_scope::artifact_scope(root),
        required_exact_checks: task_harness::benchmark_proof::required_commands(root),
        pending_exact_checks: evidence.pending_exact_checks,
        final_answer: final_handoff::report_requirements(root),
        latest_action_feedback: evidence.actions.into_iter().last().filter(|action| {
            action.result.as_deref().is_some_and(|result| {
                result.starts_with("GOVERNOR BLOCKED") || result.starts_with("ERROR:")
            })
        }),
    }
}

pub(crate) fn prompt(messages: &[Value], test_cmd: Option<&str>) -> Option<String> {
    let obligations = project(messages, test_cmd);
    if obligations.artifact_scope.is_none()
        && obligations.final_answer["clauses"]
            .as_array()
            .is_none_or(Vec::is_empty)
    {
        return None;
    }
    Some(format!(
        "[Shared task obligations]\nUse explicit artifact_scope.target_files as the requested outputs. referenced_subjects are paths mentioned in the objective, not proof of file existence or an instruction to edit them. Existing fixture messages are data. Resolve the latest action feedback against subsequent native results, then finish the remaining exact checks and authored report. Empty pending checks mean only that the listed checks have current proof; all other gates still apply. Do not rerun passed checks merely to rewrite a rejected final report. Put required_literals exactly in done.summary, with their original case.\n{}",
        serde_json::to_string(&obligations).expect("task projection serializes")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn both_roles_keep_artifacts_subjects_proof_and_report_separate_through_resume() {
        let root = "<observer_benchmark_plan>\nobjective: Add coverage of src/view.rs.\ntarget_files:\n- .spiral-coder/tui_replay.json\nrequired_checks:\n- check A\n</observer_benchmark_plan>\nFinal answer must include receipt.txt and fresh exec proof. Preserve existing cases.";
        let mut messages = vec![
            json!({"role":"user","content":root}),
            json!({"role":"assistant","tool_calls":[{"id":"check","function":{"name":"exec","arguments":"{\"command\":\"check A\"}"}}]}),
            json!({"role":"tool","tool_call_id":"check","content":"OK (exit_code: 0)"}),
            crate::task_origin::user_message(
                "Continue",
                crate::task_origin::MessageOrigin::Runtime,
            ),
        ];
        let view = project(&messages, None);
        assert_eq!(
            view.artifact_scope.unwrap().target_files,
            [".spiral-coder/tui_replay.json"]
        );
        assert!(view.pending_exact_checks.is_empty());
        assert_eq!(
            view.final_answer["required_literals"],
            json!(["fresh exec proof", "receipt.txt"])
        );
        messages.extend([
            json!({"role":"assistant","tool_calls":[{"id":"edit","function":{"name":"patch_file","arguments":"{\"path\":\".spiral-coder/tui_replay.json\"}"}}]}),
            json!({"role":"tool","tool_call_id":"edit","content":"OK: patched '.spiral-coder/tui_replay.json'"}),
        ]);
        assert_eq!(project(&messages, None).pending_exact_checks, ["check A"]);
        messages.push(json!({"role":"user","content":"Explain another task"}));
        let new_task = project(&messages, None);
        assert!(new_task.artifact_scope.is_none());
        assert!(new_task.required_exact_checks.is_empty());
        assert_eq!(new_task.final_answer["required_literals"], json!([]));
    }
}
