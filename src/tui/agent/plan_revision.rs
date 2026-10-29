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
