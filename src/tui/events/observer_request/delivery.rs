//! Give a bounded native read window back to the Coder after rejecting oversized advice.
use super::*;

pub(super) fn return_for_overlong_read(
    reply: Option<&suggestion::ObserverSuggestionEnvelope>,
    app: &App,
) -> Option<serde_json::Value> {
    let request = app.observer_assist_request.as_ref()?;
    if request.terminal_diagnosis {
        return None;
    }
    let original = reply?;
    // Only the primary action can supply a window. Secondary proposals and
    // the model's response contract never become part of the native handoff.
    let primary = original.suggestions.first()?;
    if primary.suggested_tool.as_deref() != Some("read_file")
        || primary.confidence < 0.75
        || primary
            .suggested_args
            .as_object()?
            .keys()
            .any(|k| !matches!(k.as_str(), "path" | "start_line" | "end_line"))
    {
        return None;
    }
    let read = serde_json::from_value::<crate::file_tools::read_range::ReadRequest>(
        primary.suggested_args.clone(),
    )
    .ok()?;
    let window = read.first_window()?;
    let session = app.coder_session.as_ref()?;
    let obligations =
        agent::task_obligations::project(&session.messages, app.project_test_cmd.as_deref());
    if obligations.required_diagnostic_read.is_none()
        && (obligations.required_recheck.is_some() || obligations.report_repair.is_some())
    {
        return None;
    }
    let allocation = app.work_budget.snapshot();
    let coder = allocation.coder_remaining();
    let costly = allocation
        .request_limit
        .saturating_sub(allocation.task_requests())
        == coder
        && obligations
            .minimum_delivery
            .as_ref()
            .is_some_and(|floor| floor.minimum_coder_requests >= coder);
    if coder == 0 || !(request.repair_started || costly) {
        return None;
    }
    if !obligations
        .task_requirements
        .pending_artifact_paths
        .contains(&read.path)
        && !obligations
            .required_diagnostic_read
            .as_ref()
            .is_some_and(|r| r.path == read.path)
    {
        return None;
    }
    let mut bounded = original.clone();
    bounded.suggestions.truncate(1);
    bounded.response_contract = Default::default();
    bounded.suggestions[0].suggested_args = window.clone();
    let text = serde_json::to_string(&bounded).ok()?;
    crate::observer_action::validation_issue_with_capacity(
        &text,
        &session.messages,
        app.project_test_cmd.as_deref(),
        Some(coder),
    )
    .is_none()
    .then_some(window)
}

pub(in crate::tui::events) fn take_delivery_window(app: &mut App) -> Option<serde_json::Value> {
    let current = request_is_current(app);
    let window = app
        .observer_assist_request
        .as_mut()?
        .return_for_delivery
        .take();
    if current {
        window
    } else {
        None
    }
}

#[cfg(test)]
mod delivery_tests {
    use super::*;
    use crate::agent_session::AgentSession;

    #[test]
    fn spent_repair_can_return_only_a_validated_bounded_read_of_an_outstanding_target() {
        let (mut app, _temp) = crate::tui::events::tests::interaction_app();
        let task="Document the CLI.\n<task_requirements>\n{\"version\":1,\"requirements\":[{\"id\":\"guide\",\"description\":\"Document behavior\",\"artifacts\":[{\"path\":\"guide.md\",\"kind\":\"documentation\"}],\"checks\":[{\"command\":\"cargo test\"}]}]}\n</task_requirements>";
        app.coder_session = Some(AgentSession::new(
            None,
            None,
            None,
            None,
            vec![json!({"role":"user","content":task})],
        ));
        app.work_budget = crate::work_budget::WorkBudget::new(task, 32, None);
        app.observer_assist_request = Some(ObserverAssistRequest::new(&app, "Inspect".into()));
        let raw = |path: &str, end: usize| {
            json!({"summary":"inspect before editing","suggestions":[{"kind":"read","suggested_tool":"read_file","suggested_args":{"path":path,"start_line":1,"end_line":end},"confidence":0.95}]}).to_string()
        };
        let read = |path, end| suggestion::parse_observer_suggestion_envelope(&raw(path, end));
        assert!(return_for_overlong_read(read("guide.md", 220).as_ref(), &app).is_none());
        app.observer_assist_request.as_mut().unwrap().repair_started = true;
        assert_eq!(
            return_for_overlong_read(read("guide.md", 220).as_ref(), &app),
            Some(json!({"path":"guide.md","start_line":1,"end_line":200}))
        );
        for (path, end) in [
            ("unrelated.md", 220),
            ("guide.md", 200),
            ("../guide.md", 220),
        ] {
            assert!(return_for_overlong_read(read(path, end).as_ref(), &app).is_none());
        }
        app.work_budget = crate::work_budget::WorkBudget::new(task, 3, None);
        app.observer_assist_request.as_mut().unwrap().repair_started = false;
        assert!(return_for_overlong_read(read("guide.md", 220).as_ref(), &app).is_some());
        app.observer_assist_request
            .as_mut()
            .unwrap()
            .terminal_diagnosis = true;
        assert!(return_for_overlong_read(read("guide.md", 220).as_ref(), &app).is_none());
    }
}
