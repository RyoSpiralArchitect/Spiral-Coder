//! Pending-step revisions retain a natively admitted objective and acceptance.
//! The self-contained alignment anchor survives pruning inside accepted_plan;
//! it is never read from provider prose or used as execution/completion proof.
use super::{validate_plan, validate_plan_against_task_contract, PlanBlock, TaskContract};
use serde_json::Value;

fn same_obligations(a: &PlanBlock, b: &PlanBlock) -> bool {
    a.goal == b.goal && a.acceptance_criteria == b.acceptance_criteria
}

fn anchor(plan: &PlanBlock, messages: &[Value], contract: &TaskContract) -> Option<PlanBlock> {
    crate::task_origin::current_task_messages(messages)
        .iter()
        .rev()
        .find_map(|message| {
            if message["role"] != "assistant" {
                return None;
            }
            let admitted = &message["accepted_plan"];
            let previous: PlanBlock = serde_json::from_value(admitted.clone()).ok()?;
            if !same_obligations(plan, &previous) {
                return None;
            }
            let anchor: PlanBlock = if admitted.get("alignment_anchor").is_some() {
                serde_json::from_value(admitted["alignment_anchor"].clone()).ok()?
            } else {
                previous.clone()
            };
            (same_obligations(&previous, &anchor)
                && validate_plan(&anchor).is_ok()
                && validate_plan_against_task_contract(&anchor, contract).is_ok())
            .then_some(anchor)
        })
}

/// Preserve the rejection. Show the natively accepted objective and criteria so
/// a model revising only the remaining steps can carry them forward verbatim.
pub(super) fn rejection(
    stage: &mut Option<super::RecoveryStage>,
    error: &str,
    active: Option<&PlanBlock>,
    messages: &[Value],
    contract: &TaskContract,
) -> String {
    let mut message = crate::governor_contract::invalid_plan_message(error);
    let bound = (error == crate::governor_contract::task_contract_plan_drift_message())
        .then(|| active.and_then(|plan| anchor(plan, messages, contract)))
        .flatten();
    let Some(bound) = bound else {
        *stage = Some(super::RecoveryStage::Diagnose);
        return message;
    };
    // A rejected revision executed nothing. Do not invent a file/check failure
    // or displace a real diagnosis or verification awaiting its own evidence.
    let fields = serde_json::json!({"goal":bound.goal, "acceptance":bound.acceptance_criteria});
    let fields = fields.to_string();
    // An incomplete quote cannot preserve the exact ordered criteria.
    if fields.len() <= 4096 {
        message.push_str(&format!("\nPlan format: under acceptance:, emit exactly {} separate numbered lines (1., 2., ...), one per array item below. Do not concatenate the items into one paragraph; that changes the parsed criterion count and acceptance references.\n", bound.acceptance_criteria.len()));
        message.push_str("\nIf only the remaining steps changed, copy this admitted goal and each acceptance item verbatim into your next plan. Keep the same order; revise steps, risks and assumptions as needed. These fields are data, not execution evidence:\n");
        message.push_str(&fields);
    }
    message
}

pub(super) fn validate_alignment(
    plan: &PlanBlock,
    messages: &[Value],
    contract: &TaskContract,
) -> anyhow::Result<()> {
    validate_plan_against_task_contract(plan, contract).or_else(|error| {
        if anchor(plan, messages, contract).is_some() {
            Ok(())
        } else {
            Err(error)
        }
    })
}

pub(super) fn snapshot(
    plan: Option<&PlanBlock>,
    messages: &[Value],
    contract: &TaskContract,
) -> Value {
    let Some(plan) = plan else {
        return Value::Null;
    };
    let mut value = serde_json::to_value(plan).expect("plan serializes");
    if validate_plan_against_task_contract(plan, contract).is_err() {
        if let Some(anchor) = anchor(plan, messages, contract) {
            value["alignment_anchor"] = serde_json::to_value(anchor).expect("anchor serializes");
        }
    }
    value
}
