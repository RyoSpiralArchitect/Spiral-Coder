//! Summarize the last native assist without replaying its patch or command.
use super::{governor_block, read_lines, target};
use crate::{
    tui::agent::{current_evidence::Freshness, evidence_packet::ActionEvidence},
    work_budget::AssistReceipt,
};
use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum Disposition {
    Executed,
    PlanGate,
    OtherGovernor,
    UserRefusal,
    AwaitingApproval,
    Unconfirmed,
}

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
            a.tool == suggested.tool
                && suggested.matching_call_ids.contains(&a.call_id)
                && receipt.call_ids.contains(&a.call_id)
        })
        .max_by_key(|a| a.result_step)
        .map(|a| RetainedMatch {
            call_id: &a.call_id,
            disposition: disposition(a),
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

fn disposition(action: &ActionEvidence) -> Disposition {
    match (action.executed, action.result.as_deref()) {
        (Some(true), _) => Disposition::Executed,
        (Some(false), Some(s)) if s.starts_with("REJECTED BY USER") => Disposition::UserRefusal,
        (Some(false), Some(s)) if s.starts_with("Awaiting approval") => {
            Disposition::AwaitingApproval
        }
        (Some(false), Some(s)) => match governor_block(s) {
            Some("plan") => Disposition::PlanGate,
            Some(_) => Disposition::OtherGovernor,
            None => Disposition::Unconfirmed,
        },
        _ => Disposition::Unconfirmed,
    }
}
