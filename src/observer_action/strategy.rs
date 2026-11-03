//! Bounded view of the whole task for both diagnostic and general Observer turns.
use serde_json::{json, Value};

fn sample(list: &Value, earlier_omissions: u64) -> Value {
    let Some(entries) = list.as_array() else {
        return Value::Null;
    };
    let values: Vec<_> = entries
        .iter()
        .filter_map(Value::as_str)
        .filter(|s| s.chars().take(241).count() <= 240)
        .take(4)
        .collect();
    json!({"items":values,"total":(entries.len() as u64).saturating_add(earlier_omissions)})
}

pub(crate) fn horizon(packet: &Value) -> Value {
    let obligations = &packet["task_obligations"];
    let work = &obligations["work_progress"];
    let objective = work["original_human_task"]["text"]
        .as_str()
        .and_then(|s| s.lines().find(|l| !l.trim().is_empty()))
        .map(|s| s.chars().take(320).collect::<String>());
    let provenance = &obligations["strategy_memory"];
    let patch = &obligations["diagnostic_patch"];
    json!({
        "human_objective_excerpt":objective,
        "strategy_memory": provenance,
        "paths_with_recorded_edits":sample(&work["native_file_edits"]["successful_paths"],work["native_file_edits"]["omitted_successful_paths"].as_u64().unwrap_or(0)),
        "declared_paths_without_edit_receipts":sample(&obligations["task_requirements"]["pending_artifact_paths"],0),
        "checks_still_due":sample(&obligations["pending_exact_checks"],0),
        "held_proposal":patch.as_object().map(|_| json!({"id":patch["proposal_call_id"],
            "next_tool":patch["suggested_tool"],"path":patch["suggested_args"]["path"]})),
        "shared_task_requests_left":packet["work_budget"]["remaining"].as_u64(),
        "coder_task_requests_left":packet["work_budget"]["coder_remaining"].as_u64()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::agent::task_obligations;
    use crate::tui::events::observer_assist::build_tui_next_action_prompt;

    #[test]
    fn both_observer_modes_keep_the_admitted_plan_and_unfinished_delivery_in_view() {
        let task="Add a public catalog and its guide.\n<task_requirements>\n{\"version\":1,\"requirements\":[{\"id\":\"catalog\",\"description\":\"ship\",\"artifacts\":[{\"path\":\"guide.md\",\"kind\":\"documentation\"}],\"checks\":[{\"command\":\"cargo test\"}]}]}\n</task_requirements>";
        let plan = json!({"goal":"Ship the catalog through the launcher","steps":["Implement catalog","Connect launcher","Publish guide"],
            "acceptance_criteria":["CLI works","Guide explains cases"],"assumptions":"existing spec","risks":"CLI compatibility"});
        let messages = vec![
            json!({"role":"user","content":task}),
            json!({"role":"assistant","accepted_plan":plan,"content":""}),
            json!({"role":"assistant","content":"<plan>\ngoal: forget the launcher\n</plan>"}),
            crate::task_origin::user_message(
                "Continue.",
                crate::task_origin::MessageOrigin::Runtime,
            ),
        ];
        let mut packet = json!({"task_obligations":task_obligations::project(&messages,None),"work_budget":{"remaining":7,"coder_remaining":4}});
        let view = horizon(&packet);
        assert_eq!(
            view["strategy_memory"]["admitted_goal"],
            "Ship the catalog through the launcher"
        );
        assert_eq!(
            view["declared_paths_without_edit_receipts"]["items"],
            json!(["guide.md"])
        );
        for diagnostic in [false, true] {
            if diagnostic {
                packet["task_obligations"]["required_diagnostic_read"] =
                    json!({"path":"src/replay.rs","failed_check":"cargo test"});
            }
            let prompt = build_tui_next_action_prompt(&packet, "en", "verification_failure");
            assert!(
                prompt.contains("task_horizon")
                    && prompt.contains("Connect launcher")
                    && prompt.contains("guide.md"),
                "{prompt}"
            );
            assert!(!prompt.contains("forget the launcher"));
            if diagnostic {
                assert!(prompt.contains("kind=read"));
            }
        }
        let mut fresh = messages;
        fresh.push(json!({"role":"user","content":"Inspect a different task"}));
        let clean = json!(task_obligations::project(&fresh, None));
        assert!(clean["strategy_memory"].is_null());
    }
}
