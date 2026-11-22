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
