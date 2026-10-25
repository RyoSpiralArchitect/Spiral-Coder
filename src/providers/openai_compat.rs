use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use reqwest::StatusCode;
use serde_json::{json, Value};
use std::time::Duration;

use crate::types::{ChatMessage, ChatRequest, ChatResponse};

use super::ChatProvider;

pub struct OpenAICompatibleProvider {
    client: reqwest::Client,
    kind_label: &'static str,
    model: String,
    api_key: Option<String>,
    base_url: String,
    timeout: Duration,
}

impl OpenAICompatibleProvider {
    pub fn new(
        client: reqwest::Client,
        kind_label: &'static str,
        model: String,
        api_key: Option<String>,
        base_url: String,
        timeout: Duration,
    ) -> Self {
        Self {
            client,
            kind_label,
            model,
            api_key,
            base_url: base_url.trim_end_matches('/').to_string(),
            timeout,
        }
    }

    async fn post_json(
        &self,
        url: &str,
        payload: &Value,
    ) -> Result<Result<Value, (StatusCode, String)>> {
        let mut req = self
            .client
            .post(url)
            .header("Content-Type", "application/json")
            .timeout(self.timeout)
            .json(payload);
        if let Some(key) = &self.api_key {
            req = req.bearer_auth(key);
        }

        let meter = crate::work_budget::begin(payload)?;
        let resp = req.send().await.context("request failed")?;
        let status = resp.status();
        if let Some(meter) = &meter {
            meter.status(&format!("http_{}", status.as_u16()));
        }
        let body = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            return Ok(Err((status, body)));
        }
        let data: Value = serde_json::from_str(&body).context("invalid JSON response")?;
        if let Some(meter) = &meter {
            if let Some(usage) = data.get("usage") {
                meter.usage(usage);
            }
            meter.status("completed");
        }
        Ok(Ok(data))
    }
}

fn http_error(kind: &str, status: StatusCode, body: &str) -> anyhow::Error {
    let hint = match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => "authentication failed (check API key)",
        StatusCode::TOO_MANY_REQUESTS => "rate limited (try again later)",
        s if s.is_server_error() => "server error",
        _ => "request failed",
    };
    if body.trim().is_empty() {
        anyhow!("{kind} API error (HTTP {status}): {hint}")
    } else {
        anyhow!("{kind} API error (HTTP {status}): {hint}\n{body}")
    }
}

fn to_openai_messages(messages: &[ChatMessage]) -> Vec<Value> {
    messages
        .iter()
        .map(|m| json!({ "role": m.role, "content": m.content }))
        .collect()
}

fn chat_urls_for_base_url(base_url: &str) -> Vec<String> {
    let base = base_url.trim_end_matches('/');

    // Codestral uses a singular endpoint: /v1/chat/completion
    // We still keep the plural path as a fallback for compatibility.
    if base.contains("codestral.mistral.ai") {
        vec![
            format!("{base}/chat/completion"),
            format!("{base}/chat/completions"),
        ]
    } else {
        vec![
            format!("{base}/chat/completions"),
            format!("{base}/chat/completion"),
        ]
    }
}

fn should_use_v1_completions(status: StatusCode, body: &str) -> bool {
    // Some OpenAI-compatible providers return 404 for non-chat models with a hint to use /v1/completions.
    // Example: "This is not a chat model ... Did you mean to use v1/completions?"
    let msg = body.to_ascii_lowercase();
    if msg.contains("not a chat model") {
        return true;
    }
    if status == StatusCode::NOT_FOUND
        && msg.contains("v1/completions")
        && msg.contains("chat/complet")
    {
        return true;
    }
    false
}

