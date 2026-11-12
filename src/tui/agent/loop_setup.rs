use super::*;

/// Persisted state loaded during loop setup plus the checkpoint handle and the
/// already-mutated message history (context notes are injected in place).
pub(crate) struct SetupPersisted {
    pub(crate) reflection_ledger: crate::reflection_ledger::ReflectionLedger,
    pub(crate) progress_state: Option<crate::progress_state::RepoProgressState>,
    pub(crate) harness_promoted_overlay: harness_evolution::GovernorContractOverlay,
    pub(crate) harness_evolution_queue: harness_evolution::HarnessEvolutionQueue,
    pub(crate) harness_evolution_path: Option<std::path::PathBuf>,
    pub(crate) reflection_ledger_path: Option<std::path::PathBuf>,
}

pub(crate) fn has_system_prefix(messages: &[serde_json::Value], prefix: &str) -> bool {
    messages.iter().any(|m| {
        m.get("role").and_then(|r| r.as_str()) == Some("system")
            && m.get("content")
                .and_then(|c| c.as_str())
                .map(|s| s.trim_start().starts_with(prefix))
                .unwrap_or(false)
    })
}

pub(crate) async fn load_persisted_state(
    tx: &mpsc::Sender<StreamToken>,
    tool_root_abs: &Option<String>,
) -> SetupPersisted {
    let reflection_ledger_path = tool_root_abs
        .as_deref()
        .map(crate::reflection_ledger::path_for_root);
    let reflection_ledger = if let Some(path) = reflection_ledger_path.as_ref() {
        match crate::reflection_ledger::ReflectionLedger::load(path) {
            Ok(ledger) => ledger,
            Err(e) => {
                let _ = tx
                    .send(StreamToken::Delta(format!(
                        "\n[reflection_ledger] WARN: {e:#}\n"
                    )))
                    .await;
                crate::reflection_ledger::ReflectionLedger::default()
            }
        }
    } else {
        crate::reflection_ledger::ReflectionLedger::default()
    };
    emit_telemetry_event(
        tx,
        "reflection_ledger_loaded",
        json!({
            "entries": reflection_ledger.entries.len(),
            "path": reflection_ledger_path.as_ref().map(|p| p.to_string_lossy().to_string()),
        }),
    )
    .await;
    let progress_state_path = tool_root_abs
        .as_deref()
        .map(crate::progress_state::path_for_root);
    let progress_state = if let Some(path) = progress_state_path.as_ref() {
        match crate::progress_state::RepoProgressState::load(path) {
            Ok(progress) => Some(progress),
            Err(_) if !path.exists() => None,
            Err(e) => {
                let _ = tx
                    .send(StreamToken::Delta(format!(
                        "\n[progress_state] WARN: {e:#}\n"
                    )))
                    .await;
                None
            }
        }
    } else {
        None
    };
    emit_telemetry_event(
        tx,
        "progress_state_loaded",
        json!({
            "path": progress_state_path.as_ref().map(|p| p.to_string_lossy().to_string()),
            "has_state": progress_state.is_some(),
            "completed_artifacts": progress_state.as_ref().map(|state| state.completed_artifacts.len()).unwrap_or(0),
            "verified_commands": progress_state.as_ref().map(|state| state.verified_commands.len()).unwrap_or(0),
        }),
    )
    .await;
    let harness_promoted_overlay_path = tool_root_abs
        .as_deref()
        .map(super::harness_evolution::overlay_path_for_root);
    let harness_promoted_overlay = if let Some(path) = harness_promoted_overlay_path.as_ref() {
        match harness_evolution::GovernorContractOverlay::load(path) {
            Ok(overlay) => overlay,
            Err(e) => {
                let _ = tx
                    .send(StreamToken::Delta(format!(
                        "\n[harness_evolution_promoted] WARN: {e:#}\n"
                    )))
                    .await;
                harness_evolution::GovernorContractOverlay::default()
            }
        }
    } else {
        harness_evolution::GovernorContractOverlay::default()
    };
    emit_telemetry_event(
        tx,
        "harness_evolution_promoted_loaded",
        json!({
            "policies": harness_promoted_overlay.promoted_policies.len(),
            "path": harness_promoted_overlay_path.as_ref().map(|p| p.to_string_lossy().to_string()),
        }),
    )
    .await;
    let harness_evolution_path = tool_root_abs
        .as_deref()
        .map(super::harness_evolution::path_for_root);
    let harness_evolution_queue = if let Some(path) = harness_evolution_path.as_ref() {
        match harness_evolution::HarnessEvolutionQueue::load(path) {
            Ok(queue) => queue,
            Err(e) => {
                let _ = tx
                    .send(StreamToken::Delta(format!(
                        "\n[harness_evolution] WARN: {e:#}\n"
                    )))
                    .await;
                harness_evolution::HarnessEvolutionQueue::default()
            }
        }
    } else {
        harness_evolution::HarnessEvolutionQueue::default()
    };
    emit_telemetry_event(
        tx,
        "harness_evolution_loaded",
        json!({
            "proposals": harness_evolution_queue.proposal_count(),
            "path": harness_evolution_path.as_ref().map(|p| p.to_string_lossy().to_string()),
        }),
    )
    .await;
    SetupPersisted {
        reflection_ledger,
        progress_state,
        harness_promoted_overlay,
        harness_evolution_queue,
        harness_evolution_path,
        reflection_ledger_path,
    }
}

