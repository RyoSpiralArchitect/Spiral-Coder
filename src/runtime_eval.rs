mod environment;
mod evidence;

pub use environment::validate_build_isolation;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

fn default_spec_version() -> u32 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeEvalSpec {
    #[serde(default = "default_spec_version")]
    pub version: u32,
    #[serde(default)]
    pub defaults: RuntimeEvalDefaults,
    #[serde(default)]
    pub cases: Vec<RuntimeEvalCase>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RuntimeEvalDefaults {
    #[serde(default)]
    pub tool_root: Option<String>,
    #[serde(default)]
    pub copy_tool_root: bool,
    #[serde(default)]
    pub lang: Option<String>,
    #[serde(default)]
    pub max_iters: Option<usize>,
    #[serde(default)]
    pub autofix: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeEvalCase {
    pub id: String,
    pub prompt: String,
    #[serde(default)]
    pub tool_root: Option<String>,
    #[serde(default)]
    pub session_seed: Option<String>,
    #[serde(default)]
    pub copy_tool_root: Option<bool>,
    #[serde(default)]
    pub lang: Option<String>,
    #[serde(default)]
    pub max_iters: Option<usize>,
    #[serde(default)]
    pub autofix: Option<usize>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub checks: Vec<RuntimeEvalCheck>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RuntimeEvalCheck {
    Completed,
    ErrorFree,
    AssistantContains {
        value: String,
    },
    AssistantNotContains {
        value: String,
    },
    ToolCallSeen {
        name: String,
    },
    ToolCallMin {
        name: String,
        min: usize,
    },
    TraceEventSeen {
        event: String,
    },
    VerifiedCommandSeen {
        command: String,
    },
    AutoTestPassed {
        #[serde(default)]
        command: Option<String>,
    },
    ToolRootFileExists {
        path: String,
    },
    ToolRootFileContains {
        path: String,
        value: String,
    },
    ToolRootFileEquals {
        path: String,
        value: String,
    },
    MessagesMin {
        min: usize,
    },
    GraphNodesMin {
        min: usize,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeEvalArtifacts {
    pub case_dir: PathBuf,
    pub trace_path: PathBuf,
    pub session_path: PathBuf,
    pub json_path: PathBuf,
    pub graph_path: PathBuf,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RuntimeEvalMetrics {
    #[serde(default)]
    pub evaluator_revision: Option<String>,
    pub completed: bool,
    #[serde(default)]
    pub task_completed: Option<bool>,
    pub error_count: usize,
    pub tool_call_count: usize,
    pub unique_tool_calls: Vec<String>,
    pub tool_call_histogram: BTreeMap<String, usize>,
    pub successful_exec_commands: Vec<String>,
    #[serde(default)]
    pub verified_exec_commands: Vec<String>,
    pub auto_test_pass_count: usize,
    #[serde(default)]
    pub fresh_auto_test_pass_count: usize,
    #[serde(default)]
    pub auto_test_command: Option<String>,
    pub trace_events: BTreeMap<String, usize>,
    pub iteration_count: usize,
    pub max_iteration: usize,
    pub messages_len: usize,
    pub graph_nodes: usize,
    pub graph_edges: usize,
    pub governor_events: usize,
    pub realize_events: usize,
    pub checkpoint_count: usize,
    pub round_count: usize,
    pub repo_map_fallback_count: usize,
    pub repo_map_fallback_histogram: BTreeMap<String, usize>,
    pub repo_map_typo_fallbacks: usize,
    pub provider_retry_count: usize,
    pub provider_retry_histogram: BTreeMap<String, usize>,
    pub provider_retry_total_delay_ms: u64,
    pub provider_retry_max_delay_ms: u64,
    pub reflection_ledger_entries_last: Option<usize>,
    pub reflection_ledger_prompt_count: usize,
    pub reflection_ledger_action_hit_count: usize,
    pub reflection_ledger_action_hit_histogram: BTreeMap<String, usize>,
    pub reflection_ledger_remember_count: usize,
    pub recovery_enter_count: usize,
    pub recovery_stage_histogram: BTreeMap<String, usize>,
    pub max_consecutive_failures: usize,
    pub max_same_command_repeats: usize,
    pub max_same_error_repeats: usize,
    pub max_same_output_repeats: usize,
    pub max_file_tool_consec_failures: usize,
    pub realize_mean_drift_last: Option<f64>,
    pub realize_mean_latency_last: Option<f64>,
    pub realize_missing_last: Option<usize>,
    pub realize_early_leakage_last: Option<usize>,
    pub transcript_input_tokens_est: usize,
    pub transcript_output_tokens_est: usize,
    pub transcript_total_tokens_est: usize,
    pub last_assistant_tokens_est: Option<usize>,
    pub last_assistant: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RuntimeEvalAgentConfig {
    pub provider: Option<String>,
    pub model: Option<String>,
    pub chat_model: Option<String>,
    pub code_model: Option<String>,
    pub base_url: Option<String>,
    pub mode: Option<String>,
    pub persona: Option<String>,
    pub temperature: Option<f64>,
    pub max_tokens: Option<u32>,
    pub timeout_seconds: Option<u64>,
    pub lang: Option<String>,
    pub max_iters: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeEvalCheckResult {
    pub label: String,
    pub ok: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeEvalCaseReport {
    pub id: String,
    pub ok: bool,
    pub root: String,
    pub duration_ms: u128,
    pub prompt: String,
    pub tags: Vec<String>,
    pub agent: RuntimeEvalAgentConfig,
    pub run_error: Option<String>,
    pub artifacts: RuntimeEvalArtifacts,
    pub metrics: RuntimeEvalMetrics,
    pub checks: Vec<RuntimeEvalCheckResult>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RuntimeEvalSummary {
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub avg_duration_ms: f64,
    pub avg_tool_calls: f64,
    pub avg_iterations: f64,
    pub avg_messages: f64,
    pub avg_transcript_input_tokens_est: f64,
    pub avg_transcript_output_tokens_est: f64,
    pub avg_transcript_total_tokens_est: f64,
    pub avg_repo_map_fallbacks: f64,
    pub avg_provider_retries: f64,
    pub avg_provider_retry_delay_ms: f64,
    pub avg_reflection_ledger_prompts: f64,
    pub avg_reflection_ledger_action_hits: f64,
    pub avg_recovery_enters: f64,
    pub passed_ids: Vec<String>,
    pub failed_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeEvalReport {
    pub version: u32,
    pub spec_path: PathBuf,
    pub out_dir: PathBuf,
    pub generated_at_ms: u128,
    pub summary: RuntimeEvalSummary,
    pub cases: Vec<RuntimeEvalCaseReport>,
}

#[derive(Debug, Clone, Deserialize)]
struct TraceLine {
    event: String,
    #[serde(default)]
    data: Value,
}

fn now_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

pub fn load_spec(path: &Path) -> Result<RuntimeEvalSpec> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read runtime eval spec: {}", path.display()))?;
    let spec: RuntimeEvalSpec = serde_json::from_str(&text)
        .with_context(|| format!("failed to parse runtime eval spec: {}", path.display()))?;
    if spec.version != 1 {
        anyhow::bail!(
            "unsupported runtime eval spec version {} (expected 1)",
            spec.version
        );
    }
    if spec.cases.is_empty() {
        anyhow::bail!("runtime eval spec contains no cases");
    }
    Ok(spec)
}

pub fn save_report(path: &Path, report: &RuntimeEvalReport) -> Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent).with_context(|| {
        format!(
            "failed to create runtime eval report dir: {}",
            parent.display()
        )
    })?;
    let text =
        serde_json::to_string_pretty(report).context("failed to serialize runtime eval report")?;
    std::fs::write(path, text)
        .with_context(|| format!("failed to write runtime eval report: {}", path.display()))?;
    Ok(())
}

pub fn sanitize_case_id(id: &str) -> String {
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

/// The runtime creates only this reserved overlay artifact during promotion.
/// All requested work artifacts must already pass before promotion is attempted.
pub fn pre_promotion_case(case: &RuntimeEvalCase) -> RuntimeEvalCase {
    let mut precheck = case.clone();
    precheck.checks.retain(|check| {
        !matches!(check,
            RuntimeEvalCheck::ToolRootFileExists { path }
                if path == ".spiral-coder/governor_contract.overlay.json"
        )
    });
    precheck
}

pub fn evaluate_case(
    case: &RuntimeEvalCase,
    root: &str,
    artifacts: RuntimeEvalArtifacts,
    duration_ms: u128,
    run_error: Option<String>,
) -> Result<RuntimeEvalCaseReport> {
    let trace_lines = load_trace_lines(&artifacts.trace_path)?;
    let agent = extract_agent_config(&trace_lines);
    let metrics = collect_metrics(&trace_lines, &artifacts, case)?;
    let checks = evaluate_checks(case, root, &artifacts, &metrics, run_error.as_deref());
    let ok = checks.iter().all(|c| c.ok) && run_error.is_none();
    Ok(RuntimeEvalCaseReport {
        id: case.id.clone(),
        ok,
        root: root.to_string(),
        duration_ms,
        prompt: case.prompt.clone(),
        tags: case.tags.clone(),
        agent,
        run_error,
        artifacts,
        metrics,
        checks,
    })
}

pub fn summarize_cases(cases: &[RuntimeEvalCaseReport]) -> RuntimeEvalSummary {
    let total = cases.len();
    let passed = cases.iter().filter(|c| c.ok).count();
    let failed = total.saturating_sub(passed);
    let avg_duration_ms = avg(cases.iter().map(|c| c.duration_ms as f64).collect());
    let avg_tool_calls = avg(cases
        .iter()
        .map(|c| c.metrics.tool_call_count as f64)
        .collect());
    let avg_iterations = avg(cases
        .iter()
        .map(|c| c.metrics.iteration_count as f64)
        .collect());
    let avg_messages = avg(cases
        .iter()
        .map(|c| c.metrics.messages_len as f64)
        .collect());
    let avg_transcript_input_tokens_est = avg(cases
        .iter()
        .map(|c| c.metrics.transcript_input_tokens_est as f64)
        .collect());
    let avg_transcript_output_tokens_est = avg(cases
        .iter()
        .map(|c| c.metrics.transcript_output_tokens_est as f64)
        .collect());
    let avg_transcript_total_tokens_est = avg(cases
        .iter()
        .map(|c| c.metrics.transcript_total_tokens_est as f64)
        .collect());
    let avg_repo_map_fallbacks = avg(cases
        .iter()
        .map(|c| c.metrics.repo_map_fallback_count as f64)
        .collect());
    let avg_provider_retries = avg(cases
        .iter()
        .map(|c| c.metrics.provider_retry_count as f64)
        .collect());
    let avg_provider_retry_delay_ms = avg(cases
        .iter()
        .map(|c| c.metrics.provider_retry_total_delay_ms as f64)
        .collect());
    let avg_reflection_ledger_prompts = avg(cases
        .iter()
        .map(|c| c.metrics.reflection_ledger_prompt_count as f64)
        .collect());
    let avg_reflection_ledger_action_hits = avg(cases
        .iter()
        .map(|c| c.metrics.reflection_ledger_action_hit_count as f64)
        .collect());
    let avg_recovery_enters = avg(cases
        .iter()
        .map(|c| c.metrics.recovery_enter_count as f64)
        .collect());
    let mut passed_ids = Vec::new();
    let mut failed_ids = Vec::new();
    for case in cases {
        if case.ok {
            passed_ids.push(case.id.clone());
        } else {
            failed_ids.push(case.id.clone());
        }
    }
    RuntimeEvalSummary {
        total,
        passed,
        failed,
        avg_duration_ms,
        avg_tool_calls,
        avg_iterations,
        avg_messages,
        avg_transcript_input_tokens_est,
        avg_transcript_output_tokens_est,
        avg_transcript_total_tokens_est,
        avg_repo_map_fallbacks,
        avg_provider_retries,
        avg_provider_retry_delay_ms,
        avg_reflection_ledger_prompts,
        avg_reflection_ledger_action_hits,
        avg_recovery_enters,
        passed_ids,
        failed_ids,
    }
}

pub fn build_report(
    spec_path: PathBuf,
    out_dir: PathBuf,
    cases: Vec<RuntimeEvalCaseReport>,
) -> RuntimeEvalReport {
    RuntimeEvalReport {
        version: 1,
        spec_path,
        out_dir,
        generated_at_ms: now_ms(),
        summary: summarize_cases(&cases),
        cases,
    }
}

fn avg(values: Vec<f64>) -> f64 {
    if values.is_empty() {
        0.0
    } else {
        values.iter().sum::<f64>() / values.len() as f64
    }
}

fn load_trace_lines(path: &Path) -> Result<Vec<TraceLine>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read trace file: {}", path.display()))?;
    let mut out = Vec::new();
    for (idx, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let trace: TraceLine = serde_json::from_str(line).with_context(|| {
            format!(
                "failed to parse trace line {} from {}",
                idx + 1,
                path.display()
            )
        })?;
        out.push(trace);
    }
    Ok(out)
}

fn collect_metrics(
    trace_lines: &[TraceLine],
    artifacts: &RuntimeEvalArtifacts,
    case: &RuntimeEvalCase,
) -> Result<RuntimeEvalMetrics> {
    let mut metrics = RuntimeEvalMetrics {
        evaluator_revision: Some("outcome-proof-v2".to_string()),
        ..RuntimeEvalMetrics::default()
    };
    let mut tool_names = BTreeSet::new();
    let mut agent_end_ok = None;
    let mut last_recovery_stage: Option<String> = None;

    for line in trace_lines {
        *metrics.trace_events.entry(line.event.clone()).or_insert(0) += 1;
        match line.event.as_str() {
            "tool_call" => {
                metrics.tool_call_count += 1;
                if let Some(name) = line.data.get("name").and_then(|v| v.as_str()) {
                    tool_names.insert(name.to_string());
                    *metrics
                        .tool_call_histogram
                        .entry(name.to_string())
                        .or_insert(0) += 1;
                }
            }
            "error" => metrics.error_count += 1,
            "checkpoint" => metrics.checkpoint_count += 1,
            "agent_iter" => {
                metrics.iteration_count += 1;
                if let Some(iter) = line.data.get("iter").and_then(|v| v.as_u64()) {
                    metrics.max_iteration = metrics.max_iteration.max(iter as usize);
                }
            }
            "repo_map_fallback" => {
                metrics.repo_map_fallback_count += 1;
                if line
                    .data
                    .get("typo_likely")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
                {
                    metrics.repo_map_typo_fallbacks += 1;
                }
                if let Some(tool) = line.data.get("tool").and_then(|v| v.as_str()) {
                    *metrics
                        .repo_map_fallback_histogram
                        .entry(tool.to_string())
                        .or_insert(0) += 1;
                }
            }
            "provider_retry" => {
                metrics.provider_retry_count += 1;
                let provider = line
                    .data
                    .get("provider")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let status = line
                    .data
                    .get("status")
                    .and_then(|v| v.as_u64())
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "send".to_string());
                let key = format!("{provider}:{status}");
                *metrics.provider_retry_histogram.entry(key).or_insert(0) += 1;
                let delay_ms = line
                    .data
                    .get("delay_ms")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                metrics.provider_retry_total_delay_ms = metrics
                    .provider_retry_total_delay_ms
                    .saturating_add(delay_ms);
                metrics.provider_retry_max_delay_ms =
                    metrics.provider_retry_max_delay_ms.max(delay_ms);
            }
            "reflection_ledger_loaded" => {
                metrics.reflection_ledger_entries_last = line
                    .data
                    .get("entries")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as usize);
            }
            "reflection_ledger_prompted" => {
                metrics.reflection_ledger_prompt_count += 1;
                if let Some(entries) = line
                    .data
                    .get("entries")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as usize)
                {
                    metrics.reflection_ledger_entries_last = Some(entries);
                }
            }
            "reflection_ledger_action_hit" => {
                metrics.reflection_ledger_action_hit_count += 1;
                if let Some(tool) = line.data.get("tool").and_then(|v| v.as_str()) {
                    *metrics
                        .reflection_ledger_action_hit_histogram
                        .entry(tool.to_string())
                        .or_insert(0) += 1;
                }
            }
            "reflection_ledger_remembered" => {
                metrics.reflection_ledger_remember_count += 1;
                if let Some(entries) = line
                    .data
                    .get("entries")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as usize)
                {
                    metrics.reflection_ledger_entries_last = Some(entries);
                }
            }
            "governor_state" => {
                metrics.governor_events += 1;
                let stage = line
                    .data
                    .get("recovery_stage")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                if let Some(ref current) = stage {
                    *metrics
                        .recovery_stage_histogram
                        .entry(current.clone())
                        .or_insert(0) += 1;
                }
                if stage.is_some() && last_recovery_stage.is_none() {
                    metrics.recovery_enter_count += 1;
                }
                last_recovery_stage = stage;
                metrics.max_consecutive_failures = metrics.max_consecutive_failures.max(
                    line.data
                        .get("consecutive_failures")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0) as usize,
                );
                metrics.max_same_command_repeats = metrics.max_same_command_repeats.max(
                    line.data
                        .get("same_command_repeats")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0) as usize,
                );
                metrics.max_same_error_repeats = metrics.max_same_error_repeats.max(
                    line.data
                        .get("same_error_repeats")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0) as usize,
                );
                metrics.max_same_output_repeats = metrics.max_same_output_repeats.max(
                    line.data
                        .get("same_output_repeats")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0) as usize,
                );
                metrics.max_file_tool_consec_failures = metrics.max_file_tool_consec_failures.max(
                    line.data
                        .get("file_tool_consec_failures")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0) as usize,
                );
            }
            "realize_state" => {
                metrics.realize_events += 1;
                metrics.realize_mean_drift_last =
                    line.data.get("mean_drift").and_then(|v| v.as_f64());
                metrics.realize_mean_latency_last = line
                    .data
                    .get("mean_realize_latency")
                    .and_then(|v| v.as_f64());
                metrics.realize_missing_last = line
                    .data
                    .get("missing")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as usize);
                metrics.realize_early_leakage_last = line
                    .data
                    .get("early_leakage")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as usize);
            }
            "round_start" => {
                metrics.round_count += 1;
                metrics.task_completed = None;
            }
            "agent_outcome" => {
                metrics.task_completed = line.data.get("completed").and_then(Value::as_bool);
            }
            "verification_config" => {
                metrics.auto_test_command = line
                    .data
                    .get("test_command")
                    .and_then(Value::as_str)
                    .map(str::to_string);
            }
            "agent_end" => {
                agent_end_ok = line.data.get("ok").and_then(|v| v.as_bool());
            }
            _ => {}
        }
    }
    metrics.unique_tool_calls = tool_names.into_iter().collect();
    metrics.completed = agent_end_ok == Some(true)
        && metrics.task_completed == Some(true)
        && metrics.error_count == 0;

    let session_value = load_session_value(&artifacts.json_path)
        .or_else(|_| load_session_value(&artifacts.session_path))
        .unwrap_or(Value::Null);
    if let Some(messages) = session_value.get("messages").and_then(|v| v.as_array()) {
        metrics.messages_len = messages.len();
        metrics.last_assistant = select_terminal_assistant_message(messages);
        let required_commands: Vec<String> = case
            .checks
            .iter()
            .filter_map(|check| match check {
                RuntimeEvalCheck::VerifiedCommandSeen { command } => Some(command.clone()),
                _ => None,
            })
            .collect();
        let proof = evidence::collect(
            messages,
            &required_commands,
            metrics.auto_test_command.as_deref(),
        );
        metrics.successful_exec_commands = proof.successful_commands;
        metrics.verified_exec_commands = proof.verified_commands;
        metrics.auto_test_pass_count = proof.auto_test_pass_count;
        metrics.fresh_auto_test_pass_count = proof.fresh_auto_test_pass_count;
        let token_estimates = estimate_transcript_tokens(messages);
        metrics.transcript_input_tokens_est = token_estimates.input;
        metrics.transcript_output_tokens_est = token_estimates.output;
        metrics.transcript_total_tokens_est = token_estimates.total;
        metrics.last_assistant_tokens_est =
            metrics.last_assistant.as_deref().map(approx_tokens_text);
    }

    let graph_value = load_graph_value(&artifacts.graph_path).unwrap_or(Value::Null);
    metrics.graph_nodes = graph_value
        .get("nodes")
        .and_then(|v| v.as_array())
        .map(|v| v.len())
        .unwrap_or(0);
    metrics.graph_edges = graph_value
        .get("edges")
        .and_then(|v| v.as_array())
        .map(|v| v.len())
        .unwrap_or(0);

    Ok(metrics)
}

