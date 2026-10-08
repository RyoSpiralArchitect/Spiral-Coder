use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::time::Duration;
use tokio::sync::mpsc;

use crate::config::{ProviderKind, RunConfig};
use crate::types::ChatMessage;

mod tool_calls;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReflectionSummary {
    pub last_outcome: Option<String>,
    pub goal_delta: Option<String>,
    pub wrong_assumption: Option<String>,
    pub strategy_change: Option<String>,
    pub next_minimal_action: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernorState {
    pub state: String,
    pub recovery_stage: Option<String>,

    pub consecutive_failures: usize,
    pub same_command_repeats: usize,
    pub same_error_repeats: usize,
    pub same_output_repeats: usize,
    pub file_tool_consec_failures: usize,

    pub done_verify_required: bool,
    pub last_mutation_step: Option<usize>,
    pub last_verify_ok_step: Option<usize>,

    pub last_reflection: Option<ReflectionSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RealizeState {
    pub pending: bool,
    pub age_turns: usize,
    pub window_end: usize,
    pub latest_drift: Option<f64>,
    pub mean_drift: f64,
    pub mean_realize_latency: f64,
    pub missing: usize,
    pub early_leakage: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryEvent {
    pub event: String,
    #[serde(default)]
    pub data: serde_json::Value,
}

#[derive(Debug, Clone)]
pub enum StreamToken {
    Delta(String),
    ToolCall(ToolCallData),
    GovernorState(GovernorState),
    RealizeState(RealizeState),
    Telemetry(TelemetryEvent),
    Done,
    Error(String),
    /// Git checkpoint hash created at session start (for rollback).
    Checkpoint(String),
}

#[derive(Debug, Clone)]
pub struct ToolCallData {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

fn stream_chat_urls_for_base_url(base_url: &str) -> Vec<String> {
    let base = base_url.trim_end_matches('/');
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

fn should_use_v1_completions(status: reqwest::StatusCode, body: &str) -> bool {
    let msg = body.to_ascii_lowercase();
    if msg.contains("not a chat model") {
        return true;
    }
    if status == reqwest::StatusCode::NOT_FOUND
        && msg.contains("v1/completions")
        && msg.contains("chat/complet")
    {
        return true;
    }
    false
}

fn should_retry_empty_bad_request(status: reqwest::StatusCode, body: &str) -> bool {
    status == reqwest::StatusCode::BAD_REQUEST && body.trim().is_empty()
}

fn is_retryable_status(status: reqwest::StatusCode) -> bool {
    status == reqwest::StatusCode::TOO_MANY_REQUESTS
        || status.is_server_error()
        || status == reqwest::StatusCode::REQUEST_TIMEOUT
}

fn format_provider_http_error(label: &str, status: reqwest::StatusCode, body: &str) -> String {
    if body.trim().is_empty() {
        format!("{label} API error (HTTP {status})\n(empty response body)")
    } else {
        format!("{label} API error (HTTP {status})\n{body}")
    }
}

fn retry_after_duration(headers: &reqwest::header::HeaderMap) -> Option<Duration> {
    // Best-effort: support only integer seconds (common for 429).
    let v = headers.get("retry-after")?;
    let s = v.to_str().ok()?.trim();
    let secs: u64 = s.parse().ok()?;
    Some(Duration::from_secs(secs.min(15)))
}

fn max_connect_retries(provider: &ProviderKind) -> usize {
    match provider {
        // Mistral commonly returns bursty 429s during eval runs; a slightly deeper budget
        // keeps the single-case harness usable without changing task logic.
        ProviderKind::Mistral => 5,
        // Give other providers one extra retry by default without making UX feel stuck.
        _ => 4,
    }
}

fn backoff_delay(
    provider: &ProviderKind,
    status: Option<reqwest::StatusCode>,
    attempt: usize,
    retry_after: Option<Duration>,
) -> Duration {
    // attempt: 0..N (0 = first retry)
    let pow = attempt.min(5) as u32;
    // Keep this robust even if the cap changes in the future.
    let factor = 1u64.checked_shl(pow).unwrap_or(u64::MAX);
    let (base_ms, cap_ms, rate_limit_floor_ms) = match provider {
        ProviderKind::Mistral => (1000u64, 20_000u64, 5000u64),
        _ => (500u64, 6000u64, 0u64),
    };
    let base_ms = base_ms.saturating_mul(factor);
    let mut d = Duration::from_millis(base_ms.min(cap_ms));
    if status == Some(reqwest::StatusCode::TOO_MANY_REQUESTS) && rate_limit_floor_ms > 0 {
        let floor = Duration::from_millis(rate_limit_floor_ms);
        if d < floor {
            d = floor;
        }
    }
    if let Some(ra) = retry_after {
        let ra = ra.min(Duration::from_secs(30));
        if ra > d {
            d = ra;
        }
    }
    d
}

fn is_retryable_send_error(e: &reqwest::Error) -> bool {
    e.is_timeout() || e.is_connect()
}

async fn emit_retry_telemetry(
    tx: &mpsc::Sender<StreamToken>,
    provider: &ProviderKind,
    status: Option<reqwest::StatusCode>,
    attempt: usize,
    delay: Duration,
    url: &str,
) {
    let _ = tx
        .send(StreamToken::Telemetry(TelemetryEvent {
            event: "provider_retry".to_string(),
            data: json!({
                "provider": provider.key(),
                "status": status.map(|s| s.as_u16()),
                "attempt": attempt + 1,
                "delay_ms": delay.as_millis() as u64,
                "url": url,
            }),
        }))
        .await;
}

fn swap_max_tokens_to_max_completion_tokens(payload: &mut serde_json::Value) {
    if let Some(obj) = payload.as_object_mut() {
        if let Some(mt) = obj.remove("max_tokens") {
            obj.insert("max_completion_tokens".to_string(), mt);
        }
    }
}

fn apply_temperature_compat(payload: &mut serde_json::Value, cfg: &RunConfig) {
    if !crate::config::should_send_temperature_for_run(cfg) {
        if let Some(obj) = payload.as_object_mut() {
            obj.remove("temperature");
        }
    }
}

fn prompt_from_json_messages(messages: &[serde_json::Value]) -> String {
    // Minimal adapter used only when a provider rejects /chat/completions.
    // This keeps TUI + server streaming usable even for completion-only models.
    let mut out = String::new();

    // Gather system messages first.
    let mut sys: Vec<String> = Vec::new();
    for m in messages {
        if m.get("role").and_then(|x| x.as_str()) == Some("system") {
            if let Some(s) = m.get("content").and_then(|x| x.as_str()) {
                if !s.trim().is_empty() {
                    sys.push(s.trim_end().to_string());
                }
            }
        }
    }
    if !sys.is_empty() {
        out.push_str(sys.join("\n").trim());
        out.push_str("\n\n");
    }

    for m in messages {
        let role = m.get("role").and_then(|x| x.as_str()).unwrap_or("");
        if role == "system" {
            continue;
        }
        let content = m.get("content").and_then(|x| x.as_str()).unwrap_or("");
        if content.trim().is_empty() {
            continue;
        }
        let label = match role {
            "user" => "User",
            "assistant" => "Assistant",
            other => other,
        };
        out.push_str(label);
        out.push_str(": ");
        out.push_str(content.trim_end());
        out.push('\n');
    }

    out.push_str("Assistant: ");
    out
}

fn normalize_mistral_messages(messages: &[serde_json::Value]) -> Vec<serde_json::Value> {
    let mut out: Vec<serde_json::Value> = Vec::with_capacity(messages.len());
    let mut system_head = String::new();
    let mut seen_non_system = false;

    for msg in messages {
        let role = msg.get("role").and_then(|x| x.as_str()).unwrap_or("");
        let content = msg
            .get("content")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .trim();
        let has_tool_calls = msg
            .get("tool_calls")
            .and_then(|x| x.as_array())
            .map(|arr| !arr.is_empty())
            .unwrap_or(false);

        // Mistral is strict about assistant turns: they must contain text or tool_calls.
        // Drop no-op assistant placeholders rather than sending invalid transcript entries.
        if role == "assistant" && content.is_empty() && !has_tool_calls {
            continue;
        }

        if role != "system" {
            seen_non_system = true;
            out.push(msg.clone());
            continue;
        }

        if content.is_empty() {
            continue;
        }

        if !seen_non_system {
            if !system_head.is_empty() {
                system_head.push_str("\n\n");
            }
            system_head.push_str(content);
        } else {
            out.push(json!({
                "role": "user",
                "content": format!("[System note]\n{content}")
            }));
        }
    }

    if system_head.is_empty() {
        out
    } else {
        let mut with_head = Vec::with_capacity(out.len() + 1);
        with_head.push(json!({"role": "system", "content": system_head}));
        with_head.extend(out);
        with_head
    }
}

/// Simple chat: converts ChatMessage slice to JSON and delegates.
pub async fn stream_openai_compat(
    client: &reqwest::Client,
    cfg: &RunConfig,
    messages: &[ChatMessage],
    tools: Option<&serde_json::Value>,
    tx: mpsc::Sender<StreamToken>,
) -> Result<()> {
    let msgs_json: Vec<serde_json::Value> = messages
        .iter()
        .map(|m| json!({"role": m.role, "content": m.content}))
        .collect();
    stream_openai_compat_json(client, cfg, &msgs_json, tools, tx).await
}

/// Agent use: accepts pre-built JSON messages so tool_call_id / tool_calls
/// fields are preserved intact across iterations.
pub async fn stream_openai_compat_json(
    client: &reqwest::Client,
    cfg: &RunConfig,
    messages: &[serde_json::Value],
    tools: Option<&serde_json::Value>,
    tx: mpsc::Sender<StreamToken>,
) -> Result<()> {
    let provider_messages = crate::task_origin::provider_messages(messages);
    let prepared_messages: Vec<serde_json::Value> = match cfg.provider {
        ProviderKind::Mistral => normalize_mistral_messages(&provider_messages),
        _ => provider_messages,
    };

    let mut payload = json!({
        "model": cfg.model,
        "messages": prepared_messages,
        "temperature": cfg.temperature,
        "max_tokens": cfg.max_tokens,
        "stream": true,
    });
    apply_temperature_compat(&mut payload, cfg);

    if let Some(t) = tools {
        payload["tools"] = t.clone();
        // "required" forces the model to call a tool on every turn,
        // preventing it from skipping exec and producing text-only responses.
        payload["tool_choice"] = json!("required");
    }

    let label = match cfg.provider {
        ProviderKind::Mistral => "Mistral",
        _ => "OpenAI-compatible",
    };

    // Try chat endpoints first, then fall back to /v1/completions if the provider says
    // this isn't a chat model.
    let mut last_err: Option<anyhow::Error> = None;
    let mut want_completions = false;

    let mut resp: Option<reqwest::Response> = None;

    let max_connect_retries = max_connect_retries(&cfg.provider);

    for url in stream_chat_urls_for_base_url(&cfg.base_url) {
        let proxy_hint = if cfg!(target_os = "windows") {
            r#"If behind a proxy, set: $env:HTTPS_PROXY="http://host:port""#
        } else {
            r#"If behind a proxy, set: export HTTPS_PROXY="http://host:port""#
        };
        let connect_ctx = format!("failed to connect to {url}\n{proxy_hint}");

        let mut payload_try = payload.clone();
        let mut attempt: usize = 0;
        loop {
            let mut req = client
                .post(&url)
                .header("Content-Type", "application/json")
                .timeout(Duration::from_secs(cfg.timeout_seconds))
                .json(&payload_try);
            if let Some(key) = &cfg.api_key {
                req = req.bearer_auth(key);
            }

            let r = match req.send().await {
                Ok(r) => r,
                Err(e) => {
                    let retryable = is_retryable_send_error(&e);
                    let err = anyhow!(e).context(connect_ctx.clone());
                    if attempt < max_connect_retries && retryable {
                        last_err = Some(err);
                        let d = backoff_delay(&cfg.provider, None, attempt, None);
                        emit_retry_telemetry(&tx, &cfg.provider, None, attempt, d, &url).await;
                        attempt = attempt.saturating_add(1);
                        tokio::time::sleep(d).await;
                        continue;
                    }
                    return Err(err);
                }
            };

            let status = r.status();
            if status.is_success() {
                resp = Some(r);
                break;
            }

            let ra = retry_after_duration(r.headers());
            let body = r.text().await.unwrap_or_default();

            // Some models reject `max_tokens` and require `max_completion_tokens`.
            if status == reqwest::StatusCode::BAD_REQUEST
                && body.contains("max_completion_tokens")
                && body.contains("max_tokens")
                && payload_try.get("max_tokens").is_some()
            {
                swap_max_tokens_to_max_completion_tokens(&mut payload_try);
                // Retry immediately with adjusted payload (do not count as backoff attempt).
                continue;
            }

            // Some providers don't support tool_choice. Retry once without it.
            if status == reqwest::StatusCode::BAD_REQUEST
                && body.to_ascii_lowercase().contains("tool_choice")
                && (body.to_ascii_lowercase().contains("unsupported")
                    || body.to_ascii_lowercase().contains("unknown"))
            {
                if let Some(obj) = payload_try.as_object_mut() {
                    obj.remove("tool_choice");
                }
                continue;
            }

            if attempt < max_connect_retries && should_retry_empty_bad_request(status, &body) {
                let d = backoff_delay(&cfg.provider, Some(status), attempt, ra);
                emit_retry_telemetry(&tx, &cfg.provider, Some(status), attempt, d, &url).await;
                attempt = attempt.saturating_add(1);
                tokio::time::sleep(d).await;
                continue;
            }

            if should_use_v1_completions(status, &body) {
                want_completions = true;
                last_err = Some(anyhow!(format_provider_http_error(label, status, &body)));
                break;
            }

            // Endpoint mismatch (e.g. Codestral uses /chat/completion). Try next candidate.
            if status == reqwest::StatusCode::NOT_FOUND {
                last_err = Some(anyhow!(format_provider_http_error(label, status, &body)));
                break;
            }

            if attempt < max_connect_retries && is_retryable_status(status) {
                let d = backoff_delay(&cfg.provider, Some(status), attempt, ra);
                emit_retry_telemetry(&tx, &cfg.provider, Some(status), attempt, d, &url).await;
                attempt = attempt.saturating_add(1);
                tokio::time::sleep(d).await;
                continue;
            }

            return Err(anyhow!(format_provider_http_error(label, status, &body)));
        }

        if resp.is_some() || want_completions {
            break;
        }
    }

    let mut resp = if let Some(r) = resp {
        r
    } else if want_completions {
        let url = format!("{}/completions", cfg.base_url.trim_end_matches('/'));
        let prompt = prompt_from_json_messages(&prepared_messages);
        let mut comp_payload = json!({
            "model": cfg.model,
            "prompt": prompt,
            "temperature": cfg.temperature,
            "max_tokens": cfg.max_tokens,
            "stream": true,
        });
        apply_temperature_compat(&mut comp_payload, cfg);

        // No tools on completions endpoint.
        let mut attempt: usize = 0;
        loop {
            let mut req = client
                .post(&url)
                .header("Content-Type", "application/json")
                .timeout(Duration::from_secs(cfg.timeout_seconds))
                .json(&comp_payload);
            if let Some(key) = &cfg.api_key {
                req = req.bearer_auth(key);
            }

            let r = match req.send().await {
                Ok(r) => r,
                Err(e) => {
                    if attempt < max_connect_retries && is_retryable_send_error(&e) {
                        let d = backoff_delay(&cfg.provider, None, attempt, None);
                        emit_retry_telemetry(&tx, &cfg.provider, None, attempt, d, &url).await;
                        attempt = attempt.saturating_add(1);
                        tokio::time::sleep(d).await;
                        continue;
                    }
                    return Err(anyhow!(e).context("request failed"));
                }
            };

            let status = r.status();
            if status.is_success() {
                break r;
            }

            let ra = retry_after_duration(r.headers());
            let body = r.text().await.unwrap_or_default();

            if status == reqwest::StatusCode::BAD_REQUEST
                && body.contains("max_completion_tokens")
                && body.contains("max_tokens")
                && comp_payload.get("max_tokens").is_some()
            {
                swap_max_tokens_to_max_completion_tokens(&mut comp_payload);
                continue;
            }

            if attempt < max_connect_retries && should_retry_empty_bad_request(status, &body) {
                let d = backoff_delay(&cfg.provider, Some(status), attempt, ra);
                emit_retry_telemetry(&tx, &cfg.provider, Some(status), attempt, d, &url).await;
                attempt = attempt.saturating_add(1);
                tokio::time::sleep(d).await;
                continue;
            }

            if attempt < max_connect_retries && is_retryable_status(status) {
                let d = backoff_delay(&cfg.provider, Some(status), attempt, ra);
                emit_retry_telemetry(&tx, &cfg.provider, Some(status), attempt, d, &url).await;
                attempt = attempt.saturating_add(1);
                tokio::time::sleep(d).await;
                continue;
            }

            return Err(anyhow!(format_provider_http_error(label, status, &body)));
        }
    } else {
        return Err(last_err.unwrap_or_else(|| anyhow!("{label} request failed")));
    };

    let mut tool_calls = tool_calls::ToolCallAccumulator::default();

    let mut buf: Vec<u8> = Vec::new();
    while let Some(chunk) = resp.chunk().await? {
        buf.extend_from_slice(&chunk);
        while let Some(frame) = take_next_sse_frame(&mut buf) {
            let frame_str = String::from_utf8_lossy(&frame);
            let mut data_lines: Vec<&str> = Vec::new();
            for line in frame_str.split('\n') {
                let line = line.trim_end_matches('\r');
                if line.is_empty() || line.starts_with(':') {
                    continue;
                }
                if let Some(rest) = line.strip_prefix("data:") {
                    data_lines.push(rest.trim_start());
                }
            }
            if data_lines.is_empty() {
                continue;
            }
            let data = data_lines.join("\n");
            if data.trim() == "[DONE]" {
                for call in tool_calls.drain() {
                    let _ = tx.send(StreamToken::ToolCall(call)).await;
                }
                let _ = tx.send(StreamToken::Done).await;
                return Ok(());
            }

            let v: serde_json::Value = match serde_json::from_str(&data) {
                Ok(v) => v,
                Err(_) => continue,
            };

            if let Some(err) = v.get("error") {
                let _ = tx.send(StreamToken::Error(err.to_string())).await;
                return Err(anyhow!("provider error: {err}"));
            }

            let finish_reason = v
                .pointer("/choices/0/finish_reason")
                .and_then(|x| x.as_str())
                .unwrap_or("");

            let delta_text = v
                .pointer("/choices/0/delta/content")
                .and_then(|x| x.as_str())
                .or_else(|| v.pointer("/choices/0/delta/text").and_then(|x| x.as_str()))
                .or_else(|| v.pointer("/choices/0/text").and_then(|x| x.as_str()))
                .unwrap_or("");
            if !delta_text.is_empty() {
                let _ = tx.send(StreamToken::Delta(delta_text.to_string())).await;
            }

            if let Some(calls) = v.pointer("/choices/0/delta/tool_calls") {
                tool_calls.push(calls)?;
            }

            if finish_reason == "tool_calls" {
                for call in tool_calls.drain() {
                    let _ = tx.send(StreamToken::ToolCall(call)).await;
                }
            }

            if finish_reason == "stop" {
                let _ = tx.send(StreamToken::Done).await;
                return Ok(());
            }
        }
    }

    let _ = tx.send(StreamToken::Done).await;
    Ok(())
}

/// Send streaming tokens from the Anthropic provider.
pub async fn stream_anthropic(
    client: &reqwest::Client,
    cfg: &RunConfig,
    messages: &[ChatMessage],
    tx: mpsc::Sender<StreamToken>,
) -> Result<()> {
    let api_key = cfg
        .api_key
        .as_ref()
        .ok_or_else(|| anyhow!("missing API key for anthropic"))?;

    let url = format!("{}/messages", cfg.base_url);

    let system_msg = messages
        .iter()
        .find(|m| m.role == "system")
        .map(|m| m.content.as_str())
        .unwrap_or("");

    let chat_msgs: Vec<serde_json::Value> = messages
        .iter()
        .filter(|m| m.role == "user" || m.role == "assistant")
        .map(|m| json!({"role": m.role, "content": m.content}))
        .collect();

    let payload = json!({
        "model": cfg.model,
        "messages": chat_msgs,
        "system": system_msg,
        "temperature": cfg.temperature,
        "max_tokens": cfg.max_tokens,
        "stream": true,
    });

    let max_connect_retries = max_connect_retries(&cfg.provider);
    let mut attempt: usize = 0;
    let resp = loop {
        let r = client
            .post(&url)
            .header("x-api-key", api_key)
            .header(
                "anthropic-version",
                crate::providers::anthropic::ANTHROPIC_VERSION,
            )
            .header("Accept", "text/event-stream")
            .header("Content-Type", "application/json")
            .timeout(Duration::from_secs(cfg.timeout_seconds))
            .json(&payload)
            .send()
            .await;

        let r = match r {
            Ok(r) => r,
            Err(e) => {
                if attempt < max_connect_retries && is_retryable_send_error(&e) {
                    let d = backoff_delay(&cfg.provider, None, attempt, None);
                    emit_retry_telemetry(&tx, &cfg.provider, None, attempt, d, &url).await;
                    attempt = attempt.saturating_add(1);
                    tokio::time::sleep(d).await;
                    continue;
                }
                return Err(anyhow!(e).context("request failed"));
            }
        };

        let status = r.status();
        if status.is_success() {
            break r;
        }

        let ra = retry_after_duration(r.headers());
        let body = r.text().await.unwrap_or_default();
        if attempt < max_connect_retries && is_retryable_status(status) {
            let d = backoff_delay(&cfg.provider, Some(status), attempt, ra);
            emit_retry_telemetry(&tx, &cfg.provider, Some(status), attempt, d, &url).await;
            attempt = attempt.saturating_add(1);
            tokio::time::sleep(d).await;
            continue;
        }

        return Err(anyhow!("Anthropic API error (HTTP {status})\n{body}"));
    };

    let mut resp = resp;
    let mut buf: Vec<u8> = Vec::new();
    while let Some(chunk) = resp.chunk().await? {
        buf.extend_from_slice(&chunk);
        while let Some(frame) = take_next_sse_frame(&mut buf) {
            let frame_str = String::from_utf8_lossy(&frame);
            let mut event: Option<&str> = None;
            let mut data_lines: Vec<&str> = Vec::new();
            for line in frame_str.split('\n') {
                let line = line.trim_end_matches('\r');
                if line.is_empty() || line.starts_with(':') {
                    continue;
                }
                if let Some(rest) = line.strip_prefix("event:") {
                    event = Some(rest.trim());
                }
                if let Some(rest) = line.strip_prefix("data:") {
                    data_lines.push(rest.trim_start());
                }
            }
            if data_lines.is_empty() {
                continue;
            }
            let data = data_lines.join("\n");
            if data.trim().is_empty() || data.trim() == "[DONE]" {
                continue;
            }

            let v: serde_json::Value = match serde_json::from_str(&data) {
                Ok(v) => v,
                Err(_) => continue,
            };

            let ty = v
                .get("type")
                .and_then(|x| x.as_str())
                .or(event)
                .unwrap_or("");

            if ty == "error" {
                let msg = v.to_string();
                let _ = tx.send(StreamToken::Error(msg.clone())).await;
                return Err(anyhow!("Anthropic stream error: {msg}"));
            }

            if ty == "content_block_delta" {
                let delta = v
                    .pointer("/delta/text")
                    .and_then(|x| x.as_str())
                    .unwrap_or("");
                if !delta.is_empty() {
                    let _ = tx.send(StreamToken::Delta(delta.to_string())).await;
                }
            }

            if ty == "message_stop" {
                let _ = tx.send(StreamToken::Done).await;
                return Ok(());
            }
        }
    }

    let _ = tx.send(StreamToken::Done).await;
    Ok(())
}

fn take_next_sse_frame(buf: &mut Vec<u8>) -> Option<Vec<u8>> {
    let pos_lf = find_subslice(buf, b"\n\n");
    let pos_crlf = find_subslice(buf, b"\r\n\r\n");
    let pos = match (pos_lf, pos_crlf) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }?;
    let sep_len = if buf[pos..].starts_with(b"\r\n\r\n") {
        4
    } else {
        2
    };
    let frame = buf[..pos].to_vec();
    buf.drain(..pos + sep_len);
    Some(frame)
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn compatible_requests_keep_runtime_context_without_local_provenance() {
        use crate::task_origin::{user_message, MessageOrigin};
        use httpmock::prelude::*;

        for provider in [ProviderKind::OpenAiCompatible, ProviderKind::Mistral] {
            let server = MockServer::start();
            let request = server.mock(|when, then| {
                when.method(POST)
                    .path("/chat/completions")
                    .matches(|request| {
                        let Ok(body) = serde_json::from_slice::<serde_json::Value>(
                            request.body.as_deref().unwrap_or_default(),
                        ) else {
                            return false;
                        };
                        body["messages"] == json!([
                            {"role":"system", "content":"system contract"},
                            {"role":"user", "content":"human task"},
                            {"role":"assistant", "tool_calls":[{"id":"read-call", "type":"function", "function":{"name":"read_file", "arguments":"{}"}}]},
                            {"role":"tool", "tool_call_id":"read-call", "content":"source evidence"},
                            {"role":"user", "content":"runtime continuation"}
                        ])
                    });
                then.status(200)
                    .header("content-type", "text/event-stream")
                    .body("data: {\"choices\":[{\"delta\":{\"content\":\"ok\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n");
            });
            let messages = vec![
                json!({"role":"system", "content":"system contract"}),
                user_message("human task", MessageOrigin::User),
                json!({"role":"assistant", "tool_calls":[{"id":"read-call", "type":"function", "function":{"name":"read_file", "arguments":"{}"}}]}),
                json!({"role":"tool", "tool_call_id":"read-call", "content":"source evidence", "recovery_focus":{"version":1,"state":{"pending":null}}}),
                user_message("runtime continuation", MessageOrigin::Runtime),
            ];
            let cfg = RunConfig {
                provider,
                model: "local-mock".into(),
                chat_model: "local-mock".into(),
                code_model: "local-mock".into(),
                api_key: None,
                base_url: server.base_url(),
                mode: crate::modes::Mode::Vibe,
                persona: String::new(),
                temperature: 0.0,
                max_tokens: 32,
                timeout_seconds: 5,
                hf_device: "cpu".into(),
                hf_local_only: true,
            };
            let (tx, mut rx) = mpsc::channel(32);
            stream_openai_compat_json(&reqwest::Client::new(), &cfg, &messages, None, tx)
                .await
                .unwrap();
            request.assert_hits(1);
            let mut text = String::new();
            while let Some(token) = rx.recv().await {
                if let StreamToken::Delta(delta) = token {
                    text.push_str(&delta);
                }
            }
            assert_eq!(text, "ok");
            assert_eq!(messages[1]["origin"], "user");
            assert_eq!(messages[4]["origin"], "runtime");
            assert_eq!(messages[3]["recovery_focus"]["version"], 1);
        }
    }

    #[test]
    fn normalize_mistral_messages_drops_empty_assistant_without_tool_calls() {
        let input = vec![
            json!({"role":"system","content":"one"}),
            json!({"role":"user","content":"hello"}),
            json!({"role":"assistant","content":""}),
            json!({"role":"system","content":"later note"}),
        ];

        let out = normalize_mistral_messages(&input);

        assert_eq!(out.len(), 3);
        assert_eq!(out[0]["role"], "system");
        assert_eq!(out[1]["role"], "user");
        assert_eq!(out[1]["content"], "hello");
        assert_eq!(out[2]["role"], "user");
        assert_eq!(out[2]["content"], "[System note]\nlater note");
    }

    #[test]
    fn normalize_mistral_messages_keeps_assistant_tool_call_turns() {
        let input = vec![
            json!({"role":"system","content":"one"}),
            json!({"role":"user","content":"hello"}),
            json!({
                "role":"assistant",
                "content":"",
                "tool_calls":[{"id":"call_1","type":"function","function":{"name":"exec","arguments":"{}"}}]
            }),
        ];

        let out = normalize_mistral_messages(&input);

        assert_eq!(out.len(), 3);
        assert_eq!(out[2]["role"], "assistant");
        assert!(out[2]["tool_calls"].is_array());
    }

    #[test]
    fn mistral_retry_budget_is_deeper() {
        assert_eq!(max_connect_retries(&ProviderKind::Mistral), 5);
        assert_eq!(max_connect_retries(&ProviderKind::Anthropic), 4);
    }

    #[test]
    fn mistral_429_backoff_has_floor() {
        let d = backoff_delay(
            &ProviderKind::Mistral,
            Some(reqwest::StatusCode::TOO_MANY_REQUESTS),
            0,
            None,
        );
        assert!(d >= Duration::from_secs(5));
    }

    #[test]
    fn empty_body_bad_request_is_marked_retryable() {
        assert!(should_retry_empty_bad_request(
            reqwest::StatusCode::BAD_REQUEST,
            ""
        ));
        assert!(should_retry_empty_bad_request(
            reqwest::StatusCode::BAD_REQUEST,
            "   "
        ));
        assert!(!should_retry_empty_bad_request(
            reqwest::StatusCode::BAD_REQUEST,
            "tool_choice unsupported"
        ));
    }

    #[test]
    fn format_provider_http_error_mentions_empty_body() {
        let msg =
            format_provider_http_error("OpenAI-compatible", reqwest::StatusCode::BAD_REQUEST, "");
        assert!(msg.contains("HTTP 400 Bad Request"));
        assert!(msg.contains("empty response body"));
    }
}
