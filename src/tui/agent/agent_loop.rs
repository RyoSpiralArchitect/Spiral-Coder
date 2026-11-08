use super::*;

#[allow(dead_code)] // public CLI/API surface preserved
pub async fn run_agentic(
    messages_in: Vec<ChatMessage>,
    cfg: &RunConfig,
    tool_root: Option<&str>,
    max_iters: usize,
    tx: mpsc::Sender<StreamToken>,
    project_context: Option<String>,
    agents_md: Option<String>,
    // Command to run after every successful file edit (e.g. "cargo test 2>&1").
    test_cmd: Option<String>,
    command_approval_required: bool,
    realize_preset: Option<RealizePreset>,
    approver: &dyn Approver,
) -> Result<AgenticEndState> {
    let messages_json: Vec<serde_json::Value> = messages_in
        .iter()
        .map(|m| json!({"role": m.role, "content": m.content}))
        .collect();
    let start = AgenticStartState {
        messages: messages_json,
        checkpoint: None,
        cur_cwd: None,
        observation_cache: None,
        session_bridge: None,
        create_checkpoint: true,
    };
    run_agentic_json(
        start,
        cfg,
        tool_root,
        max_iters,
        tx,
        project_context,
        agents_md,
        test_cmd,
        command_approval_required,
        realize_preset,
        None,
        approver,
    )
    .await
}

