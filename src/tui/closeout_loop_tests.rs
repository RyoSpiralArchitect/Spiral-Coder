#[tokio::test]
async fn empty_provider_turn_cannot_complete_a_task_without_its_authored_report() {
    assert_authored_closeout_retry(false, false).await;
}

#[tokio::test]
async fn empty_provider_turn_can_recover_to_a_native_authored_report() {
    assert_authored_closeout_retry(true, false).await;
}

#[tokio::test]
async fn invalid_authored_acceptance_is_retried_without_reassigning_proof() {
    assert_authored_closeout_retry(true, true).await;
}

#[tokio::test]
async fn repeated_invalid_acceptance_stays_incomplete_within_the_request_cap() {
    assert_authored_closeout_retry(false, true).await;
}

async fn assert_authored_closeout_retry(report: bool, invalid_acceptance: bool) {
    use httpmock::prelude::*;
    let server = MockServer::start();
    let (mut app, root) = interaction_app();
    app.tool_root = Some(root.path().display().to_string());
    let suite: serde_json::Value =
        serde_json::from_str(include_str!("../../tests/fixtures/coupled-loop-v1.json")).unwrap();
    let goal = suite["cases"][1]["prompt"].as_str().unwrap();
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/runtime-benchmark-plan-exec-proof");
    for file in [
        ".spiral-coder/runtime_eval.json",
        "docs/runtime-architecture.md",
    ] {
        let target = root.path().join(file);
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::copy(source.join(file), target).unwrap();
    }
    std::fs::write(
        root.path().join("verification_receipt.txt"),
        "verified-current",
    )
    .unwrap();
    let first_check =
        "grep -q \"src/tui/agent/benchmark_proof.rs\" .spiral-coder/runtime_eval.json";
    let last_check = "grep -q \"src/tui/agent/benchmark_proof.rs\" docs/runtime-architecture.md && printf verified-current > verification_receipt.txt";
    let mut seed: crate::agent_session::AgentSession = serde_json::from_str(include_str!(
        "../../tests/fixtures/runtime-benchmark-plan-exec-proof/session_seed.json"
    ))
    .unwrap();
    seed.tool_root = app.tool_root.clone();
    seed.cur_cwd = app.tool_root.clone();
    seed.messages[0] =
        crate::task_origin::user_message(goal, crate::task_origin::MessageOrigin::User);
    seed.messages.extend([
        json!({"role":"assistant","content":"<impact>\nchanged: prepared benchmark artifacts\nprogress: verification now targets the current artifacts\nremaining_gap: final report after the remaining check\n</impact>","tool_calls":[{"id":"fresh-proof","type":"function","function":{"name":"exec","arguments":json!({"command":last_check}).to_string()}}]}),
        json!({"role":"tool","tool_call_id":"fresh-proof","content":format!("OK (exit_code: 0)\ncwd: {}", root.path().display())}),
    ]);
    app.coder_session = Some(seed);
    app.coder_intent_anchor = Some(intent::apply_intent_update(
        None,
        intent::normalize_intent_update(goal, None),
        goal,
    ));
    app.coder.push_user(goal.into());
    app.coder
        .messages
        .push(msg(Role::Assistant, "[agent] iteration cap reached."));
    app.project_test_cmd = Some(first_check.into());
    app.project_stack_label = Some("fixture".into());
    app.work_budget = crate::work_budget::WorkBudget::new(goal, 3, None);
    app.coder_cfg.base_url = server.base_url();
    app.coder_cfg.api_key = Some("local-test-key".into());
    app.coder_cfg.timeout_seconds = 5;
    let sse = |delta: serde_json::Value| {
        format!(
            "data: {}\n\ndata: [DONE]\n\n",
            json!({"choices":[{"delta":delta,"finish_reason":"stop"}]})
        )
    };
    let marker = if invalid_acceptance { "[Authored acceptance mapping]" } else { "[Closeout retry]" };
    let content = "<plan>\ngoal: Finish prepared .spiral-coder/runtime_eval.json and docs/runtime-architecture.md artifacts for src/tui/agent/benchmark_proof.rs using current verification evidence\nsteps:\n1. Inspect the retained native verification evidence\n2. Report the verified artifacts\nacceptance:\n- Both artifacts include src/tui/agent/benchmark_proof.rs\n- verification_receipt.txt contains verified-current\n- Fresh exec proof is provided\nrisks: do not claim stale proof\nassumptions: recorded tool results are current\n</plan>\n<think>\ngoal: report verified artifacts\nstep: 2\ntool: done\nrisk: stale proof\ndoubt: none\nnext: finish the original report\nverify: retained native exact checks\n</think>";
    let payload = |valid: bool| {
        let mut args = json!({"summary":"verification_receipt.txt contains verified-current; fresh exec proof is recorded.","completed_acceptance":["acceptance 1","acceptance 2","acceptance 3"],"remaining_acceptance":[],"acceptance_evidence":[{"criterion":"acceptance 1","command":first_check},{"criterion":"acceptance 2","command":last_check},{"criterion":"acceptance 3","command":last_check}]});
        if !valid { args["acceptance_evidence"][2]["criterion"] = json!("acceptance 1"); }
        json!({"content":content,"tool_calls":[{"index":0,"id":if valid {"authored-closeout"} else {"invalid-closeout"},"type":"function","function":{"name":"done","arguments":args.to_string()}}]})
    };
    let empty = server.mock(|when, then| {
        when.method(POST).path("/chat/completions").matches(|request| {
            let body = String::from_utf8_lossy(request.body.as_deref().unwrap_or_default());
            !body.contains("[Authored acceptance mapping]") && !body.contains("[Closeout retry]")
        });
        then.status(200).header("content-type", "text/event-stream")
            .body(sse(if invalid_acceptance { payload(false) } else { json!({"content":""}) }));
    });
    let retry = server.mock(|when,then| {
        when.method(POST).path("/chat/completions").body_contains(marker);
        let delta = if report { payload(true) }
            else if invalid_acceptance { payload(false) }
            else { json!({"content":""}) };
        then.status(200).header("content-type","text/event-stream").body(sse(delta));
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
        app.coder_completed,
        Some(report),
        "{:?}",
        app.coder.messages
    );
    empty.assert_hits(1);
    retry.assert_hits(if report { 1 } else { 2 });
    assert_eq!(
        app.work_budget.snapshot().requests.len(),
        if report { 2 } else { 3 }
    );
    let messages = &app.coder_session.as_ref().unwrap().messages;
    assert_eq!(crate::task_origin::root_user_text(messages), goal);
    assert!(messages.iter().any(|m| m["tool_call_id"] == "fresh-proof"));
    assert_eq!(
        messages
            .iter()
            .filter(|m| m["tool_call_id"] == "authored-closeout")
            .count(),
        usize::from(report)
    );
    if invalid_acceptance {
        assert!(messages.iter().any(|m| m["tool_call_id"] == "invalid-closeout" && m["content"].as_str().unwrap_or("").starts_with("GOVERNOR BLOCKED")));
        assert!(!app.coder.messages.iter().any(|m| m.content.contains("[compat] repaired done acceptance")));
    }
    assert!(
        app.coder
            .messages
            .iter()
            .all(|m| !m.content.contains("[goal_check:tests] run")),
        "a missing report must not rerun already successful checks"
    );
}
