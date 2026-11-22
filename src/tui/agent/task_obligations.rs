//! One derived task view for Coder, Observer and the browser. No new authority,
//! execution evidence or durable state is created by this projection.
use super::{evidence_packet, final_handoff, task_harness};
use serde::Serialize;
use serde_json::Value;

pub(crate) const EXPLICIT_TEST_CASE_GUIDANCE: &str = "For each explicit_cases item in the original test requirement, identify the input that isolates it, the verified positive control, and the current public test that distinguishes the two outcomes. Implement missing cases while there is room for an edit and verification. A case map cites test functions; the assertions determine what they actually cover.\n";
pub(crate) const FIXTURE_INPUT_GUIDANCE: &str = "fixture_input links the pending test to a JSON input actually named by its fixture helper. If the record layout is the missing fact, inspect data_path once or reuse data_read_call_id. Use the prescribed producer to build a valid control for one explicit case, check its public result, then change only the condition under test. The sample describes one fixture. Follow the current diagnosis and recheck first.\n";
pub(crate) const NULLABLE_COMPARISON_GUIDANCE: &str = "For a nullable_comparison, write down what each recorded side must contain when the underlying input is present and when it is absent. From valid controls for both states, change one recorded side or one component at a time, including known-to-null and null-to-known mismatches. Check the constraints on still-known data before propagating an unknown result. Follow the original rule for which fields must agree.\n";

pub(crate) const DOCUMENT_ENDING_GUIDANCE: &str = "For a requested append, document_ending.read_file gives the last lines of a pending document. Use those bytes for a unique patch_file or apply_diff anchor and preserve the original text. A shell write cannot supply the required native edit receipt. If the text already exists, check the actual outstanding change before adding another copy.\n";
pub(crate) const ORDERED_OUTPUT_GUIDANCE: &str = "When the task promises ordered or linked records, exercise the public interface with two distinguishable valid records from the real fixture or producer. Compare the ordered identities and their associated values on parsed output. A single record cannot exercise order.\n";

/// Show a discriminating control only for behavior linked to this test file.
pub(crate) fn test_controls<'a>(ordered: bool, cases: impl Iterator<Item = &'a str>) -> String {
    let mut text = String::new();
    if ordered {
        text.push_str(ORDERED_OUTPUT_GUIDANCE);
        text.push_str("Preserve the producer's links and counts when extending the fixture. If the public result returns records, derive the expected sequence of positions and identities from the fixture, then compare that sequence in order.\n");
    }
    let mut cases = cases;
    if cases.any(|case| {
        let case = case.to_ascii_lowercase();
        let mut previous = "";
        case.split(|c: char| !c.is_alphanumeric())
            .filter(|w| !w.is_empty())
            .any(|word| {
                let found = matches!(word, "symlink" | "symlinks")
                    || (previous == "symbolic" && matches!(word, "link" | "links"));
                previous = word;
                found
            })
    }) {
        text.push_str("For a requested symbolic link case, compare the ordinary valid component with a link at the same path to its saved, existing contents. Assert the public outcome specified for that link. A dangling target also tests missing data; keep a requested dangling case separately.\n");
    }
    text
}

pub(crate) use super::recovery_focus::{
    fixture_repair_prompt, FAILURE_SITE_GUIDANCE, FIXTURE_CONTEXT_GUIDANCE,
};

pub(crate) const SUBTEST_GUIDANCE: &str = "Use reported_subtests as quoted observations from the latest failed check. Trace each row through the test setup to its controlling source condition. Name which rows the next edit addresses, which control must keep passing, and which rows still need a separate change or read. For a patch suggestion, compare its literal search and replacement before claiming coverage. Keep changes needed elsewhere in the remaining work. Rerun the original exact check to measure coverage.";
pub(crate) const UNCHANGED_PATCH_GUIDANCE: &str = "unchanged_after_patch records the same labeled assertions before and after one successful source edit. Trace one failing input from the public entry to the edited branch. Inspect the caller or earlier return that selects the branch, and how its result is consumed. Identify a bounded observation that distinguishes a skipped branch from an ineffective change before revising the same helper again. The matching results alone do not identify the cause.";

