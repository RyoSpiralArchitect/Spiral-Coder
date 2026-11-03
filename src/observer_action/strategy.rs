//! Bounded view of the whole task for both diagnostic and general Observer turns.
use crate::tui::agent::task_obligations::TaskObligations;
use crate::work_budget::WorkState;
use serde_json::{json, Value};

pub(crate) fn native_budget(state: &WorkState) -> Value {
    json!({"limit":state.request_limit,"used":state.requests.len(),
        "remaining":state.request_limit.saturating_sub(state.task_requests()),
        "coder_remaining":state.coder_remaining()})
}

/// Distinguish a pre-send estimate from capacity observed after a charged reply.
pub(crate) fn project_budget(
    budget: &mut Value,
    obligations: Option<&TaskObligations>,
    before_send: bool,
) -> Option<usize> {
    let fields = budget.as_object_mut()?;
    fields.remove("next_coder_turn");
    fields.remove("next_coder_requests");
    let shared = fields.get("remaining")?.as_u64()?;
    let coder = fields.get("coder_remaining")?.as_u64()?;
    let next = usize::try_from(coder.min(shared.saturating_sub(u64::from(before_send)))).ok()?;
    fields.insert("next_coder_requests".into(), json!(next));
    fields.insert(
        "next_coder_turn".into(),
        json!(obligations.map(|o| o.coder_turn(next))),
    );
    Some(next)
}

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
        "coder_task_requests_left":packet["work_budget"]["coder_remaining"].as_u64(),
        "next_coder_requests":packet["work_budget"]["next_coder_requests"].as_u64(),
        "next_coder_turn":packet["work_budget"]["next_coder_turn"].as_str()
    })
}

pub(super) fn needs_handoff(packet: &Value) -> bool {
    let turn = &packet["work_budget"]["next_coder_turn"];
    let o = &packet["task_obligations"];
    turn == "exhausted"
        || (turn == "report_only"
            && ([
                &o["pending_exact_checks"],
                &o["pending_recovery"],
                &o["task_requirements"]["pending"],
                &o["task_requirements"]["pending_baselines"],
            ]
            .iter()
            .any(|gaps| gaps.as_array().is_some_and(|rows| !rows.is_empty()))
                || !o["required_diagnostic_read"].is_null()
                || !o["required_recheck"].is_null()))
}

