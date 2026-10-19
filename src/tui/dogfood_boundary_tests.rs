#[tokio::test]
async fn malformed_native_arguments_retry_without_inventing_an_edit_failure() {
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
    app.project_test_cmd = None;
    app.work_budget = crate::work_budget::WorkBudget::new(goal, 2, None);
    app.coder_cfg.base_url = server.base_url();
    app.coder_cfg.api_key = Some("local-test-key".into());
    app.coder_cfg.timeout_seconds = 5;
    let content = "<plan>\ngoal: repair config.json status\nsteps:\n1. Patch the observed status from before to after\n2. Verify the changed file\nacceptance: config.json status is after and verification passes\nrisks: preserve other fields\nassumptions: config.json was read\n</plan>\n<think>\ngoal: repair config.json status\nstep: 1\ntool: patch_file\nrisk: preserve other fields\ndoubt: none\nnext: verify the changed file\nverify: inspect status after patch\n</think>\n<evidence>\ntarget_files: 1) config.json\ntarget_symbols: 1) status\nevidence: the prior read shows status is before\nopen_questions: none\nnext_probe: patch config.json status to after\n</evidence>";
    let response = |id, arguments: &str| {
        format!(
            "data: {}\n\ndata: [DONE]\n\n",
            json!({"choices":[{"delta":{"content":content,"tool_calls":[{"index":0,"id":id,"type":"function","function":{"name":"patch_file","arguments":arguments}}]},"finish_reason":"stop"}]})
        )
    };
    let broken = server.mock(|when, then| {
        when.method(POST).path("/chat/completions").matches(|r| {
            !String::from_utf8_lossy(r.body.as_deref().unwrap_or_default())
                .contains("[Tool arguments]")
        });
        then.status(200)
            .header("content-type", "text/event-stream")
            .body(response(
                "broken-json",
                "{\"path\":\"config.json\",\"search\":\"before\",\"replace\":\"after",
            ));
    });
    let repaired = server.mock(|when, then| {
        when.method(POST)
            .path("/chat/completions")
            .body_contains("[Tool arguments]");
        then.status(200)
            .header("content-type", "text/event-stream")
            .body(response(
                "repaired-json",
                "{\"path\":\"config.json\",\"search\":\"before\",\"replace\":\"after\"}",
            ));
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
    assert_eq!(
        std::fs::read_to_string(root.path().join("config.json")).unwrap(),
        "{\"status\":\"after\"}",
        "{:?}",
        app.coder.messages
    );
    broken.assert_hits(1);
    repaired.assert_hits(1);
    assert_eq!(app.work_budget.snapshot().requests.len(), 2);
    assert_eq!(app.coder_completed, Some(false));
    let messages = &app.coder_session.as_ref().unwrap().messages;
    let evidence = agent::evidence_packet::project(messages, None);
    let refused = evidence
        .actions
        .iter()
        .find(|a| a.call_id == "broken-json")
        .unwrap();
    assert_eq!(refused.executed, Some(false));
    assert!(messages.iter().any(|m| m["tool_call_id"] == "broken-json"
        && m["content"].as_str().unwrap_or("").contains("JSON")));
}

#[tokio::test]
async fn final_report_wording_cannot_auto_complete_an_unedited_task() {
    use httpmock::prelude::*;
    let server = MockServer::start();
    let (mut app, root) = interaction_app();
    app.tool_root = Some(root.path().display().to_string());
    std::fs::write(root.path().join("pilot.py"), "# unchanged\n").unwrap();
    let goal="Implement --case filtering in pilot.py and run tests. Your final report must name the changed files, state the actual test outcomes, and disclose anything incomplete.";
    app.coder_cfg.base_url = server.base_url();
    app.coder_cfg.api_key = Some("local-test-key".into());
    app.coder_cfg.timeout_seconds = 5;
    app.coder_max_iters = Some(3);
    app.project_stack_label = Some("fixture".into());
    app.project_test_cmd = Some("printf baseline-passed > checks-ran.txt".into());
    let wire=server.mock(|when,then| {
        when.method(POST).path("/chat/completions");
        then.status(200).header("content-type","text/event-stream").body(format!("data: {}\n\ndata: [DONE]\n\n",json!({"choices":[{"delta":{"content":"call:default_api:read_file{path:"},"finish_reason":"stop"}]})));
    });
    let (tx, mut rx) = mpsc::channel(256);
    send_coder_with_text(&mut app, &tx, goal.into()).await;
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
    assert_eq!(app.coder_completed, Some(false), "{:?}", app.coder.messages);
    assert!(
        !root.path().join("checks-ran.txt").exists(),
        "An existing passing test cannot supply the requested authored report"
    );
    assert_eq!(
        std::fs::read_to_string(root.path().join("pilot.py")).unwrap(),
        "# unchanged\n"
    );
    assert_eq!(app.work_budget.snapshot().requests.len(), 3);
    wire.assert_hits(3);
}

#[tokio::test]
async fn authored_closeout_does_not_block_a_repair_read_after_an_auto_test() {
    use httpmock::prelude::*;
    let server = MockServer::start();
    let (mut app, root) = interaction_app();
    app.tool_root = Some(root.path().display().to_string());
    std::fs::write(root.path().join("config.json"), "{\"status\":\"partial\"}").unwrap();
    let goal = "Finish changing config.json status to after. Your final report must name the changed files and test outcomes.";
    let plan = "<plan>\ngoal: Finish changing config.json status to after\nsteps:\n1. Inspect config.json\n2. Repair the remaining status value\n3. Verify and report the result\nacceptance:\n1. config.json has the requested status\n2. Verification passes\nrisks: preserve other fields\nassumptions: config.json exists\n</plan>";
    let messages = vec![
        crate::task_origin::user_message(goal, crate::task_origin::MessageOrigin::User),
        json!({"role":"assistant","content":plan,"tool_calls":[{"id":"read","function":{"name":"read_file","arguments":"{\"path\":\"config.json\"}"}}]}),
        json!({"role":"tool","tool_call_id":"read","content":"[config.json]\n{\"status\":\"before\"}"}),
        json!({"role":"assistant","tool_calls":[{"id":"patch","function":{"name":"patch_file","arguments":"{\"path\":\"config.json\",\"search\":\"before\",\"replace\":\"partial\"}"}}]}),
        json!({"role":"tool","tool_call_id":"patch","content":"OK: patched 'config.json'\n[auto-test] ✓ PASSED (exit 0)"}),
        json!({"role":"assistant","tool_calls":[{"id":"miss","function":{"name":"patch_file","arguments":"{\"path\":\"config.json\",\"search\":\"missing\",\"replace\":\"after\"}"}}]}),
        json!({"role":"tool","tool_call_id":"miss","content":"ERROR: search string not found in 'config.json'."}),
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
    app.project_test_cmd = Some("python3 -S -m unittest".into());
    app.work_budget = crate::work_budget::WorkBudget::new(goal, 1, None);
    app.coder_cfg.base_url = server.base_url();
    app.coder_cfg.api_key = Some("local-test-key".into());
    app.coder_cfg.timeout_seconds = 5;
    let content = format!("{plan}\n<think>\ngoal: repair config.json\nstep: 2\ntool: read_file\nrisk: none\ndoubt: patch anchor changed\nnext: read config.json to find the current anchor\nverify: read config.json\n</think>");
    let wire = server.mock(|when, then| {
        when.method(POST).path("/chat/completions");
        then.status(200).header("content-type", "text/event-stream").body(format!("data: {}\n\ndata: [DONE]\n\n", json!({"choices":[{"delta":{"content":content,"tool_calls":[{"index":0,"id":"repair-read","type":"function","function":{"name":"read_file","arguments":"{\"path\":\"config.json\"}"}}]},"finish_reason":"stop"}]})));
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
    wire.assert_hits(1);
    let session = app.coder_session.as_ref().unwrap();
    let result = session
        .messages
        .iter()
        .find(|m| m["tool_call_id"] == "repair-read")
        .unwrap();
    assert!(
        result["content"]
            .as_str()
            .unwrap()
            .starts_with("[config.json]"),
        "{result}"
    );
    assert_eq!(app.coder_completed, Some(false));
}
