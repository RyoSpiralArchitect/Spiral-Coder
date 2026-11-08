//! No-tool-call turns: recovery hints, realize deferral, implied-action
//! execution, goal checks, text-finalize and done paths. Extracted from the
//! agent loop verbatim; every path returns `Continue` (next iteration) or
//! `Break` (model finished).
use super::*;
use serde_json::json;
use std::sync::Arc;

pub(super) enum NoToolFlow {
    Continue,
    Break,
}

pub(super) struct NoToolCtx<'a> {
    pub tx: &'a mpsc::Sender<StreamToken>,
    pub autosaver: &'a Option<Arc<crate::agent_session::SessionAutoSaver>>,
    pub approver: &'a dyn Approver,
    pub cfg: &'a RunConfig,
    pub tool_root_abs: &'a Option<String>,
    pub checkpoint: &'a Option<String>,
    pub root_user_text: &'a str,
    pub assistant_text_clean: &'a String,
    pub test_cmd: &'a Option<String>,
    pub task_contract: &'a TaskContract,
    pub instruction_resolver: &'a InstructionResolver,
    pub realize_cfg: &'a RealizeOnDemandConfig,
    pub realize_reason: &'a Option<String>,
    pub parsed_plan: &'a Option<PlanBlock>,
    pub parsed_think: &'a Option<ThinkBlock>,
    pub last_reflection: &'a Option<ReflectionBlock>,
    pub root_read_only: bool,
    pub goal_wants_actions: bool,
    pub command_approval_required: bool,
    pub wants_repo_goal: bool,
    pub wants_test_goal: bool,
    pub wants_build_goal: bool,
    pub iter: usize,
    pub max_iters: usize,
    pub first_action_deadline: usize,
    pub read_only_diagnose_streak: usize,
    pub file_tool_consec_failures: usize,
    pub last_mutation_step: Option<usize>,
    pub path_required_verification: VerificationLevel,
    pub messages: &'a mut Vec<serde_json::Value>,
    pub state: &'a mut AgentState,
    pub recovery: &'a mut RecoveryGovernor,
    pub pending_system_hint: &'a mut Option<String>,
    pub working_mem: &'a mut WorkingMemory,
    pub assumption_ledger: &'a mut AssumptionLedger,
    pub active_plan: &'a mut Option<PlanBlock>,
    pub latent_plan: &'a mut Option<LatentPlanBuffer>,
    pub realize_metrics: &'a mut RealizeMetrics,
    pub task_outcome: &'a mut outcome::TaskOutcome,
    pub goal_checks: &'a mut GoalCheckTracker,
    pub cur_cwd: &'a mut Option<String>,
    pub mem: &'a mut FailureMemory,
    pub file_cache: &'a mut std::collections::HashMap<String, String>,
    pub observation_evidence: &'a mut ObservationEvidence,
    pub step_seq: &'a mut usize,
    pub tool_calls_this_run: &'a mut usize,
    pub read_only_diagnose_rescue_count: &'a mut usize,
    pub forced_tool_once: &'a mut bool,
    pub last_verify_ok_step: &'a mut Option<usize>,
    pub last_behavioral_verify_ok_step: &'a mut Option<usize>,
    pub last_build_verify_ok_step: &'a mut Option<usize>,
    pub intent_required_verification: &'a mut VerificationLevel,
    pub required_verification: &'a mut VerificationLevel,
}

