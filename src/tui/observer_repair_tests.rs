fn repair_response_missing_tool() -> String {
    let mut response: serde_json::Value = serde_json::from_str(&suggestion_response("verify", "exec", json!({"command":"cargo test"}))).unwrap();
    response["suggestions"][0].as_object_mut().unwrap().remove("suggested_tool");
    response.to_string()
}

fn seed_observer_failure(app: &mut App) {
    let goal = "Fix src/lib.rs and run cargo test";
    app.coder_intent_anchor = Some(intent::apply_intent_update(None, intent::normalize_intent_update(goal, None), goal));
    app.coder.push_user(goal.into());
    app.coder.messages.push(msg(Role::Assistant, "[GOVERNOR BLOCK] Missing verification."));
}

#[test]
fn observer_repair_keeps_invalid_source_and_never_infers_a_tool() {
    let (mut app, _root) = interaction_app();
    seed_observer_failure(&mut app);
    let raw = repair_response_missing_tool();
    let outcome = replay_observer_next_action_case(&mut app, "last-fail", "missing_verification", &raw).unwrap();
    let prompt = outcome.observer_repair_prompt.unwrap();
    assert!(prompt.starts_with(&outcome.observer_prompt));
    assert!(prompt.contains("suggested_tool is required for kind=verify"));
    assert!(prompt.contains("attempt 1 of 1"));
    assert!(app.observer.messages.iter().any(|m| m.role == Role::Assistant && m.content == raw));
    assert!(outcome.pending_observer_hint.is_none());
    assert!(outcome.pending_observer_contract.is_none());
    assert!(app.last_observer_suggestion.as_ref().unwrap().suggestions[0].suggested_tool.is_none());
}

#[test]
fn observer_repair_does_not_inflate_uncertainty_or_repair_valid_no_tool_advice() {
    for (kind, confidence) in [("verify", 0.3), ("clarify", 0.95)] {
        let (mut app, _root) = interaction_app();
        seed_observer_failure(&mut app);
        let mut response: serde_json::Value = serde_json::from_str(&suggestion_response(kind, "exec", json!({"command":"cargo test"}))).unwrap();
        response["suggestions"][0]["confidence"] = json!(confidence);
        if kind == "clarify" {
            response["suggestions"][0]["suggested_tool"] = json!(null);
            response["suggestions"][0]["suggested_args"] = json!({});
        }
        let outcome = replay_observer_next_action_case(&mut app, "last-fail", "missing_verification", &response.to_string()).unwrap();
        assert!(outcome.observer_repair_prompt.is_none());
        assert_eq!(outcome.pending_observer_hint.is_some(), kind == "clarify");
    }
}

#[tokio::test]
async fn observer_repair_round_trip_is_bounded_and_uses_the_real_transport_and_consumer() {
    use httpmock::prelude::*;
    fn is_repair(request: &HttpMockRequest) -> bool {
        String::from_utf8_lossy(request.body.as_deref().unwrap_or_default()).contains("[Observer action repair:")
    }
    async fn finish_response(app: &mut App, rx: &mut mpsc::Receiver<StreamToken>) {
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                let token = rx.recv().await.expect("response token");
                let finished = matches!(token, StreamToken::Done | StreamToken::Error(_));
                handle_observer_token(token, app);
                if finished { break; }
            }
        }).await.expect("bounded local response");
    }
    for repaired in [true, false] {
        let server = MockServer::start();
        let initial = repair_response_missing_tool();
        let final_response = if repaired { suggestion_response("verify", "exec", json!({"command":"cargo test"})) } else { "{malformed".to_string() };
        let sse = |content: &str| format!("data: {}\n\ndata: [DONE]\n\n", json!({"choices":[{"delta":{"content":content},"finish_reason":"stop"}]}));
        let first = server.mock(|when, then| {
            when.method(POST).path("/chat/completions").matches(|r| !is_repair(r));
            then.status(200).header("content-type", "text/event-stream").body(sse(&initial));
        });
        let repair = server.mock(|when, then| {
            when.method(POST).path("/chat/completions").matches(is_repair).body_contains("suggested_tool is required for kind=verify");
            then.status(200).header("content-type", "text/event-stream").body(sse(&final_response));
        });
        let (mut app, root) = interaction_app();
        seed_observer_failure(&mut app);
        app.observer_cfg.base_url = server.base_url();
        app.observer_cfg.api_key = Some("local-test-key".into());
        app.observer_cfg.timeout_seconds = 5;
        let (tx, mut rx) = mpsc::channel(32);
        send_next_action_assist(&mut app, &tx, "last-fail", "missing_verification").await;
        finish_response(&mut app, &mut rx).await;
        assert!(app.pending_observer_hint.is_none());
        assert!(app.pending_observer_contract.is_none());
        maybe_observer_action_repair(&mut app, &tx).await;
        finish_response(&mut app, &mut rx).await;
        for _ in 0..3 { maybe_observer_action_repair(&mut app, &tx).await; }
        first.assert_hits(1);
        repair.assert_hits(1);
        assert_eq!(app.pending_observer_hint.is_some(), repaired);
        assert_eq!(app.pending_observer_contract.is_some(), repaired);
        assert!(app.observer_assist_request.as_ref().unwrap().pending_repair.is_none());
        let assistants: Vec<_> = app.observer.messages.iter().filter(|m| m.role == Role::Assistant).collect();
        assert_eq!(assistants.len(), 2);
        assert_eq!(assistants[0].content, initial);
        if !repaired { assert_eq!(assistants[1].content, final_response); }
        assert!(!root.path().join("unrelated-file").exists());
        assert!(!app.coder.streaming);
    }
}

