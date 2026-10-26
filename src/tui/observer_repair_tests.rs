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
fn declared_repair_brief_reaches_coder_tui_and_browser_after_native_resume() {
    let (mut app, _root) = interaction_app();
    seed_observer_failure(&mut app);
    let goal = "Repair src/lib.rs.\n<task_requirements>\n{\"version\":1,\"requirements\":[{\"id\":\"repair\",\"description\":\"Repair behavior\",\"artifacts\":[{\"path\":\"src/lib.rs\",\"kind\":\"source\"}],\"checks\":[{\"command\":\"cargo test\"}]}]}\n</task_requirements>";
    let messages = vec![
        json!({"role":"user","content":goal}),
        json!({"role":"assistant","tool_calls":[{"id":"observed","function":{"name":"read_file","arguments":"{\"path\":\"src/lib.rs\",\"start_line\":30,\"end_line\":60}"}}]}),
        json!({"role":"tool","tool_call_id":"observed","content":"[src/lib.rs] (90 lines, 900 bytes) [range: lines 30-60; partial file]\nobserved source"}),
        json!({"role":"assistant","tool_calls":[{"id":"before","function":{"name":"exec","arguments":"{\"command\":\"cargo test\"}"}}]}),
        json!({"role":"tool","tool_call_id":"before","content":"FAILED (exit_code: 1)\nFAIL: test_reverse (selection.Tests.test_reverse)"}),
        json!({"role":"assistant","tool_calls":[{"id":"after","function":{"name":"exec","arguments":"{\"command\":\"cargo test\"}"}}]}),
        json!({"role":"tool","tool_call_id":"after","content":"FAILED (exit_code: 1)\nFAIL: test_default (selection.Tests.test_default)"}),
    ];
    let resumed: Vec<serde_json::Value> = serde_json::from_str(&serde_json::to_string(&messages).unwrap()).unwrap();
    app.coder_session = Some(crate::agent_session::AgentSession::new(None,None,None,None,resumed.clone()));
    let mut encoded: serde_json::Value = serde_json::from_str(&suggestion_response("read", "read_file", json!({"path":"src/lib.rs"}))).unwrap();
    encoded["suggestions"][0]["suggested_args"] = json!(encoded["suggestions"][0]["suggested_args"].to_string());
    let response = format!("Next action.\n```json\n{encoded}\n```\nEnd.");
    let tui = replay_observer_next_action_case(&mut app,"last-fail","diagnose",&response).unwrap();
    assert!(tui.pending_observer_hint.is_some() && tui.observer_repair_prompt.is_none());
    let displayed = app.observer.messages.iter().rev().find(|m| m.role == Role::Assistant).unwrap();
    assert_eq!(displayed.raw_content.as_deref(), Some(response.as_str()));
    assert!(displayed.content.starts_with("--- blocker ---"));
    assert_eq!(tui.observer_raw_response, response);
    let coder = agent::task_obligations::prompt(&resumed,None).unwrap();
    for prompt in [&coder, &tui.observer_prompt] {
        assert!(prompt.contains("repair_brief") && prompt.contains("bookkeeping"));
        assert!(prompt.contains("observed"));
        assert!(prompt.contains("recent_check_failures") && prompt.contains("test_default") && prompt.contains("test_reverse"));
    }
    let view = serde_json::to_value(agent::task_obligations::project(&resumed,None)).unwrap();
    let browser = crate::observer_action::evaluate(crate::observer_action::Request {
        packet:json!({"task_obligations":{"repair_brief":{"source_artifacts":["invented.rs"]}}}),
        messages:resumed,test_command:None,lang:"en".into(),reason:"diagnose".into(),
        response:Some(response),final_summary:None,repair_native:true,
    });
    assert_eq!(browser.packet["task_obligations"]["repair_brief"],view["repair_brief"]);
    assert_eq!(view["repair_brief"]["source_artifacts"],json!(["src/lib.rs"]));
    assert_eq!(view["repair_brief"]["observed_sources"][0]["start_line"],30);
    assert_eq!(view["repair_brief"]["recent_check_failures"][1]["call_id"],"after");
}