fn to_completions_prompt(messages: &[ChatMessage]) -> String {
    // Minimal, robust chat->prompt adapter.
    // This is used only as a fallback when the provider rejects /chat/completions.
    let mut out = String::new();

    let mut system_parts: Vec<&str> = Vec::new();
    for m in messages {
        if m.role == "system" {
            system_parts.push(m.content.as_str());
        }
    }
    if !system_parts.is_empty() {
        out.push_str(system_parts.join("\n").trim());
        out.push_str("\n\n");
    }

    for m in messages {
        if m.role == "system" {
            continue;
        }
        let label = match m.role.as_str() {
            "user" => "User",
            "assistant" => "Assistant",
            other => other,
        };
        out.push_str(label);
        out.push_str(": ");
        out.push_str(m.content.trim_end());
        out.push('\n');
    }

    // Cue the next assistant completion.
    out.push_str("Assistant: ");
    out
}

fn extract_chat_content(data: &Value) -> Option<String> {
    data.pointer("/choices/0/message/content")
        .and_then(|x| x.as_str())
        .map(|s| s.to_string())
        .or_else(|| {
            // Some providers return text completions format even on chat endpoints.
            data.pointer("/choices/0/text")
                .and_then(|x| x.as_str())
                .map(|s| s.to_string())
        })
}

fn extract_completion_text(data: &Value) -> Option<String> {
    data.pointer("/choices/0/text")
        .and_then(|x| x.as_str())
        .map(|s| s.to_string())
        .or_else(|| extract_chat_content(data))
}

fn maybe_insert_temperature(payload: &mut Value, base_url: &str, model: &str, temperature: f64) {
    if crate::config::should_send_temperature(
        &crate::config::ProviderKind::OpenAiCompatible,
        base_url,
        model,
    ) {
        payload["temperature"] = json!(temperature);
    }
}

