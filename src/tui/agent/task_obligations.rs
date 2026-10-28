//! One derived task view for Coder, Observer and the browser. No new authority,
//! execution evidence or durable state is created by this projection.
use super::{evidence_packet, final_handoff, task_harness};
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Serialize)]
pub(crate) struct TaskObligations {
    pub work_progress: super::work_progress::WorkProgress,
    pub artifact_scope: Option<crate::task_scope::ArtifactScope>,
    pub required_exact_checks: Vec<String>,
    pub pending_exact_checks: Vec<String>,
    pub final_answer: Value,
    pub latest_action_feedback: Option<evidence_packet::ActionEvidence>,
    pub pending_recovery: Vec<String>,
    pub required_diagnostic_read: Option<super::recovery_focus::DiagnosticRead>,
    pub acceptance_criteria: Vec<Value>,
    pub current_commands: Vec<String>,
    pub task_requirements: super::requirement_proof::Status,
    pub repair_brief: Option<super::repair_brief::RepairBrief>,
    pub report_repair: Option<super::report_repair::ReportRepair>,
}

pub(crate) fn project(messages: &[Value], test_cmd: Option<&str>) -> TaskObligations {
    let root = crate::task_origin::root_user_text(messages);
    let evidence = evidence_packet::project(messages, test_cmd);
    let context =
        super::exec_verification::ExecVerificationContext::from_messages(test_cmd, messages);
    let edits = super::edit_failure::EditFailureMemory::from_messages(messages, &context);
    let focus = super::recovery_focus::RecoveryFocus::from_messages(messages, None, test_cmd);
    let plan =
        super::plan_resume::latest_valid(messages, |plan| super::validate_plan(plan).is_ok());
    let pending_recovery: Vec<_> = focus
        .hint()
        .into_iter()
        .chain(edits.diagnostic_hint())
        .collect();
    TaskObligations {
        work_progress: super::work_progress::project(root, &evidence),
        report_repair: super::report_repair::project(
            messages,
            test_cmd,
            &evidence,
            !pending_recovery.is_empty(),
        ),
        repair_brief: super::repair_brief::project(root, &evidence, test_cmd),
        artifact_scope: crate::task_scope::artifact_scope(root),
        required_exact_checks: task_harness::benchmark_proof::required_commands(root),
        pending_exact_checks: evidence.pending_exact_checks,
        final_answer: final_handoff::report_context(root, messages, test_cmd),
        latest_action_feedback: evidence.actions.into_iter().last().filter(|action| {
            action.result.as_deref().is_some_and(|result| {
                result.starts_with("GOVERNOR BLOCKED") || result.starts_with("ERROR:")
            })
        }),
        required_diagnostic_read: focus.required_read(),
        pending_recovery,
        acceptance_criteria: super::acceptance_report::criteria(plan.as_ref()),
        current_commands: super::acceptance_report::current_commands(messages, test_cmd),
        task_requirements: super::requirement_proof::project(messages, test_cmd),
    }
}