#[test]
fn observer_wrong_diagnostic_target_is_repaired_consistently_after_resume() {
    let (mut app, _root) = interaction_app();
    seed_observer_failure(&mut app);
    let mut messages = vec![
        json!({"role":"user","content":"Fix greet.py without changing the supplied tests"}),
        json!({"role":"assistant","tool_calls":[{"id":"red","function":{"name":"exec","arguments":"{\"command\":\"python3 test_greet.py\"}"}}]}),
        json!({"role":"tool","tool_call_id":"red","content":"⚠ LOGIC ERROR: assertion failed\nFAILED (exit_code: 1)","recovery_focus":{"version":1,"state":{"pending":{"path":"test_greet.py","diagnostic":"AssertionError: wrong punctuation","check":"python3 test_greet.py","observation":"awaiting_read","location":"current"}}}}),
    ];
    messages = serde_json::from_str(&serde_json::to_string(&messages).unwrap()).unwrap();
    app.coder_session = Some(crate::agent_session::AgentSession::new(None,None,None,None,messages.clone()));
    let wrong = suggestion_response("read", "read_file", json!({"path":"greet.py"}));
    let tui = replay_observer_next_action_case(&mut app, "last-fail", "diagnose", &wrong).unwrap();
    assert!(tui.observer_prompt.contains("required_diagnostic_read"));
    assert!(tui.pending_observer_hint.is_none());
    assert!(tui.observer_repair_prompt.unwrap().contains("read_file(path=test_greet.py)"));
    let web = crate::observer_action::evaluate(crate::observer_action::Request {
        packet:json!({"task_obligations":{"required_diagnostic_read":null}}),messages:messages.clone(),
        test_command:None,lang:"en".into(),reason:"diagnose".into(),response:Some(wrong.clone()),final_summary:None,repair_native:true,
    });
    assert!(web.issue.unwrap().contains("read_file(path=test_greet.py)"));
    assert!(web.hint.is_none());
    assert_eq!(web.packet["task_obligations"]["required_diagnostic_read"]["path"],"test_greet.py");
    let right = suggestion_response("read", "read_file", json!({"path":"./test_greet.py"}));
    let tui = replay_observer_next_action_case(&mut app,"last-fail","diagnose",&right).unwrap();
    assert!(tui.pending_observer_hint.is_some()); assert!(tui.observer_repair_prompt.is_none());
    let mut uncertain: serde_json::Value = serde_json::from_str(&wrong).unwrap();
    uncertain["suggestions"][0]["confidence"] = json!(0.2);
    assert!(crate::observer_action::validation_issue(&uncertain.to_string(), &messages, None).is_none());
    // A real read discharges only the diagnosis route, not the failed check.
    messages.extend([
        json!({"role":"assistant","tool_calls":[{"id":"read","function":{"name":"read_file","arguments":"{\"path\":\"test_greet.py\"}"}}]}),
        json!({"role":"tool","tool_call_id":"read","content":"[test_greet.py] (2 lines, 30 bytes)\nassert greet('') == 'Hello, !'"}),
    ]);
    assert!(crate::observer_action::validation_issue(&wrong, &messages, None).is_none());
    let view=agent::task_obligations::project(&messages,None);
    assert!(view.required_diagnostic_read.is_none()); assert!(!view.pending_recovery.is_empty());
    messages.push(json!({"role":"user","content":"Explain another task"}));
    assert!(agent::task_obligations::project(&messages,None).required_diagnostic_read.is_none());
}