/// Emits the git checkpoint token/notice and resolves the checkpoint to use.
pub(crate) async fn create_or_reemit_checkpoint(
    tx: &mpsc::Sender<StreamToken>,
    tool_root_abs: &Option<String>,
    checkpoint: &Option<String>,
    create_checkpoint: bool,
) -> Option<String> {
    let mut checkpoint = checkpoint.clone();
    if let Some(ref root) = tool_root_abs {
        if checkpoint.is_none() && create_checkpoint {
            if let Some(hash) = git_create_checkpoint(root).await {
                checkpoint = Some(hash.clone());
                let short = hash[..hash.len().min(8)].to_string();
                let _ = tx.send(StreamToken::Checkpoint(hash)).await;
                let _ = tx
                    .send(StreamToken::Delta(format!(
                        "[git checkpoint] {short} saved — use /rollback to restore\n\n"
                    )))
                    .await;
            }
        } else if let Some(ref hash) = checkpoint {
            let _ = tx.send(StreamToken::Checkpoint(hash.clone())).await;
        }
    }
    checkpoint
}

/// All loop-scoped state built by the run_agentic_json setup prologue, before
/// the iteration loop starts. The caller destructures this back into local
/// bindings so the loop body is unchanged.
pub(super) struct LoopSetup<'a> {
    pub(super) state: AgentState,
    pub(super) task_outcome: outcome::TaskOutcome,
    pub(super) pending_system_hint: Option<String>,
    pub(super) reflection_required: Option<String>,
    pub(super) impact_required: Option<String>,
    pub(super) reflection_trigger_sig: Option<ActionIdentity>,
    pub(super) reflection_guard: Option<ReflectionBlock>,
    pub(super) forced_tool_once: bool,
    pub(super) tool_calls_this_run: usize,
    pub(super) budget_warned: bool,
    pub(super) edit_failures: EditFailureMemory,
    pub(super) file_tool_consec_failures: usize,
    pub(super) root_user_text: String,
    pub(super) root_read_only: bool,
    pub(super) task_harness: TaskHarness,
    pub(super) exec_verification_context: exec_verification::ExecVerificationContext<'a>,
    pub(super) progress_context: crate::progress_state::ProgressSaveContext,
    pub(super) wants_repo_goal: bool,
    pub(super) wants_test_goal: bool,
    pub(super) wants_build_goal: bool,
    pub(super) goal_checks: GoalCheckTracker,
    pub(super) goal_wants_actions: bool,
    pub(super) intent_required_verification: VerificationLevel,
    pub(super) task_contract: TaskContract,
    pub(super) instruction_resolver: InstructionResolver,
    pub(super) realize_cfg: RealizeOnDemandConfig,
    pub(super) latent_plan: Option<LatentPlanBuffer>,
    pub(super) realize_metrics: RealizeMetrics,
    pub(super) messages: Vec<serde_json::Value>,
    pub(super) active_plan: ActivePlan,
    pub(super) path_required_verification: VerificationLevel,
    pub(super) required_verification: VerificationLevel,
    pub(super) last_reflection: Option<ReflectionBlock>,
    pub(super) working_mem: WorkingMemory,
    pub(super) assumption_ledger: AssumptionLedger,
    pub(super) observation_evidence: ObservationEvidence,
    pub(super) session_bridge: SessionBridgeView,
    pub(super) prompt_cache: StablePromptCache,
    pub(super) prompted_harness_overlay_ids: std::collections::BTreeSet<String>,
    pub(super) tool_root_abs: Option<String>,
    pub(super) mem: FailureMemory,
    pub(super) recovery: RecoveryGovernor,
    pub(super) step_seq: usize,
    pub(super) last_mutation_step: Option<usize>,
    pub(super) last_build_verify_ok_step: Option<usize>,
    pub(super) last_behavioral_verify_ok_step: Option<usize>,
    pub(super) last_exec_step: Option<usize>,
    pub(super) last_verify_ok_step: Option<usize>,
    pub(super) last_impact_step: Option<usize>,
    pub(super) reflection_ledger: crate::reflection_ledger::ReflectionLedger,
    pub(super) progress_state: Option<crate::progress_state::RepoProgressState>,
    pub(super) harness_promoted_overlay: harness_evolution::GovernorContractOverlay,
    pub(super) harness_evolution_queue: harness_evolution::HarnessEvolutionQueue,
    pub(super) harness_evolution_path: Option<std::path::PathBuf>,
    pub(super) reflection_ledger_path: Option<std::path::PathBuf>,
    pub(super) reflection_ledger_warned: bool,
    pub(super) harness_evolution_warned: bool,
    pub(super) checkpoint: Option<String>,
    pub(super) cur_cwd: Option<String>,
    pub(super) file_cache: std::collections::HashMap<String, String>,
    pub(super) read_only_diagnose_streak: usize,
    pub(super) read_only_diagnose_rescue_count: usize,
    pub(super) first_action_deadline: usize,
    pub(super) max_iters: usize,
    pub(super) initial_handoff_steps: usize,
    pub(super) observer_handoff: Option<observer_handoff::Failure>,
    pub(super) blocked_terminal: bool,
    pub(super) repair_extensions: usize,
    pub(super) repair_iterations: usize,
}

