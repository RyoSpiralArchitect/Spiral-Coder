use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};

use crate::config::{PartialConfig, RunConfig};
use crate::modes::Mode;
use crate::tui::app::{App, Message, Role};
use crate::tui::{events, intent};

pub(crate) mod diagnostics;

fn default_spec_version() -> u32 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TuiReplaySpec {
    #[serde(default = "default_spec_version")]
    pub version: u32,
    #[serde(default)]
    pub defaults: TuiReplayDefaults,
    #[serde(default)]
    pub cases: Vec<TuiReplayCase>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TuiReplayDefaults {
    #[serde(default)]
    pub tool_root: Option<String>,
    #[serde(default)]
    pub lang: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TuiReplayCase {
    pub id: String,
    pub prompt: String,
    #[serde(default)]
    pub tool_root: Option<String>,
    #[serde(default)]
    pub lang: Option<String>,
    #[serde(default)]
    pub selector: Option<String>,
    #[serde(default)]
    pub reason_hint: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub coder_messages: Vec<TuiReplayMessage>,
    pub observer_response: Value,
    #[serde(default)]
    pub checks: Vec<TuiReplayCheck>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TuiReplayMessage {
    pub role: TuiReplayMessageRole,
    pub content: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TuiReplayMessageRole {
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TuiReplayCheck {
    SuggestionParsed,
    HintQueued,
    ContractQueued,
    ObserverContains { value: String },
    CoderSystemContains { value: String },
    TargetMessageContains { value: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TuiReplayArtifacts {
    pub case_dir: PathBuf,
    pub packet_path: PathBuf,
    pub observer_prompt_path: PathBuf,
    pub observer_response_path: PathBuf,
    pub preview_messages_path: PathBuf,
    pub case_report_path: PathBuf,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TuiReplayMetrics {
    pub suggestion_parsed: bool,
    pub hint_queued: bool,
    pub contract_queued: bool,
    pub observer_message_count: usize,
    pub coder_preview_system_count: usize,
    pub target_message_id: Option<String>,
    pub failure_kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TuiReplayCheckResult {
    pub label: String,
    pub ok: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TuiReplayCaseReport {
    pub id: String,
    pub ok: bool,
    pub prompt: String,
    pub tags: Vec<String>,
    pub root: String,
    pub lang: String,
    pub artifacts: TuiReplayArtifacts,
    pub metrics: TuiReplayMetrics,
    pub checks: Vec<TuiReplayCheckResult>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TuiReplaySummary {
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub passed_ids: Vec<String>,
    pub failed_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TuiReplayReport {
    pub version: u32,
    pub spec_path: PathBuf,
    pub out_dir: PathBuf,
    pub summary: TuiReplaySummary,
    pub cases: Vec<TuiReplayCaseReport>,
}

pub async fn run(args: crate::TuiReplayArgs, common: crate::CommonArgs) -> Result<()> {
    let cwd = std::env::current_dir().context("failed to get current directory")?;
    let base_root = args
        .tool_root
        .clone()
        .or_else(|| cwd.to_str().map(|s| s.to_string()))
        .unwrap_or_else(|| ".".to_string());
    let base_root_path = PathBuf::from(&base_root);
    let spec_path = resolve_path(&base_root_path, args.spec);
    let spec = load_spec(&spec_path)?;

    let out_dir = args.out_dir.unwrap_or_else(|| {
        base_root_path.join(format!(
            ".tmp/tui_replay_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
        ))
    });
    std::fs::create_dir_all(&out_dir)
        .with_context(|| format!("failed to create tui replay out dir: {}", out_dir.display()))?;

    let filter_lc = args.filter.as_ref().map(|s| s.to_ascii_lowercase());
    let mut selected: Vec<TuiReplayCase> = spec
        .cases
        .iter()
        .filter(|case| {
            if let Some(filter) = filter_lc.as_deref() {
                case.id.to_ascii_lowercase().contains(filter)
                    || case
                        .tags
                        .iter()
                        .any(|tag| tag.to_ascii_lowercase().contains(filter))
            } else {
                true
            }
        })
        .cloned()
        .collect();
    if let Some(limit) = args.max_cases.map(|n| n.max(1)) {
        selected.truncate(limit);
    }
    if selected.is_empty() {
        anyhow::bail!("tui replay selected 0 cases");
    }

    eprintln!(
        "[tui-replay] spec={} cases={} out={}",
        spec_path.display(),
        selected.len(),
        out_dir.display()
    );

    let mut reports = Vec::new();
    for (idx, case) in selected.iter().enumerate() {
        eprintln!(
            "[tui-replay] case {}/{}: {}",
            idx + 1,
            selected.len(),
            case.id
        );
        let report = run_case(
            idx,
            &common,
            &spec.defaults,
            case,
            &base_root_path,
            &out_dir,
        )?;
        eprintln!(
            "[tui-replay] {} {}",
            if report.ok { "PASS" } else { "FAIL" },
            report.id
        );
        reports.push(report);
    }

    let report = build_report(spec_path.clone(), out_dir.clone(), reports);
    let report_path = out_dir.join("report.json");
    save_report(&report_path, &report)?;
    eprintln!(
        "[tui-replay] summary: {}/{} passed, report={}",
        report.summary.passed,
        report.summary.total,
        report_path.display()
    );
    if report.summary.failed > 0 {
        anyhow::bail!(diagnostics::failed_report(&report));
    }
    Ok(())
}

fn run_case(
    idx: usize,
    common: &crate::CommonArgs,
    defaults: &TuiReplayDefaults,
    case: &TuiReplayCase,
    base_root_path: &Path,
    out_dir: &Path,
) -> Result<TuiReplayCaseReport> {
    validate_case(case)?;
    let root = resolve_case_root(base_root_path, defaults, case);
    let lang = case
        .lang
        .clone()
        .or_else(|| defaults.lang.clone())
        .unwrap_or_else(|| "en".to_string());
    let mut app = build_app(common, &root, &lang)?;
    app.coder_intent_anchor = Some(intent::apply_intent_update(
        None,
        intent::normalize_intent_update(&case.prompt, None),
        &case.prompt,
    ));
    app.coder.messages = case
        .coder_messages
        .iter()
        .map(|m| Message::new_complete(map_role(m.role), m.content.clone()))
        .collect();

    let (selector, reason_hint) = resolve_selector_and_reason(&app, case)?;
    let observer_response_raw = render_observer_response(&case.observer_response)?;
    let outcome = events::replay_observer_next_action_case(
        &mut app,
        &selector,
        &reason_hint,
        &observer_response_raw,
    )
    .map_err(|e| anyhow::anyhow!(e))?;

    let case_dir = out_dir.join(format!("{:03}-{}", idx + 1, sanitize_case_id(&case.id)));
    std::fs::create_dir_all(&case_dir)
        .with_context(|| format!("failed to create case dir: {}", case_dir.display()))?;

    let packet_path = case_dir.join("packet.json");
    let observer_prompt_path = case_dir.join("observer_prompt.txt");
    let observer_response_path = case_dir.join("observer_response.txt");
    let preview_messages_path = case_dir.join("coder_preview_messages.json");
    let case_report_path = case_dir.join("case_report.json");

    std::fs::write(&packet_path, serde_json::to_string_pretty(&outcome.packet)?)
        .with_context(|| format!("failed to write {}", packet_path.display()))?;
    std::fs::write(&observer_prompt_path, outcome.observer_prompt.as_bytes())
        .with_context(|| format!("failed to write {}", observer_prompt_path.display()))?;
    std::fs::write(
        &observer_response_path,
        outcome.observer_raw_response.as_bytes(),
    )
    .with_context(|| format!("failed to write {}", observer_response_path.display()))?;
    std::fs::write(
        &preview_messages_path,
        serde_json::to_string_pretty(&outcome.coder_preview_messages)?,
    )
    .with_context(|| format!("failed to write {}", preview_messages_path.display()))?;

    let metrics = TuiReplayMetrics {
        suggestion_parsed: outcome.parsed_suggestion.is_some(),
        hint_queued: outcome.pending_observer_hint.is_some(),
        contract_queued: outcome.pending_observer_contract.is_some(),
        observer_message_count: app.observer.messages.len(),
        coder_preview_system_count: outcome
            .coder_preview_messages
            .iter()
            .filter(|m| m.role == "system")
            .count(),
        target_message_id: outcome.target_message_id.clone(),
        failure_kind: outcome.failure_kind.clone(),
    };
    let checks = evaluate_checks(case, &outcome, &metrics);
    let ok = checks.iter().all(|c| c.ok);

    let report = TuiReplayCaseReport {
        id: case.id.clone(),
        ok,
        prompt: case.prompt.clone(),
        tags: case.tags.clone(),
        root,
        lang,
        artifacts: TuiReplayArtifacts {
            case_dir: case_dir.clone(),
            packet_path,
            observer_prompt_path,
            observer_response_path,
            preview_messages_path,
            case_report_path: case_report_path.clone(),
        },
        metrics,
        checks,
    };
    std::fs::write(&case_report_path, serde_json::to_string_pretty(&report)?)
        .with_context(|| format!("failed to write {}", case_report_path.display()))?;
    Ok(report)
}

fn build_app(common: &crate::CommonArgs, root: &str, lang: &str) -> Result<App> {
    let mut partial: PartialConfig = common.to_partial_config();
    if partial.mode.is_none() {
        partial.mode = Some(Mode::Vibe);
    }
    let coder_cfg = partial
        .resolve()
        .context("failed to resolve coder config for tui replay")?;
    let observer_cfg = RunConfig {
        mode: Mode::Observer,
        ..coder_cfg.clone()
    };
    let chat_cfg = RunConfig {
        mode: Mode::Chat,
        ..coder_cfg.clone()
    };
    Ok(App::new(
        coder_cfg,
        observer_cfg,
        chat_cfg,
        Some(root.to_string()),
        Some(root.to_string()),
        false,
        lang.to_string(),
        None,
    ))
}

fn resolve_selector_and_reason(app: &App, case: &TuiReplayCase) -> Result<(String, String)> {
    if let Some(selector) = case.selector.clone() {
        let reason = case
            .reason_hint
            .clone()
            .unwrap_or_else(|| "manual_replay".to_string());
        return Ok((selector, reason));
    }
    let Some((idx, reason)) = events::latest_tui_next_action_target(app) else {
        anyhow::bail!("could not infer a stuck target from coder_messages");
    };
    Ok((
        format!("msg:coder-{idx}"),
        case.reason_hint.clone().unwrap_or(reason),
    ))
}

fn resolve_case_root(
    base_root_path: &Path,
    defaults: &TuiReplayDefaults,
    case: &TuiReplayCase,
) -> String {
    let raw = case
        .tool_root
        .clone()
        .or_else(|| defaults.tool_root.clone())
        .unwrap_or_else(|| base_root_path.to_string_lossy().into_owned());
    resolve_path(base_root_path, PathBuf::from(raw))
        .to_string_lossy()
        .into_owned()
}

fn render_observer_response(value: &Value) -> Result<String> {
    match value {
        Value::String(s) => Ok(s.clone()),
        _ => serde_json::to_string_pretty(value).context("failed to serialize observer_response"),
    }
}

fn evaluate_checks(
    case: &TuiReplayCase,
    outcome: &events::TuiNextActionReplayOutcome,
    metrics: &TuiReplayMetrics,
) -> Vec<TuiReplayCheckResult> {
    case.checks
        .iter()
        .map(|check| match check {
            TuiReplayCheck::SuggestionParsed => TuiReplayCheckResult {
                label: "suggestion_parsed".to_string(),
                ok: metrics.suggestion_parsed,
                detail: format!("suggestion_parsed={}", metrics.suggestion_parsed),
            },
            TuiReplayCheck::HintQueued => TuiReplayCheckResult {
                label: "hint_queued".to_string(),
                ok: metrics.hint_queued,
                detail: format!("hint_queued={}", metrics.hint_queued),
            },
            TuiReplayCheck::ContractQueued => TuiReplayCheckResult {
                label: "contract_queued".to_string(),
                ok: metrics.contract_queued,
                detail: format!("contract_queued={}", metrics.contract_queued),
            },
            TuiReplayCheck::ObserverContains { value } => {
                let observed = outcome
                    .parsed_suggestion
                    .as_ref()
                    .map(serde_json::to_string)
                    .transpose()
                    .ok()
                    .flatten()
                    .unwrap_or_else(|| outcome.observer_raw_response.clone());
                TuiReplayCheckResult {
                    label: format!("observer_contains:{value}"),
                    ok: observed.contains(value),
                    detail: format!("matched={}", observed.contains(value)),
                }
            }
            TuiReplayCheck::CoderSystemContains { value } => {
                let matched = outcome
                    .coder_preview_messages
                    .iter()
                    .filter(|m| m.role == "system")
                    .any(|m| m.content.contains(value));
                TuiReplayCheckResult {
                    label: format!("coder_system_contains:{value}"),
                    ok: matched,
                    detail: format!("matched={matched}"),
                }
            }
            TuiReplayCheck::TargetMessageContains { value } => {
                let matched = outcome
                    .target_message_id
                    .as_deref()
                    .unwrap_or("")
                    .contains(value);
                TuiReplayCheckResult {
                    label: format!("target_message_contains:{value}"),
                    ok: matched,
                    detail: format!("matched={matched}"),
                }
            }
        })
        .collect()
}

fn build_report(
    spec_path: PathBuf,
    out_dir: PathBuf,
    cases: Vec<TuiReplayCaseReport>,
) -> TuiReplayReport {
    let total = cases.len();
    let passed = cases.iter().filter(|c| c.ok).count();
    let failed = total.saturating_sub(passed);
    let passed_ids = cases
        .iter()
        .filter(|c| c.ok)
        .map(|c| c.id.clone())
        .collect();
    let failed_ids = cases
        .iter()
        .filter(|c| !c.ok)
        .map(|c| c.id.clone())
        .collect();
    TuiReplayReport {
        version: 1,
        spec_path,
        out_dir,
        summary: TuiReplaySummary {
            total,
            passed,
            failed,
            passed_ids,
            failed_ids,
        },
        cases,
    }
}

#[cfg(test)]
pub(crate) fn replay_spec_for_test(
    path: &Path,
    root: &Path,
    out_dir: &Path,
) -> Result<TuiReplayReport> {
    use clap::Parser;
    let common = crate::CommonArgs::try_parse_from([
        "spiral-coder",
        "--provider",
        "openai",
        "--model",
        "offline-replay-fixture",
        "--base-url",
        "http://127.0.0.1:9/v1",
    ])?;
    let spec = load_spec(path)?;
    let cases = spec
        .cases
        .iter()
        .enumerate()
        .map(|(index, case)| run_case(index, &common, &spec.defaults, case, root, out_dir))
        .collect::<Result<Vec<_>>>()?;
    Ok(build_report(
        path.to_path_buf(),
        out_dir.to_path_buf(),
        cases,
    ))
}

fn load_spec(path: &Path) -> Result<TuiReplaySpec> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read tui replay spec: {}", path.display()))?;
    let spec: TuiReplaySpec =
        serde_json::from_str(&text).map_err(|error| diagnostics::parse_error(path, error))?;
    if spec.version != 1 {
        anyhow::bail!(
            "unsupported tui replay spec version {} (expected 1)",
            spec.version
        );
    }
    if spec.cases.is_empty() {
        anyhow::bail!("tui replay spec contains no cases");
    }
    for case in &spec.cases {
        validate_case(case)?;
    }
    Ok(spec)
}

fn validate_case(case: &TuiReplayCase) -> Result<()> {
    if case.checks.is_empty() {
        anyhow::bail!(
            "tui replay case '{}' requires at least one top-level checks entry; nested replay.checks is not part of the case schema",
            case.id
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests;

fn save_report(path: &Path, report: &TuiReplayReport) -> Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)
        .with_context(|| format!("failed to create replay report dir: {}", parent.display()))?;
    std::fs::write(path, serde_json::to_string_pretty(report)?)
        .with_context(|| format!("failed to write replay report: {}", path.display()))?;
    Ok(())
}

fn resolve_path(base_root: &Path, path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        base_root.join(path)
    }
}

fn sanitize_case_id(id: &str) -> String {
    let mut out = String::with_capacity(id.len());
    let mut prev_dash = false;
    for ch in id.trim().chars() {
        let good = ch.is_ascii_alphanumeric() || ch == '_' || ch == '-';
        let mapped = if good { ch } else { '-' };
        if mapped == '-' {
            if prev_dash {
                continue;
            }
            prev_dash = true;
            out.push('-');
        } else {
            prev_dash = false;
            out.push(mapped.to_ascii_lowercase());
        }
    }
    let out = out.trim_matches('-');
    if out.is_empty() {
        "case".to_string()
    } else {
        out.to_string()
    }
}

fn map_role(role: TuiReplayMessageRole) -> Role {
    match role {
        TuiReplayMessageRole::User => Role::User,
        TuiReplayMessageRole::Assistant => Role::Assistant,
        TuiReplayMessageRole::Tool => Role::Tool,
    }
}