#[test]
fn observer_replay_routes_the_new_failure_and_keeps_the_previous_check_pending() {
    let (mut app, _root) = interaction_app();
    seed_observer_failure(&mut app);
    let messages = vec![
        json!({"role":"user","content":"Repair source.rs; preserve both supplied tests"}),
        json!({"role":"assistant","tool_calls":[{"id":"new","function":{"name":"patch_file","arguments":"{\"path\":\"source.rs\"}"}}]}),
        json!({"role":"tool","tool_call_id":"new","content":"OK: patched 'source.rs'\n[auto-test] ✗ FAILED (exit 101)","recovery_focus":{"version":2,"state":{
            "pending":{"path":"later_test.rs","diagnostic":"thread 'regression' panicked at later_test.rs:397:5:","check":"cargo test --locked","observation":"awaiting_read","location":"current"},
            "deferred":[{"path":"visible_test.rs","diagnostic":"visible regression failed","check":"cargo test visible","observation":"repair_attempted","location":"current"}]
        }}}),
    ];
    app.coder_session = Some(crate::agent_session::AgentSession::new(None,None,None,None,messages.clone()));
    let stale = suggestion_response("read", "read_file", json!({"path":"visible_test.rs"}));
    let tui = replay_observer_next_action_case(&mut app, "last-fail", "diagnose", &stale).unwrap();
    assert!(tui.pending_observer_hint.is_none());
    assert!(tui.observer_repair_prompt.unwrap().contains("read_file(path=later_test.rs)"));
    assert!(tui.observer_prompt.contains("cargo test visible"));
    let right = suggestion_response("read", "read_file", json!({"path":"later_test.rs","start_line":380,"end_line":410}));
    let tui = replay_observer_next_action_case(&mut app,"last-fail","diagnose",&right).unwrap();
    assert!(tui.pending_observer_hint.is_some());
    let web = crate::observer_action::evaluate(crate::observer_action::Request {
        packet:json!({"task_obligations":{"required_diagnostic_read":{"path":"visible_test.rs"}}}),messages,
        test_command:Some("cargo test --locked".into()),lang:"en".into(),reason:"diagnose".into(),response:Some(stale),final_summary:None,repair_native:true,
    });
    assert!(web.hint.is_none());
    assert!(web.issue.unwrap().contains("read_file(path=later_test.rs)"));
    assert_eq!(web.packet["task_obligations"]["required_diagnostic_read"]["failed_check"], "cargo test --locked");
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
fn observer_repair_preserves_the_specific_serde_diagnostic_in_tui_and_browser() {
    let mut invalid = serde_json::from_str::<serde_json::Value>(&suggestion_response("edit", "patch_file", json!({"path":"artifact.json","search":"old","replace":"new"}))).unwrap();
    invalid["suggestions"][0]["kind"] = json!("patch_file");
    let raw = invalid.to_string();
    let (mut app, _root) = interaction_app();
    seed_observer_failure(&mut app);
    let outcome = replay_observer_next_action_case(&mut app, "last-fail", "bad_kind", &raw).unwrap();
    let prompt = outcome.observer_repair_prompt.unwrap();
    assert!(prompt.contains("unknown variant `patch_file`"), "{prompt}");
    assert!(prompt.contains("`edit`"));
    assert!(outcome.pending_observer_hint.is_none());
    let browser = crate::observer_action::evaluate(crate::observer_action::Request {
        packet: json!({}), messages: vec![], test_command: None, lang:"en".into(), reason:"bad_kind".into(), response:Some(raw), final_summary:None, repair_native:false,
    });
    assert!(browser.issue.unwrap().contains("unknown variant `patch_file`"));
    assert!(browser.repair_prompt.unwrap().contains("unknown variant `patch_file`"));
    assert!(browser.hint.is_none());
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
    async fn finish_response(app: &mut App, rx: &mut mpsc::Receiver<RequestEvent>) {
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                let token = rx.recv().await.expect("response token");
                let finished = matches!(&token.payload, request_stream::Payload::Token(StreamToken::Done | StreamToken::Error(_)));
                request_stream::dispatch(token, app, PaneId::Observer);
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

#[test]
fn native_report_refusal_rejects_stale_read_advice_in_tui_replay() {
    let (mut app, _root) = interaction_app();
    seed_observer_failure(&mut app);
    app.project_test_cmd = Some("python3 -S -m unittest -v".into());
    let messages = crate::observer_action::report_repair_tests::rejected_report();
    app.coder_session = Some(crate::agent_session::AgentSession::new(None, None, None, None, messages));
    let raw = suggestion_response("read", "read_file", json!({"path":"greet.py"}));
    let replay = replay_observer_next_action_case(&mut app, "last-fail", "tool_error", &raw).unwrap();
    assert!(replay.observer_prompt.contains("reason_hint: authored_report"));
    assert!(replay.observer_repair_prompt.unwrap().contains("handoff-ready"));
    assert!(replay.pending_observer_hint.is_none());
    assert!(replay.pending_observer_contract.is_none());
    assert!(app.observer.messages.iter().any(|m| m.role == Role::Assistant && m.content == raw));
}
