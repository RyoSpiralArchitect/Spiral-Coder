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

pub(super) fn latest_valid(
    messages: &[Value],
    validate: impl Fn(&PlanBlock) -> bool,
) -> Option<PlanBlock> {
    restore(messages, validate).map(|r| r.plan)
}

pub(super) struct RestoredPlan {
    pub(super) plan: PlanBlock,
    pub(super) closeout_only: bool,
}

pub(super) fn restore(
    messages: &[Value],
    validate: impl Fn(&PlanBlock) -> bool,
) -> Option<RestoredPlan> {
    let task = crate::task_origin::current_task_messages(messages);
    let mut closeout = None;
    for (index, message) in task.iter().enumerate().rev() {
        if super::blocked_report::is_accepted_closeout(task, index) {
            if closeout.is_none() {
                closeout = serde_json::from_value(message["accepted_plan"].clone())
                    .ok()
                    .filter(&validate);
            }
        } else if let Some(plan) = from_message(message).filter(&validate) {
            return Some(RestoredPlan {
                plan,
                closeout_only: false,
            });
        }
    }
    // A closeout can supply the task's first natively admitted plan. Keep its
    // acceptance and admission when there is no earlier validated work plan.
    closeout.map(|plan| RestoredPlan {
        plan,
        closeout_only: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task_origin::{user_message, MessageOrigin};
    use serde_json::json;

    #[test]
    fn only_admitted_plan_survives_an_accepted_blocked_closeout_and_pruning() {
        let plan = PlanBlock {
            goal: "Repair fixture.txt".into(),
            steps: vec![
                "Report incomplete work".into(),
                "List pending checks".into(),
            ],
            acceptance_criteria: vec!["cargo test -q passes".into()],
            risks: "regression".into(),
            assumptions: "fixture exists".into(),
        };
        let mut messages = vec![
            user_message(
                "Repair fixture.txt. Run exactly: cargo test -q",
                MessageOrigin::User,
            ),
            json!({"role":"assistant","accepted_plan":plan,
                "tool_calls":[{"id":"closeout","function":{"name":"done","arguments":"{}"}}]}),
            json!({"role":"tool","tool_call_id":"closeout","content":"OK: blocked",
                "blocked_report":{"remaining_acceptance":["acceptance 1"]}}),
        ];
        for _ in 0..70 {
            messages.push(json!({"role":"assistant","content":"Work remains pending."}));
        }
        super::super::message_window::prune_message_window(&mut messages);
        let restored = restore(&messages, |p| super::super::validate_plan(p).is_ok())
            .expect("the only admitted plan remains available");
        assert!(restored.closeout_only);
        assert_eq!(restored.plan.goal, plan.goal);
        assert_eq!(restored.plan.acceptance_criteria, plan.acceptance_criteria);
        assert_eq!(
            super::super::task_obligations::project(&messages, None).acceptance_criteria,
            super::super::acceptance_report::criteria(Some(&plan)),
        );
    }

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
            serde_json::to_value(latest_valid(&resumed.messages, |_| true)).unwrap(),
            json!(plan)
        );
        assert!(crate::task_origin::provider_messages(&resumed.messages)
            .iter()
            .all(|m| m.get("accepted_plan").is_none()));
        let mut new_task = resumed.messages;
        new_task.push(user_message("Read another file", MessageOrigin::User));
        assert!(latest_valid(&new_task, |_| true).is_none());
    }
}
