//! Per-iteration call assembly: clones the transcript, appends every
//! system prompt/hint the current state warrants, binds the tool schema,
//! and returns the request payloads plus the check catalog for later
//! resolution. Extracted verbatim from the agent loop.
use super::*;
use serde_json::json;

pub(super) struct AssemblyOut {
    pub messages: Vec<serde_json::Value>,
    pub tools: serde_json::Value,
    pub check_catalog: check_catalog::Catalog,
    pub meta_harness: MetaHarness,
    pub evaluator_loop: EvaluatorLoop,
}

pub(super) struct AssemblyCtx<'a> {
    pub tx: &'a mpsc::Sender<StreamToken>,
    pub cfg: &'a RunConfig,
    pub messages: &'a Vec<serde_json::Value>,
    pub tools: &'a serde_json::Value,
    pub next_action: &'a next_action::Contract,
    pub task_harness: task_harness::TaskHarness,
    pub recovery: &'a RecoveryGovernor,
    pub session_bridge: &'a SessionBridgeView,
    pub root_user_text: &'a str,
    pub test_cmd: &'a Option<String>,
    pub tool_root_abs: &'a Option<String>,
    pub task_contract: &'a TaskContract,
    pub instruction_resolver: &'a InstructionResolver,
    pub realize_cfg: &'a RealizeOnDemandConfig,
    pub latent_plan: &'a Option<LatentPlanBuffer>,
    pub latent_drift_for_prompt: Option<f64>,
    pub realize_metrics: &'a RealizeMetrics,
    pub reflection_ledger: &'a crate::reflection_ledger::ReflectionLedger,
    pub harness_promoted_overlay: &'a harness_evolution::GovernorContractOverlay,
    pub harness_evolution_path: &'a Option<std::path::PathBuf>,
    pub progress_state: &'a Option<crate::progress_state::RepoProgressState>,
    pub progress_context: &'a crate::progress_state::ProgressSaveContext,
    pub active_plan: &'a Option<PlanBlock>,
    pub edit_failures: &'a EditFailureMemory,
    pub mem: &'a FailureMemory,
    pub assumption_ledger: &'a AssumptionLedger,
    pub working_mem: &'a WorkingMemory,
    pub goal_checks: &'a GoalCheckTracker,
    pub reflection_required: &'a Option<String>,
    pub impact_required: &'a Option<String>,
    pub last_reflection: &'a Option<ReflectionBlock>,
    pub state: AgentState,
    pub iter: usize,
    pub file_tool_consec_failures: usize,
    pub last_mutation_step: Option<usize>,
    pub last_verify_ok_step: Option<usize>,
    pub required_verification: VerificationLevel,
    pub prompt_cache: &'a mut StablePromptCache,
    pub pending_system_hint: &'a mut Option<String>,
    pub prompted_harness_overlay_ids: &'a mut std::collections::BTreeSet<String>,
    pub harness_evolution_queue: &'a mut harness_evolution::HarnessEvolutionQueue,
    pub harness_evolution_warned: &'a mut bool,
}

pub(super) async fn assemble_turn(ctx: &mut AssemblyCtx<'_>) -> AssemblyOut {
    let tx = ctx.tx;
    let cfg = ctx.cfg;
    let messages = ctx.messages;
    let tools = ctx.tools;
    let next_action = ctx.next_action;
    let task_harness = ctx.task_harness;
    let recovery = ctx.recovery;
    let session_bridge = ctx.session_bridge;
    let root_user_text = ctx.root_user_text;
    let test_cmd = ctx.test_cmd;
    let tool_root_abs = ctx.tool_root_abs;
    let task_contract = ctx.task_contract;
    let instruction_resolver = ctx.instruction_resolver;
    let realize_cfg = ctx.realize_cfg;
    let latent_plan = ctx.latent_plan;
    let latent_drift_for_prompt = ctx.latent_drift_for_prompt;
    let realize_metrics = ctx.realize_metrics;
    let reflection_ledger = ctx.reflection_ledger;
    let harness_promoted_overlay = ctx.harness_promoted_overlay;
    let harness_evolution_path = ctx.harness_evolution_path;
    let progress_state = ctx.progress_state;
    let progress_context = ctx.progress_context;
    let active_plan = ctx.active_plan;
    let edit_failures = ctx.edit_failures;
    let mem = ctx.mem;
    let assumption_ledger = ctx.assumption_ledger;
    let working_mem = ctx.working_mem;
    let goal_checks = ctx.goal_checks;
    let reflection_required = ctx.reflection_required;
    let impact_required = ctx.impact_required;
    let last_reflection = ctx.last_reflection;
    let state = ctx.state;
    let iter = ctx.iter;
    let file_tool_consec_failures = ctx.file_tool_consec_failures;
    let last_mutation_step = ctx.last_mutation_step;
    let last_verify_ok_step = ctx.last_verify_ok_step;
    let required_verification = ctx.required_verification;
    let prompt_cache = &mut *ctx.prompt_cache;
    let pending_system_hint = &mut *ctx.pending_system_hint;
    let prompted_harness_overlay_ids = &mut *ctx.prompted_harness_overlay_ids;
    let harness_evolution_queue = &mut *ctx.harness_evolution_queue;
    let harness_evolution_warned = &mut *ctx.harness_evolution_warned;

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
                if !*harness_evolution_warned {
                    *harness_evolution_warned = true;
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
            harness_evolution_queue.build_overlay(task_harness, current_harness_proposal.as_ref())
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
                    if !*harness_evolution_warned {
                        *harness_evolution_warned = true;
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
    let acceptance = acceptance_report::CurrentEvidence::from_native(
        messages,
        test_cmd.as_deref(),
        tool_root_abs.as_deref(),
    );
    acceptance_schema::bind(
        &mut tools_clone,
        active_plan.as_ref(),
        &acceptance.commands,
        &acceptance.citable_checks,
    );
    acceptance_schema::bind_summary(
        &mut tools_clone,
        &final_handoff::report_context(&root_user_text, &messages, test_cmd.as_deref()),
    );
    msgs_for_call.push(json!({"role":"system", "content":next_action.prompt()}));
    let (msgs_for_call, compaction) =
        context_compaction::coder(&msgs_for_call, test_cmd.as_deref());
    emit_telemetry_event(&tx, "context_compaction", json!(compaction)).await;
    AssemblyOut {
        messages: msgs_for_call,
        tools: tools_clone,
        check_catalog,
        meta_harness,
        evaluator_loop,
    }
}
