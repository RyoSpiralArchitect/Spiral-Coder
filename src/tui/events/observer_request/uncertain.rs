//! A valid but uncertain primary action can trigger a native report, never an edit.
use super::*;

pub(super) fn queue_report(app: &mut App, reply: &suggestion::ObserverSuggestionEnvelope) {
    let Some(request) = app.observer_assist_request.as_ref() else {
        return;
    };
    if request.terminal_diagnosis || app.coder.streaming || app.pending_observer_hint.is_some() {
        return;
    }
    let Some(primary) = suggestion::primary_suggestion(reply) else {
        return;
    };
    if primary.confidence >= 0.75
        || !super::super::report_closeout::ready(
            app,
            super::super::report_closeout::Reason::LowConfidence,
        )
    {
        return;
    }
    app.observer_assist_request
        .as_mut()
        .unwrap()
        .pending_report_closeout = Some(super::super::report_closeout::Reason::LowConfidence);
    app.observer.push_tool("(next-action) primary confidence is below 0.75; advice was not forwarded. Returning one native report-only request to preserve unfinished work. No suggested tool or arguments are adopted.".into());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_handoff_preserves_threshold_terminal_and_approval_boundaries() {
        for case in [
            "low",
            "threshold",
            "terminal",
            "stale",
            "refused",
            "pending_approval",
            "blocked",
            "completed",
            "undeclared",
            "spent",
            "attempted",
            "malformed",
        ] {
            let (mut app, _root) = crate::tui::events::tests::interaction_app();
            let mut messages = crate::tui::events::tests::closeout_messages();
            messages.truncate(6);
            if case == "undeclared" {
                messages[0]["content"] = json!("An ordinary task");
            }
            if matches!(case, "refused" | "pending_approval") {
                messages.extend([
                json!({"role":"assistant","tool_calls":[{"id":"refusal","function":{"name":"exec","arguments":"{\"command\":\"dangerous\"}"}}]}),
                json!({"role":"tool","tool_call_id":"refusal","content":if case=="refused" {"REJECTED BY USER"} else {"Awaiting approval"}})
            ]);
            }
            app.coder_session = Some(crate::agent_session::AgentSession::new(
                None, None, None, None, messages,
            ));
            app.work_budget = crate::work_budget::WorkBudget::new(
                "task",
                if case == "spent" { 0 } else { 10 },
                None,
            );
            app.report_closeout_attempted = case == "attempted";
            if case == "blocked" {
                app.coder_yield_reason = Some(crate::tui::coder_yield::CoderYieldReason::Blocked);
            }
            if case == "completed" {
                app.coder_completed = Some(true);
            }
            app.observer_assist_request = Some(
                ObserverAssistRequest::new(&app, "Inspect".into()).for_reason(
                    if case == "terminal" {
                        "blocked_terminal_diagnosis"
                    } else {
                        "incomplete_task"
                    },
                ),
            );
            if case == "stale" {
                app.coder_session
                    .as_mut()
                    .unwrap()
                    .messages
                    .push(json!({"role":"user","content":"Different task"}));
            }
            let raw = if case == "malformed" {
                r#"{"summary":"a","summary":"duplicate"}"#.to_owned()
            } else {
                json!({"summary":"Uncertain","suggestions":[{"kind":"read","confidence":if case=="threshold" {0.75} else {0.74},"suggested_tool":"read_file","suggested_args":{"path":"source.py"}}]}).to_string()
            };
            let mut message = Message::new_streaming(Role::Assistant);
            message.content = raw;
            message.complete = true;
            app.observer.messages.push(message);
            finalize_observer_next_action_suggestion(&mut app);
            let queued = app
                .observer_assist_request
                .as_ref()
                .and_then(|r| r.pending_report_closeout);
            assert_eq!(
                queued.is_some(),
                case == "low",
                "{case}: {:?}",
                app.observer.messages
            );
            if case == "low" {
                assert!(
                    app.pending_observer_hint.is_none() && app.pending_observer_advice.is_none()
                );
            }
            if case == "threshold" {
                assert!(app.pending_observer_hint.is_some());
            }
        }
    }
}
