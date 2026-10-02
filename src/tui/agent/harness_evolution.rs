use super::evaluator_loop::{EvaluatorBlockScope, EvaluatorLoop};
use super::meta_harness::MetaHarness;
use super::task_harness::TaskHarness;
use super::*;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};

const MAX_PROPOSALS: usize = 16;
const MAX_PROMOTED_POLICIES: usize = 16;

fn now_ms() -> u128 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn blocked_scope_label(scope: EvaluatorBlockScope) -> &'static str {
    match scope {
        EvaluatorBlockScope::ExactRepeatOnly => "exact_repeat_only",
        EvaluatorBlockScope::AnyBlockedTool => "any_blocked_tool",
    }
}

fn proposal_id(task_harness: TaskHarness, policy_id: &str) -> String {
    format!("{}::{policy_id}", task_harness.lane_label())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct ContractPatchProposal {
    pub id: String,
    pub policy_id: String,
    pub source: String,
    pub lane: String,
    pub artifact_mode: String,
    pub pattern: String,
    pub policy_action: String,
    pub required_action: String,
    pub preferred_tools: Vec<String>,
    pub blocked_tools: Vec<String>,
    pub blocked_scope: String,
    #[serde(default)]
    pub blocked_command_display: Option<String>,
    #[serde(default)]
    pub blocked_command_signature: Option<String>,
    #[serde(default)]
    pub next_target: Option<String>,
    pub exit_hint: String,
    #[serde(default)]
    pub support_note: Option<String>,
    #[serde(default)]
    pub evidence_count: usize,
    #[serde(default)]
    pub seen_count: u32,
    #[serde(default)]
    pub applied_count: u32,
    #[serde(default)]
    pub promotion_ready: bool,
    #[serde(default)]
    pub first_seen_ms: u128,
    #[serde(default)]
    pub last_seen_ms: u128,
    #[serde(default)]
    pub last_applied_ms: u128,
}

impl ContractPatchProposal {
    pub(super) fn from_signals(
        task_harness: TaskHarness,
        meta_harness: &MetaHarness,
        evaluator_loop: &EvaluatorLoop,
    ) -> Option<Self> {
        let policy = meta_harness.policy()?;
        let patch = evaluator_loop.patch()?;
        Some(Self {
            id: proposal_id(task_harness, patch.id),
            policy_id: patch.id.to_string(),
            source: "meta_harness+evaluator_loop".to_string(),
            lane: task_harness.lane_label().to_string(),
            artifact_mode: task_harness.artifact_mode_label().to_string(),
            pattern: policy.pattern.label().to_string(),
            policy_action: policy.action.label().to_string(),
            required_action: policy.action.required_action_hint().to_string(),
            preferred_tools: patch
                .preferred_tools
                .iter()
                .map(|tool| (*tool).to_string())
                .collect(),
            blocked_tools: patch
                .blocked_tools
                .iter()
                .map(|tool| (*tool).to_string())
                .collect(),
            blocked_scope: blocked_scope_label(patch.blocked_scope).to_string(),
            blocked_command_display: patch.blocked_command_display.clone(),
            blocked_command_signature: patch.blocked_command_signature.clone(),
            next_target: policy.next_target.clone(),
            exit_hint: patch.exit_hint.clone(),
            support_note: patch.support_note.clone(),
            evidence_count: policy.evidence_count,
            seen_count: 0,
            applied_count: 0,
            promotion_ready: false,
            first_seen_ms: 0,
            last_seen_ms: 0,
            last_applied_ms: 0,
        })
    }

    fn refresh_promotion_ready(&mut self) {
        self.promotion_ready = self.seen_count >= 2 && self.applied_count >= 1;
    }

    pub(super) fn telemetry_payload(&self, was_new: bool) -> Value {
        serde_json::json!({
            "id": self.id,
            "policy_id": self.policy_id,
            "lane": self.lane,
            "artifact_mode": self.artifact_mode,
            "pattern": self.pattern,
            "policy_action": self.policy_action,
            "preferred_tools": self.preferred_tools,
            "blocked_tools": self.blocked_tools,
            "next_target": self.next_target,
            "evidence_count": self.evidence_count,
            "seen_count": self.seen_count,
            "applied_count": self.applied_count,
            "promotion_ready": self.promotion_ready,
            "was_new": was_new,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ProposalUpdate {
    pub proposal: ContractPatchProposal,
    pub was_new: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct HarnessEvolutionQueue {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub updated_at_ms: u128,
    #[serde(default)]
    proposals: Vec<ContractPatchProposal>,
}

fn default_version() -> u32 {
    1
}

impl HarnessEvolutionQueue {
    pub const VERSION: u32 = 1;

    pub(crate) fn proposal_count(&self) -> usize {
        self.proposals.len()
    }

    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let text = std::fs::read_to_string(path).with_context(|| {
            format!("failed to read harness evolution queue: {}", path.display())
        })?;
        let mut queue: Self = serde_json::from_str(&text).with_context(|| {
            format!(
                "failed to parse harness evolution queue: {}",
                path.display()
            )
        })?;
        if queue.version != Self::VERSION {
            anyhow::bail!(
                "unsupported harness evolution queue version {} (expected {})",
                queue.version,
                Self::VERSION
            );
        }
        queue.sort_entries();
        Ok(queue)
    }

    pub fn save_atomic(&self, path: &Path) -> Result<()> {
        let json = serde_json::to_string_pretty(self)
            .context("failed to serialize harness evolution queue")?;
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        std::fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create harness evolution dir: {}",
                parent.display()
            )
        })?;
        let tmp = path.with_extension(format!("tmp.{}", std::process::id()));
        std::fs::write(&tmp, json.as_bytes()).with_context(|| {
            format!(
                "failed to write temp harness evolution queue: {}",
                tmp.display()
            )
        })?;
        std::fs::rename(&tmp, path).with_context(|| {
            format!(
                "failed to replace harness evolution queue {} -> {}",
                tmp.display(),
                path.display()
            )
        })?;
        Ok(())
    }

    pub(super) fn remember(&mut self, proposal: ContractPatchProposal) -> ProposalUpdate {
        let now = now_ms();
        let proposal_id = proposal.id.clone();
        let mut was_new = false;
        if let Some(existing) = self
            .proposals
            .iter_mut()
            .find(|entry| entry.id == proposal.id)
        {
            existing.policy_id = proposal.policy_id;
            existing.source = proposal.source;
            existing.lane = proposal.lane;
            existing.artifact_mode = proposal.artifact_mode;
            existing.pattern = proposal.pattern;
            existing.policy_action = proposal.policy_action;
            existing.required_action = proposal.required_action;
            existing.preferred_tools = proposal.preferred_tools;
            existing.blocked_tools = proposal.blocked_tools;
            existing.blocked_scope = proposal.blocked_scope;
            existing.blocked_command_display = proposal.blocked_command_display;
            existing.blocked_command_signature = proposal.blocked_command_signature;
            existing.next_target = proposal.next_target;
            existing.exit_hint = proposal.exit_hint;
            existing.support_note = proposal.support_note;
            existing.evidence_count = existing.evidence_count.max(proposal.evidence_count);
            existing.seen_count = existing.seen_count.saturating_add(1);
            existing.last_seen_ms = now;
            existing.refresh_promotion_ready();
        } else {
            let mut proposal = proposal;
            proposal.seen_count = 1;
            proposal.first_seen_ms = now;
            proposal.last_seen_ms = now;
            proposal.refresh_promotion_ready();
            self.proposals.push(proposal);
            was_new = true;
        }
        self.updated_at_ms = now;
        self.sort_entries();
        ProposalUpdate {
            proposal: self
                .proposals
                .iter()
                .find(|entry| entry.id == proposal_id)
                .cloned()
                .expect("proposal exists after remember"),
            was_new,
        }
    }

    pub(super) fn mark_prompted(&mut self, ids: &[String]) -> bool {
        let now = now_ms();
        let mut changed = false;
        for id in ids {
            if let Some(entry) = self.proposals.iter_mut().find(|entry| entry.id == *id) {
                entry.applied_count = entry.applied_count.saturating_add(1);
                entry.last_applied_ms = now;
                entry.refresh_promotion_ready();
                changed = true;
            }
        }
        if changed {
            self.updated_at_ms = now;
            self.sort_entries();
        }
        changed
    }

    pub(super) fn build_overlay(
        &self,
        task_harness: TaskHarness,
        current_proposal: Option<&ContractPatchProposal>,
    ) -> Option<RuntimePolicyOverlay> {
        let current_id = current_proposal.map(|proposal| proposal.id.as_str());
        let lane = task_harness.lane_label();
        let mut entries: Vec<&ContractPatchProposal> = self
            .proposals
            .iter()
            .filter(|proposal| proposal.lane == lane || Some(proposal.id.as_str()) == current_id)
            .collect();
        if entries.is_empty() {
            return None;
        }
        entries.sort_by_key(|proposal| {
            (
                std::cmp::Reverse(Some(proposal.id.as_str()) == current_id),
                std::cmp::Reverse(proposal.promotion_ready),
                std::cmp::Reverse(proposal.seen_count),
                std::cmp::Reverse(proposal.applied_count),
                std::cmp::Reverse(proposal.last_seen_ms),
            )
        });
        entries.truncate(2);
        Some(RuntimePolicyOverlay::from_entries(entries, current_id))
    }

    fn sort_entries(&mut self) {
        self.proposals.sort_by_key(|proposal| {
            (
                std::cmp::Reverse(proposal.promotion_ready),
                std::cmp::Reverse(proposal.seen_count),
                std::cmp::Reverse(proposal.applied_count),
                std::cmp::Reverse(proposal.last_seen_ms),
            )
        });
        if self.proposals.len() > MAX_PROPOSALS {
            self.proposals.truncate(MAX_PROPOSALS);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RuntimePolicyOverlay {
    pub active_ids: Vec<String>,
    pub full_prompt: String,
    pub compact_prompt: String,
    pub promotion_ready_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct PromotedPolicyPatch {
    pub id: String,
    pub policy_id: String,
    pub lane: String,
    pub artifact_mode: String,
    pub pattern: String,
    pub policy_action: String,
    pub required_action: String,
    pub preferred_tools: Vec<String>,
    pub blocked_tools: Vec<String>,
    pub blocked_scope: String,
    #[serde(default)]
    pub blocked_command_display: Option<String>,
    #[serde(default)]
    pub next_target: Option<String>,
    pub exit_hint: String,
    #[serde(default)]
    pub support_note: Option<String>,
    #[serde(default)]
    pub evidence_count: usize,
    #[serde(default)]
    pub seen_count: u32,
    #[serde(default)]
    pub applied_count: u32,
    #[serde(default)]
    pub green_case_ids: Vec<String>,
    #[serde(default)]
    pub promoted_at_ms: u128,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct GovernorContractOverlay {
    #[serde(default = "default_overlay_version")]
    pub version: u32,
    #[serde(default)]
    pub updated_at_ms: u128,
    #[serde(default)]
    pub promoted_policies: Vec<PromotedPolicyPatch>,
}

fn default_overlay_version() -> u32 {
    1
}

impl GovernorContractOverlay {
    pub const VERSION: u32 = 1;

    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let text = std::fs::read_to_string(path).with_context(|| {
            format!(
                "failed to read governor contract overlay: {}",
                path.display()
            )
        })?;
        let mut overlay: Self = serde_json::from_str(&text).with_context(|| {
            format!(
                "failed to parse governor contract overlay: {}",
                path.display()
            )
        })?;
        if overlay.version != Self::VERSION {
            anyhow::bail!(
                "unsupported governor contract overlay version {} (expected {})",
                overlay.version,
                Self::VERSION
            );
        }
        overlay.sort_entries();
        Ok(overlay)
    }

    pub fn save_atomic(&self, path: &Path) -> Result<()> {
        let json = serde_json::to_string_pretty(self)
            .context("failed to serialize governor contract overlay")?;
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        std::fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create governor contract overlay dir: {}",
                parent.display()
            )
        })?;
        let tmp = path.with_extension(format!("tmp.{}", std::process::id()));
        std::fs::write(&tmp, json.as_bytes()).with_context(|| {
            format!(
                "failed to write temp governor contract overlay: {}",
                tmp.display()
            )
        })?;
        std::fs::rename(&tmp, path).with_context(|| {
            format!(
                "failed to replace governor contract overlay {} -> {}",
                tmp.display(),
                path.display()
            )
        })?;
        Ok(())
    }

    pub(crate) fn promote_from_queue(
        &mut self,
        queue: &HarnessEvolutionQueue,
        case_id: &str,
    ) -> bool {
        let now = now_ms();
        let mut changed = false;
        for proposal in queue
            .proposals
            .iter()
            .filter(|proposal| proposal.promotion_ready)
        {
            if let Some(existing) = self
                .promoted_policies
                .iter_mut()
                .find(|policy| policy.id == proposal.id)
            {
                existing.policy_id = proposal.policy_id.clone();
                existing.lane = proposal.lane.clone();
                existing.artifact_mode = proposal.artifact_mode.clone();
                existing.pattern = proposal.pattern.clone();
                existing.policy_action = proposal.policy_action.clone();
                existing.required_action = proposal.required_action.clone();
                existing.preferred_tools = proposal.preferred_tools.clone();
                existing.blocked_tools = proposal.blocked_tools.clone();
                existing.blocked_scope = proposal.blocked_scope.clone();
                existing.blocked_command_display = proposal.blocked_command_display.clone();
                existing.next_target = proposal.next_target.clone();
                existing.exit_hint = proposal.exit_hint.clone();
                existing.support_note = proposal.support_note.clone();
                existing.evidence_count = existing.evidence_count.max(proposal.evidence_count);
                existing.seen_count = existing.seen_count.max(proposal.seen_count);
                existing.applied_count = existing.applied_count.max(proposal.applied_count);
                if !existing.green_case_ids.iter().any(|id| id == case_id) {
                    existing.green_case_ids.push(case_id.to_string());
                    existing.green_case_ids.sort();
                }
                existing.promoted_at_ms = now;
                changed = true;
            } else {
                self.promoted_policies.push(PromotedPolicyPatch {
                    id: proposal.id.clone(),
                    policy_id: proposal.policy_id.clone(),
                    lane: proposal.lane.clone(),
                    artifact_mode: proposal.artifact_mode.clone(),
                    pattern: proposal.pattern.clone(),
                    policy_action: proposal.policy_action.clone(),
                    required_action: proposal.required_action.clone(),
                    preferred_tools: proposal.preferred_tools.clone(),
                    blocked_tools: proposal.blocked_tools.clone(),
                    blocked_scope: proposal.blocked_scope.clone(),
                    blocked_command_display: proposal.blocked_command_display.clone(),
                    next_target: proposal.next_target.clone(),
                    exit_hint: proposal.exit_hint.clone(),
                    support_note: proposal.support_note.clone(),
                    evidence_count: proposal.evidence_count,
                    seen_count: proposal.seen_count,
                    applied_count: proposal.applied_count,
                    green_case_ids: vec![case_id.to_string()],
                    promoted_at_ms: now,
                });
                changed = true;
            }
        }
        if changed {
            self.updated_at_ms = now;
            self.sort_entries();
        }
        changed
    }

    pub(super) fn telemetry_payload(&self, task_harness: TaskHarness) -> Option<Value> {
        let lane = task_harness.lane_label();
        let active_count = self
            .promoted_policies
            .iter()
            .filter(|policy| policy.lane == lane)
            .count();
        if active_count == 0 {
            return None;
        }
        Some(serde_json::json!({
            "lane": lane,
            "active_count": active_count,
            "promoted_policies": self.promoted_policies.len(),
        }))
    }

    pub(super) fn prompt(&self, task_harness: TaskHarness) -> Option<String> {
        let lane = task_harness.lane_label();
        let mut entries: Vec<&PromotedPolicyPatch> = self
            .promoted_policies
            .iter()
            .filter(|policy| policy.lane == lane)
            .collect();
        if entries.is_empty() {
            return None;
        }
        entries.sort_by_key(|policy| {
            (
                std::cmp::Reverse(policy.green_case_ids.len()),
                std::cmp::Reverse(policy.seen_count),
                std::cmp::Reverse(policy.applied_count),
                std::cmp::Reverse(policy.promoted_at_ms),
            )
        });
        entries.truncate(2);
        let mut full = String::from("[Harness Evolution Promoted Overlay]\n");
        full.push_str(
            "These guardrails were promoted after passing runtime eval cases.\n\
Treat them as high-confidence defaults unless current tool output contradicts them.\n",
        );
        for policy in entries {
            full.push_str(&format!(
                "- promoted_id: {}\n  policy_id: {}\n  pattern: {}\n  action: {}\n  required_now: {}\n  preferred_tools: {}\n  blocked_tools: {}\n  blocked_scope: {}\n  exit_hint: {}\n  green_cases: {}\n",
                policy.id,
                policy.policy_id,
                policy.pattern,
                policy.policy_action,
                compact_one_line(policy.required_action.as_str(), 180),
                policy.preferred_tools.join(", "),
                policy.blocked_tools.join(", "),
                policy.blocked_scope,
                compact_one_line(policy.exit_hint.as_str(), 180),
                policy.green_case_ids.join(", "),
            ));
            if let Some(command) = policy.blocked_command_display.as_deref() {
                full.push_str(&format!(
                    "  blocked_repeat: {}\n",
                    compact_one_line(command, 180)
                ));
            }
            if let Some(target) = policy.next_target.as_deref() {
                full.push_str(&format!(
                    "  next_target: {}\n",
                    compact_one_line(target, 180)
                ));
            }
        }
        Some(full)
    }

    pub(super) fn compact_prompt(&self, task_harness: TaskHarness) -> Option<String> {
        let lane = task_harness.lane_label();
        let mut entries: Vec<&PromotedPolicyPatch> = self
            .promoted_policies
            .iter()
            .filter(|policy| policy.lane == lane)
            .collect();
        if entries.is_empty() {
            return None;
        }
        entries.sort_by_key(|policy| {
            (
                std::cmp::Reverse(policy.green_case_ids.len()),
                std::cmp::Reverse(policy.promoted_at_ms),
            )
        });
        let mut lines = vec![
            "[Harness Evolution promoted cache]".to_string(),
            format!("lane: {lane}"),
            format!("promoted: {}", entries.len()),
        ];
        for policy in entries.into_iter().take(2) {
            lines.push(format!(
                "- {} => {} (green_cases={})",
                policy.policy_id,
                policy.preferred_tools.join(", "),
                policy.green_case_ids.len()
            ));
        }
        Some(lines.join("\n"))
    }

    fn sort_entries(&mut self) {
        self.promoted_policies.sort_by_key(|policy| {
            (
                std::cmp::Reverse(policy.green_case_ids.len()),
                std::cmp::Reverse(policy.seen_count),
                std::cmp::Reverse(policy.applied_count),
                std::cmp::Reverse(policy.promoted_at_ms),
            )
        });
        if self.promoted_policies.len() > MAX_PROMOTED_POLICIES {
            self.promoted_policies.truncate(MAX_PROMOTED_POLICIES);
        }
    }
}

impl RuntimePolicyOverlay {
    fn from_entries(entries: Vec<&ContractPatchProposal>, current_id: Option<&str>) -> Self {
        let mut full = String::from("[Harness Evolution Overlay]\n");
        full.push_str(
            "These runtime overlays were learned from recent trace failures for this repo.\n\
They are overlay-only rules for this run. Do NOT mutate `shared/governor_contract.json` from them automatically.\n",
        );
        let mut compact = vec![
            "[Harness Evolution cache]".to_string(),
            format!("overlays: {}", entries.len()),
        ];
        let mut active_ids = Vec::new();
        let mut promotion_ready_count = 0usize;
        for proposal in entries {
            active_ids.push(proposal.id.clone());
            if proposal.promotion_ready {
                promotion_ready_count += 1;
            }
            let current_marker = if Some(proposal.id.as_str()) == current_id {
                "yes"
            } else {
                "no"
            };
            full.push_str(&format!(
                "- overlay_id: {}\n  current: {}\n  policy_id: {}\n  pattern: {}\n  action: {}\n  required_now: {}\n  preferred_tools: {}\n  blocked_tools: {}\n  blocked_scope: {}\n  exit_hint: {}\n  meta: seen={} | applied={} | promotion_ready={}\n",
                proposal.id,
                current_marker,
                proposal.policy_id,
                proposal.pattern,
                proposal.policy_action,
                compact_one_line(proposal.required_action.as_str(), 180),
                proposal.preferred_tools.join(", "),
                proposal.blocked_tools.join(", "),
                proposal.blocked_scope,
                compact_one_line(proposal.exit_hint.as_str(), 180),
                proposal.seen_count,
                proposal.applied_count,
                proposal.promotion_ready,
            ));
            if let Some(command) = proposal.blocked_command_display.as_deref() {
                full.push_str(&format!(
                    "  blocked_repeat: {}\n",
                    compact_one_line(command, 180)
                ));
            }
            if let Some(target) = proposal.next_target.as_deref() {
                full.push_str(&format!(
                    "  next_target: {}\n",
                    compact_one_line(target, 180)
                ));
            }
            if let Some(note) = proposal.support_note.as_deref() {
                full.push_str(&format!("  support: {}\n", compact_one_line(note, 180)));
            }
            compact.push(format!(
                "- {} => {} (seen={} applied={} ready={})",
                proposal.policy_id,
                proposal.preferred_tools.join(", "),
                proposal.seen_count,
                proposal.applied_count,
                proposal.promotion_ready
            ));
        }
        full.push_str(
            "Treat these as runtime-updated guardrails. If current tool output contradicts them, trust the current evidence and let the next trace update the overlay.",
        );
        Self {
            active_ids,
            full_prompt: full,
            compact_prompt: compact.join("\n"),
            promotion_ready_count,
        }
    }

    pub(super) fn telemetry_payload(&self) -> Value {
        serde_json::json!({
            "active_ids": self.active_ids,
            "active_count": self.active_ids.len(),
            "promotion_ready_count": self.promotion_ready_count,
        })
    }
}

impl Default for HarnessEvolutionQueue {
    fn default() -> Self {
        Self {
            version: Self::VERSION,
            updated_at_ms: 0,
            proposals: Vec::new(),
        }
    }
}

impl Default for GovernorContractOverlay {
    fn default() -> Self {
        Self {
            version: Self::VERSION,
            updated_at_ms: 0,
            promoted_policies: Vec::new(),
        }
    }
}

pub(crate) fn path_for_root(root: &str) -> PathBuf {
    Path::new(root).join(".spiral-coder/policy_patch_queue.json")
}

pub(crate) fn overlay_path_for_root(root: &str) -> PathBuf {
    Path::new(root).join(".spiral-coder/governor_contract.overlay.json")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reflection_ledger::ReflectionLedger;
    use crate::tui::agent::meta_harness::{FailurePattern, PolicyAction, PolicyDelta};
    use crate::tui::agent::task_harness::{ArtifactMode, TaskLane};

    fn temp_path(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "spiral-coder_harness_evolution_{name}_{}_{}",
            std::process::id(),
            now_ms()
        ));
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir.join("policy_patch_queue.json")
    }

    fn fix_task_harness() -> TaskHarness {
        TaskHarness {
            lane: TaskLane::FixExisting,
            artifact_mode: ArtifactMode::ExistingFiles,
        }
    }

    fn init_repo_harness() -> TaskHarness {
        TaskHarness {
            lane: TaskLane::InitRepo,
            artifact_mode: ArtifactMode::NewRepo,
        }
    }

    fn fix_proposal() -> ContractPatchProposal {
        let policy = PolicyDelta {
            pattern: FailurePattern::RepeatedObservationLoop,
            action: PolicyAction::MutateExistingNow,
            evidence_count: 3,
            attempted_command: Some("read_file(path=src/lib.rs)".to_string()),
            next_target: Some("src/lib.rs".to_string()),
        };
        let meta = MetaHarness::for_test(policy);
        let evaluator = EvaluatorLoop::analyze(
            fix_task_harness(),
            &meta,
            &ReflectionLedger::default(),
            None,
            Some("cargo test 2>&1"),
            None,
            0,
        );
        ContractPatchProposal::from_signals(fix_task_harness(), &meta, &evaluator)
            .expect("fix proposal")
    }

    fn repo_proposal() -> ContractPatchProposal {
        let policy = PolicyDelta {
            pattern: FailurePattern::RepoScaffoldDrift,
            action: PolicyAction::AdvanceRepoScaffold,
            evidence_count: 2,
            attempted_command: Some("list_dir(path=demo_repo)".to_string()),
            next_target: Some("demo_repo/.git".to_string()),
        };
        let meta = MetaHarness::for_test(policy);
        let evaluator = EvaluatorLoop::analyze(
            init_repo_harness(),
            &meta,
            &ReflectionLedger::default(),
            None,
            None,
            None,
            0,
        );
        ContractPatchProposal::from_signals(init_repo_harness(), &meta, &evaluator)
            .expect("repo proposal")
    }

    #[test]
    fn derive_fix_proposal_from_meta_and_evaluator_signals() {
        let proposal = fix_proposal();
        assert_eq!(proposal.policy_id, "force_mutation_after_observation_loop");
        assert_eq!(proposal.lane, "fix_existing_files");
        assert!(proposal.preferred_tools.contains(&"patch_file".to_string()));
        assert_eq!(
            proposal.blocked_command_display.as_deref(),
            Some("read_file(path=src/lib.rs)")
        );
    }

    #[test]
    fn queue_roundtrip_preserves_overlay_state() {
        let path = temp_path("roundtrip");
        let mut queue = HarnessEvolutionQueue::default();
        let proposal = fix_proposal();
        let update = queue.remember(proposal.clone());
        assert_eq!(update.proposal.seen_count, 1);
        assert!(!update.proposal.promotion_ready);
        assert!(queue.mark_prompted(&[proposal.id]));
        queue.save_atomic(&path).expect("save queue");

        let loaded = HarnessEvolutionQueue::load(&path).expect("load queue");
        assert_eq!(loaded.proposals.len(), 1);
        assert_eq!(loaded.proposals[0].applied_count, 1);
    }

    #[test]
    fn promotion_ready_after_repeat_and_prompt_application() {
        let mut queue = HarnessEvolutionQueue::default();
        let proposal = fix_proposal();
        let id = proposal.id.clone();

        let first = queue.remember(proposal.clone());
        assert!(!first.proposal.promotion_ready);
        assert!(queue.mark_prompted(std::slice::from_ref(&id)));

        let second = queue.remember(proposal);
        assert!(second.proposal.promotion_ready);
        assert_eq!(second.proposal.seen_count, 2);
        assert_eq!(second.proposal.applied_count, 1);
    }

    #[test]
    fn overlay_focuses_current_lane() {
        let mut queue = HarnessEvolutionQueue::default();
        let fix = fix_proposal();
        let repo = repo_proposal();
        queue.remember(fix.clone());
        queue.remember(repo);

        let overlay = queue
            .build_overlay(fix_task_harness(), Some(&fix))
            .expect("overlay");
        assert_eq!(overlay.active_ids.len(), 1);
        assert!(overlay.active_ids[0].contains("fix_existing_files"));
        assert!(overlay
            .full_prompt
            .contains("force_mutation_after_observation_loop"));
        assert!(!overlay
            .full_prompt
            .contains("advance_repo_scaffold_artifact"));
    }

    #[test]
    fn promoted_overlay_roundtrip_and_prompt() {
        let path = temp_path("promoted");
        let mut queue = HarnessEvolutionQueue::default();
        let proposal = repo_proposal();
        let proposal_id = proposal.id.clone();
        queue.remember(proposal.clone());
        assert!(queue.mark_prompted(std::slice::from_ref(&proposal_id)));
        let promoted = queue.remember(proposal);
        assert!(promoted.proposal.promotion_ready);

        let mut overlay = GovernorContractOverlay::default();
        assert!(overlay.promote_from_queue(&queue, "init-repo-artifact"));
        overlay.save_atomic(&path).expect("save promoted overlay");

        let loaded = GovernorContractOverlay::load(&path).expect("load promoted overlay");
        assert_eq!(loaded.promoted_policies.len(), 1);
        let prompt = loaded.prompt(init_repo_harness()).expect("prompt");
        assert!(prompt.contains("Harness Evolution Promoted Overlay"));
        assert!(prompt.contains("init-repo-artifact"));
    }
}
