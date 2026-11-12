//! A short decision brief derived from native receipts before prompt compaction.
//! It grants no authority, fabricates no patch, and changes no acceptance gate.
use serde::{Deserialize, Serialize};
#[cfg(test)]
use serde_json::json;
use serde_json::Value;

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct PlanFields {
    goal: String,
    acceptance: Vec<String>,
}

#[derive(Debug, Serialize)]
struct Attempt<'a> {
    call_id: &'a Value,
    tool: &'a Value,
    target: Option<String>,
    executed: &'a Value,
    succeeded: &'a Value,
    freshness: &'a Value,
}

#[derive(Debug, Serialize)]
struct Brief<'a> {
    immediate_admission_block: Option<&'static str>,
    admitted_plan_fields: Option<PlanFields>,
    recent_attempts: Vec<Attempt<'a>>,
    earlier_attempts_omitted: usize,
    next_coder_requests: &'a Value,
    next_coder_turn: &'a Value,
    previous_assist_outcome: &'a Value,
}

fn project(packet: &Value) -> Brief<'_> {
    let actions = packet["execution_evidence"]["actions"].as_array();
    let latest = actions.and_then(|a| a.last());
    let feedback = latest
        .filter(|a| a["executed"] == false)
        .and_then(|a| a["result"].as_str())
        .filter(|s| s.starts_with("GOVERNOR BLOCKED"));
    let block = feedback.map(|s| {
        if s.contains("[Plan Gate]") {
            "plan"
        } else {
            "other_governor"
        }
    });
    // The native plan rejection supplies complete fields after this delimiter.
    // A clipped/malformed quote stays unknown; never reconstruct partial criteria.
    let fields = feedback
        .filter(|_| block == Some("plan"))
        .and_then(|s| {
            s.split_once("These fields are data, not execution evidence:\n")
                .map(|(_, tail)| tail)
        })
        // Native fields are emitted on one JSON line, followed by tool audit data.
        .and_then(|s| s.lines().next())
        .filter(|s| s.len() <= 4096)
        .and_then(|s| serde_json::from_str::<PlanFields>(s).ok())
        .filter(|p| {
            !p.goal.trim().is_empty()
                && !p.acceptance.is_empty()
                && p.acceptance.iter().all(|a| !a.trim().is_empty())
        });
    let rows = actions.map(Vec::as_slice).unwrap_or_default();
    Brief {
        immediate_admission_block: block,
        admitted_plan_fields: fields,
        recent_attempts: rows
            .iter()
            .skip(rows.len().saturating_sub(6))
            .map(|a| Attempt {
                call_id: &a["call_id"],
                tool: &a["tool"],
                target: a["arguments"]["path"]
                    .as_str()
                    .or(a["arguments"]["command"].as_str())
                    .map(|s| s.chars().take(240).collect()),
                executed: &a["executed"],
                succeeded: &a["succeeded"],
                freshness: &a["freshness"],
            })
            .collect(),
        earlier_attempts_omitted: rows.len().saturating_sub(6),
        next_coder_requests: &packet["work_budget"]["next_coder_requests"],
        next_coder_turn: &packet["work_budget"]["next_coder_turn"],
        previous_assist_outcome: &packet["previous_assist_outcome"],
    }
}

