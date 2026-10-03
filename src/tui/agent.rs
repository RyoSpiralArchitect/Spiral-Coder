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

mod done_gate;
mod evaluator_loop;
mod exec_classification;
mod exec_proof;
mod exec_verification;
mod failure_localization;
mod final_handoff;
mod followup_requirements;
mod harness_evolution;
mod impact_recovery;
mod memory;
mod message_window;
mod meta_harness;
mod outcome;
mod progress_bridge;
mod protocol_fields;
mod provider_compat;
mod read_only;
mod repo_scaffold;
mod session_bridge;
mod task_harness;
#[cfg(test)]
mod tool_result_history_tests;
use self::done_gate::{
    build_done_acceptance_recovery_hint, build_post_verify_done_completion_hint,
    build_read_only_completion_hint, build_read_only_evidence_scores,
    build_read_only_iteration_cap_final_answer, build_read_only_strong_final_answer,
    canonicalize_known_acceptance_commands, latest_assistant_message_is_done,
    maybe_build_read_only_auto_final_answer, maybe_build_verified_action_auto_final_answer,
    rescue_invalid_done_payload_for_verified_action, should_prefer_done_after_verified_action,
    synthesize_action_done_summary, validate_done_acceptance,
};
use self::evaluator_loop::EvaluatorLoop;
use self::exec_proof::restore_done_gate_from_messages;
use self::failure_localization::interesting_failure_line;
use self::final_handoff::{
    authored_final_answer_hint, enrich_text_final_handoff, requires_authored_final_answer,
    validate_authored_done_summary,
};
use self::followup_requirements::{
    coerce_existing_followup_tool_call, matches_required_existing_followup,
    required_existing_followup_no_tool_hint,
};
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
use self::message_window::prune_message_window;
use self::meta_harness::MetaHarness;
use self::progress_bridge::ProgressBridgeView;
use self::protocol_fields::parse_tag_fields;
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
use self::session_bridge::SessionBridgeView;
use self::task_harness::{
    allows_artifact_creation_during_diagnose, allows_artifact_creation_during_verify,
    allows_benchmark_plan_followup_during_verify, benchmark_plan_missing_required_exec_proof,
    benchmark_plan_pending_required_exec_command, build_fix_stage_progress_hint,
    build_progress_gate_block, coerce_artifact_creation_tool_call, coerce_benchmark_plan_tool_call,
    coerce_fix_existing_blocked_mutation_tool_call, coerce_fix_existing_literal_mutation_tool_call,
    coerce_fix_existing_tool_call, coerce_repo_goal_completion_tool_call,
    repair_fix_existing_mutation_tool_call, repair_repo_scaffold_write_tool_call,
    synthesize_benchmark_plan_no_tool_call, synthesize_fix_existing_no_tool_mutation_tool_call,
    TaskHarness,
};

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

async fn autosave_best_effort(
    autosaver: &Option<Arc<crate::agent_session::SessionAutoSaver>>,
    tx: &mpsc::Sender<StreamToken>,
    tool_root_abs: Option<&str>,
    checkpoint: Option<&str>,
    cur_cwd: Option<&str>,
    messages: &[serde_json::Value],
) {
    let Some(ref saver) = autosaver else {
        return;
    };
    let Some(warn) = saver.save_best_effort(tool_root_abs, checkpoint, cur_cwd, messages) else {
        return;
    };
    let _ = tx
        .send(StreamToken::Delta(format!("\n[autosave] WARN: {warn}\n")))
        .await;
}

fn sync_observation_cache_autosave(
    autosaver: &Option<Arc<crate::agent_session::SessionAutoSaver>>,
    evidence: &ObservationEvidence,
) {
    if let Some(saver) = autosaver {
        saver.set_observation_cache(Some(evidence.to_session_cache()));
    }
}

fn sync_progress_context_autosave(
    autosaver: &Option<Arc<crate::agent_session::SessionAutoSaver>>,
    context: crate::progress_state::ProgressSaveContext,
) {
    if let Some(saver) = autosaver {
        saver.set_progress_context(Some(context));
    }
}

async fn emit_telemetry_event(
    tx: &mpsc::Sender<StreamToken>,
    event: &str,
    data: serde_json::Value,
) {
    let _ = tx
        .send(StreamToken::Telemetry(TelemetryEvent {
            event: event.to_string(),
            data,
        }))
        .await;
}

async fn emit_repo_map_fallback_telemetry(
    tx: &mpsc::Sender<StreamToken>,
    tool_name: &str,
    query: &str,
    fallback: &crate::repo_map::RepoMapFallback,
) {
    emit_telemetry_event(
        tx,
        "repo_map_fallback",
        json!({
            "tool": tool_name,
            "query": query,
            "top_path": fallback.top_path,
            "top_dir": fallback.top_dir,
            "top_confidence": fallback.top_confidence,
            "typo_likely": fallback.typo_likely,
            "reasons": fallback.top_path_reasons,
        }),
    )
    .await;
}

async fn emit_resolution_memory_hit_telemetry(
    tx: &mpsc::Sender<StreamToken>,
    tool_name: &str,
    query: &str,
    canonical_path: &str,
) {
    emit_telemetry_event(
        tx,
        "resolution_memory_hit",
        json!({
            "tool": tool_name,
            "query": query,
            "canonical_path": canonical_path,
        }),
    )
    .await;
}

fn reflection_ledger_action_match<'a>(
    ledger: &'a crate::reflection_ledger::ReflectionLedger,
    tc: &ToolCallData,
) -> Option<(
    &'a crate::reflection_ledger::ReflectionLedgerEntry,
    f32,
    String,
)> {
    let action = canonicalize_tool_call_command(tc.name.as_str(), tc.arguments.as_str())
        .unwrap_or_else(|| blocked_tool_call_signature(tc.name.as_str(), tc.arguments.as_str()));
    let action_sig = normalize_for_signature(action.as_str());
    let action_tokens = keyword_tokens(action.as_str());
    let tool_name = tc.name.to_ascii_lowercase();
    let args: serde_json::Value = serde_json::from_str(&tc.arguments).unwrap_or_else(|_| json!({}));
    let primary_target = args
        .get("path")
        .or_else(|| args.get("dir"))
        .or_else(|| args.get("command"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let target_sig = normalize_for_signature(primary_target.as_str());

    let mut best: Option<(&crate::reflection_ledger::ReflectionLedgerEntry, f32)> = None;
    for entry in &ledger.entries {
        let next = entry.next_minimal_action.as_str();
        let next_sig = normalize_for_signature(next);
        let next_tokens = keyword_tokens(next);
        let mut score = token_overlap_score(&next_tokens, &action_tokens);
        if !tool_name.is_empty() && next_sig.contains(tool_name.as_str()) {
            score += 0.10;
        }
        if !target_sig.is_empty() && next_sig.contains(target_sig.as_str()) {
            score += 0.35;
        }
        if next_sig.contains(action_sig.as_str()) || action_sig.contains(next_sig.as_str()) {
            score += 0.25;
        }
        score = score.clamp(0.0, 1.0);
        if score >= 0.45 {
            match best {
                Some((_, best_score)) if best_score >= score => {}
                _ => best = Some((entry, score)),
            }
        }
    }
    best.map(|(entry, score)| (entry, score, action))
}

fn push_blocked_tool_exchange(
    messages: &mut Vec<serde_json::Value>,
    assistant_text: &str,
    tc: &ToolCallData,
    block: &str,
) {
    messages.push(json!({
        "role": "assistant",
        "content": assistant_text,
        "tool_calls": [{
            "id": tc.id,
            "type": "function",
            "function": {
                "name": tc.name,
                "arguments": tc.arguments
            }
        }]
    }));
    messages.push(json!({
        "role": "tool",
        "tool_call_id": tc.id,
        "content": format!(
            "GOVERNOR BLOCKED\n\n{block}\n\ntool:\n{}\narguments:\n{}",
            tc.name, tc.arguments
        ),
    }));
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RepoMapHintStrength {
    Strong,
    Medium,
    Weak,
}

fn repo_map_hint_strength(confidence: f64) -> RepoMapHintStrength {
    if confidence >= 0.85 {
        RepoMapHintStrength::Strong
    } else if confidence >= 0.60 {
        RepoMapHintStrength::Medium
    } else {
        RepoMapHintStrength::Weak
    }
}

fn repo_map_action_verb(confidence: f64) -> &'static str {
    match repo_map_hint_strength(confidence) {
        RepoMapHintStrength::Strong => "Prefer",
        RepoMapHintStrength::Medium => "Start with",
        RepoMapHintStrength::Weak => "Consider",
    }
}

fn repo_map_typo_intro(confidence: f64) -> &'static str {
    match repo_map_hint_strength(confidence) {
        RepoMapHintStrength::Strong => "repo_map strong path-typo.",
        RepoMapHintStrength::Medium => "repo_map medium path-typo.",
        RepoMapHintStrength::Weak => "repo_map weak path-typo.",
    }
}

fn repo_map_fallback_intro(confidence: f64) -> &'static str {
    match repo_map_hint_strength(confidence) {
        RepoMapHintStrength::Strong => "repo_map strong candidate.",
        RepoMapHintStrength::Medium => "repo_map medium candidate.",
        RepoMapHintStrength::Weak => "repo_map weak candidate.",
    }
}

fn repo_map_confidence_guidance(confidence: f64) -> &'static str {
    match repo_map_hint_strength(confidence) {
        RepoMapHintStrength::Strong => {
            "Treat the top candidate as the default next step unless your broader plan clearly points elsewhere."
        }
        RepoMapHintStrength::Medium => {
            "Start there, but keep the rest of the fallback list in view before committing."
        }
        RepoMapHintStrength::Weak => {
            "Treat the fallback list as suggestions only and compare multiple candidates before retrying."
        }
    }
}

fn repo_map_reason_suffix(reasons: &[String]) -> String {
    if reasons.is_empty() {
        String::new()
    } else {
        format!(" [{}]", reasons.join(", "))
    }
}

fn repo_map_display_level(confidence: f64) -> &'static str {
    match repo_map_hint_strength(confidence) {
        RepoMapHintStrength::Strong => "strong",
        RepoMapHintStrength::Medium => "medium",
        RepoMapHintStrength::Weak => "weak",
    }
}

fn repo_map_display_next_step(fallback: &crate::repo_map::RepoMapFallback) -> String {
    match (fallback.top_dir.as_deref(), fallback.top_path.as_deref()) {
        (Some(candidate_dir), Some(candidate_path)) => {
            format!(
                "list_dir '{}' -> read_file '{}'",
                candidate_dir, candidate_path
            )
        }
        (Some(candidate_dir), None) => format!("list_dir '{}'", candidate_dir),
        (None, Some(candidate_path)) => format!("read_file '{}'", candidate_path),
        (None, None) => "inspect fallback list".to_string(),
    }
}

fn build_repo_map_display_banner(
    tool_name: &str,
    fallback: &crate::repo_map::RepoMapFallback,
) -> String {
    let confidence = fallback.top_confidence.unwrap_or(0.0);
    let level = repo_map_display_level(confidence);
    let flavor = if fallback.typo_likely {
        "path-typo"
    } else {
        "candidate"
    };
    let next = repo_map_display_next_step(fallback);
    format!("[repo_map:{tool_name}] {level} {flavor} -> {next}\n")
}

fn build_repo_map_search_hint(
    pattern: &str,
    fallback: &crate::repo_map::RepoMapFallback,
) -> String {
    let confidence = fallback.top_confidence.unwrap_or(0.0);
    let action = repo_map_action_verb(confidence);
    let guidance = repo_map_confidence_guidance(confidence);
    match fallback.top_path.as_deref() {
        Some(path) => format!(
            "Literal search found no matches for '{}'. {}\n\
{} read_file '{}' next (top_confidence={:.2}).\n\
{}",
            pattern,
            repo_map_fallback_intro(confidence),
            action,
            path,
            confidence,
            guidance
        ),
        None => format!(
            "Literal search found no matches for '{}'. Use the repo_map fallback candidates from the tool output before retrying.\n\
{}",
            pattern, guidance
        ),
    }
}

fn build_repo_map_read_hint(path: &str, fallback: &crate::repo_map::RepoMapFallback) -> String {
    let confidence = fallback.top_confidence.unwrap_or(0.0);
    let action = repo_map_action_verb(confidence);
    let reason_suffix = repo_map_reason_suffix(&fallback.top_path_reasons);
    let guidance = repo_map_confidence_guidance(confidence);
    match fallback.top_path.as_deref() {
        Some(candidate) if fallback.typo_likely => format!(
            "read_file failed for '{}'. {}\n\
{} read_file '{}' next (top_confidence={:.2}){}.\n\
{}",
            path,
            repo_map_typo_intro(confidence),
            action,
            candidate,
            confidence,
            reason_suffix,
            guidance
        ),
        Some(candidate) => format!(
            "read_file failed for '{}'. {}\n\
{} read_file '{}' next (top_confidence={:.2}).\n\
{}",
            path,
            repo_map_fallback_intro(confidence),
            action,
            candidate,
            confidence,
            guidance
        ),
        None => format!(
            "read_file failed for '{}'. Use the repo_map fallback candidates from the tool output before retrying.\n\
{}",
            path, guidance
        ),
    }
}

fn build_repo_map_list_dir_hint(dir: &str, fallback: &crate::repo_map::RepoMapFallback) -> String {
    let confidence = fallback.top_confidence.unwrap_or(0.0);
    let action = repo_map_action_verb(confidence);
    let reason_suffix = repo_map_reason_suffix(&fallback.top_path_reasons);
    let guidance = repo_map_confidence_guidance(confidence);
    match (fallback.top_dir.as_deref(), fallback.top_path.as_deref(), fallback.typo_likely) {
        (Some(candidate_dir), Some(candidate_path), true) => format!(
            "list_dir failed for '{}'. {}\n\
{} list_dir '{}' next; if you were actually targeting a file, read_file '{}' after that (top_confidence={:.2}){}.\n\
{}",
            dir,
            repo_map_typo_intro(confidence),
            action,
            candidate_dir,
            candidate_path,
            confidence,
            reason_suffix,
            guidance
        ),
        (Some(candidate_dir), None, true) => format!(
            "list_dir failed for '{}'. {}\n\
{} list_dir '{}' next (top_confidence={:.2}){}.\n\
{}",
            dir,
            repo_map_typo_intro(confidence),
            action,
            candidate_dir,
            confidence,
            reason_suffix,
            guidance
        ),
        (None, Some(candidate_path), true) => format!(
            "list_dir failed for '{}'. {}\n\
{} read_file '{}' next (top_confidence={:.2}){}.\n\
{}",
            dir,
            repo_map_typo_intro(confidence),
            action,
            candidate_path,
            confidence,
            reason_suffix,
            guidance
        ),
        (Some(candidate_dir), Some(candidate_path), false) => format!(
            "list_dir failed for '{}'. {}\n\
{} list_dir '{}' first; if that still misses your target, read_file '{}' next (top_confidence={:.2}).\n\
{}",
            dir,
            repo_map_fallback_intro(confidence),
            action,
            candidate_dir,
            candidate_path,
            confidence,
            guidance
        ),
        (Some(candidate_dir), None, false) => format!(
            "list_dir failed for '{}'. {}\n\
{} list_dir '{}' first (top_confidence={:.2}).\n\
{}",
            dir,
            repo_map_fallback_intro(confidence),
            action,
            candidate_dir,
            confidence,
            guidance
        ),
        (None, Some(candidate_path), false) => format!(
            "list_dir failed for '{}'. {}\n\
{} read_file '{}' first (top_confidence={:.2}).\n\
{}",
            dir,
            repo_map_fallback_intro(confidence),
            action,
            candidate_path,
            confidence,
            guidance
        ),
        (None, None, _) => format!(
            "list_dir failed for '{}'. Use the repo_map fallback candidates from the tool output before retrying.\n\
{}",
            dir, guidance
        ),
    }
}

fn build_repo_map_glob_hint(pattern: &str, fallback: &crate::repo_map::RepoMapFallback) -> String {
    let confidence = fallback.top_confidence.unwrap_or(0.0);
    let action = repo_map_action_verb(confidence);
    let reason_suffix = repo_map_reason_suffix(&fallback.top_path_reasons);
    let guidance = repo_map_confidence_guidance(confidence);
    match (fallback.top_dir.as_deref(), fallback.top_path.as_deref(), fallback.typo_likely) {
        (Some(candidate_dir), Some(candidate_path), true) => format!(
            "Literal glob found no matches for '{}'. {}\n\
{} inspect '{}' first; if needed, read_file '{}' after that (top_confidence={:.2}){}.\n\
{}",
            pattern,
            repo_map_typo_intro(confidence),
            action,
            candidate_dir,
            candidate_path,
            confidence,
            reason_suffix,
            guidance
        ),
        (Some(candidate_dir), None, true) => format!(
            "Literal glob found no matches for '{}'. {}\n\
{} inspect '{}' first (top_confidence={:.2}){}.\n\
{}",
            pattern,
            repo_map_typo_intro(confidence),
            action,
            candidate_dir,
            confidence,
            reason_suffix,
            guidance
        ),
        (None, Some(candidate_path), true) => format!(
            "Literal glob found no matches for '{}'. {}\n\
{} read_file '{}' next (top_confidence={:.2}){}.\n\
{}",
            pattern,
            repo_map_typo_intro(confidence),
            action,
            candidate_path,
            confidence,
            reason_suffix,
            guidance
        ),
        (Some(candidate_dir), Some(candidate_path), false) => format!(
            "Literal glob found no matches for '{}'. {}\n\
{} list_dir '{}' first; if needed, read_file '{}' next (top_confidence={:.2}).\n\
{}",
            pattern,
            repo_map_fallback_intro(confidence),
            action,
            candidate_dir,
            candidate_path,
            confidence,
            guidance
        ),
        (Some(candidate_dir), None, false) => format!(
            "Literal glob found no matches for '{}'. {}\n\
{} list_dir '{}' first (top_confidence={:.2}).\n\
{}",
            pattern,
            repo_map_fallback_intro(confidence),
            action,
            candidate_dir,
            confidence,
            guidance
        ),
        (None, Some(candidate_path), false) => format!(
            "Literal glob found no matches for '{}'. {}\n\
{} read_file '{}' first (top_confidence={:.2}).\n\
{}",
            pattern,
            repo_map_fallback_intro(confidence),
            action,
            candidate_path,
            confidence,
            guidance
        ),
        (None, None, _) => format!(
            "Literal glob found no matches for '{}'. Use the repo_map fallback candidates from the tool output before retrying.\n\
{}",
            pattern, guidance
        ),
    }
}

fn rewrite_tool_call_to_search_files(
    tc: &ToolCallData,
    pattern: &str,
    dir: &str,
) -> Option<(ToolCallData, String, String)> {
    let rewritten = ToolCallData {
        id: tc.id.clone(),
        name: "search_files".to_string(),
        arguments: json!({
            "pattern": pattern,
            "dir": dir,
        })
        .to_string(),
    };
    let original = canonicalize_tool_call_command(tc.name.as_str(), tc.arguments.as_str())
        .unwrap_or_else(|| blocked_tool_call_signature(tc.name.as_str(), tc.arguments.as_str()));
    let coerced =
        canonicalize_tool_call_command(rewritten.name.as_str(), rewritten.arguments.as_str())
            .unwrap_or_else(|| {
                format!(
                    "search_files(pattern={}, dir={})",
                    compact_one_line(pattern, 120),
                    compact_one_line(dir, 160)
                )
            });
    if original == coerced {
        None
    } else {
        Some((rewritten, original, coerced))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DriftMetric {
    Cos,
    Kl,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RealizePreset {
    Off,
    Low,
    Mid,
    High,
}

impl RealizePreset {
    pub const fn tui_default() -> Self {
        Self::Mid
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Low => "low",
            Self::Mid => "mid",
            Self::High => "high",
        }
    }

    fn config(self) -> RealizeOnDemandConfig {
        match self {
            Self::Off => {
                let mut cfg = RealizeOnDemandConfig::default();
                cfg.enabled = false;
                cfg
            }
            Self::Low => RealizeOnDemandConfig {
                enabled: true,
                defer_threshold: 0.62,
                window_start: 1,
                window_end: 2,
                drift_metric: DriftMetric::Cos,
                lambda_min: 0.08,
                lambda_max: 0.45,
            },
            Self::Mid => RealizeOnDemandConfig {
                enabled: true,
                defer_threshold: 0.45,
                window_start: 1,
                window_end: 3,
                drift_metric: DriftMetric::Cos,
                lambda_min: 0.15,
                lambda_max: 0.90,
            },
            Self::High => RealizeOnDemandConfig {
                enabled: true,
                defer_threshold: 0.30,
                window_start: 1,
                window_end: 4,
                drift_metric: DriftMetric::Cos,
                lambda_min: 0.25,
                lambda_max: 1.30,
            },
        }
    }

    pub fn summary(self) -> String {
        if self == Self::Off {
            return "off (latent claim/plan defer disabled)".to_string();
        }
        let cfg = self.config();
        format!(
            "{} (threshold={:.2}, window={}..{}, drift={}, lambda={:.2}..{:.2})",
            self.label(),
            cfg.defer_threshold,
            cfg.window_start,
            cfg.window_end,
            match cfg.drift_metric {
                DriftMetric::Cos => "cos",
                DriftMetric::Kl => "kl",
            },
            cfg.lambda_min,
            cfg.lambda_max
        )
    }
}

impl std::str::FromStr for RealizePreset {
    type Err = ();

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "off" => Ok(Self::Off),
            "low" => Ok(Self::Low),
            "mid" | "medium" => Ok(Self::Mid),
            "high" => Ok(Self::High),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct RealizeOnDemandConfig {
    enabled: bool,
    defer_threshold: f64,
    window_start: usize,
    window_end: usize,
    drift_metric: DriftMetric,
    lambda_min: f64,
    lambda_max: f64,
}

impl Default for RealizeOnDemandConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            defer_threshold: 0.45,
            window_start: 1,
            window_end: 3,
            drift_metric: DriftMetric::Cos,
            lambda_min: 0.15,
            lambda_max: 0.90,
        }
    }
}

#[derive(Debug, Clone)]
struct LatentPlanBuffer {
    raw_text: String,
    plan: PlanBlock,
    summary: String,
    latest_intent: Option<String>,
    anchor_baseline: String,
    created_iter: usize,
    defer_score: f64,
    tail_updates: usize,
}

#[derive(Debug, Clone, Default)]
struct RealizeMetrics {
    within_window_turns: usize,
    early_leakage: usize,
    missing: usize,
    total_drift: f64,
    drift_samples: usize,
    realize_count: usize,
    total_realize_latency: usize,
}

impl RealizeMetrics {
    fn mean_drift(&self) -> f64 {
        if self.drift_samples == 0 {
            0.0
        } else {
            self.total_drift / self.drift_samples as f64
        }
    }

    fn mean_realize_latency(&self) -> f64 {
        if self.realize_count == 0 {
            0.0
        } else {
            self.total_realize_latency as f64 / self.realize_count as f64
        }
    }
}

fn env_bool(name: &str, default: bool) -> bool {
    match std::env::var(name) {
        Ok(value) => matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        ),
        Err(_) => default,
    }
}

fn env_f64(name: &str, default: f64) -> f64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.trim().parse::<f64>().ok())
        .unwrap_or(default)
}

fn parse_realization_window(raw: &str) -> Option<(usize, usize)> {
    let trimmed = raw.trim();
    let (lhs, rhs) = trimmed
        .split_once(':')
        .or_else(|| trimmed.split_once(','))
        .or_else(|| trimmed.split_once('-'))?;
    let start = lhs.trim().parse::<usize>().ok()?;
    let end = rhs.trim().parse::<usize>().ok()?;
    if start > end || end == 0 {
        return None;
    }
    Some((start, end))
}

impl RealizeOnDemandConfig {
    fn resolve(preset: Option<RealizePreset>) -> Self {
        match preset {
            Some(preset) => preset.config(),
            None => Self::from_env(),
        }
    }

    fn from_env() -> Self {
        let mut cfg = Self::default();
        cfg.enabled = env_bool("SPIRAL_CODER_REALIZE_ON_DEMAND", false);
        cfg.defer_threshold =
            env_f64("SPIRAL_CODER_REALIZE_DEFER_THRESHOLD", cfg.defer_threshold).clamp(0.05, 1.0);
        if let Ok(raw) = std::env::var("SPIRAL_CODER_REALIZATION_WINDOW") {
            if let Some((start, end)) = parse_realization_window(&raw) {
                cfg.window_start = start;
                cfg.window_end = end.max(start);
            }
        }
        cfg.drift_metric = match std::env::var("SPIRAL_CODER_REALIZE_DRIFT_METRIC")
            .unwrap_or_else(|_| "cos".to_string())
            .trim()
            .to_ascii_lowercase()
            .as_str()
        {
            "kl" => DriftMetric::Kl,
            _ => DriftMetric::Cos,
        };
        cfg.lambda_min = env_f64("SPIRAL_CODER_REALIZE_LAMBDA_MIN", cfg.lambda_min).clamp(0.0, 4.0);
        cfg.lambda_max = env_f64("SPIRAL_CODER_REALIZE_LAMBDA_MAX", cfg.lambda_max).clamp(0.0, 4.0);
        if cfg.lambda_max < cfg.lambda_min {
            cfg.lambda_max = cfg.lambda_min;
        }
        cfg
    }

    fn within_window(&self, age_turns: usize) -> bool {
        age_turns >= self.window_start && age_turns < self.window_end
    }

    fn lambda_for_age(&self, age_turns: usize) -> f64 {
        if self.window_end <= self.window_start {
            return self.lambda_max;
        }
        let numer = age_turns.saturating_sub(self.window_start) as f64;
        let denom = (self.window_end - self.window_start).max(1) as f64;
        let frac = (numer / denom).clamp(0.0, 1.0);
        self.lambda_min + (self.lambda_max - self.lambda_min) * frac
    }
}

fn token_freqs(s: &str) -> BTreeMap<String, f64> {
    let mut out = BTreeMap::<String, f64>::new();
    let mut cur = String::new();
    for ch in s.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
            cur.push(ch.to_ascii_lowercase());
        } else if !cur.is_empty() {
            *out.entry(std::mem::take(&mut cur)).or_insert(0.0) += 1.0;
        }
    }
    if !cur.is_empty() {
        *out.entry(cur).or_insert(0.0) += 1.0;
    }
    out
}

fn cosine_token_distance(a: &str, b: &str) -> f64 {
    let fa = token_freqs(a);
    let fb = token_freqs(b);
    if fa.is_empty() || fb.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0;
    let mut na = 0.0;
    let mut nb = 0.0;
    for value in fa.values() {
        na += value * value;
    }
    for value in fb.values() {
        nb += value * value;
    }
    for (token, av) in &fa {
        if let Some(bv) = fb.get(token) {
            dot += av * bv;
        }
    }
    let denom = na.sqrt() * nb.sqrt();
    if denom <= f64::EPSILON {
        0.0
    } else {
        (1.0 - (dot / denom)).clamp(0.0, 1.0)
    }
}

fn kl_token_distance(a: &str, b: &str) -> f64 {
    let fa = token_freqs(a);
    let fb = token_freqs(b);
    if fa.is_empty() || fb.is_empty() {
        return 0.0;
    }
    let sum_a: f64 = fa.values().sum();
    let sum_b: f64 = fb.values().sum();
    let eps = 1e-6;
    let mut all_keys: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    all_keys.extend(fa.keys().cloned());
    all_keys.extend(fb.keys().cloned());
    let mut kl = 0.0;
    for key in all_keys {
        let pa = fa.get(&key).copied().unwrap_or(0.0) / sum_a + eps;
        let pb = fb.get(&key).copied().unwrap_or(0.0) / sum_b + eps;
        kl += pa * (pa / pb).ln();
    }
    kl.clamp(0.0, 4.0) / 4.0
}

fn drift_distance(metric: DriftMetric, tail: &str, anchor: &str) -> f64 {
    match metric {
        DriftMetric::Cos => cosine_token_distance(tail, anchor),
        DriftMetric::Kl => kl_token_distance(tail, anchor),
    }
}

fn strip_tag_block_owned(text: &str, tag: &str) -> String {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let Some(start) = text.find(&open) else {
        return text.to_string();
    };
    let rest = &text[start + open.len()..];
    let Some(end_rel) = rest.find(&close) else {
        return text.to_string();
    };
    let end = start + open.len() + end_rel + close.len();
    let mut out = String::new();
    out.push_str(text[..start].trim_end());
    if !out.is_empty() && !text[end..].trim_start().is_empty() {
        out.push('\n');
    }
    out.push_str(text[end..].trim_start());
    out
}

fn parse_realize_block(text: &str) -> Option<String> {
    let body = extract_tag_block(text, "realize")?;
    let fields = parse_tag_fields(body, "realize");
    for (key, value) in fields {
        if key == "reason" && !value.trim().is_empty() {
            return Some(compact_one_line(value.trim(), 160));
        }
    }
    if body.trim().is_empty() {
        None
    } else {
        Some(compact_one_line(body.trim(), 160))
    }
}

fn strip_known_latent_tags(text: &str) -> String {
    let no_plan = strip_tag_block_owned(text, "plan");
    let no_think = strip_tag_block_owned(&no_plan, "think");
    strip_tag_block_owned(&no_think, "realize")
}

fn think_intent_summary(think: &ThinkBlock) -> String {
    format!(
        "tool: {}; next: {}; verify: {}",
        compact_one_line(think.tool.as_str(), 24),
        compact_one_line(think.next.as_str(), 80),
        compact_one_line(think.verify.as_str(), 70)
    )
}

fn latent_plan_summary(plan: &PlanBlock, raw_text: &str, latest_intent: Option<&str>) -> String {
    let mut out = format!("goal: {}", compact_one_line(plan.goal.as_str(), 120));
    if !plan.steps.is_empty() {
        let steps = plan
            .steps
            .iter()
            .take(2)
            .map(|s| compact_one_line(s, 80))
            .collect::<Vec<_>>()
            .join(" | ");
        out.push_str(&format!("; steps: {steps}"));
    }
    if !plan.acceptance_criteria.is_empty() {
        let acc = plan
            .acceptance_criteria
            .iter()
            .take(2)
            .map(|s| compact_one_line(s, 70))
            .collect::<Vec<_>>()
            .join(" | ");
        out.push_str(&format!("; acceptance: {acc}"));
    }
    let tail = compact_one_line(strip_known_latent_tags(raw_text).as_str(), 120);
    if tail != "-" {
        out.push_str(&format!("; tail: {tail}"));
    }
    if let Some(intent) = latest_intent.map(str::trim).filter(|s| !s.is_empty()) {
        out.push_str(&format!("; intent: {}", compact_one_line(intent, 120)));
    }
    out
}

fn latest_intent_anchor_baseline(messages: &[serde_json::Value]) -> Option<String> {
    messages.iter().rev().find_map(|message| {
        if message["role"].as_str() != Some("system") {
            return None;
        }
        let content = message["content"].as_str()?;
        if !content.starts_with("[Intent Anchor]") {
            return None;
        }
        content.lines().find_map(|line| {
            line.strip_prefix("baseline: ")
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(ToOwned::to_owned)
        })
    })
}

fn build_anchor_baseline(
    messages: &[serde_json::Value],
    root_user_text: &str,
    active_plan: Option<&PlanBlock>,
    working_mem: &WorkingMemory,
) -> String {
    let mut parts = vec![latest_intent_anchor_baseline(messages)
        .unwrap_or_else(|| compact_one_line(root_user_text, 220))];
    if let Some(plan) = active_plan {
        parts.push(format!(
            "active_goal: {}",
            compact_one_line(plan.goal.as_str(), 120)
        ));
    }
    if let Some(strategy) = working_mem.chosen_strategy.as_deref() {
        parts.push(format!("strategy: {}", compact_one_line(strategy, 120)));
    }
    parts.join(" | ")
}

fn latent_plan_defer_score(raw_text: &str, plan: &PlanBlock) -> f64 {
    let plan_bonus = if plan.steps.len() >= 2 { 0.45 } else { 0.35 };
    let acceptance_bonus = (plan.acceptance_criteria.len().min(2) as f64) * 0.10;
    let narrative = strip_known_latent_tags(raw_text);
    let narrative_bonus = (narrative.trim().chars().count().min(160) as f64 / 160.0) * 0.15;
    (plan_bonus + acceptance_bonus + narrative_bonus).clamp(0.0, 1.0)
}

fn latent_intent_defer_score(raw_text: &str, think: &ThinkBlock) -> f64 {
    let tool_bonus = 0.18;
    let next_bonus = if think.next.trim().is_empty() {
        0.0
    } else {
        0.16
    };
    let verify_bonus = if think.verify.trim().is_empty() {
        0.0
    } else {
        0.12
    };
    let narrative = strip_known_latent_tags(raw_text);
    let narrative_bonus = (narrative.trim().chars().count().min(120) as f64 / 120.0) * 0.10;
    (tool_bonus + next_bonus + verify_bonus + narrative_bonus).clamp(0.0, 1.0)
}

fn build_realize_pending_hint(
    cfg: &RealizeOnDemandConfig,
    latent: &LatentPlanBuffer,
    metrics: &RealizeMetrics,
    age_turns: usize,
    drift: f64,
) -> String {
    let window_state = if cfg.within_window(age_turns) {
        "within_window"
    } else {
        "deadline_near"
    };
    let lambda = cfg.lambda_for_age(age_turns);
    format!(
        "[Realize-on-Demand]\n\
There is a pending latent plan/claim buffer that is NOT committed yet.\n\
window_state: {window_state}\n\
age_turns: {age_turns}/{window_end}\n\
defer_score: {defer_score:.2}\n\
drift_metric: {metric}\n\
drift: {drift:.3}\n\
drift_penalty_lambda: {lambda:.2}\n\
mean_drift: {mean_drift:.3}\n\
summary: {summary}\n\
latest_intent: {intent}\n\
Rule: stay close to that summary. If you are ready to commit it, emit <realize>reason: why now</realize>.\n\
A non-done tool call also counts as realization. `done` does NOT; commit first, then finalize.",
        window_end = cfg.window_end,
        defer_score = latent.defer_score,
        metric = match cfg.drift_metric {
            DriftMetric::Cos => "cos",
            DriftMetric::Kl => "kl",
        },
        mean_drift = metrics.mean_drift(),
        summary = latent.summary,
        intent = latent.latest_intent.as_deref().unwrap_or("-")
    )
}

fn build_realize_banner(
    reason: &str,
    latency: usize,
    drift: f64,
    metrics: &RealizeMetrics,
) -> String {
    format!(
        "\n[realize] {reason} latency={latency} mean_latency={mean_latency:.2} drift={drift:.3} mean_drift={mean_drift:.3} missing={missing} leakage={leakage}\n",
        mean_latency = metrics.mean_realize_latency(),
        mean_drift = metrics.mean_drift(),
        missing = metrics.missing,
        leakage = metrics.early_leakage
    )
}

fn tool_call_realizes_latent(
    realize_reason: Option<&str>,
    tool_call: Option<&ToolCallData>,
) -> bool {
    if realize_reason.is_some() {
        return true;
    }
    tool_call
        .map(|tc| tc.name.as_str() != "done")
        .unwrap_or(false)
}

fn build_realize_done_gate_message(latent: &LatentPlanBuffer) -> String {
    format!(
        "[Realize Gate] Pending latent content must be committed before `done`.\n\
summary: {}\n\
latest_intent: {}\n\
Required now:\n\
- Emit <realize>reason: enough evidence to commit</realize>.\n\
- On the following turn, call `done` with the finalized summary/evidence.\n\
Do not call `done` while latent_pending=true.",
        compact_one_line(latent.summary.as_str(), 180),
        latent.latest_intent.as_deref().unwrap_or("-")
    )
}

fn build_realize_state(
    cfg: &RealizeOnDemandConfig,
    latent: Option<&LatentPlanBuffer>,
    iter: usize,
    latest_drift: Option<f64>,
    metrics: &RealizeMetrics,
) -> RealizeState {
    RealizeState {
        pending: latent.is_some(),
        age_turns: latent
            .map(|pending| iter.saturating_sub(pending.created_iter))
            .unwrap_or(0),
        window_end: cfg.window_end,
        latest_drift: if latent.is_some() { latest_drift } else { None },
        mean_drift: metrics.mean_drift(),
        mean_realize_latency: metrics.mean_realize_latency(),
        missing: metrics.missing,
        early_leakage: metrics.early_leakage,
    }
}

async fn emit_realize_state(
    tx: &mpsc::Sender<StreamToken>,
    cfg: &RealizeOnDemandConfig,
    latent: Option<&LatentPlanBuffer>,
    iter: usize,
    latest_drift: Option<f64>,
    metrics: &RealizeMetrics,
) {
    let _ = tx
        .send(StreamToken::RealizeState(build_realize_state(
            cfg,
            latent,
            iter,
            latest_drift,
            metrics,
        )))
        .await;
}

fn read_only_task_addon() -> &'static str {
    "[Read-Only Task Contract]\n\
This task is read-only inspection only.\n\
- Do NOT edit files.\n\
- Do NOT run exec/build/test/manual-behavioral checks just to finish.\n\
- Use observation tools only: list_dir, glob, search_files, read_file.\n\
- Once the file path and handling context are confirmed by successful observation commands, call done directly.\n\
- In read-only plans, acceptance should focus on locating and confirming the code path/context; meta constraints like `no files modified` are constraints, not acceptance targets."
}

// ── Tunables ─────────────────────────────────────────────────────────────────

pub const DEFAULT_MAX_ITERS: usize = 12;
const MAX_STDOUT_CHARS: usize = 1500;
const MAX_STDERR_CHARS: usize = 600;
const KEEP_RECENT_TOOL_TURNS: usize = 4;
const KEEP_RECENT_ASSISTANT_TURNS: usize = 6;
const SUCCESS_TOOL_HISTORY_MAX_LINES: usize = 10;
const SUCCESS_TOOL_HISTORY_MAX_CHARS: usize = 1200;
const TOKEN_BUDGET_WARN_TOKENS: usize = 9000;

/// Marker appended to every exec call so we can persist working directory across tool runs.
///
/// IMPORTANT: Each `exec` runs in a fresh process. Without this, `cd` is lost between calls,
/// which causes nested-git disasters and "why did it run in the repo root?" failures.
const PWD_MARKER: &str = "__SPIRAL_CODER_PWD__=";

// ── System prompt addons ──────────────────────────────────────────────────────

/// Injected for Windows PowerShell environments.
const WIN_SYSTEM_ADDON: &str = "\n\n[Windows execution rules]\n\
- You are on Windows. Use PowerShell syntax.\n\
- NEVER use here-strings (@' or @\"). Write files line-by-line with Set-Content / Add-Content, or use Out-File.\n\
- Use single-line commands or semicolons to chain statements.\n\
- Check $LASTEXITCODE after commands that may fail.\n\
- Prefer relative paths inside the project directory.";

// ── Tool definition ───────────────────────────────────────────────────────────

pub fn exec_tool_def() -> serde_json::Value {
    json!({
        "type": "function",
        "function": {
            "name": "exec",
            "description": "Execute a shell command on the local machine and return stdout/stderr/exit_code.\n\
                            On Windows: PowerShell.  On Linux/macOS: sh.\n\
                            ALWAYS check exit_code — 0 = success, non-zero = failure.\n\
                            If exit_code != 0: STOP, diagnose the error, then fix it.\n\
                            Do NOT proceed to the next step while any command is failing.",
            "parameters": {
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "The shell command to execute. Single-line preferred."
                    },
                    "cwd": {
                        "type": "string",
                        "description": "Optional working directory (absolute or relative path)."
                    }
                },
                "required": ["command"]
            }
        }
    })
}

pub fn read_file_tool_def() -> serde_json::Value {
    json!({
        "type": "function",
        "function": {
            "name": "read_file",
            "description": "Read the full content of a file. Use before editing to see the exact current text. \
                            Path is relative to tool_root (or absolute within tool_root). \
                            Large files are truncated automatically.",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "File path relative to tool_root."
                    }
                },
                "required": ["path"]
            }
        }
    })
}

pub fn write_file_tool_def() -> serde_json::Value {
    json!({
        "type": "function",
        "function": {
            "name": "write_file",
            "description": "Atomically create or overwrite a file with the given content. \
                            Creates parent directories automatically. \
                            More reliable than exec+echo for file creation (handles encoding, special chars, newlines). \
                            Path is relative to tool_root.",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "File path relative to tool_root."
                    },
                    "content": {
                        "type": "string",
                        "description": "Complete file content to write."
                    }
                },
                "required": ["path", "content"]
            }
        }
    })
}

pub fn patch_file_tool_def() -> serde_json::Value {
    json!({
        "type": "function",
        "function": {
            "name": "patch_file",
            "description": "Edit a file by replacing an exact text snippet. \
                            The search string MUST appear exactly once in the file. \
                            Call read_file first to see the exact current text if unsure. \
                            For whole-file rewrites use write_file instead. \
                            Path is relative to tool_root.",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "File path relative to tool_root."
                    },
                    "search": {
                        "type": "string",
                        "description": "Exact text to find (must match character-for-character, including whitespace and newlines)."
                    },
                    "replace": {
                        "type": "string",
                        "description": "Text to substitute in place of the search string."
                    }
                },
                "required": ["path", "search", "replace"]
            }
        }
    })
}

pub fn search_files_tool_def() -> serde_json::Value {
    json!({
        "type": "function",
        "function": {
            "name": "search_files",
            "description": "Search file contents for a literal text pattern (like grep -rn). \
                            Returns matching lines as 'file:line: content'. \
                            PREFER this over exec+grep — it is faster, safer, and token-efficient. \
                            Use to find function definitions, TODO items, error strings, or any \
                            pattern across the codebase. Dir is relative to tool_root.",
            "parameters": {
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "Literal text to search for (not a regex)."
                    },
                    "dir": {
                        "type": "string",
                        "description": "Subdirectory to search in (relative to tool_root). \
                                        Omit or leave empty to search all files under tool_root."
                    },
                    "case_insensitive": {
                        "type": "boolean",
                        "description": "If true, search ignores case. Default: false."
                    }
                },
                "required": ["pattern"]
            }
        }
    })
}

pub fn apply_diff_tool_def() -> serde_json::Value {
    json!({
        "type": "function",
        "function": {
            "name": "apply_diff",
            "description": "Apply a unified diff to a file. More reliable than patch_file for \
                            complex edits that span many lines or have multiple hunks. \
                            Use standard @@ unified diff format. Each hunk is matched by \
                            content (context + remove lines), so exact line numbers are not required. \
                            Multiple hunks per call are supported. \
                            ALWAYS include 2-3 context lines around changes for reliable matching.",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "File path relative to tool_root."
                    },
                    "diff": {
                        "type": "string",
                        "description": "Unified diff string with @@ hunks. Example:\n@@ -10,5 +10,6 @@\n context\n-old line\n+new line\n context"
                    }
                },
                "required": ["path", "diff"]
            }
        }
    })
}

pub fn list_dir_tool_def() -> serde_json::Value {
    json!({
        "type": "function",
        "function": {
            "name": "list_dir",
            "description": "List a directory (non-recursive). Use this to quickly discover the project structure.\n\
                            Prefer this over glob when you don't know the exact pattern yet.\n\
                            Dir is relative to tool_root. If dir is empty, list tool_root/current directory.",
            "parameters": {
                "type": "object",
                "properties": {
                    "dir": {
                        "type": "string",
                        "description": "Directory to list (relative to tool_root). Empty = tool_root/current directory."
                    },
                    "max_entries": {
                        "type": "integer",
                        "description": "Max entries to return. Default: 200. Max: 500."
                    },
                    "include_hidden": {
                        "type": "boolean",
                        "description": "Include dotfiles/directories. Default: false."
                    }
                }
            }
        }
    })
}

pub fn glob_tool_def() -> serde_json::Value {
    json!({
        "type": "function",
        "function": {
            "name": "glob",
            "description": "Find files by name/path pattern (like find -name). \
                            Supports * (single component), ** (any depth), ? (single char). \
                            Examples: '**/*.rs', 'src/*.ts', 'test_*'. \
                            Returns relative paths sorted alphabetically. \
                            PREFER this over exec+find/ls — OS-agnostic and token-efficient.",
            "parameters": {
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "Glob pattern, e.g. '**/*.rs' or 'src/*.ts'"
                    },
                    "dir": {
                        "type": "string",
                        "description": "Subdirectory to search in (relative to tool_root). \
                                        Omit to search all of tool_root."
                    }
                },
                "required": ["pattern"]
            }
        }
    })
}

pub fn done_tool_def() -> serde_json::Value {
    let acceptance_evidence_required = governor_contract::done_acceptance_evidence_fields();
    let required_args = governor_contract::done_required_args();
    json!({
        "type": "function",
        "function": {
            "name": "done",
            "description": "Signal that the task is complete and end the agent loop. \
                            Use only after verifying with commands/tests.",
            "parameters": {
                "type": "object",
                "properties": {
                    "summary": {
                        "type": "string",
                        "description": "Brief [DONE] summary: what was built/changed and where it lives."
                    },
                    "completed_acceptance": {
                        "type": "array",
                        "description": "Current plan acceptance criteria that are already satisfied. Cite by number and/or criterion text.",
                        "items": {
                            "type": "string"
                        }
                    },
                    "remaining_acceptance": {
                        "type": "array",
                        "description": "Current plan acceptance criteria that are still not satisfied. Must cover the rest of the current plan criteria.",
                        "items": {
                            "type": "string"
                        }
                    },
                    "acceptance_evidence": {
                        "type": "array",
                        "description": "For each completed acceptance criterion, cite the real verification command that proved it.",
                        "items": {
                            "type": "object",
                            "properties": {
                                "criterion": {
                                    "type": "string",
                                    "description": "Acceptance criterion reference by number and/or text."
                                },
                                "command": {
                                    "type": "string",
                                    "description": "Verification command that already succeeded for this criterion."
                                }
                            },
                            "required": acceptance_evidence_required
                        }
                    },
                    "next_steps": {
                        "type": "string",
                        "description": "How to run/verify, or any follow-up work."
                    }
                },
                "required": required_args
            }
        }
    })
}

// ── System prompt builders ────────────────────────────────────────────────────

/// Fixed base for the TUI Coder pane — always an agentic executor, not a chat bot.
const CODER_BASE_SYSTEM: &str = "\
You are an autonomous coding agent with 9 tools:\n\
  exec(command, cwd?)                       — run shell commands (build, test, git, installs)\n\
  read_file(path)                           — read a file's exact content\n\
  write_file(path, content)                 — create or overwrite a file reliably\n\
  patch_file(path, search, replace)         — replace an exact snippet in a file\n\
  apply_diff(path, diff)                    — apply a unified @@ diff (multiple hunks)\n\
  search_files(pattern, dir?, ci?)          — find text across files (like grep -rn)\n\
  list_dir(dir?, max_entries?, include_hidden?) — list a directory (non-recursive)\n\
  glob(pattern, dir?)                       — find files by name pattern (like find -name)\n\
  done(summary, completed_acceptance, remaining_acceptance, acceptance_evidence, next_steps?)\n\
                                           — finish the task and end the loop\n\
\n\
RULE: You MUST call a tool on every single turn. Never respond with text only.\n\
\n\
Fallback for non-tool models (implied actions):\n\
  - To create a NEW file, output this exact pattern:\n\
      File: path/to/file.ext\n\
      ```lang\n\
      file contents...\n\
      ```\n\
    (Spiral-Coder may auto-write the file with approval; it will NOT overwrite existing files.)\n\
  - To run commands, paste a PowerShell/bash code fence.\n\
\n\
PRIORITY (safety-first default when unsure):\n\
  1) read_file                  — before editing any existing file\n\
  2) list_dir/glob/search_files  — discover files (prefer list_dir for quick structure)\n\
  3) patch_file/apply_diff       — prefer for edits (less destructive)\n\
  4) write_file                  — ONLY for new files, or after read_file if overwriting\n\
  5) exec                        — build/test/git/install; avoid for file I/O\n\
\n\
Plan enforcement:\n\
  - Every tool call MUST map to a step in your <plan>. If not, update <plan> first.\n\
  - Your <plan> MUST include explicit `acceptance:` criteria. Verification must satisfy them.\n\
  - __INSTRUCTION_RESOLVER_SCRATCHPAD_RULE__\n\
  - For existing-file mutation, emit <evidence> first; if evidence is weak, inspect instead of mutating.\n\
\n\
Choose the right tool:\n\
  Quick directory listing  → list_dir      (structure discovery, low token)\n\
  List files by pattern    → glob          (NOT exec+ls/find; OS-agnostic, token-efficient)\n\
  Find text in files       → search_files  (NOT exec+grep; safer, token-efficient)\n\
  Create/overwrite file    → write_file    (handles encoding; more reliable than exec+echo)\n\
  Small targeted edit      → read_file → patch_file  (simple single-snippet replacement)\n\
  Complex multi-hunk edit  → read_file → apply_diff  (multiple changes, spans many lines)\n\
  Run programs/tests       → exec\n\
  Git / installs           → exec\n\
\n\
apply_diff format (include 2-3 context lines for reliable matching):\n\
  @@ -10,4 +10,5 @@\n\
   context line\n\
  -old line to remove\n\
  +new line to add\n\
   context line\n\
\n\
After every file edit: tests run automatically if configured — check the result.\n\
After every build/test: confirm exit_code == 0 before proceeding.\n\
\n\
When ALL steps from your <plan> are verified complete:\n\
  call exec one final time to run a smoke test or confirm the deliverable exists,\n\
  then call done with: (1) a brief summary, (2) which acceptance criteria are satisfied,\n\
  (3) which acceptance criteria remain, if any, and (4) the exact verification command used for each completed criterion.\n\
  Do NOT call done while any command is still failing.";

fn realize_on_demand_addon(preset: Option<RealizePreset>) -> Option<String> {
    let cfg = RealizeOnDemandConfig::resolve(preset);
    if !cfg.enabled {
        return None;
    }
    let preset_label = match preset {
        Some(preset) => preset.label(),
        None => "env",
    };
    Some(format!(
        "Realize-on-demand (experimental) is enabled.\n\
Plan-bearing no-tool turns may be held latent before they are committed.\n\
- If you want to commit a pending latent plan/claim explicitly, emit <realize>reason: why now</realize>.\n\
- A non-done tool call also counts as realization.\n\
- `done` requires latent content to be committed first.\n\
- Stay close to the latest latent summary while it is pending; wandering raises drift.\n\
- Session preset: {}.\n\
- Current defaults: defer_threshold={:.2}, realization_window={}..{}, drift_metric={}, lambda={}..{}.",
        preset_label,
        cfg.defer_threshold,
        cfg.window_start,
        cfg.window_end,
        match cfg.drift_metric {
            DriftMetric::Cos => "cos",
            DriftMetric::Kl => "kl",
        },
        cfg.lambda_min,
        cfg.lambda_max
    ))
}

fn has_in_path(cmd: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    let exts: Vec<String> = if cfg!(target_os = "windows") {
        let pathext =
            std::env::var("PATHEXT").unwrap_or_else(|_| ".EXE;.CMD;.BAT;.COM".to_string());
        pathext
            .split(';')
            .map(|e| e.trim())
            .filter(|e| !e.is_empty())
            .map(|e| e.to_ascii_lowercase())
            .collect()
    } else {
        vec![String::new()]
    };
    for dir in std::env::split_paths(&path) {
        if cfg!(target_os = "windows") {
            for ext in &exts {
                let name = if ext.is_empty() {
                    cmd.to_string()
                } else if cmd.to_ascii_lowercase().ends_with(ext) {
                    cmd.to_string()
                } else {
                    format!("{cmd}{ext}")
                };
                if dir.join(&name).is_file() {
                    return true;
                }
            }
        } else if dir.join(cmd).is_file() {
            return true;
        }
    }
    false
}

fn host_capability_addon() -> String {
    let os = if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else {
        "unknown"
    };

    let bash = has_in_path("bash");
    let sh = has_in_path("sh");
    let shell = if cfg!(target_os = "windows") {
        "powershell"
    } else if bash {
        "bash"
    } else if sh {
        "sh"
    } else {
        "unknown"
    };

    let mut has: Vec<&'static str> = Vec::new();
    if has_in_path("git") {
        has.push("git");
    }
    if has_in_path("rg") {
        has.push("rg");
    }
    if has_in_path("node") {
        has.push("node");
    }
    if has_in_path("python3") {
        has.push("python3");
    } else if has_in_path("python") {
        has.push("python");
    }
    if has_in_path("cargo") {
        has.push("cargo");
    }
    if has_in_path("npm") {
        has.push("npm");
    }

    let has_list = if has.is_empty() {
        "(none detected)".to_string()
    } else {
        has.join(", ")
    };

    format!(
        "[Host Capability Probe]\n\
os: {os}\n\
exec_shell: {shell}\n\
tools_in_path: {has_list}\n\
Rules:\n\
- If os != windows: do NOT use PowerShell syntax.\n\
- Prefer built-in tools (list_dir/search_files/glob) over exec-based discovery.\n\
- Use bash-compatible syntax only when exec_shell=bash."
    )
}

/// Build the full Coder system prompt: base + scratchpad + OS rules + persona + language.
pub fn coder_system(
    persona_prompt: &str,
    lang_instruction: &str,
    realize_preset: Option<RealizePreset>,
) -> String {
    let mut s = CODER_BASE_SYSTEM.to_string();
    s = s.replace(
        "__INSTRUCTION_RESOLVER_SCRATCHPAD_RULE__",
        governor_contract::instruction_resolver_scratchpad_rule_message().as_str(),
    );
    s.push_str(&governor_contract::scratchpad_addon());
    s.push_str("\n\n");
    s.push_str(&host_capability_addon());
    if let Some(addon) = realize_on_demand_addon(realize_preset) {
        s.push_str("\n\n");
        s.push_str(&addon);
    }
    if cfg!(target_os = "windows") {
        s.push_str(WIN_SYSTEM_ADDON);
    }
    if !persona_prompt.is_empty() {
        s.push_str("\n\n");
        s.push_str(persona_prompt);
    }
    if !lang_instruction.is_empty() {
        s.push_str("\n\n");
        s.push_str(lang_instruction);
    }
    s
}

// ── Token-efficient helpers ───────────────────────────────────────────────────

/// Trim output that exceeds `max_chars`, appending a truncation notice.
/// This is the biggest single token-saver: a `cargo build` or `ls -R` can
/// return tens of thousands of characters; we only need the key lines.
fn truncate_output(s: &str, max_chars: usize) -> String {
    let s = s.trim_end();
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    let truncated: String = s.chars().take(max_chars).collect();
    let total_lines = s.lines().count();
    format!("{truncated}\n[…truncated — {total_lines} lines total, first {max_chars} chars shown]")
}

/// Tail-biased truncation: errors appear at the END of compiler output, not the beginning.
/// E.g. `cargo build` prints 100+ "Compiling …" lines before the actual error.
/// Showing the last `max_chars` puts the real error in view.
fn truncate_output_tail(s: &str, max_chars: usize) -> String {
    let s = s.trim_end();
    let char_count = s.chars().count();
    if char_count <= max_chars {
        return s.to_string();
    }
    let skip = char_count - max_chars;
    let tail: String = s.chars().skip(skip).collect();
    let total_lines = s.lines().count();
    let shown_lines = tail.lines().count();
    let hidden = total_lines.saturating_sub(shown_lines);
    format!("[…{hidden} earlier lines omitted — showing last {max_chars} chars]\n{tail}")
}

/// Line- and char-bounded preview truncation (used for approval prompts).
fn truncate_preview(s: &str, max_chars: usize, max_lines: usize) -> String {
    let mut out = String::new();
    let mut lines = 0usize;
    for line in s.lines() {
        if lines >= max_lines {
            out.push_str("\n...truncated...\n");
            break;
        }
        if out.len() + line.len() + 1 > max_chars {
            out.push_str("\n...truncated...\n");
            break;
        }
        out.push_str(line);
        out.push('\n');
        lines += 1;
    }
    out.trim_end().to_string()
}

fn compact_one_line(s: &str, max_chars: usize) -> String {
    let flat = s
        .replace("\r\n", "\n")
        .lines()
        .collect::<Vec<_>>()
        .join(" ");
    let collapsed = flat.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut out = String::new();
    let mut n = 0usize;
    for ch in collapsed.chars() {
        if n >= max_chars {
            out.push('…');
            break;
        }
        out.push(ch);
        n += 1;
    }
    if out.is_empty() {
        "-".to_string()
    } else {
        out
    }
}

fn compact_multiline_block(s: &str) -> String {
    s.replace("\r\n", "\n")
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn approx_tokens_text(s: &str) -> usize {
    // Rough heuristic:
    // - ASCII-ish text: ~4 chars/token
    // - Non-ASCII (CJK etc): closer to 1 char/token
    let mut ascii = 0usize;
    let mut non_ascii = 0usize;
    for ch in s.chars() {
        if ch.is_ascii() {
            ascii += 1;
        } else {
            non_ascii += 1;
        }
    }
    (ascii + 3) / 4 + non_ascii
}

fn approx_tokens_messages(messages: &[serde_json::Value]) -> usize {
    messages
        .iter()
        .map(|m| {
            m.get("content")
                .and_then(|c| c.as_str())
                .map(approx_tokens_text)
                .unwrap_or(0)
        })
        .sum()
}

fn simple_before_after(old: &str, new: &str) -> String {
    if old == new {
        return "(no changes)".to_string();
    }
    let old_p = truncate_preview(old, 1400, 60);
    let new_p = truncate_preview(new, 1400, 60);
    format!("--- before ---\n{old_p}\n\n--- after ---\n{new_p}\n")
        .trim_end()
        .to_string()
}

/// Extract a compact digest of error lines from command output.
/// Helps the model see ALL errors even when stdout is very long.
/// Returns None when no clear error lines are found.
fn extract_error_digest(stdout: &str, stderr: &str) -> Option<String> {
    let patterns: &[&str] = &[
        "error[e",         // Rust: error[E0XXX]
        "error: aborting", // Rust: summary line
        " --> ",           // Rust: file:line pointer
        "syntaxerror:",    // Python / JS
        "typeerror:",
        "nameerror:",
        "attributeerror:",
        "valueerror:",
        "runtimeerror:",
        "importerror:",
        "modulenotfounderror:",
        "referenceerror:", // JS
        "traceback (most recent call last)",
        "error: ", // generic (space avoids false positives)
        "fatal: ",
        "fatal error:",
    ];

    let mut lines: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for src in [stderr, stdout] {
        for line in src.lines() {
            let t = line.trim();
            if t.is_empty() {
                continue;
            }
            let low = t.to_ascii_lowercase();
            if patterns.iter().any(|p| low.contains(p)) {
                if seen.insert(t.to_string()) {
                    lines.push(t.to_string());
                    if lines.len() >= 20 {
                        break;
                    }
                }
            }
        }
        if lines.len() >= 20 {
            break;
        }
    }

    if lines.is_empty() {
        return None;
    }
    Some(format!(
        "[ERROR DIGEST — {} line(s)]\n{}",
        lines.len(),
        lines.join("\n")
    ))
}

/// Returns true if a tool result can safely be pruned (= it was a success).
/// Failures are never pruned because they are critical context for recovery.
fn is_prunable_tool_result(content: &str) -> bool {
    let c = content.trim_start();
    // exec success
    c.starts_with("OK (exit_code: 0)")
        // write_file / patch_file success
        || c.starts_with("OK: wrote '")
        || c.starts_with("OK: patched '")
        || c.starts_with("OK: applied ")
        // read_file success — header starts with "[path] (N lines"
        || (c.starts_with('[') && c.contains("] (") && c.contains(" lines,"))
        || c.starts_with("[search_files:")
        || c.starts_with("[list_dir:")
        || c.starts_with("[glob")
}

fn push_unique_tool_line(
    out: &mut Vec<String>,
    seen: &mut std::collections::HashSet<String>,
    line: &str,
) {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return;
    }
    let compact = compact_one_line(trimmed, 220);
    if seen.insert(compact.clone()) {
        out.push(compact);
    }
}

fn compact_success_tool_result_for_history(tool_name: &str, content: &str) -> String {
    if matches!(tool_name, "read_file") {
        return content.to_string();
    }
    if content.len() <= SUCCESS_TOOL_HISTORY_MAX_CHARS
        && content.lines().count() <= SUCCESS_TOOL_HISTORY_MAX_LINES
    {
        return content.to_string();
    }

    let lines: Vec<&str> = content.lines().collect();
    if lines.is_empty() {
        return content.to_string();
    }

    let mut kept = Vec::new();
    let mut seen = std::collections::HashSet::new();
    push_unique_tool_line(&mut kept, &mut seen, lines[0]);

    // The first runtime status is authoritative, even when later stdout prints
    // another status. Preserve it verbatim before applying the digest budget.
    if crate::execution_evidence::file_edit_succeeded(tool_name, content) {
        if let Some(status) = lines.iter().find(|line| line.starts_with("[auto-test]")) {
            seen.insert((*status).to_string());
            kept.push((*status).to_string());
        }
    }

    if tool_name == "exec" {
        for line in lines.iter().skip(1).take(3) {
            push_unique_tool_line(&mut kept, &mut seen, line);
        }
    }

    let marker_patterns = [
        "[auto-test]",
        "[hash]",
        "PASSED (exit 0)",
        "FAILED (exit ",
        "✓ auto-verify",
        "✗ auto-verify",
        "test result:",
        "Finished ",
        "running ",
        "cwd:",
        "cwd_after:",
    ];
    for line in &lines {
        let trimmed = line.trim();
        if marker_patterns.iter().any(|pat| trimmed.contains(pat)) {
            push_unique_tool_line(&mut kept, &mut seen, trimmed);
        }
        if kept.len() >= SUCCESS_TOOL_HISTORY_MAX_LINES {
            break;
        }
    }

    let fill_from = match tool_name {
        "search_files" | "list_dir" | "glob" => 1,
        "write_file" | "patch_file" | "apply_diff" => 1,
        _ => 2,
    };
    for line in lines.iter().skip(fill_from) {
        push_unique_tool_line(&mut kept, &mut seen, line);
        if kept.len() >= SUCCESS_TOOL_HISTORY_MAX_LINES {
            break;
        }
    }

    let kept_lines = kept.len();
    let total_lines = lines.len();
    if kept_lines < total_lines {
        kept.insert(
            1.min(kept.len()),
            format!(
                "[history digest — kept {}/{} lines, {} chars]",
                kept_lines,
                total_lines,
                content.len()
            ),
        );
    }
    kept.join("\n")
}

/// Collapse tool result messages older than KEEP_RECENT_TOOL_TURNS to a
/// compact summary, preserving the first automatic-test status if present.
/// Each collapsed result saves ~200-2000 tokens.
/// Only the content field is modified; tool_call_id stays intact.
fn prune_old_tool_results(messages: &mut Vec<serde_json::Value>) {
    let tool_indices: Vec<usize> = messages
        .iter()
        .enumerate()
        .filter(|(_, m)| m["role"].as_str() == Some("tool"))
        .map(|(i, _)| i)
        .collect();

    if tool_indices.len() <= KEEP_RECENT_TOOL_TURNS {
        return;
    }

    let prune_count = tool_indices.len() - KEEP_RECENT_TOOL_TURNS;
    for &idx in tool_indices.iter().take(prune_count) {
        if let Some(content) = messages[idx]["content"].as_str() {
            // Never prune failures: they are the most important context for recovery.
            // Prune successful exec outputs and file tool outputs.
            if !is_prunable_tool_result(content) {
                continue;
            }
            let line_count = content.lines().count();
            if line_count > 2 {
                let first = content.lines().next().unwrap_or("[done]").to_string();
                let mut summary = format!("{first} [pruned {line_count}L]");
                if ["write_file", "patch_file", "apply_diff"]
                    .iter()
                    .any(|name| crate::execution_evidence::file_edit_succeeded(name, content))
                {
                    if let Some(status) =
                        content.lines().find(|line| line.starts_with("[auto-test]"))
                    {
                        summary.push('\n');
                        summary.push_str(status);
                    }
                }
                messages[idx]["content"] = serde_json::Value::String(summary);
            }
        }
    }
}

fn assistant_message_is_compactable(msg: &serde_json::Value) -> bool {
    if msg.get("role").and_then(|v| v.as_str()) != Some("assistant") {
        return false;
    }
    let content = msg.get("content").and_then(|v| v.as_str()).unwrap_or("");
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return false;
    }
    if trimmed.starts_with("[DONE]")
        || trimmed.contains("[error]")
        || trimmed.contains("GOVERNOR BLOCKED")
    {
        return false;
    }
    msg.get("tool_calls")
        .and_then(|v| v.as_array())
        .map(|items| !items.is_empty())
        .unwrap_or(false)
        || parse_plan_block(trimmed).is_some()
        || parse_think_block(trimmed).is_some()
        || parse_reflection_block(trimmed).is_some()
        || parse_impact_block(trimmed).is_some()
        || parse_evidence_block(trimmed).is_some()
}

fn summarize_assistant_message(msg: &serde_json::Value) -> Option<String> {
    let content = msg.get("content").and_then(|v| v.as_str()).unwrap_or("");
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut parts = Vec::new();
    if let Some(plan) = parse_plan_block(trimmed) {
        parts.push(format!(
            "plan goal={} steps={} acceptance={}",
            compact_one_line(plan.goal.as_str(), 90),
            plan.steps.len(),
            plan.acceptance_criteria.len()
        ));
    }
    if let Some(think) = parse_think_block(trimmed) {
        parts.push(format!(
            "think step={} tool={} next={}",
            think.step,
            compact_one_line(think.tool.as_str(), 24),
            compact_one_line(think.next.as_str(), 90)
        ));
    }
    if let Some(reflect) = parse_reflection_block(trimmed) {
        parts.push(format!(
            "reflect delta={} strategy={} next={}",
            reflect.goal_delta.as_str(),
            reflect.strategy_change.as_str(),
            compact_one_line(reflect.next_minimal_action.as_str(), 90)
        ));
    }
    if let Some(impact) = parse_impact_block(trimmed) {
        parts.push(format!(
            "impact progress={} gap={}",
            compact_one_line(impact.progress.as_str(), 90),
            compact_one_line(impact.remaining_gap.as_str(), 90)
        ));
    }
    if let Some(evidence) = parse_evidence_block(trimmed) {
        parts.push(format!(
            "evidence files={} next_probe={}",
            evidence.target_files.len(),
            compact_one_line(evidence.next_probe.as_str(), 90)
        ));
    }
    if let Some(tool_calls) = msg.get("tool_calls").and_then(|v| v.as_array()) {
        let names = tool_calls
            .iter()
            .filter_map(|tc| {
                tc.get("function")
                    .and_then(|f| f.get("name"))
                    .and_then(|v| v.as_str())
                    .map(str::trim)
                    .filter(|name| !name.is_empty())
                    .map(ToString::to_string)
            })
            .collect::<Vec<_>>();
        if !names.is_empty() {
            parts.push(format!("tools={}", names.join(",")));
        }
    }
    if parts.is_empty() {
        parts.push(compact_one_line(trimmed, 140));
    }
    Some(format!(
        "[assistant-summary] {} [compacted]",
        parts.join(" | ")
    ))
}

fn prune_old_assistant_messages(messages: &mut Vec<serde_json::Value>) {
    let assistant_indices: Vec<usize> = messages
        .iter()
        .enumerate()
        .filter(|(_, msg)| msg.get("role").and_then(|v| v.as_str()) == Some("assistant"))
        .map(|(idx, _)| idx)
        .collect();

    if assistant_indices.len() <= KEEP_RECENT_ASSISTANT_TURNS {
        return;
    }

    let prune_count = assistant_indices.len() - KEEP_RECENT_ASSISTANT_TURNS;
    for &idx in assistant_indices.iter().take(prune_count) {
        if !assistant_message_is_compactable(&messages[idx]) {
            continue;
        }
        let Some(summary) = summarize_assistant_message(&messages[idx]) else {
            continue;
        };
        messages[idx]["content"] = serde_json::Value::String(summary);
    }
}

// ── Error classification ──────────────────────────────────────────────────────

/// Classifies the kind of error from stderr/stdout so that targeted recovery
/// hints can be injected before the generic diagnosis protocol.
#[derive(Debug, Clone, PartialEq)]
enum ErrorClass {
    Environment, // command not found, permission denied, not recognized
    Syntax,      // parse error, unexpected token, syntax error
    Path,        // no such file, cannot find path, path not found
    Dependency,  // module not found, missing package/crate
    Network,     // connection refused, timeout, could not connect
    Logic,       // assertion failed, test failed, expected X got Y
    Unknown,
}

impl Default for ErrorClass {
    fn default() -> Self {
        ErrorClass::Unknown
    }
}

fn classify_error(stderr: &str, stdout: &str) -> ErrorClass {
    let combined = format!("{stderr}\n{stdout}");
    let low = combined.to_ascii_lowercase();

    if low.contains("command not found")
        || low.contains("is not recognized as the name")
        || low.contains("is not recognized as an internal")
        || low.contains("permission denied")
        || low.contains("access is denied")
        || low.contains("access denied")
        || low.contains("commandnotfoundexception")
    {
        ErrorClass::Environment
    } else if low.contains("syntax error")
        || low.contains("unexpected token")
        || low.contains("parse error")
        || low.contains("parsererror")
        || low.contains("invalid syntax")
        || low.contains("missing expression")
        || low.contains("unexpected end of")
    {
        ErrorClass::Syntax
    } else if low.contains("no such file")
        || low.contains("cannot find path")
        || low.contains("path not found")
        || (low.contains("does not exist") && !low.contains("package"))
    {
        ErrorClass::Path
    } else if low.contains("modulenotfounderror")
        || low.contains("cannot find module")
        || low.contains("no module named")
        || low.contains("package not found")
        || low.contains("no such package")
        || (low.contains("could not find") && (low.contains("package") || low.contains("crate")))
    {
        ErrorClass::Dependency
    } else if low.contains("connection refused")
        || low.contains("timed out")
        || low.contains("network unreachable")
        || low.contains("could not connect")
        || low.contains("name resolution failed")
        || low.contains("could not resolve host")
        || low.contains("couldn't resolve host")
        || low.contains("temporary failure in name resolution")
    {
        ErrorClass::Network
    } else if low.contains("assertion")
        || low.contains("test failed")
        || (low.contains("expected") && low.contains("actual"))
    {
        ErrorClass::Logic
    } else {
        ErrorClass::Unknown
    }
}

/// Returns a targeted one-line recovery hint for each error class,
/// or an empty string for Unknown (no hint injected).
fn error_class_hint(class: &ErrorClass) -> &'static str {
    match class {
        ErrorClass::Environment =>
            "⚠ ENVIRONMENT ERROR: The binary/permission is missing. Fix the environment first — do NOT modify source code.",
        ErrorClass::Syntax =>
            "⚠ SYNTAX ERROR: There is a language/parser mistake. Fix the exact line — do NOT change unrelated code.",
        ErrorClass::Path => {
            if cfg!(target_os = "windows") {
                "⚠ PATH ERROR: A file or directory is missing. Verify paths with Get-ChildItem/dir before creating or reading."
            } else {
                "⚠ PATH ERROR: A file or directory is missing. Verify paths with ls before creating or reading."
            }
        }
        ErrorClass::Dependency =>
            "⚠ DEPENDENCY ERROR: A required package is missing. Install it first, then retry the original command.",
        ErrorClass::Network =>
            "⚠ NETWORK ERROR: Cannot reach a remote service. Check if the service is running and proxy vars are clear.",
        ErrorClass::Logic =>
            "⚠ LOGIC ERROR: The code ran but produced wrong results. Re-read the relevant logic before re-running.",
        ErrorClass::Unknown => "",
    }
}

/// Extra targeted hints for known high-frequency Windows/Git failure modes.
fn specific_recovery_hint(stderr: &str, stdout: &str) -> &'static str {
    let combined = format!("{stderr}\n{stdout}");
    let low = combined.to_ascii_lowercase();

    // GitHub HTTPS routed via a dead local proxy (common in locked-down networks).
    // Example: "Failed to connect to github.com port 443 via 127.0.0.1 ... Could not connect to server"
    let git_github_proxy = (low.contains("github.com") || low.contains("ssh.github.com"))
        && low.contains("port 443")
        && (low.contains("via 127.0.0.1") || low.contains("via localhost"))
        && (low.contains("could not connect") || low.contains("failed to connect"));
    if git_github_proxy {
        return "HINT: Git HTTPS appears to be routed via a dead local proxy (127.0.0.1/localhost).\n\
- Try clearing proxy env vars: HTTP_PROXY / HTTPS_PROXY / ALL_PROXY / GIT_HTTP_PROXY / GIT_HTTPS_PROXY.\n\
- For GitHub, prefer SSH-over-443:\n\
  git push ssh://git@ssh.github.com:443/<owner>/<repo>.git main\n\
  (and similarly for fetch/pull).";
    }

    let dns_failure = low.contains("could not resolve host")
        || low.contains("couldn't resolve host")
        || low.contains("temporary failure in name resolution")
        || low.contains("name resolution failed");
    if dns_failure && (low.contains("crates.io") || low.contains("index.crates.io")) {
        return "HINT: Cargo could not reach crates.io/index (DNS/network failure).\n\
- If this environment is offline/sandboxed, you cannot download new crates; stop retrying `cargo` in a loop.\n\
- Try `cargo ... --offline` only if dependencies are already cached.\n\
- Otherwise proceed with static edits and defer builds/tests to a networked environment.";
    }
    if dns_failure {
        return "HINT: Network/DNS failure (host name resolution failed).\n\
- Stop retrying the same command.\n\
- Check connectivity and clear proxy env vars if needed (HTTP_PROXY/HTTPS_PROXY/ALL_PROXY).\n\
- If running in an offline sandbox, installs/downloads will not work; switch to an offline strategy.";
    }

    // Windows: `cargo run` cannot overwrite a running .exe (locked file handle).
    // Example: "failed to remove file ... spiral-coder.exe ... access is denied (os error 5)"
    let cargo_exe_lock = low.contains("failed to remove file")
        && low.contains("spiral-coder.exe")
        && (low.contains("os error 5") || low.contains("access is denied"));
    if cargo_exe_lock {
        return "HINT: On Windows, a running .exe cannot be overwritten.\n\
- Stop the running process (`Stop-Process -Name spiral-coder -Force`) OR restart the terminal.\n\
- Or run cargo with an isolated target dir to avoid the lock:\n\
  $env:CARGO_TARGET_DIR = '.tmp/cargo-target-tui'; cargo run -- tui";
    }

    ""
}

// ── Tool output builders ──────────────────────────────────────────────────────

/// Build the tool result string for a failed command with structured
/// diagnosis guidance.  Forces the model to reason about the error rather
/// than blindly retrying or continuing.
fn build_failed_tool_output(stdout: &str, stderr: &str, exit_code: i32) -> String {
    let class = classify_error(stderr, stdout);

    let specific_hint = specific_recovery_hint(stderr, stdout);
    let class_hint = error_class_hint(&class);

    let mut hint_prefix = String::new();
    if !specific_hint.is_empty() {
        hint_prefix.push_str(specific_hint);
        hint_prefix.push_str("\n\n");
    }
    if !class_hint.is_empty() {
        hint_prefix.push_str(class_hint);
        hint_prefix.push_str("\n\n");
    }
    let mut out = format!(
        "{hint_prefix}FAILED (exit_code: {exit_code})\n\
         \n\
         ⚠ STOP — diagnosis required before your next action:\n\
         1. Quote the exact line causing the error.\n\
         2. Identify the root cause in one sentence.\n\
         3. Fix it with a single corrected command.\n\
         Do NOT continue the original plan until the fix succeeds.\n"
    );

    // Error digest first: compact list of all error lines so the model sees
    // ALL errors even when stdout is long (e.g. cargo build with many "Compiling" lines).
    if let Some(digest) = extract_error_digest(stdout, stderr) {
        out.push_str(&format!("\n{digest}\n"));
    }

    // Tail-biased: errors appear at the END of compiler output, not the beginning.
    // Showing the last N chars ensures the real errors are in view.
    let stdout_t = truncate_output_tail(stdout, MAX_STDOUT_CHARS);
    let stderr_t = truncate_output(stderr, MAX_STDERR_CHARS);
    if !stdout_t.is_empty() {
        out.push_str(&format!("\nstdout (tail):\n{stdout_t}\n"));
    }
    if !stderr_t.is_empty() {
        out.push_str(&format!("\nstderr:\n{stderr_t}\n"));
    }
    out
}

fn build_ok_tool_output(stdout: &str) -> String {
    let stdout_t = truncate_output(stdout, MAX_STDOUT_CHARS);
    if stdout_t.is_empty() {
        "OK (exit_code: 0)".to_string()
    } else {
        format!("OK (exit_code: 0)\nstdout:\n{stdout_t}")
    }
}

fn wrap_exec_with_pwd(cmd: &str) -> String {
    let raw = cmd.trim();
    if raw.is_empty() {
        return String::new();
    }

    if cfg!(target_os = "windows") {
        // Emit marker even on failures to keep recovery loops from losing cwd.
        // NOTE: if this wrapped script triggers bash->PowerShell translation, the translator MUST
        // preserve standalone `}` lines (see exec.rs).
        return [
            "$ErrorActionPreference = 'Stop'",
            "try {",
            raw,
            "} finally {",
            &format!("Write-Output (\"{}\" + (Get-Location).Path)", PWD_MARKER),
            "}",
        ]
        .join("\n");
    }

    // POSIX: keep behavior simple while preserving the wrapped command's exit status.
    format!(
        "{{\n{raw}\n__spiral_coder_status=$?\necho \"{PWD_MARKER}$(pwd)\"\nif [ \"$__spiral_coder_status\" -ne 0 ]; then\n  sh -c \"exit $__spiral_coder_status\"\nelse\n  true\nfi\n}}"
    )
}

fn strip_pwd_marker(stdout_raw: &str) -> (String, Option<String>) {
    let raw = stdout_raw.replace("\r\n", "\n");
    if raw.is_empty() {
        return (String::new(), None);
    }
    let mut kept: Vec<&str> = Vec::new();
    let mut pwd: Option<String> = None;
    for ln in raw.split('\n') {
        if let Some(rest) = ln.strip_prefix(PWD_MARKER) {
            let p = rest.trim();
            if !p.is_empty() {
                pwd = Some(p.to_string());
            }
            continue;
        }
        kept.push(ln);
    }
    (kept.join("\n").trim_end().to_string(), pwd)
}

fn normalize_path_sep(s: &str) -> String {
    s.replace('\\', "/")
}

fn is_within_root(path: &str, root: &str) -> bool {
    let p = normalize_path_sep(path)
        .replace('\u{0}', "")
        .trim()
        .trim_end_matches('/')
        .to_string();
    let r = normalize_path_sep(root)
        .replace('\u{0}', "")
        .trim()
        .trim_end_matches('/')
        .to_string();
    if p.is_empty() || r.is_empty() {
        return false;
    }
    if cfg!(target_os = "windows") {
        let p = p.to_ascii_lowercase();
        let r = r.to_ascii_lowercase();
        p == r || p.starts_with(&(r + "/"))
    } else {
        p == r || p.starts_with(&(r + "/"))
    }
}

fn absolutize_path(path: &str) -> Option<String> {
    let p = path.trim();
    if p.is_empty() {
        return None;
    }
    let pb = std::path::PathBuf::from(p);
    let abs = if pb.is_absolute() {
        pb
    } else {
        let cwd = std::env::current_dir().ok()?;
        cwd.join(pb)
    };
    Some(abs.to_string_lossy().into_owned())
}

fn inject_cwd(tool_output: &str, cwd_line: &str, note: Option<&str>) -> String {
    let t = tool_output.trim_end_matches('\n');
    if t.is_empty() {
        return String::new();
    }
    let (first, rest) = match t.split_once('\n') {
        Some((a, b)) => (a, b),
        None => (t, ""),
    };

    let mut out = String::new();
    out.push_str(first);
    out.push('\n');
    out.push_str(cwd_line);
    if let Some(n) = note {
        if !n.trim().is_empty() {
            out.push('\n');
            out.push_str(n.trim_end());
        }
    }
    if !rest.trim().is_empty() {
        out.push('\n');
        out.push_str(rest);
    }
    out
}

// ── Loop Governor (Coder strengthening) ────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AgentState {
    Planning,
    Executing,
    Verifying,
    Recovery,
    Done,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RecoveryStage {
    Diagnose,
    Fix,
    Verify,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExecKind {
    Diagnostic,
    Action,
    Verify,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
enum VerificationLevel {
    #[default]
    Build,
    Behavioral,
}

impl VerificationLevel {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Build => "build",
            Self::Behavioral => "behavioral",
        }
    }

    fn satisfies(self, required: Self) -> bool {
        matches!(
            (self, required),
            (VerificationLevel::Behavioral, _)
                | (VerificationLevel::Build, VerificationLevel::Build)
        )
    }

    fn max(self, other: Self) -> Self {
        if self >= other {
            self
        } else {
            other
        }
    }
}

#[derive(Debug, Clone, Default)]
struct GoalCheckStatus {
    attempts: usize,
    ok: bool,
}

#[derive(Debug, Clone, Default)]
struct GoalCheckTracker {
    repo: GoalCheckStatus,
    tests: GoalCheckStatus,
    build: GoalCheckStatus,
}

impl GoalCheckTracker {
    fn get_mut(&mut self, key: &str) -> Option<&mut GoalCheckStatus> {
        match key {
            "repo" => Some(&mut self.repo),
            "tests" => Some(&mut self.tests),
            "build" => Some(&mut self.build),
            _ => None,
        }
    }

    fn any_attempted(&self) -> bool {
        self.repo.attempts > 0 || self.tests.attempts > 0 || self.build.attempts > 0
    }
}

#[derive(Debug, Default)]
struct RecoveryGovernor {
    stage: Option<RecoveryStage>,
    required_verification: VerificationLevel,
}

impl RecoveryGovernor {
    fn stage_label(&self) -> &'static str {
        match self.stage {
            None => "none",
            Some(RecoveryStage::Diagnose) => "diagnose",
            Some(RecoveryStage::Fix) => "fix",
            Some(RecoveryStage::Verify) => "verify",
        }
    }

    fn in_recovery(&self) -> bool {
        self.stage.is_some()
    }

    fn restore_from_session(
        mem: &FailureMemory,
        messages: &[serde_json::Value],
        required_verification: VerificationLevel,
    ) -> Self {
        let mut g = RecoveryGovernor {
            stage: None,
            required_verification,
        };
        if mem.consecutive_failures > 0 || last_tool_looks_failed(messages) {
            g.stage = Some(RecoveryStage::Diagnose);
        }
        g
    }

    fn maybe_block_tool(
        &self,
        tc: &ToolCallData,
        test_cmd: Option<&str>,
        task_harness: TaskHarness,
        allow_existing_followup_verify: bool,
    ) -> Option<String> {
        let Some(stage) = self.stage else {
            return None;
        };
        let name = tc.name.as_str();

        // Note: `done` is handled earlier in the main loop.
        match stage {
            RecoveryStage::Diagnose => {
                if is_diagnostic_tool_name(name) {
                    return None;
                }
                if name == "exec" {
                    let cmd =
                        parse_exec_command_from_args(tc.arguments.as_str()).unwrap_or_default();
                    if is_diagnostic_command(cmd.as_str()) {
                        return None;
                    }
                }
                if allows_artifact_creation_during_diagnose(task_harness, tc) {
                    return None;
                }
                Some(format!(
                    "[Recovery Gate] stage=diagnose\n\
You are in recovery mode. Do NOT start new work yet.\n\
Required now: run diagnostics first (e.g. `pwd`, `ls`/`dir`, `git status`, `git rev-parse --show-toplevel`)."
                ))
            }
            RecoveryStage::Fix => None, // allow edits/commands to fix
            RecoveryStage::Verify => {
                if allows_artifact_creation_during_verify(task_harness, tc)
                    || allow_existing_followup_verify
                {
                    return None;
                }
                if name == "exec" {
                    let cmd =
                        parse_exec_command_from_args(tc.arguments.as_str()).unwrap_or_default();
                    let verify_level = classify_verify_level(cmd.as_str(), test_cmd);
                    if verify_level
                        .map(|level| level.satisfies(self.required_verification))
                        .unwrap_or(false)
                    {
                        return None;
                    }
                }
                Some(format!(
                    "[Recovery Gate] stage=verify\n\
You already applied a fix. Verify before continuing.\n\
Required now: {}",
                    verification_requirement_hint(self.required_verification, test_cmd)
                ))
            }
        }
    }

    fn on_diagnostic_result(&mut self, ok: bool) {
        if !self.in_recovery() && !ok {
            self.stage = Some(RecoveryStage::Diagnose);
            return;
        }
        if !ok {
            self.stage = Some(RecoveryStage::Diagnose);
            return;
        }
        if self.stage == Some(RecoveryStage::Diagnose) {
            self.stage = Some(RecoveryStage::Fix);
        }
    }

    fn on_fix_result(&mut self, ok: bool, verified_level: Option<VerificationLevel>) {
        if !self.in_recovery() && !ok {
            self.stage = Some(RecoveryStage::Diagnose);
            return;
        }
        if !ok {
            self.stage = Some(RecoveryStage::Diagnose);
            return;
        }
        if verified_level
            .map(|level| level.satisfies(self.required_verification))
            .unwrap_or(false)
        {
            self.stage = None;
            return;
        }
        match self.stage {
            Some(RecoveryStage::Diagnose) | Some(RecoveryStage::Fix) => {
                self.stage = Some(RecoveryStage::Verify);
            }
            _ => {}
        }
    }

    fn on_exec_result(
        &mut self,
        kind: ExecKind,
        verify_level: Option<VerificationLevel>,
        ok: bool,
    ) {
        if !ok {
            self.stage = Some(RecoveryStage::Diagnose);
            return;
        }
        if kind == ExecKind::Verify
            && verify_level
                .map(|level| level.satisfies(self.required_verification))
                .unwrap_or(false)
        {
            // A successful verification ends recovery regardless of the current stage.
            self.stage = None;
            return;
        }
        match (self.stage, kind) {
            (Some(RecoveryStage::Diagnose), ExecKind::Diagnostic) => {
                self.stage = Some(RecoveryStage::Fix);
            }
            (Some(RecoveryStage::Fix), ExecKind::Action) => {
                self.stage = Some(RecoveryStage::Verify);
            }
            _ => {}
        }
    }
}

#[derive(Debug, Default)]
struct FailureMemory {
    consecutive_failures: usize,

    last_command_sig: Option<String>,
    same_command_repeats: usize,

    last_error_sig: Option<String>,
    same_error_repeats: usize,

    last_output_hash: Option<u64>,
    same_output_repeats: usize,

    last_error_class: ErrorClass,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GoalDelta {
    Closer,
    Same,
    Farther,
    Unknown,
}

impl GoalDelta {
    fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "closer" => Self::Closer,
            "same" => Self::Same,
            "farther" | "further" => Self::Farther,
            _ => Self::Unknown,
        }
    }

    fn as_str(&self) -> &'static str {
        match self {
            Self::Closer => "closer",
            Self::Same => "same",
            Self::Farther => "farther",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StrategyChange {
    Keep,
    Adjust,
    Abandon,
    Unknown,
}

impl StrategyChange {
    fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "keep" => Self::Keep,
            "adjust" => Self::Adjust,
            "abandon" => Self::Abandon,
            _ => Self::Unknown,
        }
    }

    fn as_str(&self) -> &'static str {
        match self {
            Self::Keep => "keep",
            Self::Adjust => "adjust",
            Self::Abandon => "abandon",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone)]
struct PlanBlock {
    goal: String,
    steps: Vec<String>,
    acceptance_criteria: Vec<String>,
    risks: String,
    assumptions: String,
}

#[derive(Debug, Clone)]
struct ThinkBlock {
    goal: String,
    step: usize,
    tool: String,
    risk: String,
    doubt: String,
    next: String,
    verify: String,
}

#[derive(Debug, Clone)]
struct ReflectionBlock {
    last_outcome: String,
    goal_delta: GoalDelta,
    wrong_assumption: String,
    strategy_change: StrategyChange,
    next_minimal_action: String,
}

#[derive(Debug, Clone)]
struct ImpactBlock {
    changed: String,
    progress: String,
    remaining_gap: String,
}

#[derive(Debug, Clone)]
struct EvidenceBlock {
    target_files: Vec<String>,
    target_symbols: Vec<String>,
    evidence: String,
    open_questions: String,
    next_probe: String,
}

#[derive(Debug, Clone)]
struct TaskContract {
    task_summary: String,
    hard_constraints: Vec<String>,
    non_goals: Vec<String>,
    output_shape: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum InstructionAuthority {
    Root,
    System,
    Project,
    User,
    Execution,
}

impl InstructionAuthority {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Root => "root",
            Self::System => "system",
            Self::Project => "project",
            Self::User => "user",
            Self::Execution => "execution",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InstructionSource {
    TaskContract,
    ProjectRules,
    UserRequest,
    Plan,
    Think,
}

impl InstructionSource {
    fn as_str(&self) -> &'static str {
        match self {
            Self::TaskContract => "task_contract",
            Self::ProjectRules => "project_rules",
            Self::UserRequest => "user_request",
            Self::Plan => "plan",
            Self::Think => "think",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct InstructionPriority {
    authority: InstructionAuthority,
    explicit: bool,
    locality: u8,
    sequence: usize,
}

impl InstructionPriority {
    fn outranks(&self, other: &Self) -> bool {
        (
            std::cmp::Reverse(governor_contract::instruction_authority_rank(
                self.authority.as_str(),
            )),
            self.explicit,
            self.locality,
            self.sequence,
        ) > (
            std::cmp::Reverse(governor_contract::instruction_authority_rank(
                other.authority.as_str(),
            )),
            other.explicit,
            other.locality,
            other.sequence,
        )
    }
}

#[derive(Debug, Clone)]
struct InstructionConflict {
    winner_authority: InstructionAuthority,
    winner_source: InstructionSource,
    loser_authority: InstructionAuthority,
    loser_source: InstructionSource,
    reason: String,
}

impl InstructionConflict {
    fn render(&self) -> String {
        governor_contract::instruction_resolver_conflict_message(
            self.winner_authority.as_str(),
            self.winner_source.as_str(),
            self.loser_authority.as_str(),
            self.loser_source.as_str(),
            self.reason.as_str(),
        )
    }
}

#[derive(Debug, Clone)]
struct InstructionResolver {
    task_summary: String,
    root_read_only: bool,
    project_rules_active: bool,
}

impl InstructionResolver {
    fn new(task_summary: &str, root_read_only: bool, project_rules_active: bool) -> Self {
        Self {
            task_summary: compact_one_line(task_summary.trim(), 220),
            root_read_only,
            project_rules_active,
        }
    }

    fn user_task_summary(&self) -> &str {
        self.task_summary.as_str()
    }

    fn prompt(&self, required_verification: VerificationLevel) -> String {
        let mut out = format!(
            "[{}]\n{}\n",
            governor_contract::instruction_resolver_title(),
            governor_contract::instruction_resolver_priority_title()
        );
        for label in governor_contract::instruction_priority_labels() {
            out.push_str("- ");
            out.push_str(label.as_str());
            out.push('\n');
        }
        out.push_str(governor_contract::instruction_resolver_rules_title());
        out.push('\n');
        for rule in governor_contract::instruction_resolver_rules() {
            out.push_str("- ");
            out.push_str(rule.as_str());
            out.push('\n');
        }
        out.push_str(governor_contract::instruction_resolver_current_title());
        out.push('\n');
        out.push_str("- ");
        out.push_str(
            governor_contract::instruction_resolver_root_runtime_line_message(
                InstructionAuthority::Root.as_str(),
            )
            .as_str(),
        );
        out.push('\n');
        out.push_str("- ");
        out.push_str(
            governor_contract::instruction_resolver_user_task_line_message(
                InstructionAuthority::User.as_str(),
                InstructionSource::UserRequest.as_str(),
                self.user_task_summary(),
            )
            .as_str(),
        );
        out.push('\n');
        if self.root_read_only {
            out.push_str("- ");
            out.push_str(
                governor_contract::instruction_resolver_read_only_line_message(
                    InstructionAuthority::System.as_str(),
                    InstructionSource::TaskContract.as_str(),
                )
                .as_str(),
            );
            out.push('\n');
        }
        out.push_str("- ");
        out.push_str(
            governor_contract::instruction_resolver_done_requires_line_message(
                InstructionAuthority::System.as_str(),
                InstructionSource::TaskContract.as_str(),
                match required_verification {
                    VerificationLevel::Build => "real build/check/lint",
                    VerificationLevel::Behavioral => "real behavioral",
                },
            )
            .as_str(),
        );
        out.push('\n');
        if self.project_rules_active {
            out.push_str("- ");
            out.push_str(
                governor_contract::instruction_resolver_project_rules_line_message(
                    InstructionAuthority::Project.as_str(),
                    InstructionSource::ProjectRules.as_str(),
                )
                .as_str(),
            );
            out.push('\n');
        }
        out
    }

    fn plan_conflict(&self, plan: &PlanBlock) -> Option<InstructionConflict> {
        if !self.root_read_only {
            return None;
        }
        let reason = read_only_plan_violation(plan)?;
        let winner = InstructionPriority {
            authority: InstructionAuthority::System,
            explicit: true,
            locality: 2,
            sequence: 1,
        };
        let loser = InstructionPriority {
            authority: InstructionAuthority::Execution,
            explicit: true,
            locality: 3,
            sequence: 1,
        };
        if !winner.outranks(&loser) {
            return None;
        }
        Some(InstructionConflict {
            winner_authority: InstructionAuthority::System,
            winner_source: InstructionSource::TaskContract,
            loser_authority: InstructionAuthority::Execution,
            loser_source: InstructionSource::Plan,
            reason,
        })
    }

    fn tool_conflict(
        &self,
        tc: &ToolCallData,
        test_cmd: Option<&str>,
    ) -> Option<InstructionConflict> {
        if !self.root_read_only {
            return None;
        }
        let reason = match tc.name.as_str() {
            "write_file" | "patch_file" | "apply_diff" => {
                Some(governor_contract::instruction_resolver_read_only_mutation_message())
            }
            "exec" => {
                let command =
                    parse_exec_command_from_args(tc.arguments.as_str()).unwrap_or_default();
                match classify_exec_kind(command.as_str(), test_cmd) {
                    ExecKind::Diagnostic => None,
                    ExecKind::Verify => Some(
                        governor_contract::instruction_resolver_read_only_verify_exec_message(
                            command.as_str(),
                        ),
                    ),
                    ExecKind::Action => Some(
                        governor_contract::instruction_resolver_read_only_action_exec_message(
                            command.as_str(),
                        ),
                    ),
                }
            }
            _ => None,
        }?;
        let winner = InstructionPriority {
            authority: InstructionAuthority::System,
            explicit: true,
            locality: 2,
            sequence: 1,
        };
        let loser = InstructionPriority {
            authority: InstructionAuthority::Execution,
            explicit: true,
            locality: 3,
            sequence: 2,
        };
        if !winner.outranks(&loser) {
            return None;
        }
        Some(InstructionConflict {
            winner_authority: InstructionAuthority::System,
            winner_source: InstructionSource::TaskContract,
            loser_authority: InstructionAuthority::Execution,
            loser_source: InstructionSource::Think,
            reason,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AssumptionStatus {
    Unknown,
    Confirmed,
    Refuted,
}

impl AssumptionStatus {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Confirmed => "confirmed",
            Self::Refuted => "refuted",
        }
    }
}

#[derive(Debug, Clone)]
struct AssumptionEntry {
    text: String,
    status: AssumptionStatus,
    evidence: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct AssumptionLedger {
    entries: Vec<AssumptionEntry>,
}

impl AssumptionLedger {
    fn remember_unknown(&mut self, assumption: &str) {
        let text = compact_one_line(assumption.trim(), 140);
        if text == "-" {
            return;
        }
        let sig = normalize_memory_entry(text.as_str());
        if sig.is_empty() {
            return;
        }
        if let Some(existing) = self
            .entries
            .iter_mut()
            .find(|entry| normalize_memory_entry(entry.text.as_str()) == sig)
        {
            if existing.status == AssumptionStatus::Unknown {
                existing.text = text;
            }
            return;
        }
        self.entries.push(AssumptionEntry {
            text,
            status: AssumptionStatus::Unknown,
            evidence: None,
        });
        if self.entries.len() > 8 {
            self.entries.remove(0);
        }
    }

    fn sync_to_plan(&mut self, plan: &PlanBlock) {
        let plan_assumptions = parse_assumption_items(plan.assumptions.as_str());
        for assumption in &plan_assumptions {
            self.remember_unknown(assumption);
        }
        self.entries.retain(|entry| {
            if entry.status == AssumptionStatus::Refuted {
                return true;
            }
            let sig = normalize_memory_entry(entry.text.as_str());
            plan_assumptions
                .iter()
                .any(|assumption| normalize_memory_entry(assumption.as_str()) == sig)
        });
    }

    fn mark_refuted(&mut self, assumption: &str, evidence: Option<&str>) {
        let text = compact_one_line(assumption.trim(), 140);
        if text == "-" {
            return;
        }
        let sig = normalize_memory_entry(text.as_str());
        let evidence = evidence
            .map(|item| compact_one_line(item.trim(), 160))
            .filter(|item| item != "-");
        if let Some(existing) = self
            .entries
            .iter_mut()
            .find(|entry| normalize_memory_entry(entry.text.as_str()) == sig)
        {
            existing.status = AssumptionStatus::Refuted;
            existing.evidence = evidence;
            existing.text = text;
            return;
        }
        self.entries.push(AssumptionEntry {
            text,
            status: AssumptionStatus::Refuted,
            evidence,
        });
        if self.entries.len() > 8 {
            self.entries.remove(0);
        }
    }

    fn refresh_confirmations(&mut self, working_mem: &WorkingMemory) {
        let support = working_mem
            .facts
            .iter()
            .chain(working_mem.successful_verifications.iter())
            .cloned()
            .collect::<Vec<_>>();
        for entry in &mut self.entries {
            if entry.status != AssumptionStatus::Unknown {
                continue;
            }
            let assumption_sig = normalize_memory_entry(entry.text.as_str());
            if assumption_sig.is_empty() {
                continue;
            }
            let assumption_tokens = keyword_tokens(entry.text.as_str());
            let best = support
                .iter()
                .map(|candidate| {
                    let candidate_sig = normalize_memory_entry(candidate.as_str());
                    let mut score = token_overlap_score(
                        &assumption_tokens,
                        &keyword_tokens(candidate.as_str()),
                    );
                    if !candidate_sig.is_empty()
                        && (candidate_sig.contains(assumption_sig.as_str())
                            || assumption_sig.contains(candidate_sig.as_str()))
                    {
                        score = score.max(0.9);
                    }
                    (score, candidate)
                })
                .max_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
            if let Some((score, candidate)) = best {
                if score >= 0.72 {
                    entry.status = AssumptionStatus::Confirmed;
                    entry.evidence = Some(compact_one_line(candidate.as_str(), 160));
                }
            }
        }
    }

    fn from_messages(messages: &[serde_json::Value], working_mem: &WorkingMemory) -> Self {
        let mut ledger = Self::default();
        for msg in messages {
            if msg.get("role").and_then(|v| v.as_str()) != Some("assistant") {
                continue;
            }
            let content = msg.get("content").and_then(|v| v.as_str()).unwrap_or("");
            if let Some(plan) = parse_plan_block(content).filter(|p| validate_plan(p).is_ok()) {
                ledger.sync_to_plan(&plan);
            }
            if let Some(reflect) = parse_reflection_block(content) {
                ledger.mark_refuted(
                    reflect.wrong_assumption.as_str(),
                    Some(reflect.next_minimal_action.as_str()),
                );
            }
        }
        ledger.refresh_confirmations(working_mem);
        ledger
    }

    fn has_refuted(&self) -> bool {
        self.entries
            .iter()
            .any(|entry| entry.status == AssumptionStatus::Refuted)
    }
}

#[derive(Debug, Clone, Default)]
struct WorkingMemory {
    facts: Vec<String>,
    completed_steps: Vec<String>,
    successful_verifications: Vec<String>,
    chosen_strategy: Option<String>,
}

impl WorkingMemory {
    fn is_empty(&self) -> bool {
        self.facts.is_empty()
            && self.completed_steps.is_empty()
            && self.successful_verifications.is_empty()
            && self
                .chosen_strategy
                .as_deref()
                .map(|s| s.trim().is_empty())
                .unwrap_or(true)
    }

    fn remember_fact(&mut self, fact: &str) {
        remember_recent_unique(&mut self.facts, fact, 6, 120);
    }

    fn remember_completed_step(&mut self, step: &str) {
        remember_recent_unique(&mut self.completed_steps, step, 7, 120);
    }

    fn remember_successful_verification(&mut self, command: &str) {
        remember_recent_unique(&mut self.successful_verifications, command, 4, 1024);
    }

    fn set_strategy(&mut self, strategy: &str) {
        let value = compact_one_line(strategy.trim(), 140);
        if value == "-" {
            return;
        }
        self.chosen_strategy = Some(value);
    }

    fn sync_to_plan(&mut self, plan: &PlanBlock) {
        self.completed_steps.retain(|step| {
            let want = normalize_memory_entry(step);
            plan.steps
                .iter()
                .any(|candidate| normalize_memory_entry(candidate) == want)
        });
    }

    fn from_messages(messages: &[serde_json::Value], test_cmd: Option<&str>) -> Self {
        #[derive(Debug, Clone)]
        struct PendingToolIntent {
            name: String,
            command: Option<String>,
            next_action: Option<String>,
            step_label: Option<String>,
        }

        let mut mem = Self::default();
        let exec_context =
            exec_verification::ExecVerificationContext::from_messages(test_cmd, messages);
        let mut active_plan: Option<PlanBlock> = None;
        let mut pending: std::collections::HashMap<String, PendingToolIntent> =
            std::collections::HashMap::new();

        for msg in messages {
            let role = msg.get("role").and_then(|v| v.as_str()).unwrap_or("");

            if role == "assistant" {
                let content = msg.get("content").and_then(|v| v.as_str()).unwrap_or("");

                if let Some(plan) = parse_plan_block(content).filter(|p| validate_plan(p).is_ok()) {
                    mem.sync_to_plan(&plan);
                    active_plan = Some(plan);
                }

                if let Some(reflect) = parse_reflection_block(content) {
                    if matches!(
                        reflect.strategy_change,
                        StrategyChange::Adjust | StrategyChange::Abandon
                    ) {
                        mem.set_strategy(reflect.next_minimal_action.as_str());
                    }
                }

                let parsed_think = parse_think_block(content);
                if let Some(tcs) = msg.get("tool_calls").and_then(|v| v.as_array()) {
                    for tc in tcs {
                        let id = tc.get("id").and_then(|v| v.as_str()).unwrap_or("").trim();
                        if id.is_empty() {
                            continue;
                        }
                        let name = tc
                            .get("function")
                            .and_then(|f| f.get("name"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .trim()
                            .to_string();
                        let args = tc
                            .get("function")
                            .and_then(|f| f.get("arguments"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .trim()
                            .to_string();
                        let command = if name == "exec" {
                            parse_exec_command_from_args(args.as_str())
                        } else {
                            None
                        };
                        let step_label =
                            resolve_step_label(active_plan.as_ref(), parsed_think.as_ref());
                        let next_action = parsed_think
                            .as_ref()
                            .map(|think| think.next.clone())
                            .filter(|s| !s.trim().is_empty());
                        pending.insert(
                            id.to_string(),
                            PendingToolIntent {
                                name,
                                command,
                                next_action,
                                step_label,
                            },
                        );
                    }
                }
                continue;
            }

            if role != "tool" {
                continue;
            }

            let id = msg
                .get("tool_call_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();
            if id.is_empty() {
                continue;
            }

            let Some(intent) = pending.remove(id) else {
                continue;
            };

            let content = msg.get("content").and_then(|v| v.as_str()).unwrap_or("");
            match intent.name.as_str() {
                "exec" => {
                    let command = intent.command.unwrap_or_default();
                    let (exit_code, stdout, stderr) = parse_exec_tool_output_sections(content);
                    let Some(exit_code) = exit_code else {
                        continue;
                    };
                    let effective_exit_code = if exit_code == 0
                        && suspicious_success_reason(stdout.as_str(), stderr.as_str()).is_some()
                    {
                        1
                    } else {
                        exit_code
                    };
                    if effective_exit_code != 0 {
                        continue;
                    }
                    let exec_kind = exec_context.classify(command.as_str()).kind;
                    update_working_memory_after_exec(
                        &mut mem,
                        command.as_str(),
                        stdout.as_str(),
                        content,
                        exec_kind,
                        intent.step_label.as_deref(),
                        intent.next_action.as_deref(),
                    );
                }
                "read_file" | "list_dir" | "glob" | "search_files" | "write_file"
                | "patch_file" | "apply_diff" => {
                    if !non_exec_tool_succeeded(content) {
                        continue;
                    }
                    let verified = crate::execution_evidence::auto_test_succeeded(
                        intent.name.as_str(),
                        content,
                    );
                    update_working_memory_after_non_exec(
                        &mut mem,
                        intent.name.as_str(),
                        verified,
                        intent.step_label.as_deref(),
                        intent.next_action.as_deref(),
                        test_cmd,
                    );
                }
                _ => {}
            }
        }

        mem
    }
}

fn strip_matching_quotes(s: &str) -> &str {
    let bytes = s.as_bytes();
    if bytes.len() >= 2
        && ((bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[bytes.len() - 1] == b'\''))
    {
        &s[1..s.len() - 1]
    } else {
        s
    }
}

fn split_top_level_args(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut depth = 0usize;
    let mut in_quote: Option<char> = None;
    let mut escaped = false;

    for ch in s.chars() {
        if escaped {
            cur.push(ch);
            escaped = false;
            continue;
        }
        match in_quote {
            Some(q) => {
                cur.push(ch);
                if ch == '\\' {
                    escaped = true;
                } else if ch == q {
                    in_quote = None;
                }
            }
            None => match ch {
                '"' | '\'' => {
                    in_quote = Some(ch);
                    cur.push(ch);
                }
                '(' | '[' | '{' => {
                    depth = depth.saturating_add(1);
                    cur.push(ch);
                }
                ')' | ']' | '}' => {
                    depth = depth.saturating_sub(1);
                    cur.push(ch);
                }
                ',' if depth == 0 => {
                    let part = cur.trim();
                    if !part.is_empty() {
                        out.push(part.to_string());
                    }
                    cur.clear();
                }
                _ => cur.push(ch),
            },
        }
    }

    let tail = cur.trim();
    if !tail.is_empty() {
        out.push(tail.to_string());
    }
    out
}

fn canonicalize_arg_value(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => compact_one_line(s.trim(), 160),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Null => "null".to_string(),
        _ => compact_one_line(&value.to_string(), 160),
    }
}

fn canonicalize_named_command(name: &str, args: &[(String, String)]) -> Option<String> {
    let normalized_name = name
        .trim()
        .trim_matches('<')
        .trim_matches('>')
        .trim()
        .to_ascii_lowercase();
    if normalized_name.is_empty() {
        return None;
    }
    let mut normalized_args = args
        .iter()
        .filter_map(|(key, value)| {
            let key = key.trim().to_ascii_lowercase();
            let value = compact_one_line(strip_matching_quotes(value.trim()).trim(), 160);
            if key.is_empty() || value.is_empty() || value == "-" {
                None
            } else {
                Some((key, value))
            }
        })
        .collect::<Vec<_>>();
    normalized_args.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    let joined = normalized_args
        .into_iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join(", ");
    Some(format!("{normalized_name}({joined})"))
}

fn canonicalize_tool_call_command(name: &str, arguments: &str) -> Option<String> {
    if name.trim().eq_ignore_ascii_case("exec") {
        return parse_exec_command_from_args(arguments).map(|cmd| compact_one_line(&cmd, 200));
    }
    let value = serde_json::from_str::<serde_json::Value>(arguments).ok()?;
    let obj = value.as_object()?;
    let args = obj
        .iter()
        .map(|(key, value)| (key.clone(), canonicalize_arg_value(value)))
        .collect::<Vec<_>>();
    canonicalize_named_command(name, &args)
}

fn parse_named_command_signature(
    command: &str,
) -> Option<(String, std::collections::BTreeMap<String, String>)> {
    let trimmed = command.trim();
    let open_idx = trimmed.find('(')?;
    if !trimmed.ends_with(')') {
        return None;
    }
    let name = trimmed[..open_idx].trim().to_ascii_lowercase();
    if name.is_empty() {
        return None;
    }
    let inner = &trimmed[open_idx + 1..trimmed.len() - 1];
    let mut args = std::collections::BTreeMap::new();
    for part in split_top_level_args(inner) {
        let (key, value) = part.split_once('=')?;
        let key = key.trim().to_ascii_lowercase();
        let value = compact_one_line(strip_matching_quotes(value.trim()).trim(), 160);
        if !key.is_empty() && !value.is_empty() && value != "-" {
            args.insert(key, value);
        }
    }
    Some((name, args))
}

fn canonicalize_evidence_command(command: &str) -> String {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if let Some(open_idx) = trimmed.find('(') {
        if trimmed.ends_with(')') {
            let name = trimmed[..open_idx].trim();
            if !name.contains(char::is_whitespace) {
                let inner = &trimmed[open_idx + 1..trimmed.len() - 1];
                let args = split_top_level_args(inner)
                    .into_iter()
                    .filter_map(|part| {
                        let (key, value) = part.split_once('=')?;
                        Some((key.trim().to_string(), value.trim().to_string()))
                    })
                    .collect::<Vec<_>>();
                if let Some(sig) = canonicalize_named_command(name, &args) {
                    return sig;
                }
            }
        }
    }

    let trimmed = trimmed
        .split_once(" (")
        .map(|(head, _)| head.trim())
        .unwrap_or(trimmed);
    let low = trimmed.to_ascii_lowercase();
    if low.starts_with("read_file of ") || low.starts_with("read file of ") {
        let path = trimmed
            .split_once(" of ")
            .map(|(_, rhs)| {
                rhs.trim()
                    .trim_matches('`')
                    .trim_matches('"')
                    .trim_matches('\'')
            })
            .unwrap_or("")
            .trim();
        if !path.is_empty() {
            if let Some(sig) =
                canonicalize_named_command("read_file", &[("path".to_string(), path.to_string())])
            {
                return sig;
            }
        }
    }
    if let Some(rest) = trimmed.strip_prefix("read_file ") {
        if let Some(sig) = canonicalize_named_command(
            "read_file",
            &[(
                "path".to_string(),
                rest.trim()
                    .trim_matches('`')
                    .trim_matches('"')
                    .trim_matches('\'')
                    .to_string(),
            )],
        ) {
            return sig;
        }
    }
    if let Some(rest) = trimmed.strip_prefix("list_dir ") {
        if let Some(sig) =
            canonicalize_named_command("list_dir", &[("path".to_string(), rest.trim().to_string())])
        {
            return sig;
        }
    }
    if let Some(rest) = trimmed.strip_prefix("glob ") {
        if let Some(sig) =
            canonicalize_named_command("glob", &[("pattern".to_string(), rest.trim().to_string())])
        {
            return sig;
        }
    }
    if low.starts_with("search_files for ") || low.starts_with("search files for ") {
        let normalized = trimmed
            .replace("search files for ", "search_files for ")
            .replace(" in `", " in ")
            .replace(" in \"", " in ")
            .replace(" in '", " in ");
        let rest = normalized
            .strip_prefix("search_files for ")
            .unwrap_or(normalized.as_str());
        if let Some((pattern, dir)) = rest.rsplit_once(" in ") {
            let pattern = pattern
                .trim()
                .trim_matches('`')
                .trim_matches('"')
                .trim_matches('\'')
                .trim();
            let dir = dir
                .trim()
                .trim_matches('`')
                .trim_matches('"')
                .trim_matches('\'')
                .trim_end_matches('/')
                .trim();
            if let Some(sig) = canonicalize_named_command(
                "search_files",
                &[
                    ("pattern".to_string(), pattern.to_string()),
                    ("dir".to_string(), dir.to_string()),
                ],
            ) {
                return sig;
            }
        }
    }
    if low.starts_with("search_files with pattern ")
        || low.starts_with("search files with pattern ")
    {
        let normalized = trimmed
            .replace("search files with pattern ", "search_files with pattern ")
            .replace(" in `", " in ")
            .replace(" in \"", " in ")
            .replace(" in '", " in ");
        let rest = normalized
            .strip_prefix("search_files with pattern ")
            .unwrap_or(normalized.as_str());
        if let Some((pattern, dir)) = rest.rsplit_once(" in ") {
            let pattern = pattern
                .trim()
                .trim_matches('`')
                .trim_matches('"')
                .trim_matches('\'')
                .trim();
            let dir = dir
                .trim()
                .trim_matches('`')
                .trim_matches('"')
                .trim_matches('\'')
                .trim_end_matches('/')
                .trim();
            if let Some(sig) = canonicalize_named_command(
                "search_files",
                &[
                    ("pattern".to_string(), pattern.to_string()),
                    ("dir".to_string(), dir.to_string()),
                ],
            ) {
                return sig;
            }
        }
    }
    if let Some(rest) = trimmed.strip_prefix("search_files ") {
        if let Some((pattern, dir)) = rest.rsplit_once(" in ") {
            if let Some(sig) = canonicalize_named_command(
                "search_files",
                &[
                    ("pattern".to_string(), pattern.trim().to_string()),
                    ("dir".to_string(), dir.trim().to_string()),
                ],
            ) {
                return sig;
            }
        }
    }
    if low.starts_with("grep ") {
        let pattern = trimmed
            .split('"')
            .nth(1)
            .or_else(|| trimmed.split('\'').nth(1))
            .map(|s| s.trim().to_string());
        let path = trimmed
            .split_whitespace()
            .last()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        if let (Some(pattern), Some(path)) = (pattern, path) {
            if let Some(sig) = canonicalize_named_command(
                "search_files",
                &[("pattern".to_string(), pattern), ("dir".to_string(), path)],
            ) {
                return sig;
            }
        }
    }
    normalize_memory_entry(trimmed)
}

fn resolve_step_label(plan: Option<&PlanBlock>, think: Option<&ThinkBlock>) -> Option<String> {
    let plan = plan?;
    let think = think?;
    think
        .step
        .checked_sub(1)
        .and_then(|idx| plan.steps.get(idx))
        .cloned()
}

fn extract_cwd_from_tool_output(content: &str) -> Option<String> {
    for line in content.lines() {
        if let Some(rest) = line.strip_prefix("cwd_after:") {
            let value = rest.trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    for line in content.lines() {
        if let Some(rest) = line.strip_prefix("cwd:") {
            let value = rest.trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

fn first_non_empty_line(s: &str) -> Option<&str> {
    s.lines().map(str::trim).find(|line| !line.is_empty())
}

fn working_memory_facts_for_exec(command: &str, stdout: &str, tool_output: &str) -> Vec<String> {
    let mut facts: Vec<String> = Vec::new();
    if let Some(cwd) = extract_cwd_from_tool_output(tool_output) {
        facts.push(format!("cwd: {cwd}"));
    }

    let sig = command_sig(command);
    if sig.contains("git rev-parse") {
        if let Some(line) = first_non_empty_line(stdout) {
            facts.push(format!("git top-level: {}", compact_one_line(line, 120)));
        }
    }
    if sig.contains("git status") {
        let low = stdout.to_ascii_lowercase();
        if low.contains("nothing to commit, working tree clean") {
            facts.push("git working tree: clean".to_string());
        } else if low.contains("changes not staged")
            || low.contains("changes to be committed")
            || low.contains("untracked files")
        {
            facts.push("git working tree: dirty".to_string());
        }
    }

    facts
}

fn non_exec_tool_succeeded(content: &str) -> bool {
    let low = content.to_ascii_lowercase();
    !low.starts_with("error:")
        && !low.contains("rejected by user")
        && !low.contains("governor blocked")
        && !low.contains("failed (exit_code:")
}

fn update_working_memory_after_exec(
    mem: &mut WorkingMemory,
    command: &str,
    stdout: &str,
    tool_output: &str,
    exec_kind: ExecKind,
    step_label: Option<&str>,
    next_action: Option<&str>,
) {
    if let Some(next) = next_action {
        mem.set_strategy(next);
    }
    for fact in working_memory_facts_for_exec(command, stdout, tool_output) {
        mem.remember_fact(fact.as_str());
    }
    if exec_kind == ExecKind::Verify {
        mem.remember_successful_verification(command);
    }
    if matches!(exec_kind, ExecKind::Diagnostic | ExecKind::Verify) {
        if let Some(step) = step_label {
            mem.remember_completed_step(step);
        }
    }
}

fn update_working_memory_after_non_exec(
    mem: &mut WorkingMemory,
    tool_name: &str,
    verified: bool,
    step_label: Option<&str>,
    next_action: Option<&str>,
    test_cmd: Option<&str>,
) {
    if let Some(next) = next_action {
        mem.set_strategy(next);
    }
    if matches!(
        tool_name,
        "read_file"
            | "list_dir"
            | "glob"
            | "search_files"
            | "write_file"
            | "patch_file"
            | "apply_diff"
    ) {
        if let Some(step) = step_label {
            mem.remember_completed_step(step);
        }
    }
    if verified {
        mem.remember_successful_verification(test_cmd.unwrap_or("post-edit auto-test"));
    }
}

fn build_working_memory_prompt(mem: &WorkingMemory) -> Option<String> {
    if mem.is_empty() {
        return None;
    }

    let mut out = String::from("[Working Memory]\n");

    if let Some(strategy) = mem
        .chosen_strategy
        .as_deref()
        .filter(|s| !s.trim().is_empty())
    {
        out.push_str(&format!("Current strategy:\n- {strategy}\n"));
    }
    if !mem.facts.is_empty() {
        out.push_str("Confirmed facts:\n");
        for fact in &mem.facts {
            out.push_str(&format!("- {fact}\n"));
        }
    }
    if !mem.completed_steps.is_empty() {
        out.push_str("Completed steps:\n");
        for step in &mem.completed_steps {
            out.push_str(&format!("- {step}\n"));
        }
    }
    if !mem.successful_verifications.is_empty() {
        out.push_str("Known-good verification commands:\n");
        for verify in &mem.successful_verifications {
            out.push_str(&format!("- {verify}\n"));
        }
    }
    out.push_str(
        "Use this memory to avoid redoing solved work. Re-check only when new tool output contradicts it.",
    );
    Some(out)
}

fn render_cached_prompt(slot: &mut Option<u64>, full: String, compact: String) -> String {
    let digest = hash_text(full.as_str());
    let unchanged = slot.replace(digest) == Some(digest);
    if unchanged {
        compact
    } else {
        full
    }
}

fn build_instruction_resolver_compact_prompt(
    resolver: &InstructionResolver,
    required_verification: VerificationLevel,
) -> String {
    let full = resolver.prompt(required_verification);
    [
        "[Instruction Resolver cache]".to_string(),
        format!("hash: {}", fmt_hash(hash_text(full.as_str()))),
        "- order: root > system > project > user > execution".to_string(),
        format!(
            "- task: {}",
            compact_one_line(resolver.user_task_summary(), 120)
        ),
        format!(
            "- read_only: {}",
            if resolver.root_read_only { "yes" } else { "no" }
        ),
        format!(
            "- project_rules: {}",
            if resolver.project_rules_active {
                "yes"
            } else {
                "no"
            }
        ),
        format!("- verification_floor: {}", required_verification.as_str()),
    ]
    .join("\n")
}

fn build_task_contract_compact_prompt(
    contract: &TaskContract,
    active_plan: Option<&PlanBlock>,
    required_verification: VerificationLevel,
) -> String {
    let full = build_task_contract_prompt(contract, active_plan, required_verification);
    let mut lines = vec![
        "[Task Contract cache]".to_string(),
        format!("hash: {}", fmt_hash(hash_text(full.as_str()))),
        format!(
            "- task: {}",
            compact_one_line(contract.task_summary.as_str(), 120)
        ),
        format!(
            "- hard_constraints: {} non_goals: {} output_shape: {}",
            contract.hard_constraints.len(),
            contract.non_goals.len(),
            contract.output_shape.len()
        ),
        format!("- verification_floor: {}", required_verification.as_str()),
    ];
    if let Some(first) = contract.hard_constraints.first() {
        lines.push(format!(
            "- key_constraint: {}",
            compact_one_line(first, 120)
        ));
    }
    if let Some(plan) = active_plan {
        lines.push(format!(
            "- acceptance_items: {}",
            plan.acceptance_criteria.len()
        ));
    }
    lines.join("\n")
}

fn build_working_memory_compact_prompt(mem: &WorkingMemory) -> Option<String> {
    let full = build_working_memory_prompt(mem)?;
    let mut lines = vec![
        "[Working Memory cache]".to_string(),
        format!("hash: {}", fmt_hash(hash_text(full.as_str()))),
        format!(
            "- facts: {} completed_steps: {} verifications: {}",
            mem.facts.len(),
            mem.completed_steps.len(),
            mem.successful_verifications.len()
        ),
    ];
    if let Some(strategy) = mem.chosen_strategy.as_deref() {
        lines.push(format!("- strategy: {}", compact_one_line(strategy, 120)));
    }
    if !mem.completed_steps.is_empty() {
        let recent = mem
            .completed_steps
            .iter()
            .rev()
            .take(2)
            .cloned()
            .collect::<Vec<_>>();
        lines.push(format!(
            "- recent_steps: {}",
            recent.into_iter().rev().collect::<Vec<_>>().join(" | ")
        ));
    }
    Some(lines.join("\n"))
}

fn build_assumption_ledger_compact_prompt(ledger: &AssumptionLedger) -> Option<String> {
    let full = build_assumption_ledger_prompt(ledger)?;
    let open = ledger
        .entries
        .iter()
        .filter(|entry| entry.status == AssumptionStatus::Unknown)
        .count();
    let confirmed = ledger
        .entries
        .iter()
        .filter(|entry| entry.status == AssumptionStatus::Confirmed)
        .count();
    let refuted = ledger
        .entries
        .iter()
        .filter(|entry| entry.status == AssumptionStatus::Refuted)
        .collect::<Vec<_>>();
    let mut lines = vec![
        "[Assumption Ledger cache]".to_string(),
        format!("hash: {}", fmt_hash(hash_text(full.as_str()))),
        format!(
            "- open: {open} confirmed: {confirmed} refuted: {}",
            refuted.len()
        ),
    ];
    for entry in refuted.iter().rev().take(2).rev() {
        lines.push(format!(
            "- refuted: {}",
            compact_one_line(entry.text.as_str(), 120)
        ));
    }
    Some(lines.join("\n"))
}

fn build_verification_requirement_compact_prompt(
    level: VerificationLevel,
    test_cmd: Option<&str>,
    plan: Option<&PlanBlock>,
) -> String {
    let full = verification_requirement_note(level, test_cmd, plan);
    [
        "[Verification Requirement cache]".to_string(),
        format!("hash: {}", fmt_hash(hash_text(full.as_str()))),
        format!("- required: {}", level.as_str()),
        format!(
            "- hint: {}",
            compact_one_line(verification_requirement_hint(level, test_cmd).as_str(), 140)
        ),
        format!(
            "- acceptance_items: {}",
            plan.map(|p| p.acceptance_criteria.len()).unwrap_or(0)
        ),
    ]
    .join(
        "
",
    )
}

fn derive_task_contract(
    root_user_text: &str,
    root_read_only: bool,
    goal_wants_actions: bool,
    required_verification: VerificationLevel,
) -> TaskContract {
    let task_summary = compact_one_line(root_user_text.trim(), 220);
    let mut hard_constraints = vec![
        "Solve the user’s requested task before any cleanup or refactor.".to_string(),
        "Keep edits evidence-backed and scope-bounded.".to_string(),
    ];
    if root_read_only {
        hard_constraints.push("This task is inspection-only: do not edit files.".to_string());
        hard_constraints.push(
            "Do not run build/test/behavior checks just to finish a read-only task.".to_string(),
        );
    } else if !goal_wants_actions {
        hard_constraints.push(
            "If the request is underspecified, inspect first and avoid speculative edits."
                .to_string(),
        );
    }
    match required_verification {
        VerificationLevel::Behavioral => hard_constraints
            .push("Completion requires a real behavioral verification command.".to_string()),
        VerificationLevel::Build => hard_constraints
            .push("Completion requires a real build/check/lint verification.".to_string()),
    }

    let non_goals = vec![
        "Do not broaden scope into unrelated files or prompt/governor rewrites.".to_string(),
        "Do not replace working code without evidence that it is the target.".to_string(),
    ];

    let output_shape = if root_read_only {
        vec![
            "Name the confirmed file path or symbol you located.".to_string(),
            "Summarize the confirmed handling context from observation evidence.".to_string(),
        ]
    } else {
        vec![
            "Keep the final answer tied to changed files, verification, and remaining gaps."
                .to_string(),
            "If unfinished, leave the next exact command/file to continue from.".to_string(),
        ]
    };

    TaskContract {
        task_summary: if task_summary == "-" {
            "complete the requested task".to_string()
        } else {
            task_summary
        },
        hard_constraints,
        non_goals,
        output_shape,
    }
}

fn build_task_contract_prompt(
    contract: &TaskContract,
    active_plan: Option<&PlanBlock>,
    required_verification: VerificationLevel,
) -> String {
    let mut out = String::from("[Task Contract]\n");
    out.push_str("Task summary:\n");
    out.push_str(&format!("- {}\n", contract.task_summary));
    out.push_str("Hard constraints:\n");
    for item in &contract.hard_constraints {
        out.push_str(&format!("- {item}\n"));
    }
    out.push_str("Non-goals:\n");
    for item in &contract.non_goals {
        out.push_str(&format!("- {item}\n"));
    }
    out.push_str("Expected output shape:\n");
    for item in &contract.output_shape {
        out.push_str(&format!("- {item}\n"));
    }
    out.push_str("Verification floor:\n");
    out.push_str(&format!(
        "- {}\n",
        match required_verification {
            VerificationLevel::Build => "real build/check/lint before done",
            VerificationLevel::Behavioral => "real behavioral test before done",
        }
    ));
    if let Some(plan) = active_plan {
        out.push_str("Current plan acceptance:\n");
        for (idx, criterion) in plan.acceptance_criteria.iter().enumerate() {
            out.push_str(&format!("- acceptance {}: {}\n", idx + 1, criterion));
        }
    }
    out.push_str("If the next action would violate this contract, inspect/replan first.");
    out
}

fn validate_plan_against_task_contract(plan: &PlanBlock, contract: &TaskContract) -> Result<()> {
    let task_tokens = keyword_tokens(contract.task_summary.as_str());
    if task_tokens.len() < 3 {
        return Ok(());
    }

    let mut plan_tokens = keyword_tokens(plan.goal.as_str());
    for step in &plan.steps {
        plan_tokens.extend(keyword_tokens(step.as_str()));
    }
    for criterion in &plan.acceptance_criteria {
        plan_tokens.extend(keyword_tokens(criterion.as_str()));
    }

    if token_overlap_score(&task_tokens, &plan_tokens) < 0.20 {
        return Err(anyhow!(
            governor_contract::task_contract_plan_drift_message()
        ));
    }

    Ok(())
}

fn build_assumption_ledger_prompt(ledger: &AssumptionLedger) -> Option<String> {
    if ledger.entries.is_empty() {
        return None;
    }
    let mut out = String::from("[Assumption Ledger]\n");
    let mut wrote = false;
    let sections = [
        ("Open assumptions", AssumptionStatus::Unknown),
        ("Confirmed assumptions", AssumptionStatus::Confirmed),
        ("Refuted assumptions", AssumptionStatus::Refuted),
    ];
    for (title, status) in sections {
        let items = ledger
            .entries
            .iter()
            .filter(|entry| entry.status == status)
            .collect::<Vec<_>>();
        if items.is_empty() {
            continue;
        }
        wrote = true;
        out.push_str(title);
        out.push_str(":\n");
        for entry in items {
            out.push_str(&format!("- [{}] {}", entry.status.as_str(), entry.text));
            if let Some(evidence) = entry.evidence.as_deref() {
                out.push_str(&format!(" — {evidence}"));
            }
            out.push('\n');
        }
    }
    if !wrote {
        return None;
    }
    out.push_str(
        "Do not rely on refuted assumptions. Prefer probes that convert open assumptions into confirmed facts.",
    );
    Some(out)
}

fn parse_evidence_block(text: &str) -> Option<EvidenceBlock> {
    let fields = parse_block_fields(text, "evidence")?;

    Some(EvidenceBlock {
        target_files: block_list_value(&fields, "target_files"),
        target_symbols: block_list_value(&fields, "target_symbols"),
        evidence: compact_one_line(block_text_value(&fields, "evidence").as_str(), 220),
        open_questions: compact_one_line(block_text_value(&fields, "open_questions").as_str(), 180),
        next_probe: compact_one_line(block_text_value(&fields, "next_probe").as_str(), 180),
    })
}

fn parse_impact_block(text: &str) -> Option<ImpactBlock> {
    let fields = parse_block_fields(text, "impact")?;

    Some(ImpactBlock {
        changed: block_text_value(&fields, "changed"),
        progress: block_text_value(&fields, "progress"),
        remaining_gap: block_text_value(&fields, "remaining_gap"),
    })
}

fn impact_progress_matches_entry(progress: &str, entry: &str) -> bool {
    let progress_sig = normalize_memory_entry(progress);
    let entry_sig = normalize_memory_entry(entry);
    if progress_sig.is_empty() || entry_sig.is_empty() {
        return false;
    }
    progress_sig.contains(&entry_sig) || entry_sig.contains(&progress_sig)
}

fn impact_progress_matches_plan(progress: &str, plan: &PlanBlock) -> bool {
    let progress_sig = normalize_memory_entry(progress);
    if progress_sig.is_empty() {
        return false;
    }

    if progress_sig.contains("step") {
        if let Some(n) = parse_first_usize(progress) {
            if n <= plan.steps.len() {
                return true;
            }
        }
    }

    if progress_sig.contains("acceptance")
        || progress_sig.contains("criterion")
        || progress_sig.contains("criteria")
    {
        if let Some(n) = parse_first_usize(progress) {
            if n <= plan.acceptance_criteria.len() {
                return true;
            }
        }
    }

    plan.steps
        .iter()
        .any(|step| impact_progress_matches_entry(progress, step))
        || plan
            .acceptance_criteria
            .iter()
            .any(|criterion| impact_progress_matches_entry(progress, criterion))
}

fn validate_impact(impact: &ImpactBlock, plan: Option<&PlanBlock>) -> Result<()> {
    if impact.changed.trim().is_empty() {
        return Err(anyhow!(governor_contract::impact_missing_changed_message()));
    }
    if impact.progress.trim().is_empty() {
        return Err(anyhow!(governor_contract::impact_missing_progress_message()));
    }
    if impact.remaining_gap.trim().is_empty() {
        return Err(anyhow!(
            governor_contract::impact_missing_remaining_gap_message()
        ));
    }
    let Some(plan) = plan else {
        return Err(anyhow!(governor_contract::impact_requires_plan_message()));
    };
    if !impact_progress_matches_plan(impact.progress.as_str(), plan) {
        return Err(anyhow!(
            governor_contract::impact_invalid_progress_reference_message()
        ));
    }
    Ok(())
}

fn refuted_assumption_conflict(
    ledger: &AssumptionLedger,
    think: &ThinkBlock,
    tc: &ToolCallData,
) -> Option<String> {
    let mut probe = format!("{} {}", think.goal, think.next);
    if tc.name == "exec" {
        if let Some(command) = parse_exec_command_from_args(tc.arguments.as_str()) {
            probe.push(' ');
            probe.push_str(command.as_str());
        }
    } else if let Some(path) = mutation_target_path(tc) {
        probe.push(' ');
        probe.push_str(path.as_str());
    }

    let probe_sig = normalize_memory_entry(probe.as_str());
    let probe_tokens = keyword_tokens(probe.as_str());
    if probe_sig.is_empty() && probe_tokens.is_empty() {
        return None;
    }

    for entry in ledger
        .entries
        .iter()
        .filter(|entry| entry.status == AssumptionStatus::Refuted)
    {
        let assumption_sig = normalize_memory_entry(entry.text.as_str());
        let assumption_tokens = keyword_tokens(entry.text.as_str());
        let overlap = token_overlap_score(&assumption_tokens, &probe_tokens);
        let exec_retry = tc.name == "exec" && overlap >= 0.50;
        if (!assumption_sig.is_empty()
            && (probe_sig.contains(assumption_sig.as_str())
                || assumption_sig.contains(probe_sig.as_str())))
            || overlap >= 0.75
            || exec_retry
        {
            let evidence_suffix = entry
                .evidence
                .as_deref()
                .map(|evidence| format!(" ({evidence})"))
                .unwrap_or_default();
            return Some(governor_contract::assumption_refuted_reuse_message(
                entry.text.as_str(),
                evidence_suffix.as_str(),
            ));
        }
    }

    None
}

fn parse_string_list_arg(value: &serde_json::Value) -> Vec<String> {
    match value {
        serde_json::Value::Array(items) => items
            .iter()
            .filter_map(|item| item.as_str())
            .map(|item| compact_one_line(item.trim(), 160))
            .filter(|item| !item.is_empty())
            .collect(),
        serde_json::Value::String(s) => parse_plan_items(s),
        _ => Vec::new(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DoneAcceptanceEvidence {
    criterion: String,
    command: String,
}

#[derive(Debug, Clone, Default)]
struct StablePromptCache {
    resolver_hash: Option<u64>,
    task_harness_hash: Option<u64>,
    meta_harness_hash: Option<u64>,
    evaluator_loop_hash: Option<u64>,
    harness_evolution_promoted_hash: Option<u64>,
    harness_evolution_hash: Option<u64>,
    progress_bridge_hash: Option<u64>,
    session_bridge_hash: Option<u64>,
    task_contract_hash: Option<u64>,
    working_memory_hash: Option<u64>,
    assumption_ledger_hash: Option<u64>,
    reflection_ledger_hash: Option<u64>,
    verification_hash: Option<u64>,
}

fn parse_done_acceptance_evidence(value: &serde_json::Value) -> Vec<DoneAcceptanceEvidence> {
    let serde_json::Value::Array(items) = value else {
        return Vec::new();
    };

    items
        .iter()
        .filter_map(|item| {
            let serde_json::Value::Object(map) = item else {
                return None;
            };
            let criterion = map
                .get("criterion")
                .and_then(|v| v.as_str())
                .map(|v| compact_one_line(v.trim(), 160))
                .filter(|v| !v.is_empty())?;
            let command = map
                .get("command")
                .and_then(|v| v.as_str())
                .map(|v| compact_one_line(v.trim(), 200))
                .filter(|v| !v.is_empty())?;
            Some(DoneAcceptanceEvidence { criterion, command })
        })
        .collect()
}

fn keyword_tokens(s: &str) -> std::collections::BTreeSet<String> {
    s.split(|ch: char| !ch.is_ascii_alphanumeric())
        .map(|part| part.trim().to_ascii_lowercase())
        .filter(|part| part.len() >= 3)
        .collect()
}

fn token_overlap_score(
    a: &std::collections::BTreeSet<String>,
    b: &std::collections::BTreeSet<String>,
) -> f32 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let overlap = a.intersection(b).count() as f32;
    let denom = a.len().min(b.len()).max(1) as f32;
    (overlap / denom).clamp(0.0, 1.0)
}

fn parse_search_hit_count(content: &str) -> usize {
    let first = content.lines().next().unwrap_or("");
    let Some(idx) = first.find("—") else {
        return 0;
    };
    let tail = first[idx + "—".len()..].trim();
    let digits = tail
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>();
    digits.parse::<usize>().ok().unwrap_or(0)
}

fn parse_search_result_paths(content: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in content.lines().skip(1) {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('[') {
            continue;
        }
        let Some((path, _rest)) = trimmed.split_once(':') else {
            continue;
        };
        let path = compact_one_line(path.trim(), 160);
        if !path.is_empty() {
            remember_recent_unique(&mut out, path.as_str(), 8, 160);
        }
    }
    out
}

fn parse_read_file_result_path(content: &str) -> Option<String> {
    let first = content.lines().next().unwrap_or("").trim();
    let inner = first.strip_prefix('[')?.split(']').next()?.trim();
    let path = compact_one_line(inner, 160);
    if path.is_empty() {
        None
    } else {
        Some(path)
    }
}

fn collect_observation_evidence(messages: &[serde_json::Value]) -> ObservationEvidence {
    #[derive(Debug, Clone)]
    enum PendingObservation {
        Search(ObservationSearchEvidence),
        Read(ObservationReadEvidence),
    }

    let mut pending: std::collections::HashMap<String, PendingObservation> =
        std::collections::HashMap::new();
    let mut evidence = ObservationEvidence::default();

    for msg in messages {
        let role = msg
            .get("role")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        if role == "assistant" {
            let Some(tool_calls) = msg.get("tool_calls").and_then(|v| v.as_array()) else {
                continue;
            };
            for tc in tool_calls {
                let id = tc.get("id").and_then(|v| v.as_str()).unwrap_or("").trim();
                if id.is_empty() {
                    continue;
                }
                let name = tc
                    .get("function")
                    .and_then(|f| f.get("name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim();
                let args = tc
                    .get("function")
                    .and_then(|f| f.get("arguments"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim();
                let parsed = serde_json::from_str::<serde_json::Value>(args).ok();
                match name {
                    "search_files" => {
                        let command =
                            canonicalize_tool_call_command(name, args).unwrap_or_default();
                        let pattern = parsed
                            .as_ref()
                            .and_then(|v| v.get("pattern"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .trim()
                            .to_string();
                        pending.insert(
                            id.to_string(),
                            PendingObservation::Search(ObservationSearchEvidence {
                                command,
                                pattern,
                                hit_count: 0,
                                paths: Vec::new(),
                            }),
                        );
                    }
                    "read_file" => {
                        let command =
                            canonicalize_tool_call_command(name, args).unwrap_or_default();
                        let path = parsed
                            .as_ref()
                            .and_then(|v| v.get("path"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .trim()
                            .to_string();
                        pending.insert(
                            id.to_string(),
                            PendingObservation::Read(ObservationReadEvidence { command, path }),
                        );
                    }
                    _ => {}
                }
            }
            continue;
        }

        if role != "tool" {
            continue;
        }

        let tool_call_id = msg
            .get("tool_call_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        if tool_call_id.is_empty() {
            continue;
        }
        let Some(pending_obs) = pending.remove(tool_call_id) else {
            continue;
        };
        let content = msg.get("content").and_then(|v| v.as_str()).unwrap_or("");
        if !non_exec_tool_succeeded(content) {
            continue;
        }

        match pending_obs {
            PendingObservation::Search(mut search) => {
                search.hit_count = parse_search_hit_count(content);
                search.paths = parse_search_result_paths(content);
                evidence.searches.push(search);
            }
            PendingObservation::Read(mut read) => {
                if read.path.trim().is_empty() {
                    if let Some(parsed_path) = parse_read_file_result_path(content) {
                        read.path = parsed_path;
                    }
                }
                if !read.path.trim().is_empty() {
                    evidence.reads.push(read);
                }
            }
        }
    }

    evidence
}

fn search_hit_specificity(hit_count: usize) -> f32 {
    match hit_count {
        0 => 0.0,
        1 => 1.0,
        2..=3 => 0.85,
        4..=10 => 0.6,
        _ => 0.35,
    }
}

fn path_filename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn evidence_path_matches(target: &str, candidate: &str) -> bool {
    let target_sig = normalize_memory_entry(target);
    let candidate_sig = normalize_memory_entry(candidate);
    if target_sig.is_empty() || candidate_sig.is_empty() {
        return false;
    }
    target_sig == candidate_sig
        || target_sig.ends_with(candidate_sig.as_str())
        || candidate_sig.ends_with(target_sig.as_str())
}

fn observation_supports_target_path(target: &str, evidence: &ObservationEvidence) -> bool {
    evidence
        .reads
        .iter()
        .any(|read| evidence_path_matches(target, read.path.as_str()))
        || evidence.searches.iter().any(|search| {
            search
                .paths
                .iter()
                .any(|path| evidence_path_matches(target, path.as_str()))
        })
}

fn mutation_tool_requires_evidence(tc: &ToolCallData) -> bool {
    matches!(tc.name.as_str(), "patch_file" | "apply_diff")
}

fn extract_loose_json_string_field(raw: &str, field: &str) -> Option<String> {
    let needle = format!("\"{field}\"");
    let start = raw.find(needle.as_str())?;
    let after_key = &raw[start + needle.len()..];
    let colon = after_key.find(':')?;
    let mut tail = after_key[colon + 1..].trim_start();
    if !tail.starts_with('"') {
        return None;
    }
    tail = &tail[1..];

    let mut out = String::new();
    let mut chars = tail.chars();
    let mut escaped = false;
    while let Some(ch) = chars.next() {
        if escaped {
            match ch {
                '"' => out.push('"'),
                '\\' => out.push('\\'),
                '/' => out.push('/'),
                'n' => out.push('\n'),
                'r' => out.push('\r'),
                't' => out.push('\t'),
                'b' => out.push('\u{0008}'),
                'f' => out.push('\u{000C}'),
                'u' => {
                    let mut hex = String::new();
                    for _ in 0..4 {
                        let Some(h) = chars.next() else {
                            return Some(out);
                        };
                        hex.push(h);
                    }
                    if let Ok(value) = u16::from_str_radix(hex.as_str(), 16) {
                        if let Some(decoded) = char::from_u32(value as u32) {
                            out.push(decoded);
                        }
                    }
                }
                other => out.push(other),
            }
            escaped = false;
            continue;
        }
        match ch {
            '\\' => escaped = true,
            '"' => return Some(out),
            other => out.push(other),
        }
    }

    Some(out)
}

fn mutation_target_path(tc: &ToolCallData) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(&tc.arguments)
        .ok()
        .and_then(|args| {
            args.get("path")
                .and_then(|v| v.as_str())
                .map(str::to_string)
        })
        .or_else(|| extract_loose_json_string_field(tc.arguments.as_str(), "path"))
        .map(|path| compact_one_line(path.trim(), 180))
        .filter(|path| path != "-")
}

fn validate_evidence_block(
    evidence_block: &EvidenceBlock,
    tc: &ToolCallData,
    observations: &ObservationEvidence,
) -> Result<()> {
    if evidence_block.target_files.is_empty() {
        return Err(anyhow!(
            governor_contract::evidence_missing_target_files_message()
        ));
    }
    if evidence_block.evidence.trim().is_empty() {
        return Err(anyhow!(
            governor_contract::evidence_missing_evidence_message()
        ));
    }
    if evidence_block.open_questions.trim().is_empty() {
        return Err(anyhow!(
            governor_contract::evidence_missing_open_questions_message()
        ));
    }
    if evidence_block.next_probe.trim().is_empty() {
        return Err(anyhow!(
            governor_contract::evidence_missing_next_probe_message()
        ));
    }
    if !mutation_tool_requires_evidence(tc) {
        return Ok(());
    }
    let Some(target_path) = mutation_target_path(tc) else {
        return Err(anyhow!(
            governor_contract::evidence_unresolved_path_message()
        ));
    };
    if !evidence_block
        .target_files
        .iter()
        .any(|path| evidence_path_matches(path.as_str(), target_path.as_str()))
    {
        return Err(anyhow!(
            governor_contract::evidence_target_mismatch_message(target_path.as_str())
        ));
    }
    if !observation_supports_target_path(target_path.as_str(), observations) {
        return Err(anyhow!(
            governor_contract::evidence_missing_observation_message(target_path.as_str())
        ));
    }
    Ok(())
}

fn build_evidence_gate_prompt(
    tc: &ToolCallData,
    observations: &ObservationEvidence,
    ledger: &AssumptionLedger,
) -> String {
    let target_path = mutation_target_path(tc).unwrap_or_else(|| "<path>".to_string());
    let mut out = format!(
        "[Evidence Gate]\n\
You are about to mutate an existing file via {}.\n\
Target path: {target_path}\n\
\n\
Before this mutation, emit exactly:\n\
<evidence>\n\
target_files: 1) <exact target path>\n\
target_symbols: 1) <symbol or area>\n\
evidence: <what previous read/search proved>\n\
open_questions: <what is still uncertain or `none`>\n\
next_probe: <exact next action or edit target>\n\
</evidence>\n\
\n\
Rules:\n\
- `target_files` must include the actual file you are about to change.\n\
- Base the block on real prior read/search evidence from this session.\n\
- If the target is not yet supported by evidence, do NOT mutate; call one diagnostic tool instead.\n\
- After the <evidence> block, call exactly one tool.",
        tc.name
    );

    if !observations.reads.is_empty() {
        out.push_str("\n\n[Observed reads]\n");
        for read in observations.reads.iter().rev().take(3).rev() {
            out.push_str(&format!("- {} -> {}\n", read.command, read.path));
        }
    }
    if !observations.searches.is_empty() {
        out.push_str("[Observed searches]\n");
        for search in observations.searches.iter().rev().take(3).rev() {
            let path_summary = if search.paths.is_empty() {
                "(no paths)".to_string()
            } else {
                search
                    .paths
                    .iter()
                    .take(3)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            out.push_str(&format!(
                "- {} -> hits={} paths={}\n",
                search.command, search.hit_count, path_summary
            ));
        }
    }
    if ledger.has_refuted() {
        out.push_str("[Refuted assumptions]\n");
        for entry in ledger
            .entries
            .iter()
            .filter(|entry| entry.status == AssumptionStatus::Refuted)
            .take(2)
        {
            out.push_str(&format!("- {}\n", entry.text));
        }
    }
    out
}

fn first_slash_literal(text: &str) -> Option<String> {
    text.split_whitespace().find_map(|token| {
        let trimmed = token
            .trim_matches(|ch: char| {
                matches!(ch, '`' | '"' | '\'' | ',' | '.' | ':' | ';' | ')' | '(')
            })
            .trim();
        if !trimmed.starts_with('/') || trimmed.len() < 2 {
            return None;
        }
        let tail = &trimmed[1..];
        if tail
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '/'))
        {
            Some(trimmed.to_string())
        } else {
            None
        }
    })
}

fn parse_leading_ordinal(reference: &str) -> Option<usize> {
    let trimmed = reference.trim_start();
    let digits_len = trimmed.chars().take_while(|ch| ch.is_ascii_digit()).count();
    if digits_len == 0 {
        return None;
    }
    let digits = &trimmed[..digits_len];
    let rest = trimmed[digits_len..].trim_start();
    let Some(number) = digits.parse::<usize>().ok() else {
        return None;
    };
    if rest.is_empty()
        || rest.starts_with(')')
        || rest.starts_with('.')
        || rest.starts_with(':')
        || rest.starts_with('-')
    {
        Some(number)
    } else {
        None
    }
}

fn resolve_acceptance_reference(reference: &str, plan: &PlanBlock) -> Option<usize> {
    let reference_sig = normalize_memory_entry(reference);
    if reference_sig.is_empty() {
        return None;
    }

    if let Some(n) = parse_leading_ordinal(reference) {
        if n <= plan.acceptance_criteria.len() {
            return Some(n - 1);
        }
    }

    if reference_sig.contains("acceptance")
        || reference_sig.contains("criterion")
        || reference_sig.contains("criteria")
    {
        if let Some(n) = parse_first_usize(reference) {
            if n <= plan.acceptance_criteria.len() {
                return Some(n - 1);
            }
        }
    }

    plan.acceptance_criteria
        .iter()
        .position(|criterion| impact_progress_matches_entry(reference, criterion))
}

fn acceptance_reference_label(plan: &PlanBlock, idx: usize) -> String {
    let criterion = plan
        .acceptance_criteria
        .get(idx)
        .map(|s| s.as_str())
        .unwrap_or("-");
    format!("acceptance {}: {}", idx + 1, criterion)
}

fn collect_successful_observation_commands(messages: &[serde_json::Value]) -> Vec<String> {
    let mut pending: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut commands = Vec::new();

    for msg in messages {
        let role = msg
            .get("role")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        if role == "assistant" {
            let Some(tool_calls) = msg.get("tool_calls").and_then(|v| v.as_array()) else {
                continue;
            };
            for tc in tool_calls {
                let id = tc.get("id").and_then(|v| v.as_str()).unwrap_or("").trim();
                if id.is_empty() {
                    continue;
                }
                let name = tc
                    .get("function")
                    .and_then(|f| f.get("name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim();
                if !matches!(name, "read_file" | "list_dir" | "glob" | "search_files") {
                    continue;
                }
                let arguments = tc
                    .get("function")
                    .and_then(|f| f.get("arguments"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim();
                let Some(signature) = canonicalize_tool_call_command(name, arguments) else {
                    continue;
                };
                pending.insert(id.to_string(), signature);
            }
            continue;
        }

        if role != "tool" {
            continue;
        }

        let tool_call_id = msg
            .get("tool_call_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        if tool_call_id.is_empty() {
            continue;
        }
        let Some(signature) = pending.remove(tool_call_id) else {
            continue;
        };
        let content = msg.get("content").and_then(|v| v.as_str()).unwrap_or("");
        if non_exec_tool_succeeded(content) {
            remember_recent_unique(&mut commands, signature.as_str(), 12, 200);
        }
    }

    commands
}

fn collect_successful_mutation_commands(messages: &[serde_json::Value]) -> Vec<String> {
    let mut pending: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut commands = Vec::new();

    for msg in messages {
        let role = msg
            .get("role")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        if role == "assistant" {
            let Some(tool_calls) = msg.get("tool_calls").and_then(|v| v.as_array()) else {
                continue;
            };
            for tc in tool_calls {
                let id = tc.get("id").and_then(|v| v.as_str()).unwrap_or("").trim();
                if id.is_empty() {
                    continue;
                }
                let name = tc
                    .get("function")
                    .and_then(|f| f.get("name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim();
                if !matches!(name, "write_file" | "patch_file" | "apply_diff") {
                    continue;
                }
                let arguments = tc
                    .get("function")
                    .and_then(|f| f.get("arguments"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim();
                let Some(signature) = canonicalize_tool_call_command(name, arguments) else {
                    continue;
                };
                pending.insert(id.to_string(), signature);
            }
            continue;
        }

        if role != "tool" {
            continue;
        }

        let tool_call_id = msg
            .get("tool_call_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        if tool_call_id.is_empty() {
            continue;
        }
        let Some(signature) = pending.remove(tool_call_id) else {
            continue;
        };
        let content = msg.get("content").and_then(|v| v.as_str()).unwrap_or("");
        if non_exec_tool_succeeded(content) {
            remember_recent_unique(&mut commands, signature.as_str(), 12, 200);
        }
    }

    commands
}

fn collect_successful_auto_test_verification_commands(
    messages: &[serde_json::Value],
    test_cmd: Option<&str>,
) -> Vec<String> {
    let Some(test_cmd) = test_cmd.map(str::trim).filter(|cmd| !cmd.is_empty()) else {
        return Vec::new();
    };
    let mut pending_ids: std::collections::BTreeMap<String, String> =
        std::collections::BTreeMap::new();
    let mut commands = Vec::new();

    for msg in messages {
        let role = msg
            .get("role")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        if role == "assistant" {
            let Some(tool_calls) = msg.get("tool_calls").and_then(|v| v.as_array()) else {
                continue;
            };
            for tc in tool_calls {
                let id = tc.get("id").and_then(|v| v.as_str()).unwrap_or("").trim();
                if id.is_empty() {
                    continue;
                }
                let name = tc
                    .get("function")
                    .and_then(|f| f.get("name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim();
                if matches!(name, "write_file" | "patch_file" | "apply_diff") {
                    pending_ids.insert(id.to_string(), name.to_string());
                }
            }
            continue;
        }

        if role != "tool" {
            continue;
        }

        let tool_call_id = msg
            .get("tool_call_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        let Some(name) = pending_ids.remove(tool_call_id) else {
            continue;
        };
        let content = msg.get("content").and_then(|v| v.as_str()).unwrap_or("");
        if crate::execution_evidence::auto_test_succeeded(&name, content) {
            remember_recent_unique(&mut commands, test_cmd, 8, 1024);
        }
    }

    commands
}

fn collect_known_acceptance_commands(
    messages: &[serde_json::Value],
    working_mem: &WorkingMemory,
    test_cmd: Option<&str>,
) -> Vec<String> {
    let mut commands = Vec::new();
    for command in &working_mem.successful_verifications {
        remember_recent_unique(&mut commands, command.as_str(), 16, 1024);
    }
    for command in collect_successful_auto_test_verification_commands(messages, test_cmd) {
        remember_recent_unique(&mut commands, command.as_str(), 16, 1024);
    }
    for command in collect_successful_mutation_commands(messages) {
        remember_recent_unique(&mut commands, command.as_str(), 16, 1024);
    }
    for command in collect_successful_observation_commands(messages) {
        remember_recent_unique(&mut commands, command.as_str(), 16, 1024);
    }
    commands
}

fn maybe_build_verified_action_closeout_text(
    root_user_text: &str,
    plan: Option<&PlanBlock>,
    messages: &[serde_json::Value],
    working_mem: &WorkingMemory,
    observation_evidence: &ObservationEvidence,
    required_verification: VerificationLevel,
    test_cmd: Option<&str>,
    last_mutation_step: Option<usize>,
    last_verify_ok_step: Option<usize>,
) -> Option<String> {
    if requires_authored_final_answer(root_user_text) {
        return None;
    }
    if latest_assistant_message_is_done(messages) {
        return None;
    }
    if required_existing_followup_no_tool_hint(messages, root_user_text).is_some() {
        return None;
    }
    if benchmark_plan_missing_required_exec_proof(root_user_text, messages, test_cmd) {
        return None;
    }

    let known_acceptance_commands = canonicalize_known_acceptance_commands(
        &collect_known_acceptance_commands(messages, working_mem, test_cmd),
        observation_evidence,
    );

    maybe_build_verified_action_auto_final_answer(
        plan,
        messages,
        &known_acceptance_commands,
        observation_evidence,
        required_verification,
        test_cmd,
        last_mutation_step,
        last_verify_ok_step,
    )
}

fn build_impact_prompt(
    reason: &str,
    plan: Option<&PlanBlock>,
    working_mem: &WorkingMemory,
) -> String {
    let goal = plan
        .map(|p| compact_one_line(p.goal.as_str(), 100))
        .unwrap_or_else(|| "-".to_string());

    let mut out = format!(
        "[Impact Check Required]\n\
Reason: {reason}\n\
Current goal: {goal}\n\
\n\
Before your next tool call, emit exactly:\n\
<impact>\n\
changed: <one short sentence>\n\
progress: <which plan step or acceptance criterion moved>\n\
remaining_gap: <one short sentence>\n\
</impact>\n\
\n\
Rules:\n\
- Keep the whole block under 60 tokens.\n\
- Mention the actual mutation effect, not intent.\n\
- `progress` must name a real step or acceptance criterion.\n\
- After the <impact> block, call exactly one tool."
    );

    if !working_mem.completed_steps.is_empty() {
        out.push_str("\n\n[Already Completed]\n");
        for step in working_mem.completed_steps.iter().rev().take(2).rev() {
            out.push_str(&format!("- {step}\n"));
        }
    }

    if let Some(plan) = plan {
        if !plan.steps.is_empty() {
            out.push_str("\n[Plan Steps]\n");
            for (idx, step) in plan.steps.iter().enumerate() {
                out.push_str(&format!("- step {}: {}\n", idx + 1, step));
            }
        }
        if !plan.acceptance_criteria.is_empty() {
            out.push_str("[Acceptance Criteria]\n");
            for (idx, criterion) in plan.acceptance_criteria.iter().enumerate() {
                out.push_str(&format!("- acceptance {}: {}\n", idx + 1, criterion));
            }
        }
    }

    out
}

fn extract_tag_block<'a>(text: &'a str, tag: &str) -> Option<&'a str> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = text.find(&open)?;
    let rest = &text[start + open.len()..];
    let end = rest
        .find(&close)
        .or_else(|| rest.find(&format!("</{tag}")))?;
    Some(rest[..end].trim_end())
}

fn parse_first_usize(s: &str) -> Option<usize> {
    let mut digits = String::new();
    for ch in s.chars() {
        if ch.is_ascii_digit() {
            digits.push(ch);
        } else if !digits.is_empty() {
            break;
        }
    }
    digits.parse::<usize>().ok().filter(|n| *n > 0)
}

#[derive(Debug, Clone)]
enum ParsedBlockValue {
    Text(String),
    List(Vec<String>),
    PositiveInt(usize),
    Canonical(String),
}

impl ParsedBlockValue {
    fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(value) | Self::Canonical(value) => Some(value.as_str()),
            Self::List(_) | Self::PositiveInt(_) => None,
        }
    }

    fn as_list(&self) -> Option<&[String]> {
        match self {
            Self::List(items) => Some(items.as_slice()),
            Self::Text(_) | Self::PositiveInt(_) | Self::Canonical(_) => None,
        }
    }

    fn as_positive_int(&self) -> Option<usize> {
        match self {
            Self::PositiveInt(value) => Some(*value),
            Self::Text(_) | Self::List(_) | Self::Canonical(_) => None,
        }
    }
}

fn parse_numbered_steps(s: &str) -> Vec<String> {
    let normalized = s.replace("\r\n", "\n").replace('\n', " ");
    let bytes = normalized.as_bytes();
    let mut markers: Vec<(usize, usize)> = Vec::new();
    let mut i = 0usize;

    while i < bytes.len() {
        if bytes[i].is_ascii_digit() && (i == 0 || bytes[i - 1].is_ascii_whitespace()) {
            let marker_start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i < bytes.len() && matches!(bytes[i], b')' | b'.' | b':' | b'-') {
                i += 1;
                while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                    i += 1;
                }
                markers.push((marker_start, i));
                continue;
            }
        }
        i += 1;
    }

    if markers.is_empty() {
        return Vec::new();
    }

    let mut steps = Vec::new();
    for (idx, (_, content_start)) in markers.iter().enumerate() {
        let content_end = markers
            .get(idx + 1)
            .map(|(next_start, _)| *next_start)
            .unwrap_or(normalized.len());
        let step = normalized[*content_start..content_end]
            .trim()
            .trim_end_matches(';')
            .trim()
            .to_string();
        if !step.is_empty() {
            steps.push(step);
        }
    }

    steps
}

fn parse_plan_items(s: &str) -> Vec<String> {
    let numbered = parse_numbered_steps(s);
    if !numbered.is_empty() {
        return numbered;
    }

    s.replace("\r\n", "\n")
        .split(['\n', ';'])
        .map(|part| compact_one_line(part.trim(), 120))
        .filter(|part| part != "-")
        .collect()
}

fn parse_assumption_items(s: &str) -> Vec<String> {
    parse_plan_items(s)
        .into_iter()
        .map(|item| compact_one_line(item.as_str(), 140))
        .filter(|item| item != "-")
        .take(6)
        .collect()
}

fn parse_block_fields(text: &str, tag: &str) -> Option<BTreeMap<String, ParsedBlockValue>> {
    let body = extract_tag_block(text, tag)?;
    let mut out = BTreeMap::new();

    for (raw_key, raw_value) in parse_tag_fields(body, tag) {
        let Some(field) = governor_contract::block_field(tag, raw_key.as_str()) else {
            continue;
        };
        let parsed = match field.kind.as_deref() {
            Some("list") => ParsedBlockValue::List(parse_plan_items(raw_value.as_str())),
            Some("positive_int") => {
                ParsedBlockValue::PositiveInt(parse_first_usize(raw_value.as_str()).unwrap_or(0))
            }
            Some("tool_name") | Some("enum") => ParsedBlockValue::Canonical(
                governor_contract::canonical_field_value(
                    tag,
                    field.key.as_str(),
                    raw_value.as_str(),
                )
                .unwrap_or_default(),
            ),
            _ => ParsedBlockValue::Text(raw_value),
        };
        out.insert(field.key.clone(), parsed);
    }

    Some(out)
}

fn block_text_value(fields: &BTreeMap<String, ParsedBlockValue>, key: &str) -> String {
    fields
        .get(key)
        .and_then(ParsedBlockValue::as_text)
        .unwrap_or_default()
        .to_string()
}

fn block_list_value(fields: &BTreeMap<String, ParsedBlockValue>, key: &str) -> Vec<String> {
    fields
        .get(key)
        .and_then(ParsedBlockValue::as_list)
        .map(|items| items.to_vec())
        .unwrap_or_default()
}

fn block_usize_value(fields: &BTreeMap<String, ParsedBlockValue>, key: &str) -> usize {
    fields
        .get(key)
        .and_then(ParsedBlockValue::as_positive_int)
        .unwrap_or(0)
}

fn plan_field_min_items(key: &str) -> Option<usize> {
    governor_contract::block_field("plan", key).and_then(|field| field.min_items)
}

fn plan_field_max_items(key: &str) -> Option<usize> {
    governor_contract::block_field("plan", key).and_then(|field| field.max_items)
}

fn parse_plan_block(text: &str) -> Option<PlanBlock> {
    let fields = parse_block_fields(text, "plan")?;

    Some(PlanBlock {
        goal: block_text_value(&fields, "goal"),
        steps: block_list_value(&fields, "steps"),
        acceptance_criteria: block_list_value(&fields, "acceptance"),
        risks: block_text_value(&fields, "risks"),
        assumptions: block_text_value(&fields, "assumptions"),
    })
}

fn parse_think_block(text: &str) -> Option<ThinkBlock> {
    let fields = parse_block_fields(text, "think")?;

    Some(ThinkBlock {
        goal: block_text_value(&fields, "goal"),
        step: block_usize_value(&fields, "step"),
        tool: block_text_value(&fields, "tool"),
        risk: block_text_value(&fields, "risk"),
        doubt: block_text_value(&fields, "doubt"),
        next: block_text_value(&fields, "next"),
        verify: block_text_value(&fields, "verify"),
    })
}

fn json_string_field(
    obj: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Option<String> {
    match obj.get(key) {
        Some(serde_json::Value::String(s)) => Some(s.trim().to_string()),
        Some(serde_json::Value::Number(n)) => Some(n.to_string()),
        Some(serde_json::Value::Bool(b)) => Some(b.to_string()),
        _ => None,
    }
}

fn json_list_field(obj: &serde_json::Map<String, serde_json::Value>, key: &str) -> Vec<String> {
    obj.get(key)
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|item| match item {
                    serde_json::Value::String(s) => Some(compact_one_line(s.trim(), 120)),
                    serde_json::Value::Number(n) => Some(n.to_string()),
                    serde_json::Value::Bool(b) => Some(b.to_string()),
                    _ => None,
                })
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

fn pseudo_block_text_from_named_args(name: &str, args: &serde_json::Value) -> Option<String> {
    let obj = args.as_object()?;

    match name.trim_matches(|c| c == '<' || c == '>') {
        "plan" => {
            let goal = json_string_field(obj, "goal")?;
            let steps = json_list_field(obj, "steps");
            let acceptance = json_list_field(obj, "acceptance");
            let risks = json_string_field(obj, "risks")
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "unknown risks; inspect carefully before editing".to_string());
            let assumptions = json_string_field(obj, "assumptions")
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "current repo scan reflects the active workspace".to_string());
            let steps_line = steps
                .iter()
                .enumerate()
                .map(|(i, s)| format!("{} ) {}", i + 1, s))
                .collect::<Vec<_>>()
                .join(" ")
                .replace(" )", ")");
            let acceptance_line = acceptance
                .iter()
                .enumerate()
                .map(|(i, s)| format!("{} ) {}", i + 1, s))
                .collect::<Vec<_>>()
                .join(" ")
                .replace(" )", ")");
            Some(format!(
                "<plan>\n\
goal: {goal}\n\
steps: {steps_line}\n\
acceptance: {acceptance_line}\n\
risks: {risks}\n\
assumptions: {assumptions}\n\
</plan>\n"
            ))
        }
        "think" => {
            let goal = json_string_field(obj, "goal")?;
            let step = json_string_field(obj, "step").unwrap_or_default();
            let tool = json_string_field(obj, "tool").unwrap_or_default();
            let risk = json_string_field(obj, "risk")
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "wrong file or target".to_string());
            let doubt = json_string_field(obj, "doubt")
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "may need a broader check".to_string());
            let next = json_string_field(obj, "next").unwrap_or_default();
            let verify = json_string_field(obj, "verify")
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "confirm the result matches the request".to_string());
            Some(format!(
                "<think>\n\
goal: {goal}\n\
step: {step}\n\
tool: {tool}\n\
risk: {risk}\n\
doubt: {doubt}\n\
next: {next}\n\
verify: {verify}\n\
</think>\n"
            ))
        }
        _ => None,
    }
}

fn pseudo_tool_call_to_block_text(tc: &ToolCallData) -> Option<String> {
    let args: serde_json::Value = serde_json::from_str(&tc.arguments).ok()?;
    pseudo_block_text_from_named_args(&tc.name, &args)
}

fn known_runtime_tool_name_from_text(text: &str) -> Option<String> {
    const TOOL_NAMES: [&str; 9] = [
        "exec",
        "read_file",
        "write_file",
        "patch_file",
        "apply_diff",
        "search_files",
        "list_dir",
        "glob",
        "done",
    ];

    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    let low = trimmed.to_ascii_lowercase();
    for tool in TOOL_NAMES {
        if low == tool {
            return Some(tool.to_string());
        }
    }
    for tool in TOOL_NAMES {
        if let Some(prefix) = low.strip_suffix(tool) {
            let boundary_ok = prefix
                .chars()
                .last()
                .map(|ch| !ch.is_ascii_alphanumeric() && ch != '_')
                .unwrap_or(true);
            if boundary_ok {
                return Some(tool.to_string());
            }
        }
    }
    None
}

fn parse_reflection_block(text: &str) -> Option<ReflectionBlock> {
    let fields = parse_block_fields(text, "reflect")?;

    Some(ReflectionBlock {
        last_outcome: block_text_value(&fields, "last_outcome"),
        goal_delta: GoalDelta::parse(block_text_value(&fields, "goal_delta").as_str()),
        wrong_assumption: block_text_value(&fields, "wrong_assumption"),
        strategy_change: StrategyChange::parse(
            block_text_value(&fields, "strategy_change").as_str(),
        ),
        next_minimal_action: block_text_value(&fields, "next_minimal_action"),
    })
}

fn last_plan_from_messages(messages: &[serde_json::Value]) -> Option<PlanBlock> {
    for msg in messages.iter().rev() {
        if msg.get("role").and_then(|r| r.as_str()) != Some("assistant") {
            continue;
        }
        let content = msg.get("content").and_then(|c| c.as_str()).unwrap_or("");
        if let Some(plan) = parse_plan_block(content) {
            return Some(plan);
        }
    }
    None
}

fn last_think_from_messages(messages: &[serde_json::Value]) -> Option<ThinkBlock> {
    for msg in messages.iter().rev() {
        if msg.get("role").and_then(|r| r.as_str()) != Some("assistant") {
            continue;
        }
        let content = msg.get("content").and_then(|c| c.as_str()).unwrap_or("");
        if let Some(think) = parse_think_block(content) {
            return Some(think);
        }
    }
    None
}

fn last_reflection_from_messages(messages: &[serde_json::Value]) -> Option<ReflectionBlock> {
    for msg in messages.iter().rev() {
        if msg.get("role").and_then(|r| r.as_str()) != Some("assistant") {
            continue;
        }
        let content = msg.get("content").and_then(|c| c.as_str()).unwrap_or("");
        if let Some(r) = parse_reflection_block(content) {
            return Some(r);
        }
    }
    None
}

fn infer_required_verification_level(
    root_user_text: &str,
    test_cmd: Option<&str>,
) -> VerificationLevel {
    let low = root_user_text.to_ascii_lowercase();
    let verification = governor_contract::verification();
    let looks_doc_only = text_contains_any(&low, &verification.intent_doc_terms)
        && !text_contains_any(&low, &verification.intent_behavioral_terms);
    if looks_doc_only {
        return VerificationLevel::Build;
    }
    if test_cmd.is_some() || text_contains_any(&low, &verification.intent_behavioral_terms) {
        return VerificationLevel::Behavioral;
    }
    VerificationLevel::Build
}

fn verification_level_from_plan(plan: &PlanBlock) -> Option<VerificationLevel> {
    if plan.acceptance_criteria.is_empty() {
        return None;
    }

    let verification = governor_contract::verification();

    let mut saw_build_like = false;
    for criterion in &plan.acceptance_criteria {
        let low = criterion.to_ascii_lowercase();
        if text_contains_any(&low, &verification.plan_behavioral_terms) {
            return Some(VerificationLevel::Behavioral);
        }
        if text_contains_any(&low, &verification.plan_build_terms) {
            saw_build_like = true;
            continue;
        }
        return Some(VerificationLevel::Behavioral);
    }

    if saw_build_like {
        Some(VerificationLevel::Build)
    } else {
        None
    }
}

fn verification_level_for_mutation_path(path: &str) -> VerificationLevel {
    let low = path.trim().to_ascii_lowercase();
    if low.is_empty() {
        return VerificationLevel::Build;
    }
    let verification = governor_contract::verification();
    if text_contains_any(&low, &verification.doc_path_terms) {
        return VerificationLevel::Build;
    }

    if verification
        .behavioral_path_extensions
        .iter()
        .any(|ext| low.ends_with(ext))
    {
        return VerificationLevel::Behavioral;
    }

    VerificationLevel::Build
}

fn effective_verify_ok_step(
    required: VerificationLevel,
    last_build_verify_ok_step: Option<usize>,
    last_behavioral_verify_ok_step: Option<usize>,
) -> Option<usize> {
    match required {
        VerificationLevel::Build => last_build_verify_ok_step
            .into_iter()
            .chain(last_behavioral_verify_ok_step)
            .max(),
        VerificationLevel::Behavioral => last_behavioral_verify_ok_step,
    }
}

fn adopt_valid_plan(
    plan: &PlanBlock,
    working_mem: &mut WorkingMemory,
    assumption_ledger: &mut AssumptionLedger,
    active_plan: &mut Option<PlanBlock>,
    intent_required_verification: &mut VerificationLevel,
    path_required_verification: VerificationLevel,
    required_verification: &mut VerificationLevel,
    recovery: &mut RecoveryGovernor,
    last_verify_ok_step: &mut Option<usize>,
    last_build_verify_ok_step: Option<usize>,
    last_behavioral_verify_ok_step: Option<usize>,
) {
    working_mem.sync_to_plan(plan);
    assumption_ledger.sync_to_plan(plan);
    assumption_ledger.refresh_confirmations(working_mem);
    if let Some(level) = verification_level_from_plan(plan) {
        *intent_required_verification = level;
        *required_verification = (*intent_required_verification).max(path_required_verification);
        recovery.required_verification = *required_verification;
        *last_verify_ok_step = effective_verify_ok_step(
            *required_verification,
            last_build_verify_ok_step,
            last_behavioral_verify_ok_step,
        );
    }
    *active_plan = Some(plan.clone());
}

fn upgrade_required_verification_from_messages(
    messages: &[serde_json::Value],
    base: VerificationLevel,
) -> VerificationLevel {
    let mut required = base;
    let mut by_id: std::collections::HashMap<String, (String, String)> =
        std::collections::HashMap::new();

    for msg in messages {
        let role = msg.get("role").and_then(|v| v.as_str()).unwrap_or("");
        if role == "assistant" {
            let Some(tcs) = msg.get("tool_calls").and_then(|v| v.as_array()) else {
                continue;
            };
            for tc in tcs {
                let id = tc.get("id").and_then(|v| v.as_str()).unwrap_or("").trim();
                if id.is_empty() {
                    continue;
                }
                let name = tc
                    .get("function")
                    .and_then(|f| f.get("name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string();
                let args = tc
                    .get("function")
                    .and_then(|f| f.get("arguments"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim();
                let path = if matches!(name.as_str(), "write_file" | "patch_file" | "apply_diff") {
                    serde_json::from_str::<serde_json::Value>(args)
                        .ok()
                        .and_then(|v| {
                            v.get("path")
                                .and_then(|p| p.as_str())
                                .map(|s| s.to_string())
                        })
                        .unwrap_or_default()
                } else {
                    String::new()
                };
                by_id.insert(id.to_string(), (name, path));
            }
            continue;
        }

        if role != "tool" {
            continue;
        }

        let id = msg
            .get("tool_call_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        if id.is_empty() {
            continue;
        }
        let Some((name, path)) = by_id.remove(id) else {
            continue;
        };
        let content = msg.get("content").and_then(|v| v.as_str()).unwrap_or("");
        if matches!(name.as_str(), "write_file" | "patch_file" | "apply_diff")
            && content.contains("[hash]")
        {
            required = required.max(verification_level_for_mutation_path(path.as_str()));
        }
    }

    required
}

fn last_impact_step_from_messages(messages: &[serde_json::Value]) -> Option<usize> {
    let mut step_seq = 0usize;
    let mut last_impact_step = None;

    for msg in messages {
        let role = msg.get("role").and_then(|v| v.as_str()).unwrap_or("");
        if role == "assistant" {
            let content = msg.get("content").and_then(|c| c.as_str()).unwrap_or("");
            if parse_impact_block(content).is_some() {
                last_impact_step = Some(step_seq);
            }
            continue;
        }

        if role == "tool" {
            step_seq = step_seq.saturating_add(1);
        }
    }

    last_impact_step
}

fn validate_plan(plan: &PlanBlock) -> Result<()> {
    if plan.goal.trim().is_empty() {
        return Err(anyhow!(governor_contract::plan_missing_goal_message()));
    }
    let min_steps = plan_field_min_items("steps").unwrap_or(2);
    let max_steps = plan_field_max_items("steps").unwrap_or(7);
    let min_acceptance = plan_field_min_items("acceptance").unwrap_or(1);
    let max_acceptance = plan_field_max_items("acceptance").unwrap_or(4);
    if plan.steps.is_empty() {
        return Err(anyhow!(governor_contract::plan_missing_steps_message()));
    }
    if plan.steps.len() < min_steps {
        return Err(anyhow!(governor_contract::plan_min_steps_message(
            min_steps
        )));
    }
    if plan.steps.len() > max_steps {
        return Err(anyhow!(governor_contract::plan_max_steps_message(
            max_steps
        )));
    }
    if plan.acceptance_criteria.is_empty() {
        return Err(anyhow!(governor_contract::plan_missing_acceptance_message()));
    }
    if plan.acceptance_criteria.len() < min_acceptance {
        return Err(anyhow!(governor_contract::plan_min_acceptance_message(
            min_acceptance
        )));
    }
    if plan.acceptance_criteria.len() > max_acceptance {
        return Err(anyhow!(governor_contract::plan_max_acceptance_message(
            max_acceptance
        )));
    }
    if plan.risks.trim().is_empty() {
        return Err(anyhow!(governor_contract::plan_missing_risks_message()));
    }
    if plan.assumptions.trim().is_empty() {
        return Err(anyhow!(
            governor_contract::plan_missing_assumptions_message()
        ));
    }
    if plan.steps.iter().any(|step| step.trim().is_empty()) {
        return Err(anyhow!(governor_contract::plan_empty_step_message()));
    }
    if plan
        .acceptance_criteria
        .iter()
        .any(|criterion| criterion.trim().is_empty())
    {
        return Err(anyhow!(governor_contract::plan_empty_acceptance_message()));
    }
    Ok(())
}

#[cfg_attr(not(test), allow(dead_code))]
fn validate_plan_for_task(plan: &PlanBlock, root_read_only: bool) -> Result<()> {
    validate_plan(plan)?;
    if root_read_only {
        if let Some(msg) = read_only_plan_violation(plan) {
            return Err(anyhow!(msg));
        }
    }
    Ok(())
}

fn validate_plan_against_instruction_resolver(
    plan: &PlanBlock,
    resolver: &InstructionResolver,
) -> Result<()> {
    if let Some(conflict) = resolver.plan_conflict(plan) {
        return Err(anyhow!(conflict.render()));
    }
    Ok(())
}

fn validate_plan_for_task_contract(
    plan: &PlanBlock,
    root_read_only: bool,
    contract: &TaskContract,
    resolver: &InstructionResolver,
) -> Result<()> {
    validate_plan(plan)?;
    validate_plan_against_task_contract(plan, contract)?;
    if root_read_only {
        validate_plan_against_instruction_resolver(plan, resolver)?;
    }
    Ok(())
}

fn think_next_matches_exec_command(next: &str, command: &str) -> bool {
    let next_sig = command_sig(next);
    let cmd_sig = command_sig(command);
    if next_sig.is_empty() || cmd_sig.is_empty() {
        return false;
    }
    if cmd_sig.contains(&next_sig) || next_sig.contains(&cmd_sig) {
        return true;
    }

    let next_prefix = next_sig
        .split_whitespace()
        .take(2)
        .collect::<Vec<_>>()
        .join(" ");
    let cmd_prefix = cmd_sig
        .split_whitespace()
        .take(2)
        .collect::<Vec<_>>()
        .join(" ");
    !next_prefix.is_empty() && next_prefix == cmd_prefix
}

fn validate_think(think: &ThinkBlock, plan: &PlanBlock, tc: &ToolCallData) -> Result<()> {
    if think.goal.trim().is_empty() {
        return Err(anyhow!(governor_contract::think_missing_goal_message()));
    }
    if think.step == 0 {
        return Err(anyhow!(governor_contract::think_invalid_step_message()));
    }
    if think.step > plan.steps.len() {
        return Err(anyhow!(governor_contract::think_step_out_of_range_message(
            think.step,
            plan.steps.len()
        )));
    }
    if think.tool.trim().is_empty() {
        return Err(anyhow!(governor_contract::think_invalid_tool_message()));
    }
    if think.risk.trim().is_empty() {
        return Err(anyhow!(governor_contract::think_missing_risk_message()));
    }
    if think.doubt.trim().is_empty() {
        return Err(anyhow!(governor_contract::think_missing_doubt_message()));
    }
    if think.next.trim().is_empty() {
        return Err(anyhow!(governor_contract::think_missing_next_message()));
    }
    if think.verify.trim().is_empty() {
        return Err(anyhow!(governor_contract::think_missing_verify_message()));
    }
    if think.tool != tc.name {
        return Err(anyhow!(governor_contract::think_tool_mismatch_message(
            &think.tool,
            &tc.name
        )));
    }

    if tc.name == "exec" {
        let command = parse_exec_command_from_args(&tc.arguments)
            .unwrap_or_else(|| tc.arguments.trim().to_string());
        if !think_next_matches_exec_command(&think.next, &command) {
            return Err(anyhow!(
                governor_contract::think_exec_prefix_mismatch_message()
            ));
        }
    }

    Ok(())
}

fn validate_reflection(
    r: &ReflectionBlock,
    mem: &FailureMemory,
    file_tool_consec_failures: usize,
) -> Result<()> {
    if r.last_outcome.trim().is_empty() {
        return Err(anyhow!(
            governor_contract::reflection_missing_last_outcome_message()
        ));
    }
    if r.wrong_assumption.trim().is_empty() {
        return Err(anyhow!(
            governor_contract::reflection_missing_wrong_assumption_message()
        ));
    }
    if r.next_minimal_action.trim().is_empty() {
        return Err(anyhow!(
            governor_contract::reflection_missing_next_minimal_action_message()
        ));
    }
    if r.goal_delta == GoalDelta::Unknown {
        return Err(anyhow!(
            governor_contract::reflection_invalid_goal_delta_message()
        ));
    }
    if r.strategy_change == StrategyChange::Unknown {
        return Err(anyhow!(
            governor_contract::reflection_invalid_strategy_change_message()
        ));
    }

    let repeated_failure = mem.same_error_repeats >= 2
        || mem.same_command_repeats >= 3
        || mem.same_output_repeats >= 2;
    let repeated_failure = repeated_failure || file_tool_consec_failures >= 2;

    if repeated_failure && r.strategy_change == StrategyChange::Keep {
        return Err(anyhow!(
            governor_contract::reflection_requires_strategy_change_message()
        ));
    }

    if matches!(r.goal_delta, GoalDelta::Same | GoalDelta::Farther)
        && matches!(
            r.strategy_change,
            StrategyChange::Keep | StrategyChange::Unknown
        )
    {
        return Err(anyhow!(
            governor_contract::reflection_non_improving_requires_change_message()
        ));
    }

    Ok(())
}

fn build_reflection_prompt(
    reason: &str,
    mem: &FailureMemory,
    state: AgentState,
    file_tool_consec_failures: usize,
) -> String {
    format!(
        "[Self Reflection Required]\n\
Reason: {reason}\n\
State: {:?}\n\
Failure memory:\n\
- consecutive_failures: {}\n\
- same_command_repeats: {}\n\
- same_error_repeats: {}\n\
- same_output_repeats: {}\n\
- file_tool_consec_failures: {}\n\
\n\
Before your next tool call, emit exactly ONE <reflect> block:\n\
<reflect>\n\
last_outcome: success|failure|partial\n\
goal_delta: closer|same|farther\n\
wrong_assumption: <one short sentence>\n\
strategy_change: keep|adjust|abandon\n\
next_minimal_action: <one short sentence>\n\
</reflect>\n\
\n\
Rules:\n\
- One line per field.\n\
- Keep the whole block under 80 tokens.\n\
- If the same error/command/output repeated, strategy_change cannot be `keep`.\n\
- If file_tool_consec_failures >= 2, strategy_change cannot be `keep`.\n\
- If goal_delta is `same` or `farther`, choose a materially different next action.\n\
- After the <reflect> block: emit your normal <think> block (required), then call exactly one tool. Do not add prose.",
        state,
        mem.consecutive_failures,
        mem.same_command_repeats,
        mem.same_error_repeats,
        mem.same_output_repeats,
        file_tool_consec_failures
    )
}

fn build_governor_state(
    state: AgentState,
    recovery: &RecoveryGovernor,
    mem: &FailureMemory,
    file_tool_consec_failures: usize,
    last_mutation_step: Option<usize>,
    last_verify_ok_step: Option<usize>,
    last_reflection: Option<&ReflectionBlock>,
) -> GovernorState {
    let done_verify_required = last_mutation_step.unwrap_or(0) > last_verify_ok_step.unwrap_or(0);
    let recovery_stage = if recovery.in_recovery() {
        Some(recovery.stage_label().to_string())
    } else {
        None
    };
    let last_reflection = last_reflection.map(|r| ReflectionSummary {
        last_outcome: if r.last_outcome.trim().is_empty() {
            None
        } else {
            Some(r.last_outcome.clone())
        },
        goal_delta: Some(r.goal_delta.as_str().to_string()),
        wrong_assumption: if r.wrong_assumption.trim().is_empty() {
            None
        } else {
            Some(r.wrong_assumption.clone())
        },
        strategy_change: Some(r.strategy_change.as_str().to_string()),
        next_minimal_action: if r.next_minimal_action.trim().is_empty() {
            None
        } else {
            Some(r.next_minimal_action.clone())
        },
    });

    GovernorState {
        state: format!("{state:?}").to_ascii_lowercase(),
        recovery_stage,

        consecutive_failures: mem.consecutive_failures,
        same_command_repeats: mem.same_command_repeats,
        same_error_repeats: mem.same_error_repeats,
        same_output_repeats: mem.same_output_repeats,
        file_tool_consec_failures,

        done_verify_required,
        last_mutation_step,
        last_verify_ok_step,

        last_reflection,
    }
}

fn last_tool_looks_failed(messages: &[serde_json::Value]) -> bool {
    let Some(last_tool) = messages
        .iter()
        .rev()
        .find(|m| m.get("role").and_then(|v| v.as_str()) == Some("tool"))
    else {
        return false;
    };
    let content = last_tool
        .get("content")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let low = content.to_ascii_lowercase();
    low.contains("failed (exit_code:")
        || low.contains("governor blocked")
        || low.contains("rejected by user")
        || low.contains("[result_file_err]")
}

fn is_diagnostic_tool_name(name: &str) -> bool {
    governor_contract::diagnostic_tool_names()
        .iter()
        .any(|tool| tool == name)
}

fn has_project_rules_context(messages: &[serde_json::Value]) -> bool {
    messages.iter().any(|msg| {
        if msg.get("role").and_then(|v| v.as_str()) != Some("system") {
            return false;
        }
        let Some(content) = msg.get("content").and_then(|v| v.as_str()) else {
            return false;
        };
        governor_contract::instruction_resolver_project_rule_markers()
            .iter()
            .any(|marker| !marker.is_empty() && content.contains(marker))
    })
}

fn is_diagnostic_command(command: &str) -> bool {
    exec_classification::is_diagnostic(command)
}

fn verification_examples(level: VerificationLevel) -> String {
    let verification = governor_contract::verification();
    let mut commands: Vec<String> = verification
        .goal_check_runners
        .iter()
        .filter_map(|runner| match level {
            VerificationLevel::Build => runner.build_command.as_ref(),
            VerificationLevel::Behavioral => runner.test_command.as_ref(),
        })
        .map(|command| command.trim().to_string())
        .filter(|command| !command.is_empty())
        .collect();

    let signatures = match level {
        VerificationLevel::Build => &verification.build_command_signatures,
        VerificationLevel::Behavioral => &verification.behavioral_command_signatures,
    };
    for command in signatures {
        let trimmed = command.trim();
        if trimmed.is_empty() || commands.iter().any(|existing| existing == trimmed) {
            continue;
        }
        commands.push(trimmed.to_string());
        if commands.len() >= 6 {
            break;
        }
    }

    let quoted = commands
        .into_iter()
        .take(6)
        .map(|command| format!("`{command}`"))
        .collect::<Vec<_>>();
    match quoted.as_slice() {
        [] => match level {
            VerificationLevel::Build => {
                "`cargo check`, `cargo build`, `tsc --noemit`, `ruff check`, or `git diff --check`"
                    .to_string()
            }
            VerificationLevel::Behavioral => {
                "`cargo test`, `cargo nextest`, `pytest`, `npm test`, `go test`, or `dotnet test`"
                    .to_string()
            }
        },
        [only] => only.clone(),
        [head @ .., tail] => format!("{} or {}", head.join(", "), tail),
    }
}

fn configured_test_cmd_verification_level(test_cmd: Option<&str>) -> Option<VerificationLevel> {
    exec_classification::configured_level(test_cmd)
}

fn verification_requirement_hint(level: VerificationLevel, test_cmd: Option<&str>) -> String {
    let preferred = test_cmd
        .and_then(|cmd| {
            let sig = command_sig(cmd);
            if sig.is_empty() {
                return None;
            }
            let cmd_level = configured_test_cmd_verification_level(Some(cmd))?;
            if cmd_level.satisfies(level) {
                Some(format!("Preferred: run `{sig}`."))
            } else {
                None
            }
        })
        .unwrap_or_else(|| format!("Examples: {}.", verification_examples(level)));
    format!(
        "run ONE real {} verification command. {preferred}",
        level.as_str()
    )
}

fn verification_requirement_note(
    level: VerificationLevel,
    test_cmd: Option<&str>,
    plan: Option<&PlanBlock>,
) -> String {
    let contrast = match level {
        VerificationLevel::Build => {
            "Behavioral tests are welcome, but a real build/check/lint is the minimum requirement."
        }
        VerificationLevel::Behavioral => {
            "Build-only checks do NOT satisfy this task; use tests or another behavioral verification."
        }
    };
    let mut out = format!(
        "[Verification Requirement]\nBefore `done`, this task requires {} verification.\n{}\n{}",
        level.as_str(),
        contrast,
        verification_requirement_hint(level, test_cmd)
    );
    if let Some(plan) = plan {
        if !plan.acceptance_criteria.is_empty() {
            out.push_str("\nAcceptance criteria:\n");
            for criterion in &plan.acceptance_criteria {
                out.push_str(&format!("- {criterion}\n"));
            }
        }
    }
    out
}

fn should_emit_verification_requirement_prompt(
    state: AgentState,
    recovery: &RecoveryGovernor,
    last_mutation_step: Option<usize>,
    last_verify_ok_step: Option<usize>,
    goal_checks: &GoalCheckTracker,
) -> bool {
    state == AgentState::Verifying
        || recovery.stage == Some(RecoveryStage::Verify)
        || last_mutation_step.unwrap_or(0) > last_verify_ok_step.unwrap_or(0)
        || goal_checks.any_attempted()
}

fn classify_verify_level(command: &str, test_cmd: Option<&str>) -> Option<VerificationLevel> {
    exec_classification::verify_level(command, test_cmd)
}

fn classify_exec_kind(command: &str, test_cmd: Option<&str>) -> ExecKind {
    exec_classification::classify(command, test_cmd)
}

pub(crate) fn exec_may_mutate_for_evaluation(command: &str, test_cmd: Option<&str>) -> bool {
    matches!(classify_exec_kind(command, test_cmd), ExecKind::Action)
}

fn normalize_for_signature_with_limit(s: &str, max_len: usize) -> String {
    // Keep this tiny: lowercased + digits collapsed removes most "At line:123" noise.
    let mut out = String::with_capacity(s.len().min(max_len));
    for ch in s.chars() {
        if ch.is_ascii_digit() {
            out.push('#');
        } else {
            out.push(ch.to_ascii_lowercase());
        }
        if out.len() >= max_len {
            break;
        }
    }
    out
}

fn normalize_for_signature(s: &str) -> String {
    normalize_for_signature_with_limit(s, 160)
}

fn text_contains_any(haystack: &str, terms: &[String]) -> bool {
    terms
        .iter()
        .any(|term| !term.is_empty() && haystack.contains(term))
}

fn goal_check_max_attempts() -> usize {
    let max_attempts = governor_contract::verification()
        .goal_check_policy
        .max_attempts_per_goal;
    max_attempts.max(1)
}

fn goal_check_order() -> Vec<String> {
    let order = governor_contract::verification()
        .goal_check_policy
        .goal_order
        .iter()
        .map(|item| item.trim())
        .filter(|item| !item.is_empty())
        .map(|item| item.to_string())
        .collect::<Vec<_>>();
    if order.is_empty() {
        vec!["repo".to_string(), "tests".to_string(), "build".to_string()]
    } else {
        order
    }
}

fn should_auto_run_goal_checks(command_approval_required: bool, max_iters: usize) -> bool {
    let policy = &governor_contract::verification().goal_check_policy;
    if !policy.run_on_stop {
        return false;
    }
    if policy.require_exec_feature {
        // TUI agentic mode always provides exec.
    }
    if policy.require_longrun && max_iters <= 1 {
        return false;
    }
    if policy.require_command_approval_off && command_approval_required {
        return false;
    }
    true
}

fn command_sig(command: &str) -> String {
    command_sig_with_limit(command, 160)
}

fn command_sig_full(command: &str) -> String {
    command_sig_with_limit(command, 4096)
}

fn command_sig_with_limit(command: &str, max_len: usize) -> String {
    // Single line, trimmed, collapsed whitespace.
    let normalized = command.replace("\r\n", "\n");
    let one = normalized
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .trim();
    let collapsed = one.split_whitespace().collect::<Vec<_>>().join(" ");
    normalize_for_signature_with_limit(&collapsed, max_len)
}

fn tool_call_action_sig(tc: &ToolCallData) -> Option<String> {
    match tc.name.as_str() {
        "exec" => {
            let args: serde_json::Value =
                serde_json::from_str(&tc.arguments).unwrap_or(json!({"command": tc.arguments}));
            let command = args["command"].as_str().unwrap_or("").trim();
            if command.is_empty() {
                None
            } else {
                Some(format!("exec:{}", command_sig(command)))
            }
        }
        "read_file" | "write_file" | "patch_file" | "apply_diff" => {
            let args: serde_json::Value = serde_json::from_str(&tc.arguments).unwrap_or(json!({}));
            let path = args["path"].as_str().unwrap_or("").trim();
            if path.is_empty() {
                None
            } else {
                Some(format!("{}:{}", tc.name, normalize_for_signature(path)))
            }
        }
        _ => None,
    }
}

fn blocked_tool_call_signature(name: &str, arguments: &str) -> String {
    canonicalize_tool_call_command(name, arguments)
        .unwrap_or_else(|| format!("{name}:{}", normalize_for_signature(arguments)))
}

fn assistant_blocked_tool_call_signature(msg: &serde_json::Value) -> Option<String> {
    let tool_calls = msg.get("tool_calls")?.as_array()?;
    let tc = tool_calls.first()?;
    let name = tc
        .get("function")
        .and_then(|f| f.get("name"))
        .and_then(|v| v.as_str())?
        .trim();
    let arguments = tc
        .get("function")
        .and_then(|f| f.get("arguments"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    if name.is_empty() {
        None
    } else {
        Some(blocked_tool_call_signature(name, arguments))
    }
}

fn assistant_blocked_tool_name(msg: &serde_json::Value) -> Option<String> {
    let tool_calls = msg.get("tool_calls")?.as_array()?;
    let tc = tool_calls.first()?;
    tc.get("function")
        .and_then(|f| f.get("name"))
        .and_then(|v| v.as_str())
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
}

fn is_missing_plan_governor_block(msg: &serde_json::Value) -> bool {
    msg.get("role").and_then(|v| v.as_str()) == Some("tool")
        && msg
            .get("content")
            .and_then(|v| v.as_str())
            .map(|content| {
                content.contains("GOVERNOR BLOCKED") && content.contains("Missing valid <plan>")
            })
            .unwrap_or(false)
}

fn is_missing_think_governor_block(msg: &serde_json::Value) -> bool {
    msg.get("role").and_then(|v| v.as_str()) == Some("tool")
        && msg
            .get("content")
            .and_then(|v| v.as_str())
            .map(|content| {
                content.contains("GOVERNOR BLOCKED") && content.contains("Missing <think>")
            })
            .unwrap_or(false)
}

fn is_missing_gate_governor_block(msg: &serde_json::Value) -> bool {
    is_missing_plan_governor_block(msg) || is_missing_think_governor_block(msg)
}

fn consecutive_missing_plan_blocks_for_tool(
    messages: &[serde_json::Value],
    tc: &ToolCallData,
) -> usize {
    let target_sig = blocked_tool_call_signature(&tc.name, &tc.arguments);
    let mut count = 0usize;
    let mut idx = messages.len();
    while idx >= 2 {
        let tool_msg = &messages[idx - 1];
        let assistant_msg = &messages[idx - 2];
        if !is_missing_plan_governor_block(tool_msg) {
            break;
        }
        let Some(sig) = assistant_blocked_tool_call_signature(assistant_msg) else {
            break;
        };
        if sig != target_sig {
            break;
        }
        count = count.saturating_add(1);
        idx -= 2;
    }
    count
}

fn consecutive_missing_plan_blocks_for_observation(messages: &[serde_json::Value]) -> usize {
    let mut count = 0usize;
    let mut idx = messages.len();
    while idx >= 2 {
        let tool_msg = &messages[idx - 1];
        let assistant_msg = &messages[idx - 2];
        if !is_missing_plan_governor_block(tool_msg) {
            break;
        }
        let Some(name) = assistant_blocked_tool_name(assistant_msg) else {
            break;
        };
        if !mistral_observation_tool(name.as_str()) {
            break;
        }
        count = count.saturating_add(1);
        idx -= 2;
    }
    count
}

fn consecutive_missing_gate_blocks_for_observation(messages: &[serde_json::Value]) -> usize {
    let mut count = 0usize;
    let mut idx = messages.len();
    while idx >= 2 {
        let tool_msg = &messages[idx - 1];
        let assistant_msg = &messages[idx - 2];
        if !is_missing_gate_governor_block(tool_msg) {
            break;
        }
        let Some(name) = assistant_blocked_tool_name(assistant_msg) else {
            break;
        };
        if !mistral_observation_tool(name.as_str()) {
            break;
        }
        count = count.saturating_add(1);
        idx -= 2;
    }
    count
}

fn consecutive_missing_think_blocks_for_tool(
    messages: &[serde_json::Value],
    tc: &ToolCallData,
) -> usize {
    let target_sig = blocked_tool_call_signature(&tc.name, &tc.arguments);
    let mut count = 0usize;
    let mut idx = messages.len();
    while idx >= 2 {
        let tool_msg = &messages[idx - 1];
        let assistant_msg = &messages[idx - 2];
        if !is_missing_think_governor_block(tool_msg) {
            break;
        }
        let Some(sig) = assistant_blocked_tool_call_signature(assistant_msg) else {
            break;
        };
        if sig != target_sig {
            break;
        }
        count = count.saturating_add(1);
        idx -= 2;
    }
    count
}

fn consecutive_missing_think_blocks_for_observation(messages: &[serde_json::Value]) -> usize {
    let mut count = 0usize;
    let mut idx = messages.len();
    while idx >= 2 {
        let tool_msg = &messages[idx - 1];
        let assistant_msg = &messages[idx - 2];
        if !is_missing_think_governor_block(tool_msg) {
            break;
        }
        let Some(name) = assistant_blocked_tool_name(assistant_msg) else {
            break;
        };
        if !mistral_observation_tool(name.as_str()) {
            break;
        }
        count = count.saturating_add(1);
        idx -= 2;
    }
    count
}

fn pick_interesting_error_line(stdout: &str, stderr: &str) -> String {
    interesting_failure_line(stdout, stderr)
}

fn error_signature(command: &str, stdout: &str, stderr: &str, exit_code: i32) -> String {
    let cmd = command_sig(command);
    let line = pick_interesting_error_line(stdout, stderr);
    format!("exit={exit_code}|cmd={cmd}|err={line}")
}

fn hash_output(stdout: &str, stderr: &str) -> u64 {
    let mut h = DefaultHasher::new();
    stdout.trim_end().hash(&mut h);
    stderr.trim_end().hash(&mut h);
    h.finish()
}

fn hash_text(s: &str) -> u64 {
    let mut h = DefaultHasher::new();
    s.trim_end().hash(&mut h);
    h.finish()
}

fn fmt_hash(h: u64) -> String {
    format!("{h:016x}")
}

fn suspicious_success_reason(stdout: &str, stderr: &str) -> Option<String> {
    // PowerShell often exits 0 even when it printed errors (non-terminating error records).
    // Cargo warnings also go to stderr, so we only trigger on strong error markers.
    let bad = [
        "parsererror",
        "unexpected token",
        "missing expression",
        "commandnotfoundexception",
        "not recognized",
        "error:",
        "fatal:",
        "exception",
        "traceback",
        "access is denied",
        "permission denied",
        "does not have a commit checked out",
        "unable to index file",
        "could not find a part of the path",
    ];

    for src in [stderr, stdout] {
        let low = src.to_ascii_lowercase();
        if bad.iter().any(|k| low.contains(k)) {
            // Return just enough to explain why we treated this as failure.
            let line = pick_interesting_error_line(stdout, stderr);
            if !line.is_empty() {
                return Some(format!(
                    "exit_code was 0, but output contained error markers (e.g. `{line}`)"
                ));
            }
            return Some("exit_code was 0, but output contained error markers".to_string());
        }
    }

    None
}

fn hint_for_known_failure(command: &str, stdout: &str, stderr: &str) -> Option<String> {
    let mut s = String::new();
    s.push_str(stdout);
    s.push('\n');
    s.push_str(stderr);
    let low = s.to_ascii_lowercase();
    let cmd_low = command.to_ascii_lowercase();

    if low.contains("the term '$' is not recognized")
        || (low.contains("the term '$'") && low.contains("not recognized"))
    {
        return Some(
            "Your command includes a transcript prompt marker like `$` / `PS>`.\n\
Fix: send ONLY the command (e.g. `git status`), not `$ git status`."
                .to_string(),
        );
    }
    if low.contains("unexpected token '}'")
        || (low.contains("unexpected token") && low.contains('}'))
    {
        return Some(
            "PowerShell saw a stray `}` in the command.\n\
Fix: remove the trailing `}` and retry."
                .to_string(),
        );
    }
    if low.contains("adding embedded git repository")
        || low.contains("does not have a commit checked out")
    {
        return Some(
            "You are trying to `git add` a nested repo directory.\n\
Fix: `cd` into the intended repo before `git add .`, or add the nested repo dir to `.gitignore` (or use `git submodule add ...`)."
                .to_string(),
        );
    }
    if low.contains("failed to remove file")
        && (low.contains("access is denied") || low.contains("permission denied"))
        && cmd_low.contains("cargo")
    {
        return Some(
            "Rust build failed because `spiral-coder.exe` is locked.\n\
Fix: stop the running process (or close the TUI/serve), then rebuild.\n\
Tip (Windows): use `scripts/run-tui.ps1` / `scripts/run-ui.ps1` which build in an isolated CARGO_TARGET_DIR and auto-kill old processes."
                .to_string(),
        );
    }
    if low.contains("could not resolve host")
        || low.contains("couldn't resolve host")
        || low.contains("temporary failure in name resolution")
        || low.contains("name resolution failed")
    {
        if cmd_low.contains("cargo") || low.contains("crates.io") || low.contains("index.crates.io")
        {
            return Some(
                "Network/DNS failure: cannot reach crates.io/index.\n\
Action: stop retrying; this is not a code bug.\n\
- If you are offline/sandboxed, you cannot download new crates here.\n\
- Try `cargo ... --offline` only if deps are already cached; otherwise proceed with static edits and defer builds/tests."
                    .to_string(),
            );
        }
        return Some(
            "Network/DNS failure: host name resolution failed.\n\
Action: stop retrying; check connectivity and clear proxy env vars (HTTP_PROXY/HTTPS_PROXY/ALL_PROXY)."
                .to_string(),
        );
    }
    if low.contains("could not connect to server")
        && low.contains("127.0.0.1")
        && cmd_low.contains("git")
    {
        return Some(
            "Git network failed via a dead local proxy (127.0.0.1).\n\
Fix: clear proxy env vars: `Remove-Item Env:HTTP_PROXY,Env:HTTPS_PROXY,Env:ALL_PROXY -ErrorAction SilentlyContinue`."
                .to_string(),
        );
    }
    if low.contains("incorrect api key")
        || low.contains("invalid_api_key")
        || (low.contains("http 401") && low.contains("api key"))
    {
        return Some(
            "Provider returned HTTP 401 (bad/missing API key).\n\
Fix: update the configured API key for the selected provider/model, then retry."
                .to_string(),
        );
    }

    None
}

fn is_git_repo_root(dir: &str) -> bool {
    let p = Path::new(dir);
    let dot_git = p.join(".git");
    dot_git.is_dir() || dot_git.is_file()
}

fn gitmodules_lists_path(repo_root: &str, rel_path: &str) -> bool {
    let p = Path::new(repo_root).join(".gitmodules");
    let Ok(text) = std::fs::read_to_string(&p) else {
        return false;
    };
    let needle = format!("path = {rel_path}");
    text.lines().any(|l| l.trim() == needle)
}

fn nested_git_dirs_shallow(repo_root: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let root = Path::new(repo_root);
    let Ok(rd) = std::fs::read_dir(root) else {
        return out;
    };
    for ent in rd.flatten() {
        let path = ent.path();
        if !path.is_dir() {
            continue;
        }
        let dot_git = path.join(".git");
        if !(dot_git.is_dir() || dot_git.is_file()) {
            continue;
        }
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        // Ignore the repo root itself (we only scan immediate children anyway).
        if name == ".git" {
            continue;
        }
        out.push(name.to_string());
    }
    out.sort();
    out.dedup();
    out
}

fn should_block_git_landmines(command: &str, tool_root_abs: Option<&str>) -> Option<String> {
    let root = tool_root_abs?;
    if !is_git_repo_root(root) {
        return None;
    }

    let cmd_low = command.to_ascii_lowercase();

    // 1) Never create a new git repo inside an existing repo; this causes embedded repos and breaks `git add`.
    if cmd_low.contains("git init") {
        return Some(format!(
            "Refusing to run `git init` inside an existing git repo (tool_root has .git): {root}\n\
This is a common agent failure mode that creates embedded repos and breaks `git add`.\n\
Fix: set tool_root to a fresh directory outside this repo (e.g. `.tmp/newrepo`), or remove `git init` and just create files."
        ));
    }

    // 2) Block `git add` when we detect nested git directories under tool_root (unless they are declared submodules).
    if cmd_low.contains("git add") {
        let nested = nested_git_dirs_shallow(root);
        let mut offenders: Vec<String> = Vec::new();
        for d in nested {
            if !gitmodules_lists_path(root, &d) {
                offenders.push(d);
            }
        }
        if !offenders.is_empty() {
            let list = offenders.join(", ");
            return Some(format!(
                "Nested git repo(s) detected under tool_root: {list}\n\
Running `git add` will trigger embedded-repo errors or index failures.\n\
Fix: move those directories outside tool_root, add them to `.gitignore`, or add them properly as submodules (`git submodule add <url> <path>`)."
            ));
        }
    }

    None
}

fn wants_local_actions(user_text: &str) -> bool {
    let s = user_text.to_ascii_lowercase();
    let kws = [
        "repo",
        "repository",
        "scaffold",
        "bootstrap",
        "init",
        "setup",
        "create",
        "generate",
        "implement",
        "build",
        "test",
        "run",
        "install",
        "folder",
        "directory",
        "file",
        "git",
        "commit",
        "push",
        // Japanese
        "リポ",
        "リポジトリ",
        "フォルダ",
        "ディレクトリ",
        "ファイル",
        "作成",
        "作る",
        "実装",
        "実行",
        "コミット",
        "プッシュ",
        // French
        "dépôt",
        "depot",
        "répertoire",
        "repertoire",
        "fichier",
        "commande",
        "créer",
        "creer",
        "générer",
        "generer",
        "exécuter",
        "executer",
        "installer",
    ];
    kws.iter().any(|k| s.contains(k))
}

#[derive(Debug, Clone)]
struct ImpliedExecScript {
    lang_hint: String,
    script: String,
}

#[derive(Debug, Clone)]
struct ImpliedWriteFile {
    path: String,
    lang_hint: String,
    content: String,
}

fn is_shell_fence_lang(lang_hint: &str) -> bool {
    matches!(
        lang_hint,
        "bash" | "sh" | "shell" | "zsh" | "powershell" | "pwsh" | "ps1" | "ps" | "console"
    )
}

fn looks_like_shell_command(script: &str) -> bool {
    let low = script.to_ascii_lowercase();
    let pats = [
        // PowerShell
        "new-item",
        "set-content",
        "add-content",
        "remove-item",
        "copy-item",
        "move-item",
        "get-content",
        "test-path",
        // Common CLIs
        "\ngit ",
        "git ",
        "\ncargo ",
        "cargo ",
        "\npython",
        "python ",
        "\nnode ",
        "node ",
        "\nnpm ",
        "npm ",
        "\npnpm ",
        "pnpm ",
        "\nyarn ",
        "yarn ",
        // Shell-ish
        "\ncd ",
        "\nmkdir",
        "mkdir ",
    ];
    pats.iter().any(|p| low.contains(p))
}

fn extract_implied_exec_scripts(text: &str) -> Vec<ImpliedExecScript> {
    let raw = text.replace("\r\n", "\n");
    let mut out: Vec<ImpliedExecScript> = Vec::new();

    let mut in_fence = false;
    let mut fence_lang = String::new();
    let mut buf: Vec<String> = Vec::new();

    for line0 in raw.lines() {
        let line = line0.trim_end_matches('\r');
        let t = line.trim_start();
        if !in_fence {
            if let Some(rest) = t.strip_prefix("```") {
                in_fence = true;
                fence_lang = rest.trim().to_ascii_lowercase();
                buf.clear();
            }
            continue;
        }

        // in_fence
        if t.starts_with("```") {
            let body = buf.join("\n");
            let script = body.trim().to_string();
            if !script.is_empty() {
                let lang = fence_lang.trim().to_string();
                if is_shell_fence_lang(lang.as_str()) || looks_like_shell_command(&script) {
                    out.push(ImpliedExecScript {
                        lang_hint: lang,
                        script,
                    });
                    if out.len() >= 3 {
                        return out;
                    }
                }
            }
            in_fence = false;
            fence_lang.clear();
            buf.clear();
            continue;
        }

        buf.push(line.to_string());
    }

    // Fallback: some models omit code fences but still paste PS prompt lines.
    if out.is_empty() {
        let mut lines: Vec<String> = Vec::new();
        for line0 in raw.lines() {
            let t = line0.trim_start();
            if t.starts_with("PS>") || t.starts_with("$ ") || t.starts_with("> ") {
                lines.push(t.to_string());
                if lines.len() >= 24 {
                    break;
                }
            }
        }
        let joined = lines.join("\n").trim().to_string();
        if !joined.is_empty() && looks_like_shell_command(&joined) {
            out.push(ImpliedExecScript {
                lang_hint: "powershell".to_string(),
                script: joined,
            });
        }
    }

    out
}

fn sanitize_implied_path_candidate(raw: &str) -> Option<String> {
    let mut p = raw.trim().to_string();
    if p.is_empty() {
        return None;
    }

    // Strip common wrappers.
    if (p.starts_with('"') && p.ends_with('"')) || (p.starts_with('\'') && p.ends_with('\'')) {
        p = p[1..p.len().saturating_sub(1)].to_string();
    }
    if p.starts_with('`') && p.ends_with('`') && p.len() >= 2 {
        p = p[1..p.len().saturating_sub(1)].to_string();
    }
    p = p.trim().to_string();
    if p.is_empty() {
        return None;
    }
    if let Some(rest) = p.strip_prefix("./") {
        p = rest.to_string();
    }
    if p.ends_with(':') {
        p.pop();
        p = p.trim().to_string();
    }

    // Avoid obvious false positives and dangerous strings.
    if p.len() > 240 {
        return None;
    }
    let low = p.to_ascii_lowercase();
    if low.starts_with("http://") || low.starts_with("https://") {
        return None;
    }
    if p.contains("```") || p.contains("<plan>") || p.contains("</plan>") {
        return None;
    }
    // Avoid Windows-forbidden characters and other obvious junk.
    if p.chars()
        .any(|c| matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*'))
    {
        return None;
    }
    // Keep it simple: paths with spaces are too ambiguous in free-form text.
    if p.chars().any(|c| c.is_whitespace()) {
        return None;
    }

    // Must look like a file path, not a version number / heading.
    let has_sep = p.contains('/') || p.contains('\\');
    let has_dot = p.contains('.');
    if !has_sep && !has_dot {
        // Common "no extension" files.
        let low2 = p.to_ascii_lowercase();
        let ok = matches!(
            low2.as_str(),
            "license" | "makefile" | "dockerfile" | "readme"
        );
        if !ok {
            return None;
        }
    } else if has_dot && !has_sep {
        // Reject things like "v0.2.0" (last segment has no alphabetic chars).
        if let Some((_, ext)) = p.rsplit_once('.') {
            let ext = ext.trim();
            if ext.is_empty() {
                return None;
            }
            let has_alpha = ext.chars().any(|c| c.is_ascii_alphabetic());
            if !has_alpha {
                return None;
            }
        }
    }
    Some(p)
}

fn parse_implied_path_line(line: &str) -> Option<String> {
    let s = line.trim();
    if s.is_empty() {
        return None;
    }

    // Examples:
    // - File: src/main.rs
    // - Path: README.md
    // - ### src/main.rs
    // - src/main.rs
    let s = s.strip_prefix("- ").unwrap_or(s);
    let s = s.strip_prefix("* ").unwrap_or(s);

    // "File: ..."
    let low = s.to_ascii_lowercase();
    for key in ["file:", "path:", "filepath:", "filename:"] {
        if let Some(rest) = low.strip_prefix(key) {
            // Use the original string slice to preserve case.
            let orig_rest = s[s.len().saturating_sub(rest.len())..].trim();
            return sanitize_implied_path_candidate(orig_rest);
        }
    }

    // Markdown headers: ### path
    if s.starts_with('#') {
        let rest = s.trim_start_matches('#').trim();
        return sanitize_implied_path_candidate(rest);
    }

    None
}

fn extract_implied_write_files(text: &str) -> Vec<ImpliedWriteFile> {
    let raw = text.replace("\r\n", "\n");
    let mut out: Vec<ImpliedWriteFile> = Vec::new();

    let mut pending_path: Option<String> = None;
    let mut in_fence = false;
    let mut fence_lang = String::new();
    let mut buf: Vec<String> = Vec::new();

    for line0 in raw.lines() {
        let line = line0.trim_end_matches('\r');
        let t = line.trim();

        if !in_fence {
            if let Some(rest) = t.strip_prefix("```") {
                in_fence = true;
                fence_lang = rest.trim().to_ascii_lowercase();
                buf.clear();
                continue;
            }
            if let Some(p) = parse_implied_path_line(t) {
                pending_path = Some(p);
            }
            continue;
        }

        // in_fence
        if t.starts_with("```") {
            let body = buf.join("\n");
            let content = body.trim_end().to_string();
            if !content.trim().is_empty() {
                if let Some(path) = pending_path.take() {
                    let lang = fence_lang.trim().to_string();
                    if !matches!(lang.as_str(), "diff" | "patch") {
                        out.push(ImpliedWriteFile {
                            path,
                            lang_hint: lang,
                            content,
                        });
                        if out.len() >= 6 {
                            return out;
                        }
                    }
                }
            }
            in_fence = false;
            fence_lang.clear();
            buf.clear();
            continue;
        }

        buf.push(line.to_string());
    }

    out
}

impl FailureMemory {
    fn from_recent_messages(messages: &[serde_json::Value]) -> Self {
        let mut mem = FailureMemory::default();

        // Map tool_call_id -> command for exec calls.
        let mut exec_by_id: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();

        for msg in messages {
            let role = msg.get("role").and_then(|v| v.as_str()).unwrap_or("");

            if role == "assistant" {
                let Some(tcs) = msg.get("tool_calls").and_then(|v| v.as_array()) else {
                    continue;
                };
                for tc in tcs {
                    let id = tc.get("id").and_then(|v| v.as_str()).unwrap_or("").trim();
                    if id.is_empty() {
                        continue;
                    }
                    let name = tc
                        .get("function")
                        .and_then(|f| f.get("name"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .trim();
                    if name != "exec" {
                        continue;
                    }
                    let args = tc
                        .get("function")
                        .and_then(|f| f.get("arguments"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .trim();
                    if let Some(cmd) = parse_exec_command_from_args(args) {
                        exec_by_id.insert(id.to_string(), cmd);
                    }
                }
                continue;
            }

            if role == "tool" {
                let tcid = msg
                    .get("tool_call_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim();
                if tcid.is_empty() {
                    continue;
                }
                let Some(command) = exec_by_id.remove(tcid) else {
                    continue;
                };
                let content = msg
                    .get("content")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let (exit_code, stdout, stderr) = parse_exec_tool_output_sections(&content);
                let Some(mut effective_exit_code) = exit_code else {
                    continue;
                };
                if effective_exit_code == 0
                    && suspicious_success_reason(stdout.as_str(), stderr.as_str()).is_some()
                {
                    effective_exit_code = 1;
                }
                let _ = mem.on_tool_result(
                    command.as_str(),
                    stdout.as_str(),
                    stderr.as_str(),
                    effective_exit_code,
                );
            }
        }

        mem
    }

    fn on_tool_result(
        &mut self,
        command: &str,
        stdout: &str,
        stderr: &str,
        effective_exit_code: i32,
    ) -> Option<String> {
        // Track repeated identical commands (common loop symptom).
        let cmd_sig = command_sig(command);
        if self.last_command_sig.as_deref() == Some(&cmd_sig) {
            self.same_command_repeats = self.same_command_repeats.saturating_add(1);
        } else {
            self.last_command_sig = Some(cmd_sig);
            self.same_command_repeats = 1;
        }

        // Track output hash (stuck detection).
        let oh = hash_output(stdout, stderr);
        if self.last_output_hash == Some(oh) {
            self.same_output_repeats = self.same_output_repeats.saturating_add(1);
        } else {
            self.last_output_hash = Some(oh);
            self.same_output_repeats = 1;
        }

        if effective_exit_code == 0 {
            self.consecutive_failures = 0;
            self.last_error_sig = None;
            self.same_error_repeats = 0;
            self.last_error_class = ErrorClass::Unknown;
            return None;
        }

        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
        self.last_error_class = classify_error(stderr, stdout);

        let sig = error_signature(command, stdout, stderr, effective_exit_code);
        if self.last_error_sig.as_deref() == Some(&sig) {
            self.same_error_repeats = self.same_error_repeats.saturating_add(1);
        } else {
            self.last_error_sig = Some(sig);
            self.same_error_repeats = 1;
        }

        // Emit hints only when crossing key thresholds to avoid spamming context.
        if self.same_error_repeats == 2 {
            if let Some(h) = hint_for_known_failure(command, stdout, stderr) {
                return Some(h);
            }
            return Some(
                "The SAME error happened twice.\n\
Action: stop repeating; gather diagnostics (`pwd`, `ls`, `git status`) then change strategy."
                    .to_string(),
            );
        }

        if self.same_command_repeats == 3 {
            return Some(
                "You ran the SAME command 3 times.\n\
Action: abandon this approach and try a different strategy (different cwd, different command, or add diagnostics)."
                    .to_string(),
            );
        }

        if self.consecutive_failures >= 3 {
            let class_ctx = error_class_hint(&self.last_error_class);
            let context = if class_ctx.is_empty() {
                String::new()
            } else {
                format!("\nLast error type: {class_ctx}")
            };
            return Some(format!(
                "3 consecutive failures.{context}\n\
Action: change strategy now; do NOT retry the same approach again."
            ));
        }

        if self.same_output_repeats >= 2 && self.same_command_repeats >= 2 {
            return Some(
                "Stuck detected: repeated identical output.\n\
Action: print diagnostics and change strategy; do not repeat the same command."
                    .to_string(),
            );
        }

        None
    }
}

fn parse_exec_command_from_args(args: &str) -> Option<String> {
    // Standard tool schema uses JSON arguments: {"command":"...","cwd":"..."}.
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(args) {
        if let Some(cmd) = v.get("command").and_then(|x| x.as_str()) {
            let t = cmd.trim();
            if !t.is_empty() {
                return Some(t.to_string());
            }
        }
    }

    // Fallback: some providers/models might pass a raw string.
    let t = args.trim();
    if !t.is_empty() {
        return Some(t.to_string());
    }
    None
}

fn parse_exit_code_from_tool_text(s: &str) -> Option<i32> {
    let low = s.to_ascii_lowercase();
    let key = "exit_code:";
    let idx = low.find(key)?;
    let after = low[idx + key.len()..].trim_start();
    let mut num = String::new();
    for ch in after.chars() {
        if ch == '-' || ch.is_ascii_digit() {
            num.push(ch);
        } else {
            break;
        }
    }
    if num.is_empty() {
        return None;
    }
    num.parse::<i32>().ok()
}

fn parse_exec_tool_output_sections(tool_content: &str) -> (Option<i32>, String, String) {
    let t = tool_content.replace("\r\n", "\n");
    let exit_code = parse_exit_code_from_tool_text(t.as_str());

    // Prefer parsing known markers so the governor sees the "real" stdout/stderr.
    // - OK: "OK (exit_code: 0)\nstdout:\n..."
    // - FAILED: "...FAILED (exit_code: X)\n...stdout (tail):\n...\nstderr:\n..."
    let mut stdout = String::new();
    let mut stderr = String::new();

    let (before_stderr, stderr_part) = match t.split_once("\nstderr:\n") {
        Some((a, b)) => (a, Some(b)),
        None => (t.as_str(), None),
    };
    if let Some(b) = stderr_part {
        stderr = b.trim_end().to_string();
    }

    // stdout markers can exist with or without stderr.
    if let Some((_, s_out)) = before_stderr.rsplit_once("\nstdout (tail):\n") {
        stdout = s_out.trim_end().to_string();
    } else if let Some((_, s_out)) = before_stderr.rsplit_once("\nstdout:\n") {
        stdout = s_out.trim_end().to_string();
    }

    // If we can't parse anything meaningful, treat the whole tool output as stderr on failures.
    if stdout.is_empty() && stderr.is_empty() {
        if exit_code.unwrap_or(0) != 0 {
            stderr = t.trim_end().to_string();
        }
    }

    (exit_code, stdout, stderr)
}

// ── Git helpers ───────────────────────────────────────────────────────────────

/// Create a git checkpoint commit in `root` (if it is a git repo).
/// Returns the HEAD hash after the commit, or None if not a git repo / git unavailable.
async fn git_create_checkpoint(root: &str) -> Option<String> {
    // Only proceed if `root` itself is the git top-level. `git -C subdir`
    // happily walks up to a parent repo; for copied eval sandboxes under
    // `.tmp/`, that would create checkpoint commits in the real workspace.
    let git_top = run_git_cmd(root, &["rev-parse", "--show-toplevel"]).await?;
    if !git_checkpoint_root_matches_tool_root(root, git_top.as_str()) {
        return None;
    }

    // Only proceed if this is a git repo.
    let head = run_git_cmd(root, &["rev-parse", "HEAD"]).await;
    if head.is_none() {
        return None;
    }

    // Stage all current changes (untracked included).
    let _ = run_git_cmd(root, &["add", "-A"]).await;

    // Commit with --allow-empty so we always get a clean ref even if nothing changed.
    let epoch = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let msg = format!("spiral-coder: pre-session checkpoint {epoch}");
    let _ = run_git_cmd(root, &["commit", "--allow-empty", "-m", &msg]).await;

    // Return the new HEAD hash.
    run_git_cmd(root, &["rev-parse", "HEAD"]).await
}

fn git_checkpoint_root_matches_tool_root(root: &str, git_top_level: &str) -> bool {
    let root_path = match std::fs::canonicalize(root) {
        Ok(path) => path,
        Err(_) => return false,
    };
    let top_path = match std::fs::canonicalize(git_top_level) {
        Ok(path) => path,
        Err(_) => return false,
    };
    normalize_checkpoint_path(&root_path) == normalize_checkpoint_path(&top_path)
}

fn normalize_checkpoint_path(path: &Path) -> String {
    let mut out = path.to_string_lossy().replace('\\', "/");
    while out.ends_with('/') && out.len() > 1 {
        out.pop();
    }
    if cfg!(target_os = "windows") {
        out = out.to_ascii_lowercase();
    }
    out
}

/// Run `git -C root <args>` with a 5-second timeout. Returns trimmed stdout or None.
async fn run_git_cmd(root: &str, args: &[&str]) -> Option<String> {
    let fut = tokio::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output();
    let out = tokio::time::timeout(std::time::Duration::from_secs(5), fut)
        .await
        .ok()?
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Run the project's test command after a file edit. Returns a formatted result string.
/// Capped at 120 seconds; stdout/stderr truncated to MAX_STDOUT_CHARS.
async fn run_test_cmd(cmd: &str, cwd: &str) -> String {
    let fut = tokio::process::Command::new(if cfg!(target_os = "windows") {
        "powershell"
    } else {
        "sh"
    })
    .args(if cfg!(target_os = "windows") {
        vec!["-Command", cmd]
    } else {
        vec!["-c", cmd]
    })
    .current_dir(cwd)
    .stdout(std::process::Stdio::piped())
    .stderr(std::process::Stdio::piped())
    .output();

    let result = match tokio::time::timeout(std::time::Duration::from_secs(120), fut).await {
        Ok(Ok(out)) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let stderr = String::from_utf8_lossy(&out.stderr);
            let combined = format!("{}{}", stdout, stderr);
            let exit = out.status.code().unwrap_or(-1);
            (combined, exit)
        }
        Ok(Err(e)) => (format!("error running test: {e}"), -1),
        Err(_) => ("test timed out after 120s".to_string(), -1),
    };

    let (combined, exit) = result;
    let truncated = truncate_output_tail(&combined, 1200);

    if exit == 0 {
        format!("\n\n[auto-test] ✓ PASSED (exit 0)\n{truncated}")
    } else {
        format!("\n\n[auto-test] ✗ FAILED (exit {exit})\n{truncated}\nFix the test failure before proceeding.")
    }
}

fn goal_check_support_line(summary: &str, fallback: &str) -> String {
    if summary.trim().is_empty() {
        fallback.to_string()
    } else {
        governor_contract::goal_check_supported_runners_message(summary)
    }
}

fn error_class_name(class: &ErrorClass) -> &'static str {
    match class {
        ErrorClass::Environment => "environment",
        ErrorClass::Syntax => "syntax",
        ErrorClass::Path => "path",
        ErrorClass::Dependency => "dependency",
        ErrorClass::Network => "network",
        ErrorClass::Logic => "logic",
        ErrorClass::Unknown => "unknown",
    }
}

fn goal_check_class_line(class: &ErrorClass) -> String {
    match class {
        ErrorClass::Unknown => String::new(),
        _ => format!("class: {}\n", error_class_name(class)),
    }
}

fn goal_check_digest_line(digest: &str) -> String {
    let trimmed = digest.trim();
    if trimmed.is_empty() {
        String::new()
    } else {
        format!("{trimmed}\n")
    }
}

fn goal_check_runner_command(
    level: VerificationLevel,
    cwd: &str,
    configured_test_cmd: Option<&str>,
) -> Option<String> {
    if matches!(level, VerificationLevel::Behavioral) {
        if let Some(command) = configured_test_cmd
            .map(str::trim)
            .filter(|command| !command.is_empty())
        {
            return Some(command.to_string());
        }
    }
    governor_contract::verification()
        .goal_check_runners
        .iter()
        .find(|runner| {
            runner
                .detect_files_any
                .iter()
                .filter(|path| !path.trim().is_empty())
                .any(|path| Path::new(cwd).join(path).exists())
        })
        .and_then(|runner| match level {
            VerificationLevel::Build => runner.build_command.clone(),
            VerificationLevel::Behavioral => runner.test_command.clone(),
        })
        .map(|command| command.trim().to_string())
        .filter(|command| !command.is_empty())
}

fn goal_check_runner_summary(level: VerificationLevel) -> String {
    governor_contract::verification()
        .goal_check_runners
        .iter()
        .filter_map(|runner| {
            let command = match level {
                VerificationLevel::Build => runner.build_command.as_deref(),
                VerificationLevel::Behavioral => runner.test_command.as_deref(),
            }?
            .trim();
            if command.is_empty() {
                return None;
            }
            let files = runner
                .detect_files_any
                .iter()
                .map(|path| path.trim())
                .filter(|path| !path.is_empty())
                .collect::<Vec<_>>();
            if files.is_empty() {
                return None;
            }
            Some(format!("{} -> {command}", files.join("/")))
        })
        .collect::<Vec<_>>()
        .join(", ")
}

async fn repo_goal_missing_labels(cwd: &str) -> Vec<String> {
    let mut missing = Vec::new();
    for requirement in &governor_contract::verification().repo_goal_requirements {
        let key = requirement.key.trim();
        let label = requirement.label.trim();
        let present = match requirement.probe.trim() {
            "dir_exists" => requirement
                .path
                .as_deref()
                .map(|path| Path::new(cwd).join(path).is_dir())
                .unwrap_or(false),
            "file_exists" => requirement
                .path
                .as_deref()
                .map(|path| Path::new(cwd).join(path).is_file())
                .unwrap_or(false),
            "git_head" => run_git_cmd(cwd, &["rev-parse", "HEAD"])
                .await
                .map(|head| !head.trim().is_empty())
                .unwrap_or(false),
            _ => false,
        };
        if !present {
            missing.push(if label.is_empty() {
                key.to_string()
            } else {
                label.to_string()
            });
        }
    }
    missing
}

#[derive(Debug)]
struct GoalCheckExecResult {
    command: String,
    passed: bool,
    error_class: ErrorClass,
    digest: String,
}

async fn run_goal_check_command(
    label: &str,
    command: &str,
    cwd: &str,
    tx: &mpsc::Sender<StreamToken>,
) -> GoalCheckExecResult {
    let sig = command_sig(command);
    let command_for_evidence = compact_one_line(command, 200);
    let _ = tx
        .send(StreamToken::Delta(format!(
            "\n{}\n",
            governor_contract::goal_check_exec_run_message(label, &sig)
        )))
        .await;

    let exec_result = exec::run_command(command, Some(cwd)).await;
    let (stdout, stderr, exit_code) = match exec_result {
        Ok(r) => (r.stdout, r.stderr, r.exit_code),
        Err(e) => (String::new(), e.to_string(), -1),
    };
    let suspicious = if exit_code == 0 {
        suspicious_success_reason(&stdout, &stderr)
    } else {
        None
    };
    let passed = exit_code == 0 && suspicious.is_none();
    let error_class = if passed {
        ErrorClass::Unknown
    } else {
        classify_error(&stderr, &stdout)
    };
    let digest = if passed {
        String::new()
    } else if let Some(reason) = suspicious {
        reason
    } else {
        let interesting = pick_interesting_error_line(&stdout, &stderr);
        if interesting.is_empty() {
            truncate_output_tail(&format!("{stdout}{stderr}"), 600)
        } else {
            interesting
        }
    };

    let summary = if passed {
        format!(
            "{}\n",
            governor_contract::goal_check_exec_ok_message(label, &sig)
        )
    } else {
        format!(
            "{}\n",
            governor_contract::goal_check_exec_fail_message(
                label,
                &sig,
                compact_one_line(&digest, 160).as_str(),
            )
        )
    };
    let _ = tx.send(StreamToken::Delta(summary)).await;

    GoalCheckExecResult {
        command: command_for_evidence,
        passed,
        error_class,
        digest,
    }
}

// ── Agentic loop ──────────────────────────────────────────────────────────────

/// Run the agentic loop.  Sends StreamToken events to `tx` for the TUI to display.
/// The caller builds the initial messages (system + history + user).
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
        done_tool_def(),
    ]);
    let mut state = AgentState::Planning;
    let mut task_outcome = outcome::TaskOutcome::default();
    let mut pending_system_hint: Option<String> = None;
    let mut reflection_required: Option<String> = None;
    let mut impact_required: Option<String> = None;
    let mut reflection_trigger_sig: Option<String> = None;
    let mut reflection_guard: Option<ReflectionBlock> = None;
    let mut forced_tool_once = false;
    let mut tool_calls_this_run: usize = 0;
    // C — token budget guardian
    let mut budget_warned = false;
    // D — consecutive file-tool failure escalation
    let mut file_tool_consec_failures: usize = 0;

    let root_user_text = start
        .messages
        .iter()
        .rev()
        .find(|m| m["role"].as_str() == Some("user"))
        .and_then(|m| m["content"].as_str())
        .unwrap_or("")
        .to_string();
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
    if root_read_only && !has_system_prefix(&messages, "[Read-Only Task Contract]") {
        let pos = messages.len().min(4);
        messages.insert(
            pos,
            json!({"role":"system","content": read_only_task_addon()}),
        );
    }
    let mut active_plan = last_plan_from_messages(&messages).filter(|plan| {
        validate_plan_for_task_contract(plan, root_read_only, &task_contract, &instruction_resolver)
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
    // Rebuild loop governor memory from the existing session so resuming runs doesn't
    // repeat the same failures from scratch.
    let mut mem = FailureMemory::from_recent_messages(&messages);
    let mut recovery =
        RecoveryGovernor::restore_from_session(&mem, &messages, required_verification);
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

    // Resolve tool_root once (absolute path) and track cwd across tool calls.
    // This prevents the classic "cd didn't persist, so git add ran in the wrong repo" failure.
    let tool_root_abs = tool_root
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .and_then(absolutize_path);
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
    for iter in 0..max_iters {
        // ── Prune old tool results before sending to save context tokens ───
        prune_old_tool_results(&mut messages);
        prune_old_assistant_messages(&mut messages);
        prune_message_window(&mut messages);
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

        if let Some(final_text) = maybe_build_verified_action_closeout_text(
            &root_user_text,
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

        // ── Progress checkpoint every 3 iterations ────────────────────────
        // Require a reflection so the model re-evaluates goal distance and next action.
        // Only fires when no higher-priority hint/reflection is already pending.
        if iter > 0
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
        if iter + 1 == max_iters {
            let final_hint = format!(
                "[Final Iteration — iter {}/{}]\n\
This is the LAST model call for this run.\n\
- If the task is fully done AND verified: run ONE final smoke test (if needed), then call `done`.\n\
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

        let mut harness_overlay =
            harness_evolution_queue.build_overlay(task_harness, current_harness_proposal.as_ref());
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

        let harness_prompt = task_harness.prompt(test_cmd.as_deref());
        msgs_for_call.push(json!({
            "role": "system",
            "content": render_cached_prompt(
                &mut prompt_cache.task_harness_hash,
                harness_prompt,
                task_harness.compact_prompt(test_cmd.as_deref()),
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
        let tools_clone = tools.clone();
        let client_clone = client.clone();
        let msgs_clone = msgs_for_call;

        let stream_task = tokio::spawn(async move {
            stream_openai_compat_json(
                &client_clone,
                &cfg_clone,
                &msgs_clone,
                Some(&tools_clone),
                token_tx,
            )
            .await
        });

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

        if tool_calls.is_empty() {
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

        if tool_calls.is_empty() {
            if let Some(tc) = synthesize_fix_existing_no_tool_mutation_tool_call(
                task_harness,
                &messages,
                &root_user_text,
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

        if tool_calls.is_empty() {
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
                            let msg =
                                governor_contract::reflection_missing_message(reason.as_str());
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
                        let msg = governor_contract::reflection_missing_message(reason.as_str());
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
                let msg =
                    governor_contract::reflection_invalid_message(&e.to_string(), reason.as_str());
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
                let msg = governor_contract::reflection_one_tool_message(tool_calls.len());
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
                            let msg = governor_contract::impact_missing_message(reason.as_str());
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
                        let msg = governor_contract::impact_missing_message(reason.as_str());
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
                let msg =
                    governor_contract::impact_invalid_message(&e.to_string(), reason.as_str());
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
                let msg = governor_contract::impact_one_tool_message(tool_calls.len());
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

        // Guardrail: the runtime supports only one tool call per assistant turn.
        // (We stream tool_calls, but the executor is single-tool.)
        if tool_calls.len() > 1 {
            let msg = governor_contract::multiple_tool_calls_message(tool_calls.len());
            let _ = tx
                .send(StreamToken::Delta(format!("\n[governor] {msg}\n")))
                .await;
            impact_recovery::emit_rejection(&tx, "single_tool", &msg, &tool_calls, &assistant_text)
                .await;
            pending_system_hint = Some(msg);
            state = AgentState::Recovery;
            continue;
        }

        let mut tool_call: Option<ToolCallData> = tool_calls.pop();

        if let Some(tc) = tool_call.as_ref() {
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

        if let Some(tc) = tool_call.as_ref() {
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

        if let Some(tc) = tool_call.as_ref() {
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

        if let Some(tc) = tool_call.as_ref() {
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

        if let Some(tc) = tool_call.as_ref() {
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

        if let Some(tc) = tool_call.as_ref() {
            if let Some((rewritten, original, coerced)) =
                coerce_fix_existing_blocked_mutation_tool_call(
                    task_harness,
                    &messages,
                    tc,
                    &root_user_text,
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

        if let Some(tc) = tool_call.as_ref() {
            if let Some((rewritten, original, coerced)) =
                coerce_existing_followup_tool_call(&messages, tc, &root_user_text)
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
            if let Some(tc) = tool_call.as_ref() {
                if let Some((rewritten, original, coerced)) =
                    coerce_fix_existing_literal_mutation_tool_call(
                        task_harness,
                        &messages,
                        tc,
                        &root_user_text,
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

        if let Some(tc) = tool_call.as_ref() {
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

        if let Some(tc) = tool_call.as_ref() {
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

        if let Some(tc) = tool_call.as_ref() {
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

        if let Some(tc) = tool_call.as_ref() {
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
                                    recovery.stage = Some(RecoveryStage::Diagnose);
                                    let block =
                                        governor_contract::invalid_plan_message(&e.to_string());

                                    let _ = tx
                                        .send(StreamToken::Delta(format!(
                                            "[RESULT][Recovery] GOVERNOR BLOCK\n{block}\n"
                                        )))
                                        .await;

                                    push_blocked_tool_exchange(
                                        &mut messages,
                                        &assistant_text_clean,
                                        tc,
                                        &block,
                                    );
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
                            } else {
                                state = AgentState::Recovery;
                                recovery.stage = Some(RecoveryStage::Diagnose);
                                let block = governor_contract::invalid_plan_message(&e.to_string());

                                let _ = tx
                                    .send(StreamToken::Delta(format!(
                                        "[RESULT][Recovery] GOVERNOR BLOCK\n{block}\n"
                                    )))
                                    .await;

                                push_blocked_tool_exchange(
                                    &mut messages,
                                    &assistant_text_clean,
                                    tc,
                                    &block,
                                );
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
                        } else {
                            state = AgentState::Recovery;
                            recovery.stage = Some(RecoveryStage::Diagnose);
                            let block = governor_contract::invalid_plan_message(&e.to_string());

                            let _ = tx
                                .send(StreamToken::Delta(format!(
                                    "[RESULT][Recovery] GOVERNOR BLOCK\n{block}\n"
                                )))
                                .await;

                            push_blocked_tool_exchange(
                                &mut messages,
                                &assistant_text_clean,
                                tc,
                                &block,
                            );
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

                                let _ = tx
                                    .send(StreamToken::Delta(format!(
                                        "[RESULT][Recovery] GOVERNOR BLOCK\n{block}\n"
                                    )))
                                    .await;

                                push_blocked_tool_exchange(
                                    &mut messages,
                                    &assistant_text_clean,
                                    tc,
                                    &block,
                                );
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

                                let _ = tx
                                    .send(StreamToken::Delta(format!(
                                        "[RESULT][Recovery] GOVERNOR BLOCK\n{block}\n"
                                    )))
                                    .await;

                                push_blocked_tool_exchange(
                                    &mut messages,
                                    &assistant_text_clean,
                                    tc,
                                    &block,
                                );
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

                                    let _ = tx
                                        .send(StreamToken::Delta(format!(
                                            "[RESULT][Recovery] GOVERNOR BLOCK\n{block}\n"
                                        )))
                                        .await;

                                    push_blocked_tool_exchange(
                                        &mut messages,
                                        &assistant_text_clean,
                                        tc,
                                        &block,
                                    );
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
                            } else {
                                state = AgentState::Recovery;
                                recovery.stage = Some(RecoveryStage::Diagnose);
                                let block = governor_contract::missing_plan_message();

                                let _ = tx
                                    .send(StreamToken::Delta(format!(
                                        "[RESULT][Recovery] GOVERNOR BLOCK\n{block}\n"
                                    )))
                                    .await;

                                push_blocked_tool_exchange(
                                    &mut messages,
                                    &assistant_text_clean,
                                    tc,
                                    &block,
                                );
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
                        } else {
                            state = AgentState::Recovery;
                            recovery.stage = Some(RecoveryStage::Diagnose);
                            let block = governor_contract::missing_plan_message();

                            let _ = tx
                                .send(StreamToken::Delta(format!(
                                    "[RESULT][Recovery] GOVERNOR BLOCK\n{block}\n"
                                )))
                                .await;

                            push_blocked_tool_exchange(
                                &mut messages,
                                &assistant_text_clean,
                                tc,
                                &block,
                            );
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
                .filter(|_| !requires_authored_final_answer(&root_user_text))
                {
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

                let _ = tx
                    .send(StreamToken::Delta(format!(
                        "[RESULT][Recovery] GOVERNOR BLOCK\n{block}\n"
                    )))
                    .await;

                push_blocked_tool_exchange(&mut messages, &assistant_text_clean, tc, &block);
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

                    let _ = tx
                        .send(StreamToken::Delta(format!(
                            "[RESULT][Recovery] GOVERNOR BLOCK\n{block}\n"
                        )))
                        .await;

                    push_blocked_tool_exchange(&mut messages, &assistant_text_clean, tc, &block);
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

                let _ = tx
                    .send(StreamToken::Delta(format!(
                        "[RESULT][Recovery] GOVERNOR BLOCK\n{block}\n"
                    )))
                    .await;

                push_blocked_tool_exchange(&mut messages, &assistant_text_clean, tc, &block);
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

            if let Some(conflict) = refuted_assumption_conflict(&assumption_ledger, think, tc) {
                state = AgentState::Recovery;
                recovery.stage = Some(RecoveryStage::Diagnose);
                let block = format!(
                    "[Assumption Ledger] {conflict}\nGather new evidence or choose a different next action before retrying."
                );

                let _ = tx
                    .send(StreamToken::Delta(format!(
                        "[RESULT][Recovery] GOVERNOR BLOCK\n{block}\n"
                    )))
                    .await;

                push_blocked_tool_exchange(&mut messages, &assistant_text_clean, tc, &block);
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

            if let Some(conflict) = instruction_resolver.tool_conflict(tc, test_cmd.as_deref()) {
                state = AgentState::Recovery;
                recovery.stage = Some(RecoveryStage::Diagnose);
                let block = conflict.render();

                let _ = tx
                    .send(StreamToken::Delta(format!(
                        "[RESULT][Recovery] GOVERNOR BLOCK\n{block}\n"
                    )))
                    .await;

                push_blocked_tool_exchange(&mut messages, &assistant_text_clean, tc, &block);
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
                let _ = tx
                    .send(StreamToken::Delta(format!(
                        "[RESULT][Recovery] GOVERNOR BLOCK\n{block}\n"
                    )))
                    .await;
                emit_telemetry_event(
                    &tx,
                    "progress_gate_blocked",
                    json!({
                        "lane": task_harness.lane_label(),
                        "tool": tc.name,
                    }),
                )
                .await;

                push_blocked_tool_exchange(&mut messages, &assistant_text_clean, tc, &block);
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

            if let Some(block) = evaluator_loop.build_violation_block(tc) {
                state = AgentState::Recovery;
                let _ = tx
                    .send(StreamToken::Delta(format!(
                        "[RESULT][Recovery] GOVERNOR BLOCK\n{block}\n"
                    )))
                    .await;
                emit_telemetry_event(
                    &tx,
                    "evaluator_loop_blocked",
                    json!({
                        "policy_id": evaluator_loop.policy_id(),
                        "tool": tc.name,
                    }),
                )
                .await;

                push_blocked_tool_exchange(&mut messages, &assistant_text_clean, tc, &block);
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
                            let _ = tx
                                .send(StreamToken::Delta(format!(
                                    "[RESULT][Recovery] GOVERNOR BLOCK\n{block}\n"
                                )))
                                .await;
                            push_blocked_tool_exchange(
                                &mut messages,
                                &assistant_text_clean,
                                tc,
                                &block,
                            );
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
                    let _ = tx
                        .send(StreamToken::Delta(format!(
                            "[RESULT][Recovery] GOVERNOR BLOCK\n{block}\n"
                        )))
                        .await;
                    push_blocked_tool_exchange(&mut messages, &assistant_text_clean, tc, &block);
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
                "tool_calls": [{
                    "id": tc.id,
                    "type": "function",
                    "function": {
                        "name": tc.name,
                        "arguments": tc.arguments
                    }
                }]
            }));
        } else {
            if realize_cfg.enabled {
                if let Some(plan) = parsed_plan.as_ref().filter(|plan| {
                    validate_plan_for_task_contract(
                        plan,
                        root_read_only,
                        &task_contract,
                        &instruction_resolver,
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
                        recovery.stage = Some(RecoveryStage::Diagnose);
                        let mut block = governor_contract::invalid_plan_message(&e.to_string());
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
                let implied_scripts = extract_implied_exec_scripts(&assistant_text_clean);
                let implied_files = extract_implied_write_files(&assistant_text_clean);
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
                        messages.push(json!({
                            "role": "user",
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
                        for imf in implied_files {
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
                            let (mut r_text, r_err) =
                                crate::file_tools::tool_write_file(&path, &imf.content, base);
                            if !r_err {
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
                                        r_text.push_str(&run_test_cmd(cmd, root).await);
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
                            messages.push(json!({
                                "role": "user",
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
                        }
                    }

                    continue;
                }
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
                                    messages.push(json!({"role":"user","content": block}));
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
                                    messages.push(json!({"role":"user","content": block}));
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
                                    messages.push(json!({"role":"user","content": block}));
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
                                    messages.push(json!({"role":"user","content": block}));
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
                                    messages.push(json!({"role":"user","content": block}));
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
                        task_outcome.verified_goal_checks(&[
                            (wants_repo_goal, goal_checks.repo.ok),
                            (wants_test_goal, goal_checks.tests.ok),
                            (wants_build_goal, goal_checks.build.ok),
                        ]);
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
                                "tool_calls": [{
                                    "id": tc.id,
                                    "type": "function",
                                    "function": {
                                        "name": tc.name,
                                        "arguments": tc.arguments
                                    }
                                }]
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
                                "tool_calls": [{
                                    "id": tc.id,
                                    "type": "function",
                                    "function": {
                                        "name": tc.name,
                                        "arguments": tc.arguments
                                    }
                                }]
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
                    .map(|s| reflection_trigger_sig.as_deref() != Some(s.as_str()))
                    .unwrap_or(true)
            {
                reflection_guard = None;
                reflection_trigger_sig = None;
            }
        }

        // ── done tool ──────────────────────────────────────────────────────
        if tc.name.as_str() == "done" {
            if realize_cfg.enabled {
                if let Some(latent) = latent_plan.as_ref() {
                    state = AgentState::Recovery;
                    recovery.stage = Some(RecoveryStage::Diagnose);
                    let block = build_realize_done_gate_message(latent);

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

                    pending_system_hint = Some(block);
                    emit_realize_state(
                        &tx,
                        &realize_cfg,
                        latent_plan.as_ref(),
                        iter,
                        latent_drift_for_prompt,
                        &realize_metrics,
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

            if let Some(command) = benchmark_plan_pending_required_exec_command(
                &root_user_text,
                &messages,
                test_cmd.as_deref(),
            ) {
                state = AgentState::Recovery;
                recovery.stage = Some(RecoveryStage::Verify);
                let block = format!(
                    "[Done Gate] Approved benchmark plan requires explicit verification proof before `done`.\n\
Required now: run `{command}`."
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

            let args: serde_json::Value = serde_json::from_str(&tc.arguments).unwrap_or(json!({}));
            let summary = args["summary"].as_str().unwrap_or("").trim();
            if !root_read_only {
                if let Err(block) = validate_authored_done_summary(&root_user_text, summary) {
                    state = AgentState::Recovery;
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
            }
            let mut completed_acceptance = parse_string_list_arg(&args["completed_acceptance"]);
            let mut remaining_acceptance = parse_string_list_arg(&args["remaining_acceptance"]);
            let mut acceptance_evidence =
                parse_done_acceptance_evidence(&args["acceptance_evidence"]);
            let next_steps = args["next_steps"].as_str().unwrap_or("").trim();
            let done_plan = validated_plan_for_turn.as_ref().or(active_plan.as_ref());
            let known_acceptance_commands = canonicalize_known_acceptance_commands(
                &collect_known_acceptance_commands(&messages, &working_mem, test_cmd.as_deref()),
                &observation_evidence,
            );
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
                    let rescued_rows =
                        if let Some((rescued_completed, rescued_remaining, rescued_evidence)) =
                            rescue_invalid_done_payload_for_verified_action(
                                done_plan,
                                &known_acceptance_commands,
                                &observation_evidence,
                                required_verification,
                                test_cmd.as_deref(),
                                last_mutation_step,
                                last_verify_ok_step,
                            )
                        {
                            if let Ok(rows) = validate_done_acceptance(
                                done_plan,
                                &rescued_completed,
                                &rescued_remaining,
                                &rescued_evidence,
                                &known_acceptance_commands,
                                &observation_evidence,
                            ) {
                                completed_acceptance = rescued_completed;
                                remaining_acceptance = rescued_remaining;
                                acceptance_evidence = rescued_evidence;
                                Some(rows)
                            } else {
                                None
                            }
                        } else {
                            None
                        };
                    if let Some(rows) = rescued_rows {
                        let _ = tx
                            .send(StreamToken::Delta(
                                "\n[compat] repaired done acceptance payload from current plan and known commands\n"
                                    .to_string(),
                            ))
                            .await;
                        rows
                    } else {
                        if root_read_only {
                            if let Some(plan) = done_plan {
                                if let Some(final_text) = build_read_only_strong_final_answer(
                                    &root_user_text,
                                    plan,
                                    &observation_evidence,
                                    &messages,
                                    &working_mem,
                                ) {
                                    task_outcome.finalized(&final_text);
                                    state = AgentState::Done;
                                    messages.push(json!({
                                        "role": "tool",
                                        "tool_call_id": tc.id,
                                        "content": "OK: done"
                                    }));
                                    messages.push(
                                        json!({"role": "assistant", "content": final_text.clone()}),
                                    );
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
                                            "\n[agent] strong read-only evidence rescued invalid done gate; auto-finalized instead.\n\n{final_text}\n"
                                        )))
                                        .await;
                                    break;
                                }
                            }
                        }
                        state = AgentState::Recovery;
                        recovery.stage = Some(if root_read_only {
                            RecoveryStage::Diagnose
                        } else {
                            RecoveryStage::Verify
                        });
                        let hint = build_done_acceptance_recovery_hint(
                            &e.to_string(),
                            &known_acceptance_commands,
                            &read_only_scores,
                        );
                        let block = format!(
                            "{}{}",
                            governor_contract::done_invalid_acceptance_message(&e.to_string()),
                            hint
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
                }
            };

            task_outcome.accept_done(&remaining_acceptance);

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
        if let Some(block) = recovery.maybe_block_tool(
            &tc,
            test_cmd.as_deref(),
            task_harness,
            allow_existing_followup_verify || allow_benchmark_plan_followup_verify,
        ) {
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

        if tc.name.as_str() == "apply_diff" {
            let args: serde_json::Value = serde_json::from_str(&tc.arguments).unwrap_or(json!({}));
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
            if !is_error {
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
                        let test_out = run_test_cmd(cmd, root).await;
                        result.push_str(&test_out);
                    }
                }
            }

            let verified =
                !is_error && crate::execution_evidence::auto_test_succeeded("apply_diff", &result);
            let verified_level = if verified {
                configured_test_cmd_verification_level(test_cmd.as_deref())
            } else {
                None
            };
            if is_error {
                recovery.on_fix_result(false, None);
            } else {
                last_mutation_step = Some(this_step);
                if let Some(level) = verified_level {
                    match level {
                        VerificationLevel::Build => last_build_verify_ok_step = Some(this_step),
                        VerificationLevel::Behavioral => {
                            last_behavioral_verify_ok_step = Some(this_step)
                        }
                    }
                    last_verify_ok_step = effective_verify_ok_step(
                        required_verification,
                        last_build_verify_ok_step,
                        last_behavioral_verify_ok_step,
                    );
                }
                recovery.on_fix_result(true, verified_level);
            }

            let first_line = result.lines().next().unwrap_or("").to_string();
            if is_error {
                let _ = tx
                    .send(StreamToken::Delta(format!(
                        "[RESULT_FILE_ERR] {first_line}\n"
                    )))
                    .await;
                state = AgentState::Recovery;
                if !rejected_by_user {
                    file_tool_consec_failures += 1;
                    pending_system_hint = Some(format!("apply_diff error: {first_line}"));
                    if file_tool_consec_failures >= 2 {
                        reflection_required = Some(format!(
                            "file tool failures repeated {} times",
                            file_tool_consec_failures
                        ));
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
                file_tool_consec_failures = 0;
                pending_system_hint = if recovery.stage == Some(RecoveryStage::Verify) {
                    Some(format!(
                        "Recovery stage=verify: {}",
                        verification_requirement_hint(required_verification, test_cmd.as_deref())
                    ))
                } else {
                    None
                };
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

            if is_error {
                reflection_required = Some(
                    pending_system_hint
                        .clone()
                        .unwrap_or_else(|| "apply_diff failed".to_string()),
                );
                reflection_trigger_sig =
                    Some(format!("apply_diff:{}", normalize_for_signature(&path)));
            }

            let history_result = if is_error {
                result.clone()
            } else {
                compact_success_tool_result_for_history("apply_diff", &result)
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

        // ── glob tool ─────────────────────────────────────────────────────
        // ── list_dir tool ────────────────────────────────────────────────────
        if tc.name.as_str() == "list_dir" {
            let args: serde_json::Value = serde_json::from_str(&tc.arguments).unwrap_or(json!({}));
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
            let args: serde_json::Value = serde_json::from_str(&tc.arguments).unwrap_or(json!({}));
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
            let args: serde_json::Value = serde_json::from_str(&tc.arguments).unwrap_or(json!({}));
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
            let args: serde_json::Value = serde_json::from_str(&tc.arguments).unwrap_or(json!({}));
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

            let (result, is_error) = match tc.name.as_str() {
                "read_file" => {
                    // ── Gap 6: serve from cache if file hasn't changed ──────
                    if let Some(cached) = file_cache.get(&cache_key) {
                        let header = cached.lines().next().unwrap_or(&path).to_string();
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
                        if !err {
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
                        (
                            format!(
                                "GOVERNOR BLOCKED: write_file refused because '{path}' already exists.\n\
Action required: call read_file(path) first to confirm current contents, then retry with patch_file/apply_diff (preferred) or write_file."
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
                            let (mut r_text, r_err) =
                                crate::file_tools::tool_write_file(&path, &content, base);
                            if !r_err {
                                file_cache.insert(cache_key.clone(), content.clone());
                                r_text.push_str(&format!(
                                    "\n[hash] before={} after={}",
                                    fmt_hash(before_hash),
                                    fmt_hash(after_hash)
                                ));
                                // A — auto-test after write
                                if let Some(ref cmd) = test_cmd {
                                    if let Some(ref root) = tool_root_abs {
                                        r_text.push_str(&run_test_cmd(cmd, root).await);
                                    }
                                }
                            }
                            (r_text, r_err)
                        }
                    }
                }
                _ => {
                    // ── patch_file ─────────────────────────────────────────
                    let search = args["search"].as_str().unwrap_or("").to_string();
                    let replace = args["replace"].as_str().unwrap_or("").to_string();

                    // B — capture old content for diff preview before patching.
                    let old_content_for_diff = file_cache.get(&cache_key).cloned().or_else(|| {
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
                        .map(|old| crate::file_tools::make_patch_diff(old, &search, &replace))
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
                            crate::file_tools::tool_patch_file(&path, &search, &replace, base);

                        if !patch_err {
                            file_cache.remove(&cache_key); // invalidate stale cache

                            // B — append diff preview (shows exactly what changed).
                            if let Some(ref old) = old_content_for_diff {
                                let diff =
                                    crate::file_tools::make_patch_diff(old, &search, &replace);
                                if !diff.is_empty() {
                                    patch_result.push_str(&format!("\n{diff}"));
                                }
                            }

                            // Gap 7: auto-verify patch was applied correctly.
                            if let Ok(abs) = crate::file_tools::resolve_safe_path(&path, base) {
                                if let Ok(new_content) = std::fs::read_to_string(&abs) {
                                    let after_hash = hash_text(&new_content);
                                    patch_result.push_str(&format!(
                                        "\n[hash] before={} after={}",
                                        fmt_hash(before_hash),
                                        fmt_hash(after_hash)
                                    ));
                                    if replace.is_empty() || new_content.contains(&replace) {
                                        patch_result
                                            .push_str("\n✓ auto-verify: patch confirmed in file");
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
                                    patch_result.push_str(&run_test_cmd(cmd, root).await);
                                }
                            }
                        }
                        (patch_result, patch_err)
                    }
                }
            };

            // D — track consecutive file-tool failures for escalation.
            if is_error {
                if !rejected_by_user {
                    file_tool_consec_failures += 1;
                }
                if file_tool_consec_failures >= 2 {
                    reflection_required = Some(format!(
                        "file tool failures repeated {} times",
                        file_tool_consec_failures
                    ));
                }
            } else {
                file_tool_consec_failures = 0;
            }

            let verified = !is_error
                && crate::execution_evidence::auto_test_succeeded(tc.name.as_str(), &result);
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
                    } else {
                        last_mutation_step = Some(this_step);
                        if let Some(level) = verified_level {
                            match level {
                                VerificationLevel::Build => {
                                    last_build_verify_ok_step = Some(this_step)
                                }
                                VerificationLevel::Behavioral => {
                                    last_behavioral_verify_ok_step = Some(this_step)
                                }
                            }
                            last_verify_ok_step = effective_verify_ok_step(
                                required_verification,
                                last_build_verify_ok_step,
                                last_behavioral_verify_ok_step,
                            );
                        }
                        recovery.on_fix_result(true, verified_level);
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
                // D — escalate after 3 consecutive file-tool failures.
                let hint = if rejected_by_user {
                    "The user rejected the edit. Choose a safer alternative or ask again with a smaller change."
                        .to_string()
                } else if file_tool_consec_failures >= 3 {
                    format!(
                        "CRITICAL: {file_tool_consec_failures} consecutive file-tool errors.\n\
                         You MUST abandon the current approach. Do NOT retry the same operation.\n\
                         Instead: call read_file to inspect the actual file state, then choose \
                         a completely different strategy (e.g. write_file instead of patch_file)."
                    )
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
                    tc.name.as_str(),
                    verified,
                    step_label_for_turn.as_deref(),
                    next_action_for_turn,
                    test_cmd.as_deref(),
                );
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
                assumption_ledger.refresh_confirmations(&working_mem);
                if matches!(tc.name.as_str(), "write_file" | "patch_file") {
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
                reflection_trigger_sig = Some(format!(
                    "{}:{}",
                    tc.name.as_str(),
                    normalize_for_signature(&path)
                ));
            }

            // Append tool result to conversation.
            let history_result = if is_error {
                result.clone()
            } else {
                compact_success_tool_result_for_history(tc.name.as_str(), &result)
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

        let args: serde_json::Value =
            serde_json::from_str(&tc.arguments).unwrap_or(json!({"command": tc.arguments}));
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

            // Update memory so repeating the same blocked command triggers stronger hints.
            let _ = mem.on_tool_result(&command, "", &block, 1);
            last_exec_step = Some(this_step);
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

            if iter + 1 == max_iters {
                let _ = tx
                    .send(StreamToken::Delta(format!(
                        "\n[agent] iteration cap ({max_iters}) reached.\n"
                    )))
                    .await;
            }
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
            reflection_trigger_sig = Some(format!("exec:{cmd_sig}"));
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
        messages.push(json!({
            "role": "tool",
            "tool_call_id": tc.id,
            "content": history_tool_output,
        }));
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

        if effective_exit_code != 0
            || mem.same_error_repeats >= 2
            || mem.same_command_repeats >= 3
            || mem.same_output_repeats >= 2
        {
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
            reflection_trigger_sig = Some(format!("exec:{}", command_sig(&command)));
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

    if let Some(final_text) = maybe_build_verified_action_closeout_text(
        &root_user_text,
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
            "completed": task_outcome.completed(&messages),
            "state": format!("{state:?}").to_ascii_lowercase(),
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
mod tests {
    use super::*;

    #[test]
    fn extracts_implied_write_files_from_markdown() {
        let text = r#"
File: README.md
```md
# Hello
```

### src/main.rs
```rust
fn main() {}
```

```diff
--- a/x
+++ b/x
@@ -1 +1 @@
-a
+b
```
"#;
        let files = extract_implied_write_files(text);
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].path, "README.md");
        assert!(files[0].content.contains("# Hello"));
        assert_eq!(files[1].path, "src/main.rs");
        assert!(files[1].content.contains("fn main"));
    }

    #[test]
    fn blocks_git_init_inside_existing_repo() {
        let td = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(td.path().join(".git")).expect("mkdir .git");
        let root = td.path().to_string_lossy();
        let msg = should_block_git_landmines("git init Foo", Some(root.as_ref()))
            .expect("expected block");
        assert!(msg.to_ascii_lowercase().contains("refusing"));
    }

    #[test]
    fn git_checkpoint_requires_tool_root_to_be_git_top_level() {
        let td = tempfile::tempdir().expect("tempdir");
        let root = td.path().join("repo");
        let child = root.join(".tmp").join("case").join("tool_root");
        std::fs::create_dir_all(&child).expect("mkdir child");

        assert!(git_checkpoint_root_matches_tool_root(
            root.to_string_lossy().as_ref(),
            root.to_string_lossy().as_ref()
        ));
        assert!(!git_checkpoint_root_matches_tool_root(
            child.to_string_lossy().as_ref(),
            root.to_string_lossy().as_ref()
        ));
    }

    #[test]
    fn blocks_git_add_when_nested_repo_detected() {
        let td = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(td.path().join(".git")).expect("mkdir .git");
        std::fs::create_dir_all(td.path().join("MazeGame").join(".git")).expect("mkdir nested");
        let root = td.path().to_string_lossy();
        let msg =
            should_block_git_landmines("git add -A", Some(root.as_ref())).expect("expected block");
        assert!(msg.contains("MazeGame"));
    }

    #[test]
    fn allows_git_add_when_nested_is_submodule() {
        let td = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(td.path().join(".git")).expect("mkdir .git");
        std::fs::create_dir_all(td.path().join("MazeGame").join(".git")).expect("mkdir nested");
        std::fs::write(
            td.path().join(".gitmodules"),
            "[submodule \"MazeGame\"]\n\tpath = MazeGame\n\turl = https://example.invalid/MazeGame.git\n",
        )
        .expect("write .gitmodules");
        let root = td.path().to_string_lossy();
        assert!(
            should_block_git_landmines("git add -A", Some(root.as_ref())).is_none(),
            "should not block when nested repo is declared as submodule"
        );
    }

    #[test]
    fn injects_git_proxy_hint_for_github_via_localhost() {
        let stderr = "fatal: unable to access 'https://github.com/x/y.git/': Failed to connect to github.com port 443 via 127.0.0.1 after 2041 ms: Could not connect to server";
        let out = build_failed_tool_output("", stderr, 1);
        assert!(
            out.contains("ssh.github.com"),
            "should suggest ssh-over-443"
        );
        assert!(
            out.contains("HTTP_PROXY") || out.contains("HTTPS_PROXY"),
            "should mention proxy env vars"
        );
    }

    #[test]
    fn injects_cargo_exe_lock_hint_on_windows_style_error() {
        let stderr = "error: failed to remove file `C:\\\\Users\\\\user\\\\spiral-coder\\\\target\\\\debug\\\\spiral-coder.exe`\nCaused by: Access is denied. (os error 5)";
        let out = build_failed_tool_output("", stderr, 1);
        assert!(
            out.to_ascii_lowercase().contains("cargo_target_dir"),
            "should suggest isolated target dir"
        );
    }

    #[test]
    fn parses_reflection_block_fields() {
        let text = "\
<reflect>\n\
last_outcome: failure\n\
goal_delta: Further\n\
wrong_assumption: I thought the file existed.\n\
strategy_change: abandon\n\
next_minimal_action: read_file src/tui/agent.rs\n\
</reflect>\n";
        let r = parse_reflection_block(text).expect("reflection parsed");
        assert_eq!(r.goal_delta, GoalDelta::Farther);
        assert_eq!(r.strategy_change, StrategyChange::Abandon);
        assert_eq!(r.wrong_assumption, "I thought the file existed.");
        assert_eq!(r.next_minimal_action, "read_file src/tui/agent.rs");
    }

    #[test]
    fn parses_impact_block_fields() {
        let text = "\
<impact>\n\
changed: patched src/tui/agent.rs validation\n\
progress: step 2 moved because runtime gate exists\n\
remaining_gap: still need to run cargo test\n\
</impact>\n";
        let impact = parse_impact_block(text).expect("impact parsed");
        assert_eq!(impact.changed, "patched src/tui/agent.rs validation");
        assert_eq!(impact.progress, "step 2 moved because runtime gate exists");
        assert_eq!(impact.remaining_gap, "still need to run cargo test");
    }

    #[test]
    fn validate_impact_requires_all_fields() {
        let impact = ImpactBlock {
            changed: "patched file".to_string(),
            progress: "".to_string(),
            remaining_gap: "".to_string(),
        };
        let plan = PlanBlock {
            goal: "ship fix".to_string(),
            steps: vec!["inspect".to_string(), "verify".to_string()],
            acceptance_criteria: vec!["cargo check passes".to_string()],
            risks: "wrong file".to_string(),
            assumptions: "tests exist".to_string(),
        };
        assert!(validate_impact(&impact, Some(&plan)).is_err());
    }

    #[test]
    fn validate_impact_accepts_matching_acceptance_criterion() {
        let impact = ImpactBlock {
            changed: "runtime gate now blocks invalid tool calls".to_string(),
            progress: "acceptance 2 moved because runtime gate blocks bad tool calls".to_string(),
            remaining_gap: "still need cargo check".to_string(),
        };
        let plan = PlanBlock {
            goal: "ship fix".to_string(),
            steps: vec!["inspect".to_string(), "verify".to_string()],
            acceptance_criteria: vec![
                "cargo check passes".to_string(),
                "runtime gate blocks bad tool calls".to_string(),
            ],
            risks: "wrong file".to_string(),
            assumptions: "tests exist".to_string(),
        };
        assert!(validate_impact(&impact, Some(&plan)).is_ok());
    }

    #[test]
    fn validate_impact_rejects_unknown_progress_reference() {
        let impact = ImpactBlock {
            changed: "updated docs".to_string(),
            progress: "made criterion 9 move".to_string(),
            remaining_gap: "still need review".to_string(),
        };
        let plan = PlanBlock {
            goal: "ship fix".to_string(),
            steps: vec!["inspect".to_string(), "verify".to_string()],
            acceptance_criteria: vec!["cargo check passes".to_string()],
            risks: "wrong file".to_string(),
            assumptions: "tests exist".to_string(),
        };
        assert!(validate_impact(&impact, Some(&plan)).is_err());
    }

    #[test]
    fn validate_done_acceptance_accepts_completed_and_remaining_coverage() {
        let plan = PlanBlock {
            goal: "ship fix".to_string(),
            steps: vec!["inspect".to_string(), "verify".to_string()],
            acceptance_criteria: vec![
                "cargo check passes".to_string(),
                "runtime gate blocks bad tool calls".to_string(),
            ],
            risks: "wrong file".to_string(),
            assumptions: "tests exist".to_string(),
        };

        let completed_acceptance = vec!["acceptance 1".to_string()];
        let remaining_acceptance = vec!["runtime gate blocks bad tool calls".to_string()];
        let acceptance_evidence = vec![DoneAcceptanceEvidence {
            criterion: "acceptance 1".to_string(),
            command: "cargo check".to_string(),
        }];
        let known_commands = vec!["cargo check".to_string()];

        assert!(validate_done_acceptance(
            Some(&plan),
            &completed_acceptance,
            &remaining_acceptance,
            &acceptance_evidence,
            &known_commands,
            &ObservationEvidence::default(),
        )
        .is_ok());
    }

    #[test]
    fn validate_done_acceptance_rejects_missing_criterion_coverage() {
        let plan = PlanBlock {
            goal: "ship fix".to_string(),
            steps: vec!["inspect".to_string(), "verify".to_string()],
            acceptance_criteria: vec![
                "cargo check passes".to_string(),
                "runtime gate blocks bad tool calls".to_string(),
            ],
            risks: "wrong file".to_string(),
            assumptions: "tests exist".to_string(),
        };

        let completed_acceptance = vec!["acceptance 1".to_string()];
        let remaining_acceptance = Vec::new();
        let acceptance_evidence = vec![DoneAcceptanceEvidence {
            criterion: "acceptance 1".to_string(),
            command: "cargo check".to_string(),
        }];
        let known_commands = vec!["cargo check".to_string()];

        assert!(validate_done_acceptance(
            Some(&plan),
            &completed_acceptance,
            &remaining_acceptance,
            &acceptance_evidence,
            &known_commands,
            &ObservationEvidence::default(),
        )
        .is_err());
    }

    #[test]
    fn validate_done_acceptance_rejects_unknown_reference() {
        let plan = PlanBlock {
            goal: "ship fix".to_string(),
            steps: vec!["inspect".to_string(), "verify".to_string()],
            acceptance_criteria: vec!["cargo check passes".to_string()],
            risks: "wrong file".to_string(),
            assumptions: "tests exist".to_string(),
        };

        let completed_acceptance = vec!["acceptance 9".to_string()];
        let remaining_acceptance = Vec::new();
        let acceptance_evidence = vec![DoneAcceptanceEvidence {
            criterion: "acceptance 9".to_string(),
            command: "cargo check".to_string(),
        }];
        let known_commands = vec!["cargo check".to_string()];

        assert!(validate_done_acceptance(
            Some(&plan),
            &completed_acceptance,
            &remaining_acceptance,
            &acceptance_evidence,
            &known_commands,
            &ObservationEvidence::default(),
        )
        .is_err());
    }

    #[test]
    fn validate_done_acceptance_rejects_missing_evidence_for_completed_criterion() {
        let plan = PlanBlock {
            goal: "ship fix".to_string(),
            steps: vec!["inspect".to_string(), "verify".to_string()],
            acceptance_criteria: vec!["cargo check passes".to_string()],
            risks: "wrong file".to_string(),
            assumptions: "tests exist".to_string(),
        };

        let completed_acceptance = vec!["acceptance 1".to_string()];
        let remaining_acceptance = Vec::new();
        let acceptance_evidence = Vec::new();
        let known_commands = vec!["cargo check".to_string()];

        assert!(validate_done_acceptance(
            Some(&plan),
            &completed_acceptance,
            &remaining_acceptance,
            &acceptance_evidence,
            &known_commands,
            &ObservationEvidence::default(),
        )
        .is_err());
    }

    #[test]
    fn validate_done_acceptance_rejects_unknown_verification_command() {
        let plan = PlanBlock {
            goal: "ship fix".to_string(),
            steps: vec!["inspect".to_string(), "verify".to_string()],
            acceptance_criteria: vec!["cargo check passes".to_string()],
            risks: "wrong file".to_string(),
            assumptions: "tests exist".to_string(),
        };

        let completed_acceptance = vec!["acceptance 1".to_string()];
        let remaining_acceptance = Vec::new();
        let acceptance_evidence = vec![DoneAcceptanceEvidence {
            criterion: "acceptance 1".to_string(),
            command: "cargo test".to_string(),
        }];
        let known_commands = vec!["cargo check".to_string()];

        assert!(validate_done_acceptance(
            Some(&plan),
            &completed_acceptance,
            &remaining_acceptance,
            &acceptance_evidence,
            &known_commands,
            &ObservationEvidence::default(),
        )
        .is_err());
    }

    #[test]
    fn validate_done_acceptance_accepts_canonicalized_tool_evidence() {
        let plan = PlanBlock {
            goal: "locate slash command".to_string(),
            steps: vec!["inspect".to_string(), "confirm".to_string()],
            acceptance_criteria: vec![
                "The exact file path is identified".to_string(),
                "The handler context is confirmed".to_string(),
            ],
            risks: "wrong file".to_string(),
            assumptions: "repo indexed".to_string(),
        };

        let completed_acceptance = vec!["acceptance 1".to_string(), "acceptance 2".to_string()];
        let remaining_acceptance = Vec::new();
        let acceptance_evidence = vec![
            DoneAcceptanceEvidence {
                criterion: "acceptance 1".to_string(),
                command: "search_files(pattern=\"/realize\", dir=\"src/tui/events.rs\")"
                    .to_string(),
            },
            DoneAcceptanceEvidence {
                criterion: "acceptance 2".to_string(),
                command: "read_file(path=\"src/tui/events.rs\")".to_string(),
            },
        ];
        let known_commands = vec![
            "search_files(dir=/Users/ignored, pattern=nope)".to_string(),
            "search_files(dir=src/tui/events.rs, pattern=/realize)".to_string(),
            "read_file(path=src/tui/events.rs)".to_string(),
        ];

        assert!(validate_done_acceptance(
            Some(&plan),
            &completed_acceptance,
            &remaining_acceptance,
            &acceptance_evidence,
            &known_commands,
            &ObservationEvidence::default(),
        )
        .is_ok());
    }

    #[test]
    fn resolve_acceptance_reference_accepts_leading_numbered_text() {
        let plan = PlanBlock {
            goal: "locate handler".to_string(),
            steps: vec!["inspect".to_string()],
            acceptance_criteria: vec![
                "The exact file path containing the `/realize` slash command handler is identified."
                    .to_string(),
                "The handler logic is confirmed to be part of the TUI component.".to_string(),
            ],
            risks: "wrong file".to_string(),
            assumptions: "repo indexed".to_string(),
        };

        assert_eq!(
            resolve_acceptance_reference(
                "1) The exact file path containing the `/realize` slash command handler is identified (`src/tui/events.rs`).",
                &plan
            ),
            Some(0)
        );
        assert_eq!(
            resolve_acceptance_reference(
                "2) The handler logic is confirmed to be part of the TUI component (within `tui/events.rs`).",
                &plan
            ),
            Some(1)
        );
    }

    #[test]
    fn build_read_only_evidence_scores_prefers_search_plus_read_confirmation() {
        let plan = PlanBlock {
            goal: "Locate the /realize slash command handler in the TUI".to_string(),
            steps: vec![
                "inspect src".to_string(),
                "search for /realize".to_string(),
                "read the matching file".to_string(),
            ],
            acceptance_criteria: vec![
                "The exact file path containing the `/realize` slash command handler is identified."
                    .to_string(),
                "The handler logic is confirmed to be part of the TUI component.".to_string(),
                "The location is verified by reading the file and confirming the context."
                    .to_string(),
            ],
            risks: "wrong file".to_string(),
            assumptions: "repo indexed".to_string(),
        };
        let evidence = ObservationEvidence {
            searches: vec![ObservationSearchEvidence {
                command: "search_files(dir=src, pattern=/realize)".to_string(),
                pattern: "/realize".to_string(),
                hit_count: 1,
                paths: vec!["src/tui/events.rs".to_string()],
            }],
            reads: vec![ObservationReadEvidence {
                command: "read_file(path=src/tui/events.rs)".to_string(),
                path: "src/tui/events.rs".to_string(),
            }],
            resolutions: Vec::new(),
        };

        let scores = build_read_only_evidence_scores(
            "Locate where the /realize slash command is handled in the TUI. Do not edit anything.",
            &plan,
            &evidence,
        );

        assert_eq!(scores.len(), 3);
        assert!(scores[0].total >= 0.80);
        assert!(scores[1].read_confirm >= 0.80);
        assert!(scores[1]
            .suggested_commands
            .first()
            .is_some_and(|cmd| cmd.contains("read_file")));
        assert!(scores[2]
            .suggested_commands
            .iter()
            .any(|cmd| cmd.contains("read_file")));
    }

    #[test]
    fn build_read_only_completion_hint_promotes_done_after_strong_observation() {
        let plan = PlanBlock {
            goal: "Locate the /realize slash command handler in the TUI".to_string(),
            steps: vec![
                "search src".to_string(),
                "read the matching file".to_string(),
                "confirm the context".to_string(),
            ],
            acceptance_criteria: vec![
                "The exact file path containing the `/realize` slash command handler is identified."
                    .to_string(),
                "The handler logic is confirmed to be part of the TUI component.".to_string(),
            ],
            risks: "wrong file".to_string(),
            assumptions: "repo indexed".to_string(),
        };
        let messages = vec![
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_search",
                    "type": "function",
                    "function": {
                        "name": "search_files",
                        "arguments": "{\"pattern\":\"/realize\",\"dir\":\"src\"}"
                    }
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_search",
                "content": "[search_files: '/realize' — 1 match(es)]\nsrc/tui/events.rs:465:         \"/realize\" => {"
            }),
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_read",
                    "type": "function",
                    "function": {
                        "name": "read_file",
                        "arguments": "{\"path\":\"src/tui/events.rs\"}"
                    }
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_read",
                "content": "[src/tui/events.rs] (2918 lines, 108025 bytes)\nfn handle_slash_command(text: &str, app: &mut App, pane: PaneId) -> bool {\n    match cmd_lc.as_str() {\n        \"/realize\" => {"
            }),
        ];

        let evidence = collect_observation_evidence(&messages);
        let hint = build_read_only_completion_hint(
            "Locate where the /realize slash command is handled in the TUI. Do not edit anything.",
            &plan,
            &evidence,
            &messages,
            &WorkingMemory::default(),
        )
        .expect("completion hint");

        assert!(hint.contains("call done directly now"));
        assert!(hint.contains("tool: done"));
        assert!(hint.contains("src/tui/events.rs") || hint.contains("read_file"));
    }

    #[test]
    fn build_read_only_search_to_read_hint_prefers_best_src_tui_candidate() {
        let plan = PlanBlock {
            goal: "Locate the /realize slash command handler in the TUI".to_string(),
            steps: vec![
                "search src".to_string(),
                "read the matching file".to_string(),
                "confirm the context".to_string(),
            ],
            acceptance_criteria: vec![
                "The exact file path containing the `/realize` slash command handler is identified."
                    .to_string(),
                "The handler logic is confirmed to be part of the TUI component.".to_string(),
            ],
            risks: "wrong file".to_string(),
            assumptions: "repo indexed".to_string(),
        };
        let evidence = ObservationEvidence {
            searches: vec![ObservationSearchEvidence {
                command: "search_files(pattern=/realize, dir=src)".to_string(),
                pattern: "/realize".to_string(),
                hit_count: 4,
                paths: vec![
                    "README.md".to_string(),
                    "src/runtime_eval.rs".to_string(),
                    "src/tui/events.rs".to_string(),
                    "src/tui/agent.rs".to_string(),
                ],
            }],
            reads: vec![],
            resolutions: Vec::new(),
        };

        let hint = build_read_only_search_to_read_hint(
            "Locate where the /realize slash command is handled in the TUI. Do not edit anything. Final answer must include the file path.",
            &plan,
            &evidence.searches[0].paths,
            &evidence,
        )
        .expect("search-to-read hint");

        assert!(hint.contains("read_file(path=\"src/tui/events.rs\")"));
        assert!(hint.contains("plausible code candidate"));
    }

    #[test]
    fn best_read_only_followup_read_path_prefers_events_over_ui_for_slash_handler() {
        let plan = PlanBlock {
            goal: "Locate where `/realize` is handled in the TUI".to_string(),
            steps: vec![
                "search src".to_string(),
                "read the matching file".to_string(),
                "confirm the handler context".to_string(),
            ],
            acceptance_criteria: vec![
                "The exact file path containing the `/realize` slash command handler is identified."
                    .to_string(),
                "The handler branch is confirmed by read_file.".to_string(),
            ],
            risks: "wrong file".to_string(),
            assumptions: "repo indexed".to_string(),
        };
        let best = best_read_only_followup_read_path(
            "Locate where the /realize slash command is handled in the TUI. Do not edit anything.",
            &plan,
            &[
                "src/tui/ui.rs".to_string(),
                "src/tui/events.rs".to_string(),
                "src/runtime_eval.rs".to_string(),
            ],
            &ObservationEvidence::default(),
        )
        .expect("best follow-up path");

        assert_eq!(best, "src/tui/events.rs");
    }

    #[test]
    fn best_read_only_followup_read_path_prefers_prefs_rs_for_prefs_task() {
        let plan = synthetic_read_only_observation_plan(
            "Find where pane-scoped TUI preferences are serialized and restored. Do not edit anything.",
        );
        let best = best_read_only_followup_read_path(
            "Find where pane-scoped TUI preferences are serialized and restored. Do not edit anything.",
            &plan,
            &[
                "src/tui/events.rs".to_string(),
                "src/tui/ui.rs".to_string(),
                "src/tui/app.rs".to_string(),
            ],
            &ObservationEvidence::default(),
        )
        .expect("best follow-up path");

        assert_eq!(best, "src/tui/prefs.rs");
    }

    #[test]
    fn best_read_only_followup_read_path_prefers_agent_rs_for_repo_map_flow_task() {
        let plan = synthetic_read_only_observation_plan(
            "Find where the coder-side repo-map fallback for read_file misses is wired into the TUI agent flow. Do not edit anything.",
        );
        let best = best_read_only_followup_read_path(
            "Find where the coder-side repo-map fallback for read_file misses is wired into the TUI agent flow. Do not edit anything.",
            &plan,
            &[
                "src/runtime_eval.rs".to_string(),
                "src/agent_session.rs".to_string(),
                "src/repo_map.rs".to_string(),
            ],
            &ObservationEvidence::default(),
        )
        .expect("best follow-up path");

        assert_eq!(best, "src/tui/agent.rs");
    }

    #[test]
    fn build_read_only_diagnose_coercion_hint_starts_with_search_when_no_observation_exists() {
        let hint = build_read_only_diagnose_coercion_hint(
            "Locate where the /realize slash command is handled in the TUI. Do not edit anything.",
            None,
            &ObservationEvidence::default(),
            &[],
            &WorkingMemory::default(),
        )
        .expect("diagnose coercion hint");

        assert!(hint.contains("search_files(pattern=\"/realize\", dir=\"src/tui\")"));
    }

    #[test]
    fn first_action_constraint_hint_prefers_observation_tool_for_read_only() {
        assert_eq!(first_action_deadline_iters(true, true), 2);
        let hint = build_first_action_constraint_hint(
            "Locate where the /realize slash command is handled in the TUI. Do not edit anything.",
            true,
            true,
        )
        .expect("hint");
        assert!(hint.contains("[First Action Constraint]"));
        assert!(hint.contains("search_files(pattern=\"/realize\", dir=\"src/tui\")"));
    }

    #[test]
    fn first_action_constraint_hint_requires_tool_for_action_tasks() {
        assert_eq!(first_action_deadline_iters(false, true), 2);
        let hint = build_first_action_constraint_hint(
            "Fix the /realize command handling and keep tests passing.",
            false,
            true,
        )
        .expect("hint");
        assert!(hint.contains("Within the first 2 turns"));
        assert!(hint.contains("call ONE real tool"));
    }

    #[test]
    fn build_read_only_diagnose_coercion_hint_switches_to_read_after_search() {
        let plan = PlanBlock {
            goal: "Locate the /realize slash command handler in the TUI".to_string(),
            steps: vec![
                "search src".to_string(),
                "read the matching file".to_string(),
                "confirm the context".to_string(),
            ],
            acceptance_criteria: vec![
                "The exact file path containing the `/realize` slash command handler is identified."
                    .to_string(),
                "The handler logic is confirmed to be part of the TUI component.".to_string(),
            ],
            risks: "wrong file".to_string(),
            assumptions: "repo indexed".to_string(),
        };
        let evidence = ObservationEvidence {
            searches: vec![ObservationSearchEvidence {
                command: "search_files(pattern=/realize, dir=src)".to_string(),
                pattern: "/realize".to_string(),
                hit_count: 1,
                paths: vec!["src/tui/events.rs".to_string()],
            }],
            reads: vec![],
            resolutions: Vec::new(),
        };

        let hint = build_read_only_diagnose_coercion_hint(
            "Locate where the /realize slash command is handled in the TUI. Do not edit anything.",
            Some(&plan),
            &evidence,
            &[],
            &WorkingMemory::default(),
        )
        .expect("diagnose coercion hint");

        assert!(hint.contains("read_file(path=\"src/tui/events.rs\")"));
    }

    #[test]
    fn choose_read_only_diagnose_rescue_action_starts_with_search() {
        let action = choose_read_only_diagnose_rescue_action(
            "Locate where the /realize slash command is handled in the TUI. Do not edit anything.",
            None,
            &ObservationEvidence::default(),
        );

        assert_eq!(
            action,
            Some(ReadOnlyDiagnoseRescueAction::Search {
                pattern: "/realize".to_string(),
                dir: "src/tui".to_string(),
            })
        );
    }

    #[test]
    fn choose_read_only_diagnose_rescue_action_advances_to_read() {
        let plan = PlanBlock {
            goal: "Locate the /realize slash command handler in the TUI".to_string(),
            steps: vec![
                "search src".to_string(),
                "read the matching file".to_string(),
                "confirm the context".to_string(),
            ],
            acceptance_criteria: vec![
                "The exact file path containing the `/realize` slash command handler is identified."
                    .to_string(),
                "The handler logic is confirmed to be part of the TUI component.".to_string(),
            ],
            risks: "wrong file".to_string(),
            assumptions: "repo indexed".to_string(),
        };
        let evidence = ObservationEvidence {
            searches: vec![ObservationSearchEvidence {
                command: "search_files(pattern=/realize, dir=src)".to_string(),
                pattern: "/realize".to_string(),
                hit_count: 1,
                paths: vec!["src/tui/events.rs".to_string()],
            }],
            reads: vec![],
            resolutions: Vec::new(),
        };

        let action = choose_read_only_diagnose_rescue_action(
            "Locate where the /realize slash command is handled in the TUI. Do not edit anything.",
            Some(&plan),
            &evidence,
        );

        assert_eq!(
            action,
            Some(ReadOnlyDiagnoseRescueAction::Read {
                path: "src/tui/events.rs".to_string(),
            })
        );
    }

    #[test]
    fn build_read_only_iteration_cap_final_answer_includes_path() {
        let plan = PlanBlock {
            goal: "Locate the /realize slash command handler in the TUI".to_string(),
            steps: vec![
                "search src".to_string(),
                "read the matching file".to_string(),
                "confirm the context".to_string(),
            ],
            acceptance_criteria: vec![
                "The exact file path containing the `/realize` slash command handler is identified."
                    .to_string(),
                "The handler logic is confirmed to be part of the TUI component.".to_string(),
            ],
            risks: "wrong file".to_string(),
            assumptions: "repo indexed".to_string(),
        };
        let messages = vec![
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_search",
                    "type": "function",
                    "function": {
                        "name": "search_files",
                        "arguments": "{\"pattern\":\"/realize\",\"dir\":\"src\"}"
                    }
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_search",
                "content": "[search_files: '/realize' — 1 match(es)]\nsrc/tui/events.rs:465:         \"/realize\" => {"
            }),
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_read",
                    "type": "function",
                    "function": {
                        "name": "read_file",
                        "arguments": "{\"path\":\"src/tui/events.rs\"}"
                    }
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_read",
                "content": "[src/tui/events.rs] (2918 lines, 108025 bytes)\nfn handle_slash_command(text: &str, app: &mut App, pane: PaneId) -> bool {\n    match cmd_lc.as_str() {\n        \"/realize\" => {"
            }),
        ];

        let evidence = collect_observation_evidence(&messages);
        let final_text = build_read_only_iteration_cap_final_answer(
            "Locate where the /realize slash command is handled in the TUI. Do not edit anything. Final answer must include the file path.",
            &plan,
            &evidence,
            &messages,
            &WorkingMemory::default(),
        )
        .expect("final answer");

        assert!(final_text.starts_with("[DONE]"));
        assert!(final_text.contains("src/tui/events.rs"));
        assert!(final_text.contains("acceptance 1"));
    }

    #[test]
    fn maybe_build_read_only_auto_final_answer_reuses_read_only_evidence() {
        let plan = PlanBlock {
            goal: "Locate the /realize slash command handler in the TUI".to_string(),
            steps: vec![
                "search src".to_string(),
                "read the matching file".to_string(),
                "confirm the context".to_string(),
            ],
            acceptance_criteria: vec![
                "The exact file path containing the `/realize` slash command handler is identified."
                    .to_string(),
                "The handler logic is confirmed to be part of the TUI component.".to_string(),
            ],
            risks: "wrong file".to_string(),
            assumptions: "repo indexed".to_string(),
        };
        let messages = vec![
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_search",
                    "type": "function",
                    "function": {
                        "name": "search_files",
                        "arguments": "{\"pattern\":\"/realize\",\"dir\":\"src\"}"
                    }
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_search",
                "content": "[search_files: '/realize' — 1 match(es)]\nsrc/tui/events.rs:465:         \"/realize\" => {"
            }),
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_read",
                    "type": "function",
                    "function": {
                        "name": "read_file",
                        "arguments": "{\"path\":\"src/tui/events.rs\"}"
                    }
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_read",
                "content": "[src/tui/events.rs] (2918 lines, 108025 bytes)\nfn handle_slash_command(text: &str, app: &mut App, pane: PaneId) -> bool {\n    match cmd_lc.as_str() {\n        \"/realize\" => {"
            }),
        ];
        let evidence = collect_observation_evidence(&messages);

        let final_text = maybe_build_read_only_auto_final_answer(
            true,
            "Locate where the /realize slash command is handled in the TUI. Do not edit anything. Final answer must include the file path.",
            Some(&plan),
            &evidence,
            &messages,
            &WorkingMemory::default(),
        )
        .expect("auto final answer");

        assert!(final_text.starts_with("[DONE]"));
        assert!(final_text.contains("src/tui/events.rs"));
        assert!(final_text.contains("via `read_file(path=src/tui/events.rs)`"));
    }

    #[test]
    fn build_read_only_strong_final_answer_requires_strong_read_backed_evidence() {
        let plan = PlanBlock {
            goal: "Locate the /realize slash command handler in the TUI".to_string(),
            steps: vec![
                "search src".to_string(),
                "read the matching file".to_string(),
                "confirm the context".to_string(),
            ],
            acceptance_criteria: vec![
                "The exact file path containing the `/realize` slash command handler is identified."
                    .to_string(),
                "The handler logic is confirmed to be part of the TUI component.".to_string(),
            ],
            risks: "wrong file".to_string(),
            assumptions: "repo indexed".to_string(),
        };
        let strong_messages = vec![
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_search",
                    "type": "function",
                    "function": {
                        "name": "search_files",
                        "arguments": "{\"pattern\":\"/realize\",\"dir\":\"src/tui\"}"
                    }
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_search",
                "content": "[search_files: '/realize' — 2 match(es)]\nsrc/tui/events.rs:763: \"/realize\" => {\nsrc/tui/ui.rs:401: \"/realize\""
            }),
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_read",
                    "type": "function",
                    "function": {
                        "name": "read_file",
                        "arguments": "{\"path\":\"src/tui/events.rs\"}"
                    }
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_read",
                "content": "[src/tui/events.rs] (3639 lines, 133416 bytes)\nfn handle_slash_command(text: &str, app: &mut App, pane: PaneId) -> bool {\n    match cmd_lc.as_str() {\n        \"/realize\" => {"
            }),
        ];
        let strong_evidence = collect_observation_evidence(&strong_messages);
        let final_text = build_read_only_strong_final_answer(
            "Locate where the /realize slash command is handled in the TUI. Do not edit anything.",
            &plan,
            &strong_evidence,
            &strong_messages,
            &WorkingMemory::default(),
        )
        .expect("strong final answer");
        assert!(final_text.contains("src/tui/events.rs"));

        let weak_messages = vec![
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_search",
                    "type": "function",
                    "function": {
                        "name": "search_files",
                        "arguments": "{\"pattern\":\"realize\"}"
                    }
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_search",
                "content": "[search_files: 'realize' — 50 match(es)]\nREADME.md:511: realize\nsrc/tui/events.rs:763: \"/realize\" => {\nsrc/tui/ui.rs:401: \"/realize\""
            }),
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_read",
                    "type": "function",
                    "function": {
                        "name": "read_file",
                        "arguments": "{\"path\":\"src/tui/ui.rs\"}"
                    }
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_read",
                "content": "[src/tui/ui.rs] (1861 lines, 67793 bytes)\npub fn render(frame: &mut Frame, app: &App) {"
            }),
        ];
        let weak_evidence = collect_observation_evidence(&weak_messages);
        assert!(build_read_only_strong_final_answer(
            "Locate where the /realize slash command is handled in the TUI. Do not edit anything.",
            &plan,
            &weak_evidence,
            &weak_messages,
            &WorkingMemory::default(),
        )
        .is_none());
    }

    #[test]
    fn build_read_only_strong_final_answer_supports_pane_prefs_task() {
        let plan = synthetic_read_only_observation_plan(
            "Find where pane-scoped TUI preferences are serialized and restored. Do not edit anything.",
        );
        let messages = vec![
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_search",
                    "type": "function",
                    "function": {
                        "name": "search_files",
                        "arguments": "{\"pattern\":\"prefs\",\"dir\":\"src/tui\"}"
                    }
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_search",
                "content": "[search_files: 'prefs' — 2 match(es)]\nsrc/tui/events.rs:124: let saved = prefs::snapshot_app_prefs(app);\nsrc/tui/prefs.rs:183: pub fn snapshot_app_prefs(app: &App) -> TuiPrefs {"
            }),
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_read",
                    "type": "function",
                    "function": {
                        "name": "read_file",
                        "arguments": "{\"path\":\"src/tui/prefs.rs\"}"
                    }
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_read",
                "content": "[src/tui/prefs.rs] (327 lines, 10301 bytes)\npub fn snapshot_app_prefs(app: &App) -> TuiPrefs {\npub fn apply_prefs_to_app(app: &mut App, prefs: &TuiPrefs) {"
            }),
        ];
        let evidence = collect_observation_evidence(&messages);
        let final_text = build_read_only_strong_final_answer(
            "Find where pane-scoped TUI preferences are serialized and restored. Do not edit anything.",
            &plan,
            &evidence,
            &messages,
            &WorkingMemory::default(),
        )
        .expect("strong final answer");

        assert!(final_text.contains("src/tui/prefs.rs"));
        assert!(final_text.contains("read_file(path=src/tui/prefs.rs)"));
    }

    #[test]
    fn maybe_build_read_only_auto_final_answer_falls_back_to_synthetic_plan() {
        let messages = vec![
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_search",
                    "type": "function",
                    "function": {
                        "name": "search_files",
                        "arguments": "{\"pattern\":\"/realize\",\"dir\":\"src/tui\"}"
                    }
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_search",
                "content": "[search_files: '/realize' — 3 match(es)]\nevents.rs:763: \"/realize\" => {\nintent.rs:519: raw_user_prompt: \"Find /realize handler\"\nagent.rs:100: something"
            }),
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_read",
                    "type": "function",
                    "function": {
                        "name": "read_file",
                        "arguments": "{\"path\":\"src/tui/events.rs\"}"
                    }
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_read",
                "content": "[src/tui/events.rs] (3639 lines, 133416 bytes)\nfn handle_slash_command(text: &str, app: &mut App, pane: PaneId) -> bool {\n    match cmd_lc.as_str() {\n        \"/realize\" => {"
            }),
        ];
        let evidence = collect_observation_evidence(&messages);

        let final_text = maybe_build_read_only_auto_final_answer(
            true,
            "Locate where the /realize slash command is handled in the TUI. Do not edit anything. Final answer must include the file path.",
            None,
            &evidence,
            &messages,
            &WorkingMemory::default(),
        )
        .expect("auto final answer with synthetic plan");

        assert!(final_text.starts_with("[DONE]"));
        assert!(final_text.contains("src/tui/events.rs"));
    }

    #[test]
    fn maybe_build_verified_action_closeout_text_finalizes_pygame_repo_after_verify() {
        let plan = PlanBlock {
            goal: "Create a small pygame maze game repo and verify it.".to_string(),
            steps: vec![
                "initialize the repo".to_string(),
                "write the pygame gameplay files".to_string(),
                "run the unittest smoke".to_string(),
                "call done with verification evidence".to_string(),
            ],
            acceptance_criteria: vec![
                "maze_game_pygame/README.md explains the controls".to_string(),
                "maze_game_pygame/game.py contains the maze gameplay logic".to_string(),
                "maze_game_pygame/main.py launches the game loop".to_string(),
                "maze_game_pygame/test_game.py passes under SDL_VIDEODRIVER=dummy python3 -m unittest -q 2>&1".to_string(),
            ],
            risks: "wrong repo layout".to_string(),
            assumptions: "pygame is available".to_string(),
        };
        let verify_command = "test -d maze_game_pygame/.git && test -f maze_game_pygame/README.md && test -f maze_game_pygame/game.py && test -f maze_game_pygame/main.py && test -f maze_game_pygame/test_game.py && cd maze_game_pygame && SDL_VIDEODRIVER=dummy python3 -m unittest -q 2>&1";
        let messages = vec![
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_game",
                    "type": "function",
                    "function": {
                        "name": "write_file",
                        "arguments": "{\"path\":\"maze_game_pygame/game.py\",\"content\":\"class MazeGame:\\n    pass\\n\"}"
                    }
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_game",
                "content": "OK: wrote 'maze_game_pygame/game.py' (2 lines, 25 bytes)"
            }),
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_main",
                    "type": "function",
                    "function": {
                        "name": "write_file",
                        "arguments": "{\"path\":\"maze_game_pygame/main.py\",\"content\":\"from game import MazeGame\\n\\nif __name__ == \\\"__main__\\\":\\n    MazeGame().run()\\n\"}"
                    }
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_main",
                "content": "OK: wrote 'maze_game_pygame/main.py' (4 lines, 67 bytes)"
            }),
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_verify",
                    "type": "function",
                    "function": {
                        "name": "exec",
                        "arguments": format!("{{\"command\":{}}}", serde_json::to_string(verify_command).expect("json"))
                    }
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_verify",
                "content": "OK (exit_code: 0)\nstdout:\nRan 3 tests in 0.002s\n\nOK"
            }),
        ];
        let mut working_mem = WorkingMemory::default();
        working_mem.remember_successful_verification(verify_command);

        let final_text = maybe_build_verified_action_closeout_text(
            "Create a pygame maze game repo.",
            Some(&plan),
            &messages,
            &working_mem,
            &ObservationEvidence::default(),
            VerificationLevel::Behavioral,
            Some("SDL_VIDEODRIVER=dummy python3 -m unittest -q 2>&1"),
            Some(7),
            Some(7),
        )
        .expect("auto final answer");

        assert!(final_text.starts_with("[DONE]"));
        assert!(final_text.contains("maze_game_pygame/game.py"));
        assert!(final_text.contains("maze_game_pygame/main.py"));
        assert!(final_text.contains("SDL_VIDEODRIVER=dummy python3 -m unittest -q 2>&1"));

        let explicit_handoff = maybe_build_verified_action_closeout_text(
            "Create a pygame maze game repo. Final answer must include verification_receipt.txt and fresh exec proof.",
            Some(&plan),
            &messages,
            &working_mem,
            &ObservationEvidence::default(),
            VerificationLevel::Behavioral,
            Some("SDL_VIDEODRIVER=dummy python3 -m unittest -q 2>&1"),
            Some(7),
            Some(7),
        );
        assert!(
            explicit_handoff.is_none(),
            "an explicit handoff must reach the model's done summary"
        );
    }

    #[test]
    fn maybe_build_verified_action_closeout_text_waits_for_required_followups() {
        let plan = PlanBlock {
            goal: "Fix the PR-ready merge approval handoff.".to_string(),
            steps: vec![
                "inspect merge approval code".to_string(),
                "patch the smallest bug".to_string(),
                "verify the configured smoke".to_string(),
                "call done with handoff paths".to_string(),
            ],
            acceptance_criteria: vec![
                "the requested change is implemented".to_string(),
                "the verification command passes".to_string(),
            ],
            risks: "missing follow-up paths".to_string(),
            assumptions: "configured smoke is relevant".to_string(),
        };
        let messages = vec![
            json!({
                "role":"assistant",
                "tool_calls":[{
                    "id":"call_patch_merge",
                    "type":"function",
                    "function":{"name":"patch_file","arguments":"{\"path\":\"src/tui/agent/merge_approval.rs\",\"search\":\"old\",\"replace\":\"new\"}"}
                }]
            }),
            json!({
                "role":"tool",
                "tool_call_id":"call_patch_merge",
                "content":"OK: patched 'src/tui/agent/merge_approval.rs' (+1 lines, 99 total)\n[auto-test] ✓ PASSED (exit 0)"
            }),
        ];
        let test_cmd =
            "cargo test -q tui::agent::merge_approval::tests:: 2>&1 && bash scripts/pr-ready-smoke.sh";

        let final_text = maybe_build_verified_action_closeout_text(
            "Update `docs/state-schema.md` and `.spiral-coder/runtime_eval.json` to include `src/tui/agent/merge_approval.rs`.",
            Some(&plan),
            &messages,
            &WorkingMemory::default(),
            &ObservationEvidence::default(),
            VerificationLevel::Behavioral,
            Some(test_cmd),
            Some(1),
            Some(1),
        );

        assert!(final_text.is_none());
    }

    #[test]
    fn root_read_only_detection_ignores_plan_drift_verbs() {
        assert!(is_root_read_only_observation_task(
            "Locate where the /realize slash command is handled in the TUI. Do not edit anything."
        ));
        assert!(!is_root_read_only_observation_task(
            "Fix the /realize command and update the handler implementation."
        ));
    }

    #[test]
    fn first_slash_literal_extracts_command_token() {
        assert_eq!(
            first_slash_literal("Locate where the `/realize` slash command is handled in the TUI."),
            Some("/realize".to_string())
        );
    }

    #[test]
    fn build_mistral_think_only_hint_suggests_real_tool() {
        let think = ThinkBlock {
            goal: "find direct references".to_string(),
            step: 1,
            tool: "search_files".to_string(),
            risk: "dynamic dispatch".to_string(),
            doubt: "might be indirect".to_string(),
            next: "search for /realize".to_string(),
            verify: "see a TUI match".to_string(),
        };
        let hint = build_mistral_think_only_hint(
            "Locate where the /realize slash command is handled in the TUI.",
            &think,
        );
        assert!(hint.contains("think` is not a real tool call"));
        assert!(hint.contains("search_files(pattern=\"/realize\", dir=\"src/tui\")"));
    }

    #[test]
    fn parse_think_block_accepts_bracketed_mistral_fields() {
        let text = "<think>\ntruegoal[\"confirm /realize handler context\"]step[\"2: inspect matching file\"]tool[\"read_file\"]risk[\"false positive match\"]doubt[\"handler may be elsewhere\"]next[\"read src/tui/events.rs\"]verify[\"see the match arm\"]</think>";
        let think = parse_think_block(text).expect("think block");
        assert_eq!(think.goal, "confirm /realize handler context");
        assert_eq!(think.step, 2);
        assert_eq!(think.tool, "read_file");
        assert_eq!(think.next, "read src/tui/events.rs");
    }

    #[test]
    fn build_read_only_plan_rewrite_hint_is_inspect_only() {
        let hint = build_read_only_plan_rewrite_hint(
            "Locate where the /realize slash command is handled in the TUI. Do not edit anything.",
        );
        assert!(hint.contains("search_files(pattern=\"/realize\", dir=\"src/tui\")"));
        assert!(hint.contains("call done once the file path and code context are confirmed"));
        assert!(hint.contains("Do NOT mention cargo test"));
        assert!(hint.contains("strictly inspect-only plan"));
    }

    #[test]
    fn build_read_only_plan_rewrite_hint_uses_prefs_specific_search() {
        let hint = build_read_only_plan_rewrite_hint(
            "Find where pane-scoped TUI preferences are serialized and restored. Do not edit anything.",
        );
        assert!(hint.contains("goal: Locate the main file where pane-scoped TUI preferences are serialized and restored."));
        assert!(hint.contains("search_files(pattern=\"prefs\", dir=\"src/tui\")"));
        assert!(hint.contains("search_files(pattern=\"save_tui_prefs\", dir=\"src/tui\")"));
        assert!(hint.contains("serialize and restore context is confirmed by read_file"));
    }

    #[test]
    fn synthetic_read_only_observation_plan_prefers_agent_flow_search_terms() {
        let plan = synthetic_read_only_observation_plan(
            "Find where the coder-side repo-map fallback for read_file misses is wired into the TUI agent flow. Do not edit anything.",
        );
        assert!(plan.goal.contains("repo-map fallback"));
        assert!(plan
            .steps
            .iter()
            .any(|step| step
                .contains("search_files(pattern=\"lazy_read_fallback\", dir=\"src/tui\")")));
        assert!(plan
            .steps
            .iter()
            .any(|step| step.contains("search_files(pattern=\"repo_map\", dir=\"src/tui\")")));
        assert!(plan
            .acceptance_criteria
            .iter()
            .any(|item| item.contains("read_file miss handling context")));
    }

    #[test]
    fn preferred_read_only_search_pattern_ignores_instruction_stopwords() {
        let pattern = preferred_read_only_search_pattern(
            "Find the failing test and explain why it fails. Do not edit anything. Final answer must include the file path and the exact mismatch.",
        );
        assert_eq!(pattern, "test");
    }

    #[test]
    fn synthetic_read_only_observation_plan_captures_failure_mismatch_tasks() {
        let plan = synthetic_read_only_observation_plan(
            "Find the failing test and explain why it fails. Do not edit anything. Final answer must include the file path and the exact mismatch.",
        );
        assert!(plan.goal.contains("failing test"));
        assert!(plan.goal.contains("assertion mismatch"));
        assert!(plan
            .acceptance_criteria
            .iter()
            .any(|item| item.contains("failing test file path")));
        assert!(plan
            .acceptance_criteria
            .iter()
            .any(|item| item.contains("exact assertion mismatch")));
    }

    #[test]
    fn compact_success_tool_result_for_history_keeps_exec_signal() {
        let content = "OK (exit_code: 0)\nduration_ms: 150\ncwd: /tmp/demo\nstdout:\nline 1\nline 2\nline 3\nline 4\nline 5\nline 6\nline 7\nline 8\nline 9\n[auto-test] ✓ PASSED (exit 0)\nfinished";
        let compact = compact_success_tool_result_for_history("exec", content);
        assert!(compact.contains("OK (exit_code: 0)"));
        assert!(compact.contains("[history digest"));
        assert!(compact.contains("[auto-test]"));
    }

    #[test]
    fn validate_plan_for_task_rejects_behavioral_acceptance_for_read_only_tasks() {
        let plan = PlanBlock {
            goal: "locate slash command handler".to_string(),
            steps: vec![
                "search src".to_string(),
                "read src/tui/events.rs".to_string(),
            ],
            acceptance_criteria: vec![
                "The exact file path is identified".to_string(),
                "A behavioral test must confirm the /realize command works in the TUI.".to_string(),
            ],
            risks: "wrong file".to_string(),
            assumptions: "repo is local".to_string(),
        };

        assert!(validate_plan_for_task(&plan, true).is_err());
        assert!(validate_plan_for_task(&plan, false).is_ok());
    }

    #[test]
    fn validate_done_acceptance_accepts_read_only_shorthand_evidence() {
        let plan = PlanBlock {
            goal: "locate slash command".to_string(),
            steps: vec![
                "inspect src".to_string(),
                "read src/tui/events.rs".to_string(),
            ],
            acceptance_criteria: vec![
                "The exact file path is identified".to_string(),
                "The handler context is confirmed".to_string(),
            ],
            risks: "wrong file".to_string(),
            assumptions: "repo indexed".to_string(),
        };

        let completed_acceptance = vec!["acceptance 1".to_string(), "acceptance 2".to_string()];
        let remaining_acceptance = Vec::new();
        let acceptance_evidence = vec![
            DoneAcceptanceEvidence {
                criterion: "acceptance 1".to_string(),
                command: "grep -n \"/realize\" src/tui/events.rs".to_string(),
            },
            DoneAcceptanceEvidence {
                criterion: "acceptance 2".to_string(),
                command: "read_file src/tui/events.rs".to_string(),
            },
        ];
        let known_commands = vec![
            "search_files(dir=src, pattern=/realize)".to_string(),
            "read_file(path=src/tui/events.rs)".to_string(),
        ];

        assert!(validate_done_acceptance(
            Some(&plan),
            &completed_acceptance,
            &remaining_acceptance,
            &acceptance_evidence,
            &known_commands,
            &ObservationEvidence::default(),
        )
        .is_ok());
    }

    #[test]
    fn validate_done_acceptance_uses_resolution_memory_for_shorthand_paths() {
        let plan = PlanBlock {
            goal: "locate slash command".to_string(),
            steps: vec![
                "inspect src".to_string(),
                "read src/tui/events.rs".to_string(),
            ],
            acceptance_criteria: vec![
                "The exact file path is identified".to_string(),
                "The handler context is confirmed".to_string(),
            ],
            risks: "wrong file".to_string(),
            assumptions: "repo indexed".to_string(),
        };

        let completed_acceptance = vec!["acceptance 1".to_string(), "acceptance 2".to_string()];
        let remaining_acceptance = Vec::new();
        let acceptance_evidence = vec![
            DoneAcceptanceEvidence {
                criterion: "acceptance 1".to_string(),
                command: "grep -n \"/realize\" tui/events.rs".to_string(),
            },
            DoneAcceptanceEvidence {
                criterion: "acceptance 2".to_string(),
                command: "read_file(path=tui/events.rs)".to_string(),
            },
        ];
        let known_commands = vec![
            "search_files(dir=src/tui/events.rs, pattern=/realize)".to_string(),
            "read_file(path=src/tui/events.rs)".to_string(),
        ];
        let mut observation_evidence = ObservationEvidence::default();
        observation_evidence.remember_resolution(
            "tui/events.rs",
            "src/tui/events.rs",
            "repo_map:read_file",
        );

        assert!(validate_done_acceptance(
            Some(&plan),
            &completed_acceptance,
            &remaining_acceptance,
            &acceptance_evidence,
            &known_commands,
            &observation_evidence,
        )
        .is_ok());
    }

    #[test]
    fn build_read_only_completion_hint_canonicalizes_resolution_memory_paths() {
        let plan = PlanBlock {
            goal: "Locate the /realize slash command handler in the TUI".to_string(),
            steps: vec![
                "search src".to_string(),
                "read the matching file".to_string(),
                "confirm the context".to_string(),
            ],
            acceptance_criteria: vec![
                "The exact file path containing the `/realize` slash command handler is identified."
                    .to_string(),
                "The handler logic is confirmed to be part of the TUI component.".to_string(),
            ],
            risks: "wrong file".to_string(),
            assumptions: "repo indexed".to_string(),
        };
        let messages = vec![
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_search",
                    "type": "function",
                    "function": {
                        "name": "search_files",
                        "arguments": "{\"pattern\":\"/realize\",\"dir\":\"src\"}"
                    }
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_search",
                "content": "[search_files: '/realize' — 1 match(es)]\nsrc/tui/events.rs:465:         \"/realize\" => {"
            }),
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_read",
                    "type": "function",
                    "function": {
                        "name": "read_file",
                        "arguments": "{\"path\":\"tui/events.rs\"}"
                    }
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_read",
                "content": "[src/tui/events.rs] (2918 lines, 108025 bytes)\nfn handle_slash_command(text: &str, app: &mut App, pane: PaneId) -> bool {\n    match cmd_lc.as_str() {\n        \"/realize\" => {"
            }),
        ];

        let mut evidence = collect_observation_evidence(&messages);
        evidence.remember_resolution("tui/events.rs", "src/tui/events.rs", "repo_map:read_file");
        let hint = build_read_only_completion_hint(
            "Locate where the /realize slash command is handled in the TUI. Do not edit anything.",
            &plan,
            &evidence,
            &messages,
            &WorkingMemory::default(),
        )
        .expect("completion hint");

        assert!(hint.contains("read_file(path=src/tui/events.rs)"));
        assert!(!hint.contains("read_file(path=tui/events.rs)"));
    }

    #[test]
    fn parses_plan_block_steps() {
        let text = "\
<plan>\n\
goal: make tests pass\n\
steps: 1) inspect failing test 2) patch the bug 3) run cargo test\n\
acceptance: 1) cargo test passes 2) bug reproduction no longer fails\n\
risks: flaky test and wrong file\n\
assumptions: repo already builds\n\
</plan>\n";
        let plan = parse_plan_block(text).expect("plan parsed");
        assert_eq!(plan.goal, "make tests pass");
        assert_eq!(plan.steps.len(), 3);
        assert_eq!(plan.acceptance_criteria.len(), 2);
        assert_eq!(plan.steps[0], "inspect failing test");
        assert_eq!(plan.steps[2], "run cargo test");
    }

    #[test]
    fn parses_plan_block_with_colon_and_dash_numbering() {
        let text = "\
<plan>\n\
goal: harden governor parsing\n\
steps: 1: inspect parser drift 2- unify block parsing 3. run tests\n\
acceptance_criteria: 1: parser accepts contract aliases 2- tests pass\n\
risks: parser mismatch\n\
assumptions: shared contract is loaded\n\
</plan>\n";
        let plan = parse_plan_block(text).expect("plan parsed");
        assert_eq!(plan.steps.len(), 3);
        assert_eq!(plan.steps[1], "unify block parsing");
        assert_eq!(plan.acceptance_criteria.len(), 2);
        assert_eq!(
            plan.acceptance_criteria[0],
            "parser accepts contract aliases"
        );
    }

    #[test]
    fn parses_xml_style_plan_block_fields() {
        let text = "\
<plan>\n\
<goal>locate the slash command handler</goal>\n\
<steps>1) search src 2) read the matching file 3) verify the handler</steps>\n\
<acceptance>1) exact file path identified 2) handler block confirmed</acceptance>\n\
<risks>wrong file or command aliasing</risks>\n\
<assumptions>the TUI code is local</assumptions>\n\
</plan>\n";
        let plan = parse_plan_block(text).expect("plan parsed");
        assert_eq!(plan.goal, "locate the slash command handler");
        assert_eq!(plan.steps.len(), 3);
        assert_eq!(plan.acceptance_criteria.len(), 2);
        assert_eq!(plan.steps[0], "search src");
        assert_eq!(plan.acceptance_criteria[1], "handler block confirmed");
    }

    #[test]
    fn parses_plan_block_with_truncated_closing_tag() {
        let text = "\
<plan>\n\
goal: locate slash command\n\
steps: 1) search src 2) read the matching file\n\
acceptance: 1) path identified 2) handler confirmed\n\
risks: wrong file\n\
assumptions: repo indexed\n\
</plan";
        let plan = parse_plan_block(text).expect("plan parsed");
        assert_eq!(plan.goal, "locate slash command");
        assert_eq!(plan.steps.len(), 2);
        assert_eq!(plan.acceptance_criteria.len(), 2);
    }

    #[test]
    fn validates_plan_requires_numbered_steps() {
        let plan = PlanBlock {
            goal: "ship fix".to_string(),
            steps: vec!["only one step".to_string()],
            acceptance_criteria: vec!["tests pass".to_string()],
            risks: "wrong file".to_string(),
            assumptions: "tests exist".to_string(),
        };
        assert!(validate_plan(&plan).is_err());
    }

    #[test]
    fn validates_plan_requires_acceptance_criteria() {
        let plan = PlanBlock {
            goal: "ship fix".to_string(),
            steps: vec!["inspect".to_string(), "verify".to_string()],
            acceptance_criteria: vec![],
            risks: "wrong file".to_string(),
            assumptions: "tests exist".to_string(),
        };
        assert!(validate_plan(&plan).is_err());
    }

    #[test]
    fn verification_level_from_plan_uses_acceptance_criteria() {
        let docs_plan = PlanBlock {
            goal: "update docs".to_string(),
            steps: vec!["edit readme".to_string(), "verify formatting".to_string()],
            acceptance_criteria: vec![
                "README wording is updated".to_string(),
                "markdown renders cleanly".to_string(),
            ],
            risks: "wrong file".to_string(),
            assumptions: "docs build exists".to_string(),
        };
        let code_plan = PlanBlock {
            goal: "fix runtime bug".to_string(),
            steps: vec![
                "inspect".to_string(),
                "patch".to_string(),
                "verify".to_string(),
            ],
            acceptance_criteria: vec![
                "cargo test passes".to_string(),
                "runtime gate blocks repeated failure".to_string(),
            ],
            risks: "wrong file".to_string(),
            assumptions: "tests exist".to_string(),
        };

        assert_eq!(
            verification_level_from_plan(&docs_plan),
            Some(VerificationLevel::Build)
        );
        assert_eq!(
            verification_level_from_plan(&code_plan),
            Some(VerificationLevel::Behavioral)
        );
    }

    #[test]
    fn parses_think_block_fields() {
        let text = "\
<think>\n\
goal: verify the fix\n\
step: 3\n\
tool: exec\n\
risk: wrong test target\n\
doubt: maybe cargo check is enough\n\
next: cargo test\n\
verify: exit code is zero\n\
</think>\n";
        let think = parse_think_block(text).expect("think parsed");
        assert_eq!(think.step, 3);
        assert_eq!(think.tool, "exec");
        assert_eq!(think.next, "cargo test");
    }

    #[test]
    fn parses_xml_style_think_block_fields() {
        let text = "\
<think>\n\
<goal>find the realize handler</goal>\n\
<step>1 (search for the slash command)</step>\n\
<tool>search_files</tool>\n\
<risk>wrong path scope</risk>\n\
<doubt>could be registered indirectly</doubt>\n\
<next>search /realize inside src</next>\n\
<verify>match appears in a TUI file</verify>\n\
</think>\n";
        let think = parse_think_block(text).expect("think parsed");
        assert_eq!(think.step, 1);
        assert_eq!(think.tool, "search_files");
        assert_eq!(think.next, "search /realize inside src");
    }

    #[test]
    fn pseudo_plan_tool_call_converts_to_plan_block_text() {
        let tc = ToolCallData {
            id: "call_1".to_string(),
            name: "plan".to_string(),
            arguments: serde_json::json!({
                "goal": "locate handler",
                "steps": ["search code", "read file", "verify"],
                "acceptance": ["path identified", "handler confirmed"],
                "risks": "wrong file",
                "assumptions": "code is local"
            })
            .to_string(),
        };

        let text = pseudo_tool_call_to_block_text(&tc).expect("block text");
        let plan = parse_plan_block(&text).expect("plan parsed");
        assert_eq!(plan.goal, "locate handler");
        assert_eq!(plan.steps.len(), 3);
        assert_eq!(plan.acceptance_criteria.len(), 2);
    }

    #[test]
    fn pseudo_think_tool_call_converts_to_think_block_text() {
        let tc = ToolCallData {
            id: "call_1".to_string(),
            name: "think".to_string(),
            arguments: serde_json::json!({
                "goal": "inspect the source",
                "step": "2",
                "tool": "read_file",
                "risk": "wrong path",
                "doubt": "maybe router is elsewhere",
                "next": "read src/tui/events.rs",
                "verify": "command returns file contents"
            })
            .to_string(),
        };

        let text = pseudo_tool_call_to_block_text(&tc).expect("block text");
        let think = parse_think_block(&text).expect("think parsed");
        assert_eq!(think.step, 2);
        assert_eq!(think.tool, "read_file");
    }

    #[test]
    fn normalize_mistral_tool_call_extracts_sidecar_plan_from_arguments() {
        let plan = serde_json::json!({
            "goal": "locate handler",
            "steps": ["search code", "read file", "verify"],
            "acceptance": ["path identified"],
            "risks": "wrong file",
            "assumptions": "repo is local"
        })
        .to_string();
        let args = serde_json::json!({"pattern":"/realize","dir":"src"}).to_string();
        let tc = ToolCallData {
            id: "call_1".to_string(),
            name: "search_files".to_string(),
            arguments: format!("{plan}{args}"),
        };

        let (blocks, normalized) = normalize_mistral_tool_call(&tc);
        assert_eq!(blocks.len(), 1);
        let plan_block = parse_plan_block(&blocks[0]).expect("plan parsed");
        assert_eq!(plan_block.goal, "locate handler");
        let normalized = normalized.expect("real tool preserved");
        assert_eq!(normalized.name, "search_files");
        assert_eq!(
            normalized.arguments,
            serde_json::json!({"pattern":"/realize","dir":"src"}).to_string()
        );
    }

    #[test]
    fn normalize_mistral_tool_call_extracts_inline_plan_name_and_nested_tool() {
        let tc = ToolCallData {
            id: "call_1".to_string(),
            name: "plan>goal: locate handler steps: 1) search src 2) read file acceptance: 1) path identified risks: wrong file assumptions: repo is local </plan>".to_string(),
            arguments: serde_json::json!({
                "tool": "search_files",
                "arguments": {"pattern": "/realize", "dir": "src"}
            })
            .to_string(),
        };

        let (blocks, normalized) = normalize_mistral_tool_call(&tc);
        assert_eq!(blocks.len(), 1);
        let plan = parse_plan_block(&blocks[0]).expect("plan parsed");
        assert_eq!(plan.goal, "locate handler");
        let normalized = normalized.expect("real tool preserved");
        assert_eq!(normalized.name, "search_files");
        assert_eq!(
            normalized.arguments,
            serde_json::json!({"pattern":"/realize","dir":"src"}).to_string()
        );
    }

    #[test]
    fn normalize_mistral_tool_call_extracts_inline_plan_with_function_wrapper_arguments() {
        let tc = ToolCallData {
            id: "call_1".to_string(),
            name: "plan><goal>locate handler</goal><steps>1) search src 2) read file</steps><acceptance>1) path identified</acceptance><risks>wrong file</risks><assumptions>repo is local</assumptions></plan>".to_string(),
            arguments: serde_json::json!({
                "type": "function",
                "function": {
                    "name": "search_files",
                    "parameters": {"pattern": "/realize", "dir": "src"}
                }
            })
            .to_string(),
        };

        let (blocks, normalized) = normalize_mistral_tool_call(&tc);
        assert_eq!(blocks.len(), 1);
        let plan = parse_plan_block(&blocks[0]).expect("plan parsed");
        assert_eq!(plan.goal, "locate handler");
        let normalized = normalized.expect("real tool preserved");
        assert_eq!(normalized.name, "search_files");
        assert_eq!(
            normalized.arguments,
            serde_json::json!({"pattern":"/realize","dir":"src"}).to_string()
        );
    }

    #[test]
    fn normalize_mistral_tool_call_extracts_inline_plan_and_nested_think() {
        let tc = ToolCallData {
            id: "call_1".to_string(),
            name: "plan>goal: locate handler steps: 1) search src 2) read file acceptance: 1) path identified risks: wrong file assumptions: repo is local </plan>".to_string(),
            arguments: serde_json::json!({
                "think": "goal: inspect slash command\nstep: 1\ntool: search_files\nrisk: wrong path\ndoubt: maybe aliased\nnext: search /realize\nverify: find a match",
                "tool": "search_files",
                "arguments": {"pattern": "/realize", "dir": "src"}
            })
            .to_string(),
        };

        let (blocks, normalized) = normalize_mistral_tool_call(&tc);
        assert_eq!(blocks.len(), 2);
        let plan = parse_plan_block(&blocks[0]).expect("plan parsed");
        let think = parse_think_block(&blocks[1]).expect("think parsed");
        assert_eq!(plan.goal, "locate handler");
        assert_eq!(think.tool, "search_files");
        let normalized = normalized.expect("real tool preserved");
        assert_eq!(normalized.name, "search_files");
    }

    #[test]
    fn normalize_mistral_tool_call_extracts_markdownish_plan_name_and_wrapper_tool() {
        let tc = ToolCallData {
            id: "call_1".to_string(),
            name: "plan**goal:** Locate where the `/realize` slash command is handled in the TUI. **•**steps:** 1) Use `search_files` to search for the literal string `/realize` in `src`. 2) Read the matching file. **•**acceptance:** 1) The file path handling `/realize` is identified. 2) The handler branch is confirmed by `read_file`. **•**risks:** wrong file. **•**assumptions:** observation tools are sufficient. <thinking> **goal:** Find direct matches. **step:** 1. **tool:** `search_files`. **risk:** wrong path. **doubt:** maybe aliased. **next:** Search `/realize` in `src`. **verify:** confirm a TUI match. </thinking>".to_string(),
            arguments: serde_json::json!({
                "type": "function",
                "function": {
                    "name": "search_files",
                    "parameters": {"pattern": "/realize", "dir": "src"}
                }
            })
            .to_string(),
        };

        let (blocks, normalized) = normalize_mistral_tool_call(&tc);
        assert_eq!(blocks.len(), 1);
        let plan = parse_plan_block(&blocks[0]).expect("plan parsed");
        assert!(plan.goal.contains("/realize"));
        let normalized = normalized.expect("real tool preserved");
        assert_eq!(normalized.name, "search_files");
        assert_eq!(
            normalized.arguments,
            serde_json::json!({"pattern":"/realize","dir":"src"}).to_string()
        );
    }

    #[test]
    fn normalize_mistral_tool_call_extracts_embedded_plan_think_and_tool_name() {
        let tc = ToolCallData {
            id: "call_1".to_string(),
            name: "plan><goal>Locate the exact file and code context where the /realize slash command is handled in the TUI.</goal><steps>1) List the TUI-related directories to confirm structure and naming conventions.</steps><steps>2) Search for the literal \"/realize\" string across Rust files to pinpoint the handler.</steps><acceptance>1) The file path containing the /realize slash command handler is confirmed by search_files evidence.</acceptance><acceptance>2) The handling context is confirmed by read_file evidence.</acceptance><risks>wrong file</risks><assumptions>repo is local</assumptions></plan>teří<think><goal>Confirm TUI directory structure and naming.</goal><step>1</step><tool>list_dir</tool><risk>TUI directory may not exist.</risk><doubt>Directory names may vary.</doubt><next>list_dir src/</next><verify>Directory listing shows TUI directory.</verify></think>teřílist_dir".to_string(),
            arguments: serde_json::json!({"dir":"src"}).to_string(),
        };

        let (blocks, normalized) = normalize_mistral_tool_call(&tc);
        assert_eq!(blocks.len(), 2);
        let plan = parse_plan_block(&blocks[0]).expect("plan parsed");
        let think = parse_think_block(&blocks[1]).expect("think parsed");
        assert!(plan.goal.contains("/realize"));
        assert_eq!(think.tool, "list_dir");
        let normalized = normalized.expect("tool restored");
        assert_eq!(normalized.name, "list_dir");
        assert_eq!(
            normalized.arguments,
            serde_json::json!({"dir":"src"}).to_string()
        );
    }

    #[test]
    fn consecutive_missing_plan_blocks_for_tool_counts_same_observation_tool() {
        let tc = ToolCallData {
            id: "call_3".to_string(),
            name: "search_files".to_string(),
            arguments: serde_json::json!({"pattern":"/realize","dir":"src"}).to_string(),
        };
        let messages = vec![
            json!({
                "role": "assistant",
                "content": "",
                "tool_calls": [{
                    "id": "call_1",
                    "type": "function",
                    "function": {
                        "name": "search_files",
                        "arguments": serde_json::json!({"pattern":"/realize","dir":"src"}).to_string()
                    }
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_1",
                "content": "GOVERNOR BLOCKED\n\n[Plan Gate] Missing valid <plan>.\n\ntool:\nsearch_files\narguments:\n{\"pattern\":\"/realize\",\"dir\":\"src\"}"
            }),
            json!({
                "role": "assistant",
                "content": "",
                "tool_calls": [{
                    "id": "call_2",
                    "type": "function",
                    "function": {
                        "name": "search_files",
                        "arguments": serde_json::json!({"pattern":"/realize","dir":"src"}).to_string()
                    }
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_2",
                "content": "GOVERNOR BLOCKED\n\n[Plan Gate] Missing valid <plan>.\n\ntool:\nsearch_files\narguments:\n{\"pattern\":\"/realize\",\"dir\":\"src\"}"
            }),
        ];

        assert_eq!(consecutive_missing_plan_blocks_for_tool(&messages, &tc), 2);
    }

    #[test]
    fn consecutive_missing_plan_blocks_for_observation_counts_mixed_tools() {
        let messages = vec![
            json!({
                "role": "assistant",
                "content": "",
                "tool_calls": [{
                    "id": "call_1",
                    "type": "function",
                    "function": {
                        "name": "search_files",
                        "arguments": serde_json::json!({"pattern":"prefs","dir":"src/tui"}).to_string()
                    }
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_1",
                "content": "GOVERNOR BLOCKED\n\n[Plan Gate] Missing valid <plan>.\n\ntool:\nsearch_files\narguments:\n{\"pattern\":\"prefs\",\"dir\":\"src/tui\"}"
            }),
            json!({
                "role": "assistant",
                "content": "",
                "tool_calls": [{
                    "id": "call_2",
                    "type": "function",
                    "function": {
                        "name": "list_dir",
                        "arguments": serde_json::json!({"dir":"src/tui"}).to_string()
                    }
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_2",
                "content": "GOVERNOR BLOCKED\n\n[Plan Gate] Missing valid <plan>.\n\ntool:\nlist_dir\narguments:\n{\"dir\":\"src/tui\"}"
            }),
        ];

        assert_eq!(
            consecutive_missing_plan_blocks_for_observation(&messages),
            2
        );
    }

    #[test]
    fn rescue_read_only_missing_plan_for_tool_turn_after_repeated_blocks() {
        let tc = ToolCallData {
            id: "call_2".to_string(),
            name: "search_files".to_string(),
            arguments: serde_json::json!({"pattern":"/realize","dir":"src"}).to_string(),
        };
        let messages = vec![
            json!({
                "role": "assistant",
                "content": "",
                "tool_calls": [{
                    "id": "call_1",
                    "type": "function",
                    "function": {
                        "name": "search_files",
                        "arguments": serde_json::json!({"pattern":"/realize","dir":"src"}).to_string()
                    }
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_1",
                "content": "GOVERNOR BLOCKED\n\n[Plan Gate] Missing valid <plan>.\n\ntool:\nsearch_files\narguments:\n{\"pattern\":\"/realize\",\"dir\":\"src\"}"
            }),
        ];

        let rescued = rescue_read_only_missing_plan_for_tool_turn(
            &messages,
            &tc,
            "Locate where the /realize slash command is handled in the TUI. Do not edit anything.",
            true,
            ProviderKind::OpenAiCompatible,
        )
        .expect("rescue plan");
        assert!(rescued.goal.contains("/realize"));
        assert!(rescued
            .acceptance_criteria
            .iter()
            .any(|item| item.contains("handler branch")));
    }

    #[test]
    fn rescue_read_only_missing_plan_for_tool_turn_after_mixed_observation_blocks() {
        let tc = ToolCallData {
            id: "call_3".to_string(),
            name: "search_files".to_string(),
            arguments: serde_json::json!({"pattern":"prefs","dir":"src/tui"}).to_string(),
        };
        let messages = vec![
            json!({
                "role": "assistant",
                "content": "",
                "tool_calls": [{
                    "id": "call_1",
                    "type": "function",
                    "function": {
                        "name": "search_files",
                        "arguments": serde_json::json!({"pattern":"prefs","dir":"src/tui"}).to_string()
                    }
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_1",
                "content": "GOVERNOR BLOCKED\n\n[Plan Gate] Missing valid <plan>.\n\ntool:\nsearch_files\narguments:\n{\"pattern\":\"prefs\",\"dir\":\"src/tui\"}"
            }),
            json!({
                "role": "assistant",
                "content": "",
                "tool_calls": [{
                    "id": "call_2",
                    "type": "function",
                    "function": {
                        "name": "list_dir",
                        "arguments": serde_json::json!({"dir":"src/tui"}).to_string()
                    }
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_2",
                "content": "GOVERNOR BLOCKED\n\n[Plan Gate] Missing valid <plan>.\n\ntool:\nlist_dir\narguments:\n{\"dir\":\"src/tui\"}"
            }),
        ];

        let rescued = rescue_read_only_missing_plan_for_tool_turn(
            &messages,
            &tc,
            "Find where pane-scoped TUI preferences are serialized and restored. Do not edit anything.",
            true,
            ProviderKind::OpenAiCompatible,
        )
        .expect("rescue plan");
        assert!(rescued.goal.contains("pane-scoped TUI preferences"));
        assert!(rescued
            .steps
            .iter()
            .any(|step| step.contains("search_files(pattern=\"prefs\", dir=\"src/tui\")")));
    }

    #[test]
    fn rescue_missing_plan_for_tool_turn_after_repeated_blocks_for_openai_actions() {
        let tc = ToolCallData {
            id: "call_2".to_string(),
            name: "read_file".to_string(),
            arguments: serde_json::json!({"path":"Cargo.toml"}).to_string(),
        };
        let messages = vec![
            json!({
                "role": "assistant",
                "content": "",
                "tool_calls": [{
                    "id": "call_1",
                    "type": "function",
                    "function": {
                        "name": "read_file",
                        "arguments": serde_json::json!({"path":"Cargo.toml"}).to_string()
                    }
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_1",
                "content": "GOVERNOR BLOCKED\n\n[Plan Gate] Missing valid <plan>.\n\ntool:\nread_file\narguments:\n{\"path\":\"Cargo.toml\"}"
            }),
        ];

        let rescued = rescue_missing_plan_for_tool_turn(
            &messages,
            &tc,
            "Fix the failing test with the smallest code change. Run tests before you finish.",
            TaskHarness::infer(
                "Fix the failing test with the smallest code change. Run tests before you finish.",
                false,
            ),
            false,
            true,
            ProviderKind::OpenAiCompatible,
            VerificationLevel::Behavioral,
            Some("cargo test 2>&1"),
        )
        .expect("synthetic action plan");

        assert!(rescued.goal.contains("Fix the failing test"));
        assert!(rescued
            .steps
            .iter()
            .any(|step| step.contains("read_file(path=Cargo.toml)")));
        assert!(rescued
            .steps
            .iter()
            .any(|step| step.contains("cargo test 2>&1")));
        assert!(rescued
            .acceptance_criteria
            .iter()
            .all(|item| item.contains("behavioral verification")));
    }

    #[test]
    fn general_plan_rescue_pairs_with_valid_synthetic_think() {
        let tc = ToolCallData {
            id: "call_2".to_string(),
            name: "search_files".to_string(),
            arguments: serde_json::json!({"pattern":"test","dir":"src"}).to_string(),
        };
        let messages = vec![
            json!({
                "role": "assistant",
                "content": "",
                "tool_calls": [{
                    "id": "call_1",
                    "type": "function",
                    "function": {
                        "name": "search_files",
                        "arguments": serde_json::json!({"pattern":"test","dir":"src"}).to_string()
                    }
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_1",
                "content": "GOVERNOR BLOCKED\n\n[Plan Gate] Missing valid <plan>.\n\ntool:\nsearch_files\narguments:\n{\"pattern\":\"test\",\"dir\":\"src\"}"
            }),
        ];

        let plan = rescue_missing_plan_for_tool_turn(
            &messages,
            &tc,
            "Fix the failing test with the smallest code change. Run tests before you finish.",
            TaskHarness::infer(
                "Fix the failing test with the smallest code change. Run tests before you finish.",
                false,
            ),
            false,
            true,
            ProviderKind::OpenAiCompatible,
            VerificationLevel::Behavioral,
            Some("cargo test 2>&1"),
        )
        .expect("synthetic action plan");
        let think = compat_synthetic_think(&tc, &plan);

        validate_think(&think, &plan, &tc).expect("valid synthetic think");
        assert_eq!(think.tool, "search_files");
        assert!(think.next.contains("search"));
    }

    #[test]
    fn rescue_missing_plan_for_create_file_accepts_write_file_turn() {
        let prompt =
            "Create `notes/todo.txt` containing exactly `ship it`. Verify it before you finish.";
        let tc = ToolCallData {
            id: "call_write_retry".to_string(),
            name: "write_file".to_string(),
            arguments: serde_json::json!({
                "path":"notes/todo.txt",
                "content":"ship it\n"
            })
            .to_string(),
        };
        let messages = vec![
            json!({
                "role": "assistant",
                "content": "",
                "tool_calls": [{
                    "id": "call_write_1",
                    "type": "function",
                    "function": {
                        "name": "write_file",
                        "arguments": serde_json::json!({
                            "path":"notes/todo.txt",
                            "content":"ship it\n"
                        }).to_string()
                    }
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_write_1",
                "content": "GOVERNOR BLOCKED\n\n[Plan Gate] Missing valid <plan>.\n\ntool:\nwrite_file\narguments:\n{\"path\":\"notes/todo.txt\",\"content\":\"ship it\\n\"}"
            }),
        ];

        let rescued = rescue_missing_plan_for_tool_turn(
            &messages,
            &tc,
            prompt,
            TaskHarness::infer(prompt, false),
            false,
            true,
            ProviderKind::OpenAiCompatible,
            VerificationLevel::Behavioral,
            Some("test -f notes/todo.txt && grep -Fx \"ship it\" notes/todo.txt"),
        )
        .expect("synthetic create-file plan");

        assert!(rescued
            .steps
            .first()
            .is_some_and(|step| step.contains("write_file(") && step.contains("notes/todo.txt")));
        assert!(rescued
            .acceptance_criteria
            .iter()
            .any(|item| item.contains("artifact")));
        assert!(rescued
            .acceptance_criteria
            .iter()
            .any(|item| item.contains("behavioral verification")));
    }

    #[test]
    fn rescue_missing_think_for_tool_turn_after_repeated_blocks_for_openai_actions() {
        let tc = ToolCallData {
            id: "call_2".to_string(),
            name: "list_dir".to_string(),
            arguments: serde_json::json!({"dir":"."}).to_string(),
        };
        let plan = rescue_missing_plan_for_tool_turn(
            &[json!({
                "role": "assistant",
                "content": "",
                "tool_calls": [{
                    "id": "call_1",
                    "type": "function",
                    "function": {
                        "name": "read_file",
                        "arguments": serde_json::json!({"path":"Cargo.toml"}).to_string()
                    }
                }]
            }), json!({
                "role": "tool",
                "tool_call_id": "call_1",
                "content": "GOVERNOR BLOCKED\n\n[Plan Gate] Missing valid <plan>.\n\ntool:\nread_file\narguments:\n{\"path\":\"Cargo.toml\"}"
            })],
            &ToolCallData {
                id: "call_plan".to_string(),
                name: "read_file".to_string(),
                arguments: serde_json::json!({"path":"Cargo.toml"}).to_string(),
            },
            "Fix the failing test with the smallest code change. Run tests before you finish.",
            TaskHarness::infer(
                "Fix the failing test with the smallest code change. Run tests before you finish.",
                false,
            ),
            false,
            true,
            ProviderKind::OpenAiCompatible,
            VerificationLevel::Behavioral,
            Some("cargo test 2>&1"),
        )
        .expect("synthetic action plan");
        let messages = vec![
            json!({
                "role": "assistant",
                "content": "",
                "tool_calls": [{
                    "id": "call_1",
                    "type": "function",
                    "function": {
                        "name": "list_dir",
                        "arguments": serde_json::json!({"dir":"."}).to_string()
                    }
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_1",
                "content": "GOVERNOR BLOCKED\n\n[Think Gate] Missing <think>.\n\ntool:\nlist_dir\narguments:\n{\"dir\":\".\"}"
            }),
        ];

        let rescued = rescue_missing_think_for_tool_turn(
            &messages,
            &tc,
            &plan,
            false,
            true,
            ProviderKind::OpenAiCompatible,
            false,
        )
        .expect("synthetic think");

        validate_think(&rescued, &plan, &tc).expect("valid synthetic think");
        assert_eq!(rescued.tool, "list_dir");
    }

    #[test]
    fn rescue_missing_think_for_tool_turn_is_immediate_for_patch_file() {
        let tc = ToolCallData {
            id: "call_patch".to_string(),
            name: "patch_file".to_string(),
            arguments: serde_json::json!({
                "path":"src/lib.rs",
                "search":"Hello, {}?",
                "replace":"Hello, {}!"
            })
            .to_string(),
        };
        let plan = PlanBlock {
            goal: "Fix the failing test with the smallest code change.".to_string(),
            steps: vec![
                "inspect the failing code path".to_string(),
                "patch the smallest confirmed bug".to_string(),
                "run cargo test 2>&1".to_string(),
                "call done with verified results".to_string(),
            ],
            acceptance_criteria: vec![
                "the requested change is implemented and confirmed by a passing behavioral verification command".to_string(),
                "the final result cites the exact passing behavioral verification command".to_string(),
            ],
            risks: "wrong file or speculative fix; the chosen test may miss the requested behavior".to_string(),
            assumptions: "the current repo state matches the task and the configured verification command is relevant".to_string(),
        };
        let messages = vec![json!({
            "role": "tool",
            "tool_call_id": "call_patch",
            "content": "GOVERNOR BLOCKED\n\n[Think Gate] Missing <think>.\n\ntool:\npatch_file\narguments:\n{\"path\":\"src/lib.rs\",\"search\":\"Hello, {}?\",\"replace\":\"Hello, {}!\"}"
        })];

        let rescued = rescue_missing_think_for_tool_turn(
            &messages,
            &tc,
            &plan,
            false,
            true,
            ProviderKind::OpenAiCompatible,
            false,
        )
        .expect("synthetic think");

        validate_think(&rescued, &plan, &tc).expect("valid synthetic think");
        assert_eq!(rescued.tool, "patch_file");
    }

    #[test]
    fn rescue_missing_think_for_tool_turn_is_immediate_for_exec() {
        let tc = ToolCallData {
            id: "call_exec".to_string(),
            name: "exec".to_string(),
            arguments: serde_json::json!({"command":"cargo test 2>&1"}).to_string(),
        };
        let plan = PlanBlock {
            goal: "Fix the failing test with the smallest code change.".to_string(),
            steps: vec![
                "inspect the failing code path".to_string(),
                "patch the smallest confirmed bug".to_string(),
                "run cargo test 2>&1".to_string(),
                "call done with verified results".to_string(),
            ],
            acceptance_criteria: vec![
                "the requested change is implemented and confirmed by a passing behavioral verification command".to_string(),
                "the final result cites the exact passing behavioral verification command".to_string(),
            ],
            risks: "wrong file or speculative fix; the chosen test may miss the requested behavior".to_string(),
            assumptions: "the current repo state matches the task and the configured verification command is relevant".to_string(),
        };
        let messages = vec![json!({
            "role": "tool",
            "tool_call_id": "call_exec",
            "content": "GOVERNOR BLOCKED\n\n[Think Gate] Missing <think>.\n\ntool:\nexec\narguments:\n{\"command\":\"cargo test 2>&1\"}"
        })];

        let rescued = rescue_missing_think_for_tool_turn(
            &messages,
            &tc,
            &plan,
            false,
            true,
            ProviderKind::OpenAiCompatible,
            false,
        )
        .expect("synthetic think");

        validate_think(&rescued, &plan, &tc).expect("valid synthetic think");
        assert_eq!(rescued.tool, "exec");
    }

    #[test]
    fn rescue_missing_think_for_tool_turn_is_immediate_for_done() {
        let tc = ToolCallData {
            id: "call_done".to_string(),
            name: "done".to_string(),
            arguments: serde_json::json!({
                "summary":"Fixed the bug and verified it.",
                "completed_acceptance":["1) the requested change is implemented"],
                "remaining_acceptance":["2) the final result cites the exact passing behavioral verification command"],
                "acceptance_evidence":[{"criterion":"1) the requested change is implemented","command":"cargo test 2>&1"}]
            })
            .to_string(),
        };
        let plan = PlanBlock {
            goal: "Fix the failing test with the smallest code change.".to_string(),
            steps: vec![
                "inspect the failing code path".to_string(),
                "patch the smallest confirmed bug".to_string(),
                "run cargo test 2>&1".to_string(),
                "call done with verified results".to_string(),
            ],
            acceptance_criteria: vec![
                "the requested change is implemented and confirmed by a passing behavioral verification command".to_string(),
                "the final result cites the exact passing behavioral verification command".to_string(),
            ],
            risks: "wrong file or speculative fix; the chosen test may miss the requested behavior".to_string(),
            assumptions: "the current repo state matches the task and the configured verification command is relevant".to_string(),
        };

        let rescued = rescue_missing_think_for_tool_turn(
            &[],
            &tc,
            &plan,
            false,
            true,
            ProviderKind::OpenAiCompatible,
            false,
        )
        .expect("synthetic think");

        validate_think(&rescued, &plan, &tc).expect("valid synthetic think");
        assert_eq!(rescued.tool, "done");
        assert!(rescued.next.contains("done"));
    }

    #[test]
    fn rescue_missing_reflection_for_tool_turn_synthesizes_valid_reflection_for_patch() {
        let tc = ToolCallData {
            id: "call_patch".to_string(),
            name: "patch_file".to_string(),
            arguments: serde_json::json!({
                "path":"src/lib.rs",
                "search":"Hello, {}?",
                "replace":"Hello, {}!"
            })
            .to_string(),
        };
        let mut mem = FailureMemory::default();
        mem.consecutive_failures = 1;

        let reflect = rescue_missing_reflection_for_tool_turn(
            "⚠ LOGIC ERROR: The code ran but produced wrong results.",
            &tc,
            false,
            true,
            ProviderKind::OpenAiCompatible,
            &mem,
            0,
        )
        .expect("synthetic reflection");

        validate_reflection(&reflect, &mem, 0).expect("valid synthetic reflection");
        assert_eq!(reflect.last_outcome, "failure");
        assert_eq!(reflect.strategy_change, StrategyChange::Adjust);
        assert!(reflect.next_minimal_action.contains("src/lib.rs"));
    }

    #[test]
    fn rescue_missing_impact_for_tool_turn_synthesizes_valid_impact_after_patch() {
        let tc = ToolCallData {
            id: "call_exec".to_string(),
            name: "exec".to_string(),
            arguments: serde_json::json!({"command":"cargo test 2>&1"}).to_string(),
        };
        let plan = PlanBlock {
            goal: "Fix the failing test with the smallest code change.".to_string(),
            steps: vec![
                "inspect the failing code path".to_string(),
                "patch the smallest confirmed bug".to_string(),
                "run cargo test 2>&1".to_string(),
                "call done with verified results".to_string(),
            ],
            acceptance_criteria: vec![
                "the requested change is implemented and confirmed by a passing behavioral verification command".to_string(),
                "the final result cites the exact passing behavioral verification command".to_string(),
            ],
            risks: "wrong file or speculative fix; the chosen test may miss the requested behavior".to_string(),
            assumptions: "the current repo state matches the task and the configured verification command is relevant".to_string(),
        };

        let impact = rescue_missing_impact_for_tool_turn(
            "successful mutation via patch_file: src/lib.rs",
            &tc,
            false,
            true,
            ProviderKind::OpenAiCompatible,
            Some(&plan),
            impact_recovery::UnreviewedMutation::from_steps(Some(1), None),
        )
        .expect("synthetic impact");

        validate_impact(&impact, Some(&plan)).expect("valid synthetic impact");
        assert!(impact.changed.contains("src/lib.rs"));
        assert!(impact.progress.contains("step 2"));
        assert!(impact.remaining_gap.contains("cargo test 2>&1"));
    }

    #[test]
    fn rescue_missing_evidence_for_tool_turn_synthesizes_valid_evidence_for_patch() {
        let tc = ToolCallData {
            id: "call_patch".to_string(),
            name: "patch_file".to_string(),
            arguments: serde_json::json!({
                "path":"src/lib.rs",
                "search":"Hello, {}?",
                "replace":"Hello, {}!"
            })
            .to_string(),
        };
        let observations = ObservationEvidence {
            reads: vec![ObservationReadEvidence {
                command: "read_file(path=src/lib.rs)".to_string(),
                path: "src/lib.rs".to_string(),
            }],
            searches: Vec::new(),
            resolutions: Vec::new(),
        };

        let evidence = rescue_missing_evidence_for_tool_turn(
            &tc,
            false,
            true,
            ProviderKind::OpenAiCompatible,
            &observations,
        )
        .expect("synthetic evidence");

        validate_evidence_block(&evidence, &tc, &observations).expect("valid synthetic evidence");
        assert_eq!(evidence.target_files, vec!["src/lib.rs".to_string()]);
        assert!(evidence.evidence.contains("read_file(path=src/lib.rs)"));
    }

    #[test]
    fn rescue_missing_evidence_for_tool_turn_accepts_truncated_patch_path() {
        let tc = ToolCallData {
            id: "call_patch".to_string(),
            name: "patch_file".to_string(),
            arguments:
                "{\"path\":\"src/observer/repo_rules.rs\",\"search\":\"const TUI_REPLAY_PATHS: &["
                    .to_string(),
        };
        let observations = ObservationEvidence {
            reads: vec![ObservationReadEvidence {
                command: "read_file(path=src/observer/repo_rules.rs)".to_string(),
                path: "src/observer/repo_rules.rs".to_string(),
            }],
            searches: Vec::new(),
            resolutions: Vec::new(),
        };

        let evidence = rescue_missing_evidence_for_tool_turn(
            &tc,
            false,
            true,
            ProviderKind::OpenAiCompatible,
            &observations,
        )
        .expect("synthetic evidence for truncated patch");

        validate_evidence_block(&evidence, &tc, &observations).expect("valid synthetic evidence");
        assert_eq!(
            evidence.target_files,
            vec!["src/observer/repo_rules.rs".to_string()]
        );
    }

    #[test]
    fn repair_truncated_patch_tool_call_from_recent_mismatch_recovers_smoke_fix() {
        let tc = ToolCallData {
            id: "call_patch".to_string(),
            name: "patch_file".to_string(),
            arguments: "{\"path\":\"src/lib.rs\",\"search\":\"pub fn greet(name: &str) -> String {\\n    format!(\\\"Hello, {}?\\\", name)\\n}\\n\",\"replace\":\"".to_string(),
        };
        let messages = vec![json!({
            "role": "tool",
            "tool_call_id": "call_exec",
            "content": "thread 'tests::greet_is_excited' panicked at src/lib.rs:12:9:\nassertion `left == right` failed\n  left: \"Hello, Ada?\"\n right: \"Hello, Ada!\""
        })];
        let observations = ObservationEvidence {
            reads: vec![ObservationReadEvidence {
                command: "read_file(path=src/lib.rs)".to_string(),
                path: "src/lib.rs".to_string(),
            }],
            searches: Vec::new(),
            resolutions: Vec::new(),
        };

        let (rewritten, original, repaired) =
            repair_truncated_patch_tool_call_from_recent_mismatch(
                &messages,
                &tc,
                false,
                true,
                ProviderKind::OpenAiCompatible,
                &observations,
            )
            .expect("repair malformed patch tool call");

        assert!(original.starts_with("patch_file"));
        assert!(original.contains("\"replace\":\""));
        assert!(repaired.starts_with("patch_file("));
        assert!(repaired.contains("Hello, {}!"));
        assert_eq!(rewritten.name, "patch_file");
        let rewritten_args =
            serde_json::from_str::<serde_json::Value>(&rewritten.arguments).expect("valid json");
        assert_eq!(
            rewritten_args.get("path").and_then(|v| v.as_str()),
            Some("src/lib.rs")
        );
        assert_eq!(
            rewritten_args.get("replace").and_then(|v| v.as_str()),
            Some("pub fn greet(name: &str) -> String {\n    format!(\"Hello, {}!\", name)\n}\n")
        );
    }

    #[test]
    fn consecutive_missing_think_blocks_for_tool_counts_same_observation_tool() {
        let tc = ToolCallData {
            id: "call_3".to_string(),
            name: "search_files".to_string(),
            arguments: serde_json::json!({"pattern":"realize","dir":"src"}).to_string(),
        };
        let messages = vec![
            json!({
                "role": "assistant",
                "content": "",
                "tool_calls": [{
                    "id": "call_1",
                    "type": "function",
                    "function": {
                        "name": "search_files",
                        "arguments": serde_json::json!({"pattern":"realize","dir":"src"}).to_string()
                    }
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_1",
                "content": "GOVERNOR BLOCKED\n\n[Think Gate] Missing <think>.\n\ntool:\nsearch_files\narguments:\n{\"pattern\":\"realize\",\"dir\":\"src\"}"
            }),
            json!({
                "role": "assistant",
                "content": "",
                "tool_calls": [{
                    "id": "call_2",
                    "type": "function",
                    "function": {
                        "name": "search_files",
                        "arguments": serde_json::json!({"pattern":"realize","dir":"src"}).to_string()
                    }
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_2",
                "content": "GOVERNOR BLOCKED\n\n[Think Gate] Missing <think>.\n\ntool:\nsearch_files\narguments:\n{\"pattern\":\"realize\",\"dir\":\"src\"}"
            }),
        ];

        assert_eq!(consecutive_missing_think_blocks_for_tool(&messages, &tc), 2);
    }

    #[test]
    fn consecutive_missing_think_blocks_for_observation_counts_mixed_tools() {
        let messages = vec![
            json!({
                "role": "assistant",
                "content": "",
                "tool_calls": [{
                    "id": "call_1",
                    "type": "function",
                    "function": {
                        "name": "search_files",
                        "arguments": serde_json::json!({"pattern":"prefs","dir":"src/tui"}).to_string()
                    }
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_1",
                "content": "GOVERNOR BLOCKED\n\n[Think Gate] Missing <think>.\n\ntool:\nsearch_files\narguments:\n{\"pattern\":\"prefs\",\"dir\":\"src/tui\"}"
            }),
            json!({
                "role": "assistant",
                "content": "",
                "tool_calls": [{
                    "id": "call_2",
                    "type": "function",
                    "function": {
                        "name": "list_dir",
                        "arguments": serde_json::json!({"dir":"src/tui"}).to_string()
                    }
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_2",
                "content": "GOVERNOR BLOCKED\n\n[Think Gate] Missing <think>.\n\ntool:\nlist_dir\narguments:\n{\"dir\":\"src/tui\"}"
            }),
        ];

        assert_eq!(
            consecutive_missing_think_blocks_for_observation(&messages),
            2
        );
    }

    #[test]
    fn rescue_read_only_missing_think_for_tool_turn_after_repeated_blocks() {
        let tc = ToolCallData {
            id: "call_2".to_string(),
            name: "search_files".to_string(),
            arguments: serde_json::json!({"pattern":"realize","dir":"src"}).to_string(),
        };
        let plan = synthetic_read_only_observation_plan(
            "Locate where the /realize slash command is handled in the TUI. Do not edit anything.",
        );
        let messages = vec![
            json!({
                "role": "assistant",
                "content": "",
                "tool_calls": [{
                    "id": "call_1",
                    "type": "function",
                    "function": {
                        "name": "search_files",
                        "arguments": serde_json::json!({"pattern":"realize","dir":"src"}).to_string()
                    }
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_1",
                "content": "GOVERNOR BLOCKED\n\n[Think Gate] Missing <think>.\n\ntool:\nsearch_files\narguments:\n{\"pattern\":\"realize\",\"dir\":\"src\"}"
            }),
        ];

        let rescued = rescue_read_only_missing_think_for_tool_turn(
            &messages,
            &tc,
            &plan,
            "Locate where the /realize slash command is handled in the TUI. Do not edit anything.",
            true,
            ProviderKind::OpenAiCompatible,
            &ObservationEvidence::default(),
        )
        .expect("synthetic think");
        assert_eq!(rescued.tool, "search_files");
        assert!(rescued.next.contains("search"));
    }

    #[test]
    fn rescue_read_only_missing_think_for_read_file_is_immediate() {
        let tc = ToolCallData {
            id: "call_1".to_string(),
            name: "read_file".to_string(),
            arguments: serde_json::json!({"path":"src/tui/events.rs"}).to_string(),
        };
        let plan = synthetic_read_only_observation_plan(
            "Locate where the /realize slash command is handled in the TUI. Do not edit anything.",
        );

        let rescued = rescue_read_only_missing_think_for_tool_turn(
            &[],
            &tc,
            &plan,
            "Locate where the /realize slash command is handled in the TUI. Do not edit anything.",
            true,
            ProviderKind::OpenAiCompatible,
            &ObservationEvidence::default(),
        )
        .expect("synthetic think");
        assert_eq!(rescued.tool, "read_file");
        assert!(rescued.next.contains("read"));
    }

    #[test]
    fn rescue_read_only_missing_think_for_done_is_immediate_with_strong_evidence() {
        let tc = ToolCallData {
            id: "call_done".to_string(),
            name: "done".to_string(),
            arguments: serde_json::json!({
                "summary":"Found prefs storage",
                "completed_acceptance":["1) file identified","2) context confirmed"],
                "remaining_acceptance":[],
                "acceptance_evidence":[{"criterion":"file identified","command":"read_file(path=src/tui/prefs.rs)"}]
            })
            .to_string(),
        };
        let plan = synthetic_read_only_observation_plan(
            "Find where pane-scoped TUI preferences are serialized and restored. Do not edit anything.",
        );
        let evidence = ObservationEvidence {
            searches: vec![ObservationSearchEvidence {
                command: "search_files(dir=src/tui, pattern=prefs)".to_string(),
                pattern: "prefs".to_string(),
                hit_count: 2,
                paths: vec!["src/tui/prefs.rs".to_string()],
            }],
            reads: vec![ObservationReadEvidence {
                command: "read_file(path=src/tui/prefs.rs)".to_string(),
                path: "src/tui/prefs.rs".to_string(),
            }],
            resolutions: Vec::new(),
        };

        let rescued = rescue_read_only_missing_think_for_tool_turn(
            &[],
            &tc,
            &plan,
            "Find where pane-scoped TUI preferences are serialized and restored. Do not edit anything.",
            true,
            ProviderKind::OpenAiCompatible,
            &evidence,
        )
        .expect("synthetic think");
        assert_eq!(rescued.tool, "done");
    }

    #[test]
    fn coerce_read_only_observation_tool_call_rewrites_search_pattern_after_repeated_gate_misses() {
        let tc = ToolCallData {
            id: "call_3".to_string(),
            name: "search_files".to_string(),
            arguments: serde_json::json!({"pattern":"pane","dir":"src"}).to_string(),
        };
        let messages = vec![
            json!({
                "role": "assistant",
                "content": "",
                "tool_calls": [{
                    "id": "call_1",
                    "type": "function",
                    "function": {
                        "name": "search_files",
                        "arguments": serde_json::json!({"pattern":"pane","dir":"src"}).to_string()
                    }
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_1",
                "content": "GOVERNOR BLOCKED\n\n[Plan Gate] Missing valid <plan>.\n\ntool:\nsearch_files\narguments:\n{\"pattern\":\"pane\",\"dir\":\"src\"}"
            }),
        ];

        let (rewritten, original, coerced) = coerce_read_only_observation_tool_call(
            &messages,
            &tc,
            "Find where pane-scoped TUI preferences are serialized and restored. Do not edit anything.",
            true,
            &ObservationEvidence::default(),
        )
        .expect("coerced");

        assert_eq!(original, "search_files(dir=src, pattern=pane)");
        assert_eq!(rewritten.name, "search_files");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&rewritten.arguments).expect("json")
                ["pattern"]
                .as_str(),
            Some("prefs")
        );
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&rewritten.arguments).expect("json")["dir"]
                .as_str(),
            Some("src/tui")
        );
        assert_eq!(coerced, "search_files(dir=src/tui, pattern=prefs)");
    }

    #[test]
    fn coerce_read_only_observation_tool_call_rewrites_list_dir_to_preferred_search() {
        let tc = ToolCallData {
            id: "call_3".to_string(),
            name: "list_dir".to_string(),
            arguments: serde_json::json!({"dir":"src"}).to_string(),
        };
        let messages = vec![
            json!({
                "role": "assistant",
                "content": "",
                "tool_calls": [{
                    "id": "call_1",
                    "type": "function",
                    "function": {
                        "name": "search_files",
                        "arguments": serde_json::json!({"pattern":"agent","dir":"src"}).to_string()
                    }
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_1",
                "content": "GOVERNOR BLOCKED\n\n[Think Gate] Missing <think>.\n\ntool:\nsearch_files\narguments:\n{\"pattern\":\"agent\",\"dir\":\"src\"}"
            }),
        ];

        let (rewritten, _original, coerced) = coerce_read_only_observation_tool_call(
            &messages,
            &tc,
            "Locate where the coder-side repo-map read_file fallback is wired into the TUI agent flow. Do not edit anything.",
            true,
            &ObservationEvidence::default(),
        )
        .expect("coerced");

        assert_eq!(rewritten.name, "search_files");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&rewritten.arguments).expect("json")
                ["pattern"]
                .as_str(),
            Some("lazy_read_fallback")
        );
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&rewritten.arguments).expect("json")["dir"]
                .as_str(),
            Some("src/tui")
        );
        assert_eq!(
            coerced,
            "search_files(dir=src/tui, pattern=lazy_read_fallback)"
        );
    }

    #[test]
    fn validate_think_rejects_tool_mismatch() {
        let plan = PlanBlock {
            goal: "ship fix".to_string(),
            steps: vec![
                "inspect".to_string(),
                "edit".to_string(),
                "verify".to_string(),
            ],
            acceptance_criteria: vec!["cargo test passes".to_string()],
            risks: "wrong file".to_string(),
            assumptions: "tests exist".to_string(),
        };
        let think = ThinkBlock {
            goal: "verify".to_string(),
            step: 3,
            tool: "read_file".to_string(),
            risk: "wrong target".to_string(),
            doubt: "might need tests".to_string(),
            next: "cargo test".to_string(),
            verify: "exit zero".to_string(),
        };
        let tc = ToolCallData {
            id: "call_1".to_string(),
            name: "exec".to_string(),
            arguments: "{\"command\":\"cargo test\"}".to_string(),
        };
        assert!(validate_think(&think, &plan, &tc).is_err());
    }

    #[test]
    fn validate_think_rejects_exec_next_mismatch() {
        let plan = PlanBlock {
            goal: "ship fix".to_string(),
            steps: vec![
                "inspect".to_string(),
                "edit".to_string(),
                "verify".to_string(),
            ],
            acceptance_criteria: vec!["cargo test passes".to_string()],
            risks: "wrong file".to_string(),
            assumptions: "tests exist".to_string(),
        };
        let think = ThinkBlock {
            goal: "verify".to_string(),
            step: 3,
            tool: "exec".to_string(),
            risk: "wrong target".to_string(),
            doubt: "might hit wrong crate".to_string(),
            next: "cargo check".to_string(),
            verify: "exit zero".to_string(),
        };
        let tc = ToolCallData {
            id: "call_1".to_string(),
            name: "exec".to_string(),
            arguments: "{\"command\":\"cargo test --lib\"}".to_string(),
        };
        assert!(validate_think(&think, &plan, &tc).is_err());
    }

    #[test]
    fn validate_think_accepts_matching_exec_prefix() {
        let plan = PlanBlock {
            goal: "ship fix".to_string(),
            steps: vec![
                "inspect".to_string(),
                "edit".to_string(),
                "verify".to_string(),
            ],
            acceptance_criteria: vec!["cargo test passes".to_string()],
            risks: "wrong file".to_string(),
            assumptions: "tests exist".to_string(),
        };
        let think = ThinkBlock {
            goal: "verify".to_string(),
            step: 3,
            tool: "exec".to_string(),
            risk: "wrong target".to_string(),
            doubt: "might need workspace".to_string(),
            next: "cargo test".to_string(),
            verify: "exit zero".to_string(),
        };
        let tc = ToolCallData {
            id: "call_1".to_string(),
            name: "exec".to_string(),
            arguments: "{\"command\":\"cargo test --lib\"}".to_string(),
        };
        assert!(validate_think(&think, &plan, &tc).is_ok());
    }

    #[test]
    fn validate_reflection_rejects_missing_fields() {
        let mem = FailureMemory::default();
        let r = ReflectionBlock {
            last_outcome: "failure".to_string(),
            goal_delta: GoalDelta::Same,
            wrong_assumption: "".to_string(),
            strategy_change: StrategyChange::Adjust,
            next_minimal_action: "".to_string(),
        };
        assert!(validate_reflection(&r, &mem, 0).is_err());
    }

    #[test]
    fn validate_reflection_rejects_keep_on_repeated_failure() {
        let mem = FailureMemory {
            consecutive_failures: 2,
            last_command_sig: None,
            same_command_repeats: 3,
            last_error_sig: None,
            same_error_repeats: 2,
            last_output_hash: None,
            same_output_repeats: 2,
            last_error_class: ErrorClass::Unknown,
        };
        let r = ReflectionBlock {
            last_outcome: "failure".to_string(),
            goal_delta: GoalDelta::Same,
            wrong_assumption: "I assumed the command would work unchanged.".to_string(),
            strategy_change: StrategyChange::Keep,
            next_minimal_action: "try again".to_string(),
        };
        assert!(validate_reflection(&r, &mem, 0).is_err());
    }

    #[test]
    fn validate_reflection_rejects_keep_on_file_tool_repeats() {
        let mem = FailureMemory::default();
        let r = ReflectionBlock {
            last_outcome: "failure".to_string(),
            goal_delta: GoalDelta::Closer,
            wrong_assumption: "I assumed the patch would apply cleanly.".to_string(),
            strategy_change: StrategyChange::Keep,
            next_minimal_action: "retry patch_file with same args".to_string(),
        };
        assert!(validate_reflection(&r, &mem, 2).is_err());
    }

    #[test]
    fn rebuilds_failure_memory_from_session_messages() {
        let messages = vec![
            json!({"role":"system","content":"sys"}),
            json!({"role":"user","content":"do thing"}),
            json!({"role":"assistant","content":"","tool_calls":[{"id":"call_1","type":"function","function":{"name":"exec","arguments":"{\"command\":\"git status\"}"}}]}),
            json!({"role":"tool","tool_call_id":"call_1","content":"FAILED (exit_code: 1)\nstderr:\nfatal: nope"}),
            json!({"role":"assistant","content":"","tool_calls":[{"id":"call_2","type":"function","function":{"name":"exec","arguments":"{\"command\":\"git status\"}"}}]}),
            json!({"role":"tool","tool_call_id":"call_2","content":"FAILED (exit_code: 1)\nstderr:\nfatal: nope"}),
        ];

        let mem = FailureMemory::from_recent_messages(&messages);
        assert_eq!(mem.consecutive_failures, 2);
        assert_eq!(mem.same_command_repeats, 2);
        assert_eq!(mem.same_error_repeats, 2);
    }

    #[test]
    fn classify_exec_kind_treats_git_status_as_diagnostic() {
        assert_eq!(classify_exec_kind("git status", None), ExecKind::Diagnostic);
        assert_eq!(classify_exec_kind("cargo check", None), ExecKind::Verify);
    }

    #[test]
    fn classify_verify_level_distinguishes_build_and_behavioral() {
        assert_eq!(
            classify_verify_level("cargo check", None),
            Some(VerificationLevel::Build)
        );
        assert_eq!(
            classify_verify_level("cargo test --lib", None),
            Some(VerificationLevel::Behavioral)
        );
        assert_eq!(
            classify_verify_level("npm run lint", None),
            Some(VerificationLevel::Build)
        );
        assert_eq!(
            classify_verify_level("python -m pytest -q", None),
            Some(VerificationLevel::Behavioral)
        );
        assert_eq!(classify_verify_level("git status", None), None);
    }

    #[test]
    fn classify_verify_level_rejects_failure_suppressed_commands() {
        assert_eq!(classify_verify_level("cargo test || true", None), None);
        assert_eq!(
            classify_verify_level(
                "cd maze_game_pygame && SDL_VIDEODRIVER=dummy python3 -m unittest -q 2>&1 || true",
                Some("cd maze_game_pygame && SDL_VIDEODRIVER=dummy python3 -m unittest -q 2>&1"),
            ),
            None
        );
    }

    #[test]
    fn classify_verify_level_requires_full_configured_test_command_match() {
        let configured = "test -d maze_game_pygame/.git && test -f maze_game_pygame/README.md && test -f maze_game_pygame/game.py && test -f maze_game_pygame/main.py && test -f maze_game_pygame/test_game.py && cd maze_game_pygame && SDL_VIDEODRIVER=dummy python3 -m unittest -q 2>&1";
        let weak_probe = "test -d maze_game_pygame/.git && test -f maze_game_pygame/readme.md && test -f maze_game_pygame/game.py && test -f maze_game_pygame/main.py && test -f maze_game_pygame/test_game.py || true";

        assert_eq!(classify_verify_level(weak_probe, Some(configured)), None);
        assert_eq!(
            classify_verify_level(configured, Some(configured)),
            Some(VerificationLevel::Behavioral)
        );
    }

    #[test]
    fn restore_done_gate_from_messages_ignores_weak_prefixed_test_probe() {
        let configured = "test -d maze_game_pygame/.git && test -f maze_game_pygame/README.md && test -f maze_game_pygame/game.py && test -f maze_game_pygame/main.py && test -f maze_game_pygame/test_game.py && cd maze_game_pygame && SDL_VIDEODRIVER=dummy python3 -m unittest -q 2>&1";
        let weak_probe = "test -d maze_game_pygame/.git && test -f maze_game_pygame/readme.md && test -f maze_game_pygame/game.py && test -f maze_game_pygame/main.py && test -f maze_game_pygame/test_game.py || true";
        let messages = vec![
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_readme",
                    "type": "function",
                    "function": {
                        "name": "write_file",
                        "arguments": "{\"path\":\"maze_game_pygame/README.md\",\"content\":\"# maze_game_pygame\\n\"}"
                    }
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_readme",
                "content": "OK: wrote 'maze_game_pygame/README.md' (1 lines, 19 bytes)\n[hash] before=0 after=1"
            }),
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_probe",
                    "type": "function",
                    "function": {
                        "name": "exec",
                        "arguments": format!("{{\"command\":{}}}", serde_json::to_string(weak_probe).expect("json"))
                    }
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_probe",
                "content": "OK (exit_code: 0)\nstdout:\n"
            }),
        ];

        let (_, last_mutation_step, last_build_verify_ok_step, last_behavioral_verify_ok_step, _) =
            restore_done_gate_from_messages(&messages, Some(configured));

        assert_eq!(last_mutation_step, Some(2));
        assert_eq!(last_build_verify_ok_step, None);
        assert_eq!(last_behavioral_verify_ok_step, None);
    }

    #[test]
    fn effective_verify_ok_step_requires_behavioral_when_requested() {
        assert_eq!(
            effective_verify_ok_step(VerificationLevel::Build, Some(3), Some(5)),
            Some(5)
        );
        assert_eq!(
            effective_verify_ok_step(VerificationLevel::Behavioral, Some(3), Some(5)),
            Some(5)
        );
        assert_eq!(
            effective_verify_ok_step(VerificationLevel::Behavioral, Some(3), None),
            None
        );
    }

    #[test]
    fn infer_required_verification_level_keeps_docs_build_only() {
        assert_eq!(
            infer_required_verification_level("Update README wording and docs only", None),
            VerificationLevel::Build
        );
        assert_eq!(
            infer_required_verification_level("Fix runtime bug in agent loop", None),
            VerificationLevel::Behavioral
        );
    }

    #[test]
    fn goal_check_policy_respects_command_approval() {
        assert!(should_auto_run_goal_checks(false, DEFAULT_MAX_ITERS));
        assert!(!should_auto_run_goal_checks(true, DEFAULT_MAX_ITERS));
    }

    #[test]
    fn recovery_gate_allows_new_file_write_during_diagnose() {
        let recovery = RecoveryGovernor {
            stage: Some(RecoveryStage::Diagnose),
            required_verification: VerificationLevel::Behavioral,
        };
        let tc = ToolCallData {
            id: "call_write".to_string(),
            name: "write_file".to_string(),
            arguments: serde_json::json!({
                "path":"notes/todo.txt",
                "content":"ship it\n"
            })
            .to_string(),
        };

        assert!(recovery
            .maybe_block_tool(
                &tc,
                Some("test -f notes/todo.txt && grep -Fx \"ship it\" notes/todo.txt"),
                TaskHarness::infer(
                    "Create `notes/todo.txt` containing exactly `ship it`.",
                    false,
                ),
                false,
            )
            .is_none());
    }

    #[test]
    fn verification_requirement_prompt_is_idle_when_no_verify_pressure() {
        let recovery = RecoveryGovernor::default();
        let goal_checks = GoalCheckTracker::default();
        assert!(!should_emit_verification_requirement_prompt(
            AgentState::Planning,
            &recovery,
            Some(2),
            Some(2),
            &goal_checks,
        ));
    }

    #[test]
    fn verification_requirement_prompt_activates_for_pending_verify_or_goal_checks() {
        let recovery = RecoveryGovernor::default();
        let goal_checks = GoalCheckTracker {
            tests: GoalCheckStatus {
                attempts: 1,
                ok: false,
            },
            ..GoalCheckTracker::default()
        };
        assert!(should_emit_verification_requirement_prompt(
            AgentState::Planning,
            &recovery,
            Some(3),
            Some(2),
            &GoalCheckTracker::default(),
        ));
        assert!(should_emit_verification_requirement_prompt(
            AgentState::Planning,
            &recovery,
            Some(2),
            Some(2),
            &goal_checks,
        ));
    }

    #[test]
    fn goal_check_runner_command_uses_shared_catalog() {
        let td = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            td.path().join("Cargo.toml"),
            "[package]\nname='x'\nversion='0.1.0'\n",
        )
        .expect("write cargo");
        let cmd = goal_check_runner_command(
            VerificationLevel::Behavioral,
            td.path().to_string_lossy().as_ref(),
            None,
        )
        .expect("runner command");
        assert_eq!(cmd, "cargo test -q");
    }

    #[test]
    fn goal_check_runner_command_prefers_configured_test_cmd() {
        let td = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            td.path().join("Cargo.toml"),
            "[package]\nname='x'\nversion='0.1.0'\n",
        )
        .expect("write cargo");

        let cmd = goal_check_runner_command(
            VerificationLevel::Behavioral,
            td.path().to_string_lossy().as_ref(),
            Some("cargo test -q demo::tests:: 2>&1 && bash scripts/smoke.sh"),
        )
        .expect("runner command");

        assert_eq!(
            cmd,
            "cargo test -q demo::tests:: 2>&1 && bash scripts/smoke.sh"
        );
    }

    #[test]
    fn goal_check_result_preserves_verification_command_case_for_evidence() {
        let td = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(td.path().join("demo_repo/.git")).expect("mkdir .git");
        std::fs::write(td.path().join("demo_repo/README.md"), "# demo\n").expect("write readme");
        std::fs::write(td.path().join("demo_repo/.gitignore"), "target/\n")
            .expect("write gitignore");
        let command =
            "test -d demo_repo/.git && test -f demo_repo/README.md && test -f demo_repo/.gitignore";
        let rt = tokio::runtime::Runtime::new().expect("rt");
        let (_tx, mut _rx) = tokio::sync::mpsc::channel(4);
        let result = rt.block_on(run_goal_check_command(
            "repo",
            command,
            td.path().to_string_lossy().as_ref(),
            &_tx,
        ));

        assert!(result.passed);
        assert_eq!(result.command, command);
    }

    #[test]
    fn repo_goal_missing_labels_follow_shared_requirements() {
        let td = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(td.path().join(".git")).expect("mkdir .git");
        let rt = tokio::runtime::Runtime::new().expect("rt");
        let missing = rt.block_on(repo_goal_missing_labels(
            td.path().to_string_lossy().as_ref(),
        ));
        assert!(missing.contains(&"HEAD (commit)".to_string()));
        assert!(missing.contains(&"README.md".to_string()));
        assert!(!missing.contains(&".git".to_string()));
    }

    #[test]
    fn verification_examples_include_shared_goal_check_runners() {
        let build = verification_examples(VerificationLevel::Build);
        let behavioral = verification_examples(VerificationLevel::Behavioral);

        assert!(build.contains("cargo build -q"));
        assert!(behavioral.contains("python -m pytest -q"));
    }

    #[test]
    fn restore_done_gate_does_not_count_git_status_as_verification() {
        let messages = vec![
            json!({"role":"assistant","content":"","tool_calls":[{"id":"call_1","type":"function","function":{"name":"exec","arguments":"{\"command\":\"touch foo.txt\"}"}}]}),
            json!({"role":"tool","tool_call_id":"call_1","content":"OK (exit_code: 0)\nstdout:\n"}),
            json!({"role":"assistant","content":"","tool_calls":[{"id":"call_2","type":"function","function":{"name":"exec","arguments":"{\"command\":\"git status\"}"}}]}),
            json!({"role":"tool","tool_call_id":"call_2","content":"OK (exit_code: 0)\nstdout:\nOn branch main\n"}),
        ];

        let (_, last_mutation_step, _last_build_verify_ok_step, last_behavioral_verify_ok_step, _) =
            restore_done_gate_from_messages(&messages, None);

        assert_eq!(last_mutation_step, Some(1));
        assert_eq!(last_behavioral_verify_ok_step, None);
    }

    #[test]
    fn upgrade_required_verification_from_messages_detects_code_mutation() {
        let messages = vec![
            json!({"role":"assistant","content":"","tool_calls":[{"id":"call_1","type":"function","function":{"name":"patch_file","arguments":"{\"path\":\"src/tui/agent.rs\",\"search\":\"a\",\"replace\":\"b\"}"}}]}),
            json!({"role":"tool","tool_call_id":"call_1","content":"OK\n[hash] before=1 after=2"}),
        ];

        assert_eq!(
            upgrade_required_verification_from_messages(&messages, VerificationLevel::Build),
            VerificationLevel::Behavioral
        );
    }

    #[test]
    fn working_memory_rebuilds_from_messages() {
        let messages = vec![
            json!({
                "role":"assistant",
                "content":"<plan>\ngoal: ship fix\nsteps: 1) inspect files 2) edit agent 3) verify build\nacceptance: 1) cargo check passes 2) runtime gate blocks bad tool calls\nrisks: wrong file, wrong command\nassumptions: cargo works\n</plan>\n<think>\ngoal: inspect file\nstep: 1\ntool: read_file\nrisk: wrong path\ndoubt: maybe wrong target\nnext: read src/tui/agent.rs\nverify: file opens\n</think>",
                "tool_calls":[{"id":"call_1","type":"function","function":{"name":"read_file","arguments":"{\"path\":\"src/tui/agent.rs\"}"}}]
            }),
            json!({"role":"tool","tool_call_id":"call_1","content":"[src/tui/agent.rs] (10 lines, 100 bytes)\nfn main() {}\n"}),
            json!({
                "role":"assistant",
                "content":"<think>\ngoal: verify build\nstep: 3\ntool: exec\nrisk: wrong target\ndoubt: maybe workspace differs\nnext: cargo check\nverify: exit code is zero\n</think>",
                "tool_calls":[{"id":"call_2","type":"function","function":{"name":"exec","arguments":"{\"command\":\"cargo check\"}"}}]
            }),
            json!({"role":"tool","tool_call_id":"call_2","content":"OK (exit_code: 0)\ncwd: /repo\nstdout:\nFinished dev [unoptimized] target(s) in 0.10s\n"}),
            json!({"role":"assistant","content":"<reflect>\nlast_outcome: partial\ngoal_delta: closer\nwrong_assumption: cargo test was necessary first\nstrategy_change: adjust\nnext_minimal_action: run targeted tests\n</reflect>"}),
        ];

        let mem = WorkingMemory::from_messages(&messages, None);
        assert!(mem.facts.iter().any(|fact| fact.contains("cwd: /repo")));
        assert!(mem
            .completed_steps
            .iter()
            .any(|step| step == "inspect files"));
        assert!(mem
            .completed_steps
            .iter()
            .any(|step| step == "verify build"));
        assert!(mem
            .successful_verifications
            .iter()
            .any(|cmd| cmd == "cargo check"));
        assert_eq!(mem.chosen_strategy.as_deref(), Some("run targeted tests"));
    }

    #[test]
    fn working_memory_sync_to_plan_drops_stale_steps() {
        let mut mem = WorkingMemory::default();
        mem.remember_completed_step("inspect files");
        mem.remember_completed_step("verify build");

        let plan = PlanBlock {
            goal: "ship fix".to_string(),
            steps: vec!["verify build".to_string(), "ship".to_string()],
            acceptance_criteria: vec!["cargo check passes".to_string()],
            risks: "wrong file".to_string(),
            assumptions: "tests exist".to_string(),
        };

        mem.sync_to_plan(&plan);
        assert_eq!(mem.completed_steps, vec!["verify build".to_string()]);
    }

    #[test]
    fn remember_successful_verification_preserves_long_configured_test_command() {
        let mut mem = WorkingMemory::default();
        let command = "test -d maze_game/.git && test -f maze_game/README.md && test -f maze_game/src/lib.rs && test -f maze_game/src/main.rs && cd maze_game && cargo test 2>&1";

        mem.remember_successful_verification(command);

        assert_eq!(mem.successful_verifications.len(), 1);
        assert!(mem.successful_verifications[0].contains("maze_game/src/main.rs"));
        assert!(mem.successful_verifications[0].contains("cargo test 2>&1"));
    }

    #[test]
    fn remember_successful_verification_preserves_pygame_unittest_tail() {
        let mut mem = WorkingMemory::default();
        let command = "test -d maze_game_pygame/.git && test -f maze_game_pygame/README.md && test -f maze_game_pygame/game.py && test -f maze_game_pygame/main.py && test -f maze_game_pygame/test_game.py && cd maze_game_pygame && SDL_VIDEODRIVER=dummy python3 -m unittest -q 2>&1";

        mem.remember_successful_verification(command);

        assert_eq!(mem.successful_verifications.len(), 1);
        assert!(mem.successful_verifications[0].contains("maze_game_pygame/test_game.py"));
        assert!(mem.successful_verifications[0]
            .contains("SDL_VIDEODRIVER=dummy python3 -m unittest -q 2>&1"));
    }

    #[test]
    fn collect_known_acceptance_commands_recovers_test_cmd_from_auto_test_pass() {
        let test_cmd = "test -d maze_game_pygame/.git && test -f maze_game_pygame/README.md && test -f maze_game_pygame/game.py && test -f maze_game_pygame/main.py && test -f maze_game_pygame/test_game.py && cd maze_game_pygame && SDL_VIDEODRIVER=dummy python3 -m unittest -q 2>&1";
        let messages = vec![
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_test_game",
                    "type": "function",
                    "function": {
                        "name": "write_file",
                        "arguments": "{\"path\":\"maze_game_pygame/test_game.py\",\"content\":\"print('ok')\\n\"}"
                    }
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_test_game",
                "content": "OK: wrote 'maze_game_pygame/test_game.py' (1 lines, 12 bytes)\n[hash] before=0 after=1\n[auto-test] ✓ PASSED (exit 0)\nRan 3 tests in 0.10s\nOK"
            }),
        ];

        let commands =
            collect_known_acceptance_commands(&messages, &WorkingMemory::default(), Some(test_cmd));

        assert!(commands.iter().any(|command| command == test_cmd));
    }

    #[test]
    fn wrap_exec_with_pwd_preserves_nonzero_exit_code_on_posix() {
        if cfg!(target_os = "windows") {
            return;
        }

        let wrapped = wrap_exec_with_pwd("false");
        let rt = tokio::runtime::Runtime::new().expect("rt");
        let result = rt
            .block_on(exec::run_command(&wrapped, None))
            .expect("run wrapped command");
        let (_stdout, pwd_after) = strip_pwd_marker(&result.stdout);

        assert_ne!(result.exit_code, 0);
        assert!(pwd_after.is_some());
    }

    #[test]
    fn last_impact_step_tracks_post_mutation_assessment() {
        let messages = vec![
            json!({"role":"assistant","content":"","tool_calls":[{"id":"call_1","type":"function","function":{"name":"write_file","arguments":"{\"path\":\"a.txt\",\"content\":\"x\"}"}}]}),
            json!({"role":"tool","tool_call_id":"call_1","content":"OK\n[hash] before=0 after=1"}),
            json!({"role":"assistant","content":"<impact>\nchanged: wrote a.txt\nprogress: step 2 moved\nremaining_gap: run cargo check\n</impact>\n"}),
        ];

        assert_eq!(last_impact_step_from_messages(&messages), Some(1));
    }

    #[test]
    fn parse_realize_block_extracts_reason() {
        let text = "<realize>\nreason: enough evidence to commit this plan\n</realize>";
        assert_eq!(
            parse_realize_block(text).as_deref(),
            Some("enough evidence to commit this plan")
        );
    }

    #[test]
    fn non_done_tool_realizes_latent_but_done_does_not() {
        let read = ToolCallData {
            id: "call_read".to_string(),
            name: "read_file".to_string(),
            arguments: "{\"path\":\"src/tui/events.rs\"}".to_string(),
        };
        let done = ToolCallData {
            id: "call_done".to_string(),
            name: "done".to_string(),
            arguments: "{\"summary\":\"ok\",\"completed_acceptance\":[],\"remaining_acceptance\":[],\"acceptance_evidence\":[]}".to_string(),
        };

        assert!(tool_call_realizes_latent(None, Some(&read)));
        assert!(!tool_call_realizes_latent(None, Some(&done)));
        assert!(tool_call_realizes_latent(
            Some("enough evidence to commit"),
            Some(&done)
        ));
    }

    #[test]
    fn build_realize_done_gate_message_mentions_commit_before_done() {
        let latent = LatentPlanBuffer {
            raw_text: "<plan>...</plan>".to_string(),
            plan: PlanBlock {
                goal: "Locate the /realize handler".to_string(),
                steps: vec!["search for /realize".to_string()],
                acceptance_criteria: vec!["identify the handler file".to_string()],
                risks: "wrong file".to_string(),
                assumptions: "observation tools are enough".to_string(),
            },
            summary: "goal: locate /realize; steps: search | read".to_string(),
            latest_intent: Some("tool: read_file; next: inspect src/tui/events.rs".to_string()),
            anchor_baseline: "goal: locate /realize".to_string(),
            created_iter: 2,
            defer_score: 0.62,
            tail_updates: 1,
        };

        let msg = build_realize_done_gate_message(&latent);
        assert!(msg.contains("[Realize Gate]"));
        assert!(msg.contains("before `done`"));
        assert!(msg.contains("goal: locate /realize"));
        assert!(msg.contains("<realize>reason: enough evidence to commit</realize>"));
    }

    #[test]
    fn strip_known_latent_tags_preserves_surrounding_text() {
        let text = "before\n<plan>\ngoal: x\nsteps: 1) a 2) b\nacceptance: 1) ok\nrisks: r\nassumptions: a\n</plan>\nafter";
        assert_eq!(strip_known_latent_tags(text), "before\nafter");
    }

    #[test]
    fn cosine_drift_is_lower_for_nearby_text() {
        let anchor = "fix repo map fallback for read_file and list_dir";
        let near = "fix repo map fallback for read_file";
        let far = "compose a critique about unrelated observer prose";
        assert!(cosine_token_distance(anchor, near) < cosine_token_distance(anchor, far));
    }

    #[test]
    fn latest_intent_anchor_baseline_reads_system_anchor() {
        let messages = vec![
            json!({"role":"system","content":"base system"}),
            json!({"role":"system","content":"[Intent Anchor]\nrevision: 2\nbaseline: goal: locate slash handler | target: src/tui/events.rs | constraints: do not edit | success: include file path | opt: -\n"}),
            json!({"role":"user","content":"continue"}),
        ];
        assert_eq!(
            latest_intent_anchor_baseline(&messages).as_deref(),
            Some(
                "goal: locate slash handler | target: src/tui/events.rs | constraints: do not edit | success: include file path | opt: -"
            )
        );
    }

    #[test]
    fn build_anchor_baseline_prefers_intent_anchor_over_root_prompt() {
        let messages = vec![json!({
            "role":"system",
            "content":"[Intent Anchor]\nrevision: 3\nbaseline: goal: stabilize coder loop | target: src/tui/agent.rs | constraints: keep scope narrow | success: preserve user intent | opt: improve readability\n"
        })];
        let baseline = build_anchor_baseline(
            &messages,
            "something vague like make it better",
            None,
            &WorkingMemory::default(),
        );
        assert!(baseline.contains("goal: stabilize coder loop"));
        assert!(!baseline.contains("something vague like make it better"));
    }

    #[test]
    fn repair_mistral_plan_for_tool_turn_fills_read_only_observation_shape() {
        let parsed = PlanBlock {
            goal: "Locate the exact file and context where `/realize` is handled in the TUI."
                .to_string(),
            steps: Vec::new(),
            acceptance_criteria: Vec::new(),
            risks: "unknown risks; inspect carefully before editing".to_string(),
            assumptions: "current repo scan reflects the active workspace".to_string(),
        };
        let tc = ToolCallData {
            id: "call_1".to_string(),
            name: "read_file".to_string(),
            arguments: "{\"path\":\"src/tui/events.rs\"}".to_string(),
        };
        let repaired = repair_mistral_plan_for_tool_turn(
            Some(&parsed),
            None,
            &tc,
            "Locate where the /realize slash command is handled in the TUI. Do not edit anything.",
            true,
        )
        .expect("repaired plan");
        assert!(repaired.steps.len() >= 2);
        assert!(!repaired.acceptance_criteria.is_empty());
        assert!(repaired.goal.contains("/realize"));
    }

    #[test]
    fn select_think_for_tool_turn_prefers_synth_over_invalid_last_for_mistral() {
        let plan = PlanBlock {
            goal: "Locate the /realize handler".to_string(),
            steps: vec![
                "search for /realize".to_string(),
                "read the matching file".to_string(),
            ],
            acceptance_criteria: vec!["report the file path".to_string()],
            risks: "wrong file".to_string(),
            assumptions: "observation tools are sufficient".to_string(),
        };
        let tc = ToolCallData {
            id: "call_2".to_string(),
            name: "done".to_string(),
            arguments: "{\"summary\":\"ok\",\"completed_acceptance\":[],\"remaining_acceptance\":[],\"acceptance_evidence\":[]}".to_string(),
        };
        let invalid_last = ThinkBlock {
            goal: String::new(),
            step: 1,
            tool: "done".to_string(),
            risk: "r".to_string(),
            doubt: "d".to_string(),
            next: "finish".to_string(),
            verify: "confirm".to_string(),
        };
        let synth = compat_synthetic_think(&tc, &plan);
        let (selected, used_synth) = select_think_for_tool_turn(
            None,
            Some(&invalid_last),
            Some(&synth),
            &plan,
            &tc,
            ProviderKind::Mistral,
        );
        assert!(used_synth);
        assert_eq!(selected.expect("selected think").goal, plan.goal);
    }

    #[test]
    fn select_think_for_tool_turn_prefers_synth_over_invalid_last_for_openai_compatible() {
        let plan = PlanBlock {
            goal: "Locate the failing test".to_string(),
            steps: vec![
                "search for the test".to_string(),
                "read the matching file".to_string(),
            ],
            acceptance_criteria: vec!["report the file path".to_string()],
            risks: "wrong file".to_string(),
            assumptions: "observation tools are sufficient".to_string(),
        };
        let tc = ToolCallData {
            id: "call_3".to_string(),
            name: "search_files".to_string(),
            arguments: "{\"pattern\":\"test\",\"dir\":\"src\"}".to_string(),
        };
        let invalid_last = ThinkBlock {
            goal: String::new(),
            step: 1,
            tool: "search_files".to_string(),
            risk: "r".to_string(),
            doubt: "d".to_string(),
            next: "search".to_string(),
            verify: "confirm".to_string(),
        };
        let synth = compat_synthetic_think(&tc, &plan);
        let (selected, used_synth) = select_think_for_tool_turn(
            None,
            Some(&invalid_last),
            Some(&synth),
            &plan,
            &tc,
            ProviderKind::OpenAiCompatible,
        );
        assert!(used_synth);
        assert_eq!(selected.expect("selected think").goal, plan.goal);
    }

    #[test]
    fn canonicalize_evidence_command_accepts_natural_language_search_and_read() {
        assert_eq!(
            canonicalize_evidence_command("search_files for `/realize` in `src/`"),
            "search_files(dir=src, pattern=/realize)"
        );
        assert_eq!(
            canonicalize_evidence_command("read_file of `src/tui/events.rs`"),
            "read_file(path=src/tui/events.rs)"
        );
        assert_eq!(
            canonicalize_evidence_command(
                "search_files with pattern \"/realize\" in src/ (succeeded with matches in src/tui/events.rs)"
            ),
            "search_files(dir=src, pattern=/realize)"
        );
        assert_eq!(
            canonicalize_evidence_command(
                "read_file src/tui/events.rs (succeeded with confirmation of handling logic)"
            ),
            "read_file(path=src/tui/events.rs)"
        );
    }

    #[test]
    fn resolution_memory_resolves_suffix_alias_to_canonical_path() {
        let mut evidence = ObservationEvidence::default();
        evidence.remember_resolution("tui/events.rs", "src/tui/events.rs", "repo_map:read_file");

        assert_eq!(
            evidence.resolve_path_alias("tui/events.rs").as_deref(),
            Some("src/tui/events.rs")
        );
        assert_eq!(
            evidence.resolve_path_alias("./tui/events.rs").as_deref(),
            Some("src/tui/events.rs")
        );
    }

    #[test]
    fn rewrite_tool_call_with_resolution_rewrites_read_file_path() {
        let mut evidence = ObservationEvidence::default();
        evidence.remember_resolution("tui/events.rs", "src/tui/events.rs", "repo_map:read_file");
        let tc = ToolCallData {
            id: "call_1".to_string(),
            name: "read_file".to_string(),
            arguments: serde_json::json!({"path":"tui/events.rs"}).to_string(),
        };

        let (rewritten, original, canonical) =
            rewrite_tool_call_with_resolution(&tc, &evidence).expect("rewritten");
        assert_eq!(original, "tui/events.rs");
        assert_eq!(canonical, "src/tui/events.rs");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&rewritten.arguments)
                .ok()
                .and_then(|v| v.get("path").and_then(|v| v.as_str()).map(str::to_string))
                .as_deref(),
            Some("src/tui/events.rs")
        );
    }

    #[test]
    fn realize_preset_parser_accepts_named_levels() {
        assert_eq!(
            "off".parse::<RealizePreset>().ok(),
            Some(RealizePreset::Off)
        );
        assert_eq!(
            "low".parse::<RealizePreset>().ok(),
            Some(RealizePreset::Low)
        );
        assert_eq!(
            "mid".parse::<RealizePreset>().ok(),
            Some(RealizePreset::Mid)
        );
        assert_eq!(
            "medium".parse::<RealizePreset>().ok(),
            Some(RealizePreset::Mid)
        );
        assert_eq!(
            "high".parse::<RealizePreset>().ok(),
            Some(RealizePreset::High)
        );
    }

    #[test]
    fn realize_preset_mid_matches_tui_default_shape() {
        let cfg = RealizeOnDemandConfig::resolve(Some(RealizePreset::Mid));
        assert!(cfg.enabled);
        assert_eq!(cfg.defer_threshold, 0.45);
        assert_eq!((cfg.window_start, cfg.window_end), (1, 3));
        assert_eq!(cfg.lambda_min, 0.15);
        assert_eq!(cfg.lambda_max, 0.90);
    }

    #[test]
    fn parses_evidence_block_fields() {
        let text = "\
<evidence>\n\
target_files: 1) src/tui/agent.rs\n\
target_symbols: 1) run_agentic_json\n\
evidence: read_file showed the recovery gate in src/tui/agent.rs\n\
open_questions: none\n\
next_probe: patch the recovery branch in run_agentic_json\n\
</evidence>\n";

        let block = parse_evidence_block(text).expect("evidence parsed");
        assert_eq!(block.target_files, vec!["src/tui/agent.rs".to_string()]);
        assert_eq!(block.target_symbols, vec!["run_agentic_json".to_string()]);
        assert_eq!(
            block.evidence,
            "read_file showed the recovery gate in src/tui/agent.rs"
        );
    }

    #[test]
    fn validate_evidence_block_accepts_observed_patch_target() {
        let evidence_block = EvidenceBlock {
            target_files: vec!["src/tui/agent.rs".to_string()],
            target_symbols: vec!["run_agentic_json".to_string()],
            evidence: "read_file confirmed the target branch in src/tui/agent.rs".to_string(),
            open_questions: "none".to_string(),
            next_probe: "patch the recovery branch".to_string(),
        };
        let tc = ToolCallData {
            id: "call_1".to_string(),
            name: "patch_file".to_string(),
            arguments: serde_json::json!({
                "path": "src/tui/agent.rs",
                "search": "old",
                "replace": "new"
            })
            .to_string(),
        };
        let observations = ObservationEvidence {
            reads: vec![ObservationReadEvidence {
                command: "read_file(path=src/tui/agent.rs)".to_string(),
                path: "src/tui/agent.rs".to_string(),
            }],
            searches: Vec::new(),
            resolutions: Vec::new(),
        };

        assert!(validate_evidence_block(&evidence_block, &tc, &observations).is_ok());
    }

    #[test]
    fn validate_evidence_block_rejects_unobserved_patch_target() {
        let evidence_block = EvidenceBlock {
            target_files: vec!["src/tui/agent.rs".to_string()],
            target_symbols: vec!["run_agentic_json".to_string()],
            evidence: "assumed the recovery branch lived there".to_string(),
            open_questions: "exact line span".to_string(),
            next_probe: "patch the recovery branch".to_string(),
        };
        let tc = ToolCallData {
            id: "call_1".to_string(),
            name: "patch_file".to_string(),
            arguments: serde_json::json!({
                "path": "src/tui/agent.rs",
                "search": "old",
                "replace": "new"
            })
            .to_string(),
        };
        let observations = ObservationEvidence {
            reads: vec![ObservationReadEvidence {
                command: "read_file(path=src/server.rs)".to_string(),
                path: "src/server.rs".to_string(),
            }],
            searches: Vec::new(),
            resolutions: Vec::new(),
        };

        let err = validate_evidence_block(&evidence_block, &tc, &observations)
            .expect_err("target without observation should fail");
        assert!(err.to_string().contains("lacks prior read/search evidence"));
    }

    #[test]
    fn build_task_contract_prompt_mentions_read_only_constraint() {
        let contract = derive_task_contract(
            "Locate the slash command handler without editing files",
            true,
            false,
            VerificationLevel::Build,
        );
        let prompt = build_task_contract_prompt(&contract, None, VerificationLevel::Build);
        assert!(prompt.contains("inspection-only: do not edit files"));
    }

    #[test]
    fn validate_plan_against_task_contract_rejects_drift() {
        let contract = derive_task_contract(
            "Fix the reflection gate in src/tui/agent.rs",
            false,
            true,
            VerificationLevel::Behavioral,
        );
        let plan = PlanBlock {
            goal: "rewrite README prose".to_string(),
            steps: vec!["edit README".to_string(), "proofread docs".to_string()],
            acceptance_criteria: vec!["README wording is updated".to_string()],
            risks: "missing context".to_string(),
            assumptions: "docs are the target".to_string(),
        };

        assert!(validate_plan_against_task_contract(&plan, &contract).is_err());
    }

    #[test]
    fn instruction_priority_uses_authority_then_explicit_then_locality_then_sequence() {
        let root = InstructionPriority {
            authority: InstructionAuthority::Root,
            explicit: false,
            locality: 0,
            sequence: 1,
        };
        let execution = InstructionPriority {
            authority: InstructionAuthority::Execution,
            explicit: true,
            locality: 4,
            sequence: 99,
        };
        let more_local = InstructionPriority {
            authority: InstructionAuthority::System,
            explicit: true,
            locality: 3,
            sequence: 1,
        };
        let less_local = InstructionPriority {
            authority: InstructionAuthority::System,
            explicit: true,
            locality: 2,
            sequence: 9,
        };
        let newer = InstructionPriority {
            authority: InstructionAuthority::System,
            explicit: true,
            locality: 3,
            sequence: 2,
        };

        assert!(root.outranks(&execution));
        assert!(more_local.outranks(&less_local));
        assert!(newer.outranks(&more_local));
    }

    #[test]
    fn instruction_resolver_blocks_read_only_mutation_tool() {
        let resolver =
            InstructionResolver::new("Locate the handler without editing files", true, false);
        let tc = ToolCallData {
            id: "call_1".to_string(),
            name: "patch_file".to_string(),
            arguments: serde_json::json!({
                "path": "src/tui/agent.rs",
                "search": "old",
                "replace": "new"
            })
            .to_string(),
        };

        let conflict = resolver
            .tool_conflict(&tc, None)
            .expect("read-only mutation should conflict");
        assert!(conflict
            .render()
            .contains("Higher-authority instruction wins"));
        assert!(conflict.render().contains("system/task_contract"));
    }

    #[test]
    fn instruction_resolver_rejects_read_only_plan_violation() {
        let resolver =
            InstructionResolver::new("Locate the handler without editing files", true, false);
        let plan = PlanBlock {
            goal: "patch the slash handler".to_string(),
            steps: vec![
                "edit src/tui/events.rs".to_string(),
                "run cargo test".to_string(),
            ],
            acceptance_criteria: vec!["handler is patched".to_string()],
            risks: "wrong file".to_string(),
            assumptions: "editing is allowed".to_string(),
        };

        let err = validate_plan_against_instruction_resolver(&plan, &resolver)
            .expect_err("read-only plan should be rejected");
        assert!(err.to_string().contains("[Instruction Resolver]"));
    }

    #[test]
    fn assumption_ledger_marks_refuted_and_confirms_from_working_memory() {
        let mut ledger = AssumptionLedger::default();
        ledger.remember_unknown("reflection prompt exists");
        ledger.mark_refuted(
            "cargo check works unchanged",
            Some("run targeted tests instead"),
        );

        let mut mem = WorkingMemory::default();
        mem.remember_fact("reflection prompt exists in src/tui/agent.rs");
        ledger.refresh_confirmations(&mem);

        assert!(ledger.entries.iter().any(|entry| {
            entry.text == "reflection prompt exists" && entry.status == AssumptionStatus::Confirmed
        }));
        assert!(ledger.entries.iter().any(|entry| {
            entry.text == "cargo check works unchanged" && entry.status == AssumptionStatus::Refuted
        }));
    }

    #[test]
    fn refuted_assumption_conflict_blocks_reuse() {
        let mut ledger = AssumptionLedger::default();
        ledger.mark_refuted("cargo check works unchanged", Some("exit code was 1"));

        let think = ThinkBlock {
            goal: "verify the build quickly".to_string(),
            step: 2,
            tool: "exec".to_string(),
            risk: "same build failure".to_string(),
            doubt: "might still fail".to_string(),
            next: "cargo check".to_string(),
            verify: "exit code is zero".to_string(),
        };
        let tc = ToolCallData {
            id: "call_1".to_string(),
            name: "exec".to_string(),
            arguments: serde_json::json!({"command":"cargo check"}).to_string(),
        };

        let msg = refuted_assumption_conflict(&ledger, &think, &tc)
            .expect("refuted assumption should conflict");
        assert!(msg.contains("cargo check works unchanged"));
    }

    #[test]
    fn render_cached_prompt_uses_compact_after_unchanged_full_prompt() {
        let mut slot = None;
        let full = "[Task Contract]
- task: tighten token usage"
            .to_string();
        let compact = "[Task Contract cache]
hash: deadbeef"
            .to_string();

        assert_eq!(
            render_cached_prompt(&mut slot, full.clone(), compact.clone()),
            full
        );
        assert_eq!(
            render_cached_prompt(&mut slot, full.clone(), compact.clone()),
            compact
        );
        assert_eq!(
            render_cached_prompt(
                &mut slot,
                "[Task Contract]
- task: changed"
                    .to_string(),
                compact,
            ),
            "[Task Contract]
- task: changed"
        );
    }

    #[test]
    fn prune_old_assistant_messages_compacts_stale_structured_turns() {
        let mut messages = Vec::new();
        for idx in 0..(KEEP_RECENT_ASSISTANT_TURNS + 1) {
            messages.push(json!({
                "role": "assistant",
                "content": format!(
                    "<plan>
goal: inspect token flow {idx}
steps: 1) inspect
acceptance: 1) token flow understood
risks: drift
assumptions: cache exists
</plan>"
                )
            }));
        }

        let original_last = messages
            .last()
            .and_then(|msg| msg["content"].as_str())
            .unwrap()
            .to_string();
        prune_old_assistant_messages(&mut messages);

        assert!(messages[0]["content"]
            .as_str()
            .unwrap_or("")
            .contains("[assistant-summary]"));
        assert_eq!(
            messages
                .last()
                .and_then(|msg| msg["content"].as_str())
                .unwrap_or(""),
            original_last
        );
    }
}
