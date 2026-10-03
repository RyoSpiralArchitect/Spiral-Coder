//! Resume impact checks from runtime evidence without depending on prompt wording.
use super::*;

#[derive(Clone, Copy, Debug)]
pub(super) struct UnreviewedMutation {
    pub step: usize,
}

impl UnreviewedMutation {
    pub(super) fn from_steps(mutation: Option<usize>, impact: Option<usize>) -> Option<Self> {
        mutation
            .filter(|step| *step > impact.unwrap_or(0))
            .map(|step| Self { step })
    }
}

pub(super) fn benchmark_resume_plan(
    pending: Option<UnreviewedMutation>,
    harness: TaskHarness,
    tc: &ToolCallData,
    root_user_text: &str,
    required_verification: VerificationLevel,
    test_cmd: Option<&str>,
) -> Option<PlanBlock> {
    pending?;
    if harness.lane != task_harness::TaskLane::BenchmarkPlan {
        return None;
    }
    Some(provider_compat::synthetic_action_plan(
        root_user_text,
        tc,
        harness,
        required_verification,
        test_cmd,
    ))
}

fn rejection_data(
    gate: &str,
    reason: &str,
    calls: &[ToolCallData],
    assistant_text: &str,
) -> serde_json::Value {
    json!({
        "gate": gate,
        "reason": reason.chars().take(480).collect::<String>(),
        "tool_count": calls.len(),
        "tool_names": calls.iter().take(4).map(|call| call.name.chars().take(64).collect::<String>()).collect::<Vec<_>>(),
        "assistant_chars": assistant_text.chars().count(),
        "has_plan": parse_plan_block(assistant_text).is_some(),
        "has_impact": parse_impact_block(assistant_text).is_some(),
        "has_reflection": parse_reflection_block(assistant_text).is_some(),
    })
}

pub(super) async fn emit_rejection(
    tx: &mpsc::Sender<StreamToken>,
    gate: &str,
    reason: &str,
    calls: &[ToolCallData],
    assistant_text: &str,
) {
    emit_telemetry_event(
        tx,
        "pre_tool_gate_rejected",
        rejection_data(gate, reason, calls, assistant_text),
    )
    .await;
}

#[cfg(test)]
mod tests;
