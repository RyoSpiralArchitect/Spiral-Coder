//! Local message provenance keeps runtime feedback from replacing human intent.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageOrigin {
    User,
    Runtime,
}

pub fn user_message(content: impl Into<String>, origin: MessageOrigin) -> Value {
    json!({"role": "user", "content": content.into(), "origin": origin})
}

fn is_human_request(message: &Value) -> bool {
    // Legacy sessions have no origin. Preserve their user messages, including
    // text that happens to look like a runtime marker. Never infer authority
    // from the prose itself or silently discard an unknown future origin.
    message["role"].as_str() == Some("user")
        && serde_json::from_value::<MessageOrigin>(message["origin"].clone()).ok()
            != Some(MessageOrigin::Runtime)
        && message["content"]
            .as_str()
            .is_some_and(|text| !text.trim().is_empty())
}

pub fn current_task_messages(messages: &[Value]) -> &[Value] {
    messages
        .iter()
        .rposition(is_human_request)
        .map(|index| &messages[index..])
        .unwrap_or(messages)
}

pub fn root_user_text(messages: &[Value]) -> &str {
    current_task_messages(messages)
        .iter()
        .find(|message| is_human_request(message))
        .and_then(|message| message["content"].as_str())
        .unwrap_or("")
}

/// Local provenance and recovery snapshots never enter the provider wire schema.
pub fn provider_messages(messages: &[Value]) -> Vec<Value> {
    messages
        .iter()
        .cloned()
        .map(|mut message| {
            if let Some(object) = message.as_object_mut() {
                object.remove("origin");
                object.remove("recovery_focus");
            }
            message
        })
        .collect()
}

pub fn continuation_prompt(lang: &str) -> &'static str {
    match lang.trim().to_ascii_lowercase().as_str() {
        "fr" => "Reprends la tâche inachevée depuis l’état précédent, en conservant les exigences initiales.",
        "en" => "Resume the unfinished task from the previous state, preserving its original requirements.",
        _ => "前回の状態から未完了の作業を再開して。元の依頼の要件を維持して。",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_session::AgentSession;

    fn continuation_message(lang: &str) -> Value {
        user_message(continuation_prompt(lang), MessageOrigin::Runtime)
    }

    #[test]
    fn resume_round_trip_preserves_root_after_assistant_tool_and_runtime_tails() {
        let root =
            "Read only: locate the handler. Final answer must include `verified by inspection`.";
        for tail in [
            json!({"role":"assistant", "content":"Still inspecting"}),
            json!({"role":"tool", "tool_call_id":"read", "content":"source"}),
            user_message("[implied_exec]\nsource output", MessageOrigin::Runtime),
            user_message("[goal_check] tests passed", MessageOrigin::Runtime),
        ] {
            let mut messages = vec![json!({"role":"user", "content":root}), tail];
            messages.push(continuation_message("en"));
            let session = AgentSession::new(None, None, None, None, messages);
            let reloaded: AgentSession =
                serde_json::from_str(&serde_json::to_string(&session).unwrap()).unwrap();
            assert_eq!(root_user_text(&reloaded.messages), root);
            assert_eq!(reloaded.version, AgentSession::VERSION);
        }
    }

    #[test]
    fn genuine_steering_replaces_root_even_when_it_looks_like_runtime_text() {
        let old = user_message("Old task", MessageOrigin::User);
        for steering in [
            user_message("[implied_exec] Explain this output", MessageOrigin::User),
            json!({"role":"user", "content":"[goal_check] Diagnose this failure"}),
            json!({"role":"user", "content":"Continue with the new scope", "origin":"future"}),
        ] {
            let messages = vec![old.clone(), continuation_message("ja"), steering.clone()];
            assert_eq!(
                root_user_text(&messages),
                steering["content"].as_str().unwrap()
            );
            assert_eq!(current_task_messages(&messages), &[steering]);
        }
    }

    #[test]
    fn runtime_feedback_alone_cannot_create_a_human_task() {
        assert_eq!(root_user_text(&[continuation_message("en")]), "");
    }

    #[test]
    fn provider_messages_strip_only_local_metadata_without_mutating_session() {
        let messages = vec![
            user_message("task", MessageOrigin::User),
            json!({"role":"assistant", "tool_calls":[{"id":"call", "function":{"name":"read_file", "arguments":"{}"}}]}),
            json!({"role":"tool", "tool_call_id":"call", "content":"source", "recovery_focus":{"version":1,"state":{"pending":null}}}),
            continuation_message("en"),
        ];
        let mut expected = messages.clone();
        expected[0].as_object_mut().unwrap().remove("origin");
        expected[3].as_object_mut().unwrap().remove("origin");
        expected[2]
            .as_object_mut()
            .unwrap()
            .remove("recovery_focus");
        assert_eq!(provider_messages(&messages), expected);
        assert_eq!(messages[0]["origin"], "user");
        assert_eq!(messages[3]["origin"], "runtime");
        assert_eq!(messages[2]["recovery_focus"]["version"], 1);
    }
}