pub(super) async fn handle_no_tool_call(ctx: &mut NoToolCtx<'_>) -> Result<NoToolFlow> {
    let tx = ctx.tx;
    let autosaver = ctx.autosaver;
    let approver = ctx.approver;
    let cfg = ctx.cfg;
    let tool_root_abs = ctx.tool_root_abs.clone();
    let checkpoint = ctx.checkpoint.clone();
    let root_user_text = ctx.root_user_text;
    let assistant_text_clean = ctx.assistant_text_clean;
    let test_cmd = ctx.test_cmd.clone();
    let task_contract = ctx.task_contract;
    let instruction_resolver = ctx.instruction_resolver;
    let realize_cfg = ctx.realize_cfg;
    let realize_reason = ctx.realize_reason;
    let parsed_plan = ctx.parsed_plan;
    let parsed_think = ctx.parsed_think;
    let last_reflection = ctx.last_reflection;
    let root_read_only = ctx.root_read_only;
    let goal_wants_actions = ctx.goal_wants_actions;
    let command_approval_required = ctx.command_approval_required;
    let wants_repo_goal = ctx.wants_repo_goal;
    let wants_test_goal = ctx.wants_test_goal;
    let wants_build_goal = ctx.wants_build_goal;
    let iter = ctx.iter;
    let max_iters = ctx.max_iters;
    let first_action_deadline = ctx.first_action_deadline;
    let read_only_diagnose_streak = ctx.read_only_diagnose_streak;
    let file_tool_consec_failures = ctx.file_tool_consec_failures;
    let last_mutation_step = ctx.last_mutation_step;
    let path_required_verification = ctx.path_required_verification;
    let messages = &mut *ctx.messages;
    let state = &mut *ctx.state;
    let recovery = &mut *ctx.recovery;
    let pending_system_hint = &mut *ctx.pending_system_hint;
    let working_mem = &mut *ctx.working_mem;
    let assumption_ledger = &mut *ctx.assumption_ledger;
    let active_plan = &mut *ctx.active_plan;
    let latent_plan = &mut *ctx.latent_plan;
    let realize_metrics = &mut *ctx.realize_metrics;
    let task_outcome = &mut *ctx.task_outcome;
    let goal_checks = &mut *ctx.goal_checks;
    let cur_cwd = &mut *ctx.cur_cwd;
    let mem = &mut *ctx.mem;
    let file_cache = &mut *ctx.file_cache;
    let observation_evidence = &mut *ctx.observation_evidence;
    let step_seq = &mut *ctx.step_seq;
    let tool_calls_this_run = &mut *ctx.tool_calls_this_run;
    let read_only_diagnose_rescue_count = &mut *ctx.read_only_diagnose_rescue_count;
    let forced_tool_once = &mut *ctx.forced_tool_once;
    let last_verify_ok_step = &mut *ctx.last_verify_ok_step;
    let last_behavioral_verify_ok_step = &mut *ctx.last_behavioral_verify_ok_step;
    let last_build_verify_ok_step = &mut *ctx.last_build_verify_ok_step;
    let intent_required_verification = &mut *ctx.intent_required_verification;
    let required_verification = &mut *ctx.required_verification;

    if let Some(hint) = recovery.focus.hint() {
        messages.push(json!({"role":"assistant", "content":assistant_text_clean}));
        messages.push(crate::task_origin::user_message(
            &hint,
            crate::task_origin::MessageOrigin::Runtime,
        ));
        *pending_system_hint = Some(hint);
        *state = AgentState::Recovery;
        autosave_best_effort(
            &autosaver,
            &tx,
            tool_root_abs.as_deref(),
            checkpoint.as_deref(),
            cur_cwd.as_deref(),
            &*messages,
        )
        .await;
        return Ok(NoToolFlow::Continue);
    }
    if realize_cfg.enabled {
        if let Some(plan) = parsed_plan.as_ref().filter(|plan| {
            validate_plan_for_task_contract(
                plan,
                root_read_only,
                &task_contract,
                &instruction_resolver,
                &*messages,
            )
            .is_ok()
        }) {
            let defer_score = latent_plan_defer_score(&assistant_text_clean, plan);
            if defer_score >= realize_cfg.defer_threshold {
                let latest_intent = parsed_think.as_ref().map(think_intent_summary);
                let leakage = !strip_known_latent_tags(&assistant_text_clean)
                    .trim()
                    .is_empty();
                if leakage {
                    realize_metrics.early_leakage += 1;
                }
                let summary =
                    latent_plan_summary(plan, &assistant_text_clean, latest_intent.as_deref());
                let anchor = build_anchor_baseline(
                    &*messages,
                    &root_user_text,
                    active_plan.as_ref(),
                    &*working_mem,
                );
                let drift = drift_distance(realize_cfg.drift_metric, &summary, &anchor);
                realize_metrics.total_drift += drift;
                realize_metrics.drift_samples += 1;
                *latent_plan = Some(LatentPlanBuffer {
                    raw_text: assistant_text_clean.clone(),
                    plan: plan.clone(),
                    summary,
                    latest_intent,
                    anchor_baseline: anchor,
                    created_iter: iter,
                    defer_score,
                    tail_updates: 0,
                });
                let _ = tx
                .send(StreamToken::Delta(format!(
                    "\n[realize] deferred plan score={defer_score:.2} drift={drift:.3} window=0/{}\n",
                    realize_cfg.window_end
                )))
                .await;
                emit_realize_state(
                    &tx,
                    &realize_cfg,
                    latent_plan.as_ref(),
                    iter,
                    Some(drift),
                    &*realize_metrics,
                )
                .await;
                return Ok(NoToolFlow::Continue);
            }
        } else if latent_plan.is_none() {
            if let (Some(active), Some(think)) = (active_plan.as_ref(), parsed_think.as_ref()) {
                let defer_score = latent_intent_defer_score(&assistant_text_clean, think);
                if defer_score >= realize_cfg.defer_threshold {
                    let latest_intent = Some(think_intent_summary(think));
                    let leakage = !strip_known_latent_tags(&assistant_text_clean)
                        .trim()
                        .is_empty();
                    if leakage {
                        realize_metrics.early_leakage += 1;
                    }
                    let summary = latent_plan_summary(
                        active,
                        &assistant_text_clean,
                        latest_intent.as_deref(),
                    );
                    let anchor = build_anchor_baseline(
                        &*messages,
                        &root_user_text,
                        active_plan.as_ref(),
                        &*working_mem,
                    );
                    let drift = drift_distance(realize_cfg.drift_metric, &summary, &anchor);
                    realize_metrics.total_drift += drift;
                    realize_metrics.drift_samples += 1;
                    *latent_plan = Some(LatentPlanBuffer {
                        raw_text: assistant_text_clean.clone(),
                        plan: active.clone(),
                        summary,
                        latest_intent,
                        anchor_baseline: anchor,
                        created_iter: iter,
                        defer_score,
                        tail_updates: 0,
                    });
                    let _ = tx
                    .send(StreamToken::Delta(format!(
                        "\n[realize] deferred intent score={defer_score:.2} drift={drift:.3} window=0/{}\n",
                        realize_cfg.window_end
                    )))
                    .await;
                    emit_realize_state(
                        &tx,
                        &realize_cfg,
                        latent_plan.as_ref(),
                        iter,
                        Some(drift),
                        &*realize_metrics,
                    )
                    .await;
                    return Ok(NoToolFlow::Continue);
                }
            }
        } else if let Some(latent) = latent_plan.as_mut() {
            if !assistant_text_clean.is_empty() {
                if !latent.raw_text.trim().is_empty() {
                    latent.raw_text.push_str("\n\n");
                }
                latent.raw_text.push_str(&assistant_text_clean);
                if let Some(think) = parsed_think.as_ref() {
                    latent.latest_intent = Some(think_intent_summary(think));
                }
                latent.summary = latent_plan_summary(
                    &latent.plan,
                    &latent.raw_text,
                    latent.latest_intent.as_deref(),
                );
                latent.tail_updates += 1;
                realize_metrics.early_leakage += 1;
                let drift = drift_distance(
                    realize_cfg.drift_metric,
                    &latent.summary,
                    &latent.anchor_baseline,
                );
                realize_metrics.total_drift += drift;
                realize_metrics.drift_samples += 1;
                let _ = tx
                    .send(StreamToken::Delta(format!(
                        "\n[realize] buffered latent tail updates={} drift={drift:.3}\n",
                        latent.tail_updates
                    )))
                    .await;
                emit_realize_state(
                    &tx,
                    &realize_cfg,
                    latent_plan.as_ref(),
                    iter,
                    Some(drift),
                    &*realize_metrics,
                )
                .await;
                return Ok(NoToolFlow::Continue);
            }
        }
    }

    if assistant_text_clean.is_empty() && realize_reason.is_some() {
        autosave_best_effort(
            &autosaver,
            &tx,
            tool_root_abs.as_deref(),
            checkpoint.as_deref(),
            cur_cwd.as_deref(),
            &*messages,
        )
        .await;
        return Ok(NoToolFlow::Continue);
    }

    if let Some(ref plan) = parsed_plan {
        match validate_plan_for_task_contract(
            &plan,
            root_read_only,
            &task_contract,
            &instruction_resolver,
            &*messages,
        ) {
            Ok(()) => {
                adopt_valid_plan(
                    &plan,
                    &mut *working_mem,
                    &mut *assumption_ledger,
                    &mut *active_plan,
                    &mut *intent_required_verification,
                    path_required_verification,
                    &mut *required_verification,
                    &mut *recovery,
                    &mut *last_verify_ok_step,
                    *last_build_verify_ok_step,
                    *last_behavioral_verify_ok_step,
                );
            }
            Err(e) => {
                *state = AgentState::Recovery;
                let mut block = plan_revision::rejection(
                    &mut (*recovery).stage,
                    &e.to_string(),
                    active_plan.as_ref(),
                    &*messages,
                    &task_contract,
                );
                if root_read_only {
                    block.push_str(
                    "\nFor this task, keep the plan strictly read-only: inspect/search/read only; no build/test/behavioral verification in steps or acceptance.",
                );
                    block.push_str(
                    "\nRewrite any build/test step into an inspection step. Example: replace `run cargo test` with `read the matching file and confirm the command branch`.",
                );
                    block.push_str("\n\n");
                    block.push_str(&build_read_only_plan_rewrite_hint(&root_user_text));
                }
                let _ = tx
                    .send(StreamToken::Delta(format!(
                        "[RESULT][Recovery] GOVERNOR BLOCK\n{block}\n"
                    )))
                    .await;
                *pending_system_hint = Some(block);
                autosave_best_effort(
                    &autosaver,
                    &tx,
                    tool_root_abs.as_deref(),
                    checkpoint.as_deref(),
                    cur_cwd.as_deref(),
                    &*messages,
                )
                .await;
                let _ = tx
                    .send(StreamToken::GovernorState(build_governor_state(
                        *state,
                        &*recovery,
                        &*mem,
                        file_tool_consec_failures,
                        last_mutation_step,
                        *last_verify_ok_step,
                        last_reflection.as_ref(),
                    )))
                    .await;
                return Ok(NoToolFlow::Continue);
            }
        }
    }
    if goal_wants_actions && iter + 1 < max_iters {
        if let Some(hint) = required_existing_followup_no_tool_hint(&*messages, &root_user_text) {
            *state = AgentState::Recovery;
            recovery.stage = Some(RecoveryStage::Fix);
            *pending_system_hint = Some(hint.clone());
            let _ = tx
                .send(StreamToken::Delta(format!(
                "\n[task_harness] blocked text-only closeout; required follow-up remains\n{hint}\n"
            )))
                .await;
            autosave_best_effort(
                &autosaver,
                &tx,
                tool_root_abs.as_deref(),
                checkpoint.as_deref(),
                cur_cwd.as_deref(),
                &*messages,
            )
            .await;
            let _ = tx
                .send(StreamToken::GovernorState(build_governor_state(
                    *state,
                    &*recovery,
                    &*mem,
                    file_tool_consec_failures,
                    last_mutation_step,
                    *last_verify_ok_step,
                    last_reflection.as_ref(),
                )))
                .await;
            return Ok(NoToolFlow::Continue);
        }
    }
    if !assistant_text_clean.is_empty() {
        let assistant_text_for_history = enrich_text_final_handoff(
            assistant_text_clean.as_str(),
            &root_user_text,
            &*messages,
            test_cmd.as_deref(),
        )
        .unwrap_or_else(|| assistant_text_clean.clone());
        messages.push(json!({"role": "assistant", "content": assistant_text_for_history}));
    }
    autosave_best_effort(
        &autosaver,
        &tx,
        tool_root_abs.as_deref(),
        checkpoint.as_deref(),
        cur_cwd.as_deref(),
        &*messages,
    )
    .await;

    if matches!(cfg.provider, ProviderKind::Mistral)
        && (parsed_plan
            .as_ref()
            .filter(|plan| {
                validate_plan_for_task_contract(
                    plan,
                    root_read_only,
                    &task_contract,
                    &instruction_resolver,
                    &*messages,
                )
                .is_ok()
            })
            .is_some()
            || parsed_think.is_some())
        && iter + 1 < max_iters
    {
        *state = AgentState::Recovery;
        let note = if parsed_think.is_some() {
            build_mistral_think_only_hint(
                &root_user_text,
                parsed_think.as_ref().expect("parsed_think checked above"),
            )
        } else {
            "\
[Mistral compatibility]\n\
Your plan block was recorded.\n\
Next assistant turn: emit a <think> block and call ONE real tool."
                .to_string()
        };
        *pending_system_hint = Some(note);
        let _ = tx
            .send(StreamToken::Delta(
                "\n[compat] recorded structured block; continuing to real tool call\n".to_string(),
            ))
            .await;
        let _ = tx
            .send(StreamToken::GovernorState(build_governor_state(
                *state,
                &*recovery,
                &*mem,
                file_tool_consec_failures,
                last_mutation_step,
                *last_verify_ok_step,
                last_reflection.as_ref(),
            )))
            .await;
        return Ok(NoToolFlow::Continue);
    }

    // Model didn't call tools. If the user asked for local actions, try implied scripts
    // (PowerShell/bash code fences) as a fallback so non-tool-calling models can still act.
    if goal_wants_actions && iter + 1 < max_iters {
        let (implied_scripts, implied_files) = implied_actions::extract(&assistant_text_clean);
        if let Some(block) = explicit_edit_scope::implied_write_issue(
            &root_user_text,
            &implied_files,
            tool_root_abs.as_deref(),
        ) {
            let feedback = format!("[implied_write_file]\nGOVERNOR BLOCKED\n\n{block}");
            messages.push(crate::task_origin::user_message(
                &feedback,
                crate::task_origin::MessageOrigin::Runtime,
            ));
            *state = AgentState::Recovery;
            let _ = tx
                .send(StreamToken::Delta(format!(
                    "[RESULT][Recovery] {feedback}\n"
                )))
                .await;
            *pending_system_hint = Some(block);
            autosave_best_effort(
                &autosaver,
                &tx,
                tool_root_abs.as_deref(),
                checkpoint.as_deref(),
                cur_cwd.as_deref(),
                &*messages,
            )
            .await;
            return Ok(NoToolFlow::Continue);
        }
        if !implied_scripts.is_empty() || !implied_files.is_empty() {
            let _ = tx
                .send(StreamToken::Delta(
                    "\n[governor] tool_call missing; executing implied actions\n".to_string(),
                ))
                .await;

            let mut implied_had_error = false;

            for im in implied_scripts {
                let command = im.script.trim().to_string();
                if command.is_empty() {
                    continue;
                }

                *state = AgentState::Executing;
                let cwd_used: Option<String> = cur_cwd.clone().or_else(|| tool_root_abs.clone());
                let cwd_used_label = cwd_used
                    .as_deref()
                    .unwrap_or("(workspace root)")
                    .to_string();

                let _ = tx
                    .send(StreamToken::Delta(format!(
                        "\n\n[IMPLIED_TOOL][{:?}] lang={}\n{command}\n[cwd] {cwd_used_label}\n",
                        *state, im.lang_hint
                    )))
                    .await;

                if let Some(block) = should_block_git_landmines(&command, tool_root_abs.as_deref())
                {
                    *state = AgentState::Recovery;
                    let _ = tx
                        .send(StreamToken::Delta(format!(
                            "[RESULT][Recovery] GOVERNOR BLOCK\n{block}\n"
                        )))
                        .await;

                    let cwd_line = format!("cwd: {cwd_used_label}");
                    let tool_output = inject_cwd(
                        &format!("GOVERNOR BLOCKED\n\n{block}\n\ncommand:\n{command}"),
                        &cwd_line,
                        None,
                    );

                    // NOTE: do not fabricate tool_call_id. Feed the block back as user text.
                    messages.push(json!({
                        "role": "user",
                        "origin": crate::task_origin::MessageOrigin::Runtime,
                        "content": format!(
                            "[implied_exec]\nlang_hint: {}\ncommand:\n{}\n\n{}",
                            im.lang_hint, command, tool_output
                        ),
                    }));
                    autosave_best_effort(
                        &autosaver,
                        &tx,
                        tool_root_abs.as_deref(),
                        checkpoint.as_deref(),
                        cur_cwd.as_deref(),
                        &*messages,
                    )
                    .await;

                    // Update memory so repeating the same blocked command triggers stronger hints.
                    let _ = mem.on_tool_result(&command, "", &block, 1);
                    *pending_system_hint = Some(block);
                    implied_had_error = true;
                    break;
                }

                let approval = approver
                    .approve(ApprovalRequest::Command {
                        command: command.clone(),
                        cwd: cwd_used.clone(),
                    })
                    .await?;
                if approval == ApprovalOutcome::Rejected {
                    *state = AgentState::Recovery;
                    let _ = tx
                        .send(StreamToken::Delta(
                            "[RESULT][Recovery] REJECTED by user\n".to_string(),
                        ))
                        .await;

                    let cwd_line = format!("cwd: {cwd_used_label}");
                    let tool_output = format!("REJECTED BY USER\n{cwd_line}\ncommand:\n{command}");
                    // NOTE: do not fabricate tool_call_id. Feed the rejection back as user text.
                    messages.push(json!({
                        "role": "user",
                        "origin": crate::task_origin::MessageOrigin::Runtime,
                        "content": format!(
                            "[implied_exec]\nlang_hint: {}\ncommand:\n{}\n\n{}",
                            im.lang_hint, command, tool_output
                        ),
                    }));
                    autosave_best_effort(
                        &autosaver,
                        &tx,
                        tool_root_abs.as_deref(),
                        checkpoint.as_deref(),
                        cur_cwd.as_deref(),
                        &*messages,
                    )
                    .await;
                    *pending_system_hint = Some(
                    "The user rejected the command. Choose a safer alternative or explain why it is necessary before retrying."
                        .to_string(),
                );
                    implied_had_error = true;
                    break;
                }

                let cargo_input_probe =
                    cwd_used
                        .as_deref()
                        .zip(tool_root_abs.as_deref())
                        .and_then(|(cwd, root)| {
                            CargoInputProbe::start(&*messages, &command, cwd, root)
                        });
                let exec_cmd = wrap_exec_with_pwd(&command);
                let exec_result = exec::run_command(&exec_cmd, cwd_used.as_deref()).await;
                let (stdout, stderr, exit_code) = match exec_result {
                    Ok(r) => (r.stdout, r.stderr, r.exit_code),
                    Err(e) => (String::new(), e.to_string(), -1),
                };

                // Update cwd from marker output.
                let (stdout, pwd_after) = strip_pwd_marker(&stdout);
                let mut cwd_after_note: Option<String> = None;
                if let Some(p) = pwd_after {
                    if let Some(ref root) = tool_root_abs {
                        if is_within_root(&p, root) {
                            *cur_cwd = Some(p);
                        } else {
                            cwd_after_note = Some(format!(
                                "NOTE: cwd_after was outside tool_root; ignored: {p}"
                            ));
                        }
                    } else {
                        *cur_cwd = Some(p);
                    }
                }

                let escaped_tool_root = cwd_after_note.is_some();
                let cwd_after_label = cur_cwd
                    .as_deref()
                    .unwrap_or(cwd_used_label.as_str())
                    .to_string();
                let cwd_line = if cwd_used_label == cwd_after_label {
                    format!("cwd: {cwd_used_label}")
                } else {
                    format!("cwd: {cwd_used_label}\ncwd_after: {cwd_after_label}")
                };

                *state = AgentState::Verifying;
                let suspicious_reason = if exit_code == 0 {
                    suspicious_success_reason(&stdout, &stderr)
                } else {
                    None
                };
                let effective_exit_code =
                    if exit_code == 0 && (suspicious_reason.is_some() || escaped_tool_root) {
                        1
                    } else {
                        exit_code
                    };

                let note = cwd_after_note.as_deref();
                let tool_output = if effective_exit_code == 0 {
                    let base = build_ok_tool_output(&stdout);
                    inject_cwd(&base, &cwd_line, note)
                } else {
                    let mut out = build_failed_tool_output(&stdout, &stderr, effective_exit_code);
                    if let Some(reason) = suspicious_reason {
                        out = format!(
                        "NOTE: command returned exit_code=0 but was treated as failure.\nreason: {reason}\n\n{out}"
                    );
                    }
                    if escaped_tool_root && exit_code == 0 {
                        out = format!(
                            "NOTE: command escaped tool_root and was treated as failure.\n\
This is blocked to prevent nested-repo / accidental repo-root modifications.\n\n{out}"
                        );
                    }
                    inject_cwd(&out, &cwd_line, note)
                };

                let result_label = if effective_exit_code == 0 {
                    format!("[RESULT][{:?}] exit=0\n", *state)
                } else {
                    format!("[RESULT][{:?}] exit={effective_exit_code} !\n", *state)
                };
                let _ = tx.send(StreamToken::Delta(result_label)).await;

                // NOTE: do not fabricate tool_call_id. Feed the result back as user text.
                let cargo_inputs =
                    cargo_input_probe.map(|p| p.finish(effective_exit_code == 0, &stdout, &stderr));
                messages.push(CargoInputReceipt::attach(
                    cargo_inputs,
                    json!({
                        "role": "user",
                        "origin": crate::task_origin::MessageOrigin::Runtime,
                        "content": format!(
                            "[implied_exec]\nlang_hint: {}\ncommand:\n{}\n\n{}",
                            im.lang_hint, command, tool_output
                        ),
                    }),
                ));
                autosave_best_effort(
                    &autosaver,
                    &tx,
                    tool_root_abs.as_deref(),
                    checkpoint.as_deref(),
                    cur_cwd.as_deref(),
                    &*messages,
                )
                .await;

                if effective_exit_code != 0 {
                    *state = AgentState::Recovery;
                } else {
                    *state = AgentState::Planning;
                }

                *pending_system_hint = if escaped_tool_root {
                    Some(
                    "SANDBOX BREACH: Your command ended outside tool_root.\n\
Action: re-run from tool_root, avoid `cd ..` / absolute paths, and verify `pwd` stays under tool_root."
                        .to_string(),
                )
                } else {
                    mem.on_tool_result(&command, &stdout, &stderr, effective_exit_code)
                };

                if effective_exit_code != 0 {
                    implied_had_error = true;
                    break;
                }
            }

            // 2) write NEW files from markdown blocks (no overwrites)
            if !implied_had_error && !implied_files.is_empty() {
                for imf in implied_files.into_iter().take(6) {
                    let path = imf.path.trim().to_string();
                    if path.is_empty() {
                        continue;
                    }

                    let base = tool_root_abs.as_deref();

                    // Resolve safe path and check existence.
                    let abs = match crate::file_tools::resolve_safe_path(&path, base) {
                        Ok(p) => p,
                        Err(e) => {
                            *state = AgentState::Recovery;
                            let msg = format!("ERROR: {e}\npath: {path}\n(action skipped)");
                            let _ = tx
                                .send(StreamToken::Delta(format!(
                                    "[RESULT_FILE_ERR] {}\n",
                                    msg.lines().next().unwrap_or("ERROR")
                                )))
                                .await;
                            messages.push(json!({
                                "role": "user",
                                "origin": crate::task_origin::MessageOrigin::Runtime,
                                "content": format!(
                                    "[implied_write_file]\npath: {}\nlang_hint: {}\n\n{}",
                                    path, imf.lang_hint, msg
                                ),
                            }));
                            autosave_best_effort(
                                &autosaver,
                                &tx,
                                tool_root_abs.as_deref(),
                                checkpoint.as_deref(),
                                cur_cwd.as_deref(),
                                &*messages,
                            )
                            .await;
                            *pending_system_hint = Some(format!(
                                "Implied file write failed due to an unsafe or invalid path.\n\
Fix: use a relative path under tool_root, without '..' or absolute paths.\n\
path: {path}"
                            ));
                            break;
                        }
                    };

                    let exists = abs
                        .metadata()
                        .ok()
                        .map(|m| m.is_file() || m.is_dir())
                        .unwrap_or(false);

                    let _ = tx
                        .send(StreamToken::Delta(format!(
                            "\n\n[IMPLIED_WRITE] {}\n",
                            path
                        )))
                        .await;

                    if exists {
                        // Non-fatal: skip instead of blocking the whole run.
                        let msg = format!(
                            "SKIP: '{path}' already exists. I will NOT overwrite it.\n\
Action required: call read_file(path), then use patch_file/apply_diff to modify it safely."
                        );
                        messages.push(json!({
                            "role": "user",
                            "origin": crate::task_origin::MessageOrigin::Runtime,
                            "content": format!(
                                "[implied_write_file]\npath: {}\nlang_hint: {}\n\n{}",
                                path, imf.lang_hint, msg
                            ),
                        }));
                        autosave_best_effort(
                            &autosaver,
                            &tx,
                            tool_root_abs.as_deref(),
                            checkpoint.as_deref(),
                            cur_cwd.as_deref(),
                            &*messages,
                        )
                        .await;
                        continue;
                    }

                    // Approval: show a compact preview.
                    let before_hash = hash_text("");
                    let after_hash = hash_text(&imf.content);
                    let preview = simple_before_after("", &imf.content);
                    let approval = approver
                        .approve(ApprovalRequest::Edit {
                            action: "implied_write_file".to_string(),
                            path: path.clone(),
                            preview,
                        })
                        .await?;
                    if approval == ApprovalOutcome::Rejected {
                        *state = AgentState::Recovery;
                        let msg = format!(
                        "REJECTED BY USER\naction: implied_write_file\npath: {path}\n(no changes applied)"
                    );
                        messages.push(json!({
                            "role": "user",
                            "origin": crate::task_origin::MessageOrigin::Runtime,
                            "content": format!(
                                "[implied_write_file]\npath: {}\nlang_hint: {}\n\n{}",
                                path, imf.lang_hint, msg
                            ),
                        }));
                        autosave_best_effort(
                            &autosaver,
                            &tx,
                            tool_root_abs.as_deref(),
                            checkpoint.as_deref(),
                            cur_cwd.as_deref(),
                            &*messages,
                        )
                        .await;
                        *pending_system_hint = Some(
                        "The user rejected the edit. Choose a safer alternative or ask again with a smaller change."
                            .to_string(),
                    );
                        break;
                    }

                    *state = AgentState::Executing;
                    let mut cargo_inputs = None;
                    let (mut r_text, r_err) =
                        crate::cargo_inputs::write_file(&path, &imf.content, base, &mut *messages);
                    if !r_err
                        && crate::execution_evidence::file_edit_succeeded("write_file", &r_text)
                    {
                        // Update cache (canonical absolute path string).
                        let cache_key = abs.to_string_lossy().into_owned();
                        file_cache.insert(cache_key, imf.content.clone());
                        r_text.push_str(&format!(
                            "\n[hash] before={} after={}",
                            fmt_hash(before_hash),
                            fmt_hash(after_hash)
                        ));
                        if let Some(ref cmd) = test_cmd {
                            if let Some(ref root) = tool_root_abs {
                                r_text.push_str(
                                    &automatic_test::observed(
                                        cmd,
                                        root,
                                        &*messages,
                                        &mut cargo_inputs,
                                    )
                                    .await,
                                );
                            }
                        }
                    }

                    let first_line = r_text.lines().next().unwrap_or("").to_string();
                    if r_err {
                        *state = AgentState::Recovery;
                        let _ = tx
                            .send(StreamToken::Delta(format!(
                                "[RESULT_FILE_ERR] {first_line}\n"
                            )))
                            .await;
                        *pending_system_hint = Some(format!(
                            "Implied file write failed: {first_line}\n\
Fix the path/permissions, or switch to explicit tools (write_file/read_file/patch_file)."
                        ));
                        messages.push(json!({
                            "role": "user",
                            "origin": crate::task_origin::MessageOrigin::Runtime,
                            "content": format!(
                                "[implied_write_file]\npath: {}\nlang_hint: {}\n\n{}",
                                path, imf.lang_hint, r_text
                            ),
                        }));
                        autosave_best_effort(
                            &autosaver,
                            &tx,
                            tool_root_abs.as_deref(),
                            checkpoint.as_deref(),
                            cur_cwd.as_deref(),
                            &*messages,
                        )
                        .await;
                        break;
                    }

                    let _ = tx
                        .send(StreamToken::Delta(format!("[RESULT_FILE] {first_line}\n")))
                        .await;
                    messages.push(CargoInputReceipt::attach(
                        cargo_inputs,
                        json!({
                            "role": "user",
                            "origin": crate::task_origin::MessageOrigin::Runtime,
                            "content": format!(
                                "[implied_write_file]\npath: {}\nlang_hint: {}\n\n{}",
                                path, imf.lang_hint, r_text
                            ),
                        }),
                    ));
                    autosave_best_effort(
                        &autosaver,
                        &tx,
                        tool_root_abs.as_deref(),
                        checkpoint.as_deref(),
                        cur_cwd.as_deref(),
                        &*messages,
                    )
                    .await;
                }
            }

            return Ok(NoToolFlow::Continue);
        }
    }

    // An empty/text-only turn cannot trade the original report contract
    // for an automatic stop check. Keep current proof and request the
    // native closeout, within the same shared request budget.
    if let Some(hint) = authored_final_answer_hint(&root_user_text) {
        let hint = format!("[Closeout retry] No final tool call was received. Complete any pending work and exact checks, then author the required report.\n{hint}");
        messages.push(crate::task_origin::user_message(
            &hint,
            crate::task_origin::MessageOrigin::Runtime,
        ));
        *pending_system_hint = Some(hint);
        *state = AgentState::Planning;
        let _ = tx
            .send(StreamToken::Delta(
                "\n[agent] Required final report is still pending; requesting native closeout.\n"
                    .into(),
            ))
            .await;
        autosave_best_effort(
            &autosaver,
            &tx,
            tool_root_abs.as_deref(),
            checkpoint.as_deref(),
            cur_cwd.as_deref(),
            &*messages,
        )
        .await;
        return Ok(NoToolFlow::Continue);
    }

    if should_auto_run_goal_checks(command_approval_required, max_iters) {
        if let Some(goal_root) = cur_cwd.as_deref().or(tool_root_abs.as_deref()) {
            let mut ran_goal_checks = false;
            let mut goal_check_blocked = false;
            let max_attempts = goal_check_max_attempts();
            for key in goal_check_order() {
                let wants_goal = match key.as_str() {
                    "repo" => wants_repo_goal,
                    "tests" => wants_test_goal,
                    "build" => wants_build_goal,
                    _ => false,
                };
                if !wants_goal {
                    continue;
                }
                let Some(status) = goal_checks.get_mut(key.as_str()) else {
                    continue;
                };
                if status.ok || status.attempts >= max_attempts {
                    continue;
                }
                status.attempts += 1;
                ran_goal_checks = true;
                *state = AgentState::Verifying;

                match key.as_str() {
                    "repo" => {
                        let _ = tx
                            .send(StreamToken::Delta(format!(
                                "\n{}\n",
                                governor_contract::goal_check_repo_start_message()
                            )))
                            .await;
                        let missing = repo_goal_missing_labels(goal_root).await;
                        if !missing.is_empty() {
                            let block = governor_contract::goal_check_repo_missing_message(
                                missing.join(", ").as_str(),
                            );
                            messages.push(crate::task_origin::user_message(
                                &block,
                                crate::task_origin::MessageOrigin::Runtime,
                            ));
                            autosave_best_effort(
                                &autosaver,
                                &tx,
                                tool_root_abs.as_deref(),
                                checkpoint.as_deref(),
                                cur_cwd.as_deref(),
                                &*messages,
                            )
                            .await;
                            *state = AgentState::Planning;
                            let _ = tx
                                .send(StreamToken::GovernorState(build_governor_state(
                                    *state,
                                    &*recovery,
                                    &*mem,
                                    file_tool_consec_failures,
                                    last_mutation_step,
                                    *last_verify_ok_step,
                                    last_reflection.as_ref(),
                                )))
                                .await;
                            goal_check_blocked = true;
                            break;
                        }
                        status.ok = true;
                        let _ = tx
                            .send(StreamToken::Delta(format!(
                                "{}\n",
                                governor_contract::goal_check_repo_ok_message()
                            )))
                            .await;
                    }
                    "tests" => {
                        let Some(command) = goal_check_runner_command(
                            VerificationLevel::Behavioral,
                            goal_root,
                            test_cmd.as_deref(),
                        ) else {
                            let summary = goal_check_runner_summary(VerificationLevel::Behavioral);
                            let support_line = goal_check_support_line(
                                summary.as_str(),
                                governor_contract::goal_check_tests_runner_fallback_message()
                                    .as_str(),
                            );
                            let block = governor_contract::goal_check_tests_no_runner_message(
                                support_line.as_str(),
                            );
                            messages.push(crate::task_origin::user_message(
                                &block,
                                crate::task_origin::MessageOrigin::Runtime,
                            ));
                            autosave_best_effort(
                                &autosaver,
                                &tx,
                                tool_root_abs.as_deref(),
                                checkpoint.as_deref(),
                                cur_cwd.as_deref(),
                                &*messages,
                            )
                            .await;
                            *state = AgentState::Planning;
                            let _ = tx
                                .send(StreamToken::GovernorState(build_governor_state(
                                    *state,
                                    &*recovery,
                                    &*mem,
                                    file_tool_consec_failures,
                                    last_mutation_step,
                                    *last_verify_ok_step,
                                    last_reflection.as_ref(),
                                )))
                                .await;
                            goal_check_blocked = true;
                            break;
                        };
                        let result =
                            run_goal_check_command("tests", command.as_str(), goal_root, &tx).await;
                        if !result.passed {
                            let block = governor_contract::goal_check_tests_failed_message(
                                goal_check_class_line(&result.error_class).as_str(),
                                goal_check_digest_line(&result.digest).as_str(),
                            );
                            messages.push(crate::task_origin::user_message(
                                &block,
                                crate::task_origin::MessageOrigin::Runtime,
                            ));
                            autosave_best_effort(
                                &autosaver,
                                &tx,
                                tool_root_abs.as_deref(),
                                checkpoint.as_deref(),
                                cur_cwd.as_deref(),
                                &*messages,
                            )
                            .await;
                            *state = AgentState::Recovery;
                            recovery.stage = Some(RecoveryStage::Fix);
                            let _ = tx
                                .send(StreamToken::GovernorState(build_governor_state(
                                    *state,
                                    &*recovery,
                                    &*mem,
                                    file_tool_consec_failures,
                                    last_mutation_step,
                                    *last_verify_ok_step,
                                    last_reflection.as_ref(),
                                )))
                                .await;
                            goal_check_blocked = true;
                            break;
                        }
                        status.ok = true;
                        working_mem.remember_successful_verification(&result.command);
                        *last_behavioral_verify_ok_step = Some(*step_seq);
                        *last_verify_ok_step = effective_verify_ok_step(
                            *required_verification,
                            *last_build_verify_ok_step,
                            *last_behavioral_verify_ok_step,
                        );
                        recovery.on_exec_result(
                            ExecKind::Verify,
                            Some(VerificationLevel::Behavioral),
                            true,
                        );
                    }
                    "build" => {
                        let Some(command) =
                            goal_check_runner_command(VerificationLevel::Build, goal_root, None)
                        else {
                            let summary = goal_check_runner_summary(VerificationLevel::Build);
                            let support_line = goal_check_support_line(
                                summary.as_str(),
                                governor_contract::goal_check_build_runner_fallback_message()
                                    .as_str(),
                            );
                            let block = governor_contract::goal_check_build_no_runner_message(
                                support_line.as_str(),
                            );
                            messages.push(crate::task_origin::user_message(
                                &block,
                                crate::task_origin::MessageOrigin::Runtime,
                            ));
                            autosave_best_effort(
                                &autosaver,
                                &tx,
                                tool_root_abs.as_deref(),
                                checkpoint.as_deref(),
                                cur_cwd.as_deref(),
                                &*messages,
                            )
                            .await;
                            *state = AgentState::Planning;
                            let _ = tx
                                .send(StreamToken::GovernorState(build_governor_state(
                                    *state,
                                    &*recovery,
                                    &*mem,
                                    file_tool_consec_failures,
                                    last_mutation_step,
                                    *last_verify_ok_step,
                                    last_reflection.as_ref(),
                                )))
                                .await;
                            goal_check_blocked = true;
                            break;
                        };
                        let result =
                            run_goal_check_command("build", command.as_str(), goal_root, &tx).await;
                        if !result.passed {
                            let block = governor_contract::goal_check_build_failed_message(
                                goal_check_class_line(&result.error_class).as_str(),
                                goal_check_digest_line(&result.digest).as_str(),
                            );
                            messages.push(crate::task_origin::user_message(
                                &block,
                                crate::task_origin::MessageOrigin::Runtime,
                            ));
                            autosave_best_effort(
                                &autosaver,
                                &tx,
                                tool_root_abs.as_deref(),
                                checkpoint.as_deref(),
                                cur_cwd.as_deref(),
                                &*messages,
                            )
                            .await;
                            *state = AgentState::Recovery;
                            recovery.stage = Some(RecoveryStage::Fix);
                            let _ = tx
                                .send(StreamToken::GovernorState(build_governor_state(
                                    *state,
                                    &*recovery,
                                    &*mem,
                                    file_tool_consec_failures,
                                    last_mutation_step,
                                    *last_verify_ok_step,
                                    last_reflection.as_ref(),
                                )))
                                .await;
                            goal_check_blocked = true;
                            break;
                        }
                        status.ok = true;
                        working_mem.remember_successful_verification(&result.command);
                        *last_build_verify_ok_step = Some(*step_seq);
                        *last_verify_ok_step = effective_verify_ok_step(
                            *required_verification,
                            *last_build_verify_ok_step,
                            *last_behavioral_verify_ok_step,
                        );
                        recovery.on_exec_result(
                            ExecKind::Verify,
                            Some(VerificationLevel::Build),
                            true,
                        );
                    }
                    _ => {}
                }
            }

            if goal_check_blocked {
                return Ok(NoToolFlow::Continue);
            }
            if ran_goal_checks {
                task_outcome.verified_goal_checks(
                    &root_user_text,
                    &[
                        (wants_repo_goal, goal_checks.repo.ok),
                        (wants_test_goal, goal_checks.tests.ok),
                        (wants_build_goal, goal_checks.build.ok),
                    ],
                );
                *state = AgentState::Done;
                let _ = tx
                    .send(StreamToken::Delta(format!(
                        "\n{}\n",
                        governor_contract::goal_check_all_passed_message()
                    )))
                    .await;
                let _ = tx
                    .send(StreamToken::GovernorState(build_governor_state(
                        *state,
                        &*recovery,
                        &*mem,
                        file_tool_consec_failures,
                        last_mutation_step,
                        *last_verify_ok_step,
                        last_reflection.as_ref(),
                    )))
                    .await;
                return Ok(NoToolFlow::Break);
            }
        }
    }

    if let Some(final_text) = maybe_build_read_only_auto_final_answer(
        root_read_only,
        &root_user_text,
        active_plan.as_ref(),
        &*observation_evidence,
        &*messages,
        &*working_mem,
    ) {
        task_outcome.finalized(&final_text);
        *state = AgentState::Done;
        messages.push(json!({"role": "assistant", "content": final_text.clone()}));
        autosave_best_effort(
            &autosaver,
            &tx,
            tool_root_abs.as_deref(),
            checkpoint.as_deref(),
            cur_cwd.as_deref(),
            &*messages,
        )
        .await;
        let _ = tx
            .send(StreamToken::GovernorState(build_governor_state(
                *state,
                &*recovery,
                &*mem,
                file_tool_consec_failures,
                last_mutation_step,
                *last_verify_ok_step,
                last_reflection.as_ref(),
            )))
            .await;
        let _ = tx
            .send(StreamToken::Delta(format!(
            "\n[agent] auto-finalized read-only inspection after no-tool turn.\n\n{final_text}\n"
        )))
            .await;
        return Ok(NoToolFlow::Break);
    }

    if root_read_only
        && provider_turn::allows_synthetic_tools(&cfg.base_url)
        && (read_only_diagnose_streak >= 2
            || (*tool_calls_this_run == 0 && iter + 1 >= first_action_deadline))
        && *read_only_diagnose_rescue_count < 3
        && iter + 1 < max_iters
    {
        if active_plan.is_none() {
            let synthetic_plan = synthetic_read_only_observation_plan(&root_user_text);
            adopt_valid_plan(
                &synthetic_plan,
                &mut *working_mem,
                &mut *assumption_ledger,
                &mut *active_plan,
                &mut *intent_required_verification,
                path_required_verification,
                &mut *required_verification,
                &mut *recovery,
                &mut *last_verify_ok_step,
                *last_build_verify_ok_step,
                *last_behavioral_verify_ok_step,
            );
        }

        if let Some(action) = choose_read_only_diagnose_rescue_action(
            &root_user_text,
            active_plan.as_ref(),
            &*observation_evidence,
        ) {
            *read_only_diagnose_rescue_count = read_only_diagnose_rescue_count.saturating_add(1);
            match action {
                ReadOnlyDiagnoseRescueAction::Search { pattern, dir } => {
                    *tool_calls_this_run = tool_calls_this_run.saturating_add(1);
                    emit_telemetry_event(
                        &tx,
                        "read_only_autorescue",
                        json!({
                            "action": "search_files",
                            "pattern": pattern,
                            "dir": dir,
                            "diagnose_streak": read_only_diagnose_streak,
                            "count": *read_only_diagnose_rescue_count,
                        }),
                    )
                    .await;
                    let arguments = json!({
                        "pattern": pattern,
                        "dir": dir,
                    })
                    .to_string();
                    let tc = ToolCallData {
                        thought_signature: None,
                        id: format!(
                            "auto_ro_diag_search_{}_{}",
                            iter, *read_only_diagnose_rescue_count
                        ),
                        name: "search_files".to_string(),
                        arguments: arguments.clone(),
                    };
                    let _ = tx
                    .send(StreamToken::Delta(format!(
                        "\n[agent] read-only diagnose rescue -> search_files(pattern=\"{}\", dir=\"{}\")\n",
                        pattern, dir
                    )))
                    .await;
                    let _ = tx
                        .send(StreamToken::Delta(format!(
                            "\n\n[SEARCH_FILES] {pattern}\n"
                        )))
                        .await;
                    let _ = tx.send(StreamToken::ToolCall(tc.clone())).await;
                    messages.push(json!({
                        "role": "assistant",
                        "content": "",
                        "tool_calls": [tc.to_json()]
                    }));
                    *step_seq = step_seq.saturating_add(1);
                    let (result, is_error) = crate::file_tools::tool_search_files(
                        pattern.as_str(),
                        dir.as_str(),
                        false,
                        tool_root_abs.as_deref(),
                    );
                    let parsed_search_paths = if is_error {
                        Vec::new()
                    } else {
                        parse_search_result_paths(result.as_str())
                    };
                    recovery.on_diagnostic_result(!is_error);
                    let first_line = result.lines().next().unwrap_or("").to_string();
                    if is_error {
                        let _ = tx
                            .send(StreamToken::Delta(format!(
                                "[RESULT_FILE_ERR] {first_line}\n"
                            )))
                            .await;
                        *pending_system_hint = Some(format!(
                            "Read-only diagnose rescue search failed: {first_line}"
                        ));
                        *state = AgentState::Recovery;
                    } else {
                        let _ = tx
                            .send(StreamToken::Delta(format!(
                                "[RESULT_SEARCH] {first_line}\n"
                            )))
                            .await;
                        *state = if recovery.in_recovery() {
                            AgentState::Recovery
                        } else {
                            AgentState::Planning
                        };
                        update_working_memory_after_non_exec(
                            &mut *working_mem,
                            "search_files",
                            false,
                            None,
                            None,
                            test_cmd.as_deref(),
                        );
                        let command =
                            canonicalize_tool_call_command("search_files", arguments.as_str())
                                .unwrap_or_else(|| {
                                    format!(
                                        "search_files(pattern={}, dir={})",
                                        compact_one_line(pattern.as_str(), 160),
                                        compact_one_line(dir.as_str(), 160)
                                    )
                                });
                        observation_evidence.remember_search(
                            command.as_str(),
                            pattern.as_str(),
                            parse_search_hit_count(result.as_str()),
                            parsed_search_paths.as_slice(),
                        );
                        sync_observation_cache_autosave(&autosaver, &*observation_evidence);
                        assumption_ledger.refresh_confirmations(&*working_mem);
                        *pending_system_hint = active_plan.as_ref().and_then(|plan| {
                            build_read_only_search_to_read_hint(
                                &root_user_text,
                                plan,
                                parsed_search_paths.as_slice(),
                                &*observation_evidence,
                            )
                            .or_else(|| {
                                build_read_only_completion_hint(
                                    &root_user_text,
                                    plan,
                                    &*observation_evidence,
                                    &*messages,
                                    &*working_mem,
                                )
                            })
                        });
                    }

                    let history_result = if is_error {
                        result.clone()
                    } else {
                        compact_success_tool_result_for_history("search_files", &result)
                    };
                    messages.push(json!({
                        "role": "tool",
                        "tool_call_id": tc.id,
                        "content": history_result,
                    }));
                    autosave_best_effort(
                        &autosaver,
                        &tx,
                        tool_root_abs.as_deref(),
                        checkpoint.as_deref(),
                        cur_cwd.as_deref(),
                        &*messages,
                    )
                    .await;
                    let _ = tx
                        .send(StreamToken::GovernorState(build_governor_state(
                            *state,
                            &*recovery,
                            &*mem,
                            file_tool_consec_failures,
                            last_mutation_step,
                            *last_verify_ok_step,
                            last_reflection.as_ref(),
                        )))
                        .await;
                    return Ok(NoToolFlow::Continue);
                }
                ReadOnlyDiagnoseRescueAction::Read { path } => {
                    *tool_calls_this_run = tool_calls_this_run.saturating_add(1);
                    emit_telemetry_event(
                        &tx,
                        "read_only_autorescue",
                        json!({
                            "action": "read_file",
                            "path": path,
                            "diagnose_streak": read_only_diagnose_streak,
                            "count": *read_only_diagnose_rescue_count,
                        }),
                    )
                    .await;
                    let arguments = json!({ "path": path }).to_string();
                    let tc = ToolCallData {
                        thought_signature: None,
                        id: format!(
                            "auto_ro_diag_read_{}_{}",
                            iter, *read_only_diagnose_rescue_count
                        ),
                        name: "read_file".to_string(),
                        arguments: arguments.clone(),
                    };
                    let _ = tx
                        .send(StreamToken::Delta(format!(
                            "\n[agent] read-only diagnose rescue -> read_file(path=\"{}\")\n",
                            path
                        )))
                        .await;
                    let _ = tx
                        .send(StreamToken::Delta(format!("\n\n[READ_FILE] {path}\n")))
                        .await;
                    let _ = tx.send(StreamToken::ToolCall(tc.clone())).await;
                    messages.push(json!({
                        "role": "assistant",
                        "content": "",
                        "tool_calls": [tc.to_json()]
                    }));
                    *step_seq = step_seq.saturating_add(1);
                    let cache_key = crate::file_tools::resolve_safe_path(
                        path.as_str(),
                        tool_root_abs.as_deref(),
                    )
                    .ok()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_else(|| {
                        format!("{}|{}", path, tool_root_abs.as_deref().unwrap_or(""))
                    });
                    if assumption_gate::needs_fresh_file_read(&*assumption_ledger, &path) {
                        file_cache.remove(&cache_key);
                    }
                    let (result, is_error) = if let Some(cached) = file_cache.get(&cache_key) {
                        let header = cached.lines().next().unwrap_or(path.as_str()).to_string();
                        let _ = tx
                            .send(StreamToken::Delta(format!("[CACHE_HIT] {header}\n")))
                            .await;
                        (
                            format!(
                                "{} [⚡ cached — unchanged since last read]\n{cached}",
                                header
                            ),
                            false,
                        )
                    } else {
                        let (content, err) = crate::file_tools::tool_read_file(
                            path.as_str(),
                            tool_root_abs.as_deref(),
                        );
                        if !err {
                            file_cache.insert(cache_key.clone(), content.clone());
                        }
                        (content, err)
                    };
                    recovery.on_diagnostic_result(!is_error);
                    let first_line = result.lines().next().unwrap_or("").to_string();
                    if is_error {
                        let _ = tx
                            .send(StreamToken::Delta(format!(
                                "[RESULT_FILE_ERR] {first_line}\n"
                            )))
                            .await;
                        *pending_system_hint = Some(format!(
                            "Read-only diagnose rescue read failed: {first_line}"
                        ));
                        *state = AgentState::Recovery;
                    } else {
                        let _ = tx
                            .send(StreamToken::Delta(format!("[RESULT_FILE] {first_line}\n")))
                            .await;
                        *state = if recovery.in_recovery() {
                            AgentState::Recovery
                        } else {
                            AgentState::Planning
                        };
                        update_working_memory_after_non_exec(
                            &mut *working_mem,
                            "read_file",
                            false,
                            None,
                            None,
                            test_cmd.as_deref(),
                        );
                        let command =
                            canonicalize_tool_call_command("read_file", arguments.as_str())
                                .unwrap_or_else(|| {
                                    format!(
                                        "read_file(path={})",
                                        compact_one_line(path.as_str(), 160)
                                    )
                                });
                        let observed_path = parse_read_file_result_path(result.as_str())
                            .unwrap_or_else(|| compact_one_line(path.as_str(), 160));
                        observation_evidence
                            .remember_read(command.as_str(), observed_path.as_str());
                        sync_observation_cache_autosave(&autosaver, &*observation_evidence);
                        assumption_gate::confirm_file_existence_after_result(
                            &mut *assumption_ledger,
                            "read_file",
                            &path,
                            &result,
                        );
                        assumption_ledger.refresh_confirmations(&*working_mem);
                        *pending_system_hint = active_plan.as_ref().and_then(|plan| {
                            build_read_only_completion_hint(
                                &root_user_text,
                                plan,
                                &*observation_evidence,
                                &*messages,
                                &*working_mem,
                            )
                        });
                    }

                    let history_result = if is_error {
                        result.clone()
                    } else {
                        compact_success_tool_result_for_history("read_file", &result)
                    };
                    messages.push(json!({
                        "role": "tool",
                        "tool_call_id": tc.id,
                        "content": history_result,
                    }));
                    autosave_best_effort(
                        &autosaver,
                        &tx,
                        tool_root_abs.as_deref(),
                        checkpoint.as_deref(),
                        cur_cwd.as_deref(),
                        &*messages,
                    )
                    .await;
                    let _ = tx
                        .send(StreamToken::GovernorState(build_governor_state(
                            *state,
                            &*recovery,
                            &*mem,
                            file_tool_consec_failures,
                            last_mutation_step,
                            *last_verify_ok_step,
                            last_reflection.as_ref(),
                        )))
                        .await;
                    return Ok(NoToolFlow::Continue);
                }
            }
        }
    }

    // Verified text/read-only completion above remains available. Only an
    // unfinished no-tool turn needs another native Google function call.
    if let Some(hint) = provider_turn::retry_unfinished_text_turn(&cfg.base_url, &mut *messages) {
        *pending_system_hint = Some(hint.to_string());
        let _ = tx
            .send(StreamToken::Delta(
                "\n[provider] No tool executed; requesting a native function call.\n".to_string(),
            ))
            .await;
        autosave_best_effort(
            &autosaver,
            &tx,
            tool_root_abs.as_deref(),
            checkpoint.as_deref(),
            cur_cwd.as_deref(),
            &*messages,
        )
        .await;
        return Ok(NoToolFlow::Continue);
    }

    // Common failure mode: model "explains what to do" but never calls tools.
    // Try once to force a tool call so long sessions keep moving.
    if !*forced_tool_once && iter + 1 < max_iters {
        *forced_tool_once = true;
        *state = AgentState::Recovery;
        let note = if goal_wants_actions {
            "\
[Tool enforcement]\n\
You MUST call ONE tool now to act locally (exec/read_file/write_file/patch_file/apply_diff/search_files/list_dir/glob/done).\n\
Do NOT respond with instructions only.\n\
Start with ONE minimal safe action, then verify and continue."
        } else {
            "\
[Tool enforcement]\n\
You responded without calling any tool.\n\
You MUST call ONE tool now (exec/read_file/write_file/patch_file/apply_diff/search_files/list_dir/glob/done).\n\
Do NOT respond with text-only instructions."
        };
        let _ = tx
            .send(StreamToken::Delta(
                "\n[governor] tool_call missing; forcing tool call\n".to_string(),
            ))
            .await;
        messages.push(json!({"role":"system","content": note}));
        autosave_best_effort(
            &autosaver,
            &tx,
            tool_root_abs.as_deref(),
            checkpoint.as_deref(),
            cur_cwd.as_deref(),
            &*messages,
        )
        .await;
        let _ = tx
            .send(StreamToken::GovernorState(build_governor_state(
                *state,
                &*recovery,
                &*mem,
                file_tool_consec_failures,
                last_mutation_step,
                *last_verify_ok_step,
                last_reflection.as_ref(),
            )))
            .await;
        return Ok(NoToolFlow::Continue);
    }

    *state = AgentState::Done;
    let _ = tx
        .send(StreamToken::Delta(format!(
            "\n[agent] *state: {:?}\n",
            *state
        )))
        .await;

    Ok(NoToolFlow::Break)
}
