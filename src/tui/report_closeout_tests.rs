const CLOSEOUT_CHECK: &str = "python3 -B -S check.py";

pub(super) fn closeout_messages() -> Vec<serde_json::Value> {
    let goal = format!("Repair source.py and docs/usage.md. Final answer must include `handoff-ready`.\n<task_requirements>\n{}\n</task_requirements>", json!({"version":1,"requirements":[{"id":"repair","description":"Repair source and docs","artifacts":[{"path":"source.py","kind":"source"},{"path":"docs/usage.md","kind":"documentation"}],"checks":[{"command":CLOSEOUT_CHECK}]}]}));
    let mut messages = vec![json!({"role":"user","content":goal}), json!({"role":"assistant","accepted_plan":{"goal":"Repair source.py and docs/usage.md","steps":["Repair source and docs","Verify and report"],"acceptance_criteria":["Source is repaired","Docs explain behavior","Exact check passes","Report is accurate"],"risks":"regression","assumptions":"check exists"}})];
    for (id, tool, args, result) in [
        ("source-edit", "patch_file", json!({"path":"source.py"}), "OK: patched 'source.py'"),
        ("docs-edit", "patch_file", json!({"path":"docs/usage.md"}), "OK: patched 'docs/usage.md'"),
        ("current-check", "exec", json!({"command":CLOSEOUT_CHECK}), "OK (exit_code: 0)"),
    ] {
        messages.extend([json!({"role":"assistant","tool_calls":[{"id":id,"type":"function","function":{"name":tool,"arguments":args.to_string()}}]}),json!({"role":"tool","tool_call_id":id,"content":result})]);
    }
    messages
}

pub(super) fn closeout_args() -> serde_json::Value {
    json!({"summary":"handoff-ready: repaired source.py and docs/usage.md; the exact check passed.", "completed_acceptance":["acceptance 1","acceptance 2","acceptance 3","acceptance 4"], "remaining_acceptance":[], "acceptance_evidence":(1..=4).map(|i| json!({"criterion":format!("acceptance {i}"),"command":CLOSEOUT_CHECK})).collect::<Vec<_>>()})
}

fn closeout_app() -> (App, tempfile::TempDir) {
    let (mut app, root) = interaction_app();
    let messages = closeout_messages();
    let goal = messages[0]["content"].as_str().unwrap();
    app.tool_root = Some(root.path().display().to_string());
    app.coder_intent_anchor = Some(intent::apply_intent_update(None, intent::normalize_intent_update(goal,None),goal));
    app.work_budget = crate::work_budget::WorkBudget::new(goal, 10, None);
    app.coder.push_user(goal.into());
    app.coder.messages.push(msg(Role::Assistant,"[agent] iteration cap reached; authored report remains."));
    app.coder_completed = Some(false);
    app.project_test_cmd = Some(CLOSEOUT_CHECK.into());
    app.coder_session = Some(crate::agent_session::AgentSession::new(app.tool_root.clone(), None, app.tool_root.clone(), None, messages));
    (app, root)
}

