use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::runtime_eval::{RuntimeEvalCaseReport, RuntimeEvalReport};

fn now_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EvalMergeRollbackStatus {
    NotRequired,
    ManualRequired,
    NoCheckpoint,
}

impl EvalMergeRollbackStatus {
    pub fn label(&self) -> &'static str {
        match self {
            Self::NotRequired => "not_required",
            Self::ManualRequired => "manual_required",
            Self::NoCheckpoint => "no_checkpoint",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EvalMergeGateCase {
    pub id: String,
    pub ok: bool,
    pub root: String,
    pub session_path: String,
    pub trace_path: String,
    #[serde(default)]
    pub checkpoint: Option<String>,
    pub rollback_status: EvalMergeRollbackStatus,
    #[serde(default)]
    pub rollback_command: Option<String>,
    #[serde(default)]
    pub promoted_overlay_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EvalMergeGateReport {
    pub version: u32,
    pub generated_at_ms: u128,
    pub report_path: String,
    pub merge_ready: bool,
    pub rollback_required: bool,
    pub promoted_overlay_count: usize,
    pub cases: Vec<EvalMergeGateCase>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EvalMergeGateViewCase {
    pub id: String,
    pub status: String,
    pub rollback_status: EvalMergeRollbackStatus,
    #[serde(default)]
    pub rollback_command: Option<String>,
    #[serde(default)]
    pub promoted_overlay_path: Option<String>,
    pub root: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EvalMergeGateView {
    pub version: u32,
    pub gate_path: String,
    pub report_path: String,
    pub status: String,
    pub merge_ready: bool,
    pub ci_ok: bool,
    pub rollback_required: bool,
    pub promoted_overlay_count: usize,
    pub total_cases: usize,
    pub passed_cases: usize,
    pub failed_cases: usize,
    pub cases: Vec<EvalMergeGateViewCase>,
    pub recommended_actions: Vec<String>,
}

pub fn default_path_for_report(report_path: &Path) -> PathBuf {
    report_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("merge_gate.json")
}

pub fn latest_path_for_root(root: &Path) -> Option<PathBuf> {
    let tmp_dir = root.join(".tmp");
    let mut best: Option<(PathBuf, SystemTime)> = None;
    for entry in std::fs::read_dir(tmp_dir).ok()? {
        let entry = entry.ok()?;
        let path = entry.path();
        let name = path.file_name()?.to_str()?;
        if !name.starts_with("runtime_eval_") {
            continue;
        }
        let gate_path = path.join("merge_gate.json");
        if !gate_path.exists() {
            continue;
        }
        let modified = gate_path.metadata().ok()?.modified().ok()?;
        if best
            .as_ref()
            .map(|(_, current)| modified > *current)
            .unwrap_or(true)
        {
            best = Some((gate_path, modified));
        }
    }
    best.map(|(path, _)| path)
}

pub fn load_merge_gate_report(path: &Path) -> Result<EvalMergeGateReport> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read eval merge gate: {}", path.display()))?;
    serde_json::from_str(&text)
        .with_context(|| format!("failed to parse eval merge gate: {}", path.display()))
}

pub fn build_merge_gate_report(
    report: &RuntimeEvalReport,
    report_path: &Path,
    promoted_overlays: &BTreeMap<String, PathBuf>,
) -> EvalMergeGateReport {
    let cases = report
        .cases
        .iter()
        .map(|case| build_case(case, promoted_overlays))
        .collect::<Vec<_>>();
    let rollback_required = cases.iter().any(|case| {
        matches!(
            case.rollback_status,
            EvalMergeRollbackStatus::ManualRequired
        )
    });
    let promoted_overlay_count = cases
        .iter()
        .filter(|case| case.promoted_overlay_path.is_some())
        .count();

    EvalMergeGateReport {
        version: 1,
        generated_at_ms: now_ms(),
        report_path: report_path.display().to_string(),
        merge_ready: report.summary.failed == 0,
        rollback_required,
        promoted_overlay_count,
        cases,
    }
}

pub fn build_merge_gate_view(report: &EvalMergeGateReport, gate_path: &Path) -> EvalMergeGateView {
    let total_cases = report.cases.len();
    let passed_cases = report.cases.iter().filter(|case| case.ok).count();
    let failed_cases = total_cases.saturating_sub(passed_cases);
    let status = if report.merge_ready && !report.rollback_required {
        "ready"
    } else if report.rollback_required {
        "rollback_required"
    } else {
        "blocked"
    }
    .to_string();
    let ci_ok = report.merge_ready && !report.rollback_required;
    let cases = report
        .cases
        .iter()
        .map(|case| EvalMergeGateViewCase {
            id: case.id.clone(),
            status: if case.ok { "passed" } else { "failed" }.to_string(),
            rollback_status: case.rollback_status.clone(),
            rollback_command: case.rollback_command.clone(),
            promoted_overlay_path: case.promoted_overlay_path.clone(),
            root: case.root.clone(),
        })
        .collect::<Vec<_>>();
    let recommended_actions = recommended_actions_for_report(report);

    EvalMergeGateView {
        version: 1,
        gate_path: gate_path.display().to_string(),
        report_path: report.report_path.clone(),
        status,
        merge_ready: report.merge_ready,
        ci_ok,
        rollback_required: report.rollback_required,
        promoted_overlay_count: report.promoted_overlay_count,
        total_cases,
        passed_cases,
        failed_cases,
        cases,
        recommended_actions,
    }
}

fn recommended_actions_for_report(report: &EvalMergeGateReport) -> Vec<String> {
    let mut actions = Vec::new();
    if report.merge_ready && !report.rollback_required {
        actions.push(
            "approve: eval is merge-ready; review the diff and proceed to PR/merge gate"
                .to_string(),
        );
    } else if report.rollback_required {
        actions.push(
            "hold: eval failed with rollback evidence; inspect rollback command previews before changing state"
                .to_string(),
        );
    } else {
        actions.push(
            "hold: eval failed without a checkpoint; inspect report/trace before attempting rollback"
                .to_string(),
        );
    }
    if report.promoted_overlay_count > 0 {
        actions.push(
            "review promoted overlay paths before applying source-contract changes".to_string(),
        );
    }
    actions
}

pub fn save_merge_gate_report(path: &Path, report: &EvalMergeGateReport) -> Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)
        .with_context(|| format!("failed to create eval merge gate dir: {}", parent.display()))?;
    let json = serde_json::to_string_pretty(report)
        .context("failed to serialize eval merge gate report")?;
    let tmp = path.with_extension(format!("tmp.{}.{}", std::process::id(), now_ms()));
    std::fs::write(&tmp, json.as_bytes()).with_context(|| {
        format!(
            "failed to write temp eval merge gate report: {}",
            tmp.display()
        )
    })?;
    std::fs::rename(&tmp, path).with_context(|| {
        format!(
            "failed to replace eval merge gate report {} -> {}",
            tmp.display(),
            path.display()
        )
    })?;
    Ok(())
}

fn build_case(
    case: &RuntimeEvalCaseReport,
    promoted_overlays: &BTreeMap<String, PathBuf>,
) -> EvalMergeGateCase {
    let checkpoint = checkpoint_from_session(&case.artifacts.session_path)
        .or_else(|| checkpoint_from_trace(&case.artifacts.trace_path));
    let rollback_status = if case.ok {
        EvalMergeRollbackStatus::NotRequired
    } else if checkpoint.is_some() {
        EvalMergeRollbackStatus::ManualRequired
    } else {
        EvalMergeRollbackStatus::NoCheckpoint
    };
    let rollback_command = match (&rollback_status, checkpoint.as_deref()) {
        (EvalMergeRollbackStatus::ManualRequired, Some(hash)) => Some(format!(
            "git -C {} reset --hard {}",
            shell_quote(&case.root),
            hash
        )),
        _ => None,
    };

    EvalMergeGateCase {
        id: case.id.clone(),
        ok: case.ok,
        root: case.root.clone(),
        session_path: case.artifacts.session_path.display().to_string(),
        trace_path: case.artifacts.trace_path.display().to_string(),
        checkpoint,
        rollback_status,
        rollback_command,
        promoted_overlay_path: promoted_overlays
            .get(case.id.as_str())
            .map(|path| path.display().to_string()),
    }
}

fn checkpoint_from_session(path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let value = serde_json::from_str::<serde_json::Value>(&text).ok()?;
    value
        .get("checkpoint")
        .and_then(|value| value.as_str())
        .map(str::to_string)
}

fn checkpoint_from_trace(path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut out = None;
    for line in text.lines() {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if value.get("event").and_then(|value| value.as_str()) != Some("checkpoint") {
            continue;
        }
        if let Some(hash) = value
            .get("data")
            .and_then(|data| data.get("hash"))
            .and_then(|value| value.as_str())
        {
            out = Some(hash.to_string());
        }
    }
    out
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r#"'\''"#))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime_eval::{
        RuntimeEvalAgentConfig, RuntimeEvalArtifacts, RuntimeEvalCaseReport, RuntimeEvalMetrics,
        RuntimeEvalReport, RuntimeEvalSummary,
    };

    fn report_case(dir: &Path, ok: bool) -> RuntimeEvalCaseReport {
        let session_path = dir.join("session.json");
        let trace_path = dir.join("trace.jsonl");
        std::fs::write(&session_path, r#"{"checkpoint":"abc123","messages":[]}"#).expect("session");
        std::fs::write(
            &trace_path,
            r#"{"event":"checkpoint","data":{"hash":"abc123"}}"#,
        )
        .expect("trace");

        RuntimeEvalCaseReport {
            id: "demo".to_string(),
            ok,
            root: dir.display().to_string(),
            duration_ms: 1,
            prompt: "demo".to_string(),
            tags: Vec::new(),
            agent: RuntimeEvalAgentConfig::default(),
            run_error: None,
            artifacts: RuntimeEvalArtifacts {
                case_dir: dir.to_path_buf(),
                trace_path,
                session_path,
                json_path: dir.join("final.json"),
                graph_path: dir.join("graph.json"),
            },
            metrics: RuntimeEvalMetrics::default(),
            checks: Vec::new(),
        }
    }

    #[test]
    fn failed_case_gets_manual_rollback_command() {
        let td = tempfile::tempdir().expect("tempdir");
        let case = report_case(td.path(), false);
        let report = RuntimeEvalReport {
            version: 1,
            spec_path: td.path().join("spec.json"),
            out_dir: td.path().to_path_buf(),
            generated_at_ms: 1,
            summary: RuntimeEvalSummary {
                total: 1,
                failed: 1,
                ..RuntimeEvalSummary::default()
            },
            cases: vec![case],
        };

        let gate =
            build_merge_gate_report(&report, &td.path().join("report.json"), &BTreeMap::new());

        assert!(!gate.merge_ready);
        assert!(gate.rollback_required);
        assert_eq!(
            gate.cases[0].rollback_status,
            EvalMergeRollbackStatus::ManualRequired
        );
        assert!(gate.cases[0]
            .rollback_command
            .as_deref()
            .unwrap()
            .contains("abc123"));
    }

    #[test]
    fn green_case_records_promoted_overlay() {
        let td = tempfile::tempdir().expect("tempdir");
        let case = report_case(td.path(), true);
        let report = RuntimeEvalReport {
            version: 1,
            spec_path: td.path().join("spec.json"),
            out_dir: td.path().to_path_buf(),
            generated_at_ms: 1,
            summary: RuntimeEvalSummary {
                total: 1,
                passed: 1,
                ..RuntimeEvalSummary::default()
            },
            cases: vec![case],
        };
        let mut overlays = BTreeMap::new();
        overlays.insert("demo".to_string(), td.path().join(".spiral-coder/overlay.json"));

        let gate = build_merge_gate_report(&report, &td.path().join("report.json"), &overlays);

        assert!(gate.merge_ready);
        assert!(!gate.rollback_required);
        assert_eq!(gate.promoted_overlay_count, 1);
        assert!(gate.cases[0].promoted_overlay_path.is_some());
    }

    #[test]
    fn rollback_command_quotes_single_quotes_in_root() {
        let td = tempfile::tempdir().expect("tempdir");
        let root = td.path().join("repo'with-quote");
        std::fs::create_dir_all(&root).expect("mkdir root");
        let mut case = report_case(&root, false);
        case.root = root.display().to_string();
        let report = RuntimeEvalReport {
            version: 1,
            spec_path: td.path().join("spec.json"),
            out_dir: td.path().to_path_buf(),
            generated_at_ms: 1,
            summary: RuntimeEvalSummary {
                total: 1,
                failed: 1,
                ..RuntimeEvalSummary::default()
            },
            cases: vec![case],
        };

        let gate =
            build_merge_gate_report(&report, &td.path().join("report.json"), &BTreeMap::new());

        let command = gate.cases[0].rollback_command.as_deref().unwrap();
        assert!(command.contains("'\\''"));
        assert!(command.contains("abc123"));
    }

    #[test]
    fn view_marks_green_gate_as_ci_ok() {
        let td = tempfile::tempdir().expect("tempdir");
        let case = report_case(td.path(), true);
        let report = EvalMergeGateReport {
            version: 1,
            generated_at_ms: 1,
            report_path: td.path().join("report.json").display().to_string(),
            merge_ready: true,
            rollback_required: false,
            promoted_overlay_count: 0,
            cases: vec![build_case(&case, &BTreeMap::new())],
        };

        let view = build_merge_gate_view(&report, &td.path().join("merge_gate.json"));

        assert_eq!(view.status, "ready");
        assert!(view.ci_ok);
        assert_eq!(view.passed_cases, 1);
        assert_eq!(view.failed_cases, 0);
    }

    #[test]
    fn view_marks_failed_gate_as_blocked() {
        let td = tempfile::tempdir().expect("tempdir");
        let mut case = report_case(td.path(), false);
        case.artifacts.session_path = td.path().join("missing-session.json");
        case.artifacts.trace_path = td.path().join("missing-trace.jsonl");
        let report = EvalMergeGateReport {
            version: 1,
            generated_at_ms: 1,
            report_path: td.path().join("report.json").display().to_string(),
            merge_ready: false,
            rollback_required: false,
            promoted_overlay_count: 0,
            cases: vec![build_case(&case, &BTreeMap::new())],
        };

        let view = build_merge_gate_view(&report, &td.path().join("merge_gate.json"));

        assert_eq!(view.status, "blocked");
        assert!(!view.ci_ok);
        assert_eq!(view.failed_cases, 1);
        assert_eq!(
            view.cases[0].rollback_status,
            EvalMergeRollbackStatus::NoCheckpoint
        );
    }
}
