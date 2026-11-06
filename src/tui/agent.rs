/// Coder agentic loop: calls the model with an `exec` tool, runs commands,
/// and loops until finish_reason == "stop" or max iterations are reached.
///
/// Reasoning improvements applied here:
///   1. Scratchpad protocol  — model outputs structured <plan>/<think> blocks
///      before every tool call (~50 tokens).  Prevents wrong-direction errors.
///   2. Tool output truncation  — stdout > 1500 chars / stderr > 600 chars are trimmed.
///      This is the single largest token-saver on long runs.
///   3. Context pruning  — tool results older than KEEP_RECENT_TOOL_TURNS are collapsed
///      to their first line.  Keeps the context window from exploding across sessions.
///   4. Error classification  — on exit_code != 0, `classify_error()` identifies the
///      error type (env/syntax/path/dep/network/logic) and injects a targeted recovery
///      hint before the generic diagnosis protocol.
///   5. tool_call_id preserved  — messages stay as serde_json::Value all the way to the
///      provider, so the id field is never silently dropped.
///   6. Progress checkpoints  — at iter 3/6/9 the model emits a short <reflect> block
///      (goal_delta, wrong_assumption, strategy_change, next_minimal_action) before continuing.
///   7. Working memory  — rebuilds confirmed facts / completed steps / known-good
///      verification commands from session messages, so resume runs continue from
///      verified context instead of only remembering failures.
///   8. Impact check  — after every successful mutation, require a short <impact>
///      block that states what changed and what acceptance criterion moved before
///      allowing the next tool call.
///   9. Realize-on-demand (experimental) — plan-bearing no-tool turns can stay latent
///      until a tool call or deadline materializes them back into audit history.
use anyhow::{anyhow, Result};
use serde_json::json;
use std::collections::hash_map::DefaultHasher;
use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::approvals::{ApprovalOutcome, ApprovalRequest, Approver};
use crate::config::{ProviderKind, RunConfig};
use crate::exec;
use crate::governor_contract;
use crate::streaming::{
    stream_openai_compat_json, GovernorState, RealizeState, ReflectionSummary, StreamToken,
    TelemetryEvent, ToolCallData,
};
use crate::types::ChatMessage;
use std::path::Path;

mod acceptance_report;
mod acceptance_schema;
mod assumption_gate;
#[cfg(test)]
mod auto_test_recovery_tests;
mod automatic_test;
mod block_parsers;
mod blocked_report;
#[cfg(test)]
mod cargo_input_runtime_tests;
mod check_catalog;
#[cfg(test)]
mod check_catalog_tests;
pub(crate) mod current_evidence;
mod deferred_edit;
mod done_gate;
mod edit_failure;
mod evaluator_loop;
mod evidence_collect;
pub(crate) mod evidence_packet;
mod exact_content;
pub(crate) mod exec_classification;
#[cfg(test)]
mod exec_location_tests;
mod exec_proof;
mod exec_verification;
mod explicit_checks;
mod explicit_edit_scope;
mod failure_diagnostics;
mod failure_localization;
mod failure_memory;
mod final_handoff;
mod final_instructions;
mod followup_requirements;
mod git_guard;
mod governor_emit;
mod governor_signals;
mod harness_evolution;
mod impact_recovery;
mod implied_actions;
mod json_fields;
mod memory;
mod message_window;
mod meta_harness;
mod native_arguments;
mod next_action;
#[cfg(test)]
mod next_action_tests;
#[cfg(test)]
mod no_change_runtime_tests;
mod observation_stall;
mod outcome;
mod output_compaction;
mod plan_governor;
mod plan_resume;
mod plan_revision;
#[cfg(test)]
mod plan_revision_tests;
mod progress_bridge;
mod prompt_setup;
mod prose_context;
mod protocol_fields;
mod protocol_framing;
mod protocol_lists;
mod protocol_state;
mod protocol_validation;
mod provider_compat;
#[cfg(test)]
mod provider_signature_tests;
mod provider_turn;
mod read_context;
mod read_only;
mod realize;
mod recovery;
mod recovery_focus;
#[cfg(test)]
mod recovery_policy_tests;
mod reflection_action;
#[cfg(test)]
mod reflection_action_tests;
mod repair_brief;
#[cfg(test)]
mod repair_feedback_tests;
#[cfg(test)]
mod replay_input_tests;
mod repo_map_hints;
mod repo_scaffold;
mod report_counts;
#[cfg(test)]
mod report_counts_tests;
mod report_outcomes;
mod report_repair;
use crate::cargo_inputs::{Probe as CargoInputProbe, Receipt as CargoInputReceipt};
#[cfg(test)]
use automatic_test::run as run_test_cmd;
pub(crate) mod iteration_handoff;
mod observer_handoff;
mod report_surface;
mod requirement_proof;
#[cfg(test)]
mod resume_contract_tests;
mod session_bridge;
mod state_prompts;
mod strategy_memory;
mod success_output;
mod task_harness;
pub(crate) mod task_obligations;
mod telemetry;
mod tool_result_history;
#[cfg(test)]
mod tool_result_history_tests;
mod verification;
mod work_progress;
use self::assumption_gate::refuted_assumption_conflict;
use self::block_parsers::*;
use self::done_gate::{
    build_done_acceptance_recovery_hint, build_post_verify_done_completion_hint,
    build_read_only_completion_hint, build_read_only_evidence_scores,
    build_read_only_iteration_cap_final_answer, build_read_only_strong_final_answer,
    canonicalize_known_acceptance_commands, maybe_build_read_only_auto_final_answer,
    maybe_build_verified_action_auto_final_answer, should_prefer_done_after_verified_action,
    synthesize_action_done_summary, validate_done_acceptance,
};
use self::edit_failure::EditFailureMemory;
use self::evaluator_loop::EvaluatorLoop;
use self::evidence_collect::*;
use self::exec_proof::restore_done_gate_from_messages;
use self::failure_localization::interesting_failure_line;
use self::failure_memory::FailureMemory;
use self::final_handoff::{
    authored_final_answer_hint, enrich_text_final_handoff, requires_authored_final_answer,
    validate_authored_done_summary,
};
use self::followup_requirements::{
    coerce_existing_followup_tool_call, matches_required_existing_followup,
    required_existing_followup_no_tool_hint,
};
use self::git_guard::*;
use self::governor_emit::*;
use self::governor_signals::*;
pub(crate) use self::harness_evolution::{
    overlay_path_for_root as harness_evolution_overlay_path_for_root,
    path_for_root as harness_evolution_queue_path_for_root,
    GovernorContractOverlay as HarnessGovernorContractOverlay,
    HarnessEvolutionQueue as PersistentHarnessEvolutionQueue,
};
use self::harness_evolution::{
    ContractPatchProposal, GovernorContractOverlay, HarnessEvolutionQueue,
};
use self::memory::{
    canonicalize_evidence_command_with_resolution, normalize_memory_entry, normalize_path_alias,
    remember_recent_unique, remember_repo_map_resolution, rewrite_tool_call_with_resolution,
    ObservationEvidence, ObservationReadEvidence, ObservationSearchEvidence,
};
#[cfg(test)]
use self::message_window::prune_message_window;
use self::message_window::prune_message_window_with_context;
use self::meta_harness::MetaHarness;
use self::output_compaction::*;
use self::plan_governor::*;
use self::progress_bridge::ProgressBridgeView;
use self::prompt_setup::*;
pub use self::prompt_setup::{
    apply_diff_tool_def, coder_system, done_tool_def, exec_tool_def, glob_tool_def,
    list_dir_tool_def, patch_file_tool_def, read_file_tool_def, search_files_tool_def,
    write_file_tool_def, DEFAULT_MAX_ITERS,
};
use self::protocol_fields::parse_tag_fields;
use self::protocol_lists::parse_plan_items;
use self::protocol_state::*;
use self::protocol_validation::{
    validate_impact, validate_plan, validate_reflection, validate_think,
};
#[cfg(test)]
use self::provider_compat::compat_synthetic_think;
use self::provider_compat::{
    build_mistral_think_only_hint, mistral_observation_tool, normalize_mistral_tool_call,
    repair_mistral_plan_for_tool_turn, repair_truncated_patch_tool_call_from_recent_mismatch,
    rescue_missing_evidence_for_tool_turn, rescue_missing_impact_for_tool_turn,
    rescue_missing_plan_for_tool_turn, rescue_missing_reflection_for_tool_turn,
    rescue_missing_think_for_tool_turn, rescue_read_only_missing_plan_for_tool_turn,
    rescue_read_only_missing_think_for_tool_turn, select_think_for_tool_turn,
};
#[cfg(test)]
use self::read_only::best_read_only_followup_read_path;
use self::read_only::{
    build_first_action_constraint_hint, build_read_only_diagnose_coercion_hint,
    build_read_only_plan_rewrite_hint, build_read_only_search_to_read_hint,
    choose_read_only_diagnose_rescue_action, coerce_read_only_followup_read_tool_call,
    coerce_read_only_observation_tool_call, first_action_deadline_iters,
    is_root_read_only_observation_task, path_prior_score, preferred_read_only_read_path_hint,
    preferred_read_only_search_dir, preferred_read_only_search_pattern, read_only_plan_violation,
    synthetic_read_only_observation_plan, ReadOnlyDiagnoseRescueAction,
};
pub use self::realize::RealizePreset;
use self::realize::*;
use self::recovery::RecoveryGovernor;
use self::reflection_action::{tool_call_action_sig, ActionIdentity};
use self::repo_map_hints::*;
use self::session_bridge::SessionBridgeView;
use self::state_prompts::*;
use self::success_output::suspicious_success_reason;
use self::task_harness::{
    allows_artifact_creation_during_diagnose, allows_artifact_creation_during_verify,
    allows_benchmark_plan_followup_during_verify, benchmark_plan_pending_required_exec_command,
    build_fix_stage_progress_hint, build_progress_gate_block, coerce_artifact_creation_tool_call,
    coerce_benchmark_plan_tool_call, coerce_fix_existing_blocked_mutation_tool_call,
    coerce_fix_existing_literal_mutation_tool_call, coerce_fix_existing_tool_call,
    coerce_repo_goal_completion_tool_call, repair_fix_existing_mutation_tool_call,
    repair_repo_scaffold_write_tool_call, synthesize_benchmark_plan_no_tool_call,
    synthesize_fix_existing_no_tool_mutation_tool_call, TaskHarness,
};
use self::telemetry::*;
pub(crate) use self::verification::exec_may_mutate_for_evaluation;
use self::verification::*;

#[derive(Debug, Clone)]
pub struct AgenticStartState {
    pub messages: Vec<serde_json::Value>,
    pub checkpoint: Option<String>,
    pub cur_cwd: Option<String>,
    pub observation_cache: Option<crate::agent_session::ObservationCache>,
    pub session_bridge: Option<crate::agent_session::SessionBridge>,
    pub create_checkpoint: bool,
}

#[derive(Debug, Clone)]
pub struct AgenticEndState {
    pub messages: Vec<serde_json::Value>,
    pub checkpoint: Option<String>,
    pub cur_cwd: Option<String>,
    pub observation_cache: Option<crate::agent_session::ObservationCache>,
}

// ── Tunables ─────────────────────────────────────────────────────────────────

// ── System prompt addons ──────────────────────────────────────────────────────

// ── Tool definition ───────────────────────────────────────────────────────────

// ── System prompt builders ────────────────────────────────────────────────────

// ── Token-efficient helpers ───────────────────────────────────────────────────

