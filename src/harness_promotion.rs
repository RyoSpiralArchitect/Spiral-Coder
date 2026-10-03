use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};

use crate::governor_contract::{GovernorContract, RuntimeOverlayTemplate};
use crate::tui::agent::HarnessGovernorContractOverlay;

fn now_ms() -> u128 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn compact_one_line(s: &str, max: usize) -> String {
    let compact = s.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = compact.trim();
    if trimmed.chars().count() <= max {
        return trimmed.to_string();
    }
    let mut out = String::new();
    for ch in trimmed.chars().take(max.saturating_sub(1)) {
        out.push(ch);
    }
    out.push('…');
    out
}

fn escape_json_pointer_segment(segment: &str) -> String {
    segment.replace('~', "~0").replace('/', "~1")
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PromotionDecision {
    Add,
    Update,
    Noop,
    Hold,
    Invalid,
}

impl PromotionDecision {
    pub(crate) fn label(self) -> &'static str {
        match self {
            PromotionDecision::Add => "add",
            PromotionDecision::Update => "update",
            PromotionDecision::Noop => "noop",
            PromotionDecision::Hold => "hold",
            PromotionDecision::Invalid => "invalid",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PromotionPatchOperation {
    pub op: String,
    pub path: String,
    #[serde(default)]
    pub value: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PromotionDisplayCard {
    pub title: String,
    pub subtitle: String,
    pub badge: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GovernorContractPromotionEntry {
    pub id: String,
    pub decision: PromotionDecision,
    pub contract_path: String,
    pub display: PromotionDisplayCard,
    pub reasons: Vec<String>,
    pub green_case_ids: Vec<String>,
    #[serde(default)]
    pub existing_template: Option<RuntimeOverlayTemplate>,
    pub proposed_template: RuntimeOverlayTemplate,
    #[serde(default)]
    pub patch: Option<PromotionPatchOperation>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct GovernorContractPromotionSummary {
    pub total: usize,
    pub add: usize,
    pub update: usize,
    pub noop: usize,
    pub hold: usize,
    pub invalid: usize,
    pub eligible: usize,
    pub min_green_cases: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GovernorContractPromotionCandidate {
    pub version: u32,
    pub generated_at_ms: u128,
    pub contract_path: String,
    pub overlay_path: String,
    pub output_path: String,
    pub summary: GovernorContractPromotionSummary,
    pub candidates: Vec<GovernorContractPromotionEntry>,
}

impl GovernorContractPromotionCandidate {
    pub const VERSION: u32 = 1;

    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path).with_context(|| {
            format!(
                "failed to read governor contract promotion candidate: {}",
                path.display()
            )
        })?;
        let candidate: Self = serde_json::from_str(&text).with_context(|| {
            format!(
                "failed to parse governor contract promotion candidate: {}",
                path.display()
            )
        })?;
        if candidate.version != Self::VERSION {
            anyhow::bail!(
                "unsupported governor contract promotion candidate version {} (expected {})",
                candidate.version,
                Self::VERSION
            );
        }
        Ok(candidate)
    }

    pub fn save_atomic(&self, path: &Path) -> Result<()> {
        let json = serde_json::to_string_pretty(self)
            .context("failed to serialize governor contract promotion candidate")?;
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        std::fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create governor contract promotion dir: {}",
                parent.display()
            )
        })?;
        let tmp = path.with_extension(format!("tmp.{}.{}", std::process::id(), now_ms()));
        std::fs::write(&tmp, json.as_bytes()).with_context(|| {
            format!(
                "failed to write temp governor contract promotion candidate: {}",
                tmp.display()
            )
        })?;
        std::fs::rename(&tmp, path).with_context(|| {
            format!(
                "failed to replace governor contract promotion candidate {} -> {}",
                tmp.display(),
                path.display()
            )
        })?;
        Ok(())
    }
}

pub fn candidate_path_for_root(root: &str) -> PathBuf {
    Path::new(root).join(".spiral-coder/governor_contract.promotion.json")
}

pub fn build_promotion_candidate(
    contract: &GovernorContract,
    overlay: &HarnessGovernorContractOverlay,
    contract_path: &Path,
    overlay_path: &Path,
    output_path: &Path,
    min_green_cases: usize,
) -> GovernorContractPromotionCandidate {
    let min_green_cases = min_green_cases.max(1);
    let mut candidates = Vec::new();
    let mut summary = GovernorContractPromotionSummary {
        min_green_cases,
        ..GovernorContractPromotionSummary::default()
    };

    for policy in &overlay.promoted_policies {
        let contract_path_string = format!(
            "/runtime_overlay_templates/{}",
            escape_json_pointer_segment(policy.id.as_str())
        );
        let proposed_template = RuntimeOverlayTemplate {
            lane: policy.lane.clone(),
            artifact_mode: policy.artifact_mode.clone(),
            pattern: policy.pattern.clone(),
            policy_action: policy.policy_action.clone(),
            required_action: policy.required_action.clone(),
            preferred_tools: policy.preferred_tools.clone(),
            blocked_tools: policy.blocked_tools.clone(),
            blocked_scope: policy.blocked_scope.clone(),
            blocked_command_display: policy.blocked_command_display.clone(),
            next_target: policy.next_target.clone(),
            exit_hint: policy.exit_hint.clone(),
            support_note: policy.support_note.clone(),
        };

        let mut reasons = Vec::new();
        let all_tools_known = proposed_template
            .preferred_tools
            .iter()
            .chain(proposed_template.blocked_tools.iter())
            .all(|tool| contract.tool_names.iter().any(|known| known == tool));
        if !all_tools_known {
            reasons.push(
                "template references tools missing from the current governor contract".to_string(),
            );
        }
        if policy.green_case_ids.len() < min_green_cases {
            reasons.push(format!(
                "needs at least {min_green_cases} green eval case(s); currently {}",
                policy.green_case_ids.len()
            ));
        } else {
            reasons.push(format!(
                "meets eval gate with {} green case(s)",
                policy.green_case_ids.len()
            ));
        }

        let existing_template = contract.runtime_overlay_templates.get(&policy.id).cloned();
        let decision = if !all_tools_known {
            PromotionDecision::Invalid
        } else if policy.green_case_ids.len() < min_green_cases {
            PromotionDecision::Hold
        } else {
            match existing_template.as_ref() {
                None => PromotionDecision::Add,
                Some(existing) if existing == &proposed_template => PromotionDecision::Noop,
                Some(_) => PromotionDecision::Update,
            }
        };

        let patch = match decision {
            PromotionDecision::Add => Some(PromotionPatchOperation {
                op: "add".to_string(),
                path: contract_path_string.clone(),
                value: Some(
                    serde_json::to_value(&proposed_template)
                        .expect("runtime overlay template should serialize"),
                ),
            }),
            PromotionDecision::Update => Some(PromotionPatchOperation {
                op: "replace".to_string(),
                path: contract_path_string.clone(),
                value: Some(
                    serde_json::to_value(&proposed_template)
                        .expect("runtime overlay template should serialize"),
                ),
            }),
            _ => None,
        };

        let badge = decision.label().to_string();
        let subtitle = match decision {
            PromotionDecision::Add => format!(
                "Promote new runtime overlay template from {} green eval case(s)",
                policy.green_case_ids.len()
            ),
            PromotionDecision::Update => format!(
                "Refresh existing template from {} green eval case(s)",
                policy.green_case_ids.len()
            ),
            PromotionDecision::Noop => {
                "Already matches the current source contract template".to_string()
            }
            PromotionDecision::Hold => {
                format!("Hold until the overlay has {min_green_cases} green eval case(s)")
            }
            PromotionDecision::Invalid => {
                "Cannot promote until the template aligns with contract tool names".to_string()
            }
        };

        candidates.push(GovernorContractPromotionEntry {
            id: policy.id.clone(),
            decision,
            contract_path: contract_path_string,
            display: PromotionDisplayCard {
                title: format!(
                    "{} :: {}",
                    compact_one_line(policy.lane.as_str(), 48),
                    compact_one_line(policy.policy_id.as_str(), 64)
                ),
                subtitle,
                badge,
            },
            reasons,
            green_case_ids: policy.green_case_ids.clone(),
            existing_template,
            proposed_template,
            patch,
        });
    }

    candidates.sort_by_key(|candidate| {
        let rank = match candidate.decision {
            PromotionDecision::Add => 0usize,
            PromotionDecision::Update => 1,
            PromotionDecision::Noop => 2,
            PromotionDecision::Hold => 3,
            PromotionDecision::Invalid => 4,
        };
        (rank, candidate.id.clone())
    });

    summary.total = candidates.len();
    for candidate in &candidates {
        match candidate.decision {
            PromotionDecision::Add => {
                summary.add += 1;
                summary.eligible += 1;
            }
            PromotionDecision::Update => {
                summary.update += 1;
                summary.eligible += 1;
            }
            PromotionDecision::Noop => summary.noop += 1,
            PromotionDecision::Hold => summary.hold += 1,
            PromotionDecision::Invalid => summary.invalid += 1,
        }
    }

    GovernorContractPromotionCandidate {
        version: GovernorContractPromotionCandidate::VERSION,
        generated_at_ms: now_ms(),
        contract_path: contract_path.to_string_lossy().into_owned(),
        overlay_path: overlay_path.to_string_lossy().into_owned(),
        output_path: output_path.to_string_lossy().into_owned(),
        summary,
        candidates,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::governor_contract::{self, RuntimeOverlayTemplate};
    use crate::tui::agent::HarnessGovernorContractOverlay;
    use std::collections::BTreeMap;

    fn contract_with_templates(
        templates: BTreeMap<String, RuntimeOverlayTemplate>,
        tool_names: &[&str],
    ) -> GovernorContract {
        let mut contract = governor_contract::contract().clone();
        contract.tool_names = tool_names.iter().map(|tool| (*tool).to_string()).collect();
        contract.runtime_overlay_templates = templates;
        contract
    }

    fn sample_overlay() -> HarnessGovernorContractOverlay {
        serde_json::from_value(serde_json::json!({
            "version": 1,
            "updated_at_ms": 1,
            "promoted_policies": [
                {
                    "id": "fix_existing_files::force_mutation_after_observation_loop",
                    "policy_id": "force_mutation_after_observation_loop",
                    "lane": "fix_existing_files",
                    "artifact_mode": "existing_files",
                    "pattern": "repeated_observation_loop",
                    "policy_action": "mutate_existing_now",
                    "required_action": "Apply the smallest edit now with `patch_file` or `apply_diff`.",
                    "preferred_tools": ["patch_file", "apply_diff"],
                    "blocked_tools": ["read_file", "search_files"],
                    "blocked_scope": "exact_repeat_only",
                    "blocked_command_display": "read_file(path=src/lib.rs)",
                    "next_target": "src/lib.rs",
                    "exit_hint": "Apply the minimal change, then verify.",
                    "support_note": "reflection matched",
                    "evidence_count": 2,
                    "seen_count": 2,
                    "applied_count": 1,
                    "green_case_ids": ["fix-failing-rust-test"],
                    "promoted_at_ms": 1
                }
            ]
        }))
        .expect("overlay")
    }

    #[test]
    fn candidate_adds_new_runtime_overlay_template() {
        let overlay = sample_overlay();
        let contract = contract_with_templates(
            BTreeMap::new(),
            &[
                "exec",
                "read_file",
                "write_file",
                "patch_file",
                "apply_diff",
                "search_files",
                "list_dir",
                "glob",
                "done",
            ],
        );
        let candidate = build_promotion_candidate(
            &contract,
            &overlay,
            Path::new("shared/governor_contract.json"),
            Path::new(".spiral-coder/governor_contract.overlay.json"),
            Path::new(".spiral-coder/governor_contract.promotion.json"),
            1,
        );
        assert_eq!(candidate.summary.add, 1);
        assert_eq!(candidate.summary.eligible, 1);
        assert_eq!(candidate.candidates[0].decision, PromotionDecision::Add);
        assert_eq!(
            candidate.candidates[0]
                .patch
                .as_ref()
                .map(|patch| patch.op.as_str()),
            Some("add")
        );
    }

    #[test]
    fn candidate_noops_when_contract_template_matches() {
        let overlay = sample_overlay();
        let template = RuntimeOverlayTemplate {
            lane: "fix_existing_files".to_string(),
            artifact_mode: "existing_files".to_string(),
            pattern: "repeated_observation_loop".to_string(),
            policy_action: "mutate_existing_now".to_string(),
            required_action: "Apply the smallest edit now with `patch_file` or `apply_diff`."
                .to_string(),
            preferred_tools: vec!["patch_file".to_string(), "apply_diff".to_string()],
            blocked_tools: vec!["read_file".to_string(), "search_files".to_string()],
            blocked_scope: "exact_repeat_only".to_string(),
            blocked_command_display: Some("read_file(path=src/lib.rs)".to_string()),
            next_target: Some("src/lib.rs".to_string()),
            exit_hint: "Apply the minimal change, then verify.".to_string(),
            support_note: Some("reflection matched".to_string()),
        };
        let contract = contract_with_templates(
            BTreeMap::from([(
                "fix_existing_files::force_mutation_after_observation_loop".to_string(),
                template,
            )]),
            &[
                "exec",
                "read_file",
                "write_file",
                "patch_file",
                "apply_diff",
                "search_files",
                "list_dir",
                "glob",
                "done",
            ],
        );
        let candidate = build_promotion_candidate(
            &contract,
            &overlay,
            Path::new("shared/governor_contract.json"),
            Path::new(".spiral-coder/governor_contract.overlay.json"),
            Path::new(".spiral-coder/governor_contract.promotion.json"),
            1,
        );
        assert_eq!(candidate.summary.noop, 1);
        assert_eq!(candidate.candidates[0].decision, PromotionDecision::Noop);
    }

    #[test]
    fn candidate_holds_when_green_gate_not_met() {
        let overlay = sample_overlay();
        let contract = contract_with_templates(
            BTreeMap::new(),
            &[
                "exec",
                "read_file",
                "write_file",
                "patch_file",
                "apply_diff",
                "search_files",
                "list_dir",
                "glob",
                "done",
            ],
        );
        let candidate = build_promotion_candidate(
            &contract,
            &overlay,
            Path::new("shared/governor_contract.json"),
            Path::new(".spiral-coder/governor_contract.overlay.json"),
            Path::new(".spiral-coder/governor_contract.promotion.json"),
            2,
        );
        assert_eq!(candidate.summary.hold, 1);
        assert_eq!(candidate.candidates[0].decision, PromotionDecision::Hold);
    }
}