pub(crate) fn handoff_prompt(packet: &Value, language: &str) -> Option<String> {
    if !needs_handoff(packet) {
        return None;
    }
    let remaining = if packet["work_budget"]["next_coder_turn"] == "exhausted" {
        "There is no Coder request available to execute a suggestion in this episode."
    } else {
        "The next Coder turn is reserved for its own final report. An unfinished report must retain the remaining criteria and a concrete next step."
    };
    let obligations = &packet["task_obligations"];
    let facts = json!({"task_horizon":horizon(packet),"required_diagnostic_read":obligations["required_diagnostic_read"],
        "required_recheck":obligations["required_recheck"],
        "latest_native_feedback":obligations["latest_action_feedback"]["result"].as_str().map(|s| s.chars().take(450).collect::<String>())});
    Some(format!("Write a handoff in {language}. {remaining} Identify what is evidenced, what remains unverified and the first action for a future continuation. Place that future action in the explanation. Return one JSON object with summary, response_contract (required=false, focus_axes as a list of strings, note as a string), and one suggestions entry with kind=clarify, suggested_tool=null, suggested_args={{}}, reason, confidence and based_on. Use the native task data below.\n<packet>\n{facts}\n</packet>"))
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

#[cfg(test)]
mod capacity_tests {
    use super::super::{diagnostic::Output, evaluate, Request};
    use serde_json::{json, Value};

    fn packet(counters: Value, response: Option<&str>, declared: bool) -> Request {
        let task = if declared {
            "Document the catalog.\n<task_requirements>\n{\"version\":1,\"requirements\":[{\"id\":\"docs\",\"description\":\"document\",\"artifacts\":[{\"path\":\"guide.md\",\"kind\":\"documentation\"}]}]}\n</task_requirements>"
        } else {
            "Inspect the guide."
        };
        serde_json::from_value(json!({"packet":{"work_budget":counters},
            "messages":[{"role":"user","content":task}], "response":response}))
        .unwrap()
    }

    fn read() -> Value {
        json!({"kind":"read","reason":"inspect before editing","confidence":0.95,
            "suggested_tool":"read_file","suggested_args":{"path":"guide.md"}})
    }

    fn clarify() -> Value {
        json!({"kind":"clarify","confidence":0.95,"reason":"On resume inspect the guide.","suggested_tool":null,"suggested_args":{}})
    }

    #[test]
    fn capacity_honors_both_counters_and_the_native_final_report_policy() {
        let advice = json!({"suggestions":[read()]}).to_string();
        for (shared, coder, declared, turn, forwarded) in [
            (1, 1, true, "exhausted", false),
            (8, 0, true, "exhausted", false),
            (2, 2, true, "report_only", false),
            (3, 2, true, "work", true),
            (2, 2, false, "work", true),
        ] {
            let result = evaluate(packet(
                json!({"remaining":shared,"coder_remaining":coder}),
                Some(&advice),
                declared,
            ));
            assert_eq!(result.packet["work_budget"]["next_coder_turn"], turn);
            assert_eq!(result.issue.is_none() && result.hint.is_some(), forwarded);
            assert_eq!(result.prompt.contains("Write a handoff"), !forwarded);
        }
        let result = evaluate(packet(
            json!({"next_coder_turn":"exhausted","next_coder_requests":0}),
            Some(&advice),
            true,
        ));
        assert!(result.packet["work_budget"]["next_coder_turn"].is_null());
        assert!(result.issue.is_none() && result.hint.is_some());
    }

    #[test]
    fn exhausted_handoff_overrides_diagnostic_read_and_creates_no_queued_work() {
        let c = json!({"remaining":1,"coder_remaining":0});
        let result = evaluate(packet(
            c.clone(),
            Some(&json!({"suggestions":[clarify()]}).to_string()),
            true,
        ));
        assert!(
            result.issue.is_none()
                && result.envelope.is_some()
                && result.hint.is_none()
                && result.response_contract.is_none()
        );
        for raw in [
            json!({"suggestions":[clarify(),read()]}),
            json!({"response_contract":{"required":true},"suggestions":[clarify()]}),
            json!({"suggestions":[{"kind":"edit","suggested_tool":"patch_file","suggested_args":{"path":"guide.md"}}]}),
        ] {
            let result = evaluate(packet(c.clone(), Some(&raw.to_string()), true));
            assert!(
                result
                    .issue
                    .as_deref()
                    .unwrap()
                    .contains("No Coder request remains")
                    && result.hint.is_none()
            );
        }
        let mut view = result.packet;
        view["task_obligations"]["required_diagnostic_read"] = json!({"path":"src/failure.rs"});
        assert_eq!(
            Output::for_packet(&view, "verification_failure"),
            Output::General
        );
        assert!(super::handoff_prompt(&view, "English")
            .unwrap()
            .contains("src/failure.rs"));
        view["work_budget"]["next_coder_turn"] = json!("work");
        assert_eq!(
            Output::for_packet(&view, "verification_failure"),
            Output::Diagnostic
        );
    }

    #[tokio::test]
    async fn browser_uses_live_remaining_after_reply_without_double_charging_the_observer() {
        use crate::work_budget::{scope, Context, WorkBudget};
        let budget = WorkBudget::new("task", 2, None);
        let context = Some(Context {
            budget: budget.clone(),
            role: "observer",
        });
        let spoofed = json!({"remaining":100,"coder_remaining":100});
        let before = scope(context.clone(), async {
            evaluate(packet(spoofed.clone(), None, false))
        })
        .await;
        assert_eq!(before.packet["work_budget"]["next_coder_requests"], 1);
        drop(budget.begin("observer", &json!({})).unwrap());
        let raw = json!({"suggestions":[read()]}).to_string();
        let after = scope(context, async {
            evaluate(packet(spoofed, Some(&raw), false))
        })
        .await;
        assert_eq!(after.packet["work_budget"]["next_coder_requests"], 1);
        assert!(after.issue.is_none() && after.hint.is_some());
    }
}
