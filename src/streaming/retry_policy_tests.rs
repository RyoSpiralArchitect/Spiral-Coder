use super::*;
use clap::Parser;
use httpmock::prelude::*;

#[tokio::test]
async fn completions_fallback_stops_on_explicit_quota_exhaustion() {
    let server = MockServer::start();
    let chat = server.mock(|when, then| {
        when.method(POST).path("/chat/completions");
        then.status(400).body("not a chat model");
    });
    let completions = server.mock(|when, then| {
        when.method(POST).path("/completions");
        then.status(429)
            .body(r#"{"error":{"code":"credit_balance_exhausted"}}"#);
    });
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
    ])
    .unwrap();
    let cfg = cli.common.to_partial_config().resolve().unwrap();
    let budget = crate::work_budget::WorkBudget::new("task", 8, None);
    let (tx, _rx) = mpsc::channel(32);
    let error = crate::work_budget::scope(
        Some(crate::work_budget::Context {
            budget: budget.clone(),
            role: "coder",
        }),
        stream_openai_compat_json(
            &reqwest::Client::new(),
            &cfg,
            &[json!({"role":"user","content":"test"})],
            None,
            tx,
        ),
    )
    .await
    .unwrap_err();
    chat.assert_hits(1);
    completions.assert_hits(1);
    assert!(error.to_string().contains("credit_balance_exhausted"));
    let rows = budget.snapshot().requests;
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].status, "http_400");
    assert_eq!(rows[1].status, "http_429");
    assert!(rows
        .iter()
        .all(|r| r.charge == crate::work_budget::RequestCharge::Task));
}
