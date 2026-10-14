//! Runtime-validated plans survive rescue, autosave and Observer continuation.
//! Local metadata is stripped from provider requests and revalidated on resume.
use super::{parse_plan_block, PlanBlock};
use serde_json::Value;

pub(super) fn from_message(message: &Value) -> Option<PlanBlock> {
    if message["role"] != "assistant" {
        return None;
    }
    serde_json::from_value(message["accepted_plan"].clone())
        .ok()
        .or_else(|| parse_plan_block(message["content"].as_str().unwrap_or("")))
}

pub(super) fn latest(messages: &[Value]) -> Option<PlanBlock> {
    crate::task_origin::current_task_messages(messages)
        .iter()
        .rev()
        .find_map(from_message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task_origin::{user_message, MessageOrigin};
    use serde_json::json;

    #[test]
    fn local_plan_survives_session_and_pruning_but_not_a_new_task_or_provider_wire() {
        let plan = PlanBlock {
            goal: "Fix the parser".into(),
            steps: vec!["Read parser".into(), "Patch and verify parser".into()],
            acceptance_criteria: vec!["Parser test passes".into()],
            risks: "Regression".into(),
            assumptions: "Test exposes the error".into(),
        };
        let mut messages = vec![
            user_message("Fix the parser", MessageOrigin::User),
            json!({"role":"assistant","content":"","accepted_plan":plan}),
        ];
        for _ in 0..60 {
            messages.push(json!({"role":"assistant","content":"Continuing"}));
        }
        messages.push(user_message("Continue", MessageOrigin::Runtime));
        super::super::message_window::prune_message_window(&mut messages);
        let session = crate::agent_session::AgentSession::new(None, None, None, None, messages);
        let resumed: crate::agent_session::AgentSession =
            serde_json::from_str(&serde_json::to_string(&session).unwrap()).unwrap();
        assert_eq!(
            serde_json::to_value(latest(&resumed.messages)).unwrap(),
            json!(plan)
        );
        assert!(crate::task_origin::provider_messages(&resumed.messages)
            .iter()
            .all(|m| m.get("accepted_plan").is_none()));
        let mut new_task = resumed.messages;
        new_task.push(user_message("Read another file", MessageOrigin::User));
        assert!(latest(&new_task).is_none());
    }
}
