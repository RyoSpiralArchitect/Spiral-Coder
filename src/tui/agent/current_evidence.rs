//! Temporal labels for advisory/closeout projections. Historical exchanges stay
//! in the session; none of these labels creates proof or executes a suggestion.
use super::{
    evidence_packet::ActionEvidence, exec_verification::ExecVerificationContext, ExecKind,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Freshness {
    Current,
    Superseded,
    BeforeMutation,
    Unexecuted,
    Pending,
}

pub(super) fn mark(
    actions: &mut [ActionEvidence],
    messages: &[Value],
    test_cmd: Option<&str>,
    mutation: Option<usize>,
) {
    let context = ExecVerificationContext::from_messages(test_cmd, messages);
    // Even an approved check can write a receipt. Never present an earlier read
    // as the latest file content after a subsequent process has run.
    let read_barrier = actions
        .iter()
        .filter(|a| {
            a.executed == Some(true)
                && (a.tool == "exec"
                    || crate::execution_evidence::file_edit_succeeded(
                        &a.tool,
                        a.result.as_deref().unwrap_or(""),
                    ))
        })
        .filter_map(|a| a.result_step)
        .max()
        .unwrap_or(0);
    let mut latest = std::collections::BTreeMap::new();
    let latest_executed = actions
        .iter()
        .filter(|a| a.executed == Some(true))
        .filter_map(|a| a.result_step)
        .max()
        .unwrap_or(0);
    let key = |a: &ActionEvidence| {
        let arguments = if a.tool == "exec" {
            // Use the runtime location header, never a cwd printed in stdout.
            // Selection IDs label a command; they are not distinct executions.
            let cwd = a
                .result
                .as_deref()
                .and_then(|r| r.lines().nth(1))
                .and_then(|line| line.strip_prefix("cwd: "))
                .or_else(|| a.arguments["cwd"].as_str());
            serde_json::json!({"command":a.arguments["command"], "cwd":cwd})
        } else {
            a.arguments.clone()
        };
        (a.tool.clone(), arguments.to_string())
    };
    for action in actions.iter().filter(|a| a.executed == Some(true)) {
        latest
            .entry(key(action))
            .and_modify(|step: &mut usize| *step = (*step).max(action.result_step.unwrap_or(0)))
            .or_insert(action.result_step.unwrap_or(0));
    }
    for action in actions {
        let Some(step) = action.result_step else {
            continue;
        };
        let barrier = if action.tool == "exec"
            && action.arguments["command"]
                .as_str()
                .is_some_and(|cmd| context.classify(cmd).kind == ExecKind::Verify)
        {
            mutation.unwrap_or(0)
        } else if matches!(
            action.tool.as_str(),
            "read_file" | "search_files" | "glob" | "list_dir"
        ) {
            read_barrier
        } else {
            mutation.unwrap_or(0)
        };
        action.freshness = if action.executed != Some(true) {
            Freshness::Unexecuted
        } else if latest
            .get(&key(action))
            .is_some_and(|latest| *latest > step)
        {
            Freshness::Superseded
        } else if action.tool == "exec"
            && step < latest_executed
            && action.arguments["command"]
                .as_str()
                .is_some_and(|cmd| context.classify(cmd).kind == ExecKind::Action)
        {
            Freshness::Superseded
        } else if step < barrier {
            Freshness::BeforeMutation
        } else {
            Freshness::Current
        };
    }
}

/// Prompt view: remove historical outputs and duplicated UI digests so they
/// cannot compete with newer native receipts. Keep the full packet for audit.
pub(crate) fn observer_prompt_packet(packet: &Value) -> Value {
    let mut packet = packet.clone();
    let Some(actions) = packet["execution_evidence"]["actions"].as_array() else {
        return packet;
    };
    let current = actions
        .iter()
        .filter(|a| a["freshness"] == "current")
        .cloned()
        .collect::<Vec<_>>();
    let historical = actions.len() - current.len();
    packet["execution_evidence"]["historical_action_count"] = serde_json::json!(historical);
    packet["execution_evidence"]["actions"] = serde_json::json!(current);
    packet["recent_tool_calls"] = Value::Null;
    packet["recent_tool_results"] = Value::Null;
    packet["recent_assistant_messages"] = Value::Null;
    packet["selected_failure_context"] = packet["actual_outcome"].take();
    if packet["task_obligations"]["report_repair"].is_object() {
        packet["selected_failure_kind"] = packet["failure_kind"].take();
        packet["selected_error_digest"] = packet["last_error_digest"].take();
        packet["current_blocker"] = serde_json::json!({
            "kind":"authored_report",
            "feedback": packet["task_obligations"]["report_repair"]["feedback"],
            "instruction":"Repair done.summary using the native report feedback. Selected historical failures do not require another read, edit or passed check. No-tool clarification remains available when uncertain."
        });
    }
    packet["evidence_precedence"] = serde_json::json!("execution_evidence contains current correlated results only. selected_failure_context and intent risks describe the selected historical failure, not a current file observation. Pending recovery and exact checks are native obligations, not an exhaustive semantic completion claim. Retain work_progress.original_human_task and acceptance_criteria when identifying unmet source work; a documentation-only proof requirement must not erase a requested source repair. work_progress records activity, not current filesystem differences or correctness. Do not repeat a check or read solely because the historical failure says stale.");
    packet
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn exchange(messages: &mut Vec<Value>, id: &str, tool: &str, args: Value, result: &str) {
        messages.extend([
            json!({"role":"assistant","tool_calls":[{"id":id,"function":{"name":tool,"arguments":args.to_string()}}]}),
            json!({"role":"tool","tool_call_id":id,"content":result}),
        ]);
    }
    #[test]
    fn current_receipt_supersedes_stale_history_through_resume_and_observer_prompt() {
        let mut messages = vec![json!({"role":"user","content":"Fix the receipt"})];
        exchange(
            &mut messages,
            "stale",
            "read_file",
            json!({"path":"receipt.txt"}),
            "[receipt.txt] (1 lines, 5 bytes)\nstale",
        );
        exchange(
            &mut messages,
            "edit",
            "exec",
            json!({"command":"printf verified-current > receipt.txt"}),
            "OK (exit_code: 0)",
        );
        exchange(
            &mut messages,
            "fresh",
            "read_file",
            json!({"path":"receipt.txt"}),
            "[receipt.txt] (1 lines, 16 bytes)\nverified-current",
        );
        exchange(
            &mut messages,
            "blocked",
            "exec",
            json!({"command":"touch receipt.txt"}),
            "GOVERNOR BLOCKED",
        );
        let resumed: Vec<Value> =
            serde_json::from_str(&serde_json::to_string(&messages).unwrap()).unwrap();
        let evidence = super::super::evidence_packet::project(&resumed, None);
        assert_eq!(evidence.actions[0].freshness, Freshness::Superseded);
        assert_eq!(evidence.actions[2].freshness, Freshness::Current);
        assert_eq!(evidence.actions[3].freshness, Freshness::Unexecuted);
        let prompt = observer_prompt_packet(
            &json!({"execution_evidence":evidence,"recent_tool_results":["stale"],"actual_outcome":"old failure"}),
        );
        let actions = prompt["execution_evidence"]["actions"].to_string();
        assert!(actions.contains("verified-current"));
        assert!(!actions.contains("stale"));
        assert!(prompt["recent_tool_results"].is_null());
        assert_eq!(prompt["selected_failure_context"], "old failure");
        messages.push(json!({"role":"user","content":"Another task"}));
        assert!(super::super::evidence_packet::project(&messages, None)
            .actions
            .is_empty());
    }
    #[test]
    fn later_failed_execution_invalidates_reads_but_unexecuted_refusal_does_not() {
        let mut messages = vec![json!({"role":"user","content":"Inspect data"})];
        exchange(
            &mut messages,
            "read",
            "read_file",
            json!({"path":"data"}),
            "[data] (1 lines, 2 bytes)\nok",
        );
        exchange(
            &mut messages,
            "reject",
            "exec",
            json!({"command":"rm data"}),
            "REJECTED BY USER",
        );
        assert_eq!(
            super::super::evidence_packet::project(&messages, None).actions[0].freshness,
            Freshness::Current
        );
        exchange(
            &mut messages,
            "failed",
            "exec",
            json!({"command":"printf partial > data; false"}),
            "FAILED (exit_code: 1)",
        );
        assert_eq!(
            super::super::evidence_packet::project(&messages, None).actions[0].freshness,
            Freshness::BeforeMutation
        );
    }
}
