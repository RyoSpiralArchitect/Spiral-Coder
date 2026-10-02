use crate::observer::{memory, Critique, ProposalStatus, Severity};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BenchmarkPlan {
    pub case_id_hint: String,
    pub lane: String,
    pub objective: String,
    pub reason: String,
    pub confidence: u8,

    #[serde(default)]
    pub target_files: Vec<String>,

    #[serde(default)]
    pub required_checks: Vec<String>,

    #[serde(default)]
    pub success_criteria: Vec<String>,

    #[serde(default)]
    pub triggered_by: Vec<String>,
}

pub fn plan(critique: &Critique) -> Option<BenchmarkPlan> {
    if critique.risks.is_empty() && critique.proposals.is_empty() {
        return None;
    }

    let diagnostic = critique.coder_diagnostic.as_ref();
    let mut target_files = BTreeSet::new();
    let mut required_checks = BTreeSet::new();
    let mut triggered_by = BTreeSet::new();
    let mut success_criteria = BTreeSet::new();

    if let Some(diagnostic) = diagnostic {
        triggered_by.insert(format!("failure_mode: {}", diagnostic.failure_mode));
        if let Some(anchor) = &diagnostic.mutation_anchor {
            target_files.insert(anchor.path.clone());
        }
        for followup in &diagnostic.required_followups {
            target_files.insert(followup.path.clone());
            triggered_by.insert(format!("required_followup: {}", followup.path));
        }
        if let Some(cmd) = &diagnostic.verification_cmd {
            required_checks.insert(cmd.clone());
        }
    }

    for risk in &critique.risks {
        if memory::is_recurring_unresolved_proposal(risk.description.as_str()) {
            triggered_by.insert("risk: recurring unresolved observer proposal".to_string());
            success_criteria.insert(
                "repeated Observer finding becomes a visible risk and Coder-facing closeout requirement"
                    .to_string(),
            );
        }
        if risk
            .description
            .contains("No verification command after edits")
        {
            triggered_by.insert("risk: missing verification after edits".to_string());
            success_criteria
                .insert("final handoff cites a concrete passing verification command".to_string());
        }
    }

    for proposal in &critique.proposals {
        if proposal.status == ProposalStatus::Escalated
            || proposal.status == ProposalStatus::Unresolved
            || proposal.severity == Severity::Crit
            || proposal.score >= 80
        {
            triggered_by.insert(format!("proposal: {}", proposal.title.trim()));
        }
    }

    let lane = choose_lane(
        &target_files,
        diagnostic.and_then(|d| d.verification_cmd.as_deref()),
    );
    if lane == "none" && triggered_by.is_empty() {
        return None;
    }

    default_required_checks(&lane, &target_files, &mut required_checks);
    default_success_criteria(&lane, &mut success_criteria);

    let objective = objective_for(&lane, diagnostic.map(|d| d.failure_mode.as_str()));
    let case_id_hint = case_id_hint(
        &lane,
        diagnostic.map(|d| d.failure_mode.as_str()),
        &target_files,
    );
    let reason = if triggered_by.is_empty() {
        critique.critical_path.trim().to_string()
    } else {
        triggered_by
            .iter()
            .take(3)
            .cloned()
            .collect::<Vec<_>>()
            .join("; ")
    };
    let confidence = confidence_score(
        diagnostic.is_some(),
        !triggered_by.is_empty(),
        !target_files.is_empty(),
    );

    Some(BenchmarkPlan {
        case_id_hint,
        lane,
        objective,
        reason,
        confidence,
        target_files: target_files.into_iter().collect(),
        required_checks: required_checks.into_iter().collect(),
        success_criteria: success_criteria.into_iter().collect(),
        triggered_by: triggered_by.into_iter().collect(),
    })
}

fn choose_lane(target_files: &BTreeSet<String>, verification_cmd: Option<&str>) -> String {
    let verify = verification_cmd.unwrap_or("").to_ascii_lowercase();
    if verify.contains("runtime_eval.json")
        || target_files.iter().any(|p| {
            p == ".spiral-coder/runtime_eval.json"
                || p.starts_with("src/tui/agent/")
                || p == "src/tui/agent.rs"
        })
    {
        return "runtime_eval".to_string();
    }
    if verify.contains("tui-replay")
        || target_files.iter().any(|p| {
            p == ".spiral-coder/tui_replay.json"
                || p == "src/tui/events.rs"
                || p == "src/tui/ui.rs"
                || p == "src/tui/suggestion.rs"
                || p == "src/tui/prefs.rs"
        })
    {
        return "tui_replay".to_string();
    }
    if target_files.iter().any(|p| p.starts_with("src/observer/")) {
        return "observer_unit".to_string();
    }
    if target_files.iter().any(|p| p.starts_with("src/")) {
        return "unit".to_string();
    }
    if target_files.iter().any(|p| p.starts_with("docs/")) {
        return "docs_review".to_string();
    }
    "none".to_string()
}

