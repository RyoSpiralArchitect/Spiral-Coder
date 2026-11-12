//! Record the delivered advice independently of its optional response contract.
use super::super::{agent, suggestion, App, Role};

fn explicit_observer_response_decision(text: &str) -> Option<suggestion::ObserverResponseMode> {
    let lower = text.to_ascii_lowercase();
    for line in lower.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("decision:") {
            return parse_observer_response_mode(rest.trim());
        }
        if let Some(rest) = trimmed.strip_prefix("observer_response:") {
            return parse_observer_response_mode(rest.trim());
        }
        if trimmed.starts_with("[observer response]") {
            if trimmed.contains("override") {
                return Some(suggestion::ObserverResponseMode::Override);
            }
            if trimmed.contains("defer") {
                return Some(suggestion::ObserverResponseMode::Defer);
            }
            if trimmed.contains("accept") {
                return Some(suggestion::ObserverResponseMode::Accept);
            }
        }
    }
    None
}

fn parse_observer_response_mode(raw: &str) -> Option<suggestion::ObserverResponseMode> {
    let t = raw.trim().to_ascii_lowercase();
    match t.as_str() {
        "accept" => Some(suggestion::ObserverResponseMode::Accept),
        "override" => Some(suggestion::ObserverResponseMode::Override),
        "defer" => Some(suggestion::ObserverResponseMode::Defer),
        _ => None,
    }
}

fn tool_call_matches_observer_suggestion(
    call_name: &str,
    call_args_json: &str,
    primary: &suggestion::ObserverSuggestion,
) -> bool {
    let Some(tool) = primary.suggested_tool.as_deref() else {
        return false;
    };
    if tool != call_name {
        return false;
    }
    let expected = match primary.suggested_args.as_object() {
        Some(obj) if !obj.is_empty() => obj,
        _ => return true,
    };
    let parsed = serde_json::from_str::<serde_json::Value>(call_args_json).ok();
    let Some(actual) = parsed.as_ref().and_then(|v| v.as_object()) else {
        return false;
    };
    expected
        .iter()
        .all(|(key, value)| actual.get(key) == Some(value))
}

pub(in crate::tui::events) fn finish(app: &mut App) {
    let first_tool = app.active_observer_first_tool.take();
    let assist_id = app
        .active_assist_id
        .take()
        .unwrap_or(app.observer_generation);
    let Some(env) = app.active_observer_advice.take() else {
        return;
    };
    let explicit = if env.response_contract.required {
        app.coder
            .messages
            .iter()
            .rev()
            .find(|m| {
                matches!(m.role, Role::Assistant) && m.complete && !m.content.trim().is_empty()
            })
            .and_then(|m| explicit_observer_response_decision(&m.content))
    } else {
        None
    };
    let inferred = match (suggestion::primary_suggestion(&env), first_tool.as_ref()) {
        (Some(primary), Some((tool, args)))
            if tool_call_matches_observer_suggestion(tool, args, primary) =>
        {
            Some(suggestion::ObserverResponseMode::Accept)
        }
        (Some(_), Some(_)) => Some(suggestion::ObserverResponseMode::Override),
        _ => None,
    };
    let decision = explicit
        .or(inferred)
        .unwrap_or(suggestion::ObserverResponseMode::Defer);
    let focus = if env.response_contract.focus_axes.is_empty() {
        "current blocker".to_string()
    } else {
        env.response_contract.focus_axes.join(", ")
    };
    let note = match decision {
        suggestion::ObserverResponseMode::Accept => {
            "(observer contract) coder accepted the latest observer suggestion."
        }
        suggestion::ObserverResponseMode::Override => {
            "(observer contract) coder overrode the latest observer suggestion."
        }
        suggestion::ObserverResponseMode::Defer => {
            "(observer contract) coder deferred the latest observer suggestion."
        }
    };
    if let Some(session) = app.coder_session.as_ref() {
        let evidence =
            agent::evidence_packet::project(&session.messages, app.project_test_cmd.as_deref());
        let actions: Vec<_> = evidence
            .actions
            .iter()
            .filter(|a| !app.active_observer_start.contains(&a.call_id))
            .collect();
        let executed = actions.iter().any(|a| a.executed == Some(true));
        let succeeded = if actions.is_empty() || actions.iter().any(|a| a.succeeded.is_none()) {
            None
        } else {
            Some(actions.iter().all(|a| a.succeeded == Some(true)))
        };
        let suggestion_execution = super::project(suggestion::primary_suggestion(&env), &actions);
        let suggestion_executed = suggestion_execution
            .as_ref()
            .and_then(|receipt| receipt.executed);
        let suggestion_succeeded = suggestion_execution
            .as_ref()
            .and_then(|receipt| receipt.succeeded);
        let receipt = crate::work_budget::AssistReceipt {
            assist_id,
            continuation_id: app.coder_generation,
            decision: decision.label().into(),
            call_ids: actions.iter().map(|a| a.call_id.clone()).collect(),
            executed,
            succeeded,
            completed: app.coder_completed,
            pending_exact_checks: evidence.pending_exact_checks,
            suggestion_execution,
        };
        if let Err(error) = app.work_budget.record_assist(receipt) {
            app.coder
                .push_tool(format!("Could not save assist receipt: {error}"));
        }
        if env.response_contract.required {
            app.coder.push_tool(format!(
                "{note} continuation_executed={executed} continuation_succeeded={succeeded:?} suggested_action_executed={suggestion_executed:?} suggested_action_succeeded={suggestion_succeeded:?} completed={:?}; focus: {focus}",
                app.coder_completed
            ));
        }
    } else if env.response_contract.required {
        app.coder.push_tool(format!(
            "{note} Execution evidence unavailable. focus: {focus}"
        ));
    }
}
