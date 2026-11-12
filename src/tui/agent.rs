/// Coder agentic loop: calls the model with an `exec` tool, runs commands,
/// and loops until finish_reason == "stop" or max iterations are reached.
///
/// Reasoning improvements applied here:
///   1. Scratchpad protocol  — model outputs structured <plan>/<think> blocks
///      before every tool call (~50 tokens).  Prevents wrong-direction errors.
///   2. Tool output truncation  — stdout > 1500 chars / stderr > 600 chars are trimmed.
///      This is the single largest token-saver on long runs.
///   3. Context projection — old exchanges are compressed for provider requests;
///      native history remains intact for recovery, proof and session resume.
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
pub mod agent_loop;
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
mod closeout;
pub(crate) mod context_compaction;
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
mod impact_gate;
mod impact_recovery;
mod implied_actions;
mod json_fields;
mod loop_setup;
mod memory;
#[cfg(test)] // legacy session-pruning regression coverage
mod message_window;
mod meta_harness;
mod native_arguments;
mod next_action;
#[cfg(test)]
mod next_action_tests;
#[cfg(test)]
mod no_change_runtime_tests;
mod no_tool_call;
mod observation_stall;
mod outcome;
mod output_compaction;
mod plan_governor;
mod plan_resume;
use plan_resume::ActivePlan;
mod plan_revision;
#[cfg(test)]
mod plan_revision_tests;
mod plan_think_gate;
mod progress_bridge;
mod prompt_assembly;
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
mod reflection_gate;
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
mod done_dispatch;
mod exec_dispatch;
mod file_tool_dispatch;
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
mod tool_coercion;
mod tool_dispatch;
mod tool_result_history;
#[cfg(test)]
mod tool_result_history_tests;
mod turn_stream;
mod verification;
mod work_progress;
#[allow(unused_imports)] // keep the historical `agent::run_agentic` path working
pub use self::agent_loop::{run_agentic, run_agentic_json};
use self::assumption_gate::refuted_assumption_conflict;
use self::block_parsers::*;
use self::closeout::*;
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
use self::harness_evolution::ContractPatchProposal;
pub(crate) use self::harness_evolution::{
    overlay_path_for_root as harness_evolution_overlay_path_for_root,
    path_for_root as harness_evolution_queue_path_for_root,
    GovernorContractOverlay as HarnessGovernorContractOverlay,
    HarnessEvolutionQueue as PersistentHarnessEvolutionQueue,
};
use self::memory::{
    canonicalize_evidence_command_with_resolution, normalize_memory_entry, normalize_path_alias,
    remember_recent_unique, remember_repo_map_resolution, rewrite_tool_call_with_resolution,
    ObservationEvidence, ObservationReadEvidence, ObservationSearchEvidence,
};
#[cfg(test)]
use self::message_window::prune_message_window;
#[cfg(test)]
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
#[cfg(test)]
mod tests;
