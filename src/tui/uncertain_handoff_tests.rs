#[tokio::test]
async fn uncertain_observer_primary_returns_one_report_without_adopting_advice() {
    use httpmock::prelude::*;
    let fixture: serde_json::Value = serde_json::from_str(include_str!("../../tests/fixtures/observer-report-closeout/low-confidence.json")).unwrap();
    for outcome in ["blocked", "unsupported_done", "mutation", "stale"] {
        let server = MockServer::start();
        let (mut app, root) = closeout_app();
        std::fs::write(root.path().join("source.py"), "original").unwrap();
        for cfg in [&mut app.coder_cfg, &mut app.observer_cfg] {
            cfg.base_url = server.base_url(); cfg.api_key = Some("offline".into()); cfg.timeout_seconds = 3;
        }
        // Declared but unfinished work: the last successful exact check is absent.
        app.coder_session.as_mut().unwrap().messages.truncate(6);
        let observer = server.mock(|when, then| {
            when.method(POST).path("/chat/completions").matches(|r| !serde_json::from_slice::<serde_json::Value>(r.body.as_deref().unwrap()).unwrap()["tools"].is_array());
            let reply=json!({"choices":[{"delta":{"content":fixture["raw"]},"finish_reason":"stop"}]});
            then.status(200).header("content-type","text/event-stream").body(format!("data: {reply}\n\ndata: [DONE]\n\n"));
        });
        let coder = server.mock(|when, then| {
            when.method(POST).path("/chat/completions").body_contains("one report-only request").matches(|r| {
                let body:serde_json::Value=serde_json::from_slice(r.body.as_deref().unwrap()).unwrap();
                body["tools"].as_array().is_some_and(|t| t.len()==1 && t[0]["function"]["name"]=="done")
                    && !body.to_string().contains("public_cli_rejects_wrong_parent_linkage")
            });
            let mut args=json!({"summary":"handoff-ready. Verification remains unfinished.","completed_acceptance":[],"remaining_acceptance":["acceptance 1","acceptance 2","acceptance 3","acceptance 4"],"acceptance_evidence":[],"next_steps":"Run the declared Python check on source.py and docs/usage.md."});
            if outcome=="unsupported_done" { args=closeout_args(); }
            let tool=if outcome=="mutation" {args=json!({"path":"source.py","content":"unauthorized advice"});"write_file"} else {"done"};
            let reply=json!({"choices":[{"delta":{"content":format!("<think>\ngoal: Report unfinished work\nstep: 2\ntool: {tool}\nrisk: unsupported claims\ndoubt: current check missing\nnext: hand off the exact check\nverify: retained native receipts\n</think>"),"tool_calls":[{"index":0,"id":"handoff","function":{"name":tool,"arguments":args.to_string()}}]},"finish_reason":"stop"}]});
            then.status(200).header("content-type","text/event-stream").body(format!("data: {reply}\n\ndata: [DONE]\n\n"));
        });
        let (obs_tx,mut obs_rx)=mpsc::channel(128);
        send_next_action_assist(&mut app,&obs_tx,"last-fail","incomplete_task").await;
        closeout_drain(&mut app,&mut obs_rx,PaneId::Observer).await;
        assert!(app.pending_observer_hint.is_none() && app.pending_observer_advice.is_none());
        if outcome=="stale" { app.coder_session.as_mut().unwrap().messages.push(json!({"role":"user","content":"A different task"})); }
        let (tx,mut rx)=mpsc::channel(128);
        assert_eq!(observer_request::maybe_report_closeout(&mut app,&tx).await,outcome!="stale", "{outcome}");
        if outcome!="stale" { closeout_drain(&mut app,&mut rx,PaneId::Coder).await; }
        assert!(!observer_request::maybe_report_closeout(&mut app,&tx).await);
        observer.assert_hits(1); coder.assert_hits(usize::from(outcome!="stale"));
        assert_ne!(app.coder_completed,Some(true));
        assert_eq!(app.coder_yield_reason==Some(crate::tui::coder_yield::CoderYieldReason::Blocked),outcome=="blocked");
        assert_eq!(std::fs::read_to_string(root.path().join("source.py")).unwrap(),"original");
        let work=app.work_budget.snapshot();
        assert_eq!(work.requests.len(),if outcome=="stale" {1} else {2});
        assert!(work.assists.is_empty());
        if outcome!="stale" { assert_eq!(serde_json::to_value(&work.requests[1]).unwrap()["purpose"],"report_closeout"); }
    }
}