#[derive(Debug, Clone, Copy, Default)]
struct TranscriptTokenEstimate {
    input: usize,
    output: usize,
    total: usize,
}

fn estimate_transcript_tokens(messages: &[Value]) -> TranscriptTokenEstimate {
    let mut out = TranscriptTokenEstimate::default();
    for msg in messages {
        let count = approx_tokens_value(msg);
        out.total += count;
        match msg.get("role").and_then(|v| v.as_str()) {
            Some("assistant") => out.output += count,
            _ => out.input += count,
        }
    }
    out
}

fn approx_tokens_value(value: &Value) -> usize {
    serde_json::to_string(value)
        .ok()
        .map(|s| approx_tokens_text(&s))
        .unwrap_or(0)
}

fn approx_tokens_text(s: &str) -> usize {
    let chars = s.chars().count();
    if chars == 0 {
        return 0;
    }
    let non_ascii = s.chars().filter(|c| !c.is_ascii()).count();
    let ascii = chars.saturating_sub(non_ascii);
    let ascii_tokens = ascii.div_ceil(4);
    ascii_tokens + non_ascii
}

fn extract_agent_config(trace_lines: &[TraceLine]) -> RuntimeEvalAgentConfig {
    let mut config = RuntimeEvalAgentConfig::default();
    for line in trace_lines {
        if line.event != "agent_start" {
            continue;
        }
        let cfg = line.data.get("cfg").unwrap_or(&Value::Null);
        config.provider = cfg
            .get("provider")
            .and_then(|v| v.as_str())
            .map(|v| v.to_string());
        config.model = cfg
            .get("model")
            .and_then(|v| v.as_str())
            .map(|v| v.to_string());
        config.chat_model = cfg
            .get("chat_model")
            .and_then(|v| v.as_str())
            .map(|v| v.to_string());
        config.code_model = cfg
            .get("code_model")
            .and_then(|v| v.as_str())
            .map(|v| v.to_string());
        config.base_url = cfg
            .get("base_url")
            .and_then(|v| v.as_str())
            .map(|v| v.to_string());
        config.mode = cfg
            .get("mode")
            .and_then(|v| v.as_str())
            .map(|v| v.to_string());
        config.persona = cfg
            .get("persona")
            .and_then(|v| v.as_str())
            .map(|v| v.to_string());
        config.temperature = cfg.get("temperature").and_then(|v| v.as_f64());
        config.max_tokens = cfg
            .get("max_tokens")
            .and_then(|v| v.as_u64())
            .map(|v| v as u32);
        config.timeout_seconds = cfg.get("timeout_seconds").and_then(|v| v.as_u64());
        config.lang = line
            .data
            .get("lang")
            .and_then(|v| v.as_str())
            .map(|v| v.to_string());
        config.max_iters = line
            .data
            .get("max_iters")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize);
        break;
    }
    config
}

