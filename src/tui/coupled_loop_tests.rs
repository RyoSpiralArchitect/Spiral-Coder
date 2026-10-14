#[tokio::test]
async fn observer_continuation_keeps_native_evidence_and_finishes_the_original_request() {
    assert_native_continuation("[GOVERNOR BLOCK] Missing final report.", 0, false).await;
}

#[tokio::test]
async fn a_successful_tool_at_the_chunk_cap_still_reaches_observer_and_final_report() {
    assert_native_continuation("[RESULT][Verifying] exit=0\n[agent] iteration cap (4) reached.", 2, false).await;
}

#[tokio::test]
async fn runtime_accepted_plan_survives_observer_continuation_without_replanning() {
    assert_native_continuation("[agent] iteration cap (4) reached.", 2, true).await;
}

async fn assert_native_continuation(display: &str, spent: usize, rescued_plan: bool) {
    use httpmock::prelude::*;
    let server = MockServer::start();
    let (mut app, root) = interaction_app();
    let goal = "Read only: inspect note.txt. Final answer must include `inspection complete`.";
    app.tool_root = Some(root.path().display().to_string());
    std::fs::write(root.path().join("note.txt"), "retained evidence").unwrap();
    app.coder_intent_anchor = Some(intent::apply_intent_update(None, intent::normalize_intent_update(goal, None), goal));
    app.coder.push_user(goal.into());
    app.coder.messages.push(msg(Role::Assistant, display));
    app.coder_completed = Some(false);
    app.coder_session = Some(crate::agent_session::AgentSession::new(app.tool_root.clone(), None, app.tool_root.clone(), None, vec![
        crate::task_origin::user_message(goal, crate::task_origin::MessageOrigin::User),
        json!({"role":"assistant","tool_calls":[{"id":"retained-read","type":"function","function":{"name":"read_file","arguments":"{\"path\":\"note.txt\"}"}}]}),
        json!({"role":"tool","tool_call_id":"retained-read","content":"[note.txt]\nretained evidence"}),
    ]));
    if rescued_plan {
        app.coder_session.as_mut().unwrap().messages[1]["accepted_plan"] = json!({
            "goal":"Report inspected note.txt",
            "steps":["Inspect note.txt", "Report retained evidence"],
            "acceptance_criteria":["inspection complete"],
            "risks":"none", "assumptions":"note.txt was read"
        });
    }
    app.work_budget = crate::work_budget::WorkBudget::new(goal, 4, None);
    for _ in 0..spent { drop(app.work_budget.begin("coder", &json!({})).unwrap()); }
    for cfg in [&mut app.observer_cfg, &mut app.coder_cfg] {
        cfg.base_url = server.base_url(); cfg.api_key = Some("local-test-key".into()); cfg.timeout_seconds = 5;
    }
    let sse = |content: serde_json::Value| format!("data: {content}\n\ndata: [DONE]\n\n");
    let observer = server.mock(|when, then| {
        when.method(POST).path("/chat/completions").body_contains("intervention mode");
        then.status(200).header("content-type", "text/event-stream").body(sse(json!({"choices":[{"delta":{"content":suggestion_response("done", "done", json!({"summary":"inspection complete","completed_acceptance":["inspection complete"],"remaining_acceptance":[],"acceptance_evidence":[{"criterion":"inspection complete","command":"read_file note.txt","evidence":"retained evidence"}]}))},"finish_reason":"stop"}]})));
    });
    let coder = server.mock(|when, then| {
        when.method(POST).path("/chat/completions").body_contains("tool_call_id").body_contains("retained-read").body_contains("inspection complete");
        let content = "<plan>\ngoal: report the inspected note.txt\nsteps:\n1. Inspect the retained note.txt result\n2. Report retained evidence\nacceptance: inspection complete\nrisks: none\nassumptions: note.txt was read\n</plan>\n<think>\ngoal: report inspection\nstep: 2\ntool: done\nrisk: none\ndoubt: none\nnext: report retained evidence\nverify: prior native read\n</think>\n[Observer response]\ndecision: accept\nreason: evidence is already available";
        let content = if rescued_plan { content.split_once("</plan>").unwrap().1 } else { content };
        then.status(200).header("content-type", "text/event-stream").body(sse(json!({"choices":[{"delta":{"content":content,"tool_calls":[{"index":0,"id":"final-report","type":"function","function":{"name":"done","arguments":json!({"summary":"inspection complete: note.txt contains retained evidence","completed_acceptance":["inspection complete"],"remaining_acceptance":[],"acceptance_evidence":[{"criterion":"inspection complete","command":"read_file note.txt","evidence":"retained evidence"}]}).to_string()}}]},"finish_reason":"stop"}]})));
    });
    async fn drain(app: &mut App, rx: &mut mpsc::Receiver<RequestEvent>, pane: PaneId) {
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                let event = rx.recv().await.unwrap();
                let done = matches!(&event.payload, request_stream::Payload::Token(StreamToken::Done));
                request_stream::dispatch(event, app, pane);
                if done { break; }
            }
        }).await.unwrap();
    }
    let (obs_tx, mut obs_rx) = mpsc::channel(64);
    maybe_auto_next_action_assist(&mut app, &obs_tx).await;
    drain(&mut app, &mut obs_rx, PaneId::Observer).await;
    assert!(app.pending_observer_hint.is_some(), "{:?}", app.observer.messages);
    let (tx, mut rx) = mpsc::channel(256);
    send_coder_input(&mut app, &tx, "Continue.".into(), CoderInput::RuntimeContinuation).await;
    drain(&mut app, &mut rx, PaneId::Coder).await;
    assert_eq!(app.coder_completed, Some(true), "{:?}", app.coder.messages);
    observer.assert_hits(1); coder.assert_hits(1);
    let session = app.coder_session.as_ref().unwrap();
    assert_eq!(crate::task_origin::root_user_text(&session.messages), goal);
    assert!(session.messages.iter().any(|m| m["tool_call_id"] == "retained-read"));
    assert_eq!(app.coder_completed, Some(true), "{:?}", app.coder.messages);
    let receipt = &app.work_budget.snapshot().assists[0];
    assert_eq!(receipt.completed, Some(true));
    assert_eq!(app.work_budget.snapshot().requests.len(), spent + 2);
}