#[test]
fn observer_done_vocabulary_matches_coder_in_tui_and_browser_without_rewriting_evidence() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!("../../tests/fixtures/plan-revision/observer-incomplete-evidence.json")).unwrap();
    let raw = fixture["raw"].as_str().unwrap();
    let (mut app, _root) = closeout_app();
    let session = app.coder_session.as_mut().unwrap();
    session.messages[1]["accepted_plan"] = fixture["accepted_plan"].clone();
    let messages = session.messages.clone();
    let tui = replay_observer_next_action_case(&mut app,"last-fail","incomplete_task",raw).unwrap();
    assert!(tui.pending_observer_hint.is_none());
    assert_eq!(tui.observer_raw_response, raw);
    assert!(tui.observer_repair_prompt.unwrap().contains("exactly once"));
    let browser = crate::observer_action::evaluate(crate::observer_action::Request {
        packet:json!({"task_obligations":{"acceptance_criteria":[{"reference":"invented"}]},"tool_argument_schemas":[{"name":"done","parameters":{}}]}),messages,test_command:Some(CLOSEOUT_CHECK.into()),lang:"en".into(),reason:"incomplete_task".into(),response:Some(raw.into()),final_summary:None,repair_native:false,
    });
    assert!(browser.issue.unwrap().contains("exactly once"));
    let done = |packet: &serde_json::Value| packet["tool_argument_schemas"].as_array().unwrap().iter().find(|s| s["name"]=="done").unwrap()["parameters"].clone();
    let props = done(&browser.packet)["properties"].clone();
    assert_eq!(done(&browser.packet), done(&tui.packet));
    assert_eq!(props["completed_acceptance"]["items"]["enum"], json!(["acceptance 1","acceptance 2","acceptance 3","acceptance 4"]));
    assert_eq!(props["acceptance_evidence"]["items"]["properties"]["command"]["enum"], browser.packet["task_obligations"]["current_commands"]);
    assert!(browser.packet["task_obligations"]["current_commands"].as_array().unwrap().contains(&json!(CLOSEOUT_CHECK)));
    assert!(props["summary"].get("default").is_none());
    assert!(tui.observer_prompt.contains("exactly one evidence row for every completed criterion"));
}

#[test]
fn report_closeout_requires_current_declared_proof_and_never_clears_missing_work() {
    for case in ["ready","missing_check","later_edit","human_steering","diagnosis","blocked","already_attempted","no_budget"] {
        let (mut app, _root) = closeout_app();
        match case {
            "missing_check" => { app.coder_session.as_mut().unwrap().messages.truncate(6); },
            "later_edit" => app.coder_session.as_mut().unwrap().messages.extend([json!({"role":"assistant","tool_calls":[{"id":"later","function":{"name":"patch_file","arguments":"{\"path\":\"source.py\"}"}}]}), json!({"role":"tool","tool_call_id":"later","content":"OK: patched 'source.py'"})]),
            "human_steering" => app.coder_session.as_mut().unwrap().messages.push(json!({"role":"user","content":"Repair a different bug"})),
            "diagnosis" => { app.coder_session.as_mut().unwrap().messages.last_mut().unwrap()["content"] = json!("FAILED (exit_code: 1)\nAssertionError: wrong result"); },
            "blocked" => app.coder_yield_reason = Some(super::super::coder_yield::CoderYieldReason::Blocked),
            "already_attempted" => app.report_closeout_attempted = true,
            "no_budget" => app.work_budget = crate::work_budget::WorkBudget::new("",0,None),
            _ => {},
        }
        let before = app.coder_session.as_ref().unwrap().messages.clone();
        assert_eq!(report_closeout::ready(&app),case=="ready","{case}");
        assert_eq!(before, app.coder_session.as_ref().unwrap().messages);
    }
}

async fn closeout_drain(app: &mut App, rx: &mut mpsc::Receiver<RequestEvent>, pane: PaneId) {
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        while let Some(event) = rx.recv().await {
            let done = matches!(&event.payload, request_stream::Payload::Token(StreamToken::Done));
            request_stream::dispatch(event, app, pane);
            if done { break; }
        }
    }).await.unwrap();
}

