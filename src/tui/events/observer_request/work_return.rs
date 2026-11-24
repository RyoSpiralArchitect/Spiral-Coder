//! Return a Coder work slice when an Observer answer yields no usable advice:
//! a completed repair that is malformed or fails the action contract, or a
//! valid answer whose primary action is below the forwarding threshold.
use super::*;

/// The Coder can still advance native work on its next turn: a declared task
/// with an outstanding artifact, exact check, diagnostic read or pending
/// recovery, a work (not report-only) turn, and no approval barrier.
pub(in crate::tui::events) fn native_work_remains(
    messages: &[serde_json::Value],
    test_command: Option<&str>,
    coder_remaining: usize,
) -> bool {
    let obligations = agent::task_obligations::project(messages, test_command);
    // A failed edit or check awaiting repair is unfinished work even when every
    // artifact and check already has proof. In trial T a non-unique edit
    // anchor and its target read were followed by an uncertain Observer answer;
    // the iteration closed with 27 of its 32 task requests unused, 20 of them
    // the Coder's, and without a new edit the run stopped.
    let actionable = !obligations
        .task_requirements
        .pending_artifact_paths
        .is_empty()
        || !obligations.pending_exact_checks.is_empty()
        || obligations.required_diagnostic_read.is_some()
        || !obligations.pending_recovery.is_empty();
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

/// The test plan is already in the Coder's native history, and the scheduler
/// has marked this specific review as dispatched. It can return that work
/// once even when an earlier diagnostic used the ordinary repair fallback.
fn take_rejected_test_review(app: &mut App) -> Option<usize> {
    let request = app.observer_assist_request.as_ref()?;
    if request.output != crate::observer_action::diagnostic::Output::TestReview
        || app.coder_yield_reason
            != Some(crate::tui::coder_yield::CoderYieldReason::AuthoredTestReview)
        || app.coder.streaming
        || app.observer.streaming
        || app.coder_completed == Some(true)
        || app.report_closeout_attempted
        || request.pending_report_closeout.is_some()
        || request.return_for_delivery.is_some()
        || app.pending_observer_hint.is_some()
    {
        return None;
    }
    let id = rejected_repair_work_return(app)?;
    app.observer_assist_request = None;
    Some(id)
}

pub(in crate::tui::events) async fn resume_rejected_test_review(
    app: &mut App,
    tx: &mpsc::Sender<RequestEvent>,
) -> Option<usize> {
    let id = take_rejected_test_review(app)?;
    send_coder_input(app, tx, "The optional Observer test review ended without usable advice. Continue with the test coverage plan already in your tool results. Choose an outstanding case from the original requirements, check the actual assertions and verify your edits. Keep all required checks, approvals and reporting obligations.".into(), CoderInput::RuntimeContinuation).await;
    Some(id)
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
    low_confidence_leaves_work(app, work.coder_remaining()).then_some(wire.id)
}

/// The latest complete Observer reply is a valid answer whose primary action
/// is below the forwarding threshold, while native work remains on a Coder
/// work turn. TUI replay evaluates this decision on recorded answers, which
/// have no wire receipt.
pub(in crate::tui::events) fn low_confidence_leaves_work(
    app: &App,
    coder_remaining: usize,
) -> bool {
    let Some(reply) = app
        .observer
        .messages
        .iter()
        .rev()
        .find(|message| message.role == Role::Assistant && message.complete)
    else {
        return false;
    };
    let raw = reply.raw_content.as_deref().unwrap_or(&reply.content);
    let Some(messages) = app.coder_session.as_ref().map(|s| &s.messages) else {
        return false;
    };
    if suggestion::transport_issue_for_raw(raw).is_some() {
        return false;
    }
    let test_command = app.project_test_cmd.as_deref();
    let resolved = crate::observer_action::resolve_advice(
        raw,
        messages,
        test_command,
        super::super::observer_assist::execution_root(app).as_deref(),
        Some(coder_remaining),
    );
    resolved.issue.is_none()
        && resolved
            .envelope
            .as_ref()
            .and_then(suggestion::primary_suggestion)
            .is_some_and(|primary| {
                primary.confidence < 0.75
                    && native_work_remains(messages, test_command, coder_remaining)
            })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failed_edit_awaiting_repair_is_native_work_left() {
        let check = Some("python3 -B -S check.py");
        let mut messages = crate::tui::events::tests::closeout_messages();
        assert!(
            !native_work_remains(&messages, check, 20),
            "every artifact is edited and the exact check passed"
        );
        // Trial T, round 4: a non-unique edit anchor, then the target read.
        for (id, tool, args, result) in [
            (
                "anchor",
                "patch_file",
                json!({"path":"source.py","search":"    return x\n","replace":"    return y\n"}),
                "ERROR: search string is not unique in 'source.py' (2 matches); add more context",
            ),
            (
                "target",
                "read_file",
                json!({"path":"source.py"}),
                "[source.py] (3 lines, 36 bytes)\ndef f(x):\n    return x\n    return x\n",
            ),
        ] {
            messages.extend([
                json!({"role":"assistant","tool_calls":[{"id":id,"type":"function","function":{"name":tool,"arguments":args.to_string()}}]}),
                json!({"role":"tool","tool_call_id":id,"content":result}),
            ]);
        }
        assert!(!agent::task_obligations::project(&messages, check)
            .pending_recovery
            .is_empty());
        assert!(native_work_remains(&messages, check, 20));
        // The report turn stays report-only.
        assert!(!native_work_remains(&messages, check, 1));
    }

    #[test]
    fn only_a_current_rejected_focused_review_returns_the_coders_own_work_once() {
        use crate::observer_action::diagnostic::Output;
        use crate::tui::coder_yield::CoderYieldReason;
        for case in [
            "review",
            "general",
            "initial",
            "diagnosis",
            "valid",
            "interrupted",
            "stale",
            "approval",
            "report",
            "no_work",
        ] {
            let (mut app, _dir) = crate::tui::events::tests::interaction_app();
            let mut messages = crate::tui::events::tests::closeout_messages();
            if case != "no_work" {
                messages.truncate(6);
            }
            if case == "approval" {
                messages.extend([
                    json!({"role":"assistant","tool_calls":[{"id":"denied","function":{"name":"exec","arguments":"{\"command\":\"verify\"}"}}]}),
                    json!({"role":"tool","tool_call_id":"denied","content":"REJECTED BY USER"}),
                ]);
            }
            app.coder_session = Some(crate::agent_session::AgentSession::new(
                None, None, None, None, messages,
            ));
            app.coder_yield_reason = Some(if case == "diagnosis" {
                CoderYieldReason::DiagnosticFollowup
            } else {
                CoderYieldReason::AuthoredTestReview
            });
            app.work_budget = crate::work_budget::WorkBudget::new(
                "task",
                if case == "report" { 2 } else { 12 },
                None,
            );
            let mut request = ObserverAssistRequest::new(&app, "Review".into());
            request.output = if case == "general" {
                Output::General
            } else {
                Output::TestReview
            };
            request.repair_started = case != "initial";
            app.observer_assist_request = Some(request);
            app.work_budget
                .begin("observer", &json!({}))
                .unwrap()
                .status(if case == "interrupted" {
                    "stream_interrupted"
                } else {
                    "completed"
                });
            let raw = if case == "valid" {
                json!({"summary":"Review the tests","suggestions":[{"kind":"clarify","reason":"Compare both outputs","confidence":0.91,"suggested_tool":null,"suggested_args":{}}]})
            } else {
                json!({"response_contract":{"required":["add tests"]},"suggestions":{"kind":"clarify"}})
            };
            app.observer
                .messages
                .push(Message::new_complete(Role::Assistant, raw.to_string()));
            if case == "stale" {
                app.coder_session
                    .as_mut()
                    .unwrap()
                    .messages
                    .push(json!({"role":"user","content":"A new task"}));
            }
            assert_eq!(
                take_rejected_test_review(&mut app),
                (case == "review").then_some(0),
                "{case}"
            );
            assert!(take_rejected_test_review(&mut app).is_none(), "{case}");
            assert!(app.pending_observer_hint.is_none() && app.pending_observer_advice.is_none());
            let work = app.work_budget.snapshot();
            assert_eq!(work.requests.len(), 1);
            assert!(work.assists.is_empty());
        }
    }
}
