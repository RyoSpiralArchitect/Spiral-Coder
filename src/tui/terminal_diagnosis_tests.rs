#[tokio::test]
async fn blocked_tui_uses_the_last_observer_slot_for_terminal_diagnosis() {
    use httpmock::prelude::*;
    let server = MockServer::start();
    let (mut app, _root) = interaction_app();
    seed_observer_failure(&mut app);
    app.coder_completed = Some(false);
    app.coder_yield_reason = Some(super::super::coder_yield::CoderYieldReason::Blocked);
    app.coder_session = Some(crate::agent_session::AgentSession::new(
        app.tool_root.clone(),
        None,
        app.tool_root.clone(),
        None,
        vec![crate::task_origin::user_message(
            "Fix src/lib.rs. Run exactly: cargo test",
            crate::task_origin::MessageOrigin::User,
        )],
    ));
    app.work_budget =
        crate::work_budget::WorkBudget::new("Fix src/lib.rs and run cargo test", 2, None);
    app.work_budget
        .configure(crate::work_budget::BudgetPolicy {
            observer_reserved_requests: 1,
            transport_repair_limit: 0,
        })
        .unwrap();
    drop(app.work_budget.begin("coder", &json!({})).unwrap());
    assert_eq!(app.work_budget.remaining(), 1);
    assert_eq!(app.work_budget.remaining_for("coder"), 0);
    app.observer_cfg.base_url = server.base_url();
    app.observer_cfg.api_key = Some("offline".into());
    let response = json!({
        "summary":"The native check remains unverified.",
        "primary_blocker":"Pending exact verification.",
        "response_contract":{"required":true,"focus_axes":["correctness"]},
        "suggestions":[{"kind":"clarify","reason":"Inspect the failing assertion before a future repair.","confidence":0.9,"suggested_tool":null,"suggested_args":{}}],
        "quickest_check":"Keep the exact pending verification unchanged."
    }).to_string();
    let request = server.mock(|when, then| {
        when.method(POST)
            .path("/chat/completions")
            .body_contains("[Blocked terminal diagnosis]");
        then.status(200)
            .header("content-type", "text/event-stream")
            .body(format!(
                "data: {}\n\ndata: [DONE]\n\n",
                json!({"choices":[{"delta":{"content":response},"finish_reason":"stop"}]})
            ));
    });
    let session_before = app.coder_session.as_ref().unwrap().messages.clone();
    let (tx, mut rx) = mpsc::channel(64);
    maybe_auto_next_action_assist(&mut app, &tx).await;
    assert!(
        app.observer.streaming,
        "one funded terminal diagnosis must start"
    );
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while app.observer.streaming {
            request_stream::dispatch(rx.recv().await.unwrap(), &mut app, PaneId::Observer);
        }
    })
    .await
    .unwrap();
    request.assert_hits(1);
    assert!(app.last_observer_suggestion.is_some());
    assert!(app.pending_observer_hint.is_none());
    assert!(app.pending_observer_contract.is_none());
    assert_eq!(app.coder_session.as_ref().unwrap().messages, session_before);
    assert_eq!(app.coder_completed, Some(false));
    assert!(!app.coder.streaming);
    assert_eq!(app.work_budget.remaining(), 0);
    assert_eq!(app.work_budget.snapshot().requests[1].role, "observer");
    maybe_auto_next_action_assist(&mut app, &tx).await;
    assert!(!app.observer.streaming);
    request.assert_hits(1);
}

#[tokio::test]
async fn terminal_diagnosis_does_not_borrow_or_restart_completed_work() {
    use super::super::coder_yield::CoderYieldReason;
    use httpmock::prelude::*;
    let server = MockServer::start();
    for (remaining, reason, completed) in [
        (0, CoderYieldReason::Blocked, false),
        (1, CoderYieldReason::VerificationFailure, false),
        (1, CoderYieldReason::Blocked, true),
    ] {
        let (mut app, _root) = interaction_app();
        seed_observer_failure(&mut app);
        app.coder_completed = Some(completed);
        app.coder_yield_reason = Some(reason);
        app.work_budget = crate::work_budget::WorkBudget::new("Fix src/lib.rs", 2, None);
        for _ in remaining..2 {
            drop(app.work_budget.begin("coder", &json!({})).unwrap());
        }
        app.observer_cfg.base_url = server.base_url();
        app.observer_cfg.api_key = Some("offline".into());
        let before = app.work_budget.snapshot().requests.len();
        let (tx, _rx) = mpsc::channel(64);
        maybe_auto_next_action_assist(&mut app, &tx).await;
        assert!(!app.observer.streaming);
        assert!(app.last_auto_next_action_sig.is_none());
        assert_eq!(app.work_budget.snapshot().requests.len(), before);
    }
}
