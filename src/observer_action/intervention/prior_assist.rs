//! Summarize the last native assist without replaying its patch or command.
use super::{disposition, read_lines, target, Disposition};
use crate::{
    tui::agent::{current_evidence::Freshness, evidence_packet::ActionEvidence},
    work_budget::AssistReceipt,
};
use serde::Serialize;

#[derive(Debug, Serialize)]
struct RetainedMatch<'a> {
    call_id: &'a str,
    disposition: Disposition,
    succeeded: Option<bool>,
    freshness: &'a Freshness,
}

#[derive(Debug, Serialize)]
pub(crate) struct PreviousAssist<'a> {
    assist_id: u64,
    decision: &'a str,
    tool: &'a str,
    target: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    read_lines: Option<[u64; 2]>,
    exact_matching_calls: usize,
    suggestion_executed: Option<bool>,
    suggestion_succeeded: Option<bool>,
    latest_retained_match: Option<RetainedMatch<'a>>,
}

pub(crate) fn previous_assist<'a>(
    receipt: Option<&'a AssistReceipt>,
    actions: &'a [ActionEvidence],
) -> Option<PreviousAssist<'a>> {
    let receipt = receipt?;
    let suggested = receipt.suggestion_execution.as_ref()?;
    let latest = actions
        .iter()
        .filter(|a| {
            suggested.matching_call_ids.contains(&a.call_id)
                && receipt.call_ids.contains(&a.call_id)
                && a.matches_suggestion(&suggested.tool, &suggested.arguments)
                && !(suggested.executed == Some(false) && a.executed == Some(true))
                // IDs are unique within a provider response, not a session.
                && actions.iter().filter(|row| row.call_id == a.call_id).take(2).count() == 1
        })
        .max_by_key(|a| a.result_step)
        .map(|a| RetainedMatch {
            call_id: &a.call_id,
            disposition: disposition(a.executed, a.result.as_deref()),
            succeeded: a.succeeded,
            freshness: &a.freshness,
        });
    Some(PreviousAssist {
        assist_id: receipt.assist_id,
        decision: &receipt.decision,
        tool: &suggested.tool,
        target: target(&suggested.arguments),
        read_lines: read_lines(&suggested.tool, &suggested.arguments),
        exact_matching_calls: suggested.matching_call_ids.len(),
        suggestion_executed: suggested.executed,
        suggestion_succeeded: suggested.succeeded,
        latest_retained_match: latest,
    })
}
