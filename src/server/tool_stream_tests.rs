//! Both frontends must retain opaque tool metadata from the same provider stream.
use super::*;
use crate::streaming::{stream_openai_compat_json, StreamToken};
use httpmock::prelude::*;
use serde_json::{json, Value};

#[tokio::test]
async fn exhausted_retry_returns_budget_error_without_another_wire_request() {
    check_one_wire_attempt(true, false).await;
    check_one_wire_attempt(false, false).await;
    check_one_wire_attempt(false, true).await;
}

async fn check_one_wire_attempt(streamed: bool, completion_fallback: bool) {
    let provider = MockServer::start();
    let mock = provider.mock(|when, then| {
        when.method(POST).path("/chat/completions");
        then.status(400).body(if completion_fallback {
            "This is not a chat model and thus not supported in the v1/chat/completions endpoint. Did you mean to use v1/completions?"
        } else { "unsupported tool_choice" });
    });
    let completions = provider.mock(|when, then| {
        when.method(POST).path("/completions");
        then.status(200).body("{}");
    });
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let body = serde_json::to_vec(&json!({"messages":[],"model":"mock",
        "base_url":provider.base_url(),"tools":[{"type":"function","function":{"name":"exec"}}]}))
    .unwrap();
    let budget = crate::work_budget::WorkBudget::new("task", 1, None);
    let proxy = tokio::spawn(crate::work_budget::scope(
        Some(crate::work_budget::Context {
            budget: budget.clone(),
            role: "coder",
        }),
        async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let state = AppState {
                client: reqwest::Client::new(),
                bounded_client: bounded_provider_client().unwrap(),
                defaults: PartialConfig::default(),
                pending_edits: crate::pending_edits::PendingEditStore::new(),
                pending_commands: crate::pending_commands::PendingCommandStore::new(),
                workspace_root: PathBuf::new(),
            };
            if streamed {
                api_chat_tools_stream(&mut socket, state, &body)
                    .await
                    .unwrap();
            } else {
                api_chat_tools(&mut socket, state, &body).await.unwrap();
            }
        },
    ));
    let mut client = TcpStream::connect(address).await.unwrap();
    let mut wire = String::new();
    tokio::time::timeout(Duration::from_secs(5), client.read_to_string(&mut wire))
        .await
        .unwrap()
        .unwrap();
    proxy.await.unwrap();
    assert!(wire.starts_with("HTTP/1.1 409"), "{wire}");
    assert!(
        wire.contains("Shared task request budget exhausted"),
        "{wire}"
    );
    assert_eq!(budget.snapshot().requests.len(), 1);
    mock.assert_hits(1);
    completions.assert_hits(0);
    if !streamed {
        let row = &budget.snapshot().requests[0];
        assert_eq!(row.status, "http_400");
        assert!(row.duration_ms.is_some());
    }
}

#[tokio::test]
async fn a_bounded_web_post_cannot_follow_an_unmetered_redirect() {
    let provider = MockServer::start();
    let target = provider.mock(|when, then| {
        when.method(POST).path("/other");
        then.status(200).json_body(json!({}));
    });
    let redirect = provider.mock(|when, then| {
        when.method(POST).path("/chat/completions");
        then.status(307).header("Location", provider.url("/other"));
    });
    let state = AppState {
        client: reqwest::Client::new(),
        bounded_client: bounded_provider_client().unwrap(),
        defaults: PartialConfig::default(),
        pending_edits: crate::pending_edits::PendingEditStore::new(),
        pending_commands: crate::pending_commands::PendingCommandStore::new(),
        workspace_root: PathBuf::new(),
    };
    let url = provider.url("/chat/completions");
    let budget = crate::work_budget::WorkBudget::new("redirect", 1, None);
    let reply = crate::work_budget::scope(
        Some(crate::work_budget::Context {
            budget: budget.clone(),
            role: "coder",
        }),
        async {
            buffered_chat::send(
                state.tool_provider_client(),
                &url,
                None,
                Duration::from_secs(3),
                json!({}),
            )
            .await
        },
    )
    .await;
    assert!(matches!(
        reply,
        buffered_chat::Reply::Rejected(reqwest::StatusCode::TEMPORARY_REDIRECT, _)
    ));
    assert_eq!(budget.snapshot().requests[0].status, "http_307");
    redirect.assert_hits(1);
    target.assert_hits(0);
    let reply = buffered_chat::send(
        state.tool_provider_client(),
        &url,
        None,
        Duration::from_secs(3),
        json!({}),
    )
    .await;
    assert!(matches!(reply, buffered_chat::Reply::Json(_)));
    target.assert_hits(1);
}