impl<'a> LoopSetup<'a> {
    /// Builds every loop-scoped binding the iteration loop needs: task contract
    /// inference, resume-state restoration, persisted state loading, checkpoint,
    /// context injection, and the initial governor/telemetry emissions.
    pub(super) async fn init(
        start: AgenticStartState,
        tx: &mpsc::Sender<StreamToken>,
        tool_root: Option<&str>,
        max_iters: usize,
        test_cmd: Option<&'a str>,
        project_context: Option<String>,
        agents_md: Option<String>,
        realize_preset: Option<RealizePreset>,
        autosaver: &Option<Arc<crate::agent_session::SessionAutoSaver>>,
    ) -> Self {
        let mut state = AgentState::Planning;
        let task_outcome = outcome::TaskOutcome::default();
        let mut pending_system_hint: Option<String> = None;
        let mut reflection_required: Option<String> = None;
        let mut impact_required: Option<String> = None;
        let mut reflection_trigger_sig: Option<ActionIdentity> = None;
        let reflection_guard: Option<ReflectionBlock> = None;
        let forced_tool_once = false;
        let tool_calls_this_run: usize = 0;
        // C — token budget guardian
        let budget_warned = false;
        // Diagnostic successes do not resolve failed edits, including after resume.
        let edit_context =
            exec_verification::ExecVerificationContext::from_messages(test_cmd, &start.messages);
        let edit_failures = EditFailureMemory::from_messages(&start.messages, &edit_context);
        let file_tool_consec_failures = edit_failures.count();
        if let Some(reason) = edit_failures.reflection_reason() {
            reflection_required = Some(reason);
        }
        if let Some((reason, trigger)) =
            observer_handoff::pending_exec_reflection(&start.messages, test_cmd)
        {
            reflection_required = Some(reason);
            reflection_trigger_sig = trigger;
        }

        let root_user_text = crate::task_origin::root_user_text(&start.messages).to_string();
        let root_user_text_low = root_user_text.to_ascii_lowercase();
        let root_read_only = is_root_read_only_observation_task(&root_user_text);
        let task_harness = TaskHarness::infer(&root_user_text, root_read_only);
        let exec_verification_context =
            exec_verification::ExecVerificationContext::from_root(test_cmd, &root_user_text);
        let progress_context = crate::progress_state::ProgressSaveContext::new(
            &root_user_text,
            task_harness.lane_label(),
            task_harness.artifact_mode_label(),
        );
        let verification_contract = governor_contract::verification();
        let wants_repo_goal = task_harness.allows_repo_goal_check()
            && text_contains_any(&root_user_text_low, &verification_contract.goal_repo_terms);
        let wants_test_goal =
            text_contains_any(&root_user_text_low, &verification_contract.goal_test_terms);
        let wants_build_goal =
            text_contains_any(&root_user_text_low, &verification_contract.goal_build_terms);
        let goal_checks = GoalCheckTracker::default();
        let goal_wants_actions = wants_local_actions(&root_user_text);
        let mut intent_required_verification =
            infer_required_verification_level(&root_user_text, test_cmd);
        let mut task_contract = derive_task_contract(
            &root_user_text,
            root_read_only,
            goal_wants_actions,
            intent_required_verification,
        );
        if is_continuation_request(&root_user_text) {
            task_contract.alignment_terms = crate::task_origin::prior_task_texts(&start.messages)
                .iter()
                .map(|text| text.to_string())
                .collect();
        }
        let instruction_resolver = InstructionResolver::new(
            task_contract.task_summary.as_str(),
            root_read_only,
            project_context.is_some()
                || agents_md.is_some()
                || has_project_rules_context(&start.messages),
        );
        let realize_cfg = RealizeOnDemandConfig::resolve(realize_preset);
        let latent_plan: Option<LatentPlanBuffer> = None;
        let realize_metrics = RealizeMetrics::default();

        // Keep messages as serde_json::Value throughout to preserve tool_call_id.
        let mut messages: Vec<serde_json::Value> = start.messages;
        if let Some(guidance) = task_harness::benchmark_replay::guidance(&root_user_text) {
            if !has_system_prefix(&messages, "[Replay case format]") {
                messages.push(json!({"role":"system","content":guidance,"ui_context":true}));
            }
        }
        if root_read_only && !has_system_prefix(&messages, "[Read-Only Task Contract]") {
            let pos = messages.len().min(4);
            messages.insert(
                pos,
                json!({"role":"system","content": read_only_task_addon()}),
            );
        }
        let active_plan = plan_resume::restore(&messages, |plan| {
            validate_plan_for_task_contract(
                plan,
                root_read_only,
                &task_contract,
                &instruction_resolver,
                &messages,
            )
            .is_ok()
        });
        if let Some(plan) = active_plan.as_ref() {
            if let Some(level) = verification_level_from_plan(plan) {
                intent_required_verification = level;
            }
        }
        let path_required_verification =
            upgrade_required_verification_from_messages(&messages, VerificationLevel::Build);
        let required_verification = intent_required_verification.max(path_required_verification);
        let last_reflection = last_reflection_from_messages(&messages);
        let working_mem = WorkingMemory::from_messages(&messages, test_cmd);
        let mut assumption_ledger = AssumptionLedger::from_messages(&messages, &working_mem);
        if let Some(work) = active_plan.work() {
            assumption_ledger.sync_to_plan(work);
        }
        assumption_ledger.refresh_confirmations(&working_mem);
        let mut observation_evidence = collect_observation_evidence(&messages);
        observation_evidence.merge_session_cache(start.observation_cache.as_ref());
        sync_observation_cache_autosave(autosaver, &observation_evidence);
        sync_progress_context_autosave(autosaver, progress_context.clone());
        let session_bridge = SessionBridgeView::resolve(start.session_bridge.as_ref(), &messages);
        let prompt_cache = StablePromptCache::default();
        let prompted_harness_overlay_ids = std::collections::BTreeSet::new();
        // Normalize before restoring file-localized recovery as well as tool execution.
        let tool_root_abs = tool_root
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .and_then(|root| {
                crate::exec::canonical_cwd(Some(root)).or_else(|| absolutize_path(root))
            });
        // Rebuild loop governor memory from the existing session so resuming runs doesn't
        // repeat the same failures from scratch.
        let mem = FailureMemory::from_recent_messages(&messages, test_cmd);
        let mut recovery =
            RecoveryGovernor::restore_from_session(&mem, &messages, required_verification);
        recovery.restore_pending_edits(&edit_failures);
        recovery.restore_focus(&messages, tool_root_abs.as_deref(), test_cmd);
        if recovery.in_recovery() {
            state = AgentState::Recovery;
        }
        // Carry the most recent declared strategy across resume, especially if we are
        // resuming mid-recovery after a failure.
        if recovery.in_recovery() {
            if let Some(ref r) = last_reflection {
                if r.strategy_change == StrategyChange::Abandon {
                    pending_system_hint = Some(format!(
                        "Strategy previously abandoned.\n\
Do not retry the previous approach.\n\
Execute only the new minimal action: {}",
                        r.next_minimal_action.as_str()
                    ));
                }
            }
        }
        // E — done gate: require verification after mutations.
        // Rebuild done-gate tracking from the existing session so resuming runs doesn't
        // require re-verifying unchanged work.
        let (
            step_seq,
            last_mutation_step,
            last_build_verify_ok_step,
            last_behavioral_verify_ok_step,
            last_exec_step,
        ) = restore_done_gate_from_messages(&messages, test_cmd);
        let last_verify_ok_step = effective_verify_ok_step(
            required_verification,
            last_build_verify_ok_step,
            last_behavioral_verify_ok_step,
        );
        let last_impact_step = last_impact_step_from_messages(&messages);
        if last_mutation_step.unwrap_or(0) > last_impact_step.unwrap_or(0) {
            impact_required =
                Some("recent mutation has not been evaluated for goal impact".to_string());
        }
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
        if realize_cfg.enabled {
            emit_realize_state(tx, &realize_cfg, None, 0, None, &realize_metrics).await;
        }
        emit_telemetry_event(
            tx,
            "task_harness",
            json!({
                "lane": task_harness.lane_label(),
                "artifact_mode": task_harness.artifact_mode_label(),
                "read_only": root_read_only,
            }),
        )
        .await;

        let persisted = load_persisted_state(tx, &tool_root_abs).await;
        let reflection_ledger = persisted.reflection_ledger;
        let progress_state = persisted.progress_state;
        let harness_promoted_overlay = persisted.harness_promoted_overlay;
        let harness_evolution_queue = persisted.harness_evolution_queue;
        let harness_evolution_path = persisted.harness_evolution_path;
        let reflection_ledger_path = persisted.reflection_ledger_path;
        let reflection_ledger_warned = false;
        let harness_evolution_warned = false;

        let checkpoint = create_or_reemit_checkpoint(
            tx,
            &tool_root_abs,
            &start.checkpoint,
            start.create_checkpoint,
        )
        .await;
        inject_setup_context(&mut messages, &tool_root_abs, project_context, agents_md);

        let mut cur_cwd: Option<String> = start.cur_cwd.clone().or_else(|| tool_root_abs.clone());
        if let (Some(ref root), Some(ref cwd)) = (tool_root_abs.as_ref(), cur_cwd.as_ref()) {
            if !is_within_root(cwd, root) {
                cur_cwd = tool_root_abs.clone();
            }
        }

        // Seed the session file early so long runs can resume even if interrupted.
        autosave_best_effort(
            autosaver,
            tx,
            tool_root_abs.as_deref(),
            checkpoint.as_deref(),
            cur_cwd.as_deref(),
            &messages,
        )
        .await;

        // Session-scoped file read cache.
        // Key: canonical path string.  Invalidated on write_file / patch_file success.
        let file_cache: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        let read_only_diagnose_streak: usize = 0;
        let read_only_diagnose_rescue_count: usize = 0;
        let first_action_deadline = first_action_deadline_iters(root_read_only, goal_wants_actions);

        let max_iters = max_iters.max(1).min(64);
        let initial_handoff_steps = step_seq;
        let observer_handoff = None;
        let blocked_terminal = false;
        let repair_extensions = 0;
        let repair_iterations =
            crate::work_budget::current().map_or(0, |c| c.budget.transport_remaining());
        LoopSetup {
            state,
            task_outcome,
            pending_system_hint,
            reflection_required,
            impact_required,
            reflection_trigger_sig,
            reflection_guard,
            forced_tool_once,
            tool_calls_this_run,
            budget_warned,
            edit_failures,
            file_tool_consec_failures,
            root_user_text,
            root_read_only,
            task_harness,
            exec_verification_context,
            progress_context,
            wants_repo_goal,
            wants_test_goal,
            wants_build_goal,
            goal_checks,
            goal_wants_actions,
            intent_required_verification,
            task_contract,
            instruction_resolver,
            realize_cfg,
            latent_plan,
            realize_metrics,
            messages,
            active_plan,
            path_required_verification,
            required_verification,
            last_reflection,
            working_mem,
            assumption_ledger,
            observation_evidence,
            session_bridge,
            prompt_cache,
            prompted_harness_overlay_ids,
            tool_root_abs,
            mem,
            recovery,
            step_seq,
            last_mutation_step,
            last_build_verify_ok_step,
            last_behavioral_verify_ok_step,
            last_exec_step,
            last_verify_ok_step,
            last_impact_step,
            reflection_ledger,
            progress_state,
            harness_promoted_overlay,
            harness_evolution_queue,
            harness_evolution_path,
            reflection_ledger_path,
            reflection_ledger_warned,
            harness_evolution_warned,
            checkpoint,
            cur_cwd,
            file_cache,
            read_only_diagnose_streak,
            read_only_diagnose_rescue_count,
            first_action_deadline,
            max_iters,
            initial_handoff_steps,
            observer_handoff,
            blocked_terminal,
            repair_extensions,
            repair_iterations,
        }
    }
}

