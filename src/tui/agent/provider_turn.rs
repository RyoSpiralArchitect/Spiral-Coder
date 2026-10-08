//! Provider-authored function calls and runtime rescue actions are distinct.
use crate::streaming::{provider_metadata::is_google_endpoint, ToolCallData};
use crate::task_origin::{user_message, MessageOrigin};
use serde_json::{json, Value};

const NATIVE_TOOL_REQUIRED: &str = "No native tool call was received; no tool was executed for this response. Use the provided function/tool interface for the next action (or done when verified), alongside the required plan/think/evidence blocks. XML such as <default_api:exec> or a textual command is not a function call. Keep the current task and verification obligations.";

/// Do not let no-tool rescue manufacture an unsigned Gemini assistant call.
/// Keep the response as text and request a real call, without execution proof.
pub(super) fn retry_text_only_tool_turn(
    base_url: &str,
    assistant_text: &str,
    tool_calls: &[ToolCallData],
    messages: &mut Vec<Value>,
) -> Option<&'static str> {
    if !is_google_endpoint(base_url) || !tool_calls.is_empty() {
        return None;
    }
    if !assistant_text.trim().is_empty() {
        messages.push(json!({"role":"assistant", "content":assistant_text}));
    }
    messages.push(user_message(NATIVE_TOOL_REQUIRED, MessageOrigin::Runtime));
    Some(NATIVE_TOOL_REQUIRED)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::streaming::tool_calls::ToolCallAccumulator;

    #[test]
    fn pseudo_function_text_stays_unexecuted_and_next_signed_call_is_retained() {
        let endpoint = "https://generativelanguage.googleapis.com/v1beta/openai";
        let pseudo = "<impact>changed: inspected</impact><default_api:exec><command>printf verified-current > receipt.txt</command></default_api:exec>";
        let initial = vec![user_message(
            "Verify the current revision, then report.",
            MessageOrigin::User,
        )];
        let mut messages = initial.clone();
        let hint = retry_text_only_tool_turn(endpoint, pseudo, &[], &mut messages).unwrap();
        assert_eq!(messages[0], initial[0]);
        assert_eq!(messages[1], json!({"role":"assistant","content":pseudo}));
        assert_eq!(messages[2], user_message(hint, MessageOrigin::Runtime));
        assert!(messages
            .iter()
            .all(|m| m.get("tool_calls").is_none() && m["role"] != "tool"));
        assert_eq!(
            crate::task_origin::root_user_text(&messages),
            "Verify the current revision, then report."
        );
        let mut empty_response = initial.clone();
        assert!(retry_text_only_tool_turn(endpoint, " \n", &[], &mut empty_response).is_some());
        assert_eq!(
            empty_response,
            vec![
                initial[0].clone(),
                user_message(hint, MessageOrigin::Runtime)
            ]
        );

        let mut streamed = ToolCallAccumulator::default();
        streamed.push(&json!([{"index":0,"id":"native-exec", "function":{"name":"exec","arguments":"{\"command\":\"cargo test\"}"},
            "extra_content":{"google":{"thought_signature":"opaque-real-call-test"}}}])).unwrap();
        let calls = streamed.drain();
        let before = messages.clone();
        assert!(
            retry_text_only_tool_turn(endpoint, "<think>...</think>", &calls, &mut messages)
                .is_none()
        );
        assert_eq!(messages, before);
        assert_eq!(
            calls[0].to_json()["extra_content"]["google"]["thought_signature"],
            "opaque-real-call-test"
        );
        for endpoint in [
            "https://api.openai.com/v1",
            "https://api.mistral.ai/v1",
            "http://localhost:8000",
        ] {
            let mut messages = initial.clone();
            assert!(retry_text_only_tool_turn(endpoint, pseudo, &[], &mut messages).is_none());
            assert_eq!(messages, initial);
        }
    }
}