fn default_required_checks(
    lane: &str,
    target_files: &BTreeSet<String>,
    out: &mut BTreeSet<String>,
) {
    match lane {
        "runtime_eval" => {
            out.insert("cargo test -q tui::agent::tests::".to_string());
            out.insert(
                "cargo run --quiet -- eval --spec .spiral-coder/runtime_eval.json --max-cases 1"
                    .to_string(),
            );
        }
        "tui_replay" => {
            out.insert("cargo test -q tui::events::tests::".to_string());
            out.insert(
                "cargo run --quiet -- tui-replay --spec .spiral-coder/tui_replay.json".to_string(),
            );
        }
        "observer_unit" => {
            out.insert("cargo test -q observer::".to_string());
        }
        "unit" => {
            out.insert("cargo test -q".to_string());
        }
        _ => {}
    }
    if target_files.iter().any(|p| p.ends_with(".rs")) {
        out.insert("cargo fmt --manifest-path Cargo.toml".to_string());
    }
}

fn default_success_criteria(lane: &str, out: &mut BTreeSet<String>) {
    match lane {
        "runtime_eval" => {
            out.insert(
                "runtime eval fails on the old behavior and passes after the harness fix"
                    .to_string(),
            );
            out.insert(
                "final handoff includes the required follow-up artifact and verification proof"
                    .to_string(),
            );
        }
        "tui_replay" => {
            out.insert(
                "TUI replay captures the Observer/Coder handoff without UI regression".to_string(),
            );
        }
        "observer_unit" => {
            out.insert(
                "Observer engine emits the expected typed contract deterministically".to_string(),
            );
        }
        "unit" => {
            out.insert(
                "targeted unit test covers the failure without relying on live provider behavior"
                    .to_string(),
            );
        }
        "docs_review" => {
            out.insert(
                "docs update names the changed contract and required verification path".to_string(),
            );
        }
        _ => {}
    }
}

fn objective_for(lane: &str, failure_mode: Option<&str>) -> String {
    match lane {
        "runtime_eval" => format!(
            "Add a runtime_eval regression for {} so the Coder must mutate, verify, and close out with required artifacts.",
            failure_mode.unwrap_or("the observed harness failure")
        ),
        "tui_replay" => "Add or refresh a tui-replay regression that proves the Observer handoff remains visible and actionable.".to_string(),
        "observer_unit" => "Add an Observer unit test for the deterministic critique/diagnostic contract.".to_string(),
        "unit" => "Add a targeted unit regression for the observed failure before broadening verification.".to_string(),
        "docs_review" => "Add a docs/state-schema or architecture check that keeps the typed contract documented.".to_string(),
        _ => "Capture this Observer finding as a small deterministic regression before continuing.".to_string(),
    }
}

fn case_id_hint(lane: &str, failure_mode: Option<&str>, target_files: &BTreeSet<String>) -> String {
    let seed = target_files
        .iter()
        .find_map(|p| {
            p.rsplit('/').next().map(|name| {
                name.trim_end_matches(".rs")
                    .trim_end_matches(".json")
                    .to_string()
            })
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| failure_mode.unwrap_or("observer-finding").to_string());
    format!("{}-{}", lane.replace('_', "-"), slug(&seed))
}

fn slug(s: &str) -> String {
    let mut out = String::new();
    for ch in s.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if (ch == '-' || ch == '_' || ch.is_whitespace()) && !out.ends_with('-') {
            out.push('-');
        }
    }
    let out = out.trim_matches('-').to_string();
    if out.is_empty() {
        "observer-finding".to_string()
    } else {
        out
    }
}

fn confidence_score(has_diagnostic: bool, has_trigger: bool, has_targets: bool) -> u8 {
    let mut score = 45u8;
    if has_diagnostic {
        score = score.saturating_add(25);
    }
    if has_trigger {
        score = score.saturating_add(15);
    }
    if has_targets {
        score = score.saturating_add(10);
    }
    score.min(95)
}

#[cfg(test)]
mod tests {
    use crate::observer::engine::run_observer_from_transcript;
    use crate::observer::memory::CritiqueMemory;

    #[test]
    fn plans_runtime_eval_for_coder_loop_followup() {
        let transcript = r#"
✎ patch_file: src/tui/agent/task_harness.rs
```bash
$ cargo test -q tui::agent::tests::
exit: 0
```
"#;

        let critique = run_observer_from_transcript(transcript, None);
        let plan = critique.benchmark_plan.expect("benchmark plan");

        assert_eq!(plan.lane, "runtime_eval");
        assert!(plan
            .target_files
            .iter()
            .any(|p| p == "src/tui/agent/task_harness.rs"));
        assert!(plan
            .required_checks
            .iter()
            .any(|cmd| cmd.contains("runtime_eval.json")));
        assert!(plan.objective.contains("runtime_eval regression"));
    }

    #[test]
    fn plans_recurring_observer_finding_regression() {
        let transcript = r#"
✎ patch_file: src/tui/agent/task_harness.rs
```bash
$ cargo test -q tui::agent::tests::
exit: 0
```
"#;
        let mut mem = CritiqueMemory::default();
        let _ = run_observer_from_transcript(transcript, Some(&mut mem));
        let critique = run_observer_from_transcript(transcript, Some(&mut mem));
        let plan = critique.benchmark_plan.expect("benchmark plan");

        assert!(plan
            .triggered_by
            .iter()
            .any(|entry| entry.contains("recurring unresolved observer proposal")));
        assert!(plan
            .success_criteria
            .iter()
            .any(|entry| entry.contains("repeated Observer finding")));
    }
}
