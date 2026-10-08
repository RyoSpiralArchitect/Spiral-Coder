//! Opaque provider continuation data belongs to the original tool call.
//! It is neither model text nor execution evidence and must not be rewritten.
use super::ToolCallData;
use anyhow::{bail, Result};
use serde_json::{json, Value};
use std::fmt;

#[derive(Clone, PartialEq, Eq)]
pub struct ThoughtSignature(String);

impl fmt::Debug for ThoughtSignature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ThoughtSignature([opaque])")
    }
}

impl ThoughtSignature {
    pub(crate) fn from_call(call: &Value) -> Result<Option<Self>> {
        let Some(value) = call.pointer("/extra_content/google/thought_signature") else {
            return Ok(None);
        };
        match value.as_str().filter(|value| !value.is_empty()) {
            Some(value) => Ok(Some(Self(value.to_owned()))),
            None => bail!("invalid Google thought signature on tool call"),
        }
    }
}

impl ToolCallData {
    pub(crate) fn to_json(&self) -> Value {
        let mut call = json!({
            "id": self.id,
            "type": "function",
            "function": {"name": self.name, "arguments": self.arguments},
        });
        if let Some(signature) = &self.thought_signature {
            call["extra_content"] = json!({"google": {"thought_signature": signature.0}});
        }
        call
    }

    /// Rewriting a signed call would misrepresent the provider's original turn.
    pub(crate) fn can_rewrite(&self) -> bool {
        self.thought_signature.is_none()
    }
}

pub(crate) fn is_google_endpoint(base_url: &str) -> bool {
    reqwest::Url::parse(base_url).is_ok_and(|url| {
        url.scheme() == "https" && url.host_str() == Some("generativelanguage.googleapis.com")
    })
}

pub(crate) fn tool_choice(base_url: &str) -> &'static str {
    // Gemini's required/ANY mode suppresses the text prelude that the governor
    // needs for plan/think/evidence. AUTO permits text plus a function call.
    if is_google_endpoint(base_url) {
        "auto"
    } else {
        "required"
    }
}

/// Preserve session data locally, but send Google continuation data only to Google.
pub(crate) fn prepare_messages(messages: &[Value], base_url: &str) -> Vec<Value> {
    let mut messages = crate::task_origin::provider_messages(messages);
    if !is_google_endpoint(base_url) {
        for message in &mut messages {
            let Some(calls) = message.get_mut("tool_calls").and_then(Value::as_array_mut) else {
                continue;
            };
            for call in calls {
                let Some(extra) = call.get_mut("extra_content").and_then(Value::as_object_mut)
                else {
                    continue;
                };
                extra.remove("google");
                if extra.is_empty() {
                    call.as_object_mut().unwrap().remove("extra_content");
                }
            }
        }
    }
    messages
}

pub(crate) fn has_signed_call(message: &Value) -> bool {
    message
        .get("tool_calls")
        .and_then(Value::as_array)
        .is_some_and(|calls| {
            calls.iter().any(|call| {
                call.pointer("/extra_content/google/thought_signature")
                    .is_some()
            })
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_switch_does_not_forward_signature_or_mutate_saved_history() {
        let messages = vec![json!({
            "role": "assistant", "content": "plan", "origin": "runtime",
            "tool_calls": [{"id":"call-1", "type":"function",
                "function":{"name":"read_file", "arguments":"{\"path\":\"src/lib.rs\"}"},
                "extra_content":{"google":{"thought_signature":"opaque-test-signature"}}}]
        })];
        let original = messages.clone();
        let google = prepare_messages(
            &messages,
            "https://generativelanguage.googleapis.com/v1beta/openai",
        );
        assert_eq!(
            tool_choice("https://generativelanguage.googleapis.com/v1beta/openai"),
            "auto"
        );
        assert_eq!(google[0]["tool_calls"], messages[0]["tool_calls"]);
        assert!(google[0].get("origin").is_none());
        for url in [
            "https://api.openai.com/v1",
            "https://api.mistral.ai/v1",
            "http://localhost:8000",
            "https://generativelanguage.googleapis.com.evil.test/v1",
            "https://example.test/generativelanguage.googleapis.com",
            "http://generativelanguage.googleapis.com/v1beta/openai",
        ] {
            assert_eq!(tool_choice(url), "required");
            let prepared = prepare_messages(&messages, url);
            assert_eq!(
                prepared[0]["tool_calls"][0],
                json!({"id":"call-1", "type":"function",
                "function":{"name":"read_file", "arguments":"{\"path\":\"src/lib.rs\"}"}})
            );
        }
        assert_eq!(messages, original);
    }
}
