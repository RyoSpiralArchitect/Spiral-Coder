//! Small task-local strategy cues. Historical choices never count as edit or check proof.
use super::{compact_one_line, validate_plan, PlanBlock};
use serde::Serialize;
use serde_json::{json, Value};

#[derive(Default, Debug, Serialize)]
struct Recall {
    past_approach: Option<String>,
    matched_historical_action: Option<String>,
    repeated_observation: Option<String>,
}

impl Recall {
    fn from_task(task: &[Value]) -> Self {
        let local = &task[0]["strategy_recall"];
        let bounded = |v: &Value| v.as_str().map(|s| compact_one_line(s, 160));
        let mut recall = Self::default();
        if local["version"] == 1 {
            recall.past_approach = bounded(&local["past_approach"]);
            recall.matched_historical_action = bounded(&local["matched_historical_action"]);
            recall.repeated_observation = bounded(&local["repeated_observation"])
                .or_else(|| bounded(&local["repeated_dead_end"]));
        }
        if let Some(bridge) = crate::agent_session::session_bridge_from_messages(task) {
            if let Some(approach) = bridge.accepted_strategies.first() {
                recall.past_approach = Some(compact_one_line(&approach.next_minimal_action, 160));
                recall.matched_historical_action =
                    Some(compact_one_line(&approach.matched_command, 160));
            }
            if let Some(dead_end) = bridge.repeated_dead_ends.first() {
                recall.repeated_observation = Some(compact_one_line(&dead_end.command, 160));
            }
        }
        recall
    }

    fn has_history(&self) -> bool {
        self.past_approach.is_some() || self.repeated_observation.is_some()
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

impl View {
    pub(super) fn standalone_prompt(&self) -> Option<String> {
        self.history.matched_historical_action.is_some().then(|| format!(
            "[Task strategy recall]\nUse these earlier choices as context when selecting your next action. Check them against current evidence.\n{}",
            serde_json::to_string(self).expect("strategy recall serializes")
        ))
    }
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
        let rendered = super::super::task_obligations::prompt(&restored, None, None).unwrap();
        assert!(
            rendered.contains("[Task strategy recall]")
                && rendered.contains("patch_file(path=src/lib.rs)")
        );
        restored.push(json!({"role":"user","content":"Repair the parser"}));
        assert!(project(&restored).is_none());
        assert!(super::super::task_obligations::prompt(&restored, None, None).is_none());
    }
    #[test]
    fn refused_or_failed_commands_never_seed_a_recalled_strategy() {
        for output in [
            "GOVERNOR BLOCKED\nexample: OK (exit_code: 0)",
            "FAILED (exit_code: 0)\nstdout:\nOK (exit_code: 0)",
        ] {
            let mut m = vec![
                json!({"role":"user","content":"Repair the parser"}),
                json!({"role":"assistant","content":"<reflect>\nlast_outcome: failure\ngoal_delta: same\nwrong_assumption: another search would help\nstrategy_change: adjust\nnext_minimal_action: cargo test\n</reflect>"}),
                json!({"role":"assistant","tool_calls":[{"id":"check","function":{"name":"exec","arguments":"{\"command\":\"cargo test\"}"}}]}),
                json!({"role":"tool","tool_call_id":"check","content":output}),
            ];
            retain_for_resume(&mut m);
            assert!(m[0].get("strategy_recall").is_none(), "{output}");
            assert!(crate::agent_session::session_bridge_from_messages(&m).is_none());
        }
    }
    #[test]
    fn inspecting_two_regions_of_one_file_is_not_a_repeated_observation() {
        let mut m = vec![json!({"role":"user","content":"Repair the two entries"})];
        let record = |m: &mut Vec<Value>, id: &str, start: usize, end: usize| {
            let args = json!({"path":"src/main.rs","start_line":start,"end_line":end});
            m.push(json!({"role":"assistant","tool_calls":[{"id":id,"function":{"name":"read_file","arguments":args.to_string()}}]}));
            m.push(json!({"role":"tool","tool_call_id":id,"content":format!("[src/main.rs] (700 lines, 1400 bytes) [range: lines {start}-{end}; partial file]\n...")}));
        };
        record(&mut m, "declaration", 326, 342);
        record(&mut m, "dispatch", 580, 650);
        assert!(project(&m).is_none());
        record(&mut m, "dispatch-again", 580, 650);
        assert_eq!(
            json!(project(&m))["repeated_observation"],
            "read_file(path=src/main.rs, lines=580-650)"
        );
    }
}

#[cfg(test)]
mod attribution_tests {
    use serde_json::json;

    #[test]
    fn mismatched_targets_and_native_actions_never_become_a_remembered_repair() {
        let mut history = vec![
            json!({"role":"user","content":"Repair the parser"}),
            json!({"role":"assistant","content":"<reflect>\nlast_outcome: failure\ngoal_delta: same\nwrong_assumption: another search would help\nstrategy_change: adjust\nnext_minimal_action: patch_file src/lib.rs. Then verify.\n</reflect>"}),
        ];
        for (id, tool, path, result) in [
            (
                "wrong",
                "patch_file",
                "src/lib.rs.bak",
                "OK: patched 'src/lib.rs.bak'",
            ),
            (
                "read",
                "read_file",
                "src/lib.rs",
                "[src/lib.rs] (1 lines, 2 bytes)\nok",
            ),
        ] {
            history.extend([
                json!({"role":"assistant","tool_calls":[{"id":id,"function":{"name":tool,"arguments":json!({"path":path}).to_string()}}]}),
                json!({"role":"tool","tool_call_id":id,"content":result}),
            ]);
        }
        assert!(super::project(&history).is_none());
        history.extend([
            json!({"role":"assistant","tool_calls":[{"id":"right","function":{"name":"patch_file","arguments":r#"{"path":"src/lib.rs"}"#}}]}),
            json!({"role":"tool","tool_call_id":"right","content":"OK: patched 'src/lib.rs'"}),
        ]);
        let saved = crate::agent_session::session_bridge_from_messages(&history).unwrap();
        assert_eq!(
            json!(super::project(&history))["matched_historical_action"],
            "patch_file(path=src/lib.rs)"
        );
        history.push(json!({"role":"user","content":"Repair the parser"}));
        assert!(crate::agent_session::session_bridge_from_messages(&history).is_none());
        assert!(
            super::super::session_bridge::SessionBridgeView::resolve(Some(&saved), &history)
                .prompt()
                .is_none()
        );
    }
}
