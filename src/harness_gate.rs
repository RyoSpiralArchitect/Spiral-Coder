use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::governor_contract::{self, RuntimeOverlayTemplate};
use crate::harness_promotion::{
    candidate_path_for_root, GovernorContractPromotionCandidate, GovernorContractPromotionEntry,
    PromotionDecision,
};

fn now_ms() -> u128 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HarnessPromotionReviewDecision {
    Approved,
    Held,
    Applied,
}

impl HarnessPromotionReviewDecision {
    fn label(self) -> &'static str {
        match self {
            HarnessPromotionReviewDecision::Approved => "approved",
            HarnessPromotionReviewDecision::Held => "held",
            HarnessPromotionReviewDecision::Applied => "applied",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HarnessPromotionBoardStatus {
    NeedsReview,
    Approved,
    Held,
    Applied,
    UpToDate,
    Blocked,
}

impl HarnessPromotionBoardStatus {
    fn rank(self) -> u8 {
        match self {
            HarnessPromotionBoardStatus::NeedsReview => 0,
            HarnessPromotionBoardStatus::Approved => 1,
            HarnessPromotionBoardStatus::Held => 2,
            HarnessPromotionBoardStatus::Applied => 3,
            HarnessPromotionBoardStatus::UpToDate => 4,
            HarnessPromotionBoardStatus::Blocked => 5,
        }
    }

    fn label(self) -> &'static str {
        match self {
            HarnessPromotionBoardStatus::NeedsReview => "needs_review",
            HarnessPromotionBoardStatus::Approved => "approved",
            HarnessPromotionBoardStatus::Held => "held",
            HarnessPromotionBoardStatus::Applied => "applied",
            HarnessPromotionBoardStatus::UpToDate => "up_to_date",
            HarnessPromotionBoardStatus::Blocked => "blocked",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HarnessPromotionReviewRecord {
    pub id: String,
    pub decision: HarnessPromotionReviewDecision,
    pub updated_at_ms: u128,
    #[serde(default)]
    pub applied_at_ms: Option<u128>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HarnessPromotionGateState {
    #[serde(default = "default_gate_version")]
    pub version: u32,
    #[serde(default)]
    pub updated_at_ms: u128,
    #[serde(default)]
    pub reviews: BTreeMap<String, HarnessPromotionReviewRecord>,
}

fn default_gate_version() -> u32 {
    1
}

impl HarnessPromotionGateState {
    pub const VERSION: u32 = 1;

    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let text = std::fs::read_to_string(path).with_context(|| {
            format!("failed to read harness promotion gate: {}", path.display())
        })?;
        let state: Self = serde_json::from_str(&text).with_context(|| {
            format!("failed to parse harness promotion gate: {}", path.display())
        })?;
        if state.version != Self::VERSION {
            anyhow::bail!(
                "unsupported harness promotion gate version {} (expected {})",
                state.version,
                Self::VERSION
            );
        }
        Ok(state)
    }

    pub fn save_atomic(&self, path: &Path) -> Result<()> {
        let json = serde_json::to_string_pretty(self)
            .context("failed to serialize harness promotion gate")?;
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        std::fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create harness promotion gate dir: {}",
                parent.display()
            )
        })?;
        let tmp = path.with_extension(format!("tmp.{}.{}", std::process::id(), now_ms()));
        std::fs::write(&tmp, json.as_bytes()).with_context(|| {
            format!(
                "failed to write temp harness promotion gate: {}",
                tmp.display()
            )
        })?;
        std::fs::rename(&tmp, path).with_context(|| {
            format!(
                "failed to replace harness promotion gate {} -> {}",
                tmp.display(),
                path.display()
            )
        })?;
        Ok(())
    }

    fn set_decision(&mut self, id: &str, decision: HarnessPromotionReviewDecision) {
        let ts = now_ms();
        self.updated_at_ms = ts;
        let record = self
            .reviews
            .entry(id.to_string())
            .or_insert(HarnessPromotionReviewRecord {
                id: id.to_string(),
                decision,
                updated_at_ms: ts,
                applied_at_ms: None,
            });
        record.decision = decision;
        record.updated_at_ms = ts;
        if matches!(decision, HarnessPromotionReviewDecision::Applied) {
            record.applied_at_ms = Some(ts);
        }
    }
}

impl Default for HarnessPromotionGateState {
    fn default() -> Self {
        Self {
            version: Self::VERSION,
            updated_at_ms: 0,
            reviews: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HarnessPromotionBoardSummary {
    pub total: usize,
    pub needs_review: usize,
    pub approved: usize,
    pub held: usize,
    pub applied: usize,
    pub up_to_date: usize,
    pub blocked: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HarnessPromotionBoardEntry {
    pub id: String,
    pub decision: PromotionDecision,
    pub review_status: HarnessPromotionBoardStatus,
    pub review_badge: String,
    pub can_approve: bool,
    pub can_hold: bool,
    pub can_apply: bool,
    #[serde(default)]
    pub review_record: Option<HarnessPromotionReviewRecord>,
    pub title: String,
    pub subtitle: String,
    pub badge: String,
    pub contract_path: String,
    pub reasons: Vec<String>,
    pub green_case_ids: Vec<String>,
    #[serde(default)]
    pub existing_template: Option<RuntimeOverlayTemplate>,
    pub proposed_template: RuntimeOverlayTemplate,
    #[serde(default)]
    pub patch_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HarnessPromotionBoard {
    pub version: u32,
    pub workspace_root: String,
    pub candidate_path: String,
    pub gate_path: String,
    #[serde(default)]
    pub contract_path: Option<String>,
    #[serde(default)]
    pub candidate_generated_at_ms: Option<u128>,
    #[serde(default)]
    pub status_message: Option<String>,
    pub summary: HarnessPromotionBoardSummary,
    pub entries: Vec<HarnessPromotionBoardEntry>,
}

impl HarnessPromotionBoard {
    pub const VERSION: u32 = 1;

    fn empty(root: &Path, candidate_path: &Path, gate_path: &Path, status_message: String) -> Self {
        Self {
            version: Self::VERSION,
            workspace_root: root.display().to_string(),
            candidate_path: candidate_path.display().to_string(),
            gate_path: gate_path.display().to_string(),
            contract_path: None,
            candidate_generated_at_ms: None,
            status_message: Some(status_message),
            summary: HarnessPromotionBoardSummary {
                total: 0,
                needs_review: 0,
                approved: 0,
                held: 0,
                applied: 0,
                up_to_date: 0,
                blocked: 0,
            },
            entries: Vec::new(),
        }
    }
}

impl Default for HarnessPromotionBoard {
    fn default() -> Self {
        Self {
            version: Self::VERSION,
            workspace_root: String::new(),
            candidate_path: String::new(),
            gate_path: String::new(),
            contract_path: None,
            candidate_generated_at_ms: None,
            status_message: None,
            summary: HarnessPromotionBoardSummary {
                total: 0,
                needs_review: 0,
                approved: 0,
                held: 0,
                applied: 0,
                up_to_date: 0,
                blocked: 0,
            },
            entries: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HarnessPromotionActionResponse {
    pub ok: bool,
    pub action: String,
    pub id: String,
    pub message: String,
    pub board: HarnessPromotionBoard,
}

fn effective_status(
    entry: &GovernorContractPromotionEntry,
    review: Option<&HarnessPromotionReviewRecord>,
) -> HarnessPromotionBoardStatus {
    match entry.decision {
        PromotionDecision::Add | PromotionDecision::Update => match review.map(|r| r.decision) {
            Some(HarnessPromotionReviewDecision::Approved) => HarnessPromotionBoardStatus::Approved,
            Some(HarnessPromotionReviewDecision::Held) => HarnessPromotionBoardStatus::Held,
            Some(HarnessPromotionReviewDecision::Applied) => HarnessPromotionBoardStatus::Applied,
            None => HarnessPromotionBoardStatus::NeedsReview,
        },
        PromotionDecision::Noop => HarnessPromotionBoardStatus::UpToDate,
        PromotionDecision::Hold | PromotionDecision::Invalid => {
            HarnessPromotionBoardStatus::Blocked
        }
    }
}

fn build_board_from_candidate(
    root: &Path,
    candidate_path: &Path,
    gate_path: &Path,
    candidate: &GovernorContractPromotionCandidate,
    gate: &HarnessPromotionGateState,
) -> HarnessPromotionBoard {
    let mut summary = HarnessPromotionBoardSummary {
        total: candidate.candidates.len(),
        needs_review: 0,
        approved: 0,
        held: 0,
        applied: 0,
        up_to_date: 0,
        blocked: 0,
    };
    let mut entries = candidate
        .candidates
        .iter()
        .map(|entry| {
            let review_record = gate.reviews.get(&entry.id).cloned();
            let review_status = effective_status(entry, review_record.as_ref());
            match review_status {
                HarnessPromotionBoardStatus::NeedsReview => summary.needs_review += 1,
                HarnessPromotionBoardStatus::Approved => summary.approved += 1,
                HarnessPromotionBoardStatus::Held => summary.held += 1,
                HarnessPromotionBoardStatus::Applied => summary.applied += 1,
                HarnessPromotionBoardStatus::UpToDate => summary.up_to_date += 1,
                HarnessPromotionBoardStatus::Blocked => summary.blocked += 1,
            }
            HarnessPromotionBoardEntry {
                id: entry.id.clone(),
                decision: entry.decision,
                review_status,
                review_badge: review_status.label().to_string(),
                can_approve: matches!(
                    entry.decision,
                    PromotionDecision::Add | PromotionDecision::Update
                ) && !matches!(review_status, HarnessPromotionBoardStatus::Applied),
                can_hold: matches!(
                    entry.decision,
                    PromotionDecision::Add | PromotionDecision::Update
                ) && !matches!(review_status, HarnessPromotionBoardStatus::Applied),
                can_apply: matches!(
                    (entry.decision, review_status),
                    (
                        PromotionDecision::Add | PromotionDecision::Update,
                        HarnessPromotionBoardStatus::Approved
                    )
                ),
                review_record,
                title: entry.display.title.clone(),
                subtitle: entry.display.subtitle.clone(),
                badge: entry.display.badge.clone(),
                contract_path: entry.contract_path.clone(),
                reasons: entry.reasons.clone(),
                green_case_ids: entry.green_case_ids.clone(),
                existing_template: entry.existing_template.clone(),
                proposed_template: entry.proposed_template.clone(),
                patch_path: entry.patch.as_ref().map(|patch| patch.path.clone()),
            }
        })
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| {
        (
            entry.review_status.rank(),
            entry.decision.label().to_string(),
            entry.title.to_ascii_lowercase(),
        )
    });
    let status_message = if entries.is_empty() {
        Some("no promotion candidates found yet".to_string())
    } else {
        None
    };
    HarnessPromotionBoard {
        version: HarnessPromotionBoard::VERSION,
        workspace_root: root.display().to_string(),
        candidate_path: candidate_path.display().to_string(),
        gate_path: gate_path.display().to_string(),
        contract_path: Some(candidate.contract_path.clone()),
        candidate_generated_at_ms: Some(candidate.generated_at_ms),
        status_message,
        summary,
        entries,
    }
}

fn resolve_contract_path(root: &Path, candidate: &GovernorContractPromotionCandidate) -> PathBuf {
    let path = PathBuf::from(&candidate.contract_path);
    if path.is_absolute() {
        path
    } else {
        root.join(path)
    }
}

fn load_candidate_and_gate(
    root: &Path,
) -> Result<(
    PathBuf,
    PathBuf,
    GovernorContractPromotionCandidate,
    HarnessPromotionGateState,
)> {
    let candidate_path = candidate_path_for_root(&root.display().to_string());
    let gate_path = gate_path_for_root(root);
    let candidate =
        GovernorContractPromotionCandidate::load(&candidate_path).with_context(|| {
            format!(
                "failed to load promotion candidate for harness gate: {}",
                candidate_path.display()
            )
        })?;
    let gate = HarnessPromotionGateState::load(&gate_path)?;
    Ok((candidate_path, gate_path, candidate, gate))
}

pub fn gate_path_for_root(root: &Path) -> PathBuf {
    root.join(".spiral-coder/governor_contract.promotion_gate.json")
}

pub fn load_board(root: &Path) -> Result<HarnessPromotionBoard> {
    let candidate_path = candidate_path_for_root(&root.display().to_string());
    let gate_path = gate_path_for_root(root);
    if !candidate_path.exists() {
        return Ok(HarnessPromotionBoard::empty(
            root,
            &candidate_path,
            &gate_path,
            "no promotion candidate found. Run `spiral-coder promote-harness` first.".to_string(),
        ));
    }
    let candidate = GovernorContractPromotionCandidate::load(&candidate_path)?;
    let gate = HarnessPromotionGateState::load(&gate_path)?;
    Ok(build_board_from_candidate(
        root,
        &candidate_path,
        &gate_path,
        &candidate,
        &gate,
    ))
}

fn update_gate_review(
    root: &Path,
    id: &str,
    decision: HarnessPromotionReviewDecision,
) -> Result<HarnessPromotionActionResponse> {
    let (candidate_path, gate_path, candidate, mut gate) = load_candidate_and_gate(root)?;
    let entry = candidate
        .candidates
        .iter()
        .find(|entry| entry.id == id)
        .ok_or_else(|| anyhow!("promotion candidate not found: {id}"))?;
    if !matches!(
        entry.decision,
        PromotionDecision::Add | PromotionDecision::Update
    ) {
        anyhow::bail!(
            "promotion candidate {} is not human-reviewable (decision={})",
            id,
            entry.decision.label()
        );
    }
    gate.set_decision(id, decision);
    gate.save_atomic(&gate_path)?;
    let board = build_board_from_candidate(root, &candidate_path, &gate_path, &candidate, &gate);
    Ok(HarnessPromotionActionResponse {
        ok: true,
        action: decision.label().to_string(),
        id: id.to_string(),
        message: format!("promotion {id} marked {}", decision.label()),
        board,
    })
}

pub fn approve(root: &Path, id: &str) -> Result<HarnessPromotionActionResponse> {
    update_gate_review(root, id, HarnessPromotionReviewDecision::Approved)
}

pub fn hold(root: &Path, id: &str) -> Result<HarnessPromotionActionResponse> {
    update_gate_review(root, id, HarnessPromotionReviewDecision::Held)
}

pub fn apply_to_contract(root: &Path, id: &str) -> Result<HarnessPromotionActionResponse> {
    let (candidate_path, gate_path, candidate, mut gate) = load_candidate_and_gate(root)?;
    let entry = candidate
        .candidates
        .iter()
        .find(|entry| entry.id == id)
        .ok_or_else(|| anyhow!("promotion candidate not found: {id}"))?;
    if !matches!(
        entry.decision,
        PromotionDecision::Add | PromotionDecision::Update
    ) {
        anyhow::bail!(
            "promotion candidate {} cannot be applied (decision={})",
            id,
            entry.decision.label()
        );
    }
    let review = gate
        .reviews
        .get(id)
        .ok_or_else(|| anyhow!("promotion candidate {id} must be approved before apply"))?;
    if !matches!(review.decision, HarnessPromotionReviewDecision::Approved) {
        anyhow::bail!("promotion candidate {id} must be approved before apply");
    }

    let contract_path = resolve_contract_path(root, &candidate);
    let mut contract = governor_contract::load_from_path(&contract_path)?;
    contract
        .runtime_overlay_templates
        .insert(id.to_string(), entry.proposed_template.clone());
    governor_contract::save_to_path(&contract, &contract_path)?;

    gate.set_decision(id, HarnessPromotionReviewDecision::Applied);
    gate.save_atomic(&gate_path)?;

    let board = build_board_from_candidate(root, &candidate_path, &gate_path, &candidate, &gate);
    Ok(HarnessPromotionActionResponse {
        ok: true,
        action: "apply_to_contract".to_string(),
        id: id.to_string(),
        message: format!("promotion {id} applied to {}", contract_path.display()),
        board,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        apply_to_contract, approve, gate_path_for_root, hold, load_board,
        HarnessPromotionBoardStatus,
    };
    use crate::governor_contract::{self, RuntimeOverlayTemplate};
    use crate::harness_promotion::{
        candidate_path_for_root, GovernorContractPromotionCandidate,
        GovernorContractPromotionEntry, GovernorContractPromotionSummary, PromotionDecision,
        PromotionDisplayCard, PromotionPatchOperation,
    };
    use std::path::Path;

    fn temp_root() -> std::path::PathBuf {
        static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let mut path = std::env::temp_dir();
        path.push(format!(
            "spiral_coder_harness_gate_{}_{}_{}",
            std::process::id(),
            super::now_ms(),
            SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(path.join(".spiral-coder")).expect("create temp root");
        std::fs::create_dir_all(path.join("shared")).expect("create shared dir");
        path
    }

    fn write_contract(root: &Path) -> std::path::PathBuf {
        let path = root.join("shared/governor_contract.json");
        let mut contract = governor_contract::contract().clone();
        contract.runtime_overlay_templates.clear();
        governor_contract::save_to_path(&contract, &path).expect("save contract");
        path
    }

    fn sample_template(pattern: &str) -> RuntimeOverlayTemplate {
        RuntimeOverlayTemplate {
            lane: "scaffold".to_string(),
            artifact_mode: "new_repo".to_string(),
            pattern: pattern.to_string(),
            policy_action: "advance_repo_scaffold_artifact".to_string(),
            required_action: "write_artifact".to_string(),
            preferred_tools: vec!["write_file".to_string()],
            blocked_tools: vec!["list_dir".to_string()],
            blocked_scope: "repo_scaffold".to_string(),
            blocked_command_display: Some("list_dir demo_repo".to_string()),
            next_target: Some("demo_repo/.gitignore".to_string()),
            exit_hint: "continue".to_string(),
            support_note: Some("note".to_string()),
        }
    }

    fn write_candidate(root: &Path, contract_path: &Path) {
        let candidate_path = candidate_path_for_root(&root.display().to_string());
        let candidate = GovernorContractPromotionCandidate {
            version: GovernorContractPromotionCandidate::VERSION,
            generated_at_ms: super::now_ms(),
            contract_path: contract_path.display().to_string(),
            overlay_path: root
                .join(".spiral-coder/governor_contract.overlay.json")
                .display()
                .to_string(),
            output_path: candidate_path.display().to_string(),
            summary: GovernorContractPromotionSummary {
                total: 1,
                add: 1,
                update: 0,
                noop: 0,
                hold: 0,
                invalid: 0,
                eligible: 1,
                min_green_cases: 1,
            },
            candidates: vec![GovernorContractPromotionEntry {
                id: "scaffold_repo::advance_repo_scaffold_artifact".to_string(),
                decision: PromotionDecision::Add,
                contract_path:
                    "/runtime_overlay_templates/scaffold_repo::advance_repo_scaffold_artifact"
                        .to_string(),
                display: PromotionDisplayCard {
                    title: "Advance repo scaffold artifact".to_string(),
                    subtitle: "Promote new runtime overlay template".to_string(),
                    badge: "add".to_string(),
                },
                reasons: vec!["meets eval gate".to_string()],
                green_case_ids: vec!["init-repo-artifact".to_string()],
                existing_template: None,
                proposed_template: sample_template("repo_scaffold_drift"),
                patch: Some(PromotionPatchOperation {
                    op: "add".to_string(),
                    path:
                        "/runtime_overlay_templates/scaffold_repo::advance_repo_scaffold_artifact"
                            .to_string(),
                    value: Some(
                        serde_json::to_value(sample_template("repo_scaffold_drift"))
                            .expect("serialize template"),
                    ),
                }),
            }],
        };
        candidate
            .save_atomic(&candidate_path)
            .expect("save candidate");
    }

    #[test]
    fn load_board_without_candidate_returns_empty_board() {
        let root = temp_root();
        let board = load_board(&root).expect("load board");
        assert!(board.entries.is_empty());
        assert!(board
            .status_message
            .unwrap_or_default()
            .contains("promote-harness"));
    }

    #[test]
    fn approve_and_apply_updates_contract_and_board() {
        let root = temp_root();
        let contract_path = write_contract(&root);
        write_candidate(&root, &contract_path);

        let approved = approve(&root, "scaffold_repo::advance_repo_scaffold_artifact")
            .expect("approve promotion");
        assert_eq!(approved.board.summary.approved, 1);
        assert_eq!(
            approved.board.entries[0].review_status,
            HarnessPromotionBoardStatus::Approved
        );

        let applied = apply_to_contract(&root, "scaffold_repo::advance_repo_scaffold_artifact")
            .expect("apply promotion");
        assert_eq!(applied.board.summary.applied, 1);
        assert_eq!(
            applied.board.entries[0].review_status,
            HarnessPromotionBoardStatus::Applied
        );

        let contract = governor_contract::load_from_path(&contract_path).expect("load contract");
        assert!(contract
            .runtime_overlay_templates
            .contains_key("scaffold_repo::advance_repo_scaffold_artifact"));
    }

    #[test]
    fn hold_marks_entry_held() {
        let root = temp_root();
        let contract_path = write_contract(&root);
        write_candidate(&root, &contract_path);

        let held =
            hold(&root, "scaffold_repo::advance_repo_scaffold_artifact").expect("hold promotion");
        assert_eq!(held.board.summary.held, 1);
        assert_eq!(
            held.board.entries[0].review_status,
            HarnessPromotionBoardStatus::Held
        );
        assert!(gate_path_for_root(&root).exists());
    }
}