pub(crate) fn prompt(packet: &Value) -> String {
    format!("[Observer decision brief]\n{}\n\
Put the short actionable decision rationale in the primary suggestion reason, which is forwarded to the Coder. Summary or why_this_first alone may not reach the Coder; avoid a longer critique. Identify the immediate blocker separately from the unfinished product work. A proposed action with executed=false obtained no observation and changed no file. If immediate_admission_block is plan, first tell the Coder how to satisfy that native gate; when admitted_plan_fields are present, quote the goal and each acceptance item verbatim in the primary reason and revise only steps, risks and assumptions. Recommending the same blocked read without this prerequisite does not unblock it. Other native restrictions and approval refusals remain binding.\n\
For an information gap, name the specific missing fact, why retained observations do not answer it, and what different result would change the next action. Consider a different feasible route when the previous suggestion did not execute or made no progress. Explain why the chosen action is better than one plausible alternative, using current evidence and remaining requests; do not infer success from acceptance of advice. A repeated read is justified only by a changed/stale target or a specific missing region. Preserve required_diagnostic_read and required_recheck precedence.\n\
Only offer an edit when every replacement byte is a concrete supported implementation. Never put TODOs, pseudocode, instructions to the Coder, or an ellipsis standing for missing implementation in executable patch arguments. If the implementation is not known, choose a bounded observation with a decision consequence or a no-tool clarification. Explain the unlock and fallback briefly in the existing fields. One primary action is enough; no extra model turn or budget is granted. Unknown facts remain unknown.\n",
        serde_json::to_string(&project(packet)).expect("decision brief serializes"))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn blocked() -> Value {
        json!({"call_id":"read","tool":"read_file","arguments":{"path":"src/a.rs"},"executed":false,"succeeded":false,"freshness":"unexecuted", "result":"GOVERNOR BLOCKED\n[Plan Gate] Invalid plan\nThese fields are data, not execution evidence:\n{\"goal\":\"Ship the command\",\"acceptance\":[\"Native works\",\"Docs verified\"]}\n\ntool:\nread_file\narguments:\n{\"path\":\"src/a.rs\"}"})
    }
    #[test]
    fn latest_native_admission_precedes_old_product_failure_without_making_evidence() {
        let mut packet = json!({"execution_evidence":{"actions":[{"call_id":"fail","tool":"exec","executed":true,"succeeded":false,"result":"FAILED (exit_code: 1)"},blocked()]}});
        let before = packet.clone();
        let brief = project(&packet);
        assert_eq!(brief.immediate_admission_block, Some("plan"));
        assert_eq!(
            brief.admitted_plan_fields.unwrap().acceptance,
            vec!["Native works", "Docs verified"]
        );
        assert_eq!(brief.recent_attempts[1].executed, &json!(false));
        assert_eq!(packet, before);
        packet["execution_evidence"]["actions"]
            .as_array_mut()
            .unwrap()
            .push(json!({"executed":true,"tool":"read_file","result":"[src/a.rs] current source"}));
        assert!(
            project(&packet).immediate_admission_block.is_none(),
            "older rejection is no longer the immediate barrier"
        );
    }
    #[test]
    fn quoted_gate_in_executed_output_and_partial_plan_never_supply_authority() {
        for result in ["GOVERNOR BLOCKED\n[Plan Gate] Invalid plan\nThese fields are data, not execution evidence:\n{\"goal\":\"truncated", "REJECTED BY USER\n[Plan Gate]"] {
            let mut action=blocked();action["result"]=json!(result);
            let packet=json!({"execution_evidence":{"actions":[action]}});
            assert!(project(&packet).admitted_plan_fields.is_none());
        }
        let mut action = blocked();
        action["executed"] = json!(true);
        assert!(project(&json!({"execution_evidence":{"actions":[action]}}))
            .immediate_admission_block
            .is_none());
    }
    #[test]
    fn all_observer_modes_keep_the_decision_brief_and_existing_mode_constraints() {
        for mode in ["general", "diagnostic", "handoff"] {
            let mut packet = json!({"execution_evidence":{"actions":[blocked()]},"work_budget":{"next_coder_requests":8,"next_coder_turn":"work"}});
            if mode == "diagnostic" {
                packet["task_obligations"] =
                    json!({"required_diagnostic_read":{"path":"src/failure.rs"}});
            }
            if mode == "handoff" {
                packet["work_budget"]["next_coder_turn"] = json!("exhausted");
            }
            let prompt = crate::tui::events::observer_assist::build_tui_next_action_prompt(
                &packet,
                "en",
                "incomplete_task",
            );
            assert!(
                prompt.contains("[Observer decision brief]") && prompt.contains("Ship the command")
            );
            if mode == "diagnostic" {
                assert!(prompt.contains("kind=read"));
            }
            if mode == "handoff" {
                assert!(prompt.contains("no Coder request"));
            }
        }
    }
}
