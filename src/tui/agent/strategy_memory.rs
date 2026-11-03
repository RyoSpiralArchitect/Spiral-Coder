//! Small task-local strategy cues. Historical choices never count as edit or check proof.
use super::{compact_one_line, validate_plan, PlanBlock};
use serde::Serialize;
use serde_json::{json, Value};

#[derive(Default, Debug, Serialize)]
struct Recall {
    past_approach: Option<String>,
    matched_historical_action: Option<String>,
    repeated_dead_end: Option<String>,
}

impl Recall {
    fn from_task(task: &[Value]) -> Self {
        let local = &task[0]["strategy_recall"];
        let bounded = |v: &Value| v.as_str().map(|s| compact_one_line(s, 160));
        let mut recall = Self::default();
        if local["version"] == 1 {
            recall.past_approach = bounded(&local["past_approach"]);
            recall.matched_historical_action = bounded(&local["matched_historical_action"]);
            recall.repeated_dead_end = bounded(&local["repeated_dead_end"]);
        }
        if let Some(bridge) = crate::agent_session::session_bridge_from_messages(task) {
            if let Some(approach) = bridge.accepted_strategies.first() {
                recall.past_approach = Some(compact_one_line(&approach.next_minimal_action, 160));
                recall.matched_historical_action =
                    Some(compact_one_line(&approach.matched_command, 160));
            }
            if let Some(dead_end) = bridge.repeated_dead_ends.first() {
                recall.repeated_dead_end = Some(compact_one_line(&dead_end.command, 160));
            }
        }
        recall
    }

    fn has_history(&self) -> bool {
        self.past_approach.is_some() || self.repeated_dead_end.is_some()
    }
}

pub(super) fn retain_for_resume(messages: &mut [Value]) {
    let Some(start) = messages
        .iter()
        .rposition(crate::task_origin::is_human_request)
    else {
        return;
    };
    let recall = Recall::from_task(&messages[start..]);
    if recall.has_history() {
        let mut local = json!(recall);
        local["version"] = json!(1);
        messages[start]["strategy_recall"] = local;
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct View {
    admitted_goal: Option<String>,
    planned_steps: Vec<String>,
    planned_steps_total: usize,
    #[serde(flatten)]
    history: Recall,
}

pub(super) fn project(messages: &[Value]) -> Option<View> {
    let task = crate::task_origin::current_task_messages(messages);
    if !task
        .first()
        .is_some_and(crate::task_origin::is_human_request)
    {
        return None;
    }
    let plan = task
        .iter()
        .rev()
        .filter(|m| m["role"] == "assistant")
        .filter_map(|m| serde_json::from_value::<PlanBlock>(m.get("accepted_plan")?.clone()).ok())
        .find(|plan| validate_plan(plan).is_ok());
    let history = Recall::from_task(task);
    if plan.is_none() && !history.has_history() {
        return None;
    }
    Some(View {
        admitted_goal: plan.as_ref().map(|p| compact_one_line(&p.goal, 240)),
        planned_steps: plan
            .iter()
            .flat_map(|p| p.steps.iter())
            .take(5)
            .map(|s| compact_one_line(s, 160))
            .collect(),
        planned_steps_total: plan.as_ref().map_or(0, |p| p.steps.len()),
        history,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn matched_approach_survives_lost_reflection_and_stays_with_its_human_task() {
        let mut msgs = vec![
            json!({"role":"user","content":"Repair the parser"}),
            json!({"role":"assistant","content":"<reflect>\nlast_outcome: failure\ngoal_delta: same\nwrong_assumption: another search would help\nstrategy_change: adjust\nnext_minimal_action: patch src/lib.rs with the smallest fix\n</reflect>"}),
            json!({"role":"assistant","tool_calls":[{"id":"patch","function":{"name":"patch_file","arguments":"{\"path\":\"src/lib.rs\"}"}}]}),
            json!({"role":"tool","tool_call_id":"patch","content":"OK: patched 'src/lib.rs'"}),
        ];
        for _ in 0..55 {
            msgs.push(json!({"role":"assistant","content":"Still inspecting."}));
        }
        super::super::prune_old_assistant_messages(&mut msgs);
        assert_eq!(msgs[0]["strategy_recall"]["version"], 1);
        // Only the root and a native continuation survive this round trip.
        let mut restored = vec![
            serde_json::from_slice(&serde_json::to_vec(&msgs[0]).unwrap()).unwrap(),
            crate::task_origin::user_message(
                "Continue.",
                crate::task_origin::MessageOrigin::Runtime,
            ),
        ];
        let view = json!(project(&restored));
        assert_eq!(
            view["matched_historical_action"],
            "patch_file(path=src/lib.rs)"
        );
        assert!(view["past_approach"]
            .as_str()
            .unwrap()
            .contains("smallest fix"));
        assert!(crate::task_origin::provider_messages(&restored)[0]
            .get("strategy_recall")
            .is_none());
        restored.push(json!({"role":"user","content":"Repair the parser"}));
        assert!(project(&restored).is_none());
    }
}
