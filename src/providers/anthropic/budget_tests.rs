use super::*;
use crate::work_budget::{self, WorkBudget};
use httpmock::prelude::*;

async fn chat_with_budget(server: &MockServer, budget: &WorkBudget) -> Result<ChatResponse> {
    let provider = AnthropicProvider::new(
        reqwest::Client::new(),
        "test-model".into(),
        Some("test-key".into()),
        server.base_url(),
        Duration::from_secs(5),
    );
    work_budget::scope(
        Some(work_budget::Context {
            budget: budget.clone(),
            role: "observer",
        }),
        provider.chat(&ChatRequest {
            messages: vec![ChatMessage {
                role: "user".into(),
                content: "Explain the failure".into(),
            }],
            temperature: None,
            max_tokens: Some(64),
            metadata: None,
        }),
    )
    .await
}

#[tokio::test]
async fn observer_usage_and_duration_survive_reload_and_prevent_another_send() {
    let server = MockServer::start();
    let usage = json!({"input_tokens": 12, "output_tokens": 7});
    let wire = server.mock(|when, then| {
        when.method(POST).path("/messages");
        then.status(200).json_body(json!({
            "content": [{"type": "text", "text": "Read the failed assertion."}],
            "usage": usage,
        }));
    });
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("work.json");
    let budget = WorkBudget::new("task", 1, Some(path.clone()));
    assert_eq!(
        chat_with_budget(&server, &budget).await.unwrap().content,
        "Read the failed assertion."
    );
    let restored = WorkBudget::load("task", path).unwrap();
    assert_eq!(restored.remaining(), 0);
    let requests = restored.snapshot().requests;
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].role, "observer");
    assert_eq!(requests[0].status, "completed");
    assert_eq!(requests[0].provider_usage.as_ref(), Some(&usage));
    assert!(requests[0].estimated_input_tokens > 0);
    assert!(requests[0].duration_ms.is_some());
    assert!(chat_with_budget(&server, &restored)
        .await
        .unwrap_err()
        .to_string()
        .contains("budget exhausted"));
    wire.assert_hits(1);
}

#[tokio::test]
async fn exhausted_observer_budget_prevents_network_io() {
    let server = MockServer::start();
    let wire = server.mock(|when, then| {
        when.method(POST).path("/messages");
        then.status(200).json_body(json!({"content": []}));
    });
    let budget = WorkBudget::new("task", 0, None);
    assert!(chat_with_budget(&server, &budget)
        .await
        .unwrap_err()
        .to_string()
        .contains("budget exhausted"));
    wire.assert_hits(0);
    assert!(budget.snapshot().requests.is_empty());
}

#[tokio::test]
async fn failed_http_attempt_is_spent_with_unknown_usage() {
    let server = MockServer::start();
    let wire = server.mock(|when, then| {
        when.method(POST).path("/messages");
        then.status(429).body("rate limited");
    });
    let budget = WorkBudget::new("task", 1, None);
    assert!(chat_with_budget(&server, &budget)
        .await
        .unwrap_err()
        .to_string()
        .contains("rate limited"));
    assert_eq!(budget.remaining(), 0);
    let requests = budget.snapshot().requests;
    assert_eq!(requests[0].status, "http_429");
    assert_eq!(requests[0].provider_usage, None);
    assert!(requests[0].duration_ms.is_some());
    wire.assert_hits(1);
}

#[tokio::test]
async fn invalid_json_attempt_remains_spent() {
    let server = MockServer::start();
    let wire = server.mock(|when, then| {
        when.method(POST).path("/messages");
        then.status(200).body("invalid json");
    });
    let budget = WorkBudget::new("task", 1, None);
    assert!(chat_with_budget(&server, &budget)
        .await
        .unwrap_err()
        .to_string()
        .contains("invalid JSON"));
    assert_eq!(budget.remaining(), 0);
    let requests = budget.snapshot().requests;
    assert_eq!(requests[0].status, "http_200");
    assert_eq!(requests[0].provider_usage, None);
    assert!(requests[0].duration_ms.is_some());
    wire.assert_hits(1);
}