fn load_session_value(path: &Path) -> Result<Value> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read session snapshot: {}", path.display()))?;
    let value: Value = serde_json::from_str(&text)
        .with_context(|| format!("failed to parse session snapshot: {}", path.display()))?;
    Ok(value)
}

fn load_graph_value(path: &Path) -> Result<Value> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read graph artifact: {}", path.display()))?;
    let value: Value = serde_json::from_str(&text)
        .with_context(|| format!("failed to parse graph artifact: {}", path.display()))?;
    Ok(value)
}

fn select_terminal_assistant_message(messages: &[Value]) -> Option<String> {
    let mut last_assistant = None;
    let mut last_done = None;
    for msg in messages {
        if msg.get("role").and_then(|v| v.as_str()) == Some("user") {
            last_assistant = None;
            last_done = None;
        }
        if msg.get("role").and_then(|v| v.as_str()) != Some("assistant") {
            continue;
        }
        let Some(content) = msg.get("content").and_then(|v| v.as_str()) else {
            continue;
        };
        let content = content.to_string();
        if content.trim_start().starts_with("[DONE]") {
            last_done = Some(content.clone());
        }
        last_assistant = Some(content);
    }
    last_done.or(last_assistant)
}

fn evaluate_checks(
    case: &RuntimeEvalCase,
    root: &str,
    artifacts: &RuntimeEvalArtifacts,
    metrics: &RuntimeEvalMetrics,
    run_error: Option<&str>,
) -> Vec<RuntimeEvalCheckResult> {
    let mut checks = case.checks.clone();
    if !checks
        .iter()
        .any(|check| matches!(check, RuntimeEvalCheck::Completed))
    {
        checks.insert(0, RuntimeEvalCheck::Completed);
    }
    if !checks
        .iter()
        .any(|check| matches!(check, RuntimeEvalCheck::ErrorFree))
    {
        checks.insert(1, RuntimeEvalCheck::ErrorFree);
    }
    checks
        .iter()
        .map(|check| evaluate_check(check, root, artifacts, metrics, run_error))
        .collect()
}

