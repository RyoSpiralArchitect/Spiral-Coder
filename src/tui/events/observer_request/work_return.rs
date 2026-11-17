//! Return a Coder work slice when an Observer answer yields no usable advice:
//! a completed repair that is malformed or fails the action contract, or a
//! valid answer whose primary action is below the forwarding threshold.
use super::*;

/// The Coder can still advance native work on its next turn: a declared task
/// with an outstanding artifact, exact check or diagnostic read, a work (not
/// report-only) turn, and no approval barrier.
pub(in crate::tui::events) fn native_work_remains(
    messages: &[serde_json::Value],
    test_command: Option<&str>,
    coder_remaining: usize,
) -> bool {
    let obligations = agent::task_obligations::project(messages, test_command);
    let actionable = !obligations
        .task_requirements
        .pending_artifact_paths
        .is_empty()
        || !obligations.pending_exact_checks.is_empty()
        || obligations.required_diagnostic_read.is_some();
    obligations.task_requirements.declared
        && actionable
        && obligations.coder_turn(coder_remaining) == agent::task_obligations::CoderTurn::Work
        && !agent::evidence_packet::project(messages, test_command).has_approval_barrier()
}

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
    let raw = reply.raw_content.as_deref().unwrap_or(&reply.content);

    let messages = &app.coder_session.as_ref()?.messages;
    let test_command = app.project_test_cmd.as_deref();
    // The same native resolution that refused to forward the reply: framing or
    // schema errors, and parsed actions that fail the action contract.
    let unusable = suggestion::transport_issue_for_raw(raw).is_some()
        || crate::observer_action::resolve_advice(
            raw,
            messages,
            test_command,
            super::super::observer_assist::execution_root(app).as_deref(),
            Some(work.coder_remaining()),
        )
        .issue
        .is_some();
    if !unusable {
        return None;
    }
    native_work_remains(messages, test_command, work.coder_remaining()).then_some(wire.id)
}

/// A valid current answer whose primary action is below the 0.75 forwarding
/// threshold. The advice stays unforwarded. While native work remains on a work
/// turn, the Coder keeps its own allowance instead of spending one report-only
/// request that closes the iteration; that request is kept for the report turn
/// or for a task with nothing left to do.
pub(in crate::tui::events) fn low_confidence_work_return(app: &App) -> Option<usize> {
    let request = app.observer_assist_request.as_ref()?;
    if request.terminal_diagnosis
        || request.pending_report_closeout.is_some()
        || app.pending_observer_hint.is_some()
        || !request_is_current(app)
        || app.work_budget.preservation_issue().is_some()
    {
        return None;
    }
    let work = app.work_budget.snapshot();
    let wire = work.requests.get(request.request_cursor..)?.last()?;
    if wire.role != "observer" || wire.status != "completed" {
        return None;
    }
    let reply = app
        .observer
        .messages
        .iter()
        .rev()
        .find(|message| message.role == Role::Assistant && message.complete)?;
    let raw = reply.raw_content.as_deref().unwrap_or(&reply.content);
    if suggestion::transport_issue_for_raw(raw).is_some() {
        return None;
    }
    let messages = &app.coder_session.as_ref()?.messages;
    let test_command = app.project_test_cmd.as_deref();
    let resolved = crate::observer_action::resolve_advice(
        raw,
        messages,
        test_command,
        super::super::observer_assist::execution_root(app).as_deref(),
        Some(work.coder_remaining()),
    );
    if resolved.issue.is_some() {
        return None;
    }
    let primary = suggestion::primary_suggestion(resolved.envelope.as_ref()?)?;
    (primary.confidence < 0.75
        && native_work_remains(messages, test_command, work.coder_remaining()))
    .then_some(wire.id)
}
