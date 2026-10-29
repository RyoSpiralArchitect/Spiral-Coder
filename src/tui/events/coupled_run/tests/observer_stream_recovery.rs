use super::*;

fn sse(text: &str, terminal: bool) -> String {
    let payload = json!({"choices":[{"delta":{"content":text},"finish_reason":if terminal {json!("stop")} else {serde_json::Value::Null}}]});
    format!(
        "data: {payload}\n\n{}",
        if terminal { "data: [DONE]\n\n" } else { "" }
    )
}

#[tokio::test]
async fn interrupted_observer_reconnects_once_without_adopting_the_partial_response() {
    let coder = MockServer::start();
    let observer = MockServer::start();
    coder.mock(|when, then| {
        when.method(POST).path("/chat/completions");
        then.status(200)
            .header("content-type", "text/event-stream")
            .body(sse("The repair remains unfinished.", true));
    });
    let fragment: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../docs/evals/2026-10-29-observer-closeout/interruption-r8.json"
    ))
    .unwrap();
    let first = observer.mock(|when, then| {
        when.method(POST).path("/chat/completions").matches(|r| {
            !String::from_utf8_lossy(r.body.as_deref().unwrap_or_default())
                .contains("Observer stream recovery: attempt 1 of 1")
        });
        then.status(200)
            .header("content-type", "text/event-stream")
            .body(sse(
                fragment["partial_observer_content"].as_str().unwrap(),
                false,
            ));
    });
    let recovered = observer.mock(|when, then| {
        when.method(POST).path("/chat/completions").body_contains("Observer stream recovery: attempt 1 of 1").matches(|r| {
            let body = String::from_utf8_lossy(r.body.as_deref().unwrap_or_default());
            let payload: serde_json::Value = serde_json::from_str(&body).unwrap();
            let prompt = payload["messages"].as_array().unwrap().last().unwrap()["content"].as_str().unwrap();
            let allowance: serde_json::Value = serde_json::from_str(prompt.lines().last().unwrap()).unwrap();
            !body.contains("parse_bool") && allowance == json!({
                "task_request_limit":8,"task_requests_spent":4,
                "remaining_task_requests_before_this_request":4,
                "remaining_coder_requests_before_this_request":3,
                "reconnect_wire_limit":1
            })
        });
        let raw=json!({"summary":"The repair is unverified.","suggestions":[{"kind":"clarify","reason":"Need a fresh source read.","confidence":0.4,"suggested_tool":null,"suggested_args":{}}]}).to_string();
        then.status(200).header("content-type","text/event-stream").body(sse(&raw,true));
    });
    let report = budget_rhythm::episode(&coder, &observer, 8, 2, 4).await;
    first.assert_hits(1);
    recovered.assert_hits(1);
    assert_eq!(report["completed"], false);
    assert_eq!(report["error"], serde_json::Value::Null);
    assert_eq!(report["termination"], "observer_unavailable");
    assert_eq!(report["role_requests"]["observer"], 2);
    let requests = report["work"]["requests"].as_array().unwrap();
    let interrupted = requests
        .iter()
        .find(|r| r["status"] == "stream_interrupted")
        .unwrap();
    assert_eq!(interrupted["charge"], "task");
    assert_eq!(interrupted["stream_interruption"], "missing_terminal_frame");
    let last = requests.last().unwrap();
    assert_eq!(last["purpose"], "observer_reconnect");
    assert_eq!(last["retry_of"], interrupted["id"]);
    assert_eq!(last["charge"], "task");
    assert!(report["observer_display"]
        .as_array()
        .unwrap()
        .iter()
        .any(|m| m["content"] == fragment["partial_observer_content"]));
    assert!(report["work"]["assists"].as_array().unwrap().is_empty());
}

fn unfinished_coder(server: &MockServer) {
    server.mock(|when, then| {
        when.method(POST).path("/chat/completions");
        then.status(200)
            .header("content-type", "text/event-stream")
            .body(sse("The repair remains unfinished.", true));
    });
}

