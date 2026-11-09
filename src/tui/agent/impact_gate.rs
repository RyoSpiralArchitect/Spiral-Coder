//! Impact enforcement before any tool call: when an impact review is
//! required, the model must emit exactly one `<impact>` block AND exactly
//! one tool call in the same assistant turn. A contract-valid resume plan
//! may be adopted before the block is judged.
use super::*;
use serde_json::json;

/// Borrowed loop state the gate reads or mutates.
pub(super) struct ImpactGateCtx<'a> {
    pub tx: &'a mpsc::Sender<StreamToken>,
    pub provider: ProviderKind,
    pub root_read_only: bool,
    pub goal_wants_actions: bool,
    pub task_harness: task_harness::TaskHarness,
    pub task_contract: &'a TaskContract,
    pub instruction_resolver: &'a InstructionResolver,
    pub root_user_text: &'a str,
    pub test_cmd: Option<&'a str>,
    pub messages: &'a Vec<serde_json::Value>,
    pub last_mutation_step: Option<usize>,
    pub last_impact_step: Option<usize>,
    pub path_required_verification: VerificationLevel,
    pub last_build_verify_ok_step: Option<usize>,
    pub last_behavioral_verify_ok_step: Option<usize>,
    pub pending_system_hint: &'a mut Option<String>,
    pub state: &'a mut AgentState,
    pub impact_required: &'a mut Option<String>,
    pub working_mem: &'a mut WorkingMemory,
    pub assumption_ledger: &'a mut AssumptionLedger,
    pub active_plan: &'a mut Option<PlanBlock>,
    pub intent_required_verification: &'a mut VerificationLevel,
    pub required_verification: &'a mut VerificationLevel,
    pub recovery: &'a mut RecoveryGovernor,
    pub last_verify_ok_step: &'a mut Option<usize>,
}

pub(super) enum ImpactGate {
    /// The turn satisfied (or never needed) the impact contract.
    Pass,
    /// The turn was rejected; the caller must skip tool dispatch.
    Reject,
}

pub(super) async fn enforce_impact(
    ctx: &mut ImpactGateCtx<'_>,
    assistant_text: &str,
    tool_calls: &[ToolCallData],
) -> ImpactGate {
    let Some(reason) = ctx.impact_required.clone() else {
        return ImpactGate::Pass;
    };
    let pending_mutation = impact_recovery::UnreviewedMutation::from_steps(
        ctx.last_mutation_step,
        ctx.last_impact_step,
    );
    let mut restored_impact_plan = false;
    let impact_plan = parse_plan_block(assistant_text)
        .filter(|plan| {
            validate_plan_for_task_contract(
                plan,
                ctx.root_read_only,
                ctx.task_contract,
                ctx.instruction_resolver,
                ctx.messages,
            )
            .is_ok()
        })
        .or_else(|| ctx.active_plan.clone())
        .or_else(|| {
            let [tc] = tool_calls else {
                return None;
            };
            let plan = impact_recovery::benchmark_resume_plan(
                pending_mutation,
                ctx.task_harness,
                tc,
                ctx.root_user_text,
                *ctx.required_verification,
                ctx.test_cmd,
            )?;
            validate_plan_for_task_contract(
                &plan,
                ctx.root_read_only,
                ctx.task_contract,
                ctx.instruction_resolver,
                ctx.messages,
            )
            .ok()?;
            restored_impact_plan = true;
            Some(plan)
        });
    // Keep the contract-validated plan before impact can reject this
    // turn, so the next request exposes the same accepted step labels.
    if restored_impact_plan {
        if let Some(plan) = impact_plan.as_ref() {
            adopt_valid_plan(
                plan,
                ctx.working_mem,
                ctx.assumption_ledger,
                ctx.active_plan,
                ctx.intent_required_verification,
                ctx.path_required_verification,
                ctx.required_verification,
                ctx.recovery,
                ctx.last_verify_ok_step,
                ctx.last_build_verify_ok_step,
                ctx.last_behavioral_verify_ok_step,
            );
            emit_telemetry_event(
                ctx.tx,
                "impact_resume_plan_restored",
                json!({
                    "mutation_step": pending_mutation.map(|pending| pending.step),
                    "lane": ctx.task_harness.lane_label(),
                }),
            )
            .await;
        }
    }

    let impact = match parse_impact_block(assistant_text) {
        Some(impact) => impact,
        None => {
            if tool_calls.len() == 1 {
                if let Some(impact) = rescue_missing_impact_for_tool_turn(
                    reason.as_str(),
                    &tool_calls[0],
                    ctx.root_read_only,
                    ctx.goal_wants_actions,
                    ctx.provider.clone(),
                    impact_plan.as_ref(),
                    pending_mutation,
                ) {
                    let _ = ctx
                        .tx
                        .send(StreamToken::Delta(
                            "\n[compat] synthesized impact block for the rescued mutation turn\n"
                                .to_string(),
                        ))
                        .await;
                    impact
                } else {
                    return reject(
                        ctx,
                        governor_contract::impact_missing_message(reason.as_str()),
                        tool_calls,
                        assistant_text,
                        reason,
                    )
                    .await;
                }
            } else {
                return reject(
                    ctx,
                    governor_contract::impact_missing_message(reason.as_str()),
                    tool_calls,
                    assistant_text,
                    reason,
                )
                .await;
            }
        }
    };

    if let Err(e) = validate_impact(&impact, impact_plan.as_ref()) {
        return reject(
            ctx,
            governor_contract::impact_invalid_message(&e.to_string(), reason.as_str()),
            tool_calls,
            assistant_text,
            reason,
        )
        .await;
    }

    if tool_calls.len() != 1 {
        return reject(
            ctx,
            governor_contract::impact_one_tool_message(tool_calls.len()),
            tool_calls,
            assistant_text,
            reason,
        )
        .await;
    }

    let _ = ctx
        .tx
        .send(StreamToken::Delta(format!(
            "\n[impact] changed={} progress={} gap={}\n",
            impact.changed.as_str(),
            impact.progress.as_str(),
            impact.remaining_gap.as_str()
        )))
        .await;

    *ctx.impact_required = None;
    ImpactGate::Pass
}

/// A rejected impact turn emits feedback, records the pending hint, and
/// keeps the requirement armed for the next provider response.
async fn reject(
    ctx: &mut ImpactGateCtx<'_>,
    reason_msg: String,
    tool_calls: &[ToolCallData],
    assistant_text: &str,
    reason: String,
) -> ImpactGate {
    let msg = provider_turn::tool_count_feedback(reason_msg, tool_calls);
    let _ = ctx
        .tx
        .send(StreamToken::Delta(format!("\n[impact] {msg}\n")))
        .await;
    impact_recovery::emit_rejection(ctx.tx, "impact", &msg, tool_calls, assistant_text).await;
    *ctx.pending_system_hint = Some(msg);
    *ctx.state = AgentState::Recovery;
    *ctx.impact_required = Some(reason);
    ImpactGate::Reject
}