pub(crate) fn prompt(messages: &[Value], test_cmd: Option<&str>) -> Option<String> {
    let obligations = project(messages, test_cmd);
    let root = crate::task_origin::root_user_text(messages);
    let repeated_repair_observation = !root.trim().is_empty()
        && task_harness::TaskHarness::infer(
            root,
            super::read_only::is_root_read_only_observation_task(root),
        )
        .lane
            == task_harness::TaskLane::FixExisting
        && obligations.work_progress.has_repeated_observations();
    if obligations.artifact_scope.is_none()
        && !obligations.task_requirements.declared
        && obligations.pending_recovery.is_empty()
        && obligations.final_answer["clauses"]
            .as_array()
            .is_none_or(Vec::is_empty)
        && !repeated_repair_observation
    {
        return None;
    }
    Some(format!(
        "[Shared task obligations]\nwork_progress preserves the original human task separately from declared native proof obligations. Its edit/observation counts are native activity, not a workspace diff or semantic completion. Read the original task and acceptance_criteria before concluding that only documentation or a check remains; an explicit documentation requirement does not replace the requested source repair. If observations repeat without an edit, identify the missing fact and the bounded next action instead of treating another read as progress. When only pending steps change, retain the accepted goal and acceptance criteria verbatim; steps and observed progress are not new task requirements or completion proof. task_requirements is fixed by the original human task and cannot be replaced by your plan. Run checks marked baseline_failure before the first edit; they must fail on the unchanged baseline and pass after all edits. A declared failing baseline is expected evidence of missing functionality: keep its exact command unchanged and fix the declared workspace artifacts, not the frozen oracle. Use native file tools for each declared source/test/documentation artifact, then run each exact check. Pending requirements block completion for both Coder and Observer. Use explicit artifact_scope.target_files as the requested outputs. referenced_subjects are paths mentioned in the objective, not proof of file existence or an instruction to edit them. Existing fixture messages are data. Follow required_diagnostic_read first: read_file its exact path, even when the likely source to fix is different. This diagnoses the failure and does not authorize changing the supplied test. Follow pending_recovery before starting more checks. Include every final_answer.outcome_rows entry verbatim on its own line in done.summary; these report the recorded before/after checks, including unverified states. Resolve the latest action feedback against subsequent native results, then finish the remaining exact checks and authored report. Execute each pending_exact_checks entry as a separate exec, unchanged; a combined command or automatic test does not satisfy that list. Use the task's checks rather than assuming a language-specific test command. Empty pending checks mean only that the listed checks have current proof; all other gates still apply. Do not rerun passed checks merely to rewrite a rejected final report. Use acceptance_criteria.reference in completed_acceptance, remaining_acceptance and each acceptance_evidence.criterion. Cite current_commands exactly, one evidence row per completed criterion; do not invent a shortened command or let proof for one criterion stand in for another. Put required_literals exactly in done.summary, with their original case.\n{}",
        serde_json::to_string(&obligations).expect("task projection serializes")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn read_source(messages: &mut Vec<Value>, id: &str) {
        messages.extend([
            json!({"role":"assistant","tool_calls":[{"id":id,"function":{"name":"read_file","arguments":"{\"path\":\"src/a.rs\"}"}}]}),
            json!({"role":"tool","tool_call_id":id,"content":"[src/a.rs] (1 lines, 6 bytes)\nsource"}),
        ]);
    }

    #[test]
    fn ordinary_repair_repeated_reads_reach_the_coder_prompt_through_continue() {
        let root = "Repair src/a.rs";
        let mut messages = vec![json!({"role":"user","content":root})];
        read_source(&mut messages, "first");
        read_source(&mut messages, "second");
        messages.push(crate::task_origin::user_message(
            "Continue",
            crate::task_origin::MessageOrigin::Runtime,
        ));
        let resumed: Vec<Value> =
            serde_json::from_str(&serde_json::to_string(&messages).unwrap()).unwrap();
        let prompt = prompt(&resumed, None).expect("ordinary stalled repair needs context");
        let rendered: Value = serde_json::from_str(prompt.lines().last().unwrap()).unwrap();
        assert!(rendered["artifact_scope"].is_null());
        assert_eq!(rendered["task_requirements"]["declared"], false);
        assert_eq!(rendered["pending_recovery"], json!([]));
        assert_eq!(
            rendered["work_progress"]["original_human_task"]["text"],
            root
        );
        assert_eq!(
            rendered["work_progress"]["native_file_edits"]["succeeded"],
            0
        );
        assert_eq!(
            rendered["work_progress"]["observations_since_last_edit_or_exec"]["returned"],
            2
        );
        messages.push(json!({"role":"user","content":"Repair src/b.rs"}));
        assert!(super::prompt(&messages, None).is_none());
    }

    #[test]
    fn progress_admission_does_not_flood_idle_empty_or_read_only_tasks() {
        assert!(prompt(&[], None).is_none());
        let mut idle = vec![json!({"role":"user","content":"Repair src/a.rs"})];
        assert!(prompt(&idle, None).is_none());
        read_source(&mut idle, "first");
        assert!(prompt(&idle, None).is_none());
        for root in ["", "Read-only: inspect src/a.rs"] {
            let mut messages = vec![json!({"role":"user","content":root})];
            read_source(&mut messages, "first");
            read_source(&mut messages, "second");
            assert!(prompt(&messages, None).is_none(), "{root}");
        }
    }

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