#[async_trait]
impl ChatProvider for OpenAICompatibleProvider {
    async fn chat(&self, request: &ChatRequest) -> Result<ChatResponse> {
        let mut payload = json!({
            "model": self.model,
            "messages": to_openai_messages(&request.messages),
        });
        maybe_insert_temperature(
            &mut payload,
            &self.base_url,
            &self.model,
            request.temperature.unwrap_or(0.4),
        );
        if let Some(max_tokens) = request.max_tokens {
            payload["max_tokens"] = json!(max_tokens);
        }
        if let Some(Value::Object(extra)) = &request.metadata {
            if let Value::Object(obj) = &mut payload {
                for (k, v) in extra {
                    obj.insert(k.clone(), v.clone());
                }
            }
        }

        crate::config::model_catalog::prepare_chat_payload(
            &mut payload,
            &if self.kind_label.eq_ignore_ascii_case("mistral") {
                crate::config::ProviderKind::Mistral
            } else {
                crate::config::ProviderKind::OpenAiCompatible
            },
            &self.base_url,
            &self.model,
        );

        let mut last_err: Option<anyhow::Error> = None;
        let mut want_completions = false;

        let mut data: Option<Value> = None;
        for url in chat_urls_for_base_url(&self.base_url) {
            match self.post_json(&url, &payload).await? {
                Ok(v) => {
                    data = Some(v);
                    break;
                }
                Err((status, body)) => {
                    // Some models reject `max_tokens` and require `max_completion_tokens` instead.
                    if status == StatusCode::BAD_REQUEST
                        && body.contains("max_completion_tokens")
                        && body.contains("max_tokens")
                        && request.max_tokens.is_some()
                    {
                        let mut payload2 = payload.clone();
                        if let Some(mt) = request.max_tokens {
                            if let Value::Object(obj) = &mut payload2 {
                                obj.remove("max_tokens");
                                obj.insert("max_completion_tokens".to_string(), json!(mt));
                            }
                        }
                        match self.post_json(&url, &payload2).await? {
                            Ok(v2) => {
                                data = Some(v2);
                                break;
                            }
                            Err((status2, body2)) => {
                                last_err = Some(http_error(self.kind_label, status2, &body2));
                                continue;
                            }
                        }
                    }

                    if should_use_v1_completions(status, &body) {
                        want_completions = true;
                        last_err = Some(http_error(self.kind_label, status, &body));
                        break;
                    }

                    // Endpoint mismatch (e.g. Codestral uses /chat/completion). Try the next path.
                    if status == StatusCode::NOT_FOUND {
                        last_err = Some(http_error(self.kind_label, status, &body));
                        continue;
                    }

                    return Err(http_error(self.kind_label, status, &body));
                }
            }
        }

        let data: Value = if let Some(v) = data {
            v
        } else if want_completions {
            let url = format!("{}/completions", self.base_url.trim_end_matches('/'));
            let mut comp_payload = json!({
                "model": self.model,
                "prompt": to_completions_prompt(&request.messages),
            });
            maybe_insert_temperature(
                &mut comp_payload,
                &self.base_url,
                &self.model,
                request.temperature.unwrap_or(0.4),
            );
            if let Some(max_tokens) = request.max_tokens {
                comp_payload["max_tokens"] = json!(max_tokens);
            }
            if let Some(Value::Object(extra)) = &request.metadata {
                if let Value::Object(obj) = &mut comp_payload {
                    for (k, v) in extra {
                        obj.insert(k.clone(), v.clone());
                    }
                }
            }

            match self.post_json(&url, &comp_payload).await? {
                Ok(v) => v,
                Err((status, body)) => {
                    // Keep parity with chat path: retry once using max_completion_tokens if suggested.
                    if status == StatusCode::BAD_REQUEST
                        && body.contains("max_completion_tokens")
                        && body.contains("max_tokens")
                        && request.max_tokens.is_some()
                    {
                        let mut payload2 = comp_payload.clone();
                        if let Some(mt) = request.max_tokens {
                            if let Value::Object(obj) = &mut payload2 {
                                obj.remove("max_tokens");
                                obj.insert("max_completion_tokens".to_string(), json!(mt));
                            }
                        }
                        match self.post_json(&url, &payload2).await? {
                            Ok(v2) => v2,
                            Err((status2, body2)) => {
                                return Err(http_error(self.kind_label, status2, &body2))
                            }
                        }
                    } else {
                        return Err(http_error(self.kind_label, status, &body));
                    }
                }
            }
        } else {
            return Err(last_err.unwrap_or_else(|| anyhow!("OpenAI-compatible request failed")));
        };

        let content = extract_chat_content(&data)
            .or_else(|| extract_completion_text(&data))
            .ok_or_else(|| anyhow!("OpenAI-compatible: missing content in response"))?;

        Ok(ChatResponse {
            content,
            model: self.model.clone(),
            raw: Some(data),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;

    fn make_request(msg: &str) -> ChatRequest {
        ChatRequest {
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: msg.to_string(),
            }],
            temperature: Some(0.4),
            max_tokens: Some(64),
            metadata: None,
        }
    }

    #[tokio::test]
    async fn returns_content_on_success() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST).path("/chat/completions");
            then.status(200).json_body(serde_json::json!({
                "choices": [{"message": {"role": "assistant", "content": "hello"}}]
            }));
        });

        let client = reqwest::Client::new();
        let provider = OpenAICompatibleProvider::new(
            client,
            "test",
            "gpt-4o-mini".to_string(),
            None,
            server.base_url(),
            Duration::from_secs(5),
        );

        let resp = provider.chat(&make_request("hi")).await.unwrap();
        assert_eq!(resp.content, "hello");
        assert_eq!(resp.model, "gpt-4o-mini");
        mock.assert();
    }

    #[tokio::test]
    async fn returns_error_on_401() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/chat/completions");
            then.status(401).body("Unauthorized");
        });

        let client = reqwest::Client::new();
        let provider = OpenAICompatibleProvider::new(
            client,
            "test",
            "gpt-4o-mini".to_string(),
            Some("bad-key".to_string()),
            server.base_url(),
            Duration::from_secs(5),
        );

        let err = provider.chat(&make_request("hi")).await.unwrap_err();
        assert!(err.to_string().contains("authentication failed"), "{err}");
    }

    #[tokio::test]
    async fn sends_bearer_auth_header() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/chat/completions")
                .header("Authorization", "Bearer my-key");
            then.status(200).json_body(serde_json::json!({
                "choices": [{"message": {"role": "assistant", "content": "ok"}}]
            }));
        });

        let client = reqwest::Client::new();
        let provider = OpenAICompatibleProvider::new(
            client,
            "test",
            "model".to_string(),
            Some("my-key".to_string()),
            server.base_url(),
            Duration::from_secs(5),
        );

        provider.chat(&make_request("hi")).await.unwrap();
        mock.assert();
    }

    #[tokio::test]
    async fn falls_back_to_singular_chat_path_on_404() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/chat/completions");
            then.status(404).body("Not Found");
        });
        let mock2 = server.mock(|when, then| {
            when.method(POST).path("/chat/completion");
            then.status(200).json_body(serde_json::json!({
                "choices": [{"message": {"role": "assistant", "content": "hello"}}]
            }));
        });

        let client = reqwest::Client::new();
        let provider = OpenAICompatibleProvider::new(
            client,
            "test",
            "model".to_string(),
            None,
            server.base_url(),
            Duration::from_secs(5),
        );

        let resp = provider.chat(&make_request("hi")).await.unwrap();
        assert_eq!(resp.content, "hello");
        mock2.assert();
    }

    #[tokio::test]
    async fn falls_back_to_v1_completions_when_not_chat_model() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/chat/completions");
            then.status(404).json_body(serde_json::json!({
                "error": { "message": "This is not a chat model and thus not supported in the v1/chat/completions endpoint. Did you mean to use v1/completions?" }
            }));
        });
        let mock2 = server.mock(|when, then| {
            when.method(POST).path("/completions");
            then.status(200).json_body(serde_json::json!({
                "choices": [{"text": "hello from completions"}]
            }));
        });

        let client = reqwest::Client::new();
        let provider = OpenAICompatibleProvider::new(
            client,
            "test",
            "model".to_string(),
            None,
            server.base_url(),
            Duration::from_secs(5),
        );

        let resp = provider.chat(&make_request("hi")).await.unwrap();
        assert!(
            resp.content.contains("hello from completions"),
            "{}",
            resp.content
        );
        mock2.assert();
    }

    #[test]
    fn maybe_insert_temperature_skips_openai_gpt5_family() {
        let mut payload = serde_json::json!({
            "model": "gpt-5-mini",
            "messages": []
        });
        maybe_insert_temperature(&mut payload, "https://api.openai.com/v1", "gpt-5-mini", 0.4);
        assert!(payload.get("temperature").is_none());
    }

    #[test]
    fn maybe_insert_temperature_skips_custom_openai_compat_gpt5_family() {
        let mut payload = serde_json::json!({
            "model": "gpt-5",
            "messages": []
        });
        maybe_insert_temperature(
            &mut payload,
            "https://proxy.example.internal/v1",
            "gpt-5",
            0.4,
        );
        assert!(payload.get("temperature").is_none());
    }
    #[tokio::test]
    async fn compatibility_retry_spends_the_same_shared_request_cap() {
        let server = MockServer::start();
        let wire = server.mock(|when, then| {
            when.method(POST).path("/chat/completions");
            then.status(400)
                .body("Unsupported parameter: 'max_tokens'. Use 'max_completion_tokens' instead.");
        });
        let provider = OpenAICompatibleProvider::new(
            reqwest::Client::new(),
            "test",
            "test".into(),
            None,
            server.base_url(),
            Duration::from_secs(3),
        );
        let budget = crate::work_budget::WorkBudget::new("task", 1, None);
        let result = crate::work_budget::scope(
            Some(crate::work_budget::Context {
                budget: budget.clone(),
                role: "observer_repair",
            }),
            provider.chat(&make_request("task")),
        )
        .await;
        assert!(format!("{:?}", result.err().unwrap()).contains("budget exhausted"));
        wire.assert_hits(1);
        assert_eq!(budget.snapshot().requests[0].role, "observer_repair");
    }
}
