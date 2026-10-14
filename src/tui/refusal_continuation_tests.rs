#[tokio::test]
async fn progress_refusal_does_not_invent_diagnosis_after_continuation() {
    use httpmock::prelude::*;
    let server = MockServer::start();
    let (mut app, root) = interaction_app();
    app.tool_root = Some(root.path().display().to_string());
    let goal = "Fix config.json status from before to after and verify it. Final answer must include config.json.";
    std::fs::write(root.path().join("config.json"), "{\"status\":\"before\"}").unwrap();
    let messages = vec![
        crate::task_origin::user_message(goal, crate::task_origin::MessageOrigin::User),
        json!({"role":"assistant","tool_calls":[{"id":"observed-config","type":"function","function":{"name":"read_file","arguments":"{\"path\":\"config.json\"}"}}]}),
        json!({"role":"tool","tool_call_id":"observed-config","content":"[config.json] (1 lines, 19 bytes)\n{\"status\":\"before\"}"}),
        json!({"role":"assistant","tool_calls":[{"id":"blocked-check","type":"function","function":{"name":"exec","arguments":"{\"command\":\"cargo test\"}"}}]}),
        json!({"role":"tool","tool_call_id":"blocked-check","content":"GOVERNOR BLOCKED\n\n[Evaluator Loop]\npolicy_id: force_mutation_after_observation_loop\nRequired now: use one of patch_file, apply_diff.\nExit hint: Patch config.json with the minimal change, then verify."}),
    ];
    app.coder_session = Some(crate::agent_session::AgentSession::new(
        app.tool_root.clone(),
        None,
        app.tool_root.clone(),
        None,
        messages,
    ));
    app.coder_intent_anchor = Some(intent::apply_intent_update(
        None,
        intent::normalize_intent_update(goal, None),
        goal,
    ));
    app.coder.push_user(goal.into());
    app.coder
        .messages
        .push(msg(Role::Assistant, "[agent] iteration cap reached."));
    app.project_stack_label = Some("fixture".into());
    app.work_budget = crate::work_budget::WorkBudget::new(goal, 1, None);
    app.coder_cfg.base_url = server.base_url();
    app.coder_cfg.api_key = Some("local-test-key".into());
    app.coder_cfg.timeout_seconds = 5;
    let coder = server.mock(|when,then| {
        when.method(POST).path("/chat/completions").body_contains("blocked-check");
        let content = "<plan>\ngoal: repair config.json status\nsteps:\n1. Patch the observed status from before to after\n2. Verify the changed file\nacceptance: config.json status is after and verification passes\nrisks: preserve other fields\nassumptions: config.json was read\n</plan>\n<think>\ngoal: repair config.json status\nstep: 1\ntool: patch_file\nrisk: preserve other fields\ndoubt: none\nnext: verify the changed file\nverify: inspect status after patch\n</think>\n<evidence>\ntarget_files: 1) config.json\ntarget_symbols: 1) status\nevidence: the prior read shows status is before\nopen_questions: none\nnext_probe: patch config.json status to after\n</evidence>";
        let delta = json!({"content":content,"tool_calls":[{"index":0,"id":"repair-config","type":"function","function":{"name":"patch_file","arguments":json!({"path":"config.json","search":"before","replace":"after"}).to_string()}}]});
        then.status(200).header("content-type","text/event-stream").body(format!("data: {}\n\ndata: [DONE]\n\n",json!({"choices":[{"delta":delta,"finish_reason":"stop"}]})));
    });
    let (tx, mut rx) = mpsc::channel(256);
    send_coder_input(
        &mut app,
        &tx,
        "Continue.".into(),
        CoderInput::RuntimeContinuation,
    )
    .await;
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        while let Some(event) = rx.recv().await {
            let done = matches!(
                &event.payload,
                request_stream::Payload::Token(StreamToken::Done)
            );
            request_stream::dispatch(event, &mut app, PaneId::Coder);
            if done {
                break;
            }
        }
    })
    .await
    .unwrap();
    coder.assert_hits(1);
    assert_eq!(
        std::fs::read_to_string(root.path().join("config.json")).unwrap(),
        "{\"status\":\"after\"}",
        "{:?}",
        app.coder.messages
    );
    assert_eq!(
        app.coder_completed,
        Some(false),
        "editing does not replace verification or the final report"
    );
    let evidence =
        agent::evidence_packet::project(&app.coder_session.as_ref().unwrap().messages, None);
    let blocked = evidence
        .actions
        .iter()
        .find(|a| a.call_id == "blocked-check")
        .unwrap();
    assert_eq!(blocked.executed, Some(false));
    assert_eq!(app.work_budget.snapshot().requests.len(), 1);
}