// ── Error classification ──────────────────────────────────────────────────────

// ── Tool output builders ──────────────────────────────────────────────────────

// ── Loop Governor (Coder strengthening) ────────────────────────────────────────

// ── Git helpers ───────────────────────────────────────────────────────────────

// ── Agentic loop ──────────────────────────────────────────────────────────────

/// Run the agentic loop.  Sends StreamToken events to `tx` for the TUI to display.
/// The caller builds the initial messages (system + history + user).
#[allow(dead_code)]
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
    let mut state = AgentState::Planning;
    let mut task_outcome = outcome::TaskOutcome::default();
    let mut pending_system_hint: Option<String> = None;
    let mut reflection_required: Option<String> = None;
    let mut impact_required: Option<String> = None;
    let mut reflection_trigger_sig: Option<ActionIdentity> = None;
    let mut reflection_guard: Option<ReflectionBlock> = None;
    let mut forced_tool_once = false;
    let mut tool_calls_this_run: usize = 0;
    // C — token budget guardian
    let mut budget_warned = false;
    // Diagnostic successes do not resolve failed edits, including after resume.
    let edit_context = exec_verification::ExecVerificationContext::from_messages(
        test_cmd.as_deref(),
        &start.messages,
    );
    let mut edit_failures = EditFailureMemory::from_messages(&start.messages, &edit_context);
    let mut file_tool_consec_failures = edit_failures.count();
    if let Some(reason) = edit_failures.reflection_reason() {
        reflection_required = Some(reason);
    }
    if let Some((reason, trigger)) =
        observer_handoff::pending_exec_reflection(&start.messages, test_cmd.as_deref())
    {
        reflection_required = Some(reason);
        reflection_trigger_sig = trigger;
    }

    let root_user_text = crate::task_origin::root_user_text(&start.messages).to_string();
    let root_user_text_low = root_user_text.to_ascii_lowercase();
    let root_read_only = is_root_read_only_observation_task(&root_user_text);
    let task_harness = TaskHarness::infer(&root_user_text, root_read_only);
    let exec_verification_context =
        exec_verification::ExecVerificationContext::from_root(test_cmd.as_deref(), &root_user_text);
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
    let mut goal_checks = GoalCheckTracker::default();
    let goal_wants_actions = wants_local_actions(&root_user_text);
    let mut intent_required_verification =
        infer_required_verification_level(&root_user_text, test_cmd.as_deref());
    let task_contract = derive_task_contract(
        &root_user_text,
        root_read_only,
        goal_wants_actions,
        intent_required_verification,
    );
    let instruction_resolver = InstructionResolver::new(
        task_contract.task_summary.as_str(),
        root_read_only,
        project_context.is_some()
            || agents_md.is_some()
            || has_project_rules_context(&start.messages),
    );
    let realize_cfg = RealizeOnDemandConfig::resolve(realize_preset);
    let mut latent_plan: Option<LatentPlanBuffer> = None;
    let mut realize_metrics = RealizeMetrics::default();

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
    let mut active_plan = plan_resume::latest_valid(&messages, |plan| {
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
    let mut path_required_verification =
        upgrade_required_verification_from_messages(&messages, VerificationLevel::Build);
    let mut required_verification = intent_required_verification.max(path_required_verification);
    let mut last_reflection = last_reflection_from_messages(&messages);
    let mut working_mem = WorkingMemory::from_messages(&messages, test_cmd.as_deref());
    let mut assumption_ledger = AssumptionLedger::from_messages(&messages, &working_mem);
    if let Some(plan) = active_plan.as_ref() {
        assumption_ledger.sync_to_plan(plan);
    }
    assumption_ledger.refresh_confirmations(&working_mem);
    let mut observation_evidence = collect_observation_evidence(&messages);
    observation_evidence.merge_session_cache(start.observation_cache.as_ref());
    sync_observation_cache_autosave(&autosaver, &observation_evidence);
    sync_progress_context_autosave(&autosaver, progress_context.clone());
    let session_bridge = SessionBridgeView::resolve(start.session_bridge.as_ref(), &messages);
    let mut prompt_cache = StablePromptCache::default();
    let mut prompted_harness_overlay_ids = std::collections::BTreeSet::new();
    // Normalize before restoring file-localized recovery as well as tool execution.
    let tool_root_abs = tool_root
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .and_then(absolutize_path)
        .map(|root| {
            std::fs::canonicalize(&root)
                .map(|path| path.to_string_lossy().into_owned())
                .unwrap_or(root)
        });
    // Rebuild loop governor memory from the existing session so resuming runs doesn't
    // repeat the same failures from scratch.
    let mut mem = FailureMemory::from_recent_messages(&messages, test_cmd.as_deref());
    let mut recovery =
        RecoveryGovernor::restore_from_session(&mem, &messages, required_verification);
    recovery.restore_pending_edits(&edit_failures);
    recovery.restore_focus(&messages, tool_root_abs.as_deref(), test_cmd.as_deref());
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
        mut step_seq,
        mut last_mutation_step,
        mut last_build_verify_ok_step,
        mut last_behavioral_verify_ok_step,
        mut last_exec_step,
    ) = restore_done_gate_from_messages(&messages, test_cmd.as_deref());
    let mut last_verify_ok_step = effective_verify_ok_step(
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
        emit_realize_state(&tx, &realize_cfg, None, 0, None, &realize_metrics).await;
    }
    emit_telemetry_event(
        &tx,
        "task_harness",
        json!({
            "lane": task_harness.lane_label(),
            "artifact_mode": task_harness.artifact_mode_label(),
            "read_only": root_read_only,
        }),
    )
    .await;

    let reflection_ledger_path = tool_root_abs
        .as_deref()
        .map(crate::reflection_ledger::path_for_root);
    let mut reflection_ledger = if let Some(path) = reflection_ledger_path.as_ref() {
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
    let mut reflection_ledger_warned = false;
    emit_telemetry_event(
        &tx,
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
        &tx,
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
        .map(self::harness_evolution::overlay_path_for_root);
    let harness_promoted_overlay = if let Some(path) = harness_promoted_overlay_path.as_ref() {
        match GovernorContractOverlay::load(path) {
            Ok(overlay) => overlay,
            Err(e) => {
                let _ = tx
                    .send(StreamToken::Delta(format!(
                        "\n[harness_evolution_promoted] WARN: {e:#}\n"
                    )))
                    .await;
                GovernorContractOverlay::default()
            }
        }
    } else {
        GovernorContractOverlay::default()
    };
    emit_telemetry_event(
        &tx,
        "harness_evolution_promoted_loaded",
        json!({
            "policies": harness_promoted_overlay.promoted_policies.len(),
            "path": harness_promoted_overlay_path.as_ref().map(|p| p.to_string_lossy().to_string()),
        }),
    )
    .await;
    let harness_evolution_path = tool_root_abs
        .as_deref()
        .map(self::harness_evolution::path_for_root);
    let mut harness_evolution_queue = if let Some(path) = harness_evolution_path.as_ref() {
        match HarnessEvolutionQueue::load(path) {
            Ok(queue) => queue,
            Err(e) => {
                let _ = tx
                    .send(StreamToken::Delta(format!(
                        "\n[harness_evolution] WARN: {e:#}\n"
                    )))
                    .await;
                HarnessEvolutionQueue::default()
            }
        }
    } else {
        HarnessEvolutionQueue::default()
    };
    let mut harness_evolution_warned = false;
    emit_telemetry_event(
        &tx,
        "harness_evolution_loaded",
        json!({
            "proposals": harness_evolution_queue.proposal_count(),
            "path": harness_evolution_path.as_ref().map(|p| p.to_string_lossy().to_string()),
        }),
    )
    .await;

    fn has_system_prefix(messages: &[serde_json::Value], prefix: &str) -> bool {
        messages.iter().any(|m| {
            m.get("role").and_then(|r| r.as_str()) == Some("system")
                && m.get("content")
                    .and_then(|c| c.as_str())
                    .map(|s| s.trim_start().starts_with(prefix))
                    .unwrap_or(false)
        })
    }

    let mut checkpoint = start.checkpoint.clone();
    // D — git checkpoint: snapshot HEAD so the user can /rollback if the session goes wrong.
    if let Some(ref root) = tool_root_abs {
        if checkpoint.is_none() && start.create_checkpoint {
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
            // Resume: re-emit checkpoint token for UI/CLI consumers.
            let _ = tx.send(StreamToken::Checkpoint(hash.clone())).await;
        }
    }

    if let Some(ref root) = tool_root_abs {
        let _ = std::fs::create_dir_all(root);
        let note = format!(
            "[Working directory]\n\
Working directory (tool_root): {root}\n\
IMPORTANT: Each exec runs in a fresh process; `cd` does NOT persist unless the tool reports cwd.\n\
 Always operate under tool_root. Create new repos under tool_root (fresh directory).\n\
 NEVER create a git repo inside another git repo. If you see 'embedded git repository', STOP and relocate."
        );
        if !has_system_prefix(&messages, "[Working directory]") {
            if messages.first().and_then(|m| m["role"].as_str()) == Some("system") {
                messages.insert(1, json!({"role":"system","content": note}));
            } else {
                messages.insert(0, json!({"role":"system","content": note}));
            }
        }
    }
    // Project context — inject once at position 2 (after [Working directory] note).
    if let Some(ctx_text) = project_context {
        if !ctx_text.is_empty() {
            if !has_system_prefix(&messages, "[Project Context") {
                let pos = messages.len().min(2);
                messages.insert(pos, json!({"role":"system","content": ctx_text}));
            }
        }
    }
    // AGENTS.md / .spiral-coder.md — project-specific rules injected right after project context.
    // These take precedence over generic instructions and can override coding conventions.
    if let Some(agents_text) = agents_md {
        if !agents_text.is_empty() {
            if !has_system_prefix(
                &messages,
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

    let mut cur_cwd: Option<String> = start.cur_cwd.clone().or_else(|| tool_root_abs.clone());
    if let (Some(ref root), Some(ref cwd)) = (tool_root_abs.as_ref(), cur_cwd.as_ref()) {
        if !is_within_root(cwd, root) {
            cur_cwd = tool_root_abs.clone();
        }
    }

    // Seed the session file early so long runs can resume even if interrupted.
    autosave_best_effort(
        &autosaver,
        &tx,
        tool_root_abs.as_deref(),
        checkpoint.as_deref(),
        cur_cwd.as_deref(),
        &messages,
    )
    .await;

    // Session-scoped file read cache.
    // Key: canonical path string.  Invalidated on write_file / patch_file success.
    let mut file_cache: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    let mut read_only_diagnose_streak: usize = 0;
    let mut read_only_diagnose_rescue_count: usize = 0;
    let first_action_deadline = first_action_deadline_iters(root_read_only, goal_wants_actions);

    let max_iters = max_iters.max(1).min(64);
    let initial_handoff_steps = step_seq;
    let mut observer_handoff = None;
    let mut blocked_terminal = false;
    let mut repair_extensions = 0;
    let repair_iterations =
        crate::work_budget::current().map_or(0, |c| c.budget.transport_remaining());
    for iter in 0..max_iters.saturating_add(repair_iterations) {
        if iter >= max_iters.saturating_add(repair_extensions)
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
        let remaining_calls = shared_remaining.unwrap_or(max_iters.saturating_sub(iter));
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
        if shared_remaining.map_or(iter + 1 == max_iters, |remaining| remaining == 1) {
            let final_hint = format!(
                "[Final Iteration — iter {}/{}]\n\
This is the LAST model call for this run.\n\
- Only ONE tool call remains. If the task is fully done AND verified, call `done` now with the requested final report. Do not rerun already successful checks just for reporting.\n\
- If the task is NOT done: call `done` with (1) verified-complete items, (2) satisfied acceptance criteria, (3) remaining acceptance criteria, (4) verification evidence for completed criteria, and (5) the exact next commands/files to continue on the next run.",
                iter + 1,
                max_iters
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
            let mut reflection_prompt =
                build_reflection_prompt(reason, &mem, state, file_tool_consec_failures);
            let recent =
                crate::agent_session::recent_reflection_summaries_from_messages(&messages, 2);
            if !recent.is_empty() {
                reflection_prompt.push_str("\n\n[Recent Reflections]\n");
                for r in recent.iter().rev() {
                    let delta = r.goal_delta.as_deref().unwrap_or("-");
                    let strat = r.strategy_change.as_deref().unwrap_or("-");
                    let wrong = compact_one_line(r.wrong_assumption.as_deref().unwrap_or("-"), 90);
                    let next =
                        compact_one_line(r.next_minimal_action.as_deref().unwrap_or("-"), 90);
                    reflection_prompt.push_str(&format!(
                        "- delta={delta} strategy={strat} wrong={wrong} next={next}\n"
                    ));
                }
                reflection_prompt.push_str("Do not repeat the same mistake verbatim.\n");
            }
            msgs_for_call.push(json!({
                "role": "system",
                "content": reflection_prompt
            }));
        }

        let (token_tx, mut token_rx) = mpsc::channel::<StreamToken>(256);
        let cfg_clone = cfg.clone();
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
        let client_clone = client.clone();
        let msgs_clone = msgs_for_call;

        let request_context = crate::work_budget::current();
        let request_cursor = request_context
            .as_ref()
            .map(|c| c.budget.snapshot().requests.len());
        let stream_task = tokio::spawn(crate::work_budget::scope(request_context, async move {
            stream_openai_compat_json(
                &client_clone,
                &cfg_clone,
                &msgs_clone,
                Some(&tools_clone),
                token_tx,
            )
            .await
        }));
        let _cancel_stream = crate::streaming::request_guard::AbortOnDrop::new(&stream_task);

        let mut assistant_text = String::new();
        let mut tool_calls: Vec<ToolCallData> = Vec::new();
        let mut stream_error: Option<String> = None;

        while let Some(token) = token_rx.recv().await {
            match token {
                StreamToken::Delta(s) => {
                    assistant_text.push_str(&s);
                    let _ = tx.send(StreamToken::Delta(s)).await;
                }
                StreamToken::ToolCall(tc) => {
                    if matches!(cfg.provider, ProviderKind::Mistral) {
                        let (blocks, normalized) = normalize_mistral_tool_call(&tc);
                        for block_text in blocks {
                            if !assistant_text.trim().is_empty() {
                                assistant_text.push_str("\n\n");
                            }
                            assistant_text.push_str(&block_text);
                            let _ = tx.send(StreamToken::Delta(block_text)).await;
                        }
                        if let Some(tc) = normalized {
                            tool_calls.push(tc);
                            continue;
                        }
                        continue;
                    }
                    tool_calls.push(tc);
                }
                StreamToken::GovernorState(_) => {} // not emitted by inner stream
                StreamToken::RealizeState(_) => {}  // not emitted by inner stream
                StreamToken::Telemetry(ev) => {
                    let _ = tx.send(StreamToken::Telemetry(ev)).await;
                }
                StreamToken::Done => break,
                StreamToken::Error(e) => {
                    let _ = tx.send(StreamToken::Error(e.clone())).await;
                    stream_error = Some(e);
                    break;
                }
                StreamToken::Checkpoint(_) => {} // not emitted by inner stream
            }
        }

        match stream_task.await {
            Err(join_err) => {
                let msg = format!("stream task panicked: {join_err}");
                if stream_error.is_none() {
                    let _ = tx.send(StreamToken::Error(msg.clone())).await;
                }
                stream_error = Some(msg);
            }
            Ok(Err(e)) => {
                // Stream failed (network error, bad status, etc.) — surface it.
                let msg = format!("{e:#}");
                if stream_error.is_none() {
                    let _ = tx.send(StreamToken::Error(msg.clone())).await;
                }
                stream_error = Some(msg);
            }
            Ok(Ok(())) => {}
        }

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
        // When reflection is required, the model MUST emit exactly one <reflect> block
        // AND exactly one tool call in the same assistant turn.
        if let Some(reason) = reflection_required.clone() {
            let reflect = match parse_reflection_block(&assistant_text) {
                Some(r) => r,
                None => {
                    if tool_calls.len() == 1 {
                        if let Some(reflect) = rescue_missing_reflection_for_tool_turn(
                            reason.as_str(),
                            &tool_calls[0],
                            root_read_only,
                            goal_wants_actions,
                            cfg.provider.clone(),
                            &mem,
                            file_tool_consec_failures,
                        ) {
                            let _ = tx
                                .send(StreamToken::Delta(
                                    "\n[compat] synthesized reflection block for the rescued tool turn\n"
                                        .to_string(),
                                ))
                                .await;
                            reflect
                        } else {
                            let msg = provider_turn::tool_count_feedback(
                                governor_contract::reflection_missing_message(reason.as_str()),
                                &tool_calls,
                            );
                            let _ = tx
                                .send(StreamToken::Delta(format!("\n[reflect] {msg}\n")))
                                .await;
                            impact_recovery::emit_rejection(
                                &tx,
                                "reflection",
                                &msg,
                                &tool_calls,
                                &assistant_text,
                            )
                            .await;
                            pending_system_hint = Some(msg);
                            state = AgentState::Recovery;
                            reflection_required = Some(reason);
                            continue;
                        }
                    } else {
                        let msg = provider_turn::tool_count_feedback(
                            governor_contract::reflection_missing_message(reason.as_str()),
                            &tool_calls,
                        );
                        let _ = tx
                            .send(StreamToken::Delta(format!("\n[reflect] {msg}\n")))
                            .await;
                        impact_recovery::emit_rejection(
                            &tx,
                            "reflection",
                            &msg,
                            &tool_calls,
                            &assistant_text,
                        )
                        .await;
                        pending_system_hint = Some(msg);
                        state = AgentState::Recovery;
                        reflection_required = Some(reason);
                        continue;
                    }
                }
            };

            if let Err(e) = validate_reflection(&reflect, &mem, file_tool_consec_failures) {
                let msg = provider_turn::tool_count_feedback(
                    governor_contract::reflection_invalid_message(&e.to_string(), reason.as_str()),
                    &tool_calls,
                );
                let _ = tx
                    .send(StreamToken::Delta(format!("\n[reflect] {msg}\n")))
                    .await;
                impact_recovery::emit_rejection(
                    &tx,
                    "reflection",
                    &msg,
                    &tool_calls,
                    &assistant_text,
                )
                .await;
                pending_system_hint = Some(msg);
                state = AgentState::Recovery;
                reflection_required = Some(reason);
                continue;
            }

            if tool_calls.len() != 1 {
                let msg = provider_turn::tool_count_feedback(
                    governor_contract::reflection_one_tool_message(tool_calls.len()),
                    &tool_calls,
                );
                let _ = tx
                    .send(StreamToken::Delta(format!("\n[reflect] {msg}\n")))
                    .await;
                impact_recovery::emit_rejection(
                    &tx,
                    "reflection",
                    &msg,
                    &tool_calls,
                    &assistant_text,
                )
                .await;
                pending_system_hint = Some(msg);
                state = AgentState::Recovery;
                reflection_required = Some(reason);
                continue;
            }

            let _ = tx
                .send(StreamToken::Delta(format!(
                    "\n[reflect] goal_delta={} strategy={} next={}\n",
                    reflect.goal_delta.as_str(),
                    reflect.strategy_change.as_str(),
                    reflect.next_minimal_action.as_str()
                )))
                .await;

            last_reflection = Some(reflect.clone());
            reflection_guard = Some(reflect);
            reflection_required = None;

            if let Some(ref r) = last_reflection {
                if reflection_ledger.remember(
                    r.wrong_assumption.as_str(),
                    r.next_minimal_action.as_str(),
                    Some(reason.as_str()),
                    r.last_outcome.as_str(),
                    r.goal_delta.as_str(),
                    r.strategy_change.as_str(),
                ) {
                    let remembered_count = reflection_ledger
                        .find_entry(r.wrong_assumption.as_str(), r.next_minimal_action.as_str())
                        .map(|entry| entry.count)
                        .unwrap_or(1);
                    emit_telemetry_event(
                        &tx,
                        "reflection_ledger_remembered",
                        json!({
                            "wrong_assumption": r.wrong_assumption,
                            "next_minimal_action": r.next_minimal_action,
                            "trigger": reason,
                            "count": remembered_count,
                            "entries": reflection_ledger.entries.len(),
                        }),
                    )
                    .await;
                    if let Some(path) = reflection_ledger_path.as_ref() {
                        if let Err(e) = reflection_ledger.save_atomic(path) {
                            if !reflection_ledger_warned {
                                reflection_ledger_warned = true;
                                let _ = tx
                                    .send(StreamToken::Delta(format!(
                                        "\n[reflection_ledger] WARN: {e:#}\n"
                                    )))
                                    .await;
                            }
                        }
                    }
                }
                if !r.wrong_assumption.trim().is_empty() {
                    assumption_ledger.mark_refuted(
                        r.wrong_assumption.as_str(),
                        Some(r.next_minimal_action.as_str()),
                    );
                }
                if matches!(
                    r.strategy_change,
                    StrategyChange::Adjust | StrategyChange::Abandon
                ) {
                    working_mem.set_strategy(r.next_minimal_action.as_str());
                    assumption_ledger.refresh_confirmations(&working_mem);
                }
                if r.strategy_change == StrategyChange::Abandon {
                    pending_system_hint = Some(format!(
                        "Strategy abandoned.\n\
Do not retry the previous approach.\n\
Execute only the new minimal action: {}",
                        r.next_minimal_action.as_str()
                    ));
                }
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

        // A generic passing check cannot discharge a localized failed check.
        if tc.name == "done" && recovery.focus.is_pending() {
            let block = format!("[Done Gate] {}", recovery.focus.hint().unwrap_or_default());
            messages.push(json!({"role":"tool", "tool_call_id":tc.id, "content":format!("GOVERNOR BLOCKED\n{block}")}));
            pending_system_hint = Some(block);
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

        // ── done tool ──────────────────────────────────────────────────────
        if tc.name.as_str() == "done" {
            if realize_cfg.enabled {
                if let Some(latent) = latent_plan.as_ref() {
                    state = AgentState::Recovery;
                    recovery.stage = Some(RecoveryStage::Diagnose);
                    let block = build_realize_done_gate_message(latent);

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
                        None,
                    )
                    .await;
                    continue;
                }
            }

            let last_mutation = last_mutation_step.unwrap_or(0);
            let last_verify_ok = last_verify_ok_step.unwrap_or(0);
            if last_mutation > 0 && last_verify_ok < last_mutation {
                state = AgentState::Recovery;
                recovery.stage = Some(RecoveryStage::Verify);
                let block = format!(
                    "[Done Gate] Verification required before `done`.\n\
Last mutation step: {last_mutation}\n\
Last verification step: {last_verify_ok}\n\
Required now: {}",
                    verification_requirement_hint(required_verification, test_cmd.as_deref())
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
                    None,
                )
                .await;
                continue;
            }

            if let Some(command) = benchmark_plan_pending_required_exec_command(
                &root_user_text,
                &messages,
                test_cmd.as_deref(),
            ) {
                state = AgentState::Recovery;
                recovery.stage = Some(RecoveryStage::Verify);
                let verification_status = task_harness::benchmark_plan_verification_hint(
                    &root_user_text,
                    &messages,
                    test_cmd.as_deref(),
                )
                .unwrap_or_default();
                let block = format!(
                    "[Done Gate] Approved benchmark plan requires explicit verification proof before `done`.\n\
Required now: run `{command}`.\n{verification_status}"
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
                    None,
                )
                .await;
                continue;
            }

            let args: serde_json::Value = serde_json::from_str(&tc.arguments)?;
            let report =
                final_handoff::report_context(&root_user_text, &messages, test_cmd.as_deref());
            let summary = report_surface::authored(args["summary"].as_str().unwrap_or(""), &report);
            let completed_acceptance = parse_string_list_arg(&args["completed_acceptance"]);
            let remaining_acceptance = parse_string_list_arg(&args["remaining_acceptance"]);
            let acceptance_evidence =
                acceptance_report::parse_done_acceptance_evidence(&args["acceptance_evidence"]);
            let next_steps = args["next_steps"].as_str().unwrap_or("").trim();
            let done_plan = validated_plan_for_turn.as_ref().or(active_plan.as_ref());
            let known_acceptance_commands =
                acceptance_report::current_commands(&messages, test_cmd.as_deref());
            let read_only_scores = if root_read_only {
                done_plan
                    .map(|plan| {
                        build_read_only_evidence_scores(
                            &root_user_text,
                            plan,
                            &observation_evidence,
                        )
                    })
                    .unwrap_or_default()
            } else {
                Vec::new()
            };

            let evidence_rows = match validate_done_acceptance(
                done_plan,
                &completed_acceptance,
                &remaining_acceptance,
                &acceptance_evidence,
                &known_acceptance_commands,
                &observation_evidence,
            ) {
                Ok(rows) => rows,
                Err(e) => {
                    state = AgentState::Recovery;
                    recovery.stage = Some(if root_read_only {
                        RecoveryStage::Diagnose
                    } else {
                        RecoveryStage::Verify
                    });
                    let hint = build_done_acceptance_recovery_hint(
                        &e.to_string(),
                        done_plan,
                        &known_acceptance_commands,
                        &read_only_scores,
                        cfg.base_url.contains("generativelanguage.googleapis.com"),
                    );
                    let block = format!(
                        "{}{}{}",
                        governor_contract::done_invalid_acceptance_message(&e.to_string()),
                        hint,
                        acceptance_report::hint(done_plan, &known_acceptance_commands)
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
                        None,
                    )
                    .await;
                    continue;
                }
            };

            let summary_text = if summary.is_empty() {
                synthesize_action_done_summary(done_plan, &messages).unwrap_or_default()
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
                let idx = resolve_acceptance_reference(item, done_plan)
                    .expect("completed acceptance validated");
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
                let idx = resolve_acceptance_reference(item, done_plan)
                    .expect("remaining acceptance validated");
                final_text.push_str("- remaining: ");
                final_text.push_str(acceptance_reference_label(done_plan, idx).as_str());
                final_text.push('\n');
            }
            if !next_steps.is_empty() {
                final_text.push_str("\n\nNext:\n");
                final_text.push_str(next_steps);
            }

            let exact_content_error = exact_content::validate_exact_content_task(
                &root_user_text,
                tool_root_abs.as_deref(),
            )
            .err();
            let needs_artifact_repair = exact_content_error.is_some();
            let requirement_error = remaining_acceptance
                .is_empty()
                .then(|| requirement_proof::issue(&messages, test_cmd.as_deref()))
                .flatten();
            let handoff_error = exact_content_error.or(requirement_error).or_else(|| {
                if root_read_only {
                    None
                } else {
                    validate_authored_done_summary(
                        &root_user_text,
                        summary,
                        &final_text,
                        &messages,
                        test_cmd.as_deref(),
                    )
                    .err()
                }
            });
            if let Some(block) = handoff_error {
                state = AgentState::Recovery;
                if needs_artifact_repair {
                    // A line-oriented test cannot override a failed exact-byte contract.
                    last_build_verify_ok_step = None;
                    last_behavioral_verify_ok_step = None;
                    last_verify_ok_step = None;
                    recovery.stage = Some(RecoveryStage::Fix);
                }
                messages.push(json!({
                    "role": "tool",
                    "tool_call_id": tc.id,
                    "content": format!("GOVERNOR BLOCKED\n\n{block}"),
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
                    .send(StreamToken::Delta(format!(
                        "[RESULT][Recovery] GOVERNOR BLOCK\n{block}\n"
                    )))
                    .await;
                pending_system_hint = Some(block);
                continue;
            }

            task_outcome.accept_done(&remaining_acceptance);

            // Close out the tool call so session JSON remains valid on resume.
            messages.push(json!({
                "role": "tool",
                "tool_call_id": tc.id,
                "content": "OK: done"
            }));
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
                .send(StreamToken::Delta(format!("\n\n{final_text}\n")))
                .await;
            break;
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
                None,
            )
            .await;
            continue;
        }

        if tc.name.as_str() == "apply_diff" {
            let args: serde_json::Value = serde_json::from_str(&tc.arguments)?;
            let path = args["path"].as_str().unwrap_or("").to_string();
            let diff = args["diff"].as_str().unwrap_or("").to_string();

            let _ = tx
                .send(StreamToken::Delta(format!("\n\n[APPLY_DIFF] {path}\n")))
                .await;
            let _ = tx.send(StreamToken::ToolCall(tc.clone())).await;

            let base = tool_root_abs.as_deref();
            let mut rejected_by_user = false;

            // B — capture old content for diff anchoring
            let old_for_cache = base.and_then(|b| {
                crate::file_tools::resolve_safe_path(&path, Some(b))
                    .ok()
                    .and_then(|abs| std::fs::read_to_string(&abs).ok())
            });

            let preview = truncate_preview(&diff, 2800, 140);
            let approval = approver
                .approve(ApprovalRequest::Edit {
                    action: "apply_diff".to_string(),
                    path: path.clone(),
                    preview,
                })
                .await?;
            let mut cargo_inputs = None;
            let (mut result, is_error) = if approval == ApprovalOutcome::Approved {
                crate::file_tools::tool_apply_diff(&path, &diff, base)
            } else {
                rejected_by_user = true;
                (
                    format!(
                        "REJECTED BY USER\naction: apply_diff\npath: {path}\n(no changes applied)"
                    ),
                    true,
                )
            };

            // Invalidate file cache on success + auto-test
            let changed = crate::execution_evidence::file_edit_succeeded("apply_diff", &result);
            if changed {
                let cache_key = crate::file_tools::resolve_safe_path(&path, base)
                    .ok()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.clone());
                file_cache.remove(&cache_key);
                let before_hash = old_for_cache.as_deref().map(hash_text);
                // Seed cache with new content + hash verification
                if let Ok(abs) = crate::file_tools::resolve_safe_path(&path, base) {
                    if let Ok(new_content) = std::fs::read_to_string(&abs) {
                        let after_hash = hash_text(&new_content);
                        match before_hash {
                            Some(bh) => result.push_str(&format!(
                                "\n[hash] before={} after={}",
                                fmt_hash(bh),
                                fmt_hash(after_hash)
                            )),
                            None => {
                                result.push_str(&format!("\n[hash] after={}", fmt_hash(after_hash)))
                            }
                        }
                        file_cache.insert(cache_key, new_content);
                    }
                }
                // A — auto-run tests
                if let Some(ref cmd) = test_cmd {
                    if let Some(ref root) = tool_root_abs {
                        let test_out =
                            automatic_test::observed(cmd, root, &messages, &mut cargo_inputs).await;
                        result.push_str(&test_out);
                    }
                }
            }

            recovery.focus.record_result(
                &tc,
                &result,
                tool_root_abs.as_deref(),
                test_cmd.as_deref(),
                &exec_verification_context,
            );
            let automatic_test =
                crate::execution_evidence::auto_test_outcome("apply_diff", &result);
            let verified =
                !is_error && automatic_test == crate::execution_evidence::AutoTestOutcome::Passed;
            let verified_level = if verified {
                configured_test_cmd_verification_level(test_cmd.as_deref())
            } else {
                None
            };
            if is_error {
                recovery.on_fix_result(false, None);
            } else if changed {
                exec_proof::record_file_result(
                    exec_proof::FileProofResult {
                        name: "apply_diff",
                        content: &result,
                        test_cmd: test_cmd.as_deref(),
                    },
                    this_step,
                    &mut last_mutation_step,
                    &mut last_build_verify_ok_step,
                    &mut last_behavioral_verify_ok_step,
                );
                last_verify_ok_step = effective_verify_ok_step(
                    required_verification,
                    last_build_verify_ok_step,
                    last_behavioral_verify_ok_step,
                );
                mem.on_file_result(&tc.name, &result, test_cmd.as_deref());
                recovery.on_successful_edit(automatic_test, verified_level);
            }

            edit_failures.on_result(&tc, &result, &exec_verification_context);
            file_tool_consec_failures = edit_failures.count();
            let first_line = result.lines().next().unwrap_or("").to_string();
            if is_error {
                let _ = tx
                    .send(StreamToken::Delta(format!(
                        "[RESULT_FILE_ERR] {first_line}\n"
                    )))
                    .await;
                state = AgentState::Recovery;
                if !rejected_by_user {
                    pending_system_hint = Some(format!("apply_diff error: {first_line}"));
                    if let Some(reason) = edit_failures.reflection_reason() {
                        reflection_required = Some(reason);
                    }
                } else {
                    pending_system_hint = Some(
                        "The user rejected the edit. Choose a safer alternative or ask again with a smaller change."
                            .to_string(),
                    );
                }
            } else {
                let _ = tx
                    .send(StreamToken::Delta(format!("[RESULT_FILE] {first_line}\n")))
                    .await;
                state = if recovery.in_recovery() {
                    AgentState::Recovery
                } else {
                    AgentState::Planning
                };
                pending_system_hint = if automatic_test
                    == crate::execution_evidence::AutoTestOutcome::Failed
                {
                    Some(recovery::AUTO_TEST_FAILURE_HINT.to_string())
                } else if recovery.stage == Some(RecoveryStage::Verify) {
                    Some(format!(
                        "Recovery stage=verify: {}",
                        verification_requirement_hint(required_verification, test_cmd.as_deref())
                    ))
                } else {
                    None
                };
                if changed {
                    update_working_memory_after_non_exec(
                        &mut working_mem,
                        "apply_diff",
                        verified,
                        step_label_for_turn.as_deref(),
                        next_action_for_turn,
                        test_cmd.as_deref(),
                    );
                    assumption_ledger.refresh_confirmations(&working_mem);
                    let path_level = verification_level_for_mutation_path(path.as_str());
                    if path_level > path_required_verification {
                        path_required_verification = path_level;
                        required_verification =
                            intent_required_verification.max(path_required_verification);
                        recovery.required_verification = required_verification;
                        last_verify_ok_step = effective_verify_ok_step(
                            required_verification,
                            last_build_verify_ok_step,
                            last_behavioral_verify_ok_step,
                        );
                        if recovery.stage == Some(RecoveryStage::Verify) {
                            pending_system_hint = Some(format!(
                                "Recovery stage=verify: {}",
                                verification_requirement_hint(
                                    required_verification,
                                    test_cmd.as_deref()
                                )
                            ));
                        }
                    }
                    impact_required = Some(format!(
                        "successful mutation via apply_diff: {}",
                        compact_one_line(path.as_str(), 80)
                    ));
                }
            }

            if is_error {
                reflection_required = Some(
                    pending_system_hint
                        .clone()
                        .unwrap_or_else(|| "apply_diff failed".to_string()),
                );
                reflection_trigger_sig = tool_call_action_sig(&tc);
            }

            let history_result = if is_error {
                result.clone()
            } else {
                compact_success_tool_result_for_history("apply_diff", &result)
            };
            messages.push(CargoInputReceipt::attach(
                cargo_inputs,
                recovery.focus.tool_message(&tc, history_result),
            ));
            if changed && !root_read_only {
                if let Some(hint) = build_fix_stage_progress_hint(
                    task_harness,
                    &messages,
                    recovery.stage,
                    test_cmd.as_deref(),
                ) {
                    pending_system_hint = Some(hint);
                }
            }
            if !is_error && root_read_only {
                if let Some(plan) = active_plan.as_ref() {
                    if iter + 1 == max_iters {
                        if let Some(final_text) = build_read_only_iteration_cap_final_answer(
                            &root_user_text,
                            plan,
                            &observation_evidence,
                            &messages,
                            &working_mem,
                        ) {
                            task_outcome.finalized(&final_text);
                            state = AgentState::Done;
                            messages
                                .push(json!({"role": "assistant", "content": final_text.clone()}));
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
                                    "\n[agent] iteration cap reached; auto-finalized read-only inspection.\n\n{final_text}\n"
                                )))
                                .await;
                            break;
                        }
                    } else if let Some(hint) = build_read_only_completion_hint(
                        &root_user_text,
                        plan,
                        &observation_evidence,
                        &messages,
                        &working_mem,
                    ) {
                        pending_system_hint = Some(hint);
                    }
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

        // ── glob tool ─────────────────────────────────────────────────────
        // ── list_dir tool ────────────────────────────────────────────────────
        if tc.name.as_str() == "list_dir" {
            let args: serde_json::Value = serde_json::from_str(&tc.arguments)?;
            let dir = args["dir"].as_str().unwrap_or("").to_string();
            let max_entries = args["max_entries"].as_u64().unwrap_or(200) as usize;
            let include_hidden = args["include_hidden"].as_bool().unwrap_or(false);

            let dir_label = if dir.trim().is_empty() {
                "."
            } else {
                dir.as_str()
            };
            let _ = tx
                .send(StreamToken::Delta(format!("\n\n[LIST_DIR] {dir_label}\n")))
                .await;
            let _ = tx.send(StreamToken::ToolCall(tc.clone())).await;

            let base = tool_root_abs.as_deref();
            let (mut result, is_error) =
                crate::file_tools::tool_list_dir(&dir, max_entries, include_hidden, base);
            let mut repo_map_hint: Option<String> = None;
            let mut repo_map_display: Option<String> = None;
            if is_error && !dir.trim().is_empty() {
                if let Some(root) = tool_root_abs.as_deref() {
                    if let Some(fallback) = crate::repo_map::lazy_list_dir_fallback(root, &dir) {
                        emit_repo_map_fallback_telemetry(&tx, "list_dir", &dir, &fallback).await;
                        remember_repo_map_resolution(
                            &mut observation_evidence,
                            dir.as_str(),
                            &fallback,
                            "repo_map:list_dir",
                        );
                        sync_observation_cache_autosave(&autosaver, &observation_evidence);
                        result.push_str("\n");
                        result.push_str(&fallback.content);
                        repo_map_display =
                            Some(build_repo_map_display_banner("list_dir", &fallback));
                        repo_map_hint = Some(build_repo_map_list_dir_hint(&dir, &fallback));
                    }
                }
            }
            recovery.on_diagnostic_result(!is_error);

            let first_line = result.lines().next().unwrap_or("").to_string();
            if is_error {
                let _ = tx
                    .send(StreamToken::Delta(format!(
                        "[RESULT_FILE_ERR] {first_line}\n"
                    )))
                    .await;
                if let Some(display) = repo_map_display {
                    let _ = tx.send(StreamToken::Delta(display)).await;
                }
                pending_system_hint = repo_map_hint;
                state = AgentState::Recovery;
            } else {
                let _ = tx
                    .send(StreamToken::Delta(format!(
                        "[RESULT_LIST_DIR] {first_line}\n"
                    )))
                    .await;
                state = if recovery.in_recovery() {
                    AgentState::Recovery
                } else {
                    AgentState::Planning
                };
                pending_system_hint = if recovery.stage == Some(RecoveryStage::Fix) {
                    Some("Recovery stage=fix: apply a minimal fix now (edit files or run a corrected command).".to_string())
                } else if recovery.stage == Some(RecoveryStage::Verify) {
                    Some(format!(
                        "Recovery stage=verify: {}",
                        verification_requirement_hint(required_verification, test_cmd.as_deref())
                    ))
                } else {
                    None
                };
                update_working_memory_after_non_exec(
                    &mut working_mem,
                    "list_dir",
                    false,
                    step_label_for_turn.as_deref(),
                    next_action_for_turn,
                    test_cmd.as_deref(),
                );
                assumption_ledger.refresh_confirmations(&working_mem);
            }

            let history_result = if is_error {
                result.clone()
            } else {
                compact_success_tool_result_for_history("list_dir", &result)
            };
            messages.push(json!({
                "role": "tool",
                "tool_call_id": tc.id,
                "content": history_result,
            }));
            if !is_error && !root_read_only {
                if let Some(hint) = build_fix_stage_progress_hint(
                    task_harness,
                    &messages,
                    recovery.stage,
                    test_cmd.as_deref(),
                ) {
                    pending_system_hint = Some(hint);
                }
            }
            if !is_error && root_read_only {
                if let Some(plan) = active_plan.as_ref() {
                    if iter + 1 == max_iters {
                        if let Some(final_text) = build_read_only_iteration_cap_final_answer(
                            &root_user_text,
                            plan,
                            &observation_evidence,
                            &messages,
                            &working_mem,
                        ) {
                            task_outcome.finalized(&final_text);
                            state = AgentState::Done;
                            messages
                                .push(json!({"role": "assistant", "content": final_text.clone()}));
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
                                    "\n[agent] iteration cap reached; auto-finalized read-only inspection.\n\n{final_text}\n"
                                )))
                                .await;
                            break;
                        }
                    } else if let Some(hint) = build_read_only_completion_hint(
                        &root_user_text,
                        plan,
                        &observation_evidence,
                        &messages,
                        &working_mem,
                    ) {
                        pending_system_hint = Some(hint);
                    }
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

        if tc.name.as_str() == "glob" {
            let args: serde_json::Value = serde_json::from_str(&tc.arguments)?;
            let pattern = args["pattern"].as_str().unwrap_or("").to_string();
            let dir = args["dir"].as_str().unwrap_or("").to_string();

            let _ = tx
                .send(StreamToken::Delta(format!("\n\n[GLOB] {pattern}\n")))
                .await;
            let _ = tx.send(StreamToken::ToolCall(tc.clone())).await;

            let base = tool_root_abs.as_deref();
            let (mut result, is_error) = crate::file_tools::tool_glob_files(&pattern, &dir, base);
            let mut repo_map_hint: Option<String> = None;
            let mut repo_map_display: Option<String> = None;
            if !is_error && result.starts_with("[glob] No files matching") {
                if let Some(root) = tool_root_abs.as_deref() {
                    if let Some(fallback) = crate::repo_map::lazy_glob_fallback(root, &pattern) {
                        emit_repo_map_fallback_telemetry(&tx, "glob", &pattern, &fallback).await;
                        remember_repo_map_resolution(
                            &mut observation_evidence,
                            pattern.as_str(),
                            &fallback,
                            "repo_map:glob",
                        );
                        sync_observation_cache_autosave(&autosaver, &observation_evidence);
                        result.push_str("\n");
                        result.push_str(&fallback.content);
                        repo_map_display = Some(build_repo_map_display_banner("glob", &fallback));
                        repo_map_hint = Some(build_repo_map_glob_hint(&pattern, &fallback));
                    }
                }
            }
            recovery.on_diagnostic_result(!is_error);

            let first_line = result.lines().next().unwrap_or("").to_string();
            if is_error {
                let _ = tx
                    .send(StreamToken::Delta(format!(
                        "[RESULT_FILE_ERR] {first_line}\n"
                    )))
                    .await;
                state = AgentState::Recovery;
            } else {
                let _ = tx
                    .send(StreamToken::Delta(format!("[RESULT_GLOB] {first_line}\n")))
                    .await;
                if let Some(display) = repo_map_display {
                    let _ = tx.send(StreamToken::Delta(display)).await;
                }
                state = if recovery.in_recovery() {
                    AgentState::Recovery
                } else {
                    AgentState::Planning
                };
                pending_system_hint = if recovery.stage == Some(RecoveryStage::Fix) {
                    Some("Recovery stage=fix: apply a minimal fix now (edit files or run a corrected command).".to_string())
                } else if recovery.stage == Some(RecoveryStage::Verify) {
                    Some(format!(
                        "Recovery stage=verify: {}",
                        verification_requirement_hint(required_verification, test_cmd.as_deref())
                    ))
                } else if repo_map_hint.is_some() {
                    repo_map_hint
                } else {
                    None
                };
                update_working_memory_after_non_exec(
                    &mut working_mem,
                    "glob",
                    false,
                    step_label_for_turn.as_deref(),
                    next_action_for_turn,
                    test_cmd.as_deref(),
                );
                assumption_ledger.refresh_confirmations(&working_mem);
            }

            let history_result = if is_error {
                result.clone()
            } else {
                compact_success_tool_result_for_history("glob", &result)
            };
            messages.push(json!({
                "role": "tool",
                "tool_call_id": tc.id,
                "content": history_result,
            }));
            if !is_error && !root_read_only {
                if let Some(hint) = build_fix_stage_progress_hint(
                    task_harness,
                    &messages,
                    recovery.stage,
                    test_cmd.as_deref(),
                ) {
                    pending_system_hint = Some(hint);
                }
            }
            if !is_error && root_read_only {
                if let Some(plan) = active_plan.as_ref() {
                    if iter + 1 == max_iters {
                        if let Some(final_text) = build_read_only_iteration_cap_final_answer(
                            &root_user_text,
                            plan,
                            &observation_evidence,
                            &messages,
                            &working_mem,
                        ) {
                            task_outcome.finalized(&final_text);
                            state = AgentState::Done;
                            messages
                                .push(json!({"role": "assistant", "content": final_text.clone()}));
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
                                    "\n[agent] iteration cap reached; auto-finalized read-only inspection.\n\n{final_text}\n"
                                )))
                                .await;
                            break;
                        }
                    } else if let Some(hint) = build_read_only_completion_hint(
                        &root_user_text,
                        plan,
                        &observation_evidence,
                        &messages,
                        &working_mem,
                    ) {
                        pending_system_hint = Some(hint);
                    }
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

        // ── search_files tool ─────────────────────────────────────────────
        if tc.name.as_str() == "search_files" {
            let args: serde_json::Value = serde_json::from_str(&tc.arguments)?;
            let pattern = args["pattern"].as_str().unwrap_or("").to_string();
            let dir = args["dir"].as_str().unwrap_or("").to_string();
            let ci = args["case_insensitive"].as_bool().unwrap_or(false);

            let _ = tx
                .send(StreamToken::Delta(format!(
                    "\n\n[SEARCH_FILES] {pattern}\n"
                )))
                .await;
            let _ = tx.send(StreamToken::ToolCall(tc.clone())).await;

            let base = tool_root_abs.as_deref();
            let (mut result, is_error) =
                crate::file_tools::tool_search_files(&pattern, &dir, ci, base);
            let parsed_search_paths = if is_error {
                Vec::new()
            } else {
                parse_search_result_paths(result.as_str())
            };
            let mut repo_map_hint: Option<String> = None;
            let mut repo_map_display: Option<String> = None;
            if !is_error
                && result.starts_with("[search_files] No matches for")
                && dir.trim().is_empty()
            {
                if let Some(root) = tool_root_abs.as_deref() {
                    if let Some(fallback) = crate::repo_map::lazy_search_fallback(root, &pattern) {
                        emit_repo_map_fallback_telemetry(&tx, "search_files", &pattern, &fallback)
                            .await;
                        result.push_str("\n");
                        result.push_str(&fallback.content);
                        repo_map_display =
                            Some(build_repo_map_display_banner("search_files", &fallback));
                        repo_map_hint = Some(build_repo_map_search_hint(&pattern, &fallback));
                    }
                }
            }
            recovery.on_diagnostic_result(!is_error);

            let first_line = result.lines().next().unwrap_or("").to_string();
            if is_error {
                let _ = tx
                    .send(StreamToken::Delta(format!(
                        "[RESULT_FILE_ERR] {first_line}\n"
                    )))
                    .await;
                state = AgentState::Recovery;
            } else {
                let _ = tx
                    .send(StreamToken::Delta(format!(
                        "[RESULT_SEARCH] {first_line}\n"
                    )))
                    .await;
                if let Some(display) = repo_map_display {
                    let _ = tx.send(StreamToken::Delta(display)).await;
                }
                state = if recovery.in_recovery() {
                    AgentState::Recovery
                } else {
                    AgentState::Planning
                };
                pending_system_hint = if recovery.stage == Some(RecoveryStage::Fix) {
                    Some("Recovery stage=fix: apply a minimal fix now (edit files or run a corrected command).".to_string())
                } else if recovery.stage == Some(RecoveryStage::Verify) {
                    Some(format!(
                        "Recovery stage=verify: {}",
                        verification_requirement_hint(required_verification, test_cmd.as_deref())
                    ))
                } else if repo_map_hint.is_some() {
                    repo_map_hint
                } else {
                    None
                };
                update_working_memory_after_non_exec(
                    &mut working_mem,
                    "search_files",
                    false,
                    step_label_for_turn.as_deref(),
                    next_action_for_turn,
                    test_cmd.as_deref(),
                );
                let command = canonicalize_tool_call_command("search_files", tc.arguments.as_str())
                    .unwrap_or_else(|| format!("search_files(pattern={pattern}, dir={dir})"));
                observation_evidence.remember_search(
                    command.as_str(),
                    pattern.as_str(),
                    parse_search_hit_count(result.as_str()),
                    parsed_search_paths.as_slice(),
                );
                sync_observation_cache_autosave(&autosaver, &observation_evidence);
                assumption_ledger.refresh_confirmations(&working_mem);
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
            if !is_error && root_read_only {
                if let Some(plan) = active_plan.as_ref() {
                    if iter + 1 == max_iters {
                        if let Some(final_text) = build_read_only_iteration_cap_final_answer(
                            &root_user_text,
                            plan,
                            &observation_evidence,
                            &messages,
                            &working_mem,
                        ) {
                            task_outcome.finalized(&final_text);
                            state = AgentState::Done;
                            messages
                                .push(json!({"role": "assistant", "content": final_text.clone()}));
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
                                    "\n[agent] iteration cap reached; auto-finalized read-only inspection.\n\n{final_text}\n"
                                )))
                                .await;
                            break;
                        }
                    } else if let Some(hint) = build_read_only_search_to_read_hint(
                        &root_user_text,
                        plan,
                        parsed_search_paths.as_slice(),
                        &observation_evidence,
                    ) {
                        pending_system_hint = Some(hint);
                    } else if let Some(hint) = build_read_only_completion_hint(
                        &root_user_text,
                        plan,
                        &observation_evidence,
                        &messages,
                        &working_mem,
                    ) {
                        pending_system_hint = Some(hint);
                    }
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

            continue;
        }

        // ── File tools: read_file / write_file / patch_file ────────────────
        if matches!(tc.name.as_str(), "read_file" | "write_file" | "patch_file") {
            let args: serde_json::Value = serde_json::from_str(&tc.arguments)?;
            let path = args["path"].as_str().unwrap_or("").to_string();

            // Emit annotation (visible in TUI).
            let tool_upper = tc.name.to_ascii_uppercase();
            let _ = tx
                .send(StreamToken::Delta(format!("\n\n[{tool_upper}] {path}\n")))
                .await;
            let _ = tx.send(StreamToken::ToolCall(tc.clone())).await;

            let base = tool_root_abs.as_deref();
            let mut repo_map_hint: Option<String> = None;
            let mut repo_map_display: Option<String> = None;

            // Cache key: canonical absolute path string.
            let cache_key = crate::file_tools::resolve_safe_path(&path, base)
                .ok()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_else(|| format!("{}|{}", path, base.unwrap_or("")));
            let mut rejected_by_user = false;

            let mut cargo_inputs = None;
            let (result, is_error) = match tc.name.as_str() {
                "read_file" => {
                    let ranged = args.get("start_line").is_some() || args.get("end_line").is_some();
                    if assumption_gate::needs_fresh_file_read(&assumption_ledger, &path)
                        || (!ranged
                            && file_cache.contains_key(&cache_key)
                            && deferred_edit::confirming_read(&messages, &tc.id, &path))
                    {
                        file_cache.remove(&cache_key);
                    }
                    // ── Gap 6: serve from cache if file hasn't changed ──────
                    if ranged {
                        let (content, err) =
                            crate::file_tools::read_range::execute_args(&args, base);
                        if !err && crate::file_tools::read_range::full_snapshot(&content) {
                            file_cache.insert(cache_key.clone(), content.clone());
                        }
                        (content, err)
                    } else if let Some(result) =
                        recovery
                            .focus
                            .read_for_diagnosis(&tc, base, &mut file_cache)
                    {
                        result
                    } else if let Some(cached) = file_cache.get(&cache_key).and_then(|cached| {
                        crate::file_tools::read_cache::for_request(cached, &path, base)
                    }) {
                        let header = cached.lines().next().unwrap_or(&path).to_string();
                        let _ = tx
                            .send(StreamToken::Delta(format!("[CACHE_HIT] {header}\n")))
                            .await;
                        (cached, false)
                    } else {
                        let (mut content, err) = crate::file_tools::tool_read_file(&path, base);
                        if err {
                            if let Some(root) = tool_root_abs.as_deref() {
                                if let Some(fallback) =
                                    crate::repo_map::lazy_read_fallback(root, &path)
                                {
                                    emit_repo_map_fallback_telemetry(
                                        &tx,
                                        "read_file",
                                        &path,
                                        &fallback,
                                    )
                                    .await;
                                    remember_repo_map_resolution(
                                        &mut observation_evidence,
                                        path.as_str(),
                                        &fallback,
                                        "repo_map:read_file",
                                    );
                                    sync_observation_cache_autosave(
                                        &autosaver,
                                        &observation_evidence,
                                    );
                                    content.push_str("\n");
                                    content.push_str(&fallback.content);
                                    repo_map_display =
                                        Some(build_repo_map_display_banner("read_file", &fallback));
                                    repo_map_hint =
                                        Some(build_repo_map_read_hint(&path, &fallback));
                                }
                            }
                        }
                        if !err && crate::file_tools::read_range::full_snapshot(&content) {
                            file_cache.insert(cache_key.clone(), content.clone());
                        }
                        (content, err)
                    }
                }
                "write_file" => {
                    let content = args["content"].as_str().unwrap_or("").to_string();

                    // Safety: do not overwrite an existing file unless we already read it in this session.
                    // This prevents accidental destructive writes when the agent guessed the path/content.
                    let file_exists = crate::file_tools::resolve_safe_path(&path, base)
                        .ok()
                        .and_then(|abs| abs.metadata().ok())
                        .map(|m| m.is_file())
                        .unwrap_or(false);
                    if file_exists && !file_cache.contains_key(&cache_key) {
                        if let Some(snapshot) =
                            read_context::matching_snapshot(&messages, &path, base)
                        {
                            file_cache.insert(cache_key.clone(), snapshot);
                        }
                    }
                    if file_exists && !file_cache.contains_key(&cache_key) {
                        (
                            format!(
                                "GOVERNOR BLOCKED: write_file refused because '{path}' already exists.\n\
Action required: obtain a complete read of current contents before write_file. For a range or truncated read, use patch_file/apply_diff to preserve unseen content."
                            ),
                            true,
                        )
                    } else {
                        // Approval: show a compact before/after preview.
                        let old = file_cache
                            .get(&cache_key)
                            .cloned()
                            .or_else(|| {
                                crate::file_tools::resolve_safe_path(&path, base)
                                    .ok()
                                    .and_then(|abs| std::fs::read_to_string(&abs).ok())
                            })
                            .unwrap_or_default();
                        let preview = simple_before_after(&old, &content);
                        let before_hash = hash_text(&old);
                        let after_hash = hash_text(&content);
                        let approval = approver
                            .approve(ApprovalRequest::Edit {
                                action: "write_file".to_string(),
                                path: path.clone(),
                                preview,
                            })
                            .await?;
                        if approval == ApprovalOutcome::Rejected {
                            rejected_by_user = true;
                            (
                            format!(
                                "REJECTED BY USER\naction: write_file\npath: {path}\n(no changes applied)"
                            ),
                            true,
                        )
                        } else {
                            let (mut r_text, r_err) = crate::cargo_inputs::write_file(
                                &path,
                                &content,
                                base,
                                &mut messages,
                            );
                            if !r_err
                                && crate::execution_evidence::file_edit_succeeded(
                                    "write_file",
                                    &r_text,
                                )
                            {
                                file_cache.insert(cache_key.clone(), content.clone());
                                r_text.push_str(&format!(
                                    "\n[hash] before={} after={}",
                                    fmt_hash(before_hash),
                                    fmt_hash(after_hash)
                                ));
                                // A — auto-test after write
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
                            (r_text, r_err)
                        }
                    }
                }
                _ => {
                    // ── patch_file ─────────────────────────────────────────
                    match crate::file_tools::json_patch::text_edit(&path, &args, base) {
                        Err(error) => (format!("ERROR: {error}"), true),
                        Ok((search, replace)) => {
                            if args.get("json_pointer").is_some() {
                                file_cache.remove(&cache_key);
                            }
                            // B — capture old content for diff preview before patching.
                            let old_content_for_diff =
                                file_cache.get(&cache_key).cloned().or_else(|| {
                                    crate::file_tools::resolve_safe_path(&path, base)
                                        .ok()
                                        .and_then(|abs| std::fs::read_to_string(&abs).ok())
                                });
                            let before_hash = old_content_for_diff
                                .as_deref()
                                .map(hash_text)
                                .unwrap_or_else(|| hash_text(""));

                            // Approval: show the computed patch diff (or search/replace when diff can't be made).
                            let mut preview = old_content_for_diff
                                .as_deref()
                                .map(|old| {
                                    crate::file_tools::make_patch_diff(old, &search, &replace)
                                })
                                .unwrap_or_default();
                            if preview.trim().is_empty() {
                                let s = truncate_preview(&search, 700, 28);
                                let r = truncate_preview(&replace, 700, 28);
                                preview = format!(
                            "(no context diff available)\n--- search ---\n{s}\n\n--- replace ---\n{r}\n"
                        );
                            }
                            let approval = approver
                                .approve(ApprovalRequest::Edit {
                                    action: "patch_file".to_string(),
                                    path: path.clone(),
                                    preview,
                                })
                                .await?;
                            if approval == ApprovalOutcome::Rejected {
                                rejected_by_user = true;
                                (
                            format!(
                                "REJECTED BY USER\naction: patch_file\npath: {path}\n(no changes applied)"
                            ),
                            true,
                        )
                            } else {
                                let (mut patch_result, patch_err) =
                                    crate::file_tools::tool_patch_file(
                                        &path, &search, &replace, base,
                                    );

                                if !patch_err
                                    && crate::execution_evidence::file_edit_succeeded(
                                        "patch_file",
                                        &patch_result,
                                    )
                                {
                                    file_cache.remove(&cache_key); // invalidate stale cache

                                    // B — append diff preview (shows exactly what changed).
                                    if let Some(ref old) = old_content_for_diff {
                                        let diff = crate::file_tools::make_patch_diff(
                                            old, &search, &replace,
                                        );
                                        if !diff.is_empty() {
                                            patch_result.push_str(&format!("\n{diff}"));
                                        }
                                    }

                                    // Gap 7: auto-verify patch was applied correctly.
                                    if let Ok(abs) =
                                        crate::file_tools::resolve_safe_path(&path, base)
                                    {
                                        if let Ok(new_content) = std::fs::read_to_string(&abs) {
                                            let after_hash = hash_text(&new_content);
                                            patch_result.push_str(&format!(
                                                "\n[hash] before={} after={}",
                                                fmt_hash(before_hash),
                                                fmt_hash(after_hash)
                                            ));
                                            if replace.is_empty() || new_content.contains(&replace)
                                            {
                                                patch_result.push_str(
                                                    "\n✓ auto-verify: patch confirmed in file",
                                                );
                                            } else {
                                                patch_result.push_str(
                                            "\n✗ auto-verify FAILED: replacement text not found — \
                                             file may be in unexpected state; call read_file to inspect",
                                        );
                                            }
                                            // Seed cache with the freshly written content.
                                            file_cache.insert(cache_key.clone(), new_content);
                                        }
                                    }
                                    // A — auto-test after successful patch
                                    if let Some(ref cmd) = test_cmd {
                                        if let Some(ref root) = tool_root_abs {
                                            patch_result.push_str(
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
                                (patch_result, patch_err)
                            }
                        }
                    }
                }
            };

            let changed = crate::execution_evidence::file_edit_succeeded(&tc.name, &result);
            edit_failures.on_result(&tc, &result, &exec_verification_context);
            file_tool_consec_failures = edit_failures.count();
            if is_error && !rejected_by_user && tc.name != "read_file" {
                if let Some(reason) = edit_failures.reflection_reason() {
                    reflection_required = Some(reason);
                }
            }

            recovery.focus.record_result(
                &tc,
                &result,
                tool_root_abs.as_deref(),
                test_cmd.as_deref(),
                &exec_verification_context,
            );
            let automatic_test =
                crate::execution_evidence::auto_test_outcome(tc.name.as_str(), &result);
            let verified =
                !is_error && automatic_test == crate::execution_evidence::AutoTestOutcome::Passed;
            let verified_level = if verified {
                configured_test_cmd_verification_level(test_cmd.as_deref())
            } else {
                None
            };
            match tc.name.as_str() {
                "read_file" => recovery.on_diagnostic_result(!is_error),
                "write_file" | "patch_file" => {
                    if is_error {
                        recovery.on_fix_result(false, None);
                    } else if changed {
                        exec_proof::record_file_result(
                            exec_proof::FileProofResult {
                                name: tc.name.as_str(),
                                content: &result,
                                test_cmd: test_cmd.as_deref(),
                            },
                            this_step,
                            &mut last_mutation_step,
                            &mut last_build_verify_ok_step,
                            &mut last_behavioral_verify_ok_step,
                        );
                        last_verify_ok_step = effective_verify_ok_step(
                            required_verification,
                            last_build_verify_ok_step,
                            last_behavioral_verify_ok_step,
                        );
                        mem.on_file_result(&tc.name, &result, test_cmd.as_deref());
                        recovery.on_successful_edit(automatic_test, verified_level);
                    }
                }
                _ => {}
            }

            // Emit result label.
            let first_line = result.lines().next().unwrap_or("").to_string();
            if is_error {
                let _ = tx
                    .send(StreamToken::Delta(format!(
                        "[RESULT_FILE_ERR] {first_line}\n"
                    )))
                    .await;
                if tc.name.as_str() == "read_file" {
                    if let Some(display) = repo_map_display {
                        let _ = tx.send(StreamToken::Delta(display)).await;
                    }
                }
                state = AgentState::Recovery;
                let hint = if rejected_by_user {
                    "The user rejected the edit. Choose a safer alternative or ask again with a smaller change."
                        .to_string()
                } else if tc.name.as_str() == "read_file" && repo_map_hint.is_some() {
                    repo_map_hint.unwrap_or_default()
                } else {
                    format!(
                        "File tool error: {first_line}\n\
                         Read the error message carefully and fix the issue before proceeding."
                    )
                };
                pending_system_hint = Some(hint);
            } else {
                let _ = tx
                    .send(StreamToken::Delta(format!("[RESULT_FILE] {first_line}\n")))
                    .await;
                state = if recovery.in_recovery() {
                    AgentState::Recovery
                } else {
                    AgentState::Planning
                };
                pending_system_hint = if automatic_test
                    == crate::execution_evidence::AutoTestOutcome::Failed
                {
                    Some(recovery::AUTO_TEST_FAILURE_HINT.to_string())
                } else if recovery.stage == Some(RecoveryStage::Fix) {
                    Some("Recovery stage=fix: apply a minimal fix now (edit files or run a corrected command).".to_string())
                } else if recovery.stage == Some(RecoveryStage::Verify) {
                    Some(format!(
                        "Recovery stage=verify: {}",
                        verification_requirement_hint(required_verification, test_cmd.as_deref())
                    ))
                } else {
                    None
                };
                if tc.name == "read_file" || changed {
                    update_working_memory_after_non_exec(
                        &mut working_mem,
                        tc.name.as_str(),
                        verified,
                        step_label_for_turn.as_deref(),
                        next_action_for_turn,
                        test_cmd.as_deref(),
                    );
                }
                if tc.name.as_str() == "read_file" {
                    let command =
                        canonicalize_tool_call_command("read_file", tc.arguments.as_str())
                            .unwrap_or_else(|| {
                                format!("read_file(path={})", compact_one_line(path.as_str(), 160))
                            });
                    let observed_path = parse_read_file_result_path(result.as_str())
                        .unwrap_or_else(|| compact_one_line(path.as_str(), 160));
                    observation_evidence.remember_read(command.as_str(), observed_path.as_str());
                    observation_evidence.remember_resolution(
                        path.as_str(),
                        observed_path.as_str(),
                        "read_file",
                    );
                    sync_observation_cache_autosave(&autosaver, &observation_evidence);
                }
                assumption_gate::confirm_file_existence_after_result(
                    &mut assumption_ledger,
                    &tc.name,
                    &path,
                    &result,
                );
                assumption_ledger.refresh_confirmations(&working_mem);
                if changed {
                    let path_level = verification_level_for_mutation_path(path.as_str());
                    if path_level > path_required_verification {
                        path_required_verification = path_level;
                        required_verification =
                            intent_required_verification.max(path_required_verification);
                        recovery.required_verification = required_verification;
                        last_verify_ok_step = effective_verify_ok_step(
                            required_verification,
                            last_build_verify_ok_step,
                            last_behavioral_verify_ok_step,
                        );
                        if recovery.stage == Some(RecoveryStage::Verify) {
                            pending_system_hint = Some(format!(
                                "Recovery stage=verify: {}",
                                verification_requirement_hint(
                                    required_verification,
                                    test_cmd.as_deref()
                                )
                            ));
                        }
                    }
                    impact_required = Some(format!(
                        "successful mutation via {}: {}",
                        tc.name.as_str(),
                        compact_one_line(path.as_str(), 80)
                    ));
                }
            }

            if is_error || file_tool_consec_failures >= 2 {
                reflection_required = Some(
                    pending_system_hint
                        .clone()
                        .unwrap_or_else(|| "file tool failure".to_string()),
                );
                reflection_trigger_sig = tool_call_action_sig(&tc);
            }

            // Append tool result to conversation.
            let history_result = if is_error {
                result.clone()
            } else {
                compact_success_tool_result_for_history(tc.name.as_str(), &result)
            };
            messages.push(CargoInputReceipt::attach(
                cargo_inputs,
                recovery.focus.tool_message(&tc, history_result),
            ));
            if !is_error && (tc.name == "read_file" || changed) && !root_read_only {
                if let Some(hint) = build_fix_stage_progress_hint(
                    task_harness,
                    &messages,
                    recovery.stage,
                    test_cmd.as_deref(),
                ) {
                    pending_system_hint = Some(hint);
                }
            }
            if !is_error && root_read_only && tc.name.as_str() == "read_file" {
                if let Some(plan) = active_plan.as_ref() {
                    if let Some(final_text) = maybe_build_read_only_auto_final_answer(
                        true,
                        &root_user_text,
                        Some(plan),
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
                                "\n[agent] auto-finalized read-only inspection after sufficient observation.\n\n{final_text}\n"
                            )))
                            .await;
                        break;
                    } else if iter + 1 == max_iters {
                        if let Some(final_text) = build_read_only_iteration_cap_final_answer(
                            &root_user_text,
                            plan,
                            &observation_evidence,
                            &messages,
                            &working_mem,
                        ) {
                            task_outcome.finalized(&final_text);
                            state = AgentState::Done;
                            messages
                                .push(json!({"role": "assistant", "content": final_text.clone()}));
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
                                    "\n[agent] iteration cap reached; auto-finalized read-only inspection.\n\n{final_text}\n"
                                )))
                                .await;
                            break;
                        }
                    } else if let Some(hint) = build_read_only_completion_hint(
                        &root_user_text,
                        plan,
                        &observation_evidence,
                        &messages,
                        &working_mem,
                    ) {
                        pending_system_hint = Some(hint);
                    }
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

            if iter + 1 == max_iters {
                let _ = tx
                    .send(StreamToken::Delta(format!(
                        "\n[agent] iteration cap ({max_iters}) reached.\n"
                    )))
                    .await;
            }
            continue; // skip exec block below
        }

        if tc.name != "exec" {
            return Err(anyhow!("unknown tool: {}", tc.name));
        }

        let args: serde_json::Value = serde_json::from_str(&tc.arguments)?;
        let command = args["command"].as_str().unwrap_or("").to_string();

        // Resolve an optional cwd (absolute or relative). Relative cwd is resolved against
        // the current tracked directory (or tool_root).
        let mut cwd_note: Option<String> = None;
        let cwd_used: Option<String> = if let Some(c0) = args["cwd"]
            .as_str()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            let pb = std::path::PathBuf::from(c0);
            let candidate = if pb.is_absolute() {
                pb
            } else if let Some(base) = cur_cwd.as_deref().or(tool_root_abs.as_deref()) {
                std::path::PathBuf::from(base).join(pb)
            } else {
                pb
            };
            let cand_str = candidate.to_string_lossy().into_owned();
            if let Some(ref root) = tool_root_abs {
                if is_within_root(&cand_str, root) {
                    Some(cand_str)
                } else {
                    cwd_note = Some(format!(
                        "NOTE: requested cwd is outside tool_root; ignoring: {c0}"
                    ));
                    cur_cwd.clone().or_else(|| tool_root_abs.clone())
                }
            } else {
                Some(cand_str)
            }
        } else {
            cur_cwd.clone().or_else(|| tool_root_abs.clone())
        };
        let cwd_used_label = cwd_used
            .as_deref()
            .unwrap_or("(workspace root)")
            .to_string();

        // Emit tool-call annotation to TUI (dim line the UI will colour differently).
        let _ = tx
            .send(StreamToken::Delta(format!(
                "\n\n[TOOL][{:?}] {command}\n[cwd] {cwd_used_label}\n",
                state
            )))
            .await;
        let _ = tx.send(StreamToken::ToolCall(tc.clone())).await;

        if let Some(block) = should_block_git_landmines(&command, tool_root_abs.as_deref()) {
            state = AgentState::Recovery;
            recovery.on_exec_result(ExecKind::Action, None, false);
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
                None,
            )
            .await;
            continue;
        }

        // Runtime block: prevent "same command spam" once it's already repeated.
        // Allow repeating after a successful mutation (e.g., fix file then rerun tests).
        let cmd_sig = command_sig(&command);
        let repeated_cmd = mem.same_command_repeats >= 3
            && mem.last_command_sig.as_deref() == Some(cmd_sig.as_str());
        let mutated_since_last_exec = last_mutation_step.unwrap_or(0) > last_exec_step.unwrap_or(0);
        if repeated_cmd && !mutated_since_last_exec {
            state = AgentState::Recovery;
            recovery.stage = Some(RecoveryStage::Diagnose);
            let block = format!(
                "Repeated identical command blocked.\n\
same_command_repeats: {}\n\
command_sig: {}\n\
\n\
Required now: change strategy (diagnose or apply a fix) before retrying.\n\
Do NOT run the exact same command again without a material change.",
                mem.same_command_repeats, cmd_sig
            );

            let _ = tx
                .send(StreamToken::Delta(format!(
                    "[RESULT][Recovery] GOVERNOR BLOCK\n{block}\n"
                )))
                .await;

            let cwd_line = format!("cwd: {cwd_used_label}");
            let tool_output = inject_cwd(
                &format!("GOVERNOR BLOCKED\n\n{block}\n\ncommand:\n{command}"),
                &cwd_line,
                cwd_note.as_deref(),
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

            let _ = mem.on_tool_result(&command, "", &block, 1);
            last_exec_step = Some(this_step);
            pending_system_hint = Some(block.clone());
            reflection_required = Some(block);
            reflection_trigger_sig = tool_call_action_sig(&tc);
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

            if iter + 1 == max_iters {
                let _ = tx
                    .send(StreamToken::Delta(format!(
                        "\n[agent] iteration cap ({max_iters}) reached.\n"
                    )))
                    .await;
            }
            continue;
        }

        let approval = approver
            .approve(ApprovalRequest::Command {
                command: command.clone(),
                cwd: cwd_used.clone(),
            })
            .await?;
        if approval == ApprovalOutcome::Rejected {
            state = AgentState::Recovery;
            recovery.on_exec_result(ExecKind::Action, None, false);
            let _ = tx
                .send(StreamToken::Delta(
                    "[RESULT][Recovery] REJECTED by user\n".to_string(),
                ))
                .await;
            let cwd_line = format!("cwd: {cwd_used_label}");
            let tool_output = format!("REJECTED BY USER\n{cwd_line}\ncommand:\n{command}");

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
            last_exec_step = Some(this_step);
            pending_system_hint = Some(
                "The user rejected the command. Choose a safer alternative or explain why it is necessary before retrying."
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

            if iter + 1 == max_iters {
                let _ = tx
                    .send(StreamToken::Delta(format!(
                        "\n[agent] iteration cap ({max_iters}) reached.\n"
                    )))
                    .await;
            }
            continue;
        }

        let cargo_input_probe = cwd_used
            .as_deref()
            .zip(tool_root_abs.as_deref())
            .and_then(|(cwd, root)| CargoInputProbe::start(&messages, &command, cwd, root));
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

        let mut note_lines: Vec<String> = Vec::new();
        if let Some(n) = cwd_note.take() {
            note_lines.push(n);
        }
        if let Some(n) = cwd_after_note.take() {
            note_lines.push(n);
        }
        let note = if note_lines.is_empty() {
            None
        } else {
            Some(note_lines.join("\n"))
        };

        state = AgentState::Verifying;
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

        let tool_output = if effective_exit_code == 0 {
            let base = build_ok_tool_output(&stdout);
            inject_cwd(&base, &cwd_line, note.as_deref())
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
            inject_cwd(&out, &cwd_line, note.as_deref())
        };

        edit_failures.on_result(&tc, &tool_output, &exec_verification_context);
        file_tool_consec_failures = edit_failures.count();
        recovery.focus.record_result(
            &tc,
            &tool_output,
            tool_root_abs.as_deref(),
            test_cmd.as_deref(),
            &exec_verification_context,
        );

        let result_label = if effective_exit_code == 0 {
            format!("[RESULT][{:?}] exit=0\n", state)
        } else {
            format!("[RESULT][{:?}] exit={effective_exit_code} !\n", state)
        };
        let _ = tx.send(StreamToken::Delta(result_label)).await;

        // ── Append tool result (with tool_call_id preserved) ───────────────
        let history_tool_output = if effective_exit_code == 0 {
            compact_success_tool_result_for_history("exec", &tool_output)
        } else {
            tool_output.clone()
        };
        let cargo_inputs =
            cargo_input_probe.map(|p| p.finish(effective_exit_code == 0, &stdout, &stderr));
        messages.push(report_counts::capture(
            CargoInputReceipt::attach(
                cargo_inputs,
                recovery.focus.tool_message(&tc, history_tool_output),
            ),
            &stdout,
            &stderr,
        ));
        last_exec_step = Some(this_step);
        autosave_best_effort(
            &autosaver,
            &tx,
            tool_root_abs.as_deref(),
            checkpoint.as_deref(),
            cur_cwd.as_deref(),
            &messages,
        )
        .await;

        // Update failure memory + recovery governor + possibly inject a system hint.
        let classification = exec_verification_context.classify(&command);
        let verify_level = classification.verification;
        let exec_kind = classification.kind;
        exec_proof::record_result(
            &command,
            &exec_verification_context,
            exec_proof::ExecProofResult {
                content: &tool_output,
                succeeded: effective_exit_code == 0 && !escaped_tool_root,
            },
            this_step,
            &mut last_mutation_step,
            &mut last_build_verify_ok_step,
            &mut last_behavioral_verify_ok_step,
        );
        last_verify_ok_step = effective_verify_ok_step(
            required_verification,
            last_build_verify_ok_step,
            last_behavioral_verify_ok_step,
        );
        if effective_exit_code == 0 && !escaped_tool_root {
            update_working_memory_after_exec(
                &mut working_mem,
                command.as_str(),
                stdout.as_str(),
                tool_output.as_str(),
                exec_kind,
                step_label_for_turn.as_deref(),
                next_action_for_turn,
            );
            assumption_ledger.refresh_confirmations(&working_mem);
            if exec_kind == ExecKind::Action {
                impact_required = Some(format!(
                    "successful mutation command: {}",
                    compact_one_line(command.as_str(), 100)
                ));
            }
        }
        let mut hint = if escaped_tool_root {
            Some(
                "SANDBOX BREACH: Your command ended outside tool_root.\n\
Action: re-run from tool_root, avoid `cd ..` / absolute paths, and verify `pwd` stays under tool_root."
                    .to_string(),
            )
        } else {
            mem.on_tool_result(&command, &stdout, &stderr, effective_exit_code)
        };

        recovery.on_exec_result(
            exec_kind,
            verify_level,
            effective_exit_code == 0 && !escaped_tool_root,
        );

        if effective_exit_code == 0 && !escaped_tool_root {
            if recovery.stage == Some(RecoveryStage::Fix) {
                hint = Some(
                    "Recovery stage=fix: apply a minimal fix now (edit files or run a corrected command)."
                        .to_string(),
                );
            } else if recovery.stage == Some(RecoveryStage::Verify) {
                hint = Some(format!(
                    "Recovery stage=verify: {}",
                    verification_requirement_hint(required_verification, test_cmd.as_deref())
                ));
            }
        }

        let hint_clone_for_reflect = hint.clone();
        pending_system_hint = hint;
        state = if effective_exit_code != 0 || recovery.in_recovery() {
            AgentState::Recovery
        } else {
            AgentState::Planning
        };

        if effective_exit_code != 0 || mem.repeated_failure_or_stall() {
            let default_reason = if effective_exit_code != 0 {
                let class_ctx = error_class_hint(&mem.last_error_class);
                if class_ctx.is_empty() {
                    "failure detected".to_string()
                } else {
                    class_ctx.to_string()
                }
            } else {
                "stall detected".to_string()
            };
            reflection_required = Some(hint_clone_for_reflect.unwrap_or_else(|| default_reason));
            reflection_trigger_sig = tool_call_action_sig(&tc);
        } else if reflection_guard.is_none() {
            reflection_trigger_sig = None;
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

        // Safety: stop if we've hit the iteration cap.
        if iter + 1 == max_iters {
            let _ = tx
                .send(StreamToken::Delta(format!(
                    "\n[agent] iteration cap ({max_iters}) reached.\n"
                )))
                .await;
        }
    }

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
    )
    .filter(|_| observer_handoff.is_none() && !blocked_terminal)
    {
        task_outcome.finalized(&final_text);
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
            .send(StreamToken::Delta(format!(
                "\n[agent] auto-finalized verified action task at session end.\n\n{final_text}\n"
            )))
            .await;
    }

    if realize_cfg.enabled {
        if let Some(latent) = latent_plan.take() {
            let latency = max_iters.saturating_sub(latent.created_iter);
            let drift = drift_distance(
                realize_cfg.drift_metric,
                &latent.summary,
                &latent.anchor_baseline,
            );
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
                    "session_end",
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
            emit_realize_state(&tx, &realize_cfg, None, max_iters, None, &realize_metrics).await;
        }
    }

    emit_telemetry_event(
        &tx,
        "agent_outcome",
        json!({
            "completed": !blocked_terminal && observer_handoff.is_none() && task_outcome.completed(&messages),
            "state": format!("{state:?}").to_ascii_lowercase(),
            "yield_reason": if blocked_terminal { Some(super::coder_yield::CoderYieldReason::Blocked) } else { observer_handoff.as_ref().map(|_| super::coder_yield::CoderYieldReason::VerificationFailure) },
        }),
    )
    .await;
    let _ = tx.send(StreamToken::Done).await;
    Ok(AgenticEndState {
        messages,
        checkpoint,
        cur_cwd,
        observation_cache: Some(observation_evidence.to_session_cache()),
    })
}

#[cfg(test)]
mod tests;
