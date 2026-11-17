//! Return a Coder work slice when a completed Observer repair has no valid envelope.
use super::*;

pub(in crate::tui::events) fn rejected_repair_work_return(app: &App) -> Option<usize> {
    let request = app.observer_assist_request.as_ref()?;
    if !request.repair_started
        || request.terminal_diagnosis
        || !request_is_current(app)
        || app.work_budget.preservation_issue().is_some()
    {
        return None;
    }
    let work = app.work_budget.snapshot();
    let [wire] = work.requests.get(request.request_cursor..)? else {
        return None;
    };
    if wire.role != "observer" || wire.status != "completed" {
        return None;
    }
    let reply = app
        .observer
        .messages
        .iter()
        .rev()
        .find(|message| message.role == Role::Assistant && message.complete)?;
    suggestion::transport_issue_for_raw(reply.raw_content.as_deref().unwrap_or(&reply.content))?;

    let messages = &app.coder_session.as_ref()?.messages;
    let test_command = app.project_test_cmd.as_deref();
    let obligations = agent::task_obligations::project(messages, test_command);
    let actionable = !obligations
        .task_requirements
        .pending_artifact_paths
        .is_empty()
        || !obligations.pending_exact_checks.is_empty()
        || obligations.required_diagnostic_read.is_some();
    if !obligations.task_requirements.declared
        || !actionable
        || obligations.coder_turn(work.coder_remaining())
            != agent::task_obligations::CoderTurn::Work
        || agent::evidence_packet::project(messages, test_command).has_approval_barrier()
    {
        return None;
    }
    Some(wire.id)
}
