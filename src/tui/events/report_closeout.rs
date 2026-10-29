//! Admission to one final authored report after an output-limited Observer
//! done repair. No advice, acceptance mapping or completion is synthesized.
use super::*;

pub(super) fn ready(app: &App) -> bool {
    if app.report_closeout_attempted
        || app.coder_completed == Some(true)
        || app.coder_yield_reason == Some(super::super::coder_yield::CoderYieldReason::Blocked)
        || app.work_budget.remaining_for("coder") == 0
        || app.work_budget.preservation_issue().is_some()
    {
        return false;
    }
    let Some(session) = &app.coder_session else {
        return false;
    };
    let obligations =
        agent::task_obligations::project(&session.messages, app.project_test_cmd.as_deref());
    obligations.task_requirements.declared
        && obligations.task_requirements.pending.is_empty()
        && obligations.task_requirements.pending_baselines.is_empty()
        && obligations
            .task_requirements
            .pending_artifact_paths
            .is_empty()
        && obligations.pending_exact_checks.is_empty()
        && obligations.pending_recovery.is_empty()
        && obligations.required_diagnostic_read.is_none()
        && !obligations.acceptance_criteria.is_empty()
        && !obligations.current_commands.is_empty()
}
