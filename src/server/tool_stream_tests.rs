//! Both frontends must retain opaque tool metadata from the same provider stream.
use super::*;
use crate::streaming::{stream_openai_compat_json, StreamToken};
use httpmock::prelude::*;
use serde_json::{json, Value};

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