#[test]
fn observer_empty_response_cannot_reuse_an_older_successful_answer() {
    let (mut app, _root) = interaction_app();
    seed_observer_failure(&mut app);
    app.observer.messages.push(msg(Role::Assistant, &suggestion_response("verify", "exec", json!({"command":"cargo test"}))));
    let outcome = replay_observer_next_action_case(&mut app, "last-fail", "missing_verification", "").unwrap();
    assert!(outcome.parsed_suggestion.is_none());
    assert!(outcome.pending_observer_hint.is_none());
    assert!(outcome.observer_repair_prompt.is_some());
}

#[tokio::test]
async fn observer_repair_and_pending_advice_expire_when_coder_context_changes() {
    let (mut app, _root) = interaction_app();
    seed_observer_failure(&mut app);
    replay_observer_next_action_case(&mut app, "last-fail", "missing_verification", &repair_response_missing_tool()).unwrap();
    app.coder.messages.last_mut().unwrap().content.push_str(" Updated failure evidence.");
    let (tx, _rx) = mpsc::channel(8);
    maybe_observer_action_repair(&mut app, &tx).await;
    assert!(app.observer_assist_request.is_none());
    assert!(!app.observer.streaming);
    // A late completed response also fails the same source-context check.
    app.observer_assist_request = Some(ObserverAssistRequest::new(&app, "prompt".into()));
    app.coder.push_user("A different task.".into());
    app.observer_next_action_mode = true;
    app.observer.messages.push(msg(Role::Assistant, &suggestion_response("verify", "exec", json!({"command":"cargo test"}))));
    handle_observer_token(StreamToken::Done, &mut app);
    assert!(app.pending_observer_hint.is_none());
    assert!(app.last_observer_suggestion.is_none());

    replay_observer_next_action_case(&mut app, "last-fail", "missing_verification", &suggestion_response("verify", "exec", json!({"command":"cargo test"}))).unwrap();
    assert!(app.pending_observer_hint.is_some());
    let new_goal = "Create an unrelated web project";
    let anchor = intent::apply_intent_update(app.coder_intent_anchor.as_ref(), intent::normalize_intent_update(new_goal, app.coder_intent_anchor.as_ref()), new_goal);
    observer_request::prepare_coder_continuation(&mut app, &anchor);
    assert!(app.pending_observer_hint.is_none());
    assert!(app.pending_observer_contract.is_none());
}

#[tokio::test]
async fn observer_completed_advice_is_scoped_and_cancel_clears_pending_repair() {
    let (mut app, _root) = interaction_app();
    seed_observer_failure(&mut app);
    let response = suggestion_response("verify", "exec", json!({"command":"cargo test"}));
    replay_observer_next_action_case(&mut app, "last-fail", "missing_verification", &response).unwrap();
    let same = intent::apply_intent_update(app.coder_intent_anchor.as_ref(), intent::normalize_intent_update("Continue.", app.coder_intent_anchor.as_ref()), "Continue.");
    observer_request::prepare_coder_continuation(&mut app, &same);
    assert!(app.pending_observer_hint.is_some(), "completed same-scope advice survives until consumption");
    replay_observer_next_action_case(&mut app, "last-fail", "missing_verification", &response).unwrap();
    app.coder.messages.last_mut().unwrap().content.push_str(" New evidence.");
    let (tx, _rx) = mpsc::channel(8);
    maybe_observer_action_repair(&mut app, &tx).await;
    observer_request::prepare_coder_continuation(&mut app, &same);
    assert!(app.pending_observer_hint.is_none(), "tick must not lose the scope stamp while keeping stale advice");
    assert!(app.pending_observer_contract.is_none());

    replay_observer_next_action_case(&mut app, "last-fail", "missing_verification", &repair_response_missing_tool()).unwrap();
    app.focus = Focus::Right;
    app.right_tab = RightTab::Observer;
    let (internal_tx, _internal_rx) = mpsc::channel(8);
    handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL), &mut app, &tx, &tx, &tx, &internal_tx).await.unwrap();
    assert!(app.observer_assist_request.is_none());
    maybe_observer_action_repair(&mut app, &tx).await;
    assert!(!app.observer.streaming);
}
