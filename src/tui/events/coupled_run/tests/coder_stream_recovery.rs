use super::*;

fn unfinished_tool() -> String {
    let frame = json!({"choices":[{"delta":{"content":"PARTIAL_CODER_SENTINEL","tool_calls":[{
        "index":0,"id":"discarded_call","type":"function",
        "function":{"name":"read_file","arguments":"{\"path\":\"unexecuted.rs\"}"}
    }]},"finish_reason":"tool_calls"}]});
    format!("data: {frame}\n\ndata: {{")
}

#[tokio::test]
async fn coder_stream_reconnect_discards_the_partial_call_before_resuming() {
    let coder = MockServer::start();
    let observer = MockServer::start();
    coder.mock(|when, then| {
        when.method(POST).path("/chat/completions").matches(|r| {
            !String::from_utf8_lossy(r.body.as_deref().unwrap_or_default())
                .contains("Coder stream recovery:")
        });
        then.status(200)
            .header("content-type", "text/event-stream")
            .body(unfinished_tool());
    });
    let recovered = coder.mock(|when, then| {
        when.method(POST)
            .path("/chat/completions")
            .body_contains("Coder stream recovery:")
            .matches(|r| {
                let body = String::from_utf8_lossy(r.body.as_deref().unwrap_or_default());
                !body.contains("PARTIAL_CODER_SENTINEL") && !body.contains("discarded_call")
            });
        let content = "<plan>\ngoal: Repair engine.rs so answer returns 42\nsteps:\n1. Read engine.rs\n2. Patch the answer\n3. Verify and report\nacceptance:\n1. The answer is 42\nrisks: preserve all checks\nassumptions: engine.rs exists\n</plan>\n<think>\ngoal: inspect engine.rs\nstep: 1\ntool: read_file\nrisk: none\ndoubt: current implementation unknown\nnext: read engine.rs\nverify: cargo test\n</think>";
        let frame = json!({"choices":[{"delta":{"content":content,"tool_calls":[{
            "index":0,"id":"fresh_read","type":"function",
            "function":{"name":"read_file","arguments":"{\"path\":\"engine.rs\"}"}
        }]},"finish_reason":"tool_calls"}]});
        then.status(200)
            .header("content-type", "text/event-stream")
            .body(format!("data: {frame}\n\ndata: [DONE]\n\n"));
    });
    let report = budget_rhythm::episode(&coder, &observer, 6, 2, 4).await;
    recovered.assert_hits(1);
    let requests = report["work"]["requests"].as_array().unwrap();
    assert_eq!(requests[0]["stream_interruption"], "missing_terminal_frame");
    assert_eq!(requests[1]["purpose"], "coder_reconnect");
    assert_eq!(requests[1]["retry_of"], requests[0]["id"]);
    assert_eq!(requests[1]["status"], "completed");
    assert!(requests[..2]
        .iter()
        .all(|r| r["role"] == "coder" && r["charge"] == "task"));
    let actions = report["execution"]["actions"].as_array().unwrap();
    assert!(
        actions
            .iter()
            .any(|a| a["call_id"] == "fresh_read" && a["executed"] == true),
        "{actions:?}"
    );
    assert!(!actions.iter().any(|a| a["call_id"] == "discarded_call"));
}

#[tokio::test]
async fn coder_stream_reconnect_stops_on_second_loss_or_exhausted_slice() {
    for (cap, reserved, expected) in [(6, 2, 2), (6, 3, 1)] {
        let coder = MockServer::start();
        let observer = MockServer::start();
        let broken = coder.mock(|when, then| {
            when.method(POST).path("/chat/completions");
            then.status(200)
                .header("content-type", "text/event-stream")
                .body(unfinished_tool());
        });
        let report = budget_rhythm::episode(&coder, &observer, cap, reserved, 4).await;
        broken.assert_hits(expected);
        assert_eq!(report["termination"], "interrupted");
        assert!(report["execution"]["actions"]
            .as_array()
            .unwrap()
            .is_empty());
        let requests = report["work"]["requests"].as_array().unwrap();
        assert_eq!(requests.len(), expected);
        assert!(requests
            .iter()
            .all(|r| r["status"] == "stream_interrupted" && r["charge"] == "task"));
    }
}

