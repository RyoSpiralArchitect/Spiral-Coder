#[tokio::test]
async fn native_verification_yield_requests_observer_without_display_keyword_or_auto_observe() {
    use httpmock::prelude::*;
    let server = MockServer::start();
    let (mut app, _root) = interaction_app();
    seed_observer_failure(&mut app);
    assert!(!app.auto_observe);
    app.coder.messages.pop();
    app.coder.streaming = true;
    app.coder.messages.push(Message::new_streaming(Role::Assistant));
    let display = "[RESULT] exit=1 !\n[agent] New verification failure; returning to Observer.";
    assert!(!meta_failure_like(display));
    handle_coder_token(StreamToken::Delta(display.into()), &mut app);
    handle_coder_token(StreamToken::Telemetry(crate::streaming::TelemetryEvent {
        event: "agent_outcome".into(),
        data: json!({"completed":false,"yield_reason":"verification_failure"}),
    }), &mut app);
    handle_coder_token(StreamToken::Done, &mut app);
    app.work_budget = crate::work_budget::WorkBudget::new("Fix src/lib.rs and run cargo test", 4, None);
    app.observer_cfg.base_url = server.base_url();
    app.observer_cfg.api_key = Some("offline".into());
    let response = suggestion_response("read", "read_file", json!({"path":"src/lib.rs"}));
    let request = server.mock(|when, then| {
        when.method(POST).path("/chat/completions").body_contains("native_verification_failure");
        then.status(200).header("content-type", "text/event-stream")
            .body(format!("data: {}\n\ndata: [DONE]\n\n", json!({"choices":[{"delta":{"content":response},"finish_reason":"stop"}]})));
    });
    let (tx, mut rx) = mpsc::channel(64);
    maybe_auto_next_action_assist(&mut app, &tx).await;
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while app.observer.streaming {
            request_stream::dispatch(rx.recv().await.unwrap(), &mut app, PaneId::Observer);
        }
    }).await.unwrap();
    request.assert_hits(1);
    assert!(app.pending_observer_hint.is_some());
    // The same completed slice cannot request a second assist.
    maybe_auto_next_action_assist(&mut app, &tx).await;
    assert!(!app.observer.streaming);
    assert_eq!(app.work_budget.snapshot().requests.len(), 1);
    coder_session::change_root(&mut app);
    assert!(app.coder_yield_reason.is_none());
    assert!(latest_tui_next_action_target(&app).is_none());
}