#[test]
fn stale_observer_events_cannot_modify_a_new_request_or_its_repair_budget() {
    let (mut app, _root) = interaction_app(); seed_observer_failure(&mut app);
    app.observer_generation = 2;
    app.observer.streaming = true;
    app.observer.messages.push(Message::new_streaming(Role::Assistant));
    app.observer_assist_request = Some(ObserverAssistRequest::new(&app, "new request".into()));
    for token in [StreamToken::Delta(repair_response_missing_tool()), StreamToken::Done, StreamToken::Error("old error".into())] {
        request_stream::dispatch(RequestEvent {generation:1, payload:request_stream::Payload::Token(token)}, &mut app, PaneId::Observer);
    }
    assert!(app.observer.streaming);
    assert!(app.observer.messages.last().unwrap().content.is_empty());
    assert!(app.observer_assist_request.as_ref().unwrap().pending_repair.is_none());
    assert!(app.pending_observer_hint.is_none());
}

#[test]
fn human_steering_changes_the_native_task_boundary_and_never_inherits_completion() {
    let old = crate::agent_session::AgentSession::new(None, None, None, None, vec![json!({"role":"user","content":"old task"}),json!({"role":"assistant","content":"done"})]);
    let new = coder_session::start_state(Some(&old), vec![ChatMessage {role:"user".into(),content:"Fix the new review finding and run the new check".into()}], false);
    assert_eq!(crate::task_origin::current_task_messages(&new.messages).len(), 1);
    assert_eq!(new.session_bridge, None);
}

#[test]
fn workspace_change_invalidates_inflight_native_and_observer_state() {
    let (mut app, _root) = interaction_app();
    seed_observer_failure(&mut app);
    app.coder_generation = 3;
    app.observer_generation = 5;
    app.project_test_cmd = Some("old tests".into());
    app.coder_session = Some(crate::agent_session::AgentSession::new(None, None, None, None, vec![json!({"role":"user","content":"old"})]));
    coder_session::change_root(&mut app);
    request_stream::dispatch(RequestEvent {generation:3, payload:request_stream::Payload::Session(crate::agent_session::AgentSession::new(None,None,None,None,vec![]))}, &mut app, PaneId::Coder);
    assert!(app.coder_session.is_none());
    assert!(app.project_test_cmd.is_none());
    assert_eq!(app.work_budget.remaining(), 0);
    assert_eq!(app.observer_generation, 6);
}