/// Injects the working-directory note, project context, and AGENTS.md/
/// .spiral-coder.md rules into the message history (once each).
pub(crate) fn inject_setup_context(
    messages: &mut Vec<serde_json::Value>,
    tool_root_abs: &Option<String>,
    project_context: Option<String>,
    agents_md: Option<String>,
) {
    if let Some(ref root) = tool_root_abs {
        let _ = std::fs::create_dir_all(root);
        let note = format!(
            "[Working directory]\n\
Working directory (tool_root): {root}\n\
IMPORTANT: Each exec runs in a fresh process; `cd` does NOT persist unless the tool reports cwd.\n\
 Always operate under tool_root. Create new repos under tool_root (fresh directory).\n\
 NEVER create a git repo inside another git repo. If you see 'embedded git repository', STOP and relocate."
        );
        if !has_system_prefix(messages, "[Working directory]") {
            if messages.first().and_then(|m| m["role"].as_str()) == Some("system") {
                messages.insert(1, json!({"role":"system","content": note}));
            } else {
                messages.insert(0, json!({"role":"system","content": note}));
            }
        }
    }
    if let Some(ctx_text) = project_context {
        if !ctx_text.is_empty() {
            if !has_system_prefix(messages, "[Project Context") {
                let pos = messages.len().min(2);
                messages.insert(pos, json!({"role":"system","content": ctx_text}));
            }
        }
    }
    if let Some(agents_text) = agents_md {
        if !agents_text.is_empty() {
            if !has_system_prefix(
                messages,
                "[Project Instructions — .spiral-coder.md / AGENTS.md]",
            ) {
                let pos = messages.len().min(3);
                messages.insert(pos, json!({
                    "role": "system",
                    "content": format!("[Project Instructions — .spiral-coder.md / AGENTS.md]\n{agents_text}")
                }));
            }
        }
    }
}
