use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::eval_merge_gate::{self, EvalMergeGateReport, EvalMergeGateView, EvalMergeGateViewCase};

fn now_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MergeGateReviewDecision {
    Approved,
    Held,
}

impl MergeGateReviewDecision {
    fn label(self) -> &'static str {
        match self {
            Self::Approved => "approved",
            Self::Held => "held",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MergeGateBoardStatus {
    NeedsReview,
    Approved,
    Held,
    RollbackAvailable,
    Blocked,
}

impl MergeGateBoardStatus {
    fn rank(self) -> u8 {
        match self {
            Self::NeedsReview => 0,
            Self::RollbackAvailable => 1,
            Self::Blocked => 2,
            Self::Approved => 3,
            Self::Held => 4,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::NeedsReview => "needs_review",
            Self::Approved => "approved",
            Self::Held => "held",
            Self::RollbackAvailable => "rollback_available",
            Self::Blocked => "blocked",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MergeGateReviewRecord {
    pub id: String,
    pub gate_path: String,
    pub report_path: String,
    pub decision: MergeGateReviewDecision,
    pub updated_at_ms: u128,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MergeGateReviewState {
    #[serde(default = "default_review_state_version")]
    pub version: u32,
    #[serde(default)]
    pub updated_at_ms: u128,
    #[serde(default)]
    pub reviews: BTreeMap<String, MergeGateReviewRecord>,
}

fn default_review_state_version() -> u32 {
    1
}

impl MergeGateReviewState {
    pub const VERSION: u32 = 1;

    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let text = std::fs::read_to_string(path).with_context(|| {
            format!("failed to read merge gate review state: {}", path.display())
        })?;
        let state: Self = serde_json::from_str(&text).with_context(|| {
            format!(
                "failed to parse merge gate review state: {}",
                path.display()
            )
        })?;
        if state.version != Self::VERSION {
            anyhow::bail!(
                "unsupported merge gate review state version {} (expected {})",
                state.version,
                Self::VERSION
            );
        }
        Ok(state)
    }

    pub fn save_atomic(&self, path: &Path) -> Result<()> {
        let json = serde_json::to_string_pretty(self)
            .context("failed to serialize merge gate review state")?;
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        std::fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create merge gate review state dir: {}",
                parent.display()
            )
        })?;
        let tmp = path.with_extension(format!("tmp.{}.{}", std::process::id(), now_ms()));
        std::fs::write(&tmp, json.as_bytes()).with_context(|| {
            format!(
                "failed to write temp merge gate review state: {}",
                tmp.display()
            )
        })?;
        std::fs::rename(&tmp, path).with_context(|| {
            format!(
                "failed to replace merge gate review state {} -> {}",
                tmp.display(),
                path.display()
            )
        })?;
        Ok(())
    }

    fn set_decision(
        &mut self,
        id: &str,
        gate_path: &str,
        report_path: &str,
        decision: MergeGateReviewDecision,
    ) {
        let ts = now_ms();
        self.updated_at_ms = ts;
        self.reviews.insert(
            id.to_string(),
            MergeGateReviewRecord {
                id: id.to_string(),
                gate_path: gate_path.to_string(),
                report_path: report_path.to_string(),
                decision,
                updated_at_ms: ts,
            },
        );
    }
}

impl Default for MergeGateReviewState {
    fn default() -> Self {
        Self {
            version: Self::VERSION,
            updated_at_ms: 0,
            reviews: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MergeGateBoardSummary {
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub needs_review: usize,
    pub approved: usize,
    pub held: usize,
    pub rollback_available: usize,
    pub blocked: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MergeGateBoardEntry {
    pub id: String,
    pub case_status: String,
    pub review_status: MergeGateBoardStatus,
    pub review_badge: String,
    pub can_approve: bool,
    pub can_hold: bool,
    pub can_preview_rollback: bool,
    #[serde(default)]
    pub review_record: Option<MergeGateReviewRecord>,
    pub root: String,
    pub gate_path: String,
    pub report_path: String,
    #[serde(default)]
    pub rollback_command: Option<String>,
    #[serde(default)]
    pub promoted_overlay_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MergeGateBoard {
    pub version: u32,
    pub workspace_root: String,
    #[serde(default)]
    pub gate_path: Option<String>,
    #[serde(default)]
    pub report_path: Option<String>,
    pub gate_status: String,
    #[serde(default)]
    pub status_message: Option<String>,
    pub summary: MergeGateBoardSummary,
    pub entries: Vec<MergeGateBoardEntry>,
    pub recommended_actions: Vec<String>,
}

impl MergeGateBoard {
    pub const VERSION: u32 = 1;

    fn empty(root: &Path, message: String) -> Self {
        Self {
            version: Self::VERSION,
            workspace_root: root.display().to_string(),
            gate_path: None,
            report_path: None,
            gate_status: "missing".to_string(),
            status_message: Some(message),
            summary: MergeGateBoardSummary {
                total: 0,
                passed: 0,
                failed: 0,
                needs_review: 0,
                approved: 0,
                held: 0,
                rollback_available: 0,
                blocked: 0,
            },
            entries: Vec::new(),
            recommended_actions: Vec::new(),
        }
    }
}

impl Default for MergeGateBoard {
    fn default() -> Self {
        Self::empty(Path::new("."), "no merge gate loaded".to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MergeGateActionResponse {
    pub ok: bool,
    pub action: String,
    pub message: String,
    pub board: MergeGateBoard,
}

fn matching_review(
    gate_path: &str,
    case: &EvalMergeGateViewCase,
    state: &MergeGateReviewState,
) -> Option<MergeGateReviewRecord> {
    state
        .reviews
        .get(case.id.as_str())
        .filter(|record| record.gate_path == gate_path)
        .cloned()
}

fn status_for_case(
    case: &EvalMergeGateViewCase,
    review: Option<&MergeGateReviewRecord>,
) -> MergeGateBoardStatus {
    if let Some(review) = review {
        return match review.decision {
            MergeGateReviewDecision::Approved => MergeGateBoardStatus::Approved,
            MergeGateReviewDecision::Held => MergeGateBoardStatus::Held,
        };
    }
    if case.status == "passed" {
        MergeGateBoardStatus::NeedsReview
    } else if case.rollback_command.is_some() {
        MergeGateBoardStatus::RollbackAvailable
    } else {
        MergeGateBoardStatus::Blocked
    }
}

fn board_from_view(
    root: &Path,
    gate_path: &Path,
    view: EvalMergeGateView,
    reviews: &MergeGateReviewState,
) -> MergeGateBoard {
    let gate_path_text = gate_path.display().to_string();
    let mut summary = MergeGateBoardSummary {
        total: view.total_cases,
        passed: view.passed_cases,
        failed: view.failed_cases,
        needs_review: 0,
        approved: 0,
        held: 0,
        rollback_available: 0,
        blocked: 0,
    };

    let mut entries = view
        .cases
        .iter()
        .map(|case| {
            let review = matching_review(gate_path_text.as_str(), case, reviews);
            let status = status_for_case(case, review.as_ref());
            match status {
                MergeGateBoardStatus::NeedsReview => summary.needs_review += 1,
                MergeGateBoardStatus::Approved => summary.approved += 1,
                MergeGateBoardStatus::Held => summary.held += 1,
                MergeGateBoardStatus::RollbackAvailable => summary.rollback_available += 1,
                MergeGateBoardStatus::Blocked => summary.blocked += 1,
            }
            MergeGateBoardEntry {
                id: case.id.clone(),
                case_status: case.status.clone(),
                review_status: status,
                review_badge: status.label().to_string(),
                can_approve: case.status == "passed"
                    && !matches!(status, MergeGateBoardStatus::Approved),
                can_hold: !matches!(status, MergeGateBoardStatus::Held),
                can_preview_rollback: case.rollback_command.is_some(),
                review_record: review,
                root: case.root.clone(),
                gate_path: gate_path_text.clone(),
                report_path: view.report_path.clone(),
                rollback_command: case.rollback_command.clone(),
                promoted_overlay_path: case.promoted_overlay_path.clone(),
            }
        })
        .collect::<Vec<_>>();

    entries.sort_by_key(|entry| (entry.review_status.rank(), entry.id.clone()));

    let status_message = if view.ci_ok && summary.approved == summary.passed && summary.passed > 0 {
        Some("merge gate approved for all passing cases".to_string())
    } else if view.ci_ok {
        Some("merge gate is ready; review passing cases before merge".to_string())
    } else {
        Some("merge gate is not ready; inspect failed cases and rollback previews".to_string())
    };

    MergeGateBoard {
        version: MergeGateBoard::VERSION,
        workspace_root: root.display().to_string(),
        gate_path: Some(gate_path_text),
        report_path: Some(view.report_path),
        gate_status: view.status,
        status_message,
        summary,
        entries,
        recommended_actions: view.recommended_actions,
    }
}

fn load_report_from_gate_path(
    gate_path: &Path,
) -> Result<(EvalMergeGateReport, EvalMergeGateView)> {
    let report = eval_merge_gate::load_merge_gate_report(gate_path)?;
    let view = eval_merge_gate::build_merge_gate_view(&report, gate_path);
    Ok((report, view))
}

pub fn gate_review_path_for_root(root: &Path) -> PathBuf {
    root.join(".spiral-coder/runtime_eval.merge_gate_review.json")
}

pub fn load_board(root: &Path) -> Result<MergeGateBoard> {
    let Some(gate_path) = eval_merge_gate::latest_path_for_root(root) else {
        return Ok(MergeGateBoard::empty(
            root,
            "no merge gate found. Run `spiral-coder eval -C . --spec .spiral-coder/runtime_eval.json` first."
                .to_string(),
        ));
    };
    load_board_from_gate(root, &gate_path)
}

pub fn load_board_from_gate(root: &Path, gate_path: &Path) -> Result<MergeGateBoard> {
    let (_report, view) = load_report_from_gate_path(gate_path)?;
    let review_path = gate_review_path_for_root(root);
    let reviews = MergeGateReviewState::load(&review_path)?;
    Ok(board_from_view(root, gate_path, view, &reviews))
}

fn update_review(
    root: &Path,
    id: &str,
    decision: MergeGateReviewDecision,
) -> Result<MergeGateActionResponse> {
    let gate_path = eval_merge_gate::latest_path_for_root(root)
        .ok_or_else(|| anyhow!("no merge gate found. Run `spiral-coder eval` first."))?;
    let board = load_board_from_gate(root, &gate_path)?;
    let entry = board
        .entries
        .iter()
        .find(|entry| entry.id == id)
        .ok_or_else(|| anyhow!("merge gate case {id} not found"))?;
    if matches!(decision, MergeGateReviewDecision::Approved) && !entry.can_approve {
        anyhow::bail!(
            "merge gate case {} cannot be approved (status={})",
            entry.id,
            entry.review_badge
        );
    }
    if matches!(decision, MergeGateReviewDecision::Held) && !entry.can_hold {
        anyhow::bail!("merge gate case {} is already held", entry.id);
    }

    let review_path = gate_review_path_for_root(root);
    let mut reviews = MergeGateReviewState::load(&review_path)?;
    reviews.set_decision(
        id,
        entry.gate_path.as_str(),
        entry.report_path.as_str(),
        decision,
    );
    reviews.save_atomic(&review_path)?;
    let board = load_board_from_gate(root, &gate_path)?;

    Ok(MergeGateActionResponse {
        ok: true,
        action: decision.label().to_string(),
        message: format!("merge gate case {id} marked {}", decision.label()),
        board,
    })
}

pub fn approve(root: &Path, id: &str) -> Result<MergeGateActionResponse> {
    update_review(root, id, MergeGateReviewDecision::Approved)
}

pub fn hold(root: &Path, id: &str) -> Result<MergeGateActionResponse> {
    update_review(root, id, MergeGateReviewDecision::Held)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval_merge_gate::{EvalMergeGateCase, EvalMergeGateReport, EvalMergeRollbackStatus};

    fn write_gate(root: &Path, ok: bool) -> PathBuf {
        let dir = root.join(".tmp/runtime_eval_1");
        std::fs::create_dir_all(&dir).expect("mkdir gate dir");
        let gate_path = dir.join("merge_gate.json");
        let report = EvalMergeGateReport {
            version: 1,
            generated_at_ms: 1,
            report_path: dir.join("report.json").display().to_string(),
            merge_ready: ok,
            rollback_required: !ok,
            promoted_overlay_count: 0,
            cases: vec![EvalMergeGateCase {
                id: "case-a".to_string(),
                ok,
                root: root.display().to_string(),
                session_path: dir.join("session.json").display().to_string(),
                trace_path: dir.join("trace.jsonl").display().to_string(),
                checkpoint: if ok { None } else { Some("abc123".to_string()) },
                rollback_status: if ok {
                    EvalMergeRollbackStatus::NotRequired
                } else {
                    EvalMergeRollbackStatus::ManualRequired
                },
                rollback_command: if ok {
                    None
                } else {
                    Some(format!("git -C '{}' reset --hard abc123", root.display()))
                },
                promoted_overlay_path: None,
            }],
        };
        eval_merge_gate::save_merge_gate_report(&gate_path, &report).expect("save gate");
        gate_path
    }

    #[test]
    fn load_board_marks_green_case_needs_review() {
        let td = tempfile::tempdir().expect("tempdir");
        write_gate(td.path(), true);

        let board = load_board(td.path()).expect("board");

        assert_eq!(board.summary.total, 1);
        assert_eq!(board.summary.needs_review, 1);
        assert!(board.entries[0].can_approve);
    }

    #[test]
    fn approve_marks_green_case_approved() {
        let td = tempfile::tempdir().expect("tempdir");
        write_gate(td.path(), true);

        let response = approve(td.path(), "case-a").expect("approve");

        assert_eq!(response.board.summary.approved, 1);
        assert_eq!(
            response.board.entries[0].review_status,
            MergeGateBoardStatus::Approved
        );
    }

    #[test]
    fn hold_records_failed_case_without_running_rollback() {
        let td = tempfile::tempdir().expect("tempdir");
        write_gate(td.path(), false);

        let board = load_board(td.path()).expect("board");
        assert_eq!(board.summary.rollback_available, 1);
        assert!(board.entries[0].rollback_command.is_some());

        let response = hold(td.path(), "case-a").expect("hold");

        assert_eq!(response.board.summary.held, 1);
        assert!(gate_review_path_for_root(td.path()).exists());
    }
}
