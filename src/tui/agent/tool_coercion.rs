//! Sequential coercion pipeline for the single authored tool call.
//!
//! Each stage may rewrite the call in place; later stages judge the
//! rewritten call. Stages run in a fixed order and emit a compat delta plus
//! a telemetry event when they fire.
use super::*;
use serde_json::json;

/// Borrowed loop state shared by every coercion stage.
pub(super) struct CoercionCtx<'a> {
    pub tx: &'a mpsc::Sender<StreamToken>,
    pub messages: &'a [serde_json::Value],
    pub task_harness: task_harness::TaskHarness,
    pub root_user_text: &'a str,
    pub root_read_only: bool,
    pub observation_evidence: &'a ObservationEvidence,
    pub active_plan: Option<&'a PlanBlock>,
    pub tool_root_abs: Option<&'a str>,
    pub recovery_stage: Option<RecoveryStage>,
    pub test_cmd: Option<&'a str>,
    pub reflection_ledger: &'a crate::reflection_ledger::ReflectionLedger,
}

/// Run the full coercion pipeline against the authored call.
pub(super) async fn coerce_authored_tool_call(
    ctx: &CoercionCtx<'_>,
    tool_call: &mut Option<ToolCallData>,
) {
    // Preserve the authored lookup through target-selection heuristics.
    // The ordinary progress and recovery gates still judge the actual call.
    let scoped_lookup = ctx.task_harness.lane == task_harness::TaskLane::FixExisting
        && ctx.recovery_stage == Some(RecoveryStage::Fix)
        && tool_call
            .as_ref()
            .is_some_and(|tc| explicit_edit_scope::symbol_lookup(ctx.root_user_text, tc));
    let can_coerce = |tc: &&ToolCallData| !scoped_lookup && check_catalog::allows_coercion(tc);

    if let Some(tc) = tool_call.as_ref().filter(can_coerce) {
        if let Some((rewritten, original, canonical)) =
            rewrite_tool_call_with_resolution(tc, ctx.observation_evidence)
        {
            let _ = ctx
                .tx
                .send(StreamToken::Delta(format!(
                    "[resolution] {} '{}' -> '{}'\n",
                    rewritten.name, original, canonical
                )))
                .await;
            emit_resolution_memory_hit_telemetry(
                ctx.tx,
                rewritten.name.as_str(),
                original.as_str(),
                canonical.as_str(),
            )
            .await;
            *tool_call = Some(rewritten);
        }
    }

    if let Some(tc) = tool_call.as_ref().filter(can_coerce) {
        apply(
            coerce_read_only_observation_tool_call(
                ctx.messages,
                tc,
                ctx.root_user_text,
                ctx.root_read_only,
                ctx.observation_evidence,
            ),
            ctx,
            tool_call,
            |o, c| format!("[compat] coerced repeated read-only observation miss: {o} -> {c}\n"),
            "read_only_tool_coercion",
            json!({}),
        )
        .await;
    }

    if let Some(tc) = tool_call.as_ref().filter(can_coerce) {
        apply(
            coerce_read_only_followup_read_tool_call(
                ctx.messages,
                tc,
                ctx.root_user_text,
                ctx.root_read_only,
                ctx.active_plan,
                ctx.observation_evidence,
            ),
            ctx,
            tool_call,
            |o, c| format!("[compat] coerced repeated read-only follow-up read: {o} -> {c}\n"),
            "read_only_tool_coercion",
            json!({"reason": "best_followup_read"}),
        )
        .await;
    }

    if let Some(tc) = tool_call.as_ref().filter(can_coerce) {
        apply(
            coerce_benchmark_plan_tool_call(
                ctx.task_harness,
                ctx.messages,
                tc,
                ctx.root_user_text,
                ctx.tool_root_abs,
                ctx.recovery_stage,
                ctx.test_cmd,
            ),
            ctx,
            tool_call,
            |o, c| format!("[task_harness] focused approved benchmark plan: {o} -> {c}\n"),
            "task_harness_tool_coercion",
            json!({"lane": ctx.task_harness.lane_label(), "reason": "approved_benchmark_plan"}),
        )
        .await;
    }

    if let Some(tc) = tool_call.as_ref().filter(can_coerce) {
        apply(
            repair_fix_existing_mutation_tool_call(
                ctx.task_harness,
                ctx.messages,
                tc,
                ctx.root_user_text,
            ),
            ctx,
            tool_call,
            |o, c| format!("[task_harness] repaired malformed fix-existing mutation: {o} -> {c}\n"),
            "task_harness_tool_coercion",
            json!({"lane": ctx.task_harness.lane_label(), "reason": "repair_fix_mutation"}),
        )
        .await;
    }

    if let Some(tc) = tool_call.as_ref().filter(can_coerce) {
        apply(
            coerce_fix_existing_blocked_mutation_tool_call(
                ctx.task_harness,
                ctx.messages,
                tc,
                ctx.root_user_text,
                ctx.recovery_stage,
            ),
            ctx,
            tool_call,
            |o, c| {
                format!("[task_harness] restored blocked fix-existing mutation: {o} -> {c}\n")
            },
            "task_harness_tool_coercion",
            json!({"lane": ctx.task_harness.lane_label(), "reason": "restore_blocked_fix_mutation"}),
        )
        .await;
    }

    if let Some(tc) = tool_call.as_ref().filter(can_coerce) {
        apply(
            coerce_existing_followup_tool_call(
                ctx.messages,
                tc,
                ctx.root_user_text,
                ctx.recovery_stage,
            ),
            ctx,
            tool_call,
            |o, c| format!("[task_harness] focused required follow-up edit: {o} -> {c}\n"),
            "task_harness_tool_coercion",
            json!({"lane": ctx.task_harness.lane_label(), "reason": "required_followup_edit"}),
        )
        .await;
    }

    let preserves_required_followup = tool_call
        .as_ref()
        .is_some_and(|tc| matches_required_existing_followup(ctx.messages, tc, ctx.root_user_text));

    if !preserves_required_followup {
        if let Some(tc) = tool_call.as_ref().filter(can_coerce) {
            apply(
                coerce_fix_existing_literal_mutation_tool_call(
                    ctx.task_harness,
                    ctx.messages,
                    tc,
                    ctx.root_user_text,
                    ctx.recovery_stage,
                ),
                ctx,
                tool_call,
                |o, c| {
                    format!("[task_harness] synthesized fix-existing mutation from gathered evidence: {o} -> {c}\n")
                },
                "task_harness_tool_coercion",
                json!({"lane": ctx.task_harness.lane_label(), "reason": "synthesize_literal_fix_mutation"}),
            )
            .await;
        }
    }

    if let Some(tc) = tool_call.as_ref().filter(can_coerce) {
        apply(
            coerce_fix_existing_tool_call(
                ctx.task_harness,
                ctx.messages,
                tc,
                ctx.test_cmd,
                ctx.tool_root_abs,
            ),
            ctx,
            tool_call,
            |o, c| format!("[task_harness] focused fix-existing action: {o} -> {c}\n"),
            "task_harness_tool_coercion",
            json!({"lane": ctx.task_harness.lane_label(), "reason": "focus_fix_existing"}),
        )
        .await;
    }

    if let Some(tc) = tool_call
        .as_ref()
        .filter(|tc| check_catalog::allows_coercion(tc))
    {
        apply(
            coerce_artifact_creation_tool_call(ctx.task_harness, ctx.messages, tc),
            ctx,
            tool_call,
            |o, c| format!("[task_harness] restored blocked artifact action: {o} -> {c}\n"),
            "task_harness_tool_coercion",
            json!({"lane": ctx.task_harness.lane_label()}),
        )
        .await;
    }

    if let Some(tc) = tool_call
        .as_ref()
        .filter(|tc| check_catalog::allows_coercion(tc))
    {
        apply(
            coerce_repo_goal_completion_tool_call(
                ctx.task_harness,
                ctx.messages,
                tc,
                ctx.test_cmd,
                ctx.tool_root_abs,
            ),
            ctx,
            tool_call,
            |o, c| format!("[task_harness] repaired repo goal completion: {o} -> {c}\n"),
            "task_harness_tool_coercion",
            json!({"lane": ctx.task_harness.lane_label(), "reason": "goal_check_missing_repo_git"}),
        )
        .await;
    }

    if let Some(tc) = tool_call
        .as_ref()
        .filter(|tc| check_catalog::allows_coercion(tc))
    {
        apply(
            repair_repo_scaffold_write_tool_call(
                ctx.task_harness,
                ctx.messages,
                tc,
                ctx.test_cmd,
                ctx.tool_root_abs,
            ),
            ctx,
            tool_call,
            |o, c| format!("[task_harness] repaired malformed repo scaffold write: {o} -> {c}\n"),
            "task_harness_tool_coercion",
            json!({"lane": ctx.task_harness.lane_label(), "reason": "repair_repo_scaffold_write"}),
        )
        .await;
    }

    if let Some(ref tc) = *tool_call {
        if let Some((entry, score, action)) =
            reflection_ledger_action_match(ctx.reflection_ledger, tc)
        {
            emit_telemetry_event(
                ctx.tx,
                "reflection_ledger_action_hit",
                json!({
                    "tool": tc.name,
                    "score": score,
                    "action": action,
                    "wrong_assumption": entry.wrong_assumption,
                    "next_minimal_action": entry.next_minimal_action,
                    "count": entry.count,
                }),
            )
            .await;
        }
    }
}

/// Emit the compat delta plus a coercion telemetry event for one fired
/// stage, then adopt the rewritten call.
async fn apply(
    outcome: Option<(ToolCallData, String, String)>,
    ctx: &CoercionCtx<'_>,
    tool_call: &mut Option<ToolCallData>,
    message: impl FnOnce(&str, &str) -> String,
    event: &str,
    extra: serde_json::Value,
) {
    let Some((rewritten, original, coerced)) = outcome else {
        return;
    };
    let _ = ctx
        .tx
        .send(StreamToken::Delta(message(&original, &coerced)))
        .await;
    let mut payload = json!({
        "from": original,
        "to": coerced,
        "tool": rewritten.name,
    });
    if let (Some(dst), Some(src)) = (payload.as_object_mut(), extra.as_object()) {
        for (k, v) in src {
            dst.insert(k.clone(), v.clone());
        }
    }
    emit_telemetry_event(ctx.tx, event, payload).await;
    *tool_call = Some(rewritten);
}
