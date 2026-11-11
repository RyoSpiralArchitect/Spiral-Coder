use super::tool_dispatch::{ToolDispatchCtx, ToolFlow};
use super::*;

pub(super) async fn run_done(ctx: &mut ToolDispatchCtx<'_>, tc: &ToolCallData) -> Result<ToolFlow> {
    // A generic passing check cannot discharge a localized failed check.
    if ctx.recovery.focus.is_pending() {
        let block = format!(
            "[Done Gate] {}",
            ctx.recovery.focus.hint().unwrap_or_default()
        );
        ctx.messages.push(json!({"role":"tool", "tool_call_id":tc.id, "content":format!("GOVERNOR BLOCKED\n{block}")}));
        *ctx.pending_system_hint = Some(block);
        *ctx.state = AgentState::Recovery;
        ctx.autosave().await;
        return Ok(ToolFlow::Continue);
    }

    if ctx.realize_cfg.enabled {
        if let Some(latent) = ctx.latent_plan.as_ref() {
            *ctx.state = AgentState::Recovery;
            ctx.recovery.stage = Some(RecoveryStage::Diagnose);
            let block = build_realize_done_gate_message(latent);
            emit_governor_block(ctx.governor_emit(), block, tc, None).await;
            return Ok(ToolFlow::Continue);
        }
    }

    let last_mutation = ctx.last_mutation_step.unwrap_or(0);
    let last_verify_ok = ctx.last_verify_ok_step.unwrap_or(0);
    if last_mutation > 0 && last_verify_ok < last_mutation {
        *ctx.state = AgentState::Recovery;
        ctx.recovery.stage = Some(RecoveryStage::Verify);
        let block = format!(
            "[Done Gate] Verification required before `done`.\n\
Last mutation step: {last_mutation}\n\
Last verification step: {last_verify_ok}\n\
Required now: {}",
            verification_requirement_hint(*ctx.required_verification, ctx.test_cmd)
        );
        emit_governor_block(ctx.governor_emit(), block, tc, None).await;
        return Ok(ToolFlow::Continue);
    }

    if let Some(command) =
        benchmark_plan_pending_required_exec_command(ctx.root_user_text, ctx.messages, ctx.test_cmd)
    {
        *ctx.state = AgentState::Recovery;
        ctx.recovery.stage = Some(RecoveryStage::Verify);
        let verification_status = task_harness::benchmark_plan_verification_hint(
            ctx.root_user_text,
            ctx.messages,
            ctx.test_cmd,
        )
        .unwrap_or_default();
        let block = format!(
            "[Done Gate] Approved benchmark plan requires explicit verification proof before `done`.\n\
Required now: run `{command}`.\n{verification_status}"
        );
        emit_governor_block(ctx.governor_emit(), block, tc, None).await;
        return Ok(ToolFlow::Continue);
    }

    let args: serde_json::Value = serde_json::from_str(&tc.arguments)?;
    let report = final_handoff::report_context(ctx.root_user_text, ctx.messages, ctx.test_cmd);
    let summary = report_surface::authored(args["summary"].as_str().unwrap_or(""), &report);
    let completed_acceptance = parse_string_list_arg(&args["completed_acceptance"]);
    let remaining_acceptance = parse_string_list_arg(&args["remaining_acceptance"]);
    let next_steps = args["next_steps"].as_str().unwrap_or("").trim();
    let done_plan = ctx
        .validated_plan_for_turn
        .as_ref()
        .or(ctx.active_plan.as_ref());
    let acceptance = acceptance_report::CurrentEvidence::from_native(
        ctx.messages,
        ctx.test_cmd,
        ctx.tool_root_abs.as_deref(),
    );
    let known_acceptance_commands = &acceptance.commands;
    let read_only_scores = if ctx.root_read_only {
        done_plan
            .map(|plan| {
                build_read_only_evidence_scores(ctx.root_user_text, plan, ctx.observation_evidence)
            })
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    let evidence_rows =
        match acceptance_report::resolve_evidence(&args["acceptance_evidence"], &acceptance)
            .and_then(|rows| {
                validate_done_acceptance(
                    done_plan,
                    &completed_acceptance,
                    &remaining_acceptance,
                    &rows,
                    known_acceptance_commands,
                    ctx.observation_evidence,
                )
            }) {
            Ok(rows) => rows,
            Err(e) => {
                *ctx.state = AgentState::Recovery;
                ctx.recovery.stage = Some(if ctx.root_read_only {
                    RecoveryStage::Diagnose
                } else {
                    RecoveryStage::Verify
                });
                let hint = build_done_acceptance_recovery_hint(
                    &e.to_string(),
                    done_plan,
                    &completed_acceptance,
                    known_acceptance_commands,
                    &read_only_scores,
                    ctx.cfg
                        .base_url
                        .contains("generativelanguage.googleapis.com"),
                );
                let block = format!(
                    "{}{}{}",
                    governor_contract::done_invalid_acceptance_message(&e.to_string()),
                    hint,
                    acceptance_report::hint(done_plan, &acceptance)
                );
                emit_governor_block(ctx.governor_emit(), block, tc, None).await;
                return Ok(ToolFlow::Continue);
            }
        };

    let summary_text = if summary.is_empty() {
        synthesize_action_done_summary(done_plan, ctx.messages).unwrap_or_default()
    } else {
        summary.to_string()
    };

    let mut final_text = String::new();
    final_text.push_str("[DONE]\n");
    if !summary_text.is_empty() {
        final_text.push_str(summary_text.as_str());
    }
    final_text.push_str("\n\nAcceptance:\n");
    let evidence_by_idx: std::collections::HashMap<usize, String> =
        evidence_rows.into_iter().collect();
    let done_plan = done_plan.expect("validated done plan");
    for item in &completed_acceptance {
        let idx =
            resolve_acceptance_reference(item, done_plan).expect("completed acceptance validated");
        final_text.push_str("- done: ");
        final_text.push_str(acceptance_reference_label(done_plan, idx).as_str());
        if let Some(command) = evidence_by_idx.get(&idx) {
            final_text.push_str(" via `");
            final_text.push_str(command.as_str());
            final_text.push('`');
        }
        final_text.push('\n');
    }
    for item in &remaining_acceptance {
        let idx =
            resolve_acceptance_reference(item, done_plan).expect("remaining acceptance validated");
        final_text.push_str("- remaining: ");
        final_text.push_str(acceptance_reference_label(done_plan, idx).as_str());
        final_text.push('\n');
    }
    if !next_steps.is_empty() {
        final_text.push_str("\n\nNext:\n");
        final_text.push_str(next_steps);
    }

    let exact_content_error = exact_content::validate_exact_content_task(
        ctx.root_user_text,
        ctx.tool_root_abs.as_deref(),
    )
    .err();
    let needs_artifact_repair = exact_content_error.is_some();
    let requirement_error = remaining_acceptance
        .is_empty()
        .then(|| requirement_proof::issue(ctx.messages, ctx.test_cmd))
        .flatten();
    let handoff_error = exact_content_error.or(requirement_error).or_else(|| {
        if ctx.root_read_only {
            None
        } else {
            validate_authored_done_summary(
                ctx.root_user_text,
                summary,
                &final_text,
                ctx.messages,
                ctx.test_cmd,
            )
            .err()
        }
    });
    if let Some(block) = handoff_error {
        *ctx.state = AgentState::Recovery;
        if needs_artifact_repair {
            // A line-oriented test cannot override a failed exact-byte contract.
            *ctx.last_build_verify_ok_step = None;
            *ctx.last_behavioral_verify_ok_step = None;
            *ctx.last_verify_ok_step = None;
            ctx.recovery.stage = Some(RecoveryStage::Fix);
        }
        ctx.messages.push(json!({
            "role": "tool",
            "tool_call_id": tc.id,
            "content": format!("GOVERNOR BLOCKED\n\n{block}"),
        }));
        ctx.autosave().await;
        let _ = ctx
            .tx
            .send(StreamToken::Delta(format!(
                "[RESULT][Recovery] GOVERNOR BLOCK\n{block}\n"
            )))
            .await;
        *ctx.pending_system_hint = Some(block);
        return Ok(ToolFlow::Continue);
    }

    ctx.task_outcome.accept_done(&remaining_acceptance);

    // Close out the tool call so session JSON remains valid on resume.
    ctx.messages.push(json!({
        "role": "tool",
        "tool_call_id": tc.id,
        "content": "OK: done"
    }));
    ctx.messages
        .push(json!({"role": "assistant", "content": final_text.clone()}));

    ctx.autosave().await;

    let _ = ctx
        .tx
        .send(StreamToken::Delta(format!("\n\n{final_text}\n")))
        .await;
    Ok(ToolFlow::Break)
}