fn evaluate_check(
    check: &RuntimeEvalCheck,
    root: &str,
    _artifacts: &RuntimeEvalArtifacts,
    metrics: &RuntimeEvalMetrics,
    run_error: Option<&str>,
) -> RuntimeEvalCheckResult {
    match check {
        RuntimeEvalCheck::Completed => RuntimeEvalCheckResult {
            label: "completed".to_string(),
            ok: metrics.completed,
            detail: format!(
                "completed={} agent_outcome={:?}",
                metrics.completed, metrics.task_completed
            ),
        },
        RuntimeEvalCheck::ErrorFree => RuntimeEvalCheckResult {
            label: "error_free".to_string(),
            ok: run_error.is_none() && metrics.error_count == 0,
            detail: match run_error {
                Some(err) => format!("run_error={err}"),
                None => format!("error_count={}", metrics.error_count),
            },
        },
        RuntimeEvalCheck::AssistantContains { value } => {
            let hay = metrics.last_assistant.as_deref().unwrap_or("");
            RuntimeEvalCheckResult {
                label: format!("assistant_contains:{value}"),
                ok: hay.contains(value),
                detail: if hay.is_empty() {
                    "last assistant message missing".to_string()
                } else {
                    format!("matched={}", hay.contains(value))
                },
            }
        }
        RuntimeEvalCheck::AssistantNotContains { value } => {
            let hay = metrics.last_assistant.as_deref().unwrap_or("");
            RuntimeEvalCheckResult {
                label: format!("assistant_not_contains:{value}"),
                ok: !hay.contains(value),
                detail: format!("matched={}", hay.contains(value)),
            }
        }
        RuntimeEvalCheck::ToolCallSeen { name } => {
            let count = metrics.tool_call_histogram.get(name).copied().unwrap_or(0);
            RuntimeEvalCheckResult {
                label: format!("tool_call_seen:{name}"),
                ok: count > 0,
                detail: format!("count={count}"),
            }
        }
        RuntimeEvalCheck::ToolCallMin { name, min } => {
            let count = metrics.tool_call_histogram.get(name).copied().unwrap_or(0);
            RuntimeEvalCheckResult {
                label: format!("tool_call_min:{name}>={min}"),
                ok: count >= *min,
                detail: format!("count={count}"),
            }
        }
        RuntimeEvalCheck::TraceEventSeen { event } => {
            let count = metrics.trace_events.get(event).copied().unwrap_or(0);
            RuntimeEvalCheckResult {
                label: format!("trace_event_seen:{event}"),
                ok: count > 0,
                detail: format!("count={count}"),
            }
        }
        RuntimeEvalCheck::VerifiedCommandSeen { command } => {
            let ok = metrics
                .verified_exec_commands
                .iter()
                .any(|seen| evidence::commands_match(seen, command));
            RuntimeEvalCheckResult {
                label: format!("verified_command_seen:{command}"),
                ok,
                detail: format!(
                    "verified_exec_commands={:?}",
                    metrics.verified_exec_commands
                ),
            }
        }
        RuntimeEvalCheck::AutoTestPassed { command } => {
            let command_ok = command.as_ref().is_none_or(|command| {
                metrics
                    .auto_test_command
                    .as_deref()
                    .is_some_and(|actual| evidence::commands_match(actual, command))
            });
            RuntimeEvalCheckResult {
                label: match command {
                    Some(command) => format!("auto_test_passed:{command}"),
                    None => "auto_test_passed".to_string(),
                },
                ok: metrics.fresh_auto_test_pass_count > 0 && command_ok,
                detail: format!(
                    "fresh_auto_test_pass_count={} test_command={:?} command_ok={command_ok}",
                    metrics.fresh_auto_test_pass_count, metrics.auto_test_command
                ),
            }
        }
        RuntimeEvalCheck::ToolRootFileExists { path } => {
            let resolved = {
                let raw = PathBuf::from(path);
                if raw.is_absolute() {
                    raw
                } else {
                    Path::new(root).join(raw)
                }
            };
            let exists = resolved.exists();
            RuntimeEvalCheckResult {
                label: format!("tool_root_file_exists:{path}"),
                ok: exists,
                detail: format!("resolved={} exists={exists}", resolved.display()),
            }
        }
        RuntimeEvalCheck::ToolRootFileContains { path, value } => {
            let resolved = {
                let raw = PathBuf::from(path);
                if raw.is_absolute() {
                    raw
                } else {
                    Path::new(root).join(raw)
                }
            };
            let content = std::fs::read_to_string(&resolved);
            let ok = content
                .as_ref()
                .is_ok_and(|body| body.contains(value.as_str()));
            RuntimeEvalCheckResult {
                label: format!("tool_root_file_contains:{path}:{value}"),
                ok,
                detail: match content {
                    Ok(body) => format!(
                        "resolved={} bytes={} matched={ok}",
                        resolved.display(),
                        body.len()
                    ),
                    Err(err) => format!("resolved={} error={err}", resolved.display()),
                },
            }
        }
        RuntimeEvalCheck::ToolRootFileEquals { path, value } => {
            let resolved = Path::new(root).join(path);
            let content = std::fs::read(&resolved);
            let ok = content.as_ref().is_ok_and(|body| body == value.as_bytes());
            RuntimeEvalCheckResult {
                label: format!("tool_root_file_equals:{path}"),
                ok,
                detail: match content {
                    Ok(body) => format!(
                        "resolved={} bytes={} expected_bytes={} exact_match={ok}",
                        resolved.display(),
                        body.len(),
                        value.len()
                    ),
                    Err(err) => format!("resolved={} error={err}", resolved.display()),
                },
            }
        }
        RuntimeEvalCheck::MessagesMin { min } => RuntimeEvalCheckResult {
            label: format!("messages_min:{min}"),
            ok: metrics.messages_len >= *min,
            detail: format!("messages={}", metrics.messages_len),
        },
        RuntimeEvalCheck::GraphNodesMin { min } => RuntimeEvalCheckResult {
            label: format!("graph_nodes_min:{min}"),
            ok: metrics.graph_nodes >= *min,
            detail: format!("graph_nodes={}", metrics.graph_nodes),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn write_json(path: &Path, value: &Value) {
        std::fs::write(path, serde_json::to_string_pretty(value).unwrap()).unwrap();
    }

    #[test]
    fn sanitize_case_id_collapses_noise() {
        assert_eq!(sanitize_case_id("  Hello World / 42  "), "hello-world-42");
        assert_eq!(sanitize_case_id("???"), "case");
    }

    #[test]
    fn load_spec_parses_copy_tool_root_flags() {
        let spec: RuntimeEvalSpec = serde_json::from_value(serde_json::json!({
            "version": 1,
            "defaults": {
                "tool_root": ".",
                "copy_tool_root": true
            },
            "cases": [
                {
                    "id": "copy-case",
                    "prompt": "do work",
                    "copy_tool_root": false,
                    "session_seed": "seed-session.json"
                }
            ]
        }))
        .expect("spec");

        assert!(spec.defaults.copy_tool_root);
        assert_eq!(spec.cases[0].copy_tool_root, Some(false));
        assert_eq!(
            spec.cases[0].session_seed.as_deref(),
            Some("seed-session.json")
        );
    }

    #[test]
    fn select_terminal_assistant_message_prefers_latest_done_summary() {
        let messages = vec![
            serde_json::json!({"role":"assistant","content":"intermediate summary"}),
            serde_json::json!({"role":"assistant","content":"[DONE]\nCreated `maze_game/src/lib.rs` and verified it."}),
            serde_json::json!({"role":"assistant","content":"generic postscript"}),
        ];

        let selected =
            select_terminal_assistant_message(&messages).expect("terminal assistant summary");

        assert!(selected.contains("maze_game/src/lib.rs"));
        assert!(selected.starts_with("[DONE]"));
    }

    #[test]
    fn evaluate_case_collects_metrics_and_checks() {
        let dir = tempdir().unwrap();
        let trace_path = dir.path().join("trace.jsonl");
        let session_path = dir.path().join("session.json");
        let json_path = dir.path().join("final.json");
        let graph_path = dir.path().join("graph.json");
        std::fs::write(
            &trace_path,
            concat!(
                "{\"event\":\"agent_start\",\"data\":{\"cfg\":{\"provider\":\"openai-compatible\",\"model\":\"gpt-5-mini\",\"chat_model\":\"gpt-5-mini\",\"code_model\":\"gpt-5-mini\",\"base_url\":\"https://api.openai.com/v1\",\"mode\":\"VIBE\",\"persona\":\"default\",\"temperature\":0.4,\"max_tokens\":1024,\"timeout_seconds\":120},\"lang\":\"en\",\"max_iters\":14}}\n",
                "{\"event\":\"tool_call\",\"data\":{\"name\":\"search_files\"}}\n",
                "{\"event\":\"tool_call\",\"data\":{\"name\":\"read_file\"}}\n",
                "{\"event\":\"reflection_ledger_loaded\",\"data\":{\"entries\":2}}\n",
                "{\"event\":\"reflection_ledger_prompted\",\"data\":{\"entries\":2,\"iter\":1}}\n",
                "{\"event\":\"reflection_ledger_action_hit\",\"data\":{\"tool\":\"read_file\",\"score\":0.88}}\n",
                "{\"event\":\"reflection_ledger_remembered\",\"data\":{\"entries\":3,\"count\":2}}\n",
                "{\"event\":\"provider_retry\",\"data\":{\"provider\":\"mistral\",\"status\":429,\"delay_ms\":5000}}\n",
                "{\"event\":\"done\",\"data\":{}}\n",
                "{\"event\":\"agent_outcome\",\"data\":{\"completed\":true}}\n",
                "{\"event\":\"agent_end\",\"data\":{\"ok\":true}}\n"
            ),
        )
        .unwrap();
        write_json(
            &json_path,
            &serde_json::json!({
                "messages": [
                    {"role": "system", "content": "sys"},
                    {
                        "role": "assistant",
                        "tool_calls": [{
                            "id": "call_patch",
                            "type": "function",
                            "function": {
                                "name": "patch_file",
                                "arguments": "{\"path\":\"created.flag\",\"search\":\"ok\",\"replace\":\"ok\"}"
                            }
                        }]
                    },
                    {
                        "role": "tool",
                        "tool_call_id": "call_patch",
                        "content": "OK: patched 'created.flag'\n[auto-test] ✓ PASSED (exit 0)"
                    },
                    {
                        "role": "assistant",
                        "tool_calls": [{
                            "id": "call_exec",
                            "type": "function",
                            "function": {
                                "name": "exec",
                                "arguments": "{\"command\":\"grep -q ok created.flag\"}"
                            }
                        }]
                    },
                    {
                        "role": "tool",
                        "tool_call_id": "call_exec",
                        "content": "OK (exit_code: 0)\nstdout:\nok"
                    },
                    {
                        "role": "assistant",
                        "content": "[DONE]\nFound it in src/tui/events.rs and verified with `grep -q ok created.flag`."
                    }
                ]
            }),
        );
        write_json(
            &session_path,
            &serde_json::json!({
                "messages": [
                    {"role": "assistant", "content": "fallback"}
                ]
            }),
        );
        write_json(
            &graph_path,
            &serde_json::json!({
                "nodes": [{"id":"1"},{"id":"2"}],
                "edges": [{"from":"1","to":"2"}]
            }),
        );

        let case = RuntimeEvalCase {
            id: "realize-events".to_string(),
            prompt: "locate slash command".to_string(),
            tool_root: None,
            session_seed: None,
            copy_tool_root: None,
            lang: None,
            max_iters: None,
            autofix: None,
            tags: vec!["smoke".to_string()],
            checks: vec![
                RuntimeEvalCheck::Completed,
                RuntimeEvalCheck::ErrorFree,
                RuntimeEvalCheck::AssistantContains {
                    value: "src/tui/events.rs".to_string(),
                },
                RuntimeEvalCheck::ToolCallSeen {
                    name: "read_file".to_string(),
                },
                RuntimeEvalCheck::ToolCallMin {
                    name: "search_files".to_string(),
                    min: 1,
                },
                RuntimeEvalCheck::VerifiedCommandSeen {
                    command: "grep -q ok created.flag".to_string(),
                },
                RuntimeEvalCheck::AutoTestPassed { command: None },
                RuntimeEvalCheck::ToolRootFileExists {
                    path: "created.flag".to_string(),
                },
                RuntimeEvalCheck::ToolRootFileContains {
                    path: "created.flag".to_string(),
                    value: "ok".to_string(),
                },
                RuntimeEvalCheck::MessagesMin { min: 2 },
                RuntimeEvalCheck::GraphNodesMin { min: 2 },
            ],
        };
        std::fs::write(dir.path().join("created.flag"), "ok").unwrap();

        let report = evaluate_case(
            &case,
            dir.path().to_str().unwrap(),
            RuntimeEvalArtifacts {
                case_dir: dir.path().to_path_buf(),
                trace_path,
                session_path,
                json_path,
                graph_path,
            },
            123,
            None,
        )
        .unwrap();

        assert!(report.ok);
        assert_eq!(report.agent.provider.as_deref(), Some("openai-compatible"));
        assert_eq!(report.agent.model.as_deref(), Some("gpt-5-mini"));
        assert_eq!(
            report.agent.base_url.as_deref(),
            Some("https://api.openai.com/v1")
        );
        assert_eq!(report.agent.lang.as_deref(), Some("en"));
        assert_eq!(report.agent.max_iters, Some(14));
        assert_eq!(report.metrics.tool_call_count, 2);
        assert_eq!(report.metrics.graph_nodes, 2);
        assert_eq!(report.metrics.messages_len, 6);
        assert_eq!(
            report.metrics.successful_exec_commands,
            vec!["grep -q ok created.flag".to_string()]
        );
        assert_eq!(report.metrics.auto_test_pass_count, 1);
        assert!(report.metrics.transcript_input_tokens_est > 0);
        assert!(report.metrics.transcript_output_tokens_est > 0);
        assert_eq!(
            report.metrics.transcript_total_tokens_est,
            report.metrics.transcript_input_tokens_est
                + report.metrics.transcript_output_tokens_est
        );
        assert!(report.metrics.last_assistant_tokens_est.unwrap_or(0) > 0);
        assert_eq!(report.metrics.repo_map_fallback_count, 0);
        assert_eq!(report.metrics.provider_retry_count, 1);
        assert_eq!(report.metrics.provider_retry_total_delay_ms, 5000);
        assert_eq!(report.metrics.provider_retry_max_delay_ms, 5000);
        assert_eq!(report.metrics.reflection_ledger_entries_last, Some(3));
        assert_eq!(report.metrics.reflection_ledger_prompt_count, 1);
        assert_eq!(report.metrics.reflection_ledger_action_hit_count, 1);
        assert_eq!(report.metrics.reflection_ledger_remember_count, 1);
        assert_eq!(
            report
                .metrics
                .reflection_ledger_action_hit_histogram
                .get("read_file"),
            Some(&1)
        );
        assert_eq!(
            report.metrics.provider_retry_histogram.get("mistral:429"),
            Some(&1)
        );
        assert!(report
            .metrics
            .last_assistant
            .unwrap_or_default()
            .contains("src/tui/events.rs"));
    }

    #[test]
    fn summarize_cases_includes_transcript_token_averages() {
        let summary = summarize_cases(&[
            RuntimeEvalCaseReport {
                id: "a".to_string(),
                ok: true,
                root: ".".to_string(),
                duration_ms: 10,
                prompt: "prompt".to_string(),
                tags: vec![],
                agent: RuntimeEvalAgentConfig::default(),
                run_error: None,
                artifacts: RuntimeEvalArtifacts {
                    case_dir: PathBuf::from("."),
                    trace_path: PathBuf::from("trace.jsonl"),
                    session_path: PathBuf::from("session.json"),
                    json_path: PathBuf::from("final.json"),
                    graph_path: PathBuf::from("graph.json"),
                },
                metrics: RuntimeEvalMetrics {
                    tool_call_count: 1,
                    iteration_count: 2,
                    messages_len: 3,
                    transcript_input_tokens_est: 100,
                    transcript_output_tokens_est: 40,
                    transcript_total_tokens_est: 140,
                    ..RuntimeEvalMetrics::default()
                },
                checks: vec![],
            },
            RuntimeEvalCaseReport {
                id: "b".to_string(),
                ok: false,
                root: ".".to_string(),
                duration_ms: 30,
                prompt: "prompt".to_string(),
                tags: vec![],
                agent: RuntimeEvalAgentConfig::default(),
                run_error: Some("boom".to_string()),
                artifacts: RuntimeEvalArtifacts {
                    case_dir: PathBuf::from("."),
                    trace_path: PathBuf::from("trace.jsonl"),
                    session_path: PathBuf::from("session.json"),
                    json_path: PathBuf::from("final.json"),
                    graph_path: PathBuf::from("graph.json"),
                },
                metrics: RuntimeEvalMetrics {
                    tool_call_count: 3,
                    iteration_count: 4,
                    messages_len: 5,
                    transcript_input_tokens_est: 200,
                    transcript_output_tokens_est: 80,
                    transcript_total_tokens_est: 280,
                    ..RuntimeEvalMetrics::default()
                },
                checks: vec![],
            },
        ]);

        assert_eq!(summary.total, 2);
        assert_eq!(summary.avg_transcript_input_tokens_est, 150.0);
        assert_eq!(summary.avg_transcript_output_tokens_est, 60.0);
        assert_eq!(summary.avg_transcript_total_tokens_est, 210.0);
    }

    #[test]
    fn default_checks_fail_on_run_error() {
        let dir = tempdir().unwrap();
        let trace_path = dir.path().join("trace.jsonl");
        let session_path = dir.path().join("session.json");
        let json_path = dir.path().join("final.json");
        let graph_path = dir.path().join("graph.json");
        std::fs::write(
            &trace_path,
            "{\"event\":\"error\",\"data\":{\"message\":\"boom\"}}\n",
        )
        .unwrap();
        write_json(&json_path, &serde_json::json!({"messages": []}));
        write_json(&session_path, &serde_json::json!({"messages": []}));
        write_json(&graph_path, &serde_json::json!({"nodes": [], "edges": []}));
        let case = RuntimeEvalCase {
            id: "broken".to_string(),
            prompt: "x".to_string(),
            tool_root: None,
            session_seed: None,
            copy_tool_root: None,
            lang: None,
            max_iters: None,
            autofix: None,
            tags: vec![],
            checks: vec![],
        };
        let report = evaluate_case(
            &case,
            ".",
            RuntimeEvalArtifacts {
                case_dir: dir.path().to_path_buf(),
                trace_path,
                session_path,
                json_path,
                graph_path,
            },
            5,
            Some("agent failed".to_string()),
        )
        .unwrap();
        assert!(!report.ok);
        assert_eq!(report.checks.len(), 2);
        assert!(report.checks.iter().any(|c| !c.ok));
    }
}

#[cfg(test)]
mod regression_tests;
