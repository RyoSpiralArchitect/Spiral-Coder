use super::*;

const QUOTA: &str = r#"{"error":{"type":"insufficient_quota","code":"credit_balance_exhausted"}}"#;

#[tokio::test]
async fn coder_quota_failure_spends_one_request_and_retains_the_provider_reason() {
    let coder = MockServer::start();
    let observer = MockServer::start();
    let failed = coder.mock(|when, then| {
        when.method(POST).path("/chat/completions");
        then.status(429).body(QUOTA);
    });
    let report = budget_rhythm::episode(&coder, &observer, 6, 2, 4).await;
    failed.assert_hits(1);
    assert_eq!(report["role_requests"], json!({"coder":1,"observer":0}));
    assert_eq!(report["termination"], "interrupted");
    assert_eq!(report["completed"], false);
    assert!(report["error"]
        .as_str()
        .unwrap()
        .contains("credit_balance_exhausted"));
    assert_eq!(report["work"]["requests"][0]["status"], "http_429");
    assert_eq!(report["work"]["requests"][0]["charge"], "task");
}

#[tokio::test]
async fn observer_quota_failure_spends_one_request_and_never_becomes_transport_credit() {
    let coder = MockServer::start();
    let observer = MockServer::start();
    coder.mock(|when, then| {
        when.method(POST).path("/chat/completions");
        then.status(200).header("content-type", "text/event-stream").body(
            "data: {\"choices\":[{\"delta\":{\"content\":\"The repair is unfinished.\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n",
        );
    });
    let failed = observer.mock(|when, then| {
        when.method(POST).path("/chat/completions");
        then.status(429).body(QUOTA);
    });
    let report = budget_rhythm::episode(&coder, &observer, 6, 2, 4).await;
    failed.assert_hits(1);
    assert_eq!(report["role_requests"]["observer"], 1);
    assert_eq!(report["termination"], "interrupted");
    assert_eq!(report["completed"], false);
    assert!(report["error"]
        .as_str()
        .unwrap()
        .contains("credit_balance_exhausted"));
    assert!(report["work"]["requests"]
        .as_array()
        .unwrap()
        .iter()
        .all(|r| r["charge"] == "task"));
}

#[tokio::test]
async fn temporary_rate_limit_keeps_its_cause_when_the_coder_slice_expires() {
    let coder = MockServer::start();
    let observer = MockServer::start();
    let failed = coder.mock(|when, then| {
        when.method(POST).path("/chat/completions");
        then.status(429)
            .body(r#"{"error":{"code":"rate_limit_exceeded"}}"#);
    });
    let report = budget_rhythm::episode(&coder, &observer, 6, 2, 4).await;
    failed.assert_hits(2);
    let error = report["error"].as_str().unwrap();
    assert!(error.contains("HTTP 429"), "{error}");
    assert!(error.contains("rate_limit_exceeded"), "{error}");
    assert!(error.contains("chunk exhausted"), "{error}");
    assert_eq!(report["completed"], false);
    assert!(report["work"]["requests"]
        .as_array()
        .unwrap()
        .iter()
        .all(|r| r["charge"] == "task"));
}
