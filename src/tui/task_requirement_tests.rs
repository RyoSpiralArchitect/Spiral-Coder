#[tokio::test]
async fn task_requirement_gate_survives_resume_and_reaches_both_roles() {
    for claim_complete in [true, false] {
        check_task_requirement_handoff(claim_complete).await;
    }
}

async fn check_task_requirement_handoff(claim_complete: bool) {
    use httpmock::prelude::*;
    let server = MockServer::start();
    let (mut app, root) = interaction_app();
    app.tool_root = Some(root.path().display().to_string());
    let goal = format!("Finish the selection feature, tests and documentation.\n<task_requirements>\n{}\n</task_requirements>", json!({
        "version":1,"requirements":[{"id":"selection","description":"Implement and document selection",
        "artifacts":[{"path":"runner.py","kind":"source"},{"path":"test_runner.py","kind":"test"},{"path":"usage.md","kind":"documentation"}],
        "checks":[{"command":"python3 acceptance.py","baseline_failure":true}]}]}));
    let plan = "<plan>\ngoal: Finish the selection feature\nsteps:\n1. Implement selection\n2. Run verification\nacceptance:\n1. Selection passes verification\nrisks: incomplete wiring\nassumptions: native receipts are available\n</plan>";
    let mut messages = vec![json!({"role":"user","content":goal}), json!({"role":"assistant","content":plan})];
    for (id, tool, args, result) in [
        ("baseline", "exec", json!({"command":"python3 acceptance.py"}), "FAILED (exit_code: 1)"),
        ("edit", "patch_file", json!({"path":"runner.py","search":"old","replace":"new"}), "OK: patched 'runner.py'"),
        ("green", "exec", json!({"command":"python3 acceptance.py"}), "OK (exit_code: 0)"),
    ] {
        messages.extend([json!({"role":"assistant","tool_calls":[{"id":id,"function":{"name":tool,"arguments":args.to_string()}}]}),json!({"role":"tool","tool_call_id":id,"content":result})]);
    }
    app.coder_session = Some(crate::agent_session::AgentSession::new(app.tool_root.clone(), None, app.tool_root.clone(), None, messages));
    app.coder_intent_anchor = Some(intent::apply_intent_update(None, intent::normalize_intent_update(&goal,None), &goal));
    app.coder.push_user(goal.clone());
    app.coder.messages.push(msg(Role::Assistant,"[agent] iteration cap reached."));
    app.project_stack_label = Some("fixture".into());
    app.project_test_cmd = None;
    app.work_budget = crate::work_budget::WorkBudget::new(&goal,1,None);
    app.coder_cfg.base_url = server.base_url();
    app.coder_cfg.api_key = Some("local-test-key".into());
    app.coder_cfg.timeout_seconds = 5;
    let wire = server.mock(|when,then| {
        when.method(POST).path("/chat/completions").body_contains("task_requirements");
        let content=format!("{plan}\n<think>\ngoal: report selection\nstep: 2\ntool: done\nrisk: incomplete work\ndoubt: none\nnext: report\nverify: python3 acceptance.py\n</think>");
        let args = if claim_complete {
            json!({"summary":"Everything implemented and verified.","completed_acceptance":["acceptance 1"],"remaining_acceptance":[],"acceptance_evidence":[{"criterion":"acceptance 1","command":"python3 acceptance.py"}]})
        } else {
            json!({"summary":"Incomplete. test_runner.py and usage.md still need edits.","completed_acceptance":[],"remaining_acceptance":["acceptance 1"],"acceptance_evidence":[]})
        };
        then.status(200).header("content-type","text/event-stream").body(format!("data: {}\n\ndata: [DONE]\n\n",json!({"choices":[{"delta":{"content":content,"tool_calls":[{"index":0,"id":"overclaim","type":"function","function":{"name":"done","arguments":args.to_string()}}]},"finish_reason":"stop"}]})));
    });
    let (tx,mut rx)=mpsc::channel(256);
    send_coder_input(&mut app,&tx,"Continue.".into(),CoderInput::RuntimeContinuation).await;
    tokio::time::timeout(std::time::Duration::from_secs(10),async {
        while let Some(event)=rx.recv().await {
            let done=matches!(&event.payload,request_stream::Payload::Token(StreamToken::Done));
            request_stream::dispatch(event,&mut app,PaneId::Coder);
            if done { break; }
        }
    }).await.unwrap();
    wire.assert_hits(1);
    assert_eq!(app.coder_completed,Some(false));
    let messages=&app.coder_session.as_ref().unwrap().messages;
    assert!(agent::evidence_packet::project(messages,None).pending_exact_checks.is_empty());
    if claim_complete {
        assert!(messages.iter().any(|m| m["tool_call_id"]=="overclaim" && m["content"].as_str().unwrap_or("").contains("[Requirement Gate]")), "{messages:?}");
    } else {
        assert!(messages.iter().any(|m| m["tool_call_id"]=="overclaim" && m["content"]=="OK: done"), "an honest partial report may stop, remaining incomplete: {messages:?}");
    }
    let view=serde_json::to_value(agent::task_obligations::project(messages,None)).unwrap();
    assert!(view["task_requirements"]["pending"].to_string().contains("test_runner.py"));
    let browser=crate::observer_action::evaluate(crate::observer_action::Request {
        packet:json!({"task_obligations":{"task_requirements":{"pending":[]}}}),
        messages:messages.clone(),test_command:None,lang:"en".into(),reason:"incomplete_task".into(),
        response:None,final_summary:Some("Everything complete.".into()),repair_native:true,
    });
    assert!(browser.completion_issue.unwrap().contains("usage.md"));
    assert_eq!(browser.packet["task_obligations"]["task_requirements"],view["task_requirements"]);
}