#[tokio::test]
async fn output_limited_done_repair_allows_only_one_authored_report_and_no_automatic_loop() {
    use httpmock::prelude::*;
    for outcome in ["valid","wrong_evidence","mutation","stale"] {
        let server = MockServer::start();
        let (mut app, root) = closeout_app();
        std::fs::write(root.path().join("source.py"), "original content").unwrap();
        for cfg in [&mut app.coder_cfg,&mut app.observer_cfg] {
            cfg.base_url = server.base_url(); cfg.api_key=Some("offline".into()); cfg.timeout_seconds=3;
        }
        fn is_repair(r: &HttpMockRequest) -> bool { String::from_utf8_lossy(r.body.as_deref().unwrap_or_default()).contains("Observer action repair: attempt") }
        let mut incomplete = closeout_args();
        incomplete["acceptance_evidence"].as_array_mut().unwrap().truncate(2);
        let first = server.mock(|when,then| {
            when.method(POST).path("/chat/completions").body_contains("intervention mode").matches(|r| !is_repair(r));
            let payload=json!({"choices":[{"delta":{"content":suggestion_response("done","done",incomplete)},"finish_reason":"stop"}]});
            then.status(200).header("content-type","text/event-stream").body(format!("data: {payload}\n\ndata: [DONE]\n\n"));
        });
        let repair=server.mock(|when,then| {
            when.method(POST).path("/chat/completions").matches(is_repair);
            then.status(200).header("content-type","text/event-stream").body("data: {\"choices\":[{\"delta\":{\"content\":\"\"},\"finish_reason\":\"length\"}]}\n\ndata: [DONE]\n\n");
        });
        let coder=server.mock(|when,then| {
            when.method(POST).path("/chat/completions").body_contains("one report-only request").matches(|r| {
                let body:serde_json::Value=serde_json::from_slice(r.body.as_deref().unwrap()).unwrap();
                body["tools"].as_array().is_some_and(|tools| tools.len()==1 && tools[0]["function"]["name"]=="done")
            });
            let mut args=closeout_args();
            if outcome=="wrong_evidence" { args["acceptance_evidence"].as_array_mut().unwrap().truncate(2); }
            let tool=if outcome=="mutation" { args=json!({"path":"source.py","content":"unsafe mutation"}); "write_file" } else {"done"};
            let content=format!("<impact>\nchanged: source.py and docs/usage.md repaired\nprogress: step 2 exact check passed after both edits\nremaining_gap: authored final report\n</impact>\n<think>\ngoal: Author the final report\nstep: 2\ntool: {tool}\nrisk: unsupported claims\ndoubt: none\nnext: report retained proof\nverify: existing exact check\n</think>");
            let payload=json!({"choices":[{"delta":{"content":content,"tool_calls":[{"index":0,"id":"closeout","function":{"name":tool,"arguments":args.to_string()}}]},"finish_reason":"stop"}]});
            then.status(200).header("content-type","text/event-stream").body(format!("data: {payload}\n\ndata: [DONE]\n\n"));
        });
        let (obs_tx,mut obs_rx)=mpsc::channel(128);
        send_next_action_assist(&mut app,&obs_tx,"last-fail","incomplete_task").await;
        closeout_drain(&mut app,&mut obs_rx,PaneId::Observer).await;
        maybe_observer_action_repair(&mut app,&obs_tx).await;
        closeout_drain(&mut app,&mut obs_rx,PaneId::Observer).await;
        assert!(app.pending_observer_hint.is_none() && app.pending_observer_contract.is_none());
        assert_ne!(app.coder_completed,Some(true));
        if outcome=="stale" { app.coder_session.as_mut().unwrap().messages.push(json!({"role":"user","content":"A new task"})); }
        let (coder_tx,mut coder_rx)=mpsc::channel(256);
        assert_eq!(observer_request::maybe_report_closeout(&mut app,&coder_tx).await,outcome!="stale");
        if outcome!="stale" { closeout_drain(&mut app,&mut coder_rx,PaneId::Coder).await; }
        for _ in 0..3 {
            assert!(!observer_request::maybe_report_closeout(&mut app,&coder_tx).await);
            maybe_observer_action_repair(&mut app,&obs_tx).await;
            if outcome!="stale" { maybe_auto_next_action_assist(&mut app,&obs_tx).await; maybe_auto_observe(&mut app,&obs_tx).await; }
        }
        first.assert_hits(1); repair.assert_hits(1); coder.assert_hits(usize::from(outcome!="stale"));
        assert_eq!(app.coder_completed==Some(true),outcome=="valid","{outcome}: {:?}",app.coder.messages);
        assert_eq!(std::fs::read_to_string(root.path().join("source.py")).unwrap(),"original content");
        let work=serde_json::to_value(app.work_budget.snapshot()).unwrap();
        assert_eq!(work["requests"].as_array().unwrap().len(),if outcome=="stale" {2} else {3});
        assert_eq!(work["requests"][1]["status"],"output_limited");
        assert_eq!(work["requests"][1]["charge"],"task");
        if outcome!="stale" { assert_eq!(work["requests"][2]["purpose"],"report_closeout"); }
    }
}