pub async fn run_agentic_json(
    start: AgenticStartState,
    cfg: &RunConfig,
    tool_root: Option<&str>,
    max_iters: usize,
    tx: mpsc::Sender<StreamToken>,
    project_context: Option<String>,
    agents_md: Option<String>,
    // Command to run after every successful file edit (e.g. "cargo test 2>&1").
    test_cmd: Option<String>,
    command_approval_required: bool,
    realize_preset: Option<RealizePreset>,
    autosaver: Option<Arc<crate::agent_session::SessionAutoSaver>>,
    approver: &dyn Approver,
) -> Result<AgenticEndState> {
    if matches!(cfg.provider, ProviderKind::Anthropic | ProviderKind::Hf) {
        return Err(anyhow!(
            "Coder agentic loop requires a tool-calling OpenAI-compatible Chat Completions API.\n\
Unsupported provider for Coder: {}\n\
Fix: use --provider openai-compatible (or --provider mistral).",
            cfg.provider
        ));
    }

    let client = reqwest::Client::new();
    let tools = json!([
        exec_tool_def(),
        read_file_tool_def(),
        write_file_tool_def(),
        patch_file_tool_def(),
        apply_diff_tool_def(),
        search_files_tool_def(),
        list_dir_tool_def(),
        glob_tool_def(),
        done_tool_def(&cfg.base_url),
    ]);
    let loop_setup::LoopSetup {
        mut state,
        mut task_outcome,
        mut pending_system_hint,
        mut reflection_required,
        mut impact_required,
        mut reflection_trigger_sig,
        mut reflection_guard,
        mut forced_tool_once,
        mut tool_calls_this_run,
        mut budget_warned,
        mut edit_failures,
        mut file_tool_consec_failures,
        root_user_text,
        root_read_only,
        task_harness,
        exec_verification_context,
        progress_context,
        wants_repo_goal,
        wants_test_goal,
        wants_build_goal,
        mut goal_checks,
        goal_wants_actions,
        mut intent_required_verification,
        task_contract,
        instruction_resolver,
        realize_cfg,
        mut latent_plan,
        mut realize_metrics,
        mut messages,
        mut active_plan,
        mut path_required_verification,
        mut required_verification,
        mut last_reflection,
        mut working_mem,
        mut assumption_ledger,
        mut observation_evidence,
        session_bridge,
        mut prompt_cache,
        mut prompted_harness_overlay_ids,
        tool_root_abs,
        mut mem,
        mut recovery,
        mut step_seq,
        mut last_mutation_step,
        mut last_build_verify_ok_step,
        mut last_behavioral_verify_ok_step,
        mut last_exec_step,
        mut last_verify_ok_step,
        last_impact_step,
        mut reflection_ledger,
        progress_state,
        harness_promoted_overlay,
        mut harness_evolution_queue,
        harness_evolution_path,
        reflection_ledger_path,
        mut reflection_ledger_warned,
        mut harness_evolution_warned,
        checkpoint,
        mut cur_cwd,
        mut file_cache,
        mut read_only_diagnose_streak,
        mut read_only_diagnose_rescue_count,
        first_action_deadline,
        max_iters,
        initial_handoff_steps,
        mut observer_handoff,
        mut blocked_terminal,
        mut repair_extensions,
        repair_iterations,
    } = loop_setup::LoopSetup::init(
        start,
        &tx,
        tool_root,
        max_iters,
        test_cmd.as_deref(),
        project_context,
        agents_md,
        realize_preset,
        &autosaver,
    )
    .await;
    // Governor-blocked turns execute no tool, so each one earns back at most
    // one extra iteration (bounded) plus a transport-pool refund when a shared
    // work budget is active. The counter is seeded from message history and
    // then tracks deltas, so pruning can never shrink the earned relief.
    let mut governor_blocked_turns = count_governor_blocked_tool_turns(&messages);
    let mut prev_blocked_turns = governor_blocked_turns;
    let mut prev_executed_turns = count_executed_tool_turns(&messages);
    // Cursor of the wire request that produced the most recent model response.
    // Governor blocks are attributed to that response's receipt rather than a
    // positional guess at the end of the request log, so interleaved or
    // multi-attempt receipts cannot be misclassified into the transport pool.
    let mut last_request_cursor: Option<usize> = None;
    for iter in 0..max_iters
        .saturating_add(repair_iterations)
        .saturating_add(GOVERNOR_BLOCK_ITER_RELIEF)
    {
        let observed_blocked_turns = count_governor_blocked_tool_turns(&messages);
        let newly_blocked = observed_blocked_turns.saturating_sub(prev_blocked_turns);
        // A transport refund requires a wholly rejected response: new blocked
        // turns and zero executed turns since the previous iteration. Mixed
        // batches (executed + blocked) stay task work.
        let observed_executed_turns = count_executed_tool_turns(&messages);
        let wholly_rejected =
            newly_blocked > 0 && observed_executed_turns.saturating_sub(prev_executed_turns) == 0;
        if newly_blocked > 0 {
            governor_blocked_turns = governor_blocked_turns.saturating_add(newly_blocked);
            if !wholly_rejected {
                // Iteration relief is unchanged; only the pool refund requires
                // the whole-response proof.
            } else if let Some(context) = crate::work_budget::current() {
                for _ in 0..newly_blocked {
                    let Some(cursor) = last_request_cursor else {
                        break;
                    };
                    if context
                        .budget
                        .reject_transport(
                            "coder",
                            cursor,
                            crate::work_budget::TransportRejection::GovernorBlock,
                        )
                        .unwrap_or(false)
                    {
                        emit_telemetry_event(
                            &tx,
                            "transport_rejection",
                            json!({"role":"coder", "cursor":cursor, "reason":"governor_block"}),
                        )
                        .await;
                    } else {
                        break;
                    }
                }
            }
        }
        let iter_allowance = max_iters
            .saturating_add(repair_extensions)
            .saturating_add(governor_block_iter_relief(&messages));
        if iter >= iter_allowance
            || crate::work_budget::current().is_some_and(|c| c.budget.slice_remaining("coder") == 0)
        {
            break;
        }
        if let Some(failure) = observer_handoff::newly_executed(
            &messages,
            test_cmd.as_deref(),
            initial_handoff_steps,
            step_seq,
        ) {
            emit_telemetry_event(&tx, "observer_verification_handoff", json!(&failure)).await;
            let _ = tx.send(StreamToken::Delta("\n[agent] New verification failure; returning to Observer with the same task and remaining budget.\n".into())).await;
            observer_handoff = Some(failure);
            break;
        }
        // ── Prune old tool results before sending to save context tokens ───
        prune_old_tool_results(&mut messages);
        prune_old_assistant_messages(&mut messages);
        prune_message_window_with_context(&mut messages, &exec_verification_context);
        // Re-seed deltas on the post-prune transcript so removed history can
        // never cancel a new blocked/executed turn in the next measurement.
        prev_blocked_turns = count_governor_blocked_tool_turns(&messages);
        prev_executed_turns = count_executed_tool_turns(&messages);
        emit_telemetry_event(
            &tx,
            "agent_iter",
            json!({
                "iter": iter + 1,
                "max_iters": max_iters,
                "messages_len": messages.len(),
                "state": format!("{state:?}").to_ascii_lowercase(),
                "recovery_stage": recovery.stage_label(),
                "latent_pending": latent_plan.is_some(),
            }),
        )
        .await;

        if let Some(final_text) = done_gate::maybe_build_verified_action_closeout_text(
            &root_user_text,
            tool_root_abs.as_deref(),
            active_plan.as_ref(),
            &messages,
            &working_mem,
            &observation_evidence,
            required_verification,
            test_cmd.as_deref(),
            last_mutation_step,
            last_verify_ok_step,
        ) {
            task_outcome.finalized(&final_text);
            state = AgentState::Done;
            let final_text = enrich_text_final_handoff(
                final_text.as_str(),
                &root_user_text,
                &messages,
                test_cmd.as_deref(),
            )
            .unwrap_or(final_text);
            messages.push(json!({"role": "assistant", "content": final_text.clone()}));
            autosave_best_effort(
                &autosaver,
                &tx,
                tool_root_abs.as_deref(),
                checkpoint.as_deref(),
                cur_cwd.as_deref(),
                &messages,
            )
            .await;
            let _ = tx
                .send(StreamToken::GovernorState(build_governor_state(
                    state,
                    &recovery,
                    &mem,
                    file_tool_consec_failures,
                    last_mutation_step,
                    last_verify_ok_step,
                    last_reflection.as_ref(),
                )))
                .await;
            let _ = tx
                .send(StreamToken::Delta(format!(
                    "\n[agent] auto-finalized verified action before another model turn.\n\n{final_text}\n"
                )))
                .await;
            break;
        }

        // C — Token budget guardian: warn once when context grows large.
        let approx_tokens = approx_tokens_messages(&messages);
        if !budget_warned
            && approx_tokens >= TOKEN_BUDGET_WARN_TOKENS
            && pending_system_hint.is_none()
        {
            budget_warned = true;
            pending_system_hint = Some(format!(
                "[Token Budget] Context approx {approx_tokens} tokens ({} messages).\n\
Be concise: prefer tool calls over long explanations. Summarise intermediate results in 1-2 lines.",
                messages.len()
            ));
        }

        let shared_remaining =
            crate::work_budget::current().map(|c| c.budget.remaining_for("coder"));
        let remaining_calls = shared_remaining.unwrap_or(iter_allowance.saturating_sub(iter));
        let pending_checks = task_harness::benchmark_proof::pending_commands(
            crate::task_origin::current_task_messages(&messages),
            &root_user_text,
            test_cmd.as_deref(),
        );
        let task_requirements = requirement_proof::project(&messages, test_cmd.as_deref());
        let floor = task_requirements.delivery_floor(
            &task_harness::benchmark_proof::required_commands(&root_user_text),
            &pending_checks,
        );
        let closing = remaining_calls <= pending_checks.len().saturating_add(3)
            || floor
                .as_ref()
                .is_some_and(|f| remaining_calls <= f.minimum_coder_requests);
        let next_action =
            next_action::Contract::project(shared_remaining, &pending_checks, &task_requirements)
                .with_report_closeout(
                    crate::work_budget::current().is_some_and(|c| c.budget.is_report_closeout()),
                )
                .with_diagnostic_read(recovery.focus.required_read())
                .with_recheck(
                    (recovery.stage == Some(RecoveryStage::Verify))
                        .then(|| recovery.focus.declared_recheck(&pending_checks))
                        .flatten(),
                );
        if closing {
            let delivery = floor.as_ref().map(|f| format!("Minimum delivery cost under the one-call contract: {}. If it exceeds the available turns, make a concrete safe advance and retain the blocker and next action for the next iteration. ", serde_json::to_string(f).unwrap_or_default())).unwrap_or_default();
            let hint = format!("[Closeout budget] {delivery}{remaining_calls} model requests remain in this run (shared task budget may be tighter). Reserve one request for your authored final report and one native exec per pending exact check: {}. Avoid scope expansion and repeated passed checks. A mutation invalidates earlier proof; automatic tests do not replace required native execs. If the work cannot fit, report remaining work truthfully; do not claim completion.", serde_json::to_string(&pending_checks).unwrap_or_default());
            match pending_system_hint.as_mut() {
                Some(existing) => {
                    existing.push_str("\n\n");
                    existing.push_str(&hint);
                }
                None => pending_system_hint = Some(hint),
            }
        }

        // ── Progress checkpoint every 3 iterations ────────────────────────
        // Require a reflection so the model re-evaluates goal distance and next action.
        // Only fires when no higher-priority hint/reflection is already pending.
        if iter > 0
            && !closing
            && iter % 3 == 0
            && pending_system_hint.is_none()
            && reflection_required.is_none()
        {
            reflection_trigger_sig = None;
            reflection_required = Some(format!(
                "progress checkpoint iter {iter}/{max_iters} (summarize DONE/REMAINING briefly)"
            ));
        }

        if root_read_only && recovery.stage == Some(RecoveryStage::Diagnose) {
            read_only_diagnose_streak = read_only_diagnose_streak.saturating_add(1);
        } else {
            read_only_diagnose_streak = 0;
        }

        if root_read_only
            && read_only_diagnose_streak >= 2
            && pending_system_hint.is_none()
            && reflection_required.is_none()
        {
            let hint = build_read_only_diagnose_coercion_hint(
                &root_user_text,
                active_plan.as_ref(),
                &observation_evidence,
                &messages,
                &working_mem,
            );
            if let Some(hint) = hint {
                pending_system_hint = Some(hint);
            }
        }

        if tool_calls_this_run == 0
            && iter + 1 >= first_action_deadline.saturating_sub(1)
            && pending_system_hint.is_none()
            && reflection_required.is_none()
        {
            if let Some(hint) = build_first_action_constraint_hint(
                &root_user_text,
                root_read_only,
                goal_wants_actions,
            ) {
                pending_system_hint = Some(hint);
            }
        }

        // ── Final iteration handoff ─────────────────────────────────────────
        // If we hit the iteration cap, force a clean handoff via `done()` so
        // long sessions are resumable (session JSON keeps full history).
        if shared_remaining.map_or(iter + 1 == iter_allowance, |remaining| remaining == 1) {
            let final_hint = format!(
                "[Final Iteration — iter {}/{}]\n\
This is the LAST model call for this run.\n\
- Only ONE tool call remains. If the task is fully done AND verified, call `done` now with the requested final report. Do not rerun already successful checks just for reporting.\n\
- If the task is NOT done: call `done` with (1) verified-complete items, (2) satisfied acceptance criteria, (3) remaining acceptance criteria, (4) verification evidence for completed criteria, and (5) the exact next commands/files to continue on the next run.",
                iter + 1,
                iter_allowance
            );
            match pending_system_hint.as_mut() {
                Some(existing) => {
                    existing.push_str("\n\n");
                    existing.push_str(&final_hint);
                }
                None => pending_system_hint = Some(final_hint),
            }
        }

        let mut latent_drift_for_prompt: Option<f64> = None;
        if realize_cfg.enabled {
            let deadline_realize = if let Some(latent) = latent_plan.as_ref() {
                let age_turns = iter.saturating_sub(latent.created_iter);
                let drift = drift_distance(
                    realize_cfg.drift_metric,
                    &latent.summary,
                    &latent.anchor_baseline,
                );
                realize_metrics.total_drift += drift;
                realize_metrics.drift_samples += 1;
                if realize_cfg.within_window(age_turns) {
                    realize_metrics.within_window_turns += 1;
                }
                latent_drift_for_prompt = Some(drift);
                if age_turns >= realize_cfg.window_end {
                    latent_plan.take().map(|buf| (buf, age_turns, drift))
                } else {
                    None
                }
            } else {
                None
            };

            if let Some((latent, latency, drift)) = deadline_realize {
                realize_metrics.missing += 1;
                realize_metrics.realize_count += 1;
                realize_metrics.total_realize_latency += latency;
                adopt_valid_plan(
                    &latent.plan,
                    &mut working_mem,
                    &mut assumption_ledger,
                    &mut active_plan,
                    &mut intent_required_verification,
                    path_required_verification,
                    &mut required_verification,
                    &mut recovery,
                    &mut last_verify_ok_step,
                    last_build_verify_ok_step,
                    last_behavioral_verify_ok_step,
                );
                messages.push(json!({"role": "assistant", "content": latent.raw_text}));
                let _ = tx
                    .send(StreamToken::Delta(build_realize_banner(
                        "deadline",
                        latency,
                        drift,
                        &realize_metrics,
                    )))
                    .await;
                autosave_best_effort(
                    &autosaver,
                    &tx,
                    tool_root_abs.as_deref(),
                    checkpoint.as_deref(),
                    cur_cwd.as_deref(),
                    &messages,
                )
                .await;
            }
            emit_realize_state(
                &tx,
                &realize_cfg,
                latent_plan.as_ref(),
                iter,
                latent_drift_for_prompt,
                &realize_metrics,
            )
            .await;
        }

        // ── Stream from model ──────────────────────────────────────────────
        // Inject a one-shot governor hint if we detected a repeated failure pattern.
        let mut msgs_for_call = messages.clone();
        if let Some(hint) =
            task_obligations::prompt(&messages, test_cmd.as_deref(), tool_root_abs.as_deref())
        {
            msgs_for_call.push(json!({"role":"system", "content":hint}));
        }
        if let Some(h) = pending_system_hint.take() {
            let note = format!(
                "[Loop Governor]\nstate: {:?}\nrecovery_stage: {}\n{}\n\nYou MUST incorporate this hint in your next tool call.\nDo not repeat the same failing command.",
                state,
                recovery.stage_label(),
                h
            );
            let _ = tx
                .send(StreamToken::Delta(format!("\n[governor] {h}\n")))
                .await;
            msgs_for_call.push(json!({"role":"system","content": note}));
        }

        if let Some(hint) = task_harness::benchmark_plan_verification_hint(
            &root_user_text,
            &messages,
            test_cmd.as_deref(),
        ) {
            msgs_for_call.push(json!({"role":"system", "content":hint}));
        }

        let resolver_prompt = instruction_resolver.prompt(required_verification);
        msgs_for_call.push(json!({
            "role": "system",
            "content": render_cached_prompt(
                &mut prompt_cache.resolver_hash,
                resolver_prompt,
                build_instruction_resolver_compact_prompt(
                    &instruction_resolver,
                    required_verification,
                ),
            ),
        }));

        let meta_harness = MetaHarness::analyze(
            task_harness,
            &messages,
            recovery.stage,
            test_cmd.as_deref(),
            tool_root_abs.as_deref(),
        );
        if let Some(telemetry) = meta_harness.telemetry_payload() {
            emit_telemetry_event(&tx, "meta_harness_policy", telemetry).await;
        }
        if let Some(meta_prompt) = meta_harness.prompt() {
            let compact_prompt = meta_harness
                .compact_prompt()
                .unwrap_or_else(|| meta_prompt.clone());
            msgs_for_call.push(json!({
                "role": "system",
                "content": render_cached_prompt(
                    &mut prompt_cache.meta_harness_hash,
                    meta_prompt,
                    compact_prompt,
                ),
            }));
        }

        let evaluator_loop = EvaluatorLoop::analyze(
            task_harness,
            &meta_harness,
            &reflection_ledger,
            last_reflection.as_ref(),
            test_cmd.as_deref(),
            last_mutation_step,
            file_tool_consec_failures,
        );
        if let Some(telemetry) = evaluator_loop.telemetry_payload(task_harness) {
            emit_telemetry_event(&tx, "evaluator_loop", telemetry).await;
        }
        if let Some(evaluator_prompt) = evaluator_loop.prompt() {
            let compact_prompt = evaluator_loop
                .compact_prompt()
                .unwrap_or_else(|| evaluator_prompt.clone());
            msgs_for_call.push(json!({
                "role": "system",
                "content": render_cached_prompt(
                    &mut prompt_cache.evaluator_loop_hash,
                    evaluator_prompt,
                    compact_prompt,
                ),
            }));
        }

        if recovery.allows_progress_pressure() {
            if let Some(telemetry) = harness_promoted_overlay.telemetry_payload(task_harness) {
                emit_telemetry_event(&tx, "harness_evolution_promoted_prompted", telemetry).await;
            }
            if let Some(promoted_prompt) = harness_promoted_overlay.prompt(task_harness) {
                let compact_prompt = harness_promoted_overlay
                    .compact_prompt(task_harness)
                    .unwrap_or_else(|| promoted_prompt.clone());
                msgs_for_call.push(json!({
                    "role": "system",
                    "content": render_cached_prompt(
                        &mut prompt_cache.harness_evolution_promoted_hash,
                        promoted_prompt,
                        compact_prompt,
                    ),
                }));
            }
        }

        let current_harness_proposal =
            ContractPatchProposal::from_signals(task_harness, &meta_harness, &evaluator_loop);
        if let Some(proposal) = current_harness_proposal.as_ref() {
            let update = harness_evolution_queue.remember(proposal.clone());
            emit_telemetry_event(
                &tx,
                "harness_evolution_patch_proposed",
                update.proposal.telemetry_payload(update.was_new),
            )
            .await;
            if let Some(path) = harness_evolution_path.as_ref() {
                if let Err(e) = harness_evolution_queue.save_atomic(path) {
                    if !harness_evolution_warned {
                        harness_evolution_warned = true;
                        let _ = tx
                            .send(StreamToken::Delta(format!(
                                "\n[harness_evolution] WARN: {e:#}\n"
                            )))
                            .await;
                    }
                }
            }
        }

        let mut harness_overlay = recovery
            .allows_progress_pressure()
            .then(|| {
                harness_evolution_queue
                    .build_overlay(task_harness, current_harness_proposal.as_ref())
            })
            .flatten();
        if let Some(overlay) = harness_overlay.as_ref() {
            let newly_prompted_ids: Vec<String> = overlay
                .active_ids
                .iter()
                .filter(|id| prompted_harness_overlay_ids.insert((*id).clone()))
                .cloned()
                .collect();
            if !newly_prompted_ids.is_empty()
                && harness_evolution_queue.mark_prompted(&newly_prompted_ids)
            {
                if let Some(path) = harness_evolution_path.as_ref() {
                    if let Err(e) = harness_evolution_queue.save_atomic(path) {
                        if !harness_evolution_warned {
                            harness_evolution_warned = true;
                            let _ = tx
                                .send(StreamToken::Delta(format!(
                                    "\n[harness_evolution] WARN: {e:#}\n"
                                )))
                                .await;
                        }
                    }
                }
                harness_overlay = harness_evolution_queue
                    .build_overlay(task_harness, current_harness_proposal.as_ref());
            }
        }
        if let Some(overlay) = harness_overlay {
            emit_telemetry_event(
                &tx,
                "harness_evolution_overlay_prompted",
                overlay.telemetry_payload(),
            )
            .await;
            msgs_for_call.push(json!({
                "role": "system",
                "content": render_cached_prompt(
                    &mut prompt_cache.harness_evolution_hash,
                    overlay.full_prompt,
                    overlay.compact_prompt,
                ),
            }));
        }

        let mut harness_prompt = task_harness.prompt(test_cmd.as_deref());
        let mut harness_compact = task_harness.compact_prompt(test_cmd.as_deref());
        if task_harness.lane == task_harness::TaskLane::BenchmarkPlan {
            if let Some(example) = task_harness::benchmark_plan_example_hint(&root_user_text) {
                harness_prompt.push_str(&example);
                harness_compact.push_str(&example);
            }
        }
        msgs_for_call.push(json!({
            "role": "system",
            "content": render_cached_prompt(
                &mut prompt_cache.task_harness_hash,
                harness_prompt,
                harness_compact,
            ),
        }));

        let task_prompt =
            build_task_contract_prompt(&task_contract, active_plan.as_ref(), required_verification);
        msgs_for_call.push(json!({
            "role": "system",
            "content": render_cached_prompt(
                &mut prompt_cache.task_contract_hash,
                task_prompt,
                build_task_contract_compact_prompt(
                    &task_contract,
                    active_plan.as_ref(),
                    required_verification,
                ),
            ),
        }));

        if let Some(memory_prompt) = build_working_memory_prompt(&working_mem) {
            let compact_prompt = build_working_memory_compact_prompt(&working_mem)
                .unwrap_or_else(|| memory_prompt.clone());
            msgs_for_call.push(json!({
                "role": "system",
                "content": render_cached_prompt(
                    &mut prompt_cache.working_memory_hash,
                    memory_prompt,
                    compact_prompt,
                ),
            }));
        }

        if let Some(ledger_prompt) = build_assumption_ledger_prompt(&assumption_ledger) {
            let compact_prompt = build_assumption_ledger_compact_prompt(&assumption_ledger)
                .unwrap_or_else(|| ledger_prompt.clone());
            msgs_for_call.push(json!({
                "role": "system",
                "content": render_cached_prompt(
                    &mut prompt_cache.assumption_ledger_hash,
                    ledger_prompt,
                    compact_prompt,
                ),
            }));
        }

        if let Some(reflection_prompt) = reflection_ledger.build_prompt() {
            let compact_prompt = reflection_ledger
                .build_compact_prompt()
                .unwrap_or_else(|| reflection_prompt.clone());
            msgs_for_call.push(json!({
                "role": "system",
                "content": render_cached_prompt(
                    &mut prompt_cache.reflection_ledger_hash,
                    reflection_prompt,
                    compact_prompt,
                ),
            }));
            emit_telemetry_event(
                &tx,
                "reflection_ledger_prompted",
                json!({
                    "entries": reflection_ledger.entries.len(),
                    "iter": iter,
                }),
            )
            .await;
        }

        let progress_bridge =
            ProgressBridgeView::resolve(progress_state.as_ref(), &progress_context, &messages);
        if let Some(telemetry) = progress_bridge.telemetry_payload() {
            emit_telemetry_event(&tx, "progress_bridge_prompted", telemetry).await;
        }
        if let Some(progress_bridge_prompt) = progress_bridge.prompt() {
            let compact_prompt = progress_bridge
                .compact_prompt()
                .unwrap_or_else(|| progress_bridge_prompt.clone());
            msgs_for_call.push(json!({
                "role": "system",
                "content": render_cached_prompt(
                    &mut prompt_cache.progress_bridge_hash,
                    progress_bridge_prompt,
                    compact_prompt,
                ),
            }));
        }

        if let Some(telemetry) = session_bridge.telemetry_payload() {
            emit_telemetry_event(&tx, "session_bridge_prompted", telemetry).await;
        }
        if let Some(session_bridge_prompt) = session_bridge.prompt() {
            let compact_prompt = session_bridge
                .compact_prompt()
                .unwrap_or_else(|| session_bridge_prompt.clone());
            msgs_for_call.push(json!({
                "role": "system",
                "content": render_cached_prompt(
                    &mut prompt_cache.session_bridge_hash,
                    session_bridge_prompt,
                    compact_prompt,
                ),
            }));
        }

        if let (true, Some(latent), Some(drift)) = (
            realize_cfg.enabled,
            latent_plan.as_ref(),
            latent_drift_for_prompt,
        ) {
            let age_turns = iter.saturating_sub(latent.created_iter);
            msgs_for_call.push(json!({
                "role": "system",
                "content": build_realize_pending_hint(
                    &realize_cfg,
                    latent,
                    &realize_metrics,
                    age_turns,
                    drift,
                ),
            }));
        }

        if should_emit_verification_requirement_prompt(
            state,
            &recovery,
            last_mutation_step,
            last_verify_ok_step,
            &goal_checks,
        ) {
            let verification_prompt = verification_requirement_note(
                required_verification,
                test_cmd.as_deref(),
                active_plan.as_ref(),
            );
            msgs_for_call.push(json!({
                "role": "system",
                "content": render_cached_prompt(
                    &mut prompt_cache.verification_hash,
                    verification_prompt,
                    build_verification_requirement_compact_prompt(
                        required_verification,
                        test_cmd.as_deref(),
                        active_plan.as_ref(),
                    ),
                ),
            }));
        }

        if let Some(reason) = impact_required.as_ref() {
            msgs_for_call.push(json!({
                "role": "system",
                "content": build_impact_prompt(reason, active_plan.as_ref(), &working_mem),
            }));
        }

        if let Some(hint) = edit_failures.strategy_hint() {
            msgs_for_call.push(json!({"role": "system", "content": hint}));
        }
        if let Some(hint) = recovery.focus.hint() {
            msgs_for_call.push(json!({"role": "system", "content": hint}));
        }

        if let Some(reason) = reflection_required.as_ref() {
            msgs_for_call.push(json!({
                "role": "system",
                "content": build_reflection_prompt_with_recent(
                    reason,
                    &mem,
                    state,
                    file_tool_consec_failures,
                    &messages,
                ),
            }));
        }

        let mut tools_clone = next_action.tools(&tools);
        let check_catalog = check_catalog::Catalog::from_root(&root_user_text);
        if let Some(prompt) =
            check_catalog.bind_for(&mut tools_clone, &cfg.base_url, next_action.recheck())
        {
            msgs_for_call.push(json!({"role":"system", "content":prompt}));
        }
        acceptance_schema::bind(
            &mut tools_clone,
            active_plan.as_ref(),
            &acceptance_report::current_commands(&messages, test_cmd.as_deref()),
        );
        acceptance_schema::bind_summary(
            &mut tools_clone,
            &final_handoff::report_context(&root_user_text, &messages, test_cmd.as_deref()),
        );
        msgs_for_call.push(json!({"role":"system", "content":next_action.prompt()}));

        let turn = turn_stream::request_turn_stream(
            &client,
            cfg,
            msgs_for_call,
            tools_clone,
            &tx,
            &mut last_request_cursor,
        )
        .await;
        let request_cursor = turn.request_cursor;
        let assistant_text = turn.assistant_text;
        let mut tool_calls = turn.tool_calls;
        let stream_error = turn.error;

        if stream_error.is_some() {
            let _ = tx
                .send(StreamToken::Delta(
                    "\n[agent] aborted due to stream error; session can be resumed.\n".to_string(),
                ))
                .await;
            break;
        }

        if let Some(hint) =
            native_arguments::block_invalid_turn(&mut messages, &assistant_text, &tool_calls)
        {
            let _ = tx.send(StreamToken::Delta(format!("\n{hint}\n"))).await;
            autosave_best_effort(
                &autosaver,
                &tx,
                tool_root_abs.as_deref(),
                checkpoint.as_deref(),
                cur_cwd.as_deref(),
                &messages,
            )
            .await;
            pending_system_hint = Some(hint);
            if let (Some(context), Some(cursor)) = (crate::work_budget::current(), request_cursor) {
                if context.budget.reject_transport(
                    "coder",
                    cursor,
                    crate::work_budget::TransportRejection::NativeArguments,
                )? {
                    repair_extensions += 1;
                    emit_telemetry_event(
                        &tx,
                        "transport_rejection",
                        json!({"role":"coder", "cursor":cursor, "reason":"native_arguments"}),
                    )
                    .await;
                }
            }
            // No tool ran; keep all recovery and proof obligations unchanged.
            continue;
        }

        // Reject the whole batch before reflection/impact gates can discard its
        // native transcript. A rejection is feedback, never execution evidence.
        let turn_rejection = if tool_calls.len() > 1 {
            Some((
                "single_tool",
                governor_contract::multiple_tool_calls_message(tool_calls.len()),
            ))
        } else {
            next_action
                .rejection(&tool_calls)
                .map(|hint| (next_action.rejection_gate(), hint))
        };
        if let Some((gate, hint)) = turn_rejection {
            provider_turn::retain_rejected_calls(
                &mut messages,
                &assistant_text,
                &tool_calls,
                &hint,
            );
            let _ = tx
                .send(StreamToken::Delta(format!("\n[GOVERNOR BLOCK] {hint}\n")))
                .await;
            impact_recovery::emit_rejection(&tx, gate, &hint, &tool_calls, &assistant_text).await;
            autosave_best_effort(
                &autosaver,
                &tx,
                tool_root_abs.as_deref(),
                checkpoint.as_deref(),
                cur_cwd.as_deref(),
                &messages,
            )
            .await;
            pending_system_hint = Some(hint);
            state = AgentState::Recovery;
            continue;
        }

        if let [call] = tool_calls.as_mut_slice() {
            match check_catalog.resolve(call, tool_root_abs.as_deref()) {
                Ok(Some(receipt)) => emit_telemetry_event(&tx, "check_selected", receipt).await,
                Ok(None) => {}
                Err(error) => {
                    let hint = format!("[Check selection] {error}. No tool executed; choose a listed ID or the original full command.");
                    provider_turn::retain_rejected_calls(
                        &mut messages,
                        &assistant_text,
                        &tool_calls,
                        &hint,
                    );
                    let _ = tx.send(StreamToken::Delta(format!("\n{hint}\n"))).await;
                    pending_system_hint = Some(hint);
                    autosave_best_effort(
                        &autosaver,
                        &tx,
                        tool_root_abs.as_deref(),
                        checkpoint.as_deref(),
                        cur_cwd.as_deref(),
                        &messages,
                    )
                    .await;
                    continue;
                }
            }
        }

        // Incomplete reporting does not execute a repair or claim success. It
        // remains possible during diagnosis/reflection and at the last task slot.
        let blocked_plan = parse_plan_block(&assistant_text).filter(|plan| {
            validate_plan_for_task_contract(
                plan,
                root_read_only,
                &task_contract,
                &instruction_resolver,
                &messages,
            )
            .is_ok()
        });
        if let Some(report) = blocked_report::prepare(
            &tool_calls,
            blocked_plan.as_ref().or(active_plan.as_ref()),
            &messages,
            test_cmd.as_deref(),
            &observation_evidence,
        ) {
            let call = &tool_calls[0];
            messages.push(json!({"role":"assistant", "content":assistant_text, "tool_calls":[call.to_json()], "accepted_plan": plan_revision::snapshot(blocked_plan.as_ref().filter(|_| report.is_ok()), &messages, &task_contract)}));
            match report {
                Ok(report) => {
                    let text = report.render();
                    messages.push(json!({"role":"tool", "tool_call_id":call.id, "content":"OK: blocked", "blocked_report":report}));
                    messages.push(json!({"role":"assistant", "content":text}));
                    let _ = tx.send(StreamToken::Delta(format!("\n{text}\n"))).await;
                    blocked_terminal = true;
                    emit_telemetry_event(&tx, "blocked_report", json!(report)).await;
                }
                Err(error) => {
                    let hint = format!("[Blocked report] {error}. Preserve every remaining criterion; cite native evidence for any claimed completed item.");
                    messages.push(json!({"role":"tool", "tool_call_id":call.id, "content":format!("GOVERNOR BLOCKED\n{hint}")}));
                    pending_system_hint = Some(hint);
                }
            }
            autosave_best_effort(
                &autosaver,
                &tx,
                tool_root_abs.as_deref(),
                checkpoint.as_deref(),
                cur_cwd.as_deref(),
                &messages,
            )
            .await;
            if blocked_terminal {
                break;
            }
            continue;
        }

        if tool_calls.is_empty() && provider_turn::allows_synthetic_tools(&cfg.base_url) {
            if let Some(tc) = meta_harness.synthesize_tool_call(iter) {
                let synthesized =
                    canonicalize_tool_call_command(tc.name.as_str(), tc.arguments.as_str())
                        .unwrap_or_else(|| {
                            format!(
                                "{}({})",
                                tc.name,
                                compact_one_line(tc.arguments.as_str(), 120)
                            )
                        });
                let _ = tx
                    .send(StreamToken::Delta(format!(
                        "\n[meta_harness] synthesized next scaffold action for stalled no-tool turn: {synthesized}\n"
                    )))
                    .await;
                emit_telemetry_event(
                    &tx,
                    "meta_harness_synthesized_tool_call",
                    json!({
                        "iter": iter,
                        "tool": tc.name,
                        "synthetic": synthesized,
                    }),
                )
                .await;
                tool_calls.push(tc);
            }
        }

        if tool_calls.is_empty() && provider_turn::allows_synthetic_tools(&cfg.base_url) {
            if let Some(tc) = synthesize_fix_existing_no_tool_mutation_tool_call(
                task_harness,
                &messages,
                &root_user_text,
                recovery.stage,
            ) {
                let synthesized =
                    canonicalize_tool_call_command(tc.name.as_str(), tc.arguments.as_str())
                        .unwrap_or_else(|| {
                            format!(
                                "{}({})",
                                tc.name,
                                compact_one_line(tc.arguments.as_str(), 120)
                            )
                        });
                let _ = tx
                    .send(StreamToken::Delta(format!(
                        "\n[task_harness] synthesized fix-existing mutation for stalled no-tool turn: {synthesized}\n"
                    )))
                    .await;
                emit_telemetry_event(
                    &tx,
                    "task_harness_synthesized_tool_call",
                    json!({
                        "iter": iter,
                        "lane": task_harness.lane_label(),
                        "tool": tc.name,
                        "synthetic": synthesized,
                        "reason": "fix_existing_no_tool_mutation",
                    }),
                )
                .await;
                tool_calls.push(tc);
            }
        }

        if tool_calls.is_empty() && provider_turn::allows_synthetic_tools(&cfg.base_url) {
            if let Some(tc) = synthesize_benchmark_plan_no_tool_call(
                task_harness,
                &messages,
                &root_user_text,
                tool_root_abs.as_deref(),
                recovery.stage,
                test_cmd.as_deref(),
            ) {
                let synthesized =
                    canonicalize_tool_call_command(tc.name.as_str(), tc.arguments.as_str())
                        .unwrap_or_else(|| {
                            format!(
                                "{}({})",
                                tc.name,
                                compact_one_line(tc.arguments.as_str(), 120)
                            )
                        });
                let _ = tx
                    .send(StreamToken::Delta(format!(
                        "\n[task_harness] synthesized benchmark-plan follow-up for stalled no-tool turn: {synthesized}\n"
                    )))
                    .await;
                emit_telemetry_event(
                    &tx,
                    "task_harness_synthesized_tool_call",
                    json!({
                        "iter": iter,
                        "lane": task_harness.lane_label(),
                        "tool": tc.name,
                        "synthetic": synthesized,
                        "reason": "benchmark_plan_no_tool_followup",
                    }),
                )
                .await;
                tool_calls.push(tc);
            }
        }

        if tool_calls.len() == 1 {
            if let Some((rewritten, original, repaired)) =
                repair_truncated_patch_tool_call_from_recent_mismatch(
                    &messages,
                    &tool_calls[0],
                    root_read_only,
                    goal_wants_actions,
                    cfg.provider.clone(),
                    &observation_evidence,
                )
            {
                let _ = tx
                    .send(StreamToken::Delta(format!(
                        "\n[compat] repaired malformed patch tool call from recent mismatch: {} -> {}\n",
                        original, repaired
                    )))
                    .await;
                tool_calls[0] = rewritten;
            }
        }

        // ── Reflection enforcement (before any tool call) ─────────────────
        {
            let mut gate = reflection_gate::ReflectionGateCtx {
                tx: &tx,
                provider: cfg.provider.clone(),
                root_read_only,
                goal_wants_actions,
                mem: &mem,
                file_tool_consec_failures,
                recovery: &recovery,
                last_mutation_step,
                last_verify_ok_step,
                pending_system_hint: &mut pending_system_hint,
                state: &mut state,
                reflection_required: &mut reflection_required,
                last_reflection: &mut last_reflection,
                reflection_guard: &mut reflection_guard,
                ledger: &mut reflection_ledger,
                ledger_path: reflection_ledger_path.as_deref(),
                ledger_warned: &mut reflection_ledger_warned,
                working_mem: &mut working_mem,
                assumption_ledger: &mut assumption_ledger,
            };
            if matches!(
                reflection_gate::enforce_reflection(&mut gate, &assistant_text, &tool_calls).await,
                reflection_gate::ReflectionGate::Reject
            ) {
                continue;
            }
        }

        if let Some(reason) = impact_required.clone() {
            let pending_mutation = impact_recovery::UnreviewedMutation::from_steps(
                last_mutation_step,
                last_impact_step,
            );
            let mut restored_impact_plan = false;
            let impact_plan = parse_plan_block(&assistant_text)
                .filter(|plan| {
                    validate_plan_for_task_contract(
                        plan,
                        root_read_only,
                        &task_contract,
                        &instruction_resolver,
                        &messages,
                    )
                    .is_ok()
                })
                .or_else(|| active_plan.clone())
                .or_else(|| {
                    let [tc] = tool_calls.as_slice() else {
                        return None;
                    };
                    let plan = impact_recovery::benchmark_resume_plan(
                        pending_mutation,
                        task_harness,
                        tc,
                        &root_user_text,
                        required_verification,
                        test_cmd.as_deref(),
                    )?;
                    validate_plan_for_task_contract(
                        &plan,
                        root_read_only,
                        &task_contract,
                        &instruction_resolver,
                        &messages,
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
                        &mut working_mem,
                        &mut assumption_ledger,
                        &mut active_plan,
                        &mut intent_required_verification,
                        path_required_verification,
                        &mut required_verification,
                        &mut recovery,
                        &mut last_verify_ok_step,
                        last_build_verify_ok_step,
                        last_behavioral_verify_ok_step,
                    );
                    emit_telemetry_event(
                        &tx,
                        "impact_resume_plan_restored",
                        json!({
                            "mutation_step": pending_mutation.map(|pending| pending.step),
                            "lane": task_harness.lane_label(),
                        }),
                    )
                    .await;
                }
            }
            let impact = match parse_impact_block(&assistant_text) {
                Some(impact) => impact,
                None => {
                    if tool_calls.len() == 1 {
                        if let Some(impact) = rescue_missing_impact_for_tool_turn(
                            reason.as_str(),
                            &tool_calls[0],
                            root_read_only,
                            goal_wants_actions,
                            cfg.provider.clone(),
                            impact_plan.as_ref(),
                            pending_mutation,
                        ) {
                            let _ = tx
                                .send(StreamToken::Delta(
                                    "\n[compat] synthesized impact block for the rescued mutation turn\n"
                                        .to_string(),
                                ))
                                .await;
                            impact
                        } else {
                            let msg = provider_turn::tool_count_feedback(
                                governor_contract::impact_missing_message(reason.as_str()),
                                &tool_calls,
                            );
                            let _ = tx
                                .send(StreamToken::Delta(format!("\n[impact] {msg}\n")))
                                .await;
                            impact_recovery::emit_rejection(
                                &tx,
                                "impact",
                                &msg,
                                &tool_calls,
                                &assistant_text,
                            )
                            .await;
                            pending_system_hint = Some(msg);
                            state = AgentState::Recovery;
                            impact_required = Some(reason);
                            continue;
                        }
                    } else {
                        let msg = provider_turn::tool_count_feedback(
                            governor_contract::impact_missing_message(reason.as_str()),
                            &tool_calls,
                        );
                        let _ = tx
                            .send(StreamToken::Delta(format!("\n[impact] {msg}\n")))
                            .await;
                        impact_recovery::emit_rejection(
                            &tx,
                            "impact",
                            &msg,
                            &tool_calls,
                            &assistant_text,
                        )
                        .await;
                        pending_system_hint = Some(msg);
                        state = AgentState::Recovery;
                        impact_required = Some(reason);
                        continue;
                    }
                }
            };

            if let Err(e) = validate_impact(&impact, impact_plan.as_ref()) {
                let msg = provider_turn::tool_count_feedback(
                    governor_contract::impact_invalid_message(&e.to_string(), reason.as_str()),
                    &tool_calls,
                );
                let _ = tx
                    .send(StreamToken::Delta(format!("\n[impact] {msg}\n")))
                    .await;
                impact_recovery::emit_rejection(&tx, "impact", &msg, &tool_calls, &assistant_text)
                    .await;
                pending_system_hint = Some(msg);
                state = AgentState::Recovery;
                impact_required = Some(reason);
                continue;
            }

            if tool_calls.len() != 1 {
                let msg = provider_turn::tool_count_feedback(
                    governor_contract::impact_one_tool_message(tool_calls.len()),
                    &tool_calls,
                );
                let _ = tx
                    .send(StreamToken::Delta(format!("\n[impact] {msg}\n")))
                    .await;
                impact_recovery::emit_rejection(&tx, "impact", &msg, &tool_calls, &assistant_text)
                    .await;
                pending_system_hint = Some(msg);
                state = AgentState::Recovery;
                impact_required = Some(reason);
                continue;
            }

            let _ = tx
                .send(StreamToken::Delta(format!(
                    "\n[impact] changed={} progress={} gap={}\n",
                    impact.changed.as_str(),
                    impact.progress.as_str(),
                    impact.remaining_gap.as_str()
                )))
                .await;

            impact_required = None;
        }

        let realize_reason = parse_realize_block(&assistant_text);
        let assistant_text_clean = strip_tag_block_owned(&assistant_text, "realize")
            .trim()
            .to_string();
        let parsed_plan = parse_plan_block(&assistant_text);
        let parsed_think = parse_think_block(&assistant_text);
        let compat_last_think = if matches!(cfg.provider, ProviderKind::Mistral) {
            last_think_from_messages(&messages)
        } else {
            None
        };
        let mut validated_plan_for_turn: Option<PlanBlock> = None;
        let mut validated_think_for_turn: Option<ThinkBlock> = None;

        let mut tool_call: Option<ToolCallData> = tool_calls.pop();
        // Preserve the authored lookup through target-selection heuristics.
        // The ordinary progress and recovery gates still judge the actual call.
        let scoped_lookup = task_harness.lane == task_harness::TaskLane::FixExisting
            && recovery.stage == Some(RecoveryStage::Fix)
            && tool_call
                .as_ref()
                .is_some_and(|tc| explicit_edit_scope::symbol_lookup(&root_user_text, tc));
        let can_coerce = |tc: &&ToolCallData| !scoped_lookup && check_catalog::allows_coercion(tc);

        if let Some(tc) = tool_call.as_ref().filter(can_coerce) {
            if let Some((rewritten, original, canonical)) =
                rewrite_tool_call_with_resolution(tc, &observation_evidence)
            {
                let _ = tx
                    .send(StreamToken::Delta(format!(
                        "[resolution] {} '{}' -> '{}'\n",
                        rewritten.name, original, canonical
                    )))
                    .await;
                emit_resolution_memory_hit_telemetry(
                    &tx,
                    rewritten.name.as_str(),
                    original.as_str(),
                    canonical.as_str(),
                )
                .await;
                tool_call = Some(rewritten);
            }
        }

        if let Some(tc) = tool_call.as_ref().filter(can_coerce) {
            if let Some((rewritten, original, coerced)) = coerce_read_only_observation_tool_call(
                &messages,
                tc,
                &root_user_text,
                root_read_only,
                &observation_evidence,
            ) {
                let _ = tx
                    .send(StreamToken::Delta(format!(
                        "[compat] coerced repeated read-only observation miss: {} -> {}\n",
                        original, coerced
                    )))
                    .await;
                emit_telemetry_event(
                    &tx,
                    "read_only_tool_coercion",
                    json!({
                        "from": original,
                        "to": coerced,
                        "tool": rewritten.name,
                    }),
                )
                .await;
                tool_call = Some(rewritten);
            }
        }

        if let Some(tc) = tool_call.as_ref().filter(can_coerce) {
            if let Some((rewritten, original, coerced)) = coerce_read_only_followup_read_tool_call(
                &messages,
                tc,
                &root_user_text,
                root_read_only,
                active_plan.as_ref(),
                &observation_evidence,
            ) {
                let _ = tx
                    .send(StreamToken::Delta(format!(
                        "[compat] coerced repeated read-only follow-up read: {} -> {}\n",
                        original, coerced
                    )))
                    .await;
                emit_telemetry_event(
                    &tx,
                    "read_only_tool_coercion",
                    json!({
                        "from": original,
                        "to": coerced,
                        "tool": rewritten.name,
                        "reason": "best_followup_read",
                    }),
                )
                .await;
                tool_call = Some(rewritten);
            }
        }

        if let Some(tc) = tool_call.as_ref().filter(can_coerce) {
            if let Some((rewritten, original, coerced)) = coerce_benchmark_plan_tool_call(
                task_harness,
                &messages,
                tc,
                &root_user_text,
                tool_root_abs.as_deref(),
                recovery.stage,
                test_cmd.as_deref(),
            ) {
                let _ = tx
                    .send(StreamToken::Delta(format!(
                        "[task_harness] focused approved benchmark plan: {} -> {}\n",
                        original, coerced
                    )))
                    .await;
                emit_telemetry_event(
                    &tx,
                    "task_harness_tool_coercion",
                    json!({
                        "lane": task_harness.lane_label(),
                        "from": original,
                        "to": coerced,
                        "tool": rewritten.name,
                        "reason": "approved_benchmark_plan",
                    }),
                )
                .await;
                tool_call = Some(rewritten);
            }
        }

        if let Some(tc) = tool_call.as_ref().filter(can_coerce) {
            if let Some((rewritten, original, coerced)) =
                repair_fix_existing_mutation_tool_call(task_harness, &messages, tc, &root_user_text)
            {
                let _ = tx
                    .send(StreamToken::Delta(format!(
                        "[task_harness] repaired malformed fix-existing mutation: {} -> {}\n",
                        original, coerced
                    )))
                    .await;
                emit_telemetry_event(
                    &tx,
                    "task_harness_tool_coercion",
                    json!({
                        "lane": task_harness.lane_label(),
                        "from": original,
                        "to": coerced,
                        "tool": rewritten.name,
                        "reason": "repair_fix_mutation",
                    }),
                )
                .await;
                tool_call = Some(rewritten);
            }
        }

        if let Some(tc) = tool_call.as_ref().filter(can_coerce) {
            if let Some((rewritten, original, coerced)) =
                coerce_fix_existing_blocked_mutation_tool_call(
                    task_harness,
                    &messages,
                    tc,
                    &root_user_text,
                    recovery.stage,
                )
            {
                let _ = tx
                    .send(StreamToken::Delta(format!(
                        "[task_harness] restored blocked fix-existing mutation: {} -> {}\n",
                        original, coerced
                    )))
                    .await;
                emit_telemetry_event(
                    &tx,
                    "task_harness_tool_coercion",
                    json!({
                        "lane": task_harness.lane_label(),
                        "from": original,
                        "to": coerced,
                        "tool": rewritten.name,
                        "reason": "restore_blocked_fix_mutation",
                    }),
                )
                .await;
                tool_call = Some(rewritten);
            }
        }

        if let Some(tc) = tool_call.as_ref().filter(can_coerce) {
            if let Some((rewritten, original, coerced)) =
                coerce_existing_followup_tool_call(&messages, tc, &root_user_text, recovery.stage)
            {
                let _ = tx
                    .send(StreamToken::Delta(format!(
                        "[task_harness] focused required follow-up edit: {} -> {}\n",
                        original, coerced
                    )))
                    .await;
                emit_telemetry_event(
                    &tx,
                    "task_harness_tool_coercion",
                    json!({
                        "lane": task_harness.lane_label(),
                        "from": original,
                        "to": coerced,
                        "tool": rewritten.name,
                        "reason": "required_followup_edit",
                    }),
                )
                .await;
                tool_call = Some(rewritten);
            }
        }

        let preserves_required_followup = tool_call
            .as_ref()
            .is_some_and(|tc| matches_required_existing_followup(&messages, tc, &root_user_text));

        if !preserves_required_followup {
            if let Some(tc) = tool_call.as_ref().filter(can_coerce) {
                if let Some((rewritten, original, coerced)) =
                    coerce_fix_existing_literal_mutation_tool_call(
                        task_harness,
                        &messages,
                        tc,
                        &root_user_text,
                        recovery.stage,
                    )
                {
                    let _ = tx
                        .send(StreamToken::Delta(format!(
                            "[task_harness] synthesized fix-existing mutation from gathered evidence: {} -> {}\n",
                            original, coerced
                        )))
                        .await;
                    emit_telemetry_event(
                        &tx,
                        "task_harness_tool_coercion",
                        json!({
                            "lane": task_harness.lane_label(),
                            "from": original,
                            "to": coerced,
                            "tool": rewritten.name,
                            "reason": "synthesize_literal_fix_mutation",
                        }),
                    )
                    .await;
                    tool_call = Some(rewritten);
                }
            }
        }

        if let Some(tc) = tool_call.as_ref().filter(can_coerce) {
            if let Some((rewritten, original, coerced)) = coerce_fix_existing_tool_call(
                task_harness,
                &messages,
                tc,
                test_cmd.as_deref(),
                tool_root_abs.as_deref(),
            ) {
                let _ = tx
                    .send(StreamToken::Delta(format!(
                        "[task_harness] focused fix-existing action: {} -> {}\n",
                        original, coerced
                    )))
                    .await;
                emit_telemetry_event(
                    &tx,
                    "task_harness_tool_coercion",
                    json!({
                        "lane": task_harness.lane_label(),
                        "from": original,
                        "to": coerced,
                        "tool": rewritten.name,
                        "reason": "focus_fix_existing",
                    }),
                )
                .await;
                tool_call = Some(rewritten);
            }
        }

        if let Some(tc) = tool_call
            .as_ref()
            .filter(|tc| check_catalog::allows_coercion(tc))
        {
            if let Some((rewritten, original, coerced)) =
                coerce_artifact_creation_tool_call(task_harness, &messages, tc)
            {
                let _ = tx
                    .send(StreamToken::Delta(format!(
                        "[task_harness] restored blocked artifact action: {} -> {}\n",
                        original, coerced
                    )))
                    .await;
                emit_telemetry_event(
                    &tx,
                    "task_harness_tool_coercion",
                    json!({
                        "lane": task_harness.lane_label(),
                        "from": original,
                        "to": coerced,
                        "tool": rewritten.name,
                    }),
                )
                .await;
                tool_call = Some(rewritten);
            }
        }

        if let Some(tc) = tool_call
            .as_ref()
            .filter(|tc| check_catalog::allows_coercion(tc))
        {
            if let Some((rewritten, original, coerced)) = coerce_repo_goal_completion_tool_call(
                task_harness,
                &messages,
                tc,
                test_cmd.as_deref(),
                tool_root_abs.as_deref(),
            ) {
                let _ = tx
                    .send(StreamToken::Delta(format!(
                        "[task_harness] repaired repo goal completion: {} -> {}\n",
                        original, coerced
                    )))
                    .await;
                emit_telemetry_event(
                    &tx,
                    "task_harness_tool_coercion",
                    json!({
                        "lane": task_harness.lane_label(),
                        "from": original,
                        "to": coerced,
                        "tool": rewritten.name,
                        "reason": "goal_check_missing_repo_git",
                    }),
                )
                .await;
                tool_call = Some(rewritten);
            }
        }

        if let Some(tc) = tool_call
            .as_ref()
            .filter(|tc| check_catalog::allows_coercion(tc))
        {
            if let Some((rewritten, original, coerced)) = repair_repo_scaffold_write_tool_call(
                task_harness,
                &messages,
                tc,
                test_cmd.as_deref(),
                tool_root_abs.as_deref(),
            ) {
                let _ = tx
                    .send(StreamToken::Delta(format!(
                        "[task_harness] repaired malformed repo scaffold write: {} -> {}\n",
                        original, coerced
                    )))
                    .await;
                emit_telemetry_event(
                    &tx,
                    "task_harness_tool_coercion",
                    json!({
                        "lane": task_harness.lane_label(),
                        "from": original,
                        "to": coerced,
                        "tool": rewritten.name,
                        "reason": "repair_repo_scaffold_write",
                    }),
                )
                .await;
                tool_call = Some(rewritten);
            }
        }

        if let Some(tc) = tool_call.as_ref() {
            if let Some((entry, score, action)) =
                reflection_ledger_action_match(&reflection_ledger, tc)
            {
                emit_telemetry_event(
                    &tx,
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

        if realize_cfg.enabled {
            let materialize_reason =
                if tool_call_realizes_latent(realize_reason.as_deref(), tool_call.as_ref()) {
                    if let Some(reason) = realize_reason.as_ref() {
                        Some(format!("explicit:{reason}"))
                    } else {
                        Some("tool_call".to_string())
                    }
                } else {
                    None
                };
            if let Some(reason) = materialize_reason {
                if let Some(latent) = latent_plan.take() {
                    let latency = iter.saturating_sub(latent.created_iter);
                    let drift = drift_distance(
                        realize_cfg.drift_metric,
                        &latent.summary,
                        &latent.anchor_baseline,
                    );
                    realize_metrics.realize_count += 1;
                    realize_metrics.total_realize_latency += latency;
                    adopt_valid_plan(
                        &latent.plan,
                        &mut working_mem,
                        &mut assumption_ledger,
                        &mut active_plan,
                        &mut intent_required_verification,
                        path_required_verification,
                        &mut required_verification,
                        &mut recovery,
                        &mut last_verify_ok_step,
                        last_build_verify_ok_step,
                        last_behavioral_verify_ok_step,
                    );
                    messages.push(json!({"role": "assistant", "content": latent.raw_text}));
                    let _ = tx
                        .send(StreamToken::Delta(build_realize_banner(
                            &reason,
                            latency,
                            drift,
                            &realize_metrics,
                        )))
                        .await;
                    autosave_best_effort(
                        &autosaver,
                        &tx,
                        tool_root_abs.as_deref(),
                        checkpoint.as_deref(),
                        cur_cwd.as_deref(),
                        &messages,
                    )
                    .await;
                    emit_realize_state(&tx, &realize_cfg, None, iter, None, &realize_metrics).await;
                }
            }
        }

        if let Some(ref tc) = tool_call {
            let mut used_general_plan_rescue = false;
            let candidate_plan = match parsed_plan.as_ref() {
                Some(plan) => {
                    if let Err(e) = validate_plan_for_task_contract(
                        plan,
                        root_read_only,
                        &task_contract,
                        &instruction_resolver,
                        &messages,
                    ) {
                        if matches!(cfg.provider, ProviderKind::Mistral) {
                            if let Some(repaired) = repair_mistral_plan_for_tool_turn(
                                Some(plan),
                                active_plan.as_ref(),
                                tc,
                                &root_user_text,
                                root_read_only,
                            ) {
                                if validate_plan_for_task_contract(
                                    &repaired,
                                    root_read_only,
                                    &task_contract,
                                    &instruction_resolver,
                                    &messages,
                                )
                                .is_ok()
                                {
                                    let _ = tx
                                        .send(StreamToken::Delta(
                                            "\n[compat] repaired Mistral plan skeleton for observation tool turn\n".to_string(),
                                        ))
                                        .await;
                                    repaired
                                } else {
                                    state = AgentState::Recovery;
                                    let block = plan_revision::rejection(
                                        &mut recovery.stage,
                                        &e.to_string(),
                                        active_plan.as_ref(),
                                        &messages,
                                        &task_contract,
                                    );

                                    emit_governor_block(
                                        GovernorEmit {
                                            tx: &tx,
                                            messages: &mut messages,
                                            autosaver: &autosaver,

                                            tool_root_abs: &tool_root_abs,
                                            checkpoint: &checkpoint,
                                            cur_cwd: &cur_cwd,

                                            pending_system_hint: &mut pending_system_hint,
                                            state,
                                            recovery: &recovery,

                                            mem: &mem,
                                            file_tool_consec_failures,
                                            last_mutation_step,

                                            last_verify_ok_step,
                                            last_reflection: last_reflection.as_ref(),
                                        },
                                        block,
                                        &tc,
                                        Some(&assistant_text_clean),
                                    )
                                    .await;
                                    continue;
                                }
                            } else {
                                state = AgentState::Recovery;
                                let block = plan_revision::rejection(
                                    &mut recovery.stage,
                                    &e.to_string(),
                                    active_plan.as_ref(),
                                    &messages,
                                    &task_contract,
                                );

                                emit_governor_block(
                                    GovernorEmit {
                                        tx: &tx,
                                        messages: &mut messages,
                                        autosaver: &autosaver,

                                        tool_root_abs: &tool_root_abs,
                                        checkpoint: &checkpoint,
                                        cur_cwd: &cur_cwd,

                                        pending_system_hint: &mut pending_system_hint,
                                        state,
                                        recovery: &recovery,

                                        mem: &mem,
                                        file_tool_consec_failures,
                                        last_mutation_step,

                                        last_verify_ok_step,
                                        last_reflection: last_reflection.as_ref(),
                                    },
                                    block,
                                    &tc,
                                    Some(&assistant_text_clean),
                                )
                                .await;
                                continue;
                            }
                        } else {
                            state = AgentState::Recovery;
                            let block = plan_revision::rejection(
                                &mut recovery.stage,
                                &e.to_string(),
                                active_plan.as_ref(),
                                &messages,
                                &task_contract,
                            );

                            emit_governor_block(
                                GovernorEmit {
                                    tx: &tx,
                                    messages: &mut messages,
                                    autosaver: &autosaver,

                                    tool_root_abs: &tool_root_abs,
                                    checkpoint: &checkpoint,
                                    cur_cwd: &cur_cwd,

                                    pending_system_hint: &mut pending_system_hint,
                                    state,
                                    recovery: &recovery,

                                    mem: &mem,
                                    file_tool_consec_failures,
                                    last_mutation_step,

                                    last_verify_ok_step,
                                    last_reflection: last_reflection.as_ref(),
                                },
                                block,
                                &tc,
                                Some(&assistant_text_clean),
                            )
                            .await;
                            continue;
                        }
                    } else {
                        plan.clone()
                    }
                }
                None => match active_plan.as_ref() {
                    Some(plan) => plan.clone(),
                    None => {
                        if let Some(rescued) = rescue_read_only_missing_plan_for_tool_turn(
                            &messages,
                            tc,
                            &root_user_text,
                            root_read_only,
                            cfg.provider.clone(),
                        ) {
                            if validate_plan_for_task_contract(
                                &rescued,
                                root_read_only,
                                &task_contract,
                                &instruction_resolver,
                                &messages,
                            )
                            .is_ok()
                            {
                                let _ = tx
                                    .send(StreamToken::Delta(
                                        "\n[compat] rescued repeated read-only plan-gate miss with synthetic observation plan\n"
                                            .to_string(),
                                    ))
                                    .await;
                                rescued
                            } else {
                                state = AgentState::Recovery;
                                recovery.stage = Some(RecoveryStage::Diagnose);
                                let block = governor_contract::missing_plan_message();

                                emit_governor_block(
                                    GovernorEmit {
                                        tx: &tx,
                                        messages: &mut messages,
                                        autosaver: &autosaver,

                                        tool_root_abs: &tool_root_abs,
                                        checkpoint: &checkpoint,
                                        cur_cwd: &cur_cwd,

                                        pending_system_hint: &mut pending_system_hint,
                                        state,
                                        recovery: &recovery,

                                        mem: &mem,
                                        file_tool_consec_failures,
                                        last_mutation_step,

                                        last_verify_ok_step,
                                        last_reflection: last_reflection.as_ref(),
                                    },
                                    block,
                                    &tc,
                                    Some(&assistant_text_clean),
                                )
                                .await;
                                continue;
                            }
                        } else if let Some(rescued) = rescue_missing_plan_for_tool_turn(
                            &messages,
                            tc,
                            &root_user_text,
                            task_harness,
                            root_read_only,
                            goal_wants_actions,
                            cfg.provider.clone(),
                            required_verification,
                            test_cmd.as_deref(),
                        ) {
                            if validate_plan_for_task_contract(
                                &rescued,
                                root_read_only,
                                &task_contract,
                                &instruction_resolver,
                                &messages,
                            )
                            .is_ok()
                            {
                                used_general_plan_rescue = true;
                                let _ = tx
                                    .send(StreamToken::Delta(
                                        "\n[compat] rescued repeated plan-gate miss with synthetic action plan\n"
                                            .to_string(),
                                    ))
                                    .await;
                                rescued
                            } else {
                                state = AgentState::Recovery;
                                recovery.stage = Some(RecoveryStage::Diagnose);
                                let block = governor_contract::missing_plan_message();

                                emit_governor_block(
                                    GovernorEmit {
                                        tx: &tx,
                                        messages: &mut messages,
                                        autosaver: &autosaver,

                                        tool_root_abs: &tool_root_abs,
                                        checkpoint: &checkpoint,
                                        cur_cwd: &cur_cwd,

                                        pending_system_hint: &mut pending_system_hint,
                                        state,
                                        recovery: &recovery,

                                        mem: &mem,
                                        file_tool_consec_failures,
                                        last_mutation_step,

                                        last_verify_ok_step,
                                        last_reflection: last_reflection.as_ref(),
                                    },
                                    block,
                                    &tc,
                                    Some(&assistant_text_clean),
                                )
                                .await;
                                continue;
                            }
                        } else if matches!(cfg.provider, ProviderKind::Mistral) {
                            if let Some(repaired) = repair_mistral_plan_for_tool_turn(
                                None,
                                None,
                                tc,
                                &root_user_text,
                                root_read_only,
                            ) {
                                if validate_plan_for_task_contract(
                                    &repaired,
                                    root_read_only,
                                    &task_contract,
                                    &instruction_resolver,
                                    &messages,
                                )
                                .is_ok()
                                {
                                    let _ = tx
                                        .send(StreamToken::Delta(
                                            "\n[compat] synthesized Mistral read-only observation plan\n".to_string(),
                                        ))
                                        .await;
                                    repaired
                                } else {
                                    state = AgentState::Recovery;
                                    recovery.stage = Some(RecoveryStage::Diagnose);
                                    let block = governor_contract::missing_plan_message();

                                    emit_governor_block(
                                        GovernorEmit {
                                            tx: &tx,
                                            messages: &mut messages,
                                            autosaver: &autosaver,

                                            tool_root_abs: &tool_root_abs,
                                            checkpoint: &checkpoint,
                                            cur_cwd: &cur_cwd,

                                            pending_system_hint: &mut pending_system_hint,
                                            state,
                                            recovery: &recovery,

                                            mem: &mem,
                                            file_tool_consec_failures,
                                            last_mutation_step,

                                            last_verify_ok_step,
                                            last_reflection: last_reflection.as_ref(),
                                        },
                                        block,
                                        &tc,
                                        Some(&assistant_text_clean),
                                    )
                                    .await;
                                    continue;
                                }
                            } else {
                                state = AgentState::Recovery;
                                recovery.stage = Some(RecoveryStage::Diagnose);
                                let block = governor_contract::missing_plan_message();

                                emit_governor_block(
                                    GovernorEmit {
                                        tx: &tx,
                                        messages: &mut messages,
                                        autosaver: &autosaver,

                                        tool_root_abs: &tool_root_abs,
                                        checkpoint: &checkpoint,
                                        cur_cwd: &cur_cwd,

                                        pending_system_hint: &mut pending_system_hint,
                                        state,
                                        recovery: &recovery,

                                        mem: &mem,
                                        file_tool_consec_failures,
                                        last_mutation_step,

                                        last_verify_ok_step,
                                        last_reflection: last_reflection.as_ref(),
                                    },
                                    block,
                                    &tc,
                                    Some(&assistant_text_clean),
                                )
                                .await;
                                continue;
                            }
                        } else {
                            state = AgentState::Recovery;
                            recovery.stage = Some(RecoveryStage::Diagnose);
                            let block = governor_contract::missing_plan_message();

                            emit_governor_block(
                                GovernorEmit {
                                    tx: &tx,
                                    messages: &mut messages,
                                    autosaver: &autosaver,

                                    tool_root_abs: &tool_root_abs,
                                    checkpoint: &checkpoint,
                                    cur_cwd: &cur_cwd,

                                    pending_system_hint: &mut pending_system_hint,
                                    state,
                                    recovery: &recovery,

                                    mem: &mem,
                                    file_tool_consec_failures,
                                    last_mutation_step,

                                    last_verify_ok_step,
                                    last_reflection: last_reflection.as_ref(),
                                },
                                block,
                                &tc,
                                Some(&assistant_text_clean),
                            )
                            .await;
                            continue;
                        }
                    }
                },
            };

            if active_plan.is_none() && root_read_only && mistral_observation_tool(tc.name.as_str())
            {
                adopt_valid_plan(
                    &candidate_plan,
                    &mut working_mem,
                    &mut assumption_ledger,
                    &mut active_plan,
                    &mut intent_required_verification,
                    path_required_verification,
                    &mut required_verification,
                    &mut recovery,
                    &mut last_verify_ok_step,
                    last_build_verify_ok_step,
                    last_behavioral_verify_ok_step,
                );
            }

            if root_read_only
                && tc.name.as_str() != "done"
                && matches!(
                    tc.name.as_str(),
                    "search_files" | "read_file" | "list_dir" | "glob"
                )
                && !observation_evidence.reads.is_empty()
            {
                if let Some(final_text) = build_read_only_strong_final_answer(
                    &root_user_text,
                    &candidate_plan,
                    &observation_evidence,
                    &messages,
                    &working_mem,
                ) {
                    task_outcome.finalized(&final_text);
                    state = AgentState::Done;
                    let final_text = enrich_text_final_handoff(
                        final_text.as_str(),
                        &root_user_text,
                        &messages,
                        test_cmd.as_deref(),
                    )
                    .unwrap_or(final_text);
                    messages.push(json!({"role": "assistant", "content": final_text.clone()}));
                    autosave_best_effort(
                        &autosaver,
                        &tx,
                        tool_root_abs.as_deref(),
                        checkpoint.as_deref(),
                        cur_cwd.as_deref(),
                        &messages,
                    )
                    .await;
                    let _ = tx
                        .send(StreamToken::GovernorState(build_governor_state(
                            state,
                            &recovery,
                            &mem,
                            file_tool_consec_failures,
                            last_mutation_step,
                            last_verify_ok_step,
                            last_reflection.as_ref(),
                        )))
                        .await;
                    let _ = tx
                        .send(StreamToken::Delta(format!(
                            "\n[agent] strong read-only evidence already satisfied the task; auto-finalized instead of executing {}.\n\n{final_text}\n",
                            tc.name
                        )))
                        .await;
                    break;
                }
            }

            let known_acceptance_commands = canonicalize_known_acceptance_commands(
                &collect_known_acceptance_commands(&messages, &working_mem, test_cmd.as_deref()),
                &observation_evidence,
            );
            if benchmark_plan_pending_required_exec_command(
                &root_user_text,
                &messages,
                test_cmd.as_deref(),
            )
            .is_none()
                && !recovery.focus.is_pending()
                // An intermediate passing test cannot finish an explicitly
                // authored task or prevent its next diagnostic/repair step.
                && !requires_authored_final_answer(&root_user_text)
                && should_prefer_done_after_verified_action(
                    tc,
                    &candidate_plan,
                    &known_acceptance_commands,
                    required_verification,
                    test_cmd.as_deref(),
                    last_mutation_step,
                    last_verify_ok_step,
                )
            {
                if let Some(final_text) = maybe_build_verified_action_auto_final_answer(
                    Some(&candidate_plan),
                    &messages,
                    &known_acceptance_commands,
                    &observation_evidence,
                    required_verification,
                    test_cmd.as_deref(),
                    last_mutation_step,
                    last_verify_ok_step,
                )
                .filter(|_| {
                    !requires_authored_final_answer(&root_user_text)
                        && exact_content::validate_exact_content_task(
                            &root_user_text,
                            tool_root_abs.as_deref(),
                        )
                        .is_ok()
                }) {
                    task_outcome.finalized(&final_text);
                    state = AgentState::Done;
                    messages.push(json!({"role": "assistant", "content": final_text.clone()}));
                    autosave_best_effort(
                        &autosaver,
                        &tx,
                        tool_root_abs.as_deref(),
                        checkpoint.as_deref(),
                        cur_cwd.as_deref(),
                        &messages,
                    )
                    .await;
                    let _ = tx
                        .send(StreamToken::GovernorState(build_governor_state(
                            state,
                            &recovery,
                            &mem,
                            file_tool_consec_failures,
                            last_mutation_step,
                            last_verify_ok_step,
                            last_reflection.as_ref(),
                        )))
                        .await;
                    let _ = tx
                        .send(StreamToken::Delta(format!(
                            "\n[agent] auto-finalized verified action instead of executing redundant post-verify {}.\n\n{final_text}\n",
                            tc.name
                        )))
                        .await;
                    break;
                }

                state = AgentState::Recovery;
                let mut block = build_post_verify_done_completion_hint(
                    &candidate_plan,
                    &known_acceptance_commands,
                    tc,
                    required_verification,
                    test_cmd.as_deref(),
                );
                if let Some(hint) = authored_final_answer_hint(&root_user_text) {
                    block.push_str("\n\n");
                    block.push_str(&hint);
                }

                emit_governor_block(
                    GovernorEmit {
                        tx: &tx,
                        messages: &mut messages,
                        autosaver: &autosaver,

                        tool_root_abs: &tool_root_abs,
                        checkpoint: &checkpoint,
                        cur_cwd: &cur_cwd,

                        pending_system_hint: &mut pending_system_hint,
                        state,
                        recovery: &recovery,

                        mem: &mem,
                        file_tool_consec_failures,
                        last_mutation_step,

                        last_verify_ok_step,
                        last_reflection: last_reflection.as_ref(),
                    },
                    block,
                    &tc,
                    Some(&assistant_text_clean),
                )
                .await;
                continue;
            }

            let compat_synth_think = rescue_read_only_missing_think_for_tool_turn(
                &messages,
                tc,
                &candidate_plan,
                &root_user_text,
                root_read_only,
                cfg.provider.clone(),
                &observation_evidence,
            )
            .or_else(|| {
                rescue_missing_think_for_tool_turn(
                    &messages,
                    tc,
                    &candidate_plan,
                    root_read_only,
                    goal_wants_actions,
                    cfg.provider.clone(),
                    used_general_plan_rescue,
                )
            });

            let (think, used_compat_synth) = select_think_for_tool_turn(
                parsed_think.as_ref(),
                compat_last_think.as_ref(),
                compat_synth_think.as_ref(),
                &candidate_plan,
                tc,
                cfg.provider.clone(),
            );

            let think = match think {
                Some(think) => think,
                None => {
                    state = AgentState::Recovery;
                    recovery.stage = Some(RecoveryStage::Diagnose);
                    let block = governor_contract::missing_think_message();

                    emit_governor_block(
                        GovernorEmit {
                            tx: &tx,
                            messages: &mut messages,
                            autosaver: &autosaver,

                            tool_root_abs: &tool_root_abs,
                            checkpoint: &checkpoint,
                            cur_cwd: &cur_cwd,

                            pending_system_hint: &mut pending_system_hint,
                            state,
                            recovery: &recovery,

                            mem: &mem,
                            file_tool_consec_failures,
                            last_mutation_step,

                            last_verify_ok_step,
                            last_reflection: last_reflection.as_ref(),
                        },
                        block,
                        &tc,
                        Some(&assistant_text_clean),
                    )
                    .await;
                    continue;
                }
            };

            if used_compat_synth {
                let _ = tx
                    .send(StreamToken::Delta(
                        "\n[compat] synthesized think block for the rescued tool turn\n"
                            .to_string(),
                    ))
                    .await;
            }

            if let Err(e) = validate_think(think, &candidate_plan, tc) {
                state = AgentState::Recovery;
                recovery.stage = Some(RecoveryStage::Diagnose);
                let block = governor_contract::invalid_think_message(&e.to_string());

                emit_governor_block(
                    GovernorEmit {
                        tx: &tx,
                        messages: &mut messages,
                        autosaver: &autosaver,

                        tool_root_abs: &tool_root_abs,
                        checkpoint: &checkpoint,
                        cur_cwd: &cur_cwd,

                        pending_system_hint: &mut pending_system_hint,
                        state,
                        recovery: &recovery,

                        mem: &mem,
                        file_tool_consec_failures,
                        last_mutation_step,

                        last_verify_ok_step,
                        last_reflection: last_reflection.as_ref(),
                    },
                    block,
                    &tc,
                    Some(&assistant_text_clean),
                )
                .await;
                continue;
            }

            if let Some(conflict) = refuted_assumption_conflict(
                &assumption_ledger,
                think,
                tc,
                &exec_verification_context,
            ) {
                state = AgentState::Recovery;
                recovery.stage = Some(RecoveryStage::Diagnose);
                let block = format!(
                    "[Assumption Ledger] {conflict}\nGather new evidence or choose a different next action before retrying."
                );

                emit_governor_block(
                    GovernorEmit {
                        tx: &tx,
                        messages: &mut messages,
                        autosaver: &autosaver,

                        tool_root_abs: &tool_root_abs,
                        checkpoint: &checkpoint,
                        cur_cwd: &cur_cwd,

                        pending_system_hint: &mut pending_system_hint,
                        state,
                        recovery: &recovery,

                        mem: &mem,
                        file_tool_consec_failures,
                        last_mutation_step,

                        last_verify_ok_step,
                        last_reflection: last_reflection.as_ref(),
                    },
                    block,
                    &tc,
                    Some(&assistant_text_clean),
                )
                .await;
                continue;
            }

            if let Some(conflict) = instruction_resolver.tool_conflict(tc, test_cmd.as_deref()) {
                state = AgentState::Recovery;
                recovery.stage = Some(RecoveryStage::Diagnose);
                let block = conflict.render();

                emit_governor_block(
                    GovernorEmit {
                        tx: &tx,
                        messages: &mut messages,
                        autosaver: &autosaver,

                        tool_root_abs: &tool_root_abs,
                        checkpoint: &checkpoint,
                        cur_cwd: &cur_cwd,

                        pending_system_hint: &mut pending_system_hint,
                        state,
                        recovery: &recovery,

                        mem: &mem,
                        file_tool_consec_failures,
                        last_mutation_step,

                        last_verify_ok_step,
                        last_reflection: last_reflection.as_ref(),
                    },
                    block,
                    &tc,
                    Some(&assistant_text_clean),
                )
                .await;
                continue;
            }

            if let Some(block) = build_progress_gate_block(
                task_harness,
                tc,
                &messages,
                recovery.stage,
                test_cmd.as_deref(),
                last_mutation_step,
                file_tool_consec_failures,
            ) {
                state = AgentState::Recovery;
                emit_governor_block(
                    GovernorEmit {
                        tx: &tx,
                        messages: &mut messages,
                        autosaver: &autosaver,
                        tool_root_abs: &tool_root_abs,
                        checkpoint: &checkpoint,
                        cur_cwd: &cur_cwd,
                        pending_system_hint: &mut pending_system_hint,
                        state,
                        recovery: &recovery,
                        mem: &mem,
                        file_tool_consec_failures,
                        last_mutation_step,
                        last_verify_ok_step,
                        last_reflection: last_reflection.as_ref(),
                    },
                    block,
                    &tc,
                    Some(&assistant_text_clean),
                )
                .await;
                continue;
            }

            if let Some(block) = evaluator_loop.build_violation_block(tc).filter(|_| {
                !requirement_proof::is_pending_baseline(tc, &messages, test_cmd.as_deref())
            }) {
                state = AgentState::Recovery;
                emit_governor_block(
                    GovernorEmit {
                        tx: &tx,
                        messages: &mut messages,
                        autosaver: &autosaver,
                        tool_root_abs: &tool_root_abs,
                        checkpoint: &checkpoint,
                        cur_cwd: &cur_cwd,
                        pending_system_hint: &mut pending_system_hint,
                        state,
                        recovery: &recovery,
                        mem: &mem,
                        file_tool_consec_failures,
                        last_mutation_step,
                        last_verify_ok_step,
                        last_reflection: last_reflection.as_ref(),
                    },
                    block,
                    &tc,
                    Some(&assistant_text_clean),
                )
                .await;
                continue;
            }

            if mutation_tool_requires_evidence(tc) {
                let observations = &observation_evidence;
                let evidence_block = match parse_evidence_block(&assistant_text) {
                    Some(block) => block,
                    None => {
                        if let Some(block) = rescue_missing_evidence_for_tool_turn(
                            tc,
                            root_read_only,
                            goal_wants_actions,
                            cfg.provider.clone(),
                            observations,
                        ) {
                            let _ = tx
                                .send(StreamToken::Delta(
                                    "\n[compat] synthesized evidence block for the rescued mutation turn\n"
                                        .to_string(),
                                ))
                                .await;
                            block
                        } else {
                            state = AgentState::Recovery;
                            recovery.stage = Some(RecoveryStage::Diagnose);
                            let block =
                                build_evidence_gate_prompt(tc, &observations, &assumption_ledger);
                            emit_governor_block(
                                GovernorEmit {
                                    tx: &tx,
                                    messages: &mut messages,
                                    autosaver: &autosaver,
                                    tool_root_abs: &tool_root_abs,
                                    checkpoint: &checkpoint,
                                    cur_cwd: &cur_cwd,
                                    pending_system_hint: &mut pending_system_hint,
                                    state,
                                    recovery: &recovery,
                                    mem: &mem,
                                    file_tool_consec_failures,
                                    last_mutation_step,
                                    last_verify_ok_step,
                                    last_reflection: last_reflection.as_ref(),
                                },
                                block,
                                &tc,
                                Some(&assistant_text_clean),
                            )
                            .await;
                            continue;
                        }
                    }
                };
                if let Err(e) = validate_evidence_block(&evidence_block, tc, &observations) {
                    state = AgentState::Recovery;
                    recovery.stage = Some(RecoveryStage::Diagnose);
                    let mut block = format!(
                        "{}\n\n{}",
                        governor_contract::evidence_invalid_message(&e.to_string()),
                        build_evidence_gate_prompt(tc, &observations, &assumption_ledger)
                    );
                    if !evidence_block.target_symbols.is_empty() {
                        block.push_str("\n\nCurrent target_symbols:\n");
                        for symbol in &evidence_block.target_symbols {
                            block.push_str("- ");
                            block.push_str(symbol);
                            block.push('\n');
                        }
                    }
                    emit_governor_block(
                        GovernorEmit {
                            tx: &tx,
                            messages: &mut messages,
                            autosaver: &autosaver,
                            tool_root_abs: &tool_root_abs,
                            checkpoint: &checkpoint,
                            cur_cwd: &cur_cwd,
                            pending_system_hint: &mut pending_system_hint,
                            state,
                            recovery: &recovery,
                            mem: &mem,
                            file_tool_consec_failures,
                            last_mutation_step,
                            last_verify_ok_step,
                            last_reflection: last_reflection.as_ref(),
                        },
                        block,
                        &tc,
                        Some(&assistant_text_clean),
                    )
                    .await;
                    continue;
                }
            }

            validated_plan_for_turn = Some(candidate_plan.clone());
            validated_think_for_turn = Some(think.clone());
        }

        // ── Append assistant turn ──────────────────────────────────────────
        if let Some(ref tc) = tool_call {
            state = AgentState::Executing;
            if let Some(plan) = validated_plan_for_turn.as_ref() {
                adopt_valid_plan(
                    plan,
                    &mut working_mem,
                    &mut assumption_ledger,
                    &mut active_plan,
                    &mut intent_required_verification,
                    path_required_verification,
                    &mut required_verification,
                    &mut recovery,
                    &mut last_verify_ok_step,
                    last_build_verify_ok_step,
                    last_behavioral_verify_ok_step,
                );
            }
            messages.push(json!({
                "role": "assistant",
                "content": assistant_text_clean,
                "tool_calls": [tc.to_json()],
                "accepted_plan": plan_revision::snapshot(validated_plan_for_turn.as_ref(), &messages, &task_contract)
            }));
        } else {
            if let Some(hint) = recovery.focus.hint() {
                messages.push(json!({"role":"assistant", "content":assistant_text_clean}));
                messages.push(crate::task_origin::user_message(
                    &hint,
                    crate::task_origin::MessageOrigin::Runtime,
                ));
                pending_system_hint = Some(hint);
                state = AgentState::Recovery;
                autosave_best_effort(
                    &autosaver,
                    &tx,
                    tool_root_abs.as_deref(),
                    checkpoint.as_deref(),
                    cur_cwd.as_deref(),
                    &messages,
                )
                .await;
                continue;
            }
            if realize_cfg.enabled {
                if let Some(plan) = parsed_plan.as_ref().filter(|plan| {
                    validate_plan_for_task_contract(
                        plan,
                        root_read_only,
                        &task_contract,
                        &instruction_resolver,
                        &messages,
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
                        let summary = latent_plan_summary(
                            plan,
                            &assistant_text_clean,
                            latest_intent.as_deref(),
                        );
                        let anchor = build_anchor_baseline(
                            &messages,
                            &root_user_text,
                            active_plan.as_ref(),
                            &working_mem,
                        );
                        let drift = drift_distance(realize_cfg.drift_metric, &summary, &anchor);
                        realize_metrics.total_drift += drift;
                        realize_metrics.drift_samples += 1;
                        latent_plan = Some(LatentPlanBuffer {
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
                            &realize_metrics,
                        )
                        .await;
                        continue;
                    }
                } else if latent_plan.is_none() {
                    if let (Some(active), Some(think)) =
                        (active_plan.as_ref(), parsed_think.as_ref())
                    {
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
                                &messages,
                                &root_user_text,
                                active_plan.as_ref(),
                                &working_mem,
                            );
                            let drift = drift_distance(realize_cfg.drift_metric, &summary, &anchor);
                            realize_metrics.total_drift += drift;
                            realize_metrics.drift_samples += 1;
                            latent_plan = Some(LatentPlanBuffer {
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
                                &realize_metrics,
                            )
                            .await;
                            continue;
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
                            &realize_metrics,
                        )
                        .await;
                        continue;
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
                    &messages,
                )
                .await;
                continue;
            }

            if let Some(ref plan) = parsed_plan {
                match validate_plan_for_task_contract(
                    &plan,
                    root_read_only,
                    &task_contract,
                    &instruction_resolver,
                    &messages,
                ) {
                    Ok(()) => {
                        adopt_valid_plan(
                            &plan,
                            &mut working_mem,
                            &mut assumption_ledger,
                            &mut active_plan,
                            &mut intent_required_verification,
                            path_required_verification,
                            &mut required_verification,
                            &mut recovery,
                            &mut last_verify_ok_step,
                            last_build_verify_ok_step,
                            last_behavioral_verify_ok_step,
                        );
                    }
                    Err(e) => {
                        state = AgentState::Recovery;
                        let mut block = plan_revision::rejection(
                            &mut recovery.stage,
                            &e.to_string(),
                            active_plan.as_ref(),
                            &messages,
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
                        pending_system_hint = Some(block);
                        autosave_best_effort(
                            &autosaver,
                            &tx,
                            tool_root_abs.as_deref(),
                            checkpoint.as_deref(),
                            cur_cwd.as_deref(),
                            &messages,
                        )
                        .await;
                        let _ = tx
                            .send(StreamToken::GovernorState(build_governor_state(
                                state,
                                &recovery,
                                &mem,
                                file_tool_consec_failures,
                                last_mutation_step,
                                last_verify_ok_step,
                                last_reflection.as_ref(),
                            )))
                            .await;
                        continue;
                    }
                }
            }
            if goal_wants_actions && iter + 1 < max_iters {
                if let Some(hint) =
                    required_existing_followup_no_tool_hint(&messages, &root_user_text)
                {
                    state = AgentState::Recovery;
                    recovery.stage = Some(RecoveryStage::Fix);
                    pending_system_hint = Some(hint.clone());
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
                        &messages,
                    )
                    .await;
                    let _ = tx
                        .send(StreamToken::GovernorState(build_governor_state(
                            state,
                            &recovery,
                            &mem,
                            file_tool_consec_failures,
                            last_mutation_step,
                            last_verify_ok_step,
                            last_reflection.as_ref(),
                        )))
                        .await;
                    continue;
                }
            }
            if !assistant_text_clean.is_empty() {
                let assistant_text_for_history = enrich_text_final_handoff(
                    assistant_text_clean.as_str(),
                    &root_user_text,
                    &messages,
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
                &messages,
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
                            &messages,
                        )
                        .is_ok()
                    })
                    .is_some()
                    || parsed_think.is_some())
                && iter + 1 < max_iters
            {
                state = AgentState::Recovery;
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
                pending_system_hint = Some(note);
                let _ = tx
                    .send(StreamToken::Delta(
                        "\n[compat] recorded structured block; continuing to real tool call\n"
                            .to_string(),
                    ))
                    .await;
                let _ = tx
                    .send(StreamToken::GovernorState(build_governor_state(
                        state,
                        &recovery,
                        &mem,
                        file_tool_consec_failures,
                        last_mutation_step,
                        last_verify_ok_step,
                        last_reflection.as_ref(),
                    )))
                    .await;
                continue;
            }

            // Model didn't call tools. If the user asked for local actions, try implied scripts
            // (PowerShell/bash code fences) as a fallback so non-tool-calling models can still act.
            if goal_wants_actions && iter + 1 < max_iters {
                let (implied_scripts, implied_files) =
                    implied_actions::extract(&assistant_text_clean);
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
                    state = AgentState::Recovery;
                    let _ = tx
                        .send(StreamToken::Delta(format!(
                            "[RESULT][Recovery] {feedback}\n"
                        )))
                        .await;
                    pending_system_hint = Some(block);
                    autosave_best_effort(
                        &autosaver,
                        &tx,
                        tool_root_abs.as_deref(),
                        checkpoint.as_deref(),
                        cur_cwd.as_deref(),
                        &messages,
                    )
                    .await;
                    continue;
                }
                if !implied_scripts.is_empty() || !implied_files.is_empty() {
                    let _ = tx
                        .send(StreamToken::Delta(
                            "\n[governor] tool_call missing; executing implied actions\n"
                                .to_string(),
                        ))
                        .await;

                    let mut implied_had_error = false;

                    for im in implied_scripts {
                        let command = im.script.trim().to_string();
                        if command.is_empty() {
                            continue;
                        }

                        state = AgentState::Executing;
                        let cwd_used: Option<String> =
                            cur_cwd.clone().or_else(|| tool_root_abs.clone());
                        let cwd_used_label = cwd_used
                            .as_deref()
                            .unwrap_or("(workspace root)")
                            .to_string();

                        let _ = tx
                            .send(StreamToken::Delta(format!(
                                "\n\n[IMPLIED_TOOL][{:?}] lang={}\n{command}\n[cwd] {cwd_used_label}\n",
                                state, im.lang_hint
                            )))
                            .await;

                        if let Some(block) =
                            should_block_git_landmines(&command, tool_root_abs.as_deref())
                        {
                            state = AgentState::Recovery;
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
                                &messages,
                            )
                            .await;

                            // Update memory so repeating the same blocked command triggers stronger hints.
                            let _ = mem.on_tool_result(&command, "", &block, 1);
                            pending_system_hint = Some(block);
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
                            state = AgentState::Recovery;
                            let _ = tx
                                .send(StreamToken::Delta(
                                    "[RESULT][Recovery] REJECTED by user\n".to_string(),
                                ))
                                .await;

                            let cwd_line = format!("cwd: {cwd_used_label}");
                            let tool_output =
                                format!("REJECTED BY USER\n{cwd_line}\ncommand:\n{command}");
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
                                &messages,
                            )
                            .await;
                            pending_system_hint = Some(
                                "The user rejected the command. Choose a safer alternative or explain why it is necessary before retrying."
                                    .to_string(),
                            );
                            implied_had_error = true;
                            break;
                        }

                        let cargo_input_probe = cwd_used
                            .as_deref()
                            .zip(tool_root_abs.as_deref())
                            .and_then(|(cwd, root)| {
                                CargoInputProbe::start(&messages, &command, cwd, root)
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
                                    cur_cwd = Some(p);
                                } else {
                                    cwd_after_note = Some(format!(
                                        "NOTE: cwd_after was outside tool_root; ignored: {p}"
                                    ));
                                }
                            } else {
                                cur_cwd = Some(p);
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

                        state = AgentState::Verifying;
                        let suspicious_reason = if exit_code == 0 {
                            suspicious_success_reason(&stdout, &stderr)
                        } else {
                            None
                        };
                        let effective_exit_code = if exit_code == 0
                            && (suspicious_reason.is_some() || escaped_tool_root)
                        {
                            1
                        } else {
                            exit_code
                        };

                        let note = cwd_after_note.as_deref();
                        let tool_output = if effective_exit_code == 0 {
                            let base = build_ok_tool_output(&stdout);
                            inject_cwd(&base, &cwd_line, note)
                        } else {
                            let mut out =
                                build_failed_tool_output(&stdout, &stderr, effective_exit_code);
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
                            format!("[RESULT][{:?}] exit=0\n", state)
                        } else {
                            format!("[RESULT][{:?}] exit={effective_exit_code} !\n", state)
                        };
                        let _ = tx.send(StreamToken::Delta(result_label)).await;

                        // NOTE: do not fabricate tool_call_id. Feed the result back as user text.
                        let cargo_inputs = cargo_input_probe
                            .map(|p| p.finish(effective_exit_code == 0, &stdout, &stderr));
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
                            &messages,
                        )
                        .await;

                        if effective_exit_code != 0 {
                            state = AgentState::Recovery;
                        } else {
                            state = AgentState::Planning;
                        }

                        pending_system_hint = if escaped_tool_root {
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
                                    state = AgentState::Recovery;
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
                                        &messages,
                                    )
                                    .await;
                                    pending_system_hint = Some(format!(
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
                                    &messages,
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
                                state = AgentState::Recovery;
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
                                    &messages,
                                )
                                .await;
                                pending_system_hint = Some(
                                    "The user rejected the edit. Choose a safer alternative or ask again with a smaller change."
                                        .to_string(),
                                );
                                break;
                            }

                            state = AgentState::Executing;
                            let mut cargo_inputs = None;
                            let (mut r_text, r_err) = crate::cargo_inputs::write_file(
                                &path,
                                &imf.content,
                                base,
                                &mut messages,
                            );
                            if !r_err
                                && crate::execution_evidence::file_edit_succeeded(
                                    "write_file",
                                    &r_text,
                                )
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
                                                &messages,
                                                &mut cargo_inputs,
                                            )
                                            .await,
                                        );
                                    }
                                }
                            }

                            let first_line = r_text.lines().next().unwrap_or("").to_string();
                            if r_err {
                                state = AgentState::Recovery;
                                let _ = tx
                                    .send(StreamToken::Delta(format!(
                                        "[RESULT_FILE_ERR] {first_line}\n"
                                    )))
                                    .await;
                                pending_system_hint = Some(format!(
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
                                    &messages,
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
                                &messages,
                            )
                            .await;
                        }
                    }

                    continue;
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
                pending_system_hint = Some(hint);
                state = AgentState::Planning;
                let _ = tx.send(StreamToken::Delta(
                    "\n[agent] Required final report is still pending; requesting native closeout.\n".into(),
                )).await;
                autosave_best_effort(
                    &autosaver,
                    &tx,
                    tool_root_abs.as_deref(),
                    checkpoint.as_deref(),
                    cur_cwd.as_deref(),
                    &messages,
                )
                .await;
                continue;
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
                        state = AgentState::Verifying;

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
                                        &messages,
                                    )
                                    .await;
                                    state = AgentState::Planning;
                                    let _ = tx
                                        .send(StreamToken::GovernorState(build_governor_state(
                                            state,
                                            &recovery,
                                            &mem,
                                            file_tool_consec_failures,
                                            last_mutation_step,
                                            last_verify_ok_step,
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
                                    let summary =
                                        goal_check_runner_summary(VerificationLevel::Behavioral);
                                    let support_line = goal_check_support_line(
                                        summary.as_str(),
                                        governor_contract::goal_check_tests_runner_fallback_message(
                                        )
                                        .as_str(),
                                    );
                                    let block =
                                        governor_contract::goal_check_tests_no_runner_message(
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
                                        &messages,
                                    )
                                    .await;
                                    state = AgentState::Planning;
                                    let _ = tx
                                        .send(StreamToken::GovernorState(build_governor_state(
                                            state,
                                            &recovery,
                                            &mem,
                                            file_tool_consec_failures,
                                            last_mutation_step,
                                            last_verify_ok_step,
                                            last_reflection.as_ref(),
                                        )))
                                        .await;
                                    goal_check_blocked = true;
                                    break;
                                };
                                let result = run_goal_check_command(
                                    "tests",
                                    command.as_str(),
                                    goal_root,
                                    &tx,
                                )
                                .await;
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
                                        &messages,
                                    )
                                    .await;
                                    state = AgentState::Recovery;
                                    recovery.stage = Some(RecoveryStage::Fix);
                                    let _ = tx
                                        .send(StreamToken::GovernorState(build_governor_state(
                                            state,
                                            &recovery,
                                            &mem,
                                            file_tool_consec_failures,
                                            last_mutation_step,
                                            last_verify_ok_step,
                                            last_reflection.as_ref(),
                                        )))
                                        .await;
                                    goal_check_blocked = true;
                                    break;
                                }
                                status.ok = true;
                                working_mem.remember_successful_verification(&result.command);
                                last_behavioral_verify_ok_step = Some(step_seq);
                                last_verify_ok_step = effective_verify_ok_step(
                                    required_verification,
                                    last_build_verify_ok_step,
                                    last_behavioral_verify_ok_step,
                                );
                                recovery.on_exec_result(
                                    ExecKind::Verify,
                                    Some(VerificationLevel::Behavioral),
                                    true,
                                );
                            }
                            "build" => {
                                let Some(command) = goal_check_runner_command(
                                    VerificationLevel::Build,
                                    goal_root,
                                    None,
                                ) else {
                                    let summary =
                                        goal_check_runner_summary(VerificationLevel::Build);
                                    let support_line = goal_check_support_line(
                                        summary.as_str(),
                                        governor_contract::goal_check_build_runner_fallback_message(
                                        )
                                        .as_str(),
                                    );
                                    let block =
                                        governor_contract::goal_check_build_no_runner_message(
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
                                        &messages,
                                    )
                                    .await;
                                    state = AgentState::Planning;
                                    let _ = tx
                                        .send(StreamToken::GovernorState(build_governor_state(
                                            state,
                                            &recovery,
                                            &mem,
                                            file_tool_consec_failures,
                                            last_mutation_step,
                                            last_verify_ok_step,
                                            last_reflection.as_ref(),
                                        )))
                                        .await;
                                    goal_check_blocked = true;
                                    break;
                                };
                                let result = run_goal_check_command(
                                    "build",
                                    command.as_str(),
                                    goal_root,
                                    &tx,
                                )
                                .await;
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
                                        &messages,
                                    )
                                    .await;
                                    state = AgentState::Recovery;
                                    recovery.stage = Some(RecoveryStage::Fix);
                                    let _ = tx
                                        .send(StreamToken::GovernorState(build_governor_state(
                                            state,
                                            &recovery,
                                            &mem,
                                            file_tool_consec_failures,
                                            last_mutation_step,
                                            last_verify_ok_step,
                                            last_reflection.as_ref(),
                                        )))
                                        .await;
                                    goal_check_blocked = true;
                                    break;
                                }
                                status.ok = true;
                                working_mem.remember_successful_verification(&result.command);
                                last_build_verify_ok_step = Some(step_seq);
                                last_verify_ok_step = effective_verify_ok_step(
                                    required_verification,
                                    last_build_verify_ok_step,
                                    last_behavioral_verify_ok_step,
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
                        continue;
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
                        state = AgentState::Done;
                        let _ = tx
                            .send(StreamToken::Delta(format!(
                                "\n{}\n",
                                governor_contract::goal_check_all_passed_message()
                            )))
                            .await;
                        let _ = tx
                            .send(StreamToken::GovernorState(build_governor_state(
                                state,
                                &recovery,
                                &mem,
                                file_tool_consec_failures,
                                last_mutation_step,
                                last_verify_ok_step,
                                last_reflection.as_ref(),
                            )))
                            .await;
                        break;
                    }
                }
            }

            if let Some(final_text) = maybe_build_read_only_auto_final_answer(
                root_read_only,
                &root_user_text,
                active_plan.as_ref(),
                &observation_evidence,
                &messages,
                &working_mem,
            ) {
                task_outcome.finalized(&final_text);
                state = AgentState::Done;
                messages.push(json!({"role": "assistant", "content": final_text.clone()}));
                autosave_best_effort(
                    &autosaver,
                    &tx,
                    tool_root_abs.as_deref(),
                    checkpoint.as_deref(),
                    cur_cwd.as_deref(),
                    &messages,
                )
                .await;
                let _ = tx
                    .send(StreamToken::GovernorState(build_governor_state(
                        state,
                        &recovery,
                        &mem,
                        file_tool_consec_failures,
                        last_mutation_step,
                        last_verify_ok_step,
                        last_reflection.as_ref(),
                    )))
                    .await;
                let _ = tx
                    .send(StreamToken::Delta(format!(
                        "\n[agent] auto-finalized read-only inspection after no-tool turn.\n\n{final_text}\n"
                    )))
                    .await;
                break;
            }

            if root_read_only
                && provider_turn::allows_synthetic_tools(&cfg.base_url)
                && (read_only_diagnose_streak >= 2
                    || (tool_calls_this_run == 0 && iter + 1 >= first_action_deadline))
                && read_only_diagnose_rescue_count < 3
                && iter + 1 < max_iters
            {
                if active_plan.is_none() {
                    let synthetic_plan = synthetic_read_only_observation_plan(&root_user_text);
                    adopt_valid_plan(
                        &synthetic_plan,
                        &mut working_mem,
                        &mut assumption_ledger,
                        &mut active_plan,
                        &mut intent_required_verification,
                        path_required_verification,
                        &mut required_verification,
                        &mut recovery,
                        &mut last_verify_ok_step,
                        last_build_verify_ok_step,
                        last_behavioral_verify_ok_step,
                    );
                }

                if let Some(action) = choose_read_only_diagnose_rescue_action(
                    &root_user_text,
                    active_plan.as_ref(),
                    &observation_evidence,
                ) {
                    read_only_diagnose_rescue_count =
                        read_only_diagnose_rescue_count.saturating_add(1);
                    match action {
                        ReadOnlyDiagnoseRescueAction::Search { pattern, dir } => {
                            tool_calls_this_run = tool_calls_this_run.saturating_add(1);
                            emit_telemetry_event(
                                &tx,
                                "read_only_autorescue",
                                json!({
                                    "action": "search_files",
                                    "pattern": pattern,
                                    "dir": dir,
                                    "diagnose_streak": read_only_diagnose_streak,
                                    "count": read_only_diagnose_rescue_count,
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
                                    iter, read_only_diagnose_rescue_count
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
                            step_seq = step_seq.saturating_add(1);
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
                                pending_system_hint = Some(format!(
                                    "Read-only diagnose rescue search failed: {first_line}"
                                ));
                                state = AgentState::Recovery;
                            } else {
                                let _ = tx
                                    .send(StreamToken::Delta(format!(
                                        "[RESULT_SEARCH] {first_line}\n"
                                    )))
                                    .await;
                                state = if recovery.in_recovery() {
                                    AgentState::Recovery
                                } else {
                                    AgentState::Planning
                                };
                                update_working_memory_after_non_exec(
                                    &mut working_mem,
                                    "search_files",
                                    false,
                                    None,
                                    None,
                                    test_cmd.as_deref(),
                                );
                                let command = canonicalize_tool_call_command(
                                    "search_files",
                                    arguments.as_str(),
                                )
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
                                sync_observation_cache_autosave(&autosaver, &observation_evidence);
                                assumption_ledger.refresh_confirmations(&working_mem);
                                pending_system_hint = active_plan.as_ref().and_then(|plan| {
                                    build_read_only_search_to_read_hint(
                                        &root_user_text,
                                        plan,
                                        parsed_search_paths.as_slice(),
                                        &observation_evidence,
                                    )
                                    .or_else(|| {
                                        build_read_only_completion_hint(
                                            &root_user_text,
                                            plan,
                                            &observation_evidence,
                                            &messages,
                                            &working_mem,
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
                                &messages,
                            )
                            .await;
                            let _ = tx
                                .send(StreamToken::GovernorState(build_governor_state(
                                    state,
                                    &recovery,
                                    &mem,
                                    file_tool_consec_failures,
                                    last_mutation_step,
                                    last_verify_ok_step,
                                    last_reflection.as_ref(),
                                )))
                                .await;
                            continue;
                        }
                        ReadOnlyDiagnoseRescueAction::Read { path } => {
                            tool_calls_this_run = tool_calls_this_run.saturating_add(1);
                            emit_telemetry_event(
                                &tx,
                                "read_only_autorescue",
                                json!({
                                    "action": "read_file",
                                    "path": path,
                                    "diagnose_streak": read_only_diagnose_streak,
                                    "count": read_only_diagnose_rescue_count,
                                }),
                            )
                            .await;
                            let arguments = json!({ "path": path }).to_string();
                            let tc = ToolCallData {
                                thought_signature: None,
                                id: format!(
                                    "auto_ro_diag_read_{}_{}",
                                    iter, read_only_diagnose_rescue_count
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
                            step_seq = step_seq.saturating_add(1);
                            let cache_key = crate::file_tools::resolve_safe_path(
                                path.as_str(),
                                tool_root_abs.as_deref(),
                            )
                            .ok()
                            .map(|p| p.to_string_lossy().into_owned())
                            .unwrap_or_else(|| {
                                format!("{}|{}", path, tool_root_abs.as_deref().unwrap_or(""))
                            });
                            if assumption_gate::needs_fresh_file_read(&assumption_ledger, &path) {
                                file_cache.remove(&cache_key);
                            }
                            let (result, is_error) =
                                if let Some(cached) = file_cache.get(&cache_key) {
                                    let header =
                                        cached.lines().next().unwrap_or(path.as_str()).to_string();
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
                                pending_system_hint = Some(format!(
                                    "Read-only diagnose rescue read failed: {first_line}"
                                ));
                                state = AgentState::Recovery;
                            } else {
                                let _ = tx
                                    .send(StreamToken::Delta(format!(
                                        "[RESULT_FILE] {first_line}\n"
                                    )))
                                    .await;
                                state = if recovery.in_recovery() {
                                    AgentState::Recovery
                                } else {
                                    AgentState::Planning
                                };
                                update_working_memory_after_non_exec(
                                    &mut working_mem,
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
                                sync_observation_cache_autosave(&autosaver, &observation_evidence);
                                assumption_gate::confirm_file_existence_after_result(
                                    &mut assumption_ledger,
                                    "read_file",
                                    &path,
                                    &result,
                                );
                                assumption_ledger.refresh_confirmations(&working_mem);
                                pending_system_hint = active_plan.as_ref().and_then(|plan| {
                                    build_read_only_completion_hint(
                                        &root_user_text,
                                        plan,
                                        &observation_evidence,
                                        &messages,
                                        &working_mem,
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
                                &messages,
                            )
                            .await;
                            let _ = tx
                                .send(StreamToken::GovernorState(build_governor_state(
                                    state,
                                    &recovery,
                                    &mem,
                                    file_tool_consec_failures,
                                    last_mutation_step,
                                    last_verify_ok_step,
                                    last_reflection.as_ref(),
                                )))
                                .await;
                            continue;
                        }
                    }
                }
            }

            // Verified text/read-only completion above remains available. Only an
            // unfinished no-tool turn needs another native Google function call.
            if let Some(hint) =
                provider_turn::retry_unfinished_text_turn(&cfg.base_url, &mut messages)
            {
                pending_system_hint = Some(hint.to_string());
                let _ = tx
                    .send(StreamToken::Delta(
                        "\n[provider] No tool executed; requesting a native function call.\n"
                            .to_string(),
                    ))
                    .await;
                autosave_best_effort(
                    &autosaver,
                    &tx,
                    tool_root_abs.as_deref(),
                    checkpoint.as_deref(),
                    cur_cwd.as_deref(),
                    &messages,
                )
                .await;
                continue;
            }

            // Common failure mode: model "explains what to do" but never calls tools.
            // Try once to force a tool call so long sessions keep moving.
            if !forced_tool_once && iter + 1 < max_iters {
                forced_tool_once = true;
                state = AgentState::Recovery;
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
                    &messages,
                )
                .await;
                let _ = tx
                    .send(StreamToken::GovernorState(build_governor_state(
                        state,
                        &recovery,
                        &mem,
                        file_tool_consec_failures,
                        last_mutation_step,
                        last_verify_ok_step,
                        last_reflection.as_ref(),
                    )))
                    .await;
                continue;
            }

            state = AgentState::Done;
            let _ = tx
                .send(StreamToken::Delta(format!(
                    "\n[agent] state: {:?}\n",
                    state
                )))
                .await;
            break; // Model finished without tool call
        }

        // ── Execute the tool ───────────────────────────────────────────────
        let tc = tool_call.unwrap();
        step_seq = step_seq.saturating_add(1);
        tool_calls_this_run = tool_calls_this_run.saturating_add(1);
        let this_step = step_seq;
        let step_label_for_turn = resolve_step_label(
            validated_plan_for_turn.as_ref(),
            validated_think_for_turn.as_ref(),
        );
        let next_action_for_turn = validated_think_for_turn
            .as_ref()
            .map(|think| think.next.as_str());

        // Reflection/action alignment: if we just required a reflection due to a failure/stall,
        // do not allow immediately repeating the same action when the reflection says
        // `adjust`/`abandon`.
        if let Some(ref r) = reflection_guard {
            let needs_change = matches!(
                r.strategy_change,
                StrategyChange::Adjust | StrategyChange::Abandon
            );
            let next_sig = tool_call_action_sig(&tc);
            if needs_change {
                if let (Some(ref trigger), Some(ref next)) =
                    (reflection_trigger_sig.as_ref(), next_sig.as_ref())
                {
                    if trigger == next {
                        state = AgentState::Recovery;
                        recovery.stage = Some(RecoveryStage::Diagnose);
                        let block = format!(
                            "Reflection/action mismatch blocked.\n\
strategy_change: {}\n\
trigger_action: {}\n\
attempted_action: {}\n\
\n\
Required: choose a materially different next action consistent with your reflection.\n\
Execute this minimal action instead: {}",
                            r.strategy_change.as_str(),
                            trigger,
                            next,
                            r.next_minimal_action.as_str()
                        );

                        let _ = tx
                            .send(StreamToken::Delta(format!(
                                "[RESULT][Recovery] GOVERNOR BLOCK\n{block}\n"
                            )))
                            .await;

                        messages.push(json!({
                            "role": "tool",
                            "tool_call_id": tc.id,
                            "content": format!(
                                "GOVERNOR BLOCKED\n\n{block}\n\ntool:\n{}\narguments:\n{}",
                                tc.name, tc.arguments
                            ),
                        }));
                        autosave_best_effort(
                            &autosaver,
                            &tx,
                            tool_root_abs.as_deref(),
                            checkpoint.as_deref(),
                            cur_cwd.as_deref(),
                            &messages,
                        )
                        .await;

                        pending_system_hint = Some(block.clone());
                        reflection_required = Some(
                            "Tool call contradicted the reflection. Emit <reflect> again and change action."
                                .to_string(),
                        );
                        let _ = tx
                            .send(StreamToken::GovernorState(build_governor_state(
                                state,
                                &recovery,
                                &mem,
                                file_tool_consec_failures,
                                last_mutation_step,
                                last_verify_ok_step,
                                last_reflection.as_ref(),
                            )))
                            .await;
                        continue;
                    }
                }
            }

            // Consume the guard once the next tool call is not an immediate retry of the trigger action.
            if !needs_change
                || reflection_trigger_sig.is_none()
                || next_sig
                    .as_ref()
                    .map(|s| reflection_trigger_sig.as_ref() != Some(s))
                    .unwrap_or(true)
            {
                reflection_guard = None;
                reflection_trigger_sig = None;
            }
        }

        // ── done tool ──────────────────────────────────────────────────────
        if tc.name.as_str() == "done" {
            let mut done_ctx = tool_dispatch::ToolDispatchCtx {
                tx: &tx,
                autosaver: &autosaver,
                approver,
                tool_root_abs: &tool_root_abs,
                checkpoint: &checkpoint,
                cur_cwd: &mut cur_cwd,
                messages: &mut messages,
                observation_evidence: &mut observation_evidence,
                recovery: &mut recovery,
                pending_system_hint: &mut pending_system_hint,
                state: &mut state,
                working_mem: &mut working_mem,
                assumption_ledger: &mut assumption_ledger,
                task_outcome: &mut task_outcome,
                mem: &mut mem,
                file_cache: &mut file_cache,
                edit_failures: &mut edit_failures,
                file_tool_consec_failures: &mut file_tool_consec_failures,
                last_mutation_step: &mut last_mutation_step,
                last_build_verify_ok_step: &mut last_build_verify_ok_step,
                last_behavioral_verify_ok_step: &mut last_behavioral_verify_ok_step,
                last_verify_ok_step: &mut last_verify_ok_step,
                last_exec_step: &mut last_exec_step,
                last_reflection: &last_reflection,
                reflection_guard: &reflection_guard,
                reflection_required: &mut reflection_required,
                reflection_trigger_sig: &mut reflection_trigger_sig,
                impact_required: &mut impact_required,
                intent_required_verification: &mut intent_required_verification,
                path_required_verification: &mut path_required_verification,
                required_verification: &mut required_verification,
                exec_verification_context: &exec_verification_context,
                cfg,
                realize_cfg: &realize_cfg,
                latent_plan: &latent_plan,
                validated_plan_for_turn: &validated_plan_for_turn,
                root_read_only,
                task_harness,
                active_plan: &active_plan,
                iter,
                max_iters: iter_allowance,
                this_step,
                root_user_text: &root_user_text,
                test_cmd: test_cmd.as_deref(),
                step_label_for_turn: step_label_for_turn.as_deref(),
                next_action_for_turn,
            };
            match done_dispatch::run_done(&mut done_ctx, &tc).await? {
                tool_dispatch::ToolFlow::Break => break,
                tool_dispatch::ToolFlow::Continue => continue,
            }
        }

        // ── apply_diff tool ───────────────────────────────────────────────
        // Recovery gate: while recovering from failures, enforce a strict
        // Diagnose -> Fix -> Verify workflow to prevent phase drift.
        let allow_existing_followup_verify =
            matches_required_existing_followup(&messages, &tc, &root_user_text);
        let allow_benchmark_plan_followup_verify = allows_benchmark_plan_followup_during_verify(
            task_harness,
            &messages,
            &tc,
            &root_user_text,
            tool_root_abs.as_deref(),
            test_cmd.as_deref(),
        );
        if let Some(block) = recovery
            .maybe_block_tool(
                &tc,
                test_cmd.as_deref(),
                task_harness,
                allow_existing_followup_verify || allow_benchmark_plan_followup_verify,
            )
            .filter(|_| {
                recovery.focus.is_pending()
                    || !requirement_proof::is_unattempted_baseline(
                        &tc,
                        &messages,
                        test_cmd.as_deref(),
                    )
            })
            .filter(|block| {
                !block.starts_with("[Recovery Gate] stage=diagnose\n")
                    || !deferred_edit::diagnosis::matches_retry(
                        &messages,
                        &tc,
                        tool_root_abs.as_deref(),
                        &recovery.focus,
                        &edit_failures,
                    )
            })
            .or_else(|| {
                explicit_edit_scope::native_issue(&root_user_text, &tc, tool_root_abs.as_deref())
            })
        {
            state = AgentState::Recovery;
            let _ = tx
                .send(StreamToken::Delta(format!(
                    "[RESULT][Recovery] GOVERNOR BLOCK\n{block}\n"
                )))
                .await;

            let cwd_label = cur_cwd
                .as_deref()
                .or(tool_root_abs.as_deref())
                .unwrap_or("(workspace root)")
                .to_string();
            let tool_output = inject_cwd(
                &format!(
                    "GOVERNOR BLOCKED\n\n{block}\n\ntool:\n{}\narguments:\n{}",
                    tc.name, tc.arguments
                ),
                &format!("cwd: {cwd_label}"),
                None,
            );

            messages.push(json!({
                "role": "tool",
                "tool_call_id": tc.id,
                "content": tool_output,
            }));
            autosave_best_effort(
                &autosaver,
                &tx,
                tool_root_abs.as_deref(),
                checkpoint.as_deref(),
                cur_cwd.as_deref(),
                &messages,
            )
            .await;

            pending_system_hint = Some(block);
            let _ = tx
                .send(StreamToken::GovernorState(build_governor_state(
                    state,
                    &recovery,
                    &mem,
                    file_tool_consec_failures,
                    last_mutation_step,
                    last_verify_ok_step,
                    last_reflection.as_ref(),
                )))
                .await;
            continue;
        }

        let mut tool_ctx = tool_dispatch::ToolDispatchCtx {
            tx: &tx,
            autosaver: &autosaver,
            approver,
            tool_root_abs: &tool_root_abs,
            checkpoint: &checkpoint,
            cur_cwd: &mut cur_cwd,
            messages: &mut messages,
            observation_evidence: &mut observation_evidence,
            recovery: &mut recovery,
            pending_system_hint: &mut pending_system_hint,
            state: &mut state,
            working_mem: &mut working_mem,
            assumption_ledger: &mut assumption_ledger,
            task_outcome: &mut task_outcome,
            mem: &mut mem,
            file_cache: &mut file_cache,
            edit_failures: &mut edit_failures,
            file_tool_consec_failures: &mut file_tool_consec_failures,
            last_mutation_step: &mut last_mutation_step,
            last_build_verify_ok_step: &mut last_build_verify_ok_step,
            last_behavioral_verify_ok_step: &mut last_behavioral_verify_ok_step,
            last_verify_ok_step: &mut last_verify_ok_step,
            last_exec_step: &mut last_exec_step,
            last_reflection: &last_reflection,
            reflection_guard: &reflection_guard,
            reflection_required: &mut reflection_required,
            reflection_trigger_sig: &mut reflection_trigger_sig,
            impact_required: &mut impact_required,
            intent_required_verification: &mut intent_required_verification,
            path_required_verification: &mut path_required_verification,
            required_verification: &mut required_verification,
            exec_verification_context: &exec_verification_context,
            cfg,
            realize_cfg: &realize_cfg,
            latent_plan: &latent_plan,
            validated_plan_for_turn: &validated_plan_for_turn,
            root_read_only,
            task_harness,
            active_plan: &active_plan,
            iter,
            max_iters: iter_allowance,
            this_step,
            root_user_text: &root_user_text,
            test_cmd: test_cmd.as_deref(),
            step_label_for_turn: step_label_for_turn.as_deref(),
            next_action_for_turn,
        };
        if tc.name.as_str() == "apply_diff" {
            match tool_dispatch::run_apply_diff(&mut tool_ctx, &tc).await? {
                tool_dispatch::ToolFlow::Break => break,
                tool_dispatch::ToolFlow::Continue => continue,
            }
        }

        // ── glob tool ─────────────────────────────────────────────────────
        // ── list_dir tool ────────────────────────────────────────────────────
        if tc.name.as_str() == "list_dir" {
            match tool_dispatch::run_list_dir(&mut tool_ctx, &tc).await? {
                tool_dispatch::ToolFlow::Break => break,
                tool_dispatch::ToolFlow::Continue => continue,
            }
        }
        if tc.name.as_str() == "glob" {
            match tool_dispatch::run_glob(&mut tool_ctx, &tc).await? {
                tool_dispatch::ToolFlow::Break => break,
                tool_dispatch::ToolFlow::Continue => continue,
            }
        }
        if tc.name.as_str() == "search_files" {
            match tool_dispatch::run_search_files(&mut tool_ctx, &tc).await? {
                tool_dispatch::ToolFlow::Break => break,
                tool_dispatch::ToolFlow::Continue => continue,
            }
        }

        // ── File tools: read_file / write_file / patch_file ────────────────
        if matches!(tc.name.as_str(), "read_file" | "write_file" | "patch_file") {
            match file_tool_dispatch::run_file_tools(&mut tool_ctx, &tc).await? {
                tool_dispatch::ToolFlow::Break => break,
                tool_dispatch::ToolFlow::Continue => continue,
            }
        }

        if tc.name != "exec" {
            return Err(anyhow!("unknown tool: {}", tc.name));
        }

        match exec_dispatch::run_exec(&mut tool_ctx, &tc).await? {
            tool_dispatch::ToolFlow::Break => break,
            tool_dispatch::ToolFlow::Continue => {}
        }
    }

    Ok(run_closeout(Closeout {
        tx: &tx,
        messages: &mut messages,
        task_outcome: &mut task_outcome,
        observer_handoff: &observer_handoff,
        blocked_terminal,
        state,
        root_user_text: &root_user_text,
        tool_root_abs: &tool_root_abs,
        checkpoint: &checkpoint,
        cur_cwd: &cur_cwd,
        autosaver: &autosaver,
        test_cmd: test_cmd.as_deref(),
        active_plan: &mut active_plan,
        working_mem: &mut working_mem,
        assumption_ledger: &mut assumption_ledger,
        observation_evidence: &observation_evidence,
        intent_required_verification: &mut intent_required_verification,
        path_required_verification,
        required_verification: &mut required_verification,
        recovery: &mut recovery,
        last_mutation_step,
        last_verify_ok_step: &mut last_verify_ok_step,
        last_build_verify_ok_step,
        last_behavioral_verify_ok_step,
        realize_cfg: &realize_cfg,
        latent_plan: &mut latent_plan,
        realize_metrics: &mut realize_metrics,
        max_iters,
    })
    .await)
}