#[tokio::test]
async fn buffered_compatibility_retry_records_both_outcomes_and_final_usage() {
    let server = MockServer::start();
    let first = server.mock(|when, then| {
        when.method(POST)
            .path("/chat/completions")
            .json_body(json!({"tool_choice":"required"}));
        then.status(400).body("unsupported tool_choice");
    });
    let second = server.mock(|when, then| {
        when.method(POST)
            .path("/chat/completions")
            .json_body(json!({}));
        then.status(200)
            .json_body(json!({"choices":[],"usage":{"total_tokens":5}}));
    });
    let b = crate::work_budget::WorkBudget::new("task", 2, None);
    let reply = crate::work_budget::scope(
        Some(crate::work_budget::Context {
            budget: b.clone(),
            role: "coder",
        }),
        buffered_chat::send(
            &reqwest::Client::new(),
            &server.url("/chat/completions"),
            None,
            Duration::from_secs(3),
            json!({"tool_choice":"required"}),
        ),
    )
    .await;
    assert!(matches!(reply, buffered_chat::Reply::Json(_)));
    let rows = b.snapshot().requests;
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].status, "http_400");
    assert_eq!(rows[1].status, "completed");
    assert_eq!(rows[1].provider_usage.as_ref().unwrap()["total_tokens"], 5);
    first.assert_hits(1);
    second.assert_hits(1);
}

#[tokio::test]
async fn native_and_web_streams_keep_late_signature_on_original_call() {
    let provider = MockServer::start();
    let original = json!({"id":"signed-read", "type":"function",
        "function":{"name":"read_file","arguments":"{\"path\":\"src/lib.rs\"}"},
        "extra_content":{"google":{"thought_signature":"opaque-roundtrip-test"}}});
    let response = [
        json!({"choices":[{"delta":{"content":"explicit plan", "tool_calls":[
            {"index":0,"id":"signed-read","function":{"name":"read_file","arguments":"{\"path\":"}}]}}]}),
        json!({"choices":[{"delta":{"tool_calls":[
            {"index":0,"function":{"name":"read_file","arguments":"\"src/lib.rs\"}"},
             "extra_content":{"google":{"thought_signature":"opaque-roundtrip-test"}}}]},"finish_reason":"tool_calls"}]}),
    ].iter().map(|v| format!("data: {v}\n\n")).collect::<String>() + "data: [DONE]\n\n";
    let mock = provider.mock(|when, then| {
        when.method(POST).path("/chat/completions");
        then.status(200)
            .header("content-type", "text/event-stream")
            .body(&response);
    });
    let cfg = crate::config::RunConfig {
        provider: ProviderKind::OpenAiCompatible,
        model: "mock".into(),
        chat_model: "mock".into(),
        code_model: "mock".into(),
        api_key: None,
        base_url: provider.base_url(),
        mode: Mode::Vibe,
        persona: String::new(),
        temperature: 0.0,
        max_tokens: 32,
        timeout_seconds: 5,
        hf_device: "cpu".into(),
        hf_local_only: true,
    };
    let (tx, mut rx) = tokio::sync::mpsc::channel(8);
    stream_openai_compat_json(
        &reqwest::Client::new(),
        &cfg,
        &[json!({"role":"user","content":"read source"})],
        None,
        tx,
    )
    .await
    .unwrap();
    let mut calls = Vec::new();
    while let Some(token) = rx.recv().await {
        if let StreamToken::ToolCall(call) = token {
            calls.push(call.to_json());
        }
    }
    assert_eq!(calls, vec![original.clone()]);

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let body = serde_json::to_vec(
        &json!({"messages":[{"role":"user","content":"read source"}],
        "model":"mock","base_url":provider.base_url()}),
    )
    .unwrap();
    let proxy = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let state = AppState {
            client: reqwest::Client::new(),
            bounded_client: bounded_provider_client().unwrap(),
            defaults: PartialConfig::default(),
            pending_edits: crate::pending_edits::PendingEditStore::new(),
            pending_commands: crate::pending_commands::PendingCommandStore::new(),
            workspace_root: PathBuf::new(),
        };
        api_chat_tools_stream(&mut socket, state, &body)
            .await
            .unwrap();
    });
    let mut client = TcpStream::connect(address).await.unwrap();
    let mut wire = String::new();
    tokio::time::timeout(Duration::from_secs(5), client.read_to_string(&mut wire))
        .await
        .unwrap()
        .unwrap();
    proxy.await.unwrap();
    let finish: Value = wire
        .split("event: finish\n")
        .nth(1)
        .unwrap()
        .lines()
        .find_map(|line| line.strip_prefix("data: "))
        .map(|data| serde_json::from_str(data).unwrap())
        .unwrap();
    assert_eq!(
        finish,
        json!({"finish_reason":"tool_calls","tool_calls":[original]})
    );
    mock.assert_hits(2);
}