fn uncertain() -> String {
    json!({"summary":"Unverified", "suggestions":[{"kind":"clarify","reason":"Need evidence","confidence":0.3,"suggested_tool":null,"suggested_args":{}}]}).to_string()
}

#[tokio::test]
async fn observer_stream_recovery_stops_after_a_second_interruption() {
    let coder = MockServer::start();
    let observer = MockServer::start();
    unfinished_coder(&coder);
    let failed = observer.mock(|when, then| {
        when.method(POST).path("/chat/completions");
        then.status(200)
            .header("content-type", "text/event-stream")
            .body(sse(&uncertain(), false));
    });
    let report = budget_rhythm::episode(&coder, &observer, 8, 2, 4).await;
    failed.assert_hits(2);
    assert_eq!(report["completed"], false);
    assert_eq!(report["termination"], "interrupted");
    assert!(report["error"]
        .as_str()
        .unwrap()
        .contains("before a terminal frame"));
    assert!(report["work"]["assists"].as_array().unwrap().is_empty());
    let requests = report["work"]["requests"].as_array().unwrap();
    assert!(requests.iter().all(|r| r["charge"] == "task"));
    assert_eq!(requests.last().unwrap()["purpose"], "observer_reconnect");
}

#[tokio::test]
async fn observer_stream_recovery_excludes_auth_quota_and_protocol_errors() {
    for (status, body) in [
        (401, "invalid API key".to_owned()),
        (403, "forbidden".to_owned()),
        (429, r#"{"error":{"code":"insufficient_quota"}}"#.to_owned()),
        (
            200,
            "data: {\"error\":\"stream ended before a terminal frame\"}\n\n".to_owned(),
        ),
        (
            200,
            format!(
                "data: {}\n\n",
                json!({"choices":[{"delta":{"tool_calls":[
                    {"index":0,"id":"a","function":{"name":"read_file","arguments":"{}"}},
                    {"index":0,"id":"b","function":{"name":"read_file","arguments":"{}"}}
                ]}}]})
            ),
        ),
    ] {
        let coder = MockServer::start();
        let observer = MockServer::start();
        unfinished_coder(&coder);
        let failed = observer.mock(|when, then| {
            when.method(POST).path("/chat/completions");
            then.status(status)
                .header("content-type", "text/event-stream")
                .body(body);
        });
        let report = budget_rhythm::episode(&coder, &observer, 8, 2, 4).await;
        failed.assert_hits(1);
        assert_eq!(report["termination"], "interrupted");
        let receipt = report["work"]["requests"]
            .as_array()
            .unwrap()
            .last()
            .unwrap();
        assert_eq!(receipt["stream_interruption"], serde_json::Value::Null);
        assert_eq!(receipt["purpose"], serde_json::Value::Null);
        assert_eq!(receipt["charge"], "task");
    }
}

#[tokio::test]
async fn interrupted_action_repair_reconnects_with_feedback_but_without_its_fragment() {
    let coder = MockServer::start();
    let observer = MockServer::start();
    unfinished_coder(&coder);
    let first = observer.mock(|when, then| {
        when.method(POST).path("/chat/completions").matches(|r| !String::from_utf8_lossy(r.body.as_deref().unwrap_or_default()).contains("Observer action repair"));
        let raw = json!({"summary":"Check", "suggestions":[{"kind":"verify","reason":"Check","confidence":0.9,"suggested_args":{"command":"cargo test"}}]}).to_string();
        then.status(200).header("content-type", "text/event-stream").body(sse(&raw, true));
    });
    let repair = observer.mock(|when, then| {
        when.method(POST)
            .path("/chat/completions")
            .body_contains("Observer action repair")
            .matches(|r| {
                !String::from_utf8_lossy(r.body.as_deref().unwrap_or_default())
                    .contains("Observer stream recovery")
            });
        then.status(200)
            .header("content-type", "text/event-stream")
            .body(sse("repair-fragment-do-not-forward", false));
    });
    let reconnect = observer.mock(|when, then| {
        when.method(POST)
            .path("/chat/completions")
            .body_contains("Observer stream recovery")
            .body_contains("Observer action repair")
            .body_contains("suggested_tool is required for kind=verify")
            .body_contains("Current request allowance")
            .matches(|r| {
                !String::from_utf8_lossy(r.body.as_deref().unwrap_or_default())
                    .contains("repair-fragment-do-not-forward")
            });
        then.status(200)
            .header("content-type", "text/event-stream")
            .body(sse(&uncertain(), true));
    });
    let report = budget_rhythm::episode(&coder, &observer, 10, 3, 4).await;
    first.assert_hits(1);
    repair.assert_hits(1);
    reconnect.assert_hits(1);
    assert_eq!(report["role_requests"]["observer"], 3);
    assert_eq!(report["error"], serde_json::Value::Null);
    assert_eq!(report["completed"], false);
}

#[tokio::test]
async fn reconnected_response_gets_at_most_the_original_action_repair() {
    for repair_completes in [false, true] {
        let coder = MockServer::start();
        let observer = MockServer::start();
        unfinished_coder(&coder);
        let first = observer.mock(|when, then| {
            when.method(POST).path("/chat/completions").matches(|r| {
                let body = String::from_utf8_lossy(r.body.as_deref().unwrap_or_default());
                !body.contains("Observer stream recovery")
                    && !body.contains("Observer action repair")
            });
            then.status(200)
                .header("content-type", "text/event-stream")
                .body(sse("unfinished-delivery", false));
        });
        let reconnect = observer.mock(|when, then| {
            when.method(POST)
                .path("/chat/completions")
                .body_contains("Observer stream recovery");
            then.status(200)
                .header("content-type", "text/event-stream")
                .body(sse("invalid-envelope", true));
        });
        let repair = observer.mock(|when, then| {
            when.method(POST)
                .path("/chat/completions")
                .body_contains("Observer action repair");
            then.status(200)
                .header("content-type", "text/event-stream")
                .body(sse(&uncertain(), repair_completes));
        });
        let report = budget_rhythm::episode(&coder, &observer, 10, 3, 4).await;
        first.assert_hits(1);
        reconnect.assert_hits(1);
        repair.assert_hits(1);
        assert_eq!(report["role_requests"]["observer"], 3);
        assert_eq!(report["error"].is_null(), repair_completes);
        assert_eq!(report["completed"], false);
        let requests = report["work"]["requests"].as_array().unwrap();
        assert_eq!(
            requests
                .iter()
                .filter(|r| r["purpose"] == "observer_reconnect")
                .count(),
            1
        );
        // The interrupted request is never refunded; a separate complete JSON
        // rejection remains subject to the pre-existing transport-pool rule.
        assert_eq!(
            requests
                .iter()
                .find(|r| r["status"] == "stream_interrupted")
                .unwrap()["charge"],
            "task"
        );
    }
}

#[tokio::test]
async fn reconnect_cannot_expand_into_a_provider_compatibility_retry() {
    let coder = MockServer::start();
    let observer = MockServer::start();
    unfinished_coder(&coder);
    let first = observer.mock(|when, then| {
        when.method(POST).path("/chat/completions").matches(|r| {
            !String::from_utf8_lossy(r.body.as_deref().unwrap_or_default())
                .contains("Observer stream recovery")
        });
        then.status(200)
            .header("content-type", "text/event-stream")
            .body(sse("unfinished", false));
    });
    let compatibility = observer.mock(|when, then| {
        when.method(POST)
            .path("/chat/completions")
            .body_contains("Observer stream recovery");
        then.status(400)
            .body("Use max_completion_tokens instead of max_tokens");
    });
    let report = budget_rhythm::episode(&coder, &observer, 8, 2, 4).await;
    first.assert_hits(1);
    compatibility.assert_hits(1);
    assert_eq!(report["role_requests"]["observer"], 2);
    assert_eq!(report["termination"], "interrupted");
    assert_eq!(
        report["work"]["requests"]
            .as_array()
            .unwrap()
            .last()
            .unwrap()["status"],
        "http_400"
    );
}