#[tokio::test]
async fn a_lost_last_delivery_carries_only_saved_work_into_the_next_iteration() {
    use super::iterations::{greeting_seed, response};
    use serde_json::Value;
    let server = MockServer::start();
    let edited = server.mock(|when,then| {
        when.method(POST).path("/chat/completions").matches(|r|{
            let v:Value=serde_json::from_slice(r.body.as_deref().unwrap()).unwrap();
            v["tools"].is_array() && !v.to_string().contains("boundary-edit")
        });
        let thought = "<evidence>\ntarget_files: 1) guide.md\ntarget_symbols: 1) greeting\nevidence: The retained read shows old in guide.md.\nopen_questions: none\nnext_probe: patch guide.md\n</evidence>\n<think>\ngoal: Update the greeting in guide.md\nstep: 1\ntool: patch_file\nrisk: stale anchor\ndoubt: none\nnext: replace the old greeting\nverify: inspect guide.md\n</think>";
        then.status(200).header("content-type","text/event-stream").body(response("patch_file","boundary-edit",json!({"path":"guide.md","search":"old","replace":"hello"}),thought));
    });
    let lost = server.mock(|when, then| {
        when.method(POST)
            .path("/chat/completions")
            .body_contains("boundary-edit")
            .matches(|r| {
                let v: Value = serde_json::from_slice(r.body.as_deref().unwrap()).unwrap();
                v["tools"].is_array() && !v.to_string().contains("[New iteration allowance]")
            });
        then.status(200)
            .header("content-type", "text/event-stream")
            .body(unfinished_tool());
    });
    let resumed = server.mock(|when, then| {
        when.method(POST)
            .path("/chat/completions")
            .body_contains("[New iteration allowance]")
            .body_contains("boundary-edit")
            .matches(|r| {
                let v: Value = serde_json::from_slice(r.body.as_deref().unwrap()).unwrap();
                v["tools"].is_array() && !v.to_string().contains("discarded_call")
            });
        then.status(200)
            .header("content-type", "text/event-stream")
            .body(response(
                "",
                "",
                json!({}),
                "Verification is still pending.",
            ));
    });
    server.mock(|when,then|{
        when.method(POST).path("/chat/completions").matches(|r|{
            let v:Value=serde_json::from_slice(r.body.as_deref().unwrap()).unwrap(); !v["tools"].is_array()
        });
        let advice=json!({"summary":"Verify the greeting.","suggestions":[{"kind":"clarify","reason":"Verify the changed greeting.","confidence":0.9,"suggested_tool":null,"suggested_args":{}}]});
        then.status(200).header("content-type","text/event-stream").body(response("","",json!({}),&advice.to_string()));
    });
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("project");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("guide.md"), "old\n").unwrap();
    let task = "Update the greeting in guide.md from old to hello.";
    let prompt = dir.path().join("task.txt");
    std::fs::write(&prompt, task).unwrap();
    let seed = dir.path().join("seed.json");
    greeting_seed(&seed, task);
    let out = dir.path().join("chain");
    let cli = crate::Cli::try_parse_from([
        "spiral-coder",
        "--provider",
        "openai",
        "--base-url",
        &server.base_url(),
        "--model",
        "offline",
        "--api-key",
        "offline",
        "coupled-run",
        "--tool-root",
        root.to_str().unwrap(),
        "--out-dir",
        out.to_str().unwrap(),
        "--prompt-file",
        prompt.to_str().unwrap(),
        "--seed-session",
        seed.to_str().unwrap(),
        "--max-requests",
        "2",
        "--iterations",
        "2",
    ])
    .unwrap();
    let Some(crate::Command::CoupledRun(args)) = cli.command else {
        panic!("expected coupled-run")
    };
    let outcome = run(args, cli.common).await;
    let manifest: Value =
        serde_json::from_slice(&std::fs::read(out.join("iterations.json")).unwrap()).unwrap();
    assert!(outcome.is_ok(), "{outcome:?} {manifest}");
    let rows = manifest["iterations"].as_array().unwrap();
    assert_eq!(rows.len(), 2, "{manifest}");
    assert_eq!(rows[0]["termination"], "interrupted");
    assert_eq!(rows[0]["task_requests"], 2);
    assert_eq!(rows[0]["continued_after_coder_stream_request"], 1);
    assert_eq!(rows[1]["parent_task_id"], rows[0]["task_id"]);
    assert_ne!(rows[1]["task_id"], rows[0]["task_id"]);
    assert_eq!(
        std::fs::read_to_string(root.join("guide.md")).unwrap(),
        "hello\n"
    );
    assert!(
        !std::fs::read_to_string(out.join("iteration-01/session.json"))
            .unwrap()
            .contains("discarded_call")
    );
    edited.assert_hits(1);
    lost.assert_hits(1);
    assert!(resumed.hits() > 0);
}