pub(crate) const DIAGNOSTIC_FOLLOWUP_GUIDANCE: &str = "The native diagnostic_followup links the current failed check with a completed read of its failure location. Use the returned observations to state the specific missing fact. Choose one different bounded observation that could distinguish likely causes, such as comparing the actual and expected result for the one failing case. A different file or line range must address an identified gap. If current evidence already supports a repair, make it in the allowed scope. Keep the original check pending for after the repair. The read and this comparison supply no completion proof.";
pub(crate) const SOURCE_REPAIR_GUIDANCE: &str = "Use the linked failed check, test read and implementation read to identify the accepted control, the single changed condition, and what the repair must preserve. When a declaration is visible but its use site is missing, search for its qualified name inside that file and read the matching lines. Use known evidence for validation and preserve the output contract for fields still unknown. Apply a shared rule where it covers every in-scope branch with the same preconditions. Ground the edit in the current source. Recheck both outcomes with the original exact command after the edit. These reads grant no proof or approval.";
const UNITTEST_COMPARISON_GUIDANCE: &str = "When diagnostic_followup.unittest_comparison is present, its arguments are a runnable exec for the single failing method. If the clipped assertion difference is the missing fact, select diagnostic_followup.comparison_check_id alone with exec when the available tool offers check_id. Otherwise use the exact full arguments from the specified cwd. Include any checks_to_refresh in the remaining verification work. The minus and plus lines are the first and second operands of the observed assertion. Use the test read to identify actual and expected. A nonzero exit can still reveal the difference. Use the differing fields to locate the source to repair. Keep the original exact check pending.";
const AUTHORED_TEST_GUIDANCE: &str = "Use task_requirements.authored_tests to map each requested case to its successful fixture control, single change, and public assertion. Ask what the test would observe if only the intended check were missing. For a uniqueness rule, change an existing value to a duplicate while preserving cardinality and other valid fields; adding a row can trip a count check. For a path-type rule, verify a temporary fixture succeeds, preserve its recorded path and replace only the component at that path. A different recorded directory name can itself cause rejection. For a valid symlink case, keep the original file or directory as the link target so all remaining data still agree. Exercise distinct cases for absent data, valid links, and dangling links when the task calls for them. For shared_requirements, match each case to the implementation branch, including contradictory flags or mismatched identities. The review_source quotes a native revision or a bounded workspace readback. A workspace readback is review context. Follow native read and approval rules for edits. Use it to inspect the assertions and identify gaps before the final sweep. Fill gaps while edit budget remains. Run the declared checks after the last edit. Preserve uncovered cases in an incomplete report.\n";

#[derive(Debug, Serialize)]
pub(crate) struct TaskObligations {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_blocked_report: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iteration_handoff: Option<super::iteration_handoff::Note>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minimum_delivery: Option<super::requirement_proof::DeliveryFloor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strategy_memory: Option<super::strategy_memory::View>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagnostic_patch: Option<super::deferred_edit::diagnosis::NextAction>,
    pub work_progress: super::work_progress::WorkProgress,
    pub artifact_scope: Option<crate::task_scope::ArtifactScope>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub explicit_edit_paths: Option<super::explicit_edit_scope::Scope>,
    pub required_exact_checks: Vec<String>,
    pub pending_exact_checks: Vec<String>,
    pub final_answer: Value,
    pub latest_action_feedback: Option<evidence_packet::ActionEvidence>,
    pub pending_recovery: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authored_test_failure: Option<super::recovery_focus::AuthoredTestFailure>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required_recheck: Option<String>,
    pub required_diagnostic_read: Option<super::recovery_focus::DiagnosticRead>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagnostic_followup: Option<super::recovery_focus::DiagnosticFollowup>,
    pub acceptance_criteria: Vec<Value>,
    pub current_commands: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub citable_checks: Vec<super::check_catalog::Check>,
    pub task_requirements: super::requirement_proof::Status,
    pub repair_brief: Option<super::repair_brief::RepairBrief>,
    pub report_repair: Option<super::report_repair::ReportRepair>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deferred_edit: Option<super::deferred_edit::Proposal>,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CoderTurn {
    Exhausted,
    ReportOnly,
    Work,
}

impl TaskObligations {
    pub(crate) fn can_offer_test_recipe(&self, path: &str, remaining: Option<usize>) -> bool {
        self.required_diagnostic_read.is_none()
            && self.required_recheck.is_none()
            && self.report_repair.is_none()
            && remaining.is_none_or(|requests| self.coder_turn(requests) == CoderTurn::Work)
            && self
                .task_requirements
                .authored_tests
                .as_ref()
                .is_some_and(|tests| tests.contains(path))
    }

