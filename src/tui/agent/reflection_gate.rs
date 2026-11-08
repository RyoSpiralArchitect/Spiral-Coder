//! Reflection enforcement before any tool call: when a reflection is
//! required, the model must emit exactly one `<reflect>` block AND exactly
//! one tool call in the same assistant turn.
use super::*;
use serde_json::json;
use std::path::Path;

/// Bundled state the gate reads or mutates, so the agent loop passes one
/// struct instead of threading a dozen locals through the enforcement block.
pub(super) struct ReflectionGateCtx<'a> {
    pub tx: &'a mpsc::Sender<StreamToken>,
    pub provider: ProviderKind,
    pub root_read_only: bool,
    pub goal_wants_actions: bool,
    pub mem: &'a FailureMemory,
    pub file_tool_consec_failures: usize,
    pub recovery: &'a RecoveryGovernor,
    pub last_mutation_step: Option<usize>,
    pub last_verify_ok_step: Option<usize>,
    pub pending_system_hint: &'a mut Option<String>,
    pub state: &'a mut AgentState,
    pub reflection_required: &'a mut Option<String>,
    pub last_reflection: &'a mut Option<ReflectionBlock>,
    pub reflection_guard: &'a mut Option<ReflectionBlock>,
    pub ledger: &'a mut crate::reflection_ledger::ReflectionLedger,
    pub ledger_path: Option<&'a Path>,
    pub ledger_warned: &'a mut bool,
    pub working_mem: &'a mut WorkingMemory,
    pub assumption_ledger: &'a mut AssumptionLedger,
}

pub(super) enum ReflectionGate {
    /// The turn satisfied (or never needed) the reflection contract.
    Pass,
    /// The turn was rejected; the caller must skip tool dispatch.
    Reject,
}

pub(super) async fn enforce_reflection(
    ctx: &mut ReflectionGateCtx<'_>,
    assistant_text: &str,
    tool_calls: &[ToolCallData],
) -> ReflectionGate {
    let Some(reason) = ctx.reflection_required.clone() else {
        return ReflectionGate::Pass;
    };

    let reflect = match parse_reflection_block(assistant_text) {
        Some(r) => r,
        None => {
            if tool_calls.len() == 1 {
                if let Some(reflect) = rescue_missing_reflection_for_tool_turn(
                    reason.as_str(),
                    &tool_calls[0],
                    ctx.root_read_only,
                    ctx.goal_wants_actions,
                    ctx.provider.clone(),
                    ctx.mem,
                    ctx.file_tool_consec_failures,
                ) {
                    let _ = ctx
                        .tx
                        .send(StreamToken::Delta(
                            "\n[compat] synthesized reflection block for the rescued tool turn\n"
                                .to_string(),
                        ))
                        .await;
                    reflect
                } else {
                    return reject(
                        ctx,
                        governor_contract::reflection_missing_message(reason.as_str()),
                        tool_calls,
                        assistant_text,
                        reason,
                    )
                    .await;
                }
            } else {
                return reject(
                    ctx,
                    governor_contract::reflection_missing_message(reason.as_str()),
                    tool_calls,
                    assistant_text,
                    reason,
                )
                .await;
            }
        }
    };

    if let Err(e) = validate_reflection(&reflect, ctx.mem, ctx.file_tool_consec_failures) {
        return reject(
            ctx,
            governor_contract::reflection_invalid_message(&e.to_string(), reason.as_str()),
            tool_calls,
            assistant_text,
            reason,
        )
        .await;
    }

    if tool_calls.len() != 1 {
        return reject(
            ctx,
            governor_contract::reflection_one_tool_message(tool_calls.len()),
            tool_calls,
            assistant_text,
            reason,
        )
        .await;
    }

    let _ = ctx
        .tx
        .send(StreamToken::Delta(format!(
            "\n[reflect] goal_delta={} strategy={} next={}\n",
            reflect.goal_delta.as_str(),
            reflect.strategy_change.as_str(),
            reflect.next_minimal_action.as_str()
        )))
        .await;

    *ctx.last_reflection = Some(reflect.clone());
    *ctx.reflection_guard = Some(reflect);
    *ctx.reflection_required = None;

    if let Some(r) = ctx.last_reflection.as_ref() {
        if ctx.ledger.remember(
            r.wrong_assumption.as_str(),
            r.next_minimal_action.as_str(),
            Some(reason.as_str()),
            r.last_outcome.as_str(),
            r.goal_delta.as_str(),
            r.strategy_change.as_str(),
        ) {
            let remembered_count = ctx
                .ledger
                .find_entry(r.wrong_assumption.as_str(), r.next_minimal_action.as_str())
                .map(|entry| entry.count)
                .unwrap_or(1);
            emit_telemetry_event(
                ctx.tx,
                "reflection_ledger_remembered",
                json!({
                    "wrong_assumption": r.wrong_assumption,
                    "next_minimal_action": r.next_minimal_action,
                    "trigger": reason,
                    "count": remembered_count,
                    "entries": ctx.ledger.entries.len(),
                }),
            )
            .await;
            if let Some(path) = ctx.ledger_path {
                if let Err(e) = ctx.ledger.save_atomic(path) {
                    if !*ctx.ledger_warned {
                        *ctx.ledger_warned = true;
                        let _ = ctx
                            .tx
                            .send(StreamToken::Delta(format!(
                                "\n[reflection_ledger] WARN: {e:#}\n"
                            )))
                            .await;
                    }
                }
            }
        }
        if !r.wrong_assumption.trim().is_empty() {
            ctx.assumption_ledger.mark_refuted(
                r.wrong_assumption.as_str(),
                Some(r.next_minimal_action.as_str()),
            );
        }
        if matches!(
            r.strategy_change,
            StrategyChange::Adjust | StrategyChange::Abandon
        ) {
            ctx.working_mem.set_strategy(r.next_minimal_action.as_str());
            ctx.assumption_ledger.refresh_confirmations(ctx.working_mem);
        }
        if r.strategy_change == StrategyChange::Abandon {
            *ctx.pending_system_hint = Some(format!(
                "Strategy abandoned.\n\
Do not retry the previous approach.\n\
Execute only the new minimal action: {}",
                r.next_minimal_action.as_str()
            ));
        }
    }

    let _ = ctx
        .tx
        .send(StreamToken::GovernorState(build_governor_state(
            *ctx.state,
            ctx.recovery,
            ctx.mem,
            ctx.file_tool_consec_failures,
            ctx.last_mutation_step,
            ctx.last_verify_ok_step,
            ctx.last_reflection.as_ref(),
        )))
        .await;

    ReflectionGate::Pass
}

/// A rejected reflection turn emits feedback, records the pending hint, and
/// keeps the requirement armed for the next provider response.
async fn reject(
    ctx: &mut ReflectionGateCtx<'_>,
    reason_msg: String,
    tool_calls: &[ToolCallData],
    assistant_text: &str,
    reason: String,
) -> ReflectionGate {
    let msg = provider_turn::tool_count_feedback(reason_msg, tool_calls);
    let _ = ctx
        .tx
        .send(StreamToken::Delta(format!("\n[reflect] {msg}\n")))
        .await;
    impact_recovery::emit_rejection(ctx.tx, "reflection", &msg, tool_calls, assistant_text).await;
    *ctx.pending_system_hint = Some(msg);
    *ctx.state = AgentState::Recovery;
    *ctx.reflection_required = Some(reason);
    ReflectionGate::Reject
}
