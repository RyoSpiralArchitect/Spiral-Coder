use super::*;
use crate::modes::Mode;
use crate::providers::{
    anthropic::AnthropicProvider, openai_compat::OpenAICompatibleProvider, ChatProvider,
};
use crate::types::ChatRequest;
use httpmock::prelude::*;

fn cfg(server: &MockServer, provider: ProviderKind, model: &str) -> RunConfig {
    RunConfig {
        provider,
        model: model.into(),
        chat_model: model.into(),
        code_model: model.into(),
        api_key: Some("test-key".into()),
        base_url: server.base_url(),
        mode: Mode::Chat,
        persona: "default".into(),
        temperature: 0.2,
        max_tokens: 64,
        timeout_seconds: 5,
        hf_device: "cpu".into(),
        hf_local_only: true,
    }
}

fn compatible_payload(request: &httpmock::prelude::HttpMockRequest) -> bool {
    let body: serde_json::Value =
        serde_json::from_slice(request.body.as_deref().unwrap_or_default()).unwrap();
    body.get("temperature").is_none()
        && match body["model"].as_str().unwrap() {
            "gpt-5.6-luna" | "gpt-6-luna" | "gpt-6-sol" => {
                body["reasoning_effort"] == "none"
                    && body["max_completion_tokens"] == 64
                    && body.get("max_tokens").is_none()
            }
            "claude-sonnet-5-5" => {
                body["max_tokens"] == 64 && body.get("reasoning_effort").is_none()
            }
            _ => false,
        }
}

#[tokio::test]
async fn coder_stream_and_observer_send_compatible_payloads_on_first_request() {
    for model in [
        "gpt-5.6-luna",
        "gpt-6-luna",
        "gpt-6-sol",
        "claude-sonnet-5-5",
    ] {
        let server = MockServer::start();
        let stream = server.mock(|when, then| {
            when.method(POST).path("/chat/completions").json_body_partial(r#"{"stream":true}"#).matches(compatible_payload);
            then.status(200).header("content-type", "text/event-stream").body("data: {\"choices\":[{\"delta\":{\"content\":\"ok\"},\"finish_reason\":null}]}\n\ndata: [DONE]\n\n");
        });
        let plain = server.mock(|when, then| {
            when.method(POST)
                .path("/chat/completions")
                .matches(|request| {
                    let body: serde_json::Value =
                        serde_json::from_slice(request.body.as_deref().unwrap()).unwrap();
                    body.get("stream").is_none() && compatible_payload(request)
                });
            then.status(200)
                .json_body(json!({"choices":[{"message":{"content":"ok"}}]}));
        });
        let cfg = cfg(&server, ProviderKind::OpenAiCompatible, model);
        let messages = vec![ChatMessage {
            role: "user".into(),
            content: "test".into(),
        }];
        let tools = json!([{"type":"function","function":{"name":"read_file","parameters":{"type":"object","properties":{"path":{"type":"string"}}}}}]);
        let (tx, _rx) = mpsc::channel(32);
        stream_openai_compat_json(
            &reqwest::Client::new(),
            &cfg,
            &messages
                .iter()
                .map(|m| json!({"role":m.role,"content":m.content}))
                .collect::<Vec<_>>(),
            Some(&tools),
            tx,
        )
        .await
        .unwrap();
        let provider = OpenAICompatibleProvider::new(
            reqwest::Client::new(),
            "openai-compatible",
            model.into(),
            cfg.api_key.clone(),
            server.base_url(),
            Duration::from_secs(5),
        );
        provider
            .chat(&ChatRequest {
                messages,
                temperature: Some(0.2),
                max_tokens: Some(64),
                metadata: None,
            })
            .await
            .unwrap();
        stream.assert_hits(1);
        plain.assert_hits(1);
    }
}

#[tokio::test]
async fn native_claude_stream_and_observer_omit_unsupported_sampling() {
    let server = MockServer::start();
    let wire = server.mock(|when, then| {
        when.method(POST)
            .path("/messages")
            .matches(compatible_payload);
        then.status(200)
            .json_body(json!({"content":[{"type":"text","text":"ok"}]}));
    });
    let cfg = cfg(&server, ProviderKind::Anthropic, "claude-sonnet-5-5");
    let messages = vec![ChatMessage {
        role: "user".into(),
        content: "test".into(),
    }];
    let (tx, _rx) = mpsc::channel(32);
    stream_anthropic(&reqwest::Client::new(), &cfg, &messages, tx)
        .await
        .unwrap();
    let provider = AnthropicProvider::new(
        reqwest::Client::new(),
        cfg.model.clone(),
        cfg.api_key.clone(),
        server.base_url(),
        Duration::from_secs(5),
    );
    provider
        .chat(&ChatRequest {
            messages,
            temperature: Some(0.2),
            max_tokens: Some(64),
            metadata: None,
        })
        .await
        .unwrap();
    wire.assert_hits(2);
}

#[tokio::test]
async fn large_four_coder_and_observer_use_greedy_parameters_on_first_request() {
    let server = MockServer::start();
    let matches = |request: &HttpMockRequest| {
        let body: serde_json::Value =
            serde_json::from_slice(request.body.as_deref().unwrap()).unwrap();
        body["temperature"] == 0.0 && body["top_p"] == 1.0 && body["max_tokens"] == 64
    };
    let stream = server.mock(|when, then| {
        when.method(POST)
            .path("/chat/completions")
            .json_body_partial(r#"{"stream":true}"#)
            .matches(matches);
        then.status(200)
            .header("content-type", "text/event-stream")
            .body("data: {\"choices\":[{\"delta\":{\"content\":\"ok\"}}]}\n\ndata: [DONE]\n\n");
    });
    let plain = server.mock(|when, then| {
        when.method(POST)
            .path("/chat/completions")
            .matches(matches)
            .matches(|request| {
                serde_json::from_slice::<serde_json::Value>(request.body.as_deref().unwrap())
                    .unwrap()
                    .get("stream")
                    .is_none()
            });
        then.status(200)
            .json_body(json!({"choices":[{"message":{"content":"ok"}}]}));
    });
    let mut cfg = cfg(&server, ProviderKind::Mistral, "mistral-large-4");
    cfg.temperature = 0.0;
    let messages = vec![json!({"role":"user","content":"test"})];
    let tools = json!([{"type":"function","function":{"name":"read_file","parameters":{"type":"object","properties":{"path":{"type":"string"}}}}}]);
    let (tx, _rx) = mpsc::channel(32);
    stream_openai_compat_json(&reqwest::Client::new(), &cfg, &messages, Some(&tools), tx)
        .await
        .unwrap();
    let provider = OpenAICompatibleProvider::new(
        reqwest::Client::new(),
        "Mistral",
        cfg.model.clone(),
        cfg.api_key.clone(),
        server.base_url(),
        Duration::from_secs(5),
    );
    provider
        .chat(&ChatRequest {
            messages: vec![ChatMessage {
                role: "user".into(),
                content: "test".into(),
            }],
            temperature: Some(0.0),
            max_tokens: Some(64),
            metadata: None,
        })
        .await
        .unwrap();
    stream.assert_hits(1);
    plain.assert_hits(1);
}