    pub(crate) fn coder_turn(&self, remaining: usize) -> CoderTurn {
        if remaining == 0 {
            CoderTurn::Exhausted
        } else if super::next_action::Contract::project(
            Some(remaining),
            &self.pending_exact_checks,
            &self.task_requirements,
        )
        .report_only()
        {
            CoderTurn::ReportOnly
        } else {
            CoderTurn::Work
        }
    }

    pub(crate) fn bind_observer_tools(&self, schemas: &mut Value) {
        super::acceptance_schema::bind_observer(schemas, self);
    }
}

pub(crate) fn project(messages: &[Value], test_cmd: Option<&str>) -> TaskObligations {
    project_advisory(messages, test_cmd, None)
}

pub(crate) fn project_at(
    messages: &[Value],
    test_cmd: Option<&str>,
    workspace: Option<&str>,
) -> TaskObligations {
    let mut obligations = project_advisory(messages, test_cmd, workspace);
    if let (Some(workspace), Some(tests)) =
        (workspace, &mut obligations.task_requirements.authored_tests)
    {
        tests.include_workspace_review_source(messages, workspace);
        if let Some(failure) = &mut obligations.authored_test_failure {
            failure.include_source(tests);
            failure.include_fixture_context(workspace);
        }
    }
    obligations.diagnostic_patch =
        workspace.and_then(|root| super::deferred_edit::diagnosis::project(messages, root));
    obligations
}

/// Project native receipts with an execution root for citations. The root in an
/// advisory packet is a comparison value and never a path to open here.
pub(crate) fn project_advisory(
    messages: &[Value],
    test_cmd: Option<&str>,
    tool_root: Option<&str>,
) -> TaskObligations {
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
    let mut task_requirements = super::requirement_proof::project(messages, test_cmd);
    if let Some(tests) = &mut task_requirements.authored_tests {
        tests.include_review_source(
            messages,
            &context,
            tool_root,
            task_requirements.pending_baselines.is_empty(),
        );
    }
    let required = task_harness::benchmark_proof::required_commands(root);
    let acceptance = super::acceptance_report::CurrentEvidence::from_packet(
        messages, test_cmd, tool_root, &evidence,
    );
    let mut diagnostic_followup = focus.diagnostic_followup(messages, tool_root);
    if let Some(followup) = diagnostic_followup.as_mut() {
        followup.include_comparison_check_cost(&required, &evidence.pending_exact_checks, test_cmd);
    }
    let mut authored_test_failure = task_requirements
        .authored_tests
        .as_ref()
        .and_then(|tests| focus.authored_test_failure(tests, tool_root))
        .filter(|_| focus.authored_source_is_current(messages, tool_root, &evidence));
    if let (Some(failure), Some(workspace)) = (&mut authored_test_failure, tool_root) {
        failure.include_observation(messages, workspace, &context);
    }
    TaskObligations {
        minimum_delivery: task_requirements
            .delivery_floor(&required, &evidence.pending_exact_checks),
        iteration_handoff: super::iteration_handoff::Note::from_task(messages),
        previous_blocked_report: evidence.actions.last()
            .filter(|a| a.tool == "done" && a.executed == Some(true) && a.result.as_deref() == Some("OK: blocked"))
            .map(|a| serde_json::json!({"call_id":a.call_id, "summary":a.arguments["summary"].as_str().map(|v| v.chars().take(600).collect::<String>()),
                "next_steps":a.arguments["next_steps"].as_str().map(|v| v.chars().take(800).collect::<String>())})),
        strategy_memory: super::strategy_memory::project(messages),
        diagnostic_patch: None,
        deferred_edit: if pending_recovery.is_empty() {
            super::deferred_edit::project(messages)
        } else {
            None
        },
        work_progress: super::work_progress::project(root, &evidence),
        report_repair: super::report_repair::project(
            messages,
            test_cmd,
            &evidence,
            tool_root,
            !pending_recovery.is_empty(),
        ),
        repair_brief: super::repair_brief::project(messages, &evidence, test_cmd, tool_root),
        artifact_scope: crate::task_scope::artifact_scope(root),
        explicit_edit_paths: super::explicit_edit_scope::Scope::parse(root),
        required_recheck: focus.declared_recheck(&evidence.pending_exact_checks),
        required_exact_checks: required,
        pending_exact_checks: evidence.pending_exact_checks,
        final_answer: final_handoff::report_context(root, messages, test_cmd),
        latest_action_feedback: evidence.actions.into_iter().last().filter(|action| {
            action.result.as_deref().is_some_and(|result| {
                result.starts_with("GOVERNOR BLOCKED") || result.starts_with("ERROR:")
            })
        }),
        required_diagnostic_read: focus.required_read(),
        diagnostic_followup,
        pending_recovery,
        acceptance_criteria: super::acceptance_report::criteria(plan.as_ref()),
        current_commands: acceptance.commands,
        citable_checks: acceptance.citable_checks,
        authored_test_failure,
        task_requirements,
    }
}

#[cfg(test)]
pub(crate) fn prompt(
    messages: &[Value],
    test_cmd: Option<&str>,
    workspace: Option<&str>,
) -> Option<String> {
    prompt_for(&project_at(messages, test_cmd, workspace), messages)
}

pub(super) fn prompt_for(obligations: &TaskObligations, messages: &[Value]) -> Option<String> {
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
        && obligations.explicit_edit_paths.is_none()
        && !obligations.task_requirements.declared
        && obligations.pending_recovery.is_empty()
        && obligations.deferred_edit.is_none()
        && obligations.diagnostic_patch.is_none()
        && obligations.iteration_handoff.is_none()
        && obligations.previous_blocked_report.is_none()
        && obligations.final_answer["clauses"]
            .as_array()
            .is_none_or(Vec::is_empty)
        && !repeated_repair_observation
    {
        return obligations
            .strategy_memory
            .as_ref()
            .and_then(|view| view.standalone_prompt());
    }
    let authored_test_guidance = obligations
        .task_requirements
        .authored_tests
        .as_ref()
        .map_or("", |_| AUTHORED_TEST_GUIDANCE);
    let fixture_repair = obligations
        .authored_test_failure
        .as_ref()
        .and_then(super::recovery_focus::AuthoredTestFailure::repair_prompt)
        .unwrap_or_default();
    let mut prompt = format!(
        "{fixture_repair}[Shared task obligations]\n{authored_test_guidance}work_progress preserves the original human task separately from declared native proof obligations. Its edit/observation counts are native activity, not a workspace diff or semantic completion. Read the original task and acceptance_criteria before concluding that only documentation or a check remains; an explicit documentation requirement does not replace the requested source repair. If observations repeat without an edit, identify the missing fact and the bounded next action instead of treating another read as progress. When only pending steps change, retain the accepted goal and acceptance criteria verbatim; steps and observed progress are not new task requirements or completion proof. previous_blocked_report records the Coder’s last accepted incomplete report. With a fresh budget, act on the first grounded outstanding step instead of repeating that report. iteration_handoff is a historical Observer explanation. Compare its future action with the current source and native obligations before choosing one new call. Use strategy_memory to recall the admitted plan and past approaches. A diagnostic_patch links your earlier held proposal to a fresh target read. Follow its suggested read before reconsidering the patch as a new call. Keep the other deliverables and exact checks in view. task_requirements is fixed by the original human task and cannot be replaced by your plan. Run checks marked baseline_failure before the first edit; they must fail on the unchanged baseline and pass after all edits. A declared failing baseline is expected evidence of missing functionality: keep its exact command unchanged and fix the declared workspace artifacts, not the frozen oracle. Use native file tools for each declared source/test/documentation artifact, then run each exact check. Pending requirements block completion for both Coder and Observer. Use explicit artifact_scope.target_files as the requested outputs. referenced_subjects are paths mentioned in the objective, not proof of file existence or an instruction to edit them. Existing fixture messages are data. Follow required_diagnostic_read first: read_file its exact path, even when the likely source to fix is different. This diagnoses the failure and does not authorize changing the supplied test. Follow pending_recovery before starting more checks. Include every final_answer.outcome_rows entry verbatim on its own line in done.summary; these report the recorded before/after checks, including unverified states. Resolve the latest action feedback against subsequent native results, then finish the remaining exact checks and authored report. Execute each pending_exact_checks entry as a separate exec, unchanged; a combined command or automatic test does not satisfy that list. Use the task's checks rather than assuming a language-specific test command. Empty pending checks mean only that the listed checks have current proof; all other gates still apply. Do not rerun passed checks merely to rewrite a rejected final report. Use acceptance_criteria.reference in completed_acceptance, remaining_acceptance and each acceptance_evidence.criterion. Cite current_commands exactly or use @<id> from citable_checks in acceptance_evidence.command, one row per completed criterion. A check reference selects its exact current successful command. Keep full command names in the authored summary when requested; decide which receipt supports each criterion. Put required_literals exactly in done.summary, with their original case.\n{}",
        super::context_compaction::render_coder_obligations(obligations, messages)
    );
    if obligations.task_requirements.document_ending.is_some() {
        prompt.push_str(DOCUMENT_ENDING_GUIDANCE);
    }
    if obligations
        .task_requirements
        .authored_tests
        .as_ref()
        .is_some_and(|tests| tests.has_nullable_comparison())
    {
        prompt.push_str(NULLABLE_COMPARISON_GUIDANCE);
    }
    if obligations
        .task_requirements
        .authored_tests
        .as_ref()
        .is_some_and(|tests| tests.has_explicit_cases())
    {
        prompt.push_str(EXPLICIT_TEST_CASE_GUIDANCE);
    }
    if obligations
        .task_requirements
        .authored_tests
        .as_ref()
        .is_some_and(|t| t.fixture_input.is_some())
    {
        prompt.push_str(FIXTURE_INPUT_GUIDANCE);
    }
    if obligations
        .task_requirements
        .authored_tests
        .as_ref()
        .is_some_and(|tests| tests.ordered_record_requirement.is_some())
    {
        prompt.push_str(ORDERED_OUTPUT_GUIDANCE);
    }
    if let Some(failure) = &obligations.authored_test_failure {
        prompt.push_str(FAILURE_SITE_GUIDANCE);
        if fixture_repair.is_empty() && failure.has_fixture_context() {
            prompt.push_str(FIXTURE_CONTEXT_GUIDANCE);
        }
    }
    if let Some(followup) = &obligations.diagnostic_followup {
        prompt.push_str("\n");
        prompt.push_str(DIAGNOSTIC_FOLLOWUP_GUIDANCE);
        if followup.has_reported_subtests() {
            prompt.push('\n');
            prompt.push_str(SUBTEST_GUIDANCE);
        }
        if followup.has_unchanged_after_patch() {
            prompt.push('\n');
            prompt.push_str(UNCHANGED_PATCH_GUIDANCE);
        }
        if followup.has_source_repair() {
            prompt.push('\n');
            prompt.push_str(SOURCE_REPAIR_GUIDANCE);
        }
        if followup.has_unittest_comparison() {
            prompt.push_str("\n");
            prompt.push_str(UNITTEST_COMPARISON_GUIDANCE);
        }
    }
    Some(prompt)
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
        let prompt = prompt(&resumed, None, None).expect("ordinary stalled repair needs context");
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
        assert!(super::prompt(&messages, None, None).is_none());
    }

    #[test]
    fn progress_admission_does_not_flood_idle_empty_or_read_only_tasks() {
        assert!(prompt(&[], None, None).is_none());
        let mut idle = vec![json!({"role":"user","content":"Repair src/a.rs"})];
        assert!(prompt(&idle, None, None).is_none());
        read_source(&mut idle, "first");
        assert!(prompt(&idle, None, None).is_none());
        for root in ["", "Read-only: inspect src/a.rs"] {
            let mut messages = vec![json!({"role":"user","content":root})];
            read_source(&mut messages, "first");
            read_source(&mut messages, "second");
            assert!(prompt(&messages, None, None).is_none(), "{root}");
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
