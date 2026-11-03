use super::*;
use crate::agent_session::AgentSession;
use serde_json::Value;

fn response(tool: &str, id: &str, args: Value, text: &str) -> String {
    let mut delta = json!({"content":text});
    if !tool.is_empty() {
        delta["tool_calls"] = json!([{"index":0,"id":id,"type":"function","function":{"name":tool,"arguments":args.to_string()}}]);
    }
    format!(
        "data: {}\n\ndata: [DONE]\n\n",
        json!({"choices":[{"delta":delta,"finish_reason":"stop"}]})
    )
}

#[tokio::test]
async fn a_new_iteration_keeps_the_edit_and_validated_observer_note_but_owns_its_budget() {
    let server = MockServer::start();
    let coder_first = server.mock(|when,then| {
        when.method(POST).path("/chat/completions").matches(|r| {
            let v:Value=serde_json::from_slice(r.body.as_deref().unwrap_or_default()).unwrap();
            v["tools"].is_array() && !v.to_string().contains("observed-edit") && !v.to_string().contains("carry-greeting-forward")
        });
        then.status(200).header("content-type","text/event-stream").body(response("patch_file","observed-edit",json!({"path":"guide.md","search":"old","replace":"hello"}),"<evidence>\ntarget_files: 1) guide.md\ntarget_symbols: 1) greeting\nevidence: The retained read shows old in guide.md.\nopen_questions: none\nnext_probe: patch guide.md\n</evidence>\n<think>\ngoal: Update the greeting in guide.md\nstep: 1\ntool: patch_file\nrisk: stale anchor\ndoubt: none\nnext: replace the old greeting\nverify: inspect guide.md\n</think>"));
    });
    let resumed = server.mock(|when, then| {
        when.method(POST).path("/chat/completions").matches(|r| {
            let v: Value = serde_json::from_slice(r.body.as_deref().unwrap_or_default()).unwrap();
            v["tools"].is_array()
                && v.to_string().contains("observed-edit")
                && v.to_string().contains("carry-greeting-forward")
        });
        then.status(200)
            .header("content-type", "text/event-stream")
            .body(response(
                "",
                "",
                json!({}),
                "The current proof is incomplete.",
            ));
    });
    let observer=server.mock(|when,then| {
        when.method(POST).path("/chat/completions").body_contains("Write a handoff");
        then.status(200).header("content-type","text/event-stream").body(response("","",json!({}),
            &json!({"summary":"The edit was recorded.","suggestions":[{"kind":"clarify","suggested_tool":null,"suggested_args":{},"reason":"carry-greeting-forward","confidence":0.9}]}).to_string()));
    });
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("project");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("guide.md"), "old\n").unwrap();
    let task = "Update the greeting in guide.md from old to hello.";
    let prompt = dir.path().join("task.txt");
    std::fs::write(&prompt, task).unwrap();
    let seed = dir.path().join("seed.json");
    AgentSession::save_atomic(&seed,&AgentSession::new(None,None,None,None,vec![
        json!({"role":"user","content":task}),json!({"role":"assistant","accepted_plan":{"goal":"Update the greeting in guide.md","steps":["Edit the greeting","Report the result"],"acceptance_criteria":["guide.md greets the reader"],"risks":"incorrect anchor","assumptions":"guide.md exists"},"tool_calls":[{"id":"prior-read","type":"function","function":{"name":"read_file","arguments":r#"{"path":"guide.md"}"#}}]}),
        json!({"role":"tool","tool_call_id":"prior-read","content":"[guide.md] (1 lines, 4 bytes)\nold\n"})])).unwrap();
    let output = dir.path().join("chain");
    let cli = crate::Cli::try_parse_from([
        "spiral-coder",
        "--provider",
        "openai",
        "--model",
        "offline",
        "--base-url",
        &server.base_url(),
        "--api-key",
        "offline",
        "--timeout-seconds",
        "3",
        "coupled-run",
        "--tool-root",
        root.to_str().unwrap(),
        "--prompt-file",
        prompt.to_str().unwrap(),
        "--seed-session",
        seed.to_str().unwrap(),
        "--out-dir",
        output.to_str().unwrap(),
        "--max-requests",
        "2",
        "--observer-reserved-requests",
        "1",
        "--iterations",
        "3",
    ])
    .unwrap();
    let Some(crate::Command::CoupledRun(args)) = cli.command else {
        panic!("coupled-run")
    };
    let result = run(args, cli.common).await;
    let manifest: Value =
        serde_json::from_slice(&std::fs::read(output.join("iterations.json")).unwrap()).unwrap();
    assert!(result.is_ok(), "{result:?} {manifest}");
    assert_eq!(
        manifest["iterations"].as_array().unwrap().len(),
        2,
        "{manifest}"
    );
    assert_eq!(manifest["stop_reason"], "no_new_edit_or_verification");
    assert_eq!(manifest["wire_requests"], 4);
    assert_eq!(
        manifest["iterations"][1]["parent_task_id"],
        manifest["iterations"][0]["task_id"]
    );
    assert_ne!(
        manifest["iterations"][1]["task_id"],
        manifest["iterations"][0]["task_id"]
    );
    assert_eq!(
        std::fs::read_to_string(root.join("guide.md")).unwrap(),
        "hello\n"
    );
    coder_first.assert_hits(1);
    resumed.assert_hits(1);
    observer.assert_hits(2);
}

#[test]
fn final_shared_request_can_be_a_report_only_after_observer_participation() {
    let (mut app, _dir) = crate::tui::events::tests::interaction_app();
    let messages = crate::tui::events::tests::closeout_messages();
    let task = crate::task_origin::root_user_text(&messages).to_owned();
    app.coder_session = Some(AgentSession::new(None, None, None, None, messages));
    app.work_budget = crate::work_budget::WorkBudget::new(&task, 3, None);
    assert!(!super::super::iterations::last_coder_report(&app));
    for role in ["coder", "observer"] {
        let request = app.work_budget.begin(role, &json!({})).unwrap();
        request.status("completed");
    }
    assert!(super::super::iterations::last_coder_report(&app));
    app.work_budget = crate::work_budget::WorkBudget::new(&task, 1, None);
    assert!(!super::super::iterations::last_coder_report(&app));
}
