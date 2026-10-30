// Exercise the same Error handler and pending-work functions used by TUI ticks.
fn queue_interrupted_observer(app: &mut App, terminal: bool) {
    seed_observer_failure(app);
    app.observer_assist_request = Some(ObserverAssistRequest::new(app, "original packet".into()).for_reason(if terminal {"blocked_terminal_diagnosis"} else {"diagnose"}));
    app.observer_next_action_mode = true;
    app.observer.streaming = true;
    app.observer.messages.push(Message::new_streaming(Role::Assistant));
    handle_observer_token(StreamToken::Delta("retained partial".into()), app);
    let guard = app.work_budget.begin("observer", &json!({})).unwrap();
    guard.interrupted(crate::work_budget::StreamInterruption::MissingTerminalFrame);
    drop(guard);
    handle_observer_token(StreamToken::Error("native stream ended before terminal".into()), app);
    assert!(app.pending_observer_hint.is_none());
    assert!(app.last_observer_suggestion.is_none());
    assert!(app.observer.messages.iter().any(|m| m.content == "retained partial"));
}

#[tokio::test]
async fn observer_reconnect_is_canceled_by_context_route_or_human_cancellation() {
    for change in ["intent", "root", "native", "model", "endpoint", "key", "max_tokens", "language", "check", "cancel"] {
        let (mut app, _root) = interaction_app();
        queue_interrupted_observer(&mut app, false);
        let (tx, mut rx) = mpsc::channel(16);
        match change {
            "intent" => app.coder.push_user("A new task".into()),
            "root" => app.tool_root = Some("/another-root".into()),
            "native" => app.coder_session = Some(crate::agent_session::AgentSession::new(None,None,None,None,vec![json!({"role":"user","content":"new native context"})])),
            "model" => app.observer_cfg.model.push_str("-new"),
            "endpoint" => app.observer_cfg.base_url = "http://127.0.0.1:1/new".into(),
            "key" => app.observer_cfg.api_key = Some("a-different-local-test-key".into()),
            "max_tokens" => app.observer_assist_max_tokens = Some(1025),
            "language" => app.lang = "fr".into(),
            "check" => app.project_test_cmd = Some("a-different-check".into()),
            "cancel" => {
                app.focus = Focus::Right;
                app.right_tab = RightTab::Observer;
                let (internal_tx, _rx) = mpsc::channel(8);
                handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL), &mut app, &tx, &tx, &tx, &internal_tx).await.unwrap();
            }
            _ => unreachable!(),
        }
        assert!(!observer_request::reconnect::maybe_start(&mut app, &tx).await, "{change}");
        assert!(app.observer_assist_request.is_none(), "{change}");
        assert!(rx.try_recv().is_err());
        assert_eq!(app.work_budget.snapshot().requests.len(), 1);
    }
}

#[tokio::test]
async fn observer_reconnect_preserves_continuation_room_and_never_retries_terminal_diagnosis() {
    for reason in ["one_request", "no_coder", "terminal", "spent_after_queue"] {
        let (mut app, _root) = interaction_app();
        app.work_budget = crate::work_budget::WorkBudget::new("task", if reason == "one_request" {2} else if reason == "no_coder" {6} else {4}, None);
        if reason == "no_coder" {
            app.work_budget.configure(crate::work_budget::BudgetPolicy {observer_reserved_requests:4, transport_repair_limit:4}).unwrap();
            for _ in 0..2 { drop(app.work_budget.begin("coder", &json!({})).unwrap()); }
        }
        queue_interrupted_observer(&mut app, reason == "terminal");
        if reason == "spent_after_queue" {
            for _ in 0..2 { drop(app.work_budget.begin("coder", &json!({})).unwrap()); }
        }
        let spent = app.work_budget.snapshot().requests.len();
        let (tx, mut rx) = mpsc::channel(16);
        assert!(!observer_request::reconnect::maybe_start(&mut app, &tx).await, "{reason}");
        // Reset normal de-duplication: the retained failed owner must still stop
        // the next automatic assist or ordinary observer commentary from firing.
        app.last_auto_next_action_sig = None;
        app.last_auto_obs_idx = None;
        maybe_auto_next_action_assist(&mut app, &tx).await;
        maybe_auto_observe(&mut app, &tx).await;
        assert!(observer_request::reconnect::blocks_automatic(&app));
        assert!(!app.observer.streaming);
        assert!(rx.try_recv().is_err());
        assert_eq!(app.work_budget.snapshot().requests.len(), spent);
    }
}

#[tokio::test]
async fn observer_reconnect_requires_a_current_typed_receipt_not_error_text() {
    let (mut app, _root) = interaction_app();
    seed_observer_failure(&mut app);
    app.observer_assist_request = Some(ObserverAssistRequest::new(&app,"packet".into()));
    let guard = app.work_budget.begin("observer", &json!({})).unwrap();
    // Older receipts and unrelated protocol errors have this broad status only.
    guard.status("stream_interrupted"); drop(guard);
    app.observer_next_action_mode = true;
    handle_observer_token(StreamToken::Error("stream ended before a terminal frame".into()), &mut app);
    let (tx, _rx) = mpsc::channel(8);
    assert!(!observer_request::reconnect::maybe_start(&mut app, &tx).await);
    assert!(observer_request::reconnect::blocks_automatic(&app));
}
