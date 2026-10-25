//! Recognize a rejected authored report without reopening already-verified work.
//! This is an advisory projection, never completion proof or generated prose.
use super::evidence_packet::{self, ExecutionPacket};
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Serialize)]
pub(crate) struct ReportRepair {
    pub rejected_call_id: String,
    pub feedback: String,
}

pub(super) fn project(
    messages: &[Value],
    test_cmd: Option<&str>,
    evidence: &ExecutionPacket,
    recovery_pending: bool,
) -> Option<ReportRepair> {
    if recovery_pending
        || !evidence.pending_exact_checks.is_empty()
        || super::requirement_proof::issue(messages, test_cmd).is_some()
    {
        return None;
    }
    let action = evidence.actions.last()?;
    if action.tool != "done" || action.executed != Some(false) {
        return None;
    }
    // Use the full correlated result: the UI excerpt can truncate the marker.
    let result = crate::task_origin::current_task_messages(messages)
        .iter()
        .rev()
        .find(|m| m["role"] == "tool" && m["tool_call_id"] == action.call_id)?["content"]
        .as_str()?;
    if !result.starts_with("GOVERNOR BLOCKED\n")
        || !result.contains("Missing from the final answer:\n")
        || action
            .arguments
            .get("remaining_acceptance")
            .is_some_and(|v| v.as_array().is_none_or(|a| !a.is_empty()))
        || evidence_packet::acceptance_issue(messages, &action.arguments, test_cmd).is_some()
    {
        return None;
    }
    let issue =
        evidence_packet::report_issue(messages, action.arguments["summary"].as_str()?, test_cmd);
    let feedback = match issue {
        Some(issue) => issue.split_once("Missing from the final answer:\n")?.1.to_owned(),
        None => "The previously rejected summary now satisfies current report validation. Re-submit done with the verified citations and an accurate authored summary; no new inspection or check is required by this report refusal.".into(),
    };
    Some(ReportRepair {
        rejected_call_id: action.call_id.clone(),
        feedback,
    })
}
