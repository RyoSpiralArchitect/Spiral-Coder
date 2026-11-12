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
    let mut first_observer_observation = observer_continuation::current();
    for iter in 0..max_iters
        .saturating_add(repair_iterations)
        .saturating_add(GOVERNOR_BLOCK_ITER_RELIEF)
    {
        let observer_observation = first_observer_observation.take();
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
        if let Some(failure) = observer_handoff::newly_executed(
            &messages,
            test_cmd.as_deref(),
            initial_handoff_steps,
            step_seq,
        ) {
            emit_telemetry_event(&tx, "observer_verification_handoff", json!(&failure)).await;
            let _ = tx.send(StreamToken::Delta("\n[agent] New verification failure; returning to Observer with the same task and remaining budget.\n".into())).await;
            observer_handoff = Some(crate::tui::coder_yield::CoderYieldReason::VerificationFailure);
            break;
        }
        if let Some(followup) = observer_handoff::after_repeated_diagnosis(
            &recovery.focus,
            &messages,
            tool_root_abs.as_deref(),
            initial_handoff_steps,
            step_seq,
        ) {
            emit_telemetry_event(&tx, "observer_diagnostic_followup", json!(&followup)).await;
            let _ = tx.send(StreamToken::Delta("\n[agent] The same failure recurred and its location has been read. Returning to Observer for a different next step.\n".into())).await;
            observer_handoff = Some(crate::tui::coder_yield::CoderYieldReason::DiagnosticFollowup);
            break;
        }
        if iter >= iter_allowance
            || crate::work_budget::current().is_some_and(|c| c.budget.slice_remaining("coder") == 0)
        {
            break;
        }
        // Keep native exchanges intact across iterations. Only the provider
        // projection is compacted; recovery and proof never consume a summary.
        strategy_memory::retain_for_resume(&mut messages);
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
        let prompt_assembly::AssemblyOut {
            messages: msgs_for_call,
            tools: tools_clone,
            check_catalog,
            meta_harness,
            evaluator_loop,
        } = prompt_assembly::assemble_turn(&mut prompt_assembly::AssemblyCtx {
            tx: &tx,
            cfg,
            messages: &messages,
            tools: &tools,
            next_action: &next_action,
            task_harness,
            recovery: &recovery,
            session_bridge: &session_bridge,
            root_user_text: &root_user_text,
            test_cmd: &test_cmd,
            tool_root_abs: &tool_root_abs,
            task_contract: &task_contract,
            instruction_resolver: &instruction_resolver,
            realize_cfg: &realize_cfg,
            latent_plan: &latent_plan,
            latent_drift_for_prompt,
            realize_metrics: &realize_metrics,
            reflection_ledger: &reflection_ledger,
            harness_promoted_overlay: &harness_promoted_overlay,
            harness_evolution_path: &harness_evolution_path,
            progress_state: &progress_state,
            progress_context: &progress_context,
            active_plan: &active_plan,
            edit_failures: &edit_failures,
            mem: &mem,
            assumption_ledger: &assumption_ledger,
            working_mem: &working_mem,
            goal_checks: &goal_checks,
            reflection_required: &reflection_required,
            impact_required: &impact_required,
            last_reflection: &last_reflection,
            state,
            iter,
            file_tool_consec_failures,
            last_mutation_step,
            last_verify_ok_step,
            required_verification,
            prompt_cache: &mut prompt_cache,
            pending_system_hint: &mut pending_system_hint,
            prompted_harness_overlay_ids: &mut prompted_harness_overlay_ids,
            harness_evolution_queue: &mut harness_evolution_queue,
            harness_evolution_warned: &mut harness_evolution_warned,
        })
        .await;

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
            tool_root_abs.as_deref(),
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

        {
            let mut gate = impact_gate::ImpactGateCtx {
                tx: &tx,
                provider: cfg.provider.clone(),
                root_read_only,
                goal_wants_actions,
                task_harness,
                task_contract: &task_contract,
                instruction_resolver: &instruction_resolver,
                root_user_text: &root_user_text,
                test_cmd: test_cmd.as_deref(),
                messages: &messages,
                last_mutation_step,
                last_impact_step,
                path_required_verification,
                last_build_verify_ok_step,
                last_behavioral_verify_ok_step,
                pending_system_hint: &mut pending_system_hint,
                state: &mut state,
                impact_required: &mut impact_required,
                working_mem: &mut working_mem,
                assumption_ledger: &mut assumption_ledger,
                active_plan: &mut active_plan,
                intent_required_verification: &mut intent_required_verification,
                required_verification: &mut required_verification,
                recovery: &mut recovery,
                last_verify_ok_step: &mut last_verify_ok_step,
            };
            if matches!(
                impact_gate::enforce_impact(&mut gate, &assistant_text, &tool_calls).await,
                impact_gate::ImpactGate::Reject
            ) {
                continue;
            }
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
        let observer_observation = observer_observation.as_ref().filter(|observation| {
            tool_call
                .as_ref()
                .is_some_and(|call| observation.matches_accepted_call(call, &assistant_text))
        });
        tool_coercion::coerce_authored_tool_call(
            &tool_coercion::CoercionCtx {
                tx: &tx,
                messages: &messages,
                task_harness,
                root_user_text: &root_user_text,
                root_read_only,
                observation_evidence: &observation_evidence,
                active_plan: active_plan.as_ref(),
                tool_root_abs: tool_root_abs.as_deref(),
                recovery_stage: recovery.stage,
                test_cmd: test_cmd.as_deref(),
                reflection_ledger: &reflection_ledger,
            },
            &mut tool_call,
        )
        .await;

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
            let out = plan_think_gate::validate_plan_think(&mut plan_think_gate::PlanThinkCtx {
                tx: &tx,
                autosaver: &autosaver,
                cfg,
                task_harness,
                evaluator_loop: &evaluator_loop,
                tc,
                assistant_text: &assistant_text,
                assistant_text_clean: &assistant_text_clean,
                parsed_plan: &parsed_plan,
                parsed_think: &parsed_think,
                observer_observation,
                compat_last_think: &compat_last_think,
                root_user_text: &root_user_text,
                test_cmd: &test_cmd,
                tool_root_abs: &tool_root_abs,
                checkpoint: &checkpoint,
                cur_cwd: &cur_cwd,
                task_contract: &task_contract,
                instruction_resolver: &instruction_resolver,
                exec_verification_context: &exec_verification_context,
                observation_evidence: &observation_evidence,
                mem: &mem,
                last_reflection: &last_reflection,
                file_tool_consec_failures,
                last_mutation_step,
                last_build_verify_ok_step,
                last_behavioral_verify_ok_step,
                path_required_verification,
                root_read_only,
                goal_wants_actions,
                messages: &mut messages,
                state: &mut state,
                recovery: &mut recovery,
                pending_system_hint: &mut pending_system_hint,
                working_mem: &mut working_mem,
                assumption_ledger: &mut assumption_ledger,
                active_plan: &mut active_plan,
                intent_required_verification: &mut intent_required_verification,
                required_verification: &mut required_verification,
                last_verify_ok_step: &mut last_verify_ok_step,
                task_outcome: &mut task_outcome,
            })
            .await?;
            match out.flow {
                plan_think_gate::PlanThinkFlow::Continue => continue,
                plan_think_gate::PlanThinkFlow::Break => break,
                plan_think_gate::PlanThinkFlow::Validated => {
                    validated_plan_for_turn = out.plan;
                    validated_think_for_turn = out.think;
                }
            }
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
            let mut no_tool_ctx = no_tool_call::NoToolCtx {
                tx: &tx,
                autosaver: &autosaver,
                approver,
                cfg,
                tool_root_abs: &tool_root_abs,
                checkpoint: &checkpoint,
                root_user_text: &root_user_text,
                assistant_text_clean: &assistant_text_clean,
                test_cmd: &test_cmd,
                task_contract: &task_contract,
                instruction_resolver: &instruction_resolver,
                realize_cfg: &realize_cfg,
                realize_reason: &realize_reason,
                parsed_plan: &parsed_plan,
                parsed_think: &parsed_think,
                last_reflection: &last_reflection,
                root_read_only,
                goal_wants_actions,
                command_approval_required,
                wants_repo_goal,
                wants_test_goal,
                wants_build_goal,
                iter,
                max_iters,
                first_action_deadline,
                read_only_diagnose_streak,
                file_tool_consec_failures,
                last_mutation_step,
                path_required_verification,
                messages: &mut messages,
                state: &mut state,
                recovery: &mut recovery,
                pending_system_hint: &mut pending_system_hint,
                working_mem: &mut working_mem,
                assumption_ledger: &mut assumption_ledger,
                active_plan: &mut active_plan,
                latent_plan: &mut latent_plan,
                realize_metrics: &mut realize_metrics,
                task_outcome: &mut task_outcome,
                goal_checks: &mut goal_checks,
                cur_cwd: &mut cur_cwd,
                mem: &mut mem,
                file_cache: &mut file_cache,
                observation_evidence: &mut observation_evidence,
                step_seq: &mut step_seq,
                tool_calls_this_run: &mut tool_calls_this_run,
                read_only_diagnose_rescue_count: &mut read_only_diagnose_rescue_count,
                forced_tool_once: &mut forced_tool_once,
                last_verify_ok_step: &mut last_verify_ok_step,
                last_behavioral_verify_ok_step: &mut last_behavioral_verify_ok_step,
                last_build_verify_ok_step: &mut last_build_verify_ok_step,
                intent_required_verification: &mut intent_required_verification,
                required_verification: &mut required_verification,
            };
            match no_tool_call::handle_no_tool_call(&mut no_tool_ctx).await? {
                no_tool_call::NoToolFlow::Continue => continue,
                no_tool_call::NoToolFlow::Break => break,
            }
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
        observer_handoff,
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
