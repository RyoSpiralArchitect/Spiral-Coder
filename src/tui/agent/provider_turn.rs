//! Provider-authored function calls and runtime rescue actions are distinct.
use crate::streaming::provider_metadata::is_google_endpoint;
use crate::task_origin::{user_message, MessageOrigin};
use serde_json::Value;

const NATIVE_TOOL_REQUIRED: &str = "No native tool call was received; no tool was executed for this response. Use the provided function/tool interface for the next action (or done when verified), alongside the required plan/think/evidence blocks. XML such as <default_api:exec> or a textual command is not a function call. Keep the current task and verification obligations.";

/// Runtime actions lack a Gemini-authored continuation signature. Existing
/// verified completion paths do not synthesize tools and remain available.
pub(super) fn allows_synthetic_tools(base_url: &str) -> bool {
    !is_google_endpoint(base_url)
}

/// Called only after ordinary no-tool completion gates declined to finalize.
pub(super) fn retry_unfinished_text_turn(
    base_url: &str,
    messages: &mut Vec<Value>,
) -> Option<&'static str> {
    if allows_synthetic_tools(base_url) {
        return None;
    }
    messages.push(user_message(NATIVE_TOOL_REQUIRED, MessageOrigin::Runtime));
    Some(NATIVE_TOOL_REQUIRED)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn unfinished_pseudo_call_stays_text_without_replacing_the_human_task() {
        let endpoint = "https://generativelanguage.googleapis.com/v1beta/openai";
        let pseudo = "<impact>changed: inspected</impact><default_api:exec><command>printf verified-current > receipt.txt</command></default_api:exec>";
        let initial = vec![
            user_message(
                "Verify the current revision, then report.",
                MessageOrigin::User,
            ),
            json!({"role":"assistant", "content":pseudo}),
        ];
        let mut messages = initial.clone();
        assert!(!allows_synthetic_tools(endpoint));
        let hint = retry_unfinished_text_turn(endpoint, &mut messages).unwrap();
        assert_eq!(&messages[..2], initial.as_slice());
        assert_eq!(messages[2], user_message(hint, MessageOrigin::Runtime));
        assert!(messages
            .iter()
            .all(|m| m.get("tool_calls").is_none() && m["role"] != "tool"));
        assert_eq!(
            crate::task_origin::root_user_text(&messages),
            "Verify the current revision, then report."
        );
        for endpoint in [
            "https://api.openai.com/v1",
            "https://api.mistral.ai/v1",
            "http://localhost:8000",
        ] {
            let mut messages = initial.clone();
            assert!(allows_synthetic_tools(endpoint));
            assert!(retry_unfinished_text_turn(endpoint, &mut messages).is_none());
            assert_eq!(messages, initial);
        }
    }
}
