//! Reserve one ledger entry per buffered provider request, including retries.
use super::{should_swap_to_max_completion_tokens, swap_max_tokens_to_max_completion_tokens};
use reqwest::StatusCode;
use serde_json::Value;
use std::time::Duration;

pub(super) enum Reply {
    Json(Value),
    Rejected(StatusCode, String),
    Budget(String),
    Failed(String),
}

pub(super) async fn send(
    client: &reqwest::Client,
    url: &str,
    key: Option<&str>,
    timeout: Duration,
    mut payload: Value,
) -> Reply {
    loop {
        let mut request = client.post(url).timeout(timeout).json(&payload);
        if let Some(key) = key.filter(|s| !s.is_empty()) {
            request = request.bearer_auth(key);
        }
        let guard = match crate::work_budget::begin(&payload) {
            Ok(guard) => guard,
            Err(e) => return Reply::Budget(e.to_string()),
        };
        let record = |status: &str| {
            if let Some(g) = &guard {
                g.status(status);
            }
        };
        let response = match request.send().await {
            Ok(response) => response,
            Err(e) => {
                record("send_error");
                return Reply::Failed(e.to_string());
            }
        };
        let status = response.status();
        let body = match response.text().await {
            Ok(body) => body,
            Err(e) => {
                record("body_error");
                return Reply::Failed(e.to_string());
            }
        };
        if status.is_success() {
            match serde_json::from_str::<Value>(&body) {
                Ok(value) => {
                    if let (Some(g), Some(usage)) =
                        (&guard, value.get("usage").filter(|v| !v.is_null()))
                    {
                        g.usage(usage);
                    }
                    record("completed");
                    return Reply::Json(value);
                }
                Err(e) => {
                    record("invalid_response");
                    return Reply::Failed(format!("invalid JSON from API: {e}"));
                }
            }
        }
        record(&format!("http_{}", status.as_u16()));
        if should_swap_to_max_completion_tokens(status, &body)
            && payload.get("max_tokens").is_some()
        {
            swap_max_tokens_to_max_completion_tokens(&mut payload);
        } else if !crate::streaming::provider_metadata::drop_unsupported_chat_option(
            status,
            &body,
            &mut payload,
        ) {
            return Reply::Rejected(status, body);
        }
    }
}
