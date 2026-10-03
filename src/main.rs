mod agent_session;
mod approvals;
mod chatbot;
mod config;
mod eval_merge_gate;
mod exec;
mod execution_evidence;
mod file_tools;
mod governor_contract;
mod harness_gate;
mod harness_promotion;
mod lang_detect;
mod loop_detect;
mod merge_gate;
mod modes;
mod observer;
mod pending_commands;
mod pending_edits;
mod personas;
mod progress_state;
mod project;
mod providers;
mod reflection_ledger;
mod repl;
mod repo_map;
mod runtime_eval;
mod server;
mod streaming;
mod task_graph;
mod trace_writer;
mod tui;
mod tui_replay;
mod types;

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use tokio::sync::mpsc;

use crate::chatbot::ChatBot;
use crate::config::{PartialConfig, ProviderPreset};
use crate::server::ServeArgs;
use crate::tui::TuiArgs;

#[derive(Parser, Debug)]
#[command(
    name = "spiral-coder",
    version,
    about = "Spiral-Coder: provider-abstracted chat runtime (CLI + local UI)"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// Prompt (shorthand for `spiral-coder chat \"<prompt>\"`)
    prompt: Option<String>,

    /// Force REPL (even if a prompt is provided)
    #[arg(long)]
    repl: bool,

    #[command(flatten)]
    common: CommonArgs,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// One-shot chat (prints the assistant response and exits)
    Chat {
        /// Prompt text
        prompt: String,
    },

    /// Headless coding agent (agentic loop with tools; like TUI Coder but CLI-only)
    Agent(AgentArgs),

    /// Run a fixture-driven runtime evaluation suite against the headless Coder
    Eval(EvalArgs),

    /// Inspect the latest runtime eval merge gate and rollback preview
    MergeGate(MergeGateArgs),

    /// Replay a deterministic TUI stuck-case and inspect Observer suggestion plumbing
    TuiReplay(TuiReplayArgs),

    /// Print an internal manifest / parity / replay inventory for this repo
    Inventory(InventoryArgs),

    /// Generate a governor-contract promotion candidate from eval-gated harness overlays
    PromoteHarness(PromoteHarnessArgs),

    /// Review `git diff` with Observer (or diff批評) and print critique
    Review(ReviewArgs),

    /// Generate `.spiral-coder.md` template in tool_root (project instructions + test_cmd)
    Init(InitArgs),

    /// Interactive REPL
    Repl,

    /// Local web UI (React) + JSON API
    Serve(ServeArgs),

    /// Terminal UI (dual-pane Coder + Observer, like Claude Code / Aider)
    Tui(TuiArgs),

    /// List built-in values
    List {
        #[arg(value_enum)]
        what: ListWhat,
    },
}

#[derive(ValueEnum, Clone, Debug)]
enum ListWhat {
    Providers,
    Modes,
    Personas,
}

#[derive(ValueEnum, Clone, Debug)]
enum CliProviderPreset {
    #[value(name = "openai")]
    OpenAi,
    #[value(name = "gemini", alias = "google", alias = "google-gemini")]
    Gemini,
    #[value(
        name = "anthropic-compat",
        alias = "claude-compat",
        alias = "anthropic_openai"
    )]
    AnthropicCompat,
    #[value(
        name = "openai-compatible",
        alias = "openai-compat",
        alias = "openai_compat"
    )]
    OpenAiCompatible,
    #[value(name = "mistral")]
    Mistral,
    #[value(name = "anthropic", alias = "claude")]
    Anthropic,
    #[value(name = "hf", alias = "huggingface", alias = "hf-local")]
    Hf,
}

impl From<CliProviderPreset> for ProviderPreset {
    fn from(value: CliProviderPreset) -> Self {
        match value {
            CliProviderPreset::OpenAi => ProviderPreset::OpenAi,
            CliProviderPreset::Gemini => ProviderPreset::Gemini,
            CliProviderPreset::AnthropicCompat => ProviderPreset::AnthropicCompat,
            CliProviderPreset::OpenAiCompatible => ProviderPreset::OpenAiCompatibleCustom,
            CliProviderPreset::Mistral => ProviderPreset::Mistral,
            CliProviderPreset::Anthropic => ProviderPreset::Anthropic,
            CliProviderPreset::Hf => ProviderPreset::HfLocal,
        }
    }
}

#[derive(Args, Debug, Clone)]
struct AgentArgs {
    /// Prompt text. If omitted, pass `--stdin` to read the prompt from stdin.
    prompt: Option<String>,

    /// Working directory for file/exec tools (defaults to current directory)
    #[arg(long, short = 'C', alias = "root")]
    tool_root: Option<String>,

    /// UI / response language hint (ja/en/fr)
    #[arg(long, default_value = "ja")]
    lang: String,

    /// Max tool-call iterations (default: 12, max: 64)
    #[arg(long)]
    max_iters: Option<usize>,

    /// Auto-approve all commands and edits (no prompts)
    #[arg(long, short = 'y')]
    yes: bool,

    /// Disable ALL approval prompts (commands + edits)
    #[arg(long, alias = "no-approvals")]
    no_approval: bool,

    /// Disable approval prompts for `exec` tool calls (and implied commands)
    #[arg(long)]
    no_command_approval: bool,

    /// Disable approval prompts for file edits (write_file/patch_file/apply_diff)
    #[arg(long)]
    no_edit_approval: bool,

    /// Save and resume an agent session from this JSON file.
    /// If the file exists, Spiral-Coder loads it and continues the conversation.
    /// If `-C/--root` is set and the session path is relative, it is resolved under `tool_root`.
    #[arg(long, short = 's', num_args = 0..=1, default_missing_value = ".tmp/spiral_coder_session.json")]
    session: Option<PathBuf>,

    /// Start a new session even if `--session` already exists.
    #[arg(long)]
    new_session: bool,

    /// Auto-fix loop: after the agent finishes, run an Observer diff review and feed it back
    /// to the agent as a follow-up prompt.
    /// Usage:
    /// - `--autofix` (runs 1 round)
    /// - `--autofix 3` (runs 3 rounds)
    /// - `--autofix-rounds 3` (alias)
    #[arg(
        long,
        num_args = 0..=1,
        default_missing_value = "1",
        value_name = "ROUNDS",
        alias = "autofix-rounds"
    )]
    autofix: Option<usize>,

    /// Write a JSONL trace of the agent run (tool calls, checkpoints, errors, done).
    /// If `-C/--root` is set and the path is relative, it is resolved under `tool_root`.
    #[arg(long, alias = "trace_out")]
    trace_out: Option<PathBuf>,

    /// Write the final session snapshot as JSON (messages + tool_calls + tool results).
    /// If `-C/--root` is set and the path is relative, it is resolved under `tool_root`.
    #[arg(long, alias = "json_out")]
    json_out: Option<PathBuf>,

    /// Write an execution graph (nodes+edges) derived from the final session messages.
    /// If `-C/--root` is set and the path is relative, it is resolved under `tool_root`.
    #[arg(long, alias = "graph_out")]
    graph_out: Option<PathBuf>,
}

#[derive(Args, Debug, Clone)]
struct ReviewArgs {
    /// Optional guidance for the review (e.g. "focus on security and tests")
    prompt: Option<String>,

    /// Git repository directory to review (defaults to current directory)
    #[arg(long, short = 'C', alias = "root")]
    tool_root: Option<String>,

    /// Review only staged changes (`git diff --staged`)
    #[arg(long)]
    staged: bool,

    /// Review only unstaged changes (`git diff`)
    #[arg(long)]
    unstaged: bool,

    /// Base revision for a combined diff (`git diff <base>`). Cannot be combined with --staged/--unstaged.
    /// Useful with the agent's printed checkpoint hash.
    #[arg(long)]
    base: Option<String>,

    /// Maximum diff characters to include in the prompt (default: 24000)
    #[arg(long)]
    max_diff_chars: Option<usize>,
}

#[derive(Args, Debug, Clone)]
struct EvalArgs {
    /// Directory to evaluate against (defaults to current directory)
    #[arg(long, short = 'C', alias = "root")]
    tool_root: Option<String>,

    /// Runtime eval spec JSON file
    #[arg(long, default_value = ".spiral-coder/runtime_eval.json")]
    spec: PathBuf,

    /// Output directory for per-case artifacts and the final report
    #[arg(long)]
    out_dir: Option<PathBuf>,

    /// Optional explicit report path (defaults to <out_dir>/report.json)
    #[arg(long)]
    report_out: Option<PathBuf>,

    /// Only run cases whose id or tags contain this substring
    #[arg(long)]
    filter: Option<String>,

    /// Cap the number of selected cases
    #[arg(long)]
    max_cases: Option<usize>,

    /// Continue running remaining cases after a failed case
    #[arg(long)]
    continue_on_error: bool,
}

#[derive(Args, Debug, Clone)]
struct MergeGateArgs {
    /// Directory to inspect (defaults to current directory)
    #[arg(long, short = 'C', alias = "root")]
    tool_root: Option<String>,

    /// Explicit merge_gate.json path (defaults to latest .tmp/runtime_eval_*/merge_gate.json)
    #[arg(long)]
    gate: Option<PathBuf>,

    /// Runtime eval report.json path; merge_gate.json is resolved next to it
    #[arg(long)]
    report: Option<PathBuf>,

    /// Emit machine-readable JSON instead of plain text
    #[arg(long)]
    json: bool,

    /// Exit non-zero unless the merge gate is ready and no rollback is required
    #[arg(long)]
    ci: bool,
}

#[derive(Args, Debug, Clone)]
struct TuiReplayArgs {
    /// Directory to replay against (defaults to current directory)
    #[arg(long, short = 'C', alias = "root")]
    tool_root: Option<String>,

    /// TUI replay spec JSON file
    #[arg(long, default_value = ".spiral-coder/tui_replay.json")]
    spec: PathBuf,

    /// Output directory for per-case artifacts and the final report
    #[arg(long)]
    out_dir: Option<PathBuf>,

    /// Only run cases whose id or tags contain this substring
    #[arg(long)]
    filter: Option<String>,

    /// Cap the number of selected cases
    #[arg(long)]
    max_cases: Option<usize>,
}

#[derive(Args, Debug, Clone)]
struct InventoryArgs {
    /// Directory to inspect (defaults to current directory)
    #[arg(long, short = 'C', alias = "root")]
    tool_root: Option<String>,

    #[arg(value_enum, default_value = "manifest")]
    what: InventoryWhat,

    /// Emit machine-readable JSON instead of plain text
    #[arg(long)]
    json: bool,

    /// Exit non-zero when inventory health meets the selected failure threshold
    #[arg(long)]
    ci: bool,

    /// Failure threshold for `--ci` health checks
    #[arg(long, value_enum, default_value = "red")]
    fail_on: InventoryFailOn,
}

#[derive(Args, Debug, Clone)]
struct PromoteHarnessArgs {
    /// Directory to inspect (defaults to current directory)
    #[arg(long, short = 'C', alias = "root")]
    tool_root: Option<String>,

    /// Source governor contract JSON file (defaults to <cwd>/shared/governor_contract.json)
    #[arg(long)]
    contract: Option<PathBuf>,

    /// Promoted overlay JSON file to read
    #[arg(long)]
    overlay: Option<PathBuf>,

    /// Output candidate JSON file
    #[arg(long)]
    out: Option<PathBuf>,

    /// Minimum number of green eval cases required before a promotion is eligible
    #[arg(long, default_value_t = 1)]
    min_green_cases: usize,

    /// Emit machine-readable JSON instead of plain text
    #[arg(long)]
    json: bool,
}

#[derive(ValueEnum, Clone, Debug)]
enum InventoryWhat {
    Health,
    Manifest,
    Commands,
    Tools,
    ReplayStatus,
    Parity,
    State,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
enum InventoryFailOn {
    Red,
    Yellow,
}

const INVENTORY_CLI_COMMANDS: &[(&str, &str)] = &[
    ("chat", "one-shot chat prompt"),
    ("agent", "headless coding agent loop"),
    ("eval", "fixture-driven runtime eval harness"),
    (
        "merge-gate",
        "inspect runtime eval merge readiness and rollback preview",
    ),
    ("tui-replay", "deterministic TUI stuck-case replay"),
    ("inventory", "repo/runtime manifest and parity surfaces"),
    (
        "promote-harness",
        "generate governor-contract promotion candidates from harness overlays",
    ),
    ("review", "Observer review over git diff"),
    ("init", "write .spiral-coder.md template"),
    ("repl", "interactive REPL"),
    ("serve", "local web UI + JSON API"),
    ("tui", "dual-pane terminal UI"),
    ("list", "list providers/modes/personas"),
];

const INVENTORY_SLASH_COMMANDS: &[(&str, &str)] = &[
    ("/provider", "set pane provider"),
    ("/base_url", "set pane base URL"),
    ("/model", "set pane model"),
    ("/mode", "set pane mode"),
    ("/persona", "set pane persona"),
    ("/temp", "set pane temperature"),
    ("/lang", "set UI language"),
    ("/tab", "switch right-side tab"),
    ("/keys", "show key help"),
    ("/realize", "set coder realize preset"),
    ("/root", "switch project root"),
    ("/find", "locate a message"),
    ("/autofix", "set autofix rounds"),
    ("/diff", "load diff into pane"),
    ("/init", "write .spiral-coder.md"),
    ("/rollback", "restore git checkpoint"),
    ("/help", "show slash help"),
    ("/meta-diagnose", "Observer meta diagnosis of a failure"),
];

const INVENTORY_PARITY_TARGETS: &[(&str, &str, &str)] = &[
    (
        "runtime_eval",
        "quality",
        "headless coder fixture harness with per-case reports",
    ),
    (
        "tui_replay",
        "quality",
        "deterministic observer/coder stuck-case replay",
    ),
    (
        "repo_map",
        "retrieval",
        "offline repo-map indexing plus runtime fallback hooks",
    ),
    (
        "observer_soft_hint",
        "observer",
        "typed Observer suggestion routed back as advisory-only coder hint",
    ),
    (
        "intent_anchor",
        "memory",
        "intent normalization and anchor-driven drift baseline",
    ),
    (
        "resolution_memory",
        "memory",
        "canonical path/evidence memory for typo and alias repair",
    ),
    (
        "typed_state_docs",
        "ops",
        "documented ownership for prefs/session/intent/runtime state",
    ),
    (
        "agent_split_plan",
        "ops",
        "module split plan for the high-churn coder loop",
    ),
    (
        "surface_parity",
        "ux",
        "headless/TUI/Web surfaces staying aligned enough for reuse",
    ),
];

#[derive(Args, Debug, Clone)]
struct InitArgs {
    /// Directory to write `.spiral-coder.md` into (defaults to current directory)
    #[arg(long, short = 'C', alias = "root")]
    tool_root: Option<String>,

    /// Overwrite `.spiral-coder.md` if it already exists
    #[arg(long)]
    force: bool,
}

#[derive(Parser, Clone, Debug)]
struct CommonArgs {
    /// Apply VIBE preset defaults (provider=mistral, model=codestral-latest, mode=VIBE)
    #[arg(long, global = true)]
    vibe: bool,

    #[arg(long, value_enum, global = true)]
    provider: Option<CliProviderPreset>,

    #[arg(long, global = true)]
    model: Option<String>,

    /// Model for chat-like modes (実況/壁打ち). Defaults to --model when omitted.
    #[arg(long, global = true)]
    chat_model: Option<String>,

    /// Model for coding-like modes (VIBE/diff批評/ログ解析). Defaults to --model when omitted.
    #[arg(long, global = true)]
    code_model: Option<String>,

    /// API key (prefer env vars to avoid shell history)
    #[arg(long, global = true)]
    api_key: Option<String>,

    /// Provider base URL (OpenAI-compatible: .../v1, Anthropic: .../v1)
    #[arg(long, global = true)]
    base_url: Option<String>,

    #[arg(long, value_enum, global = true)]
    mode: Option<modes::Mode>,

    #[arg(long, global = true)]
    persona: Option<String>,

    #[arg(long, default_value_t = 0.4, global = true)]
    temperature: f64,

    #[arg(long, default_value_t = 1024, global = true)]
    max_tokens: u32,

    #[arg(long, default_value_t = 120, global = true)]
    timeout_seconds: u64,

    /// Read a diff/patch file and inject it into the prompt (for diff批評)
    #[arg(long, global = true)]
    diff_file: Option<PathBuf>,

    /// Path to a log file (for ログ解析 mode). Defaults to stdin when omitted.
    #[arg(long, global = true)]
    log_file: Option<PathBuf>,

    /// HF: device (auto|cpu|cuda)
    #[arg(long, global = true)]
    device: Option<String>,

    /// HF: local_files_only
    #[arg(long, global = true)]
    hf_local_only: bool,

    /// Read stdin and append it to the prompt
    #[arg(long, global = true)]
    stdin: bool,
}

impl CommonArgs {
    fn to_partial_config(&self) -> PartialConfig {
        let mut partial = PartialConfig {
            vibe: self.vibe,
            provider: None,
            model: self.model.clone(),
            chat_model: self.chat_model.clone(),
            code_model: self.code_model.clone(),
            api_key: self.api_key.clone(),
            base_url: self.base_url.clone(),
            mode: self.mode.clone(),
            persona: self.persona.clone(),
            temperature: Some(self.temperature),
            max_tokens: Some(self.max_tokens),
            timeout_seconds: Some(self.timeout_seconds),
            hf_device: self.device.clone(),
            hf_local_only: if self.hf_local_only { Some(true) } else { None },
        };
        if let Some(preset) = self.provider.clone() {
            crate::config::apply_provider_preset(&mut partial, preset.into());
        }
        partial
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Command::Chat { prompt }) => run_chat(prompt, cli.common).await,
        Some(Command::Agent(args)) => run_agent(args, cli.common).await,
        Some(Command::Eval(args)) => run_eval(args, cli.common).await,
        Some(Command::MergeGate(args)) => run_merge_gate(args).await,
        Some(Command::TuiReplay(args)) => tui_replay::run(args, cli.common).await,
        Some(Command::Inventory(args)) => run_inventory(args).await,
        Some(Command::PromoteHarness(args)) => run_promote_harness(args).await,
        Some(Command::Review(args)) => run_review(args, cli.common).await,
        Some(Command::Init(args)) => run_init(args, cli.common).await,
        Some(Command::Repl) => repl::run(cli.common.to_partial_config()).await,
        Some(Command::Serve(args)) => server::run(args, cli.common.to_partial_config()).await,
        Some(Command::Tui(args)) => tui::run(args, cli.common.to_partial_config()).await,
        Some(Command::List { what }) => {
            run_list(what);
            Ok(())
        }
        None => {
            if let Some(prompt) = cli.prompt {
                return run_chat(prompt, cli.common).await;
            }
            if cli.repl {
                return repl::run(cli.common.to_partial_config()).await;
            }
            server::run(
                ServeArgs {
                    host: "127.0.0.1".to_string(),
                    port: 8080,
                },
                cli.common.to_partial_config(),
            )
            .await
        }
    }
}

fn run_list(what: ListWhat) {
    match what {
        ListWhat::Providers => {
            for p in config::supported_providers() {
                println!("{p}");
            }
        }
        ListWhat::Modes => {
            for m in modes::supported_modes() {
                println!("{m}");
            }
        }
        ListWhat::Personas => {
            for p in personas::supported_personas() {
                let label = personas::resolve_persona(p).map(|d| d.label).unwrap_or("");
                if label.trim().is_empty() {
                    println!("{p}");
                } else {
                    println!("{p}\t{label}");
                }
            }
        }
    }
}

async fn run_inventory(args: InventoryArgs) -> Result<()> {
    let root = args.tool_root.clone().or_else(|| {
        std::env::current_dir()
            .ok()
            .map(|p| p.to_string_lossy().into_owned())
    });
    let root = PathBuf::from(root.unwrap_or_else(|| ".".to_string()));

    let value = match args.what {
        InventoryWhat::Health => build_inventory_health(&root)?,
        InventoryWhat::Manifest => build_inventory_manifest(&root)?,
        InventoryWhat::Commands => build_inventory_commands(),
        InventoryWhat::Tools => build_inventory_tools(),
        InventoryWhat::ReplayStatus => build_inventory_replay_status(&root)?,
        InventoryWhat::Parity => build_inventory_parity(&root)?,
        InventoryWhat::State => build_inventory_state(&root)?,
    };

    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&value).context("failed to serialize inventory json")?
        );
    } else {
        match args.what {
            InventoryWhat::Health => print_inventory_health(&value),
            InventoryWhat::Manifest => print_inventory_manifest(&value),
            InventoryWhat::Commands => print_inventory_commands(&value),
            InventoryWhat::Tools => print_inventory_tools(&value),
            InventoryWhat::ReplayStatus => print_inventory_replay_status(&value),
            InventoryWhat::Parity => print_inventory_parity(&value),
            InventoryWhat::State => print_inventory_state(&value),
        }
    }

    if args.ci {
        enforce_inventory_ci(&args, &value)?;
    }
    Ok(())
}

async fn run_promote_harness(args: PromoteHarnessArgs) -> Result<()> {
    let cwd = std::env::current_dir().context("failed to get current directory")?;
    let root = args
        .tool_root
        .clone()
        .or_else(|| Some(cwd.to_string_lossy().into_owned()));
    let root = normalize_tool_root(root).unwrap_or_else(|| ".".to_string());
    let overlay_path = args
        .overlay
        .map(|path| resolve_promote_path(path, &cwd, Some(root.as_str())))
        .unwrap_or_else(|| crate::tui::agent::harness_evolution_overlay_path_for_root(&root));
    let out_path = args
        .out
        .map(|path| resolve_promote_path(path, &cwd, Some(root.as_str())))
        .unwrap_or_else(|| crate::harness_promotion::candidate_path_for_root(&root));
    let contract_path = args
        .contract
        .map(|path| {
            if path.is_absolute() {
                path
            } else {
                cwd.join(path)
            }
        })
        .unwrap_or_else(|| cwd.join("shared/governor_contract.json"));

    let overlay = crate::tui::agent::HarnessGovernorContractOverlay::load(&overlay_path)
        .with_context(|| {
            format!(
                "failed to load promoted harness overlay: {}",
                overlay_path.display()
            )
        })?;
    let contract = crate::governor_contract::load_from_path(&contract_path)?;
    let candidate = crate::harness_promotion::build_promotion_candidate(
        &contract,
        &overlay,
        &contract_path,
        &overlay_path,
        &out_path,
        args.min_green_cases,
    );
    candidate.save_atomic(&out_path)?;

    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&candidate)
                .context("failed to serialize promotion candidate json")?
        );
    } else {
        println!("promotion candidate: {}", out_path.display());
        println!(
            "summary: total={} eligible={} add={} update={} noop={} hold={} invalid={}",
            candidate.summary.total,
            candidate.summary.eligible,
            candidate.summary.add,
            candidate.summary.update,
            candidate.summary.noop,
            candidate.summary.hold,
            candidate.summary.invalid
        );
        if candidate.candidates.is_empty() {
            println!("no promoted harness policies found");
        } else {
            for entry in candidate.candidates.iter().take(6) {
                println!(
                    "- {} [{}] {}",
                    entry.display.title, entry.display.badge, entry.display.subtitle
                );
            }
        }
    }

    Ok(())
}

async fn run_merge_gate(args: MergeGateArgs) -> Result<()> {
    let cwd = std::env::current_dir().context("failed to get current directory")?;
    let root = args
        .tool_root
        .clone()
        .unwrap_or_else(|| cwd.to_string_lossy().into_owned());
    let root =
        normalize_tool_root(Some(root)).unwrap_or_else(|| cwd.to_string_lossy().into_owned());
    let root_path = PathBuf::from(&root);
    let gate_path = resolve_merge_gate_path(&args, &cwd, &root_path)?;
    let gate = crate::eval_merge_gate::load_merge_gate_report(&gate_path)?;
    let view = crate::eval_merge_gate::build_merge_gate_view(&gate, &gate_path);

    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&view)
                .context("failed to serialize merge gate view json")?
        );
    } else {
        print_merge_gate_view(&view);
    }

    if args.ci && !view.ci_ok {
        anyhow::bail!(
            "merge gate is not ready: status={} failed_cases={} rollback_required={}",
            view.status,
            view.failed_cases,
            view.rollback_required
        );
    }

    Ok(())
}

fn resolve_merge_gate_path(args: &MergeGateArgs, cwd: &Path, root: &Path) -> Result<PathBuf> {
    if args.gate.is_some() && args.report.is_some() {
        anyhow::bail!("use either --gate or --report, not both");
    }

    if let Some(path) = args.gate.clone() {
        return Ok(resolve_promote_path(
            path,
            cwd,
            Some(root.to_string_lossy().as_ref()),
        ));
    }

    if let Some(path) = args.report.clone() {
        let report_path = resolve_promote_path(path, cwd, Some(root.to_string_lossy().as_ref()));
        return Ok(crate::eval_merge_gate::default_path_for_report(
            &report_path,
        ));
    }

    crate::eval_merge_gate::latest_path_for_root(root).with_context(|| {
        format!(
            "no merge_gate.json found under {}/.tmp/runtime_eval_*; run `spiral-coder eval -C {} --spec .spiral-coder/runtime_eval.json` first or pass --gate",
            root.display(),
            root.display()
        )
    })
}

fn print_merge_gate_view(view: &crate::eval_merge_gate::EvalMergeGateView) {
    println!("merge gate: {}", view.status);
    println!("gate: {}", view.gate_path);
    println!("report: {}", view.report_path);
    println!(
        "summary: cases={} passed={} failed={} rollback_required={} promoted_overlays={}",
        view.total_cases,
        view.passed_cases,
        view.failed_cases,
        view.rollback_required,
        view.promoted_overlay_count
    );
    println!("ci: {}", if view.ci_ok { "pass" } else { "fail" });

    if !view.cases.is_empty() {
        println!("cases:");
        for case in &view.cases {
            println!(
                "- {} [{}] rollback={}",
                case.id,
                case.status,
                case.rollback_status.label()
            );
            if let Some(path) = case.promoted_overlay_path.as_deref() {
                println!("  promoted_overlay: {path}");
            }
            if let Some(command) = case.rollback_command.as_deref() {
                println!("  rollback_preview: {command}");
            }
        }
    }

    if !view.recommended_actions.is_empty() {
        println!("recommended:");
        for action in &view.recommended_actions {
            println!("- {action}");
        }
    }
}

fn enforce_inventory_ci(args: &InventoryArgs, value: &serde_json::Value) -> Result<()> {
    if !matches!(args.what, InventoryWhat::Health) {
        anyhow::bail!("--ci is only supported with `spiral-coder inventory health`");
    }
    let overall = value
        .get("overall")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let should_fail = match args.fail_on {
        InventoryFailOn::Red => overall == "red",
        InventoryFailOn::Yellow => matches!(overall, "yellow" | "red"),
    };
    if should_fail {
        anyhow::bail!(
            "inventory health check failed (overall={overall}, fail_on={})",
            match args.fail_on {
                InventoryFailOn::Red => "red",
                InventoryFailOn::Yellow => "yellow",
            }
        );
    }
    Ok(())
}

fn build_inventory_health(root: &Path) -> Result<serde_json::Value> {
    let parity = build_inventory_parity(root)?;
    let state = build_inventory_state(root)?;
    let replay = build_inventory_replay_status(root)?;
    let runtime_latest = replay
        .get("runtime_eval")
        .and_then(|v| v.get("latest_report"));
    let tui_latest = replay
        .get("tui_replay")
        .and_then(|v| v.get("latest_report"));
    let runtime_freshness = inventory_report_freshness(runtime_latest);
    let tui_freshness = inventory_report_freshness(tui_latest);

    let runtime_green = parity
        .get("health")
        .and_then(|v| v.get("latest_runtime_eval_green"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let tui_replay_green = parity
        .get("health")
        .and_then(|v| v.get("latest_tui_replay_green"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let split_modules_present = parity
        .get("health")
        .and_then(|v| v.get("split_modules_present"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let parity_missing = parity
        .get("summary")
        .and_then(|v| v.get("missing"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let state_missing = state
        .get("summary")
        .and_then(|v| v.get("missing"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let prefs_present = state
        .get("health")
        .and_then(|v| v.get("prefs_file_present"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let runtime_spec_present = state
        .get("health")
        .and_then(|v| v.get("runtime_eval_spec_present"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let tui_replay_spec_present = state
        .get("health")
        .and_then(|v| v.get("tui_replay_spec_present"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let mut checks = vec![
        json!({
            "key": "runtime_eval_proof",
            "ok": runtime_green,
            "severity": if runtime_green { "info" } else { "warn" },
            "detail": if runtime_green { "latest runtime eval report is green" } else { "latest runtime eval proof is missing or stale" },
            "freshness": runtime_freshness,
        }),
        json!({
            "key": "tui_replay_proof",
            "ok": tui_replay_green,
            "severity": if tui_replay_green { "info" } else { "warn" },
            "detail": if tui_replay_green { "latest TUI replay report is green" } else { "latest TUI replay proof is missing or stale" },
            "freshness": tui_freshness,
        }),
        json!({
            "key": "typed_state",
            "ok": state_missing == 0,
            "severity": if state_missing == 0 { "info" } else { "error" },
            "detail": format!("state inventory missing layers: {state_missing}"),
        }),
        json!({
            "key": "split_progress",
            "ok": split_modules_present >= 4,
            "severity": if split_modules_present >= 4 { "info" } else { "warn" },
            "detail": format!("split modules present: {split_modules_present}/4"),
        }),
        json!({
            "key": "surface_parity",
            "ok": parity_missing == 0,
            "severity": if parity_missing == 0 { "info" } else { "warn" },
            "detail": format!("parity missing surfaces: {parity_missing}"),
        }),
        json!({
            "key": "local_state_files",
            "ok": prefs_present && runtime_spec_present && tui_replay_spec_present,
            "severity": if prefs_present && runtime_spec_present && tui_replay_spec_present { "info" } else { "warn" },
            "detail": format!(
                "prefs={} runtime_eval_spec={} tui_replay_spec={}",
                prefs_present, runtime_spec_present, tui_replay_spec_present
            ),
        }),
        json!({
            "key": "proof_freshness",
            "ok": matches!(runtime_freshness, "fresh" | "recent" | "missing")
                && matches!(tui_freshness, "fresh" | "recent" | "missing"),
            "severity": if matches!(runtime_freshness, "old") || matches!(tui_freshness, "old") {
                "warn"
            } else {
                "info"
            },
            "detail": format!("runtime_eval={} tui_replay={}", runtime_freshness, tui_freshness),
        }),
    ];

    let red = checks.iter().any(|c| {
        !c.get("ok").and_then(|v| v.as_bool()).unwrap_or(false)
            && c.get("severity").and_then(|v| v.as_str()) == Some("error")
    });
    let yellow = checks
        .iter()
        .any(|c| !c.get("ok").and_then(|v| v.as_bool()).unwrap_or(false));
    let overall = if red {
        "red"
    } else if yellow {
        "yellow"
    } else {
        "green"
    };

    let mut next_actions = Vec::new();
    if !runtime_green {
        next_actions.push(json!({
            "key": "runtime_eval_proof",
            "action": "run `spiral-coder eval -C . --spec .spiral-coder/runtime_eval.json`"
        }));
    }
    if !tui_replay_green {
        next_actions.push(json!({
            "key": "tui_replay_proof",
            "action": "run `spiral-coder tui-replay -C . --spec .spiral-coder/tui_replay.json`"
        }));
    }
    if split_modules_present < 4 {
        next_actions.push(json!({
            "key": "split_progress",
            "action": "continue extracting high-churn logic from src/tui/agent.rs"
        }));
    }
    if !prefs_present {
        next_actions.push(json!({
            "key": "local_state_files",
            "action": "open the TUI once and persist project-local prefs into .spiral-coder/tui_prefs.json"
        }));
    }
    if matches!(runtime_freshness, "stale" | "old") && runtime_green {
        next_actions.push(json!({
            "key": "runtime_eval_proof",
            "action": "refresh runtime eval proof so health reflects current behavior"
        }));
    }
    if matches!(tui_freshness, "stale" | "old") && tui_replay_green {
        next_actions.push(json!({
            "key": "tui_replay_proof",
            "action": "refresh TUI replay proof so observer hint health stays current"
        }));
    }

    checks.sort_by_key(
        |c| match c.get("severity").and_then(|v| v.as_str()).unwrap_or("info") {
            "error" => 0,
            "warn" => 1,
            _ => 2,
        },
    );

    Ok(json!({
        "overall": overall,
        "freshness": {
            "runtime_eval": runtime_freshness,
            "tui_replay": tui_freshness,
        },
        "checks": checks,
        "next_actions": next_actions,
        "state": state,
        "parity": parity,
        "replay_status": replay,
    }))
}

fn build_inventory_manifest(root: &Path) -> Result<serde_json::Value> {
    let cargo_toml = root.join("Cargo.toml");
    let cargo_text = std::fs::read_to_string(&cargo_toml).unwrap_or_default();
    let package_name = inventory_toml_value(&cargo_text, "name");
    let package_version = inventory_toml_value(&cargo_text, "version");
    let runtime_spec_path = root.join(".spiral-coder/runtime_eval.json");
    let tui_replay_spec_path = root.join(".spiral-coder/tui_replay.json");
    let runtime_cases = crate::runtime_eval::load_spec(&runtime_spec_path)
        .map(|spec| spec.cases.len())
        .unwrap_or(0);
    let tui_replay_cases = std::fs::read_to_string(&tui_replay_spec_path)
        .ok()
        .and_then(|text| serde_json::from_str::<crate::tui_replay::TuiReplaySpec>(&text).ok())
        .map(|spec| spec.cases.len())
        .unwrap_or(0);
    let providers = crate::config::supported_provider_presets(false)
        .into_iter()
        .map(|preset| {
            json!({
                "key": preset.key(),
                "default_model": preset.default_model(false),
                "coder_supported": preset.coder_supported(),
            })
        })
        .collect::<Vec<_>>();
    let personas = crate::personas::supported_personas()
        .into_iter()
        .map(|key| {
            let label = crate::personas::resolve_persona(key)
                .map(|p| p.label)
                .unwrap_or("");
            json!({ "key": key, "label": label })
        })
        .collect::<Vec<_>>();
    let docs = [
        "README.md",
        "AGENTS.md",
        "docs/runtime-architecture.md",
        "docs/state-schema.md",
        "docs/tui-agent-split-plan.md",
    ]
    .into_iter()
    .map(|rel| {
        let path = root.join(rel);
        json!({
            "path": rel,
            "exists": path.exists(),
        })
    })
    .collect::<Vec<_>>();
    let split_modules = [
        "src/tui/agent/done_gate.rs",
        "src/tui/agent/read_only.rs",
        "src/tui/agent/provider_compat.rs",
        "src/tui/agent/memory.rs",
    ]
    .into_iter()
    .map(|rel| {
        json!({
            "path": rel,
            "exists": root.join(rel).exists(),
        })
    })
    .collect::<Vec<_>>();

    Ok(json!({
        "root": root,
        "package": {
            "name": package_name,
            "version": package_version,
        },
        "providers": providers,
        "modes": crate::modes::supported_modes(),
        "personas": personas,
        "specs": {
            "runtime_eval_cases": runtime_cases,
            "tui_replay_cases": tui_replay_cases,
        },
        "docs": docs,
        "state_files": [
            ".spiral-coder/runtime_eval.json",
            ".spiral-coder/tui_replay.json",
            ".spiral-coder/tui_prefs.json",
            ".spiral-coder/repo_map.config.json",
            ".spiral-coder/repo_map.eval.json",
        ],
        "split_modules": split_modules,
    }))
}

fn build_inventory_commands() -> serde_json::Value {
    json!({
        "cli": INVENTORY_CLI_COMMANDS
            .iter()
            .map(|(name, about)| json!({ "name": name, "about": about }))
            .collect::<Vec<_>>(),
        "slash": INVENTORY_SLASH_COMMANDS
            .iter()
            .map(|(name, about)| json!({ "name": name, "about": about }))
            .collect::<Vec<_>>(),
    })
}

fn build_inventory_tools() -> serde_json::Value {
    let defs = vec![
        crate::tui::agent::exec_tool_def(),
        crate::tui::agent::read_file_tool_def(),
        crate::tui::agent::write_file_tool_def(),
        crate::tui::agent::patch_file_tool_def(),
        crate::tui::agent::search_files_tool_def(),
        crate::tui::agent::apply_diff_tool_def(),
        crate::tui::agent::list_dir_tool_def(),
        crate::tui::agent::glob_tool_def(),
        crate::tui::agent::done_tool_def(),
    ];
    let tools = defs
        .into_iter()
        .filter_map(|def| {
            let function = def.get("function")?;
            let params = function.get("parameters");
            let properties = params
                .and_then(|p| p.get("properties"))
                .and_then(|p| p.as_object())
                .map(|obj| obj.keys().cloned().collect::<Vec<_>>())
                .unwrap_or_default();
            let required = params
                .and_then(|p| p.get("required"))
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            Some(json!({
                "name": function.get("name").and_then(|v| v.as_str()).unwrap_or("unknown"),
                "description": function.get("description").and_then(|v| v.as_str()).unwrap_or(""),
                "property_keys": properties,
                "required": required,
            }))
        })
        .collect::<Vec<_>>();
    json!({ "tools": tools })
}

fn build_inventory_replay_status(root: &Path) -> Result<serde_json::Value> {
    let runtime_spec_path = root.join(".spiral-coder/runtime_eval.json");
    let runtime_cases = crate::runtime_eval::load_spec(&runtime_spec_path)
        .map(|spec| spec.cases)
        .unwrap_or_default();
    let tui_replay_spec_path = root.join(".spiral-coder/tui_replay.json");
    let tui_replay_cases = std::fs::read_to_string(&tui_replay_spec_path)
        .ok()
        .and_then(|text| serde_json::from_str::<crate::tui_replay::TuiReplaySpec>(&text).ok())
        .map(|spec| spec.cases)
        .unwrap_or_default();

    let tmp_dir = root.join(".tmp");
    let latest_runtime = inventory_latest_report_with_prefix(&tmp_dir, "runtime_eval_");
    let latest_tui_replay = inventory_latest_report_with_prefix(&tmp_dir, "tui_replay");

    Ok(json!({
        "runtime_eval": {
            "spec_path": inventory_rel(root, &runtime_spec_path),
            "cases": runtime_cases.iter().map(|case| json!({ "id": case.id, "tags": case.tags })).collect::<Vec<_>>(),
            "latest_report": latest_runtime,
        },
        "tui_replay": {
            "spec_path": inventory_rel(root, &tui_replay_spec_path),
            "cases": tui_replay_cases.iter().map(|case| json!({ "id": case.id, "tags": case.tags })).collect::<Vec<_>>(),
            "latest_report": latest_tui_replay,
        }
    }))
}

fn build_inventory_parity(root: &Path) -> Result<serde_json::Value> {
    let tmp_dir = root.join(".tmp");
    let latest_runtime = inventory_latest_report_with_prefix(&tmp_dir, "runtime_eval_");
    let latest_tui_replay = inventory_latest_report_with_prefix(&tmp_dir, "tui_replay");
    let runtime_green = inventory_report_is_green(latest_runtime.as_ref());
    let tui_replay_green = inventory_report_is_green(latest_tui_replay.as_ref());
    let split_paths = [
        "src/tui/agent/done_gate.rs",
        "src/tui/agent/read_only.rs",
        "src/tui/agent/provider_compat.rs",
        "src/tui/agent/memory.rs",
    ];
    let split_count = split_paths
        .iter()
        .filter(|rel| root.join(rel).exists())
        .count();

    let entries = INVENTORY_PARITY_TARGETS
        .iter()
        .map(|(key, area, detail)| {
            let (status, evidence) = match *key {
                "runtime_eval" => (
                    if runtime_green {
                        "implemented"
                    } else if root.join(".spiral-coder/runtime_eval.json").exists() {
                        "partial"
                    } else {
                        "missing"
                    },
                    if let Some(report) = latest_runtime.as_ref() {
                        report
                            .get("path")
                            .and_then(|v| v.as_str())
                            .unwrap_or(".spiral-coder/runtime_eval.json")
                    } else {
                        ".spiral-coder/runtime_eval.json"
                    },
                ),
                "tui_replay" => (
                    if tui_replay_green {
                        "implemented"
                    } else if root.join(".spiral-coder/tui_replay.json").exists() {
                        "partial"
                    } else {
                        "missing"
                    },
                    if let Some(report) = latest_tui_replay.as_ref() {
                        report
                            .get("path")
                            .and_then(|v| v.as_str())
                            .unwrap_or(".spiral-coder/tui_replay.json")
                    } else {
                        ".spiral-coder/tui_replay.json"
                    },
                ),
                "repo_map" => (
                    if root.join("scripts/repo_map.py").exists()
                        && root.join(".spiral-coder/repo_map.config.json").exists()
                        && root.join(".spiral-coder/repo_map.eval.json").exists()
                    {
                        "implemented"
                    } else {
                        "missing"
                    },
                    "scripts/repo_map.py + .spiral-coder/repo_map.*",
                ),
                "observer_soft_hint" => (
                    if root.join("src/tui/suggestion.rs").exists()
                        && root.join("src/tui/events.rs").exists()
                    {
                        "implemented"
                    } else {
                        "missing"
                    },
                    "src/tui/suggestion.rs",
                ),
                "intent_anchor" => (
                    if root.join("src/tui/intent.rs").exists() {
                        "implemented"
                    } else {
                        "missing"
                    },
                    "src/tui/intent.rs",
                ),
                "resolution_memory" => (
                    if root.join("src/tui/agent/memory.rs").exists() {
                        "implemented"
                    } else {
                        "missing"
                    },
                    "src/tui/agent/memory.rs",
                ),
                "typed_state_docs" => (
                    if root.join("docs/state-schema.md").exists() {
                        "implemented"
                    } else {
                        "missing"
                    },
                    "docs/state-schema.md",
                ),
                "agent_split_plan" => (
                    if root.join("docs/tui-agent-split-plan.md").exists() && split_count == 4 {
                        "implemented"
                    } else if root.join("docs/tui-agent-split-plan.md").exists() && split_count > 0
                    {
                        "partial"
                    } else {
                        "missing"
                    },
                    "docs/tui-agent-split-plan.md + src/tui/agent/*",
                ),
                "surface_parity" => (
                    if runtime_green && tui_replay_green {
                        "implemented"
                    } else if root.join("src/tui").exists() && root.join("web").exists() {
                        "partial"
                    } else {
                        "missing"
                    },
                    "runtime_eval + tui_replay + TUI/Web surfaces",
                ),
                _ => ("unknown", "-"),
            };
            json!({
                "key": key,
                "area": area,
                "status": status,
                "detail": detail,
                "evidence": evidence,
            })
        })
        .collect::<Vec<_>>();
    let implemented = entries
        .iter()
        .filter(|entry| entry.get("status").and_then(|v| v.as_str()) == Some("implemented"))
        .count();
    let partial = entries
        .iter()
        .filter(|entry| entry.get("status").and_then(|v| v.as_str()) == Some("partial"))
        .count();
    let missing = entries
        .iter()
        .filter(|entry| entry.get("status").and_then(|v| v.as_str()) == Some("missing"))
        .count();
    let gaps = entries
        .iter()
        .filter_map(|entry| {
            let status = entry
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            if status == "implemented" {
                return None;
            }
            let key = entry
                .get("key")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let next = match key {
                "runtime_eval" => {
                    "run `spiral-coder eval -C . --spec .spiral-coder/runtime_eval.json` to refresh proof"
                }
                "tui_replay" => {
                    "run `spiral-coder tui-replay -C . --spec .spiral-coder/tui_replay.json` to refresh proof"
                }
                "surface_parity" => {
                    "compare TUI/Web/headless flows and promote shared checks into replay/eval"
                }
                "agent_split_plan" => "continue extracting high-churn logic from src/tui/agent.rs",
                _ => "add missing wiring or proof artifact",
            };
            Some(json!({
                "key": key,
                "status": status,
                "next_action": next,
            }))
        })
        .collect::<Vec<_>>();
    Ok(json!({
        "summary": {
            "implemented": implemented,
            "partial": partial,
            "missing": missing,
            "total": entries.len(),
        },
        "health": {
            "latest_runtime_eval_green": runtime_green,
            "latest_tui_replay_green": tui_replay_green,
            "split_modules_present": split_count,
        },
        "entries": entries,
        "gaps": gaps,
    }))
}

fn build_inventory_state(root: &Path) -> Result<serde_json::Value> {
    let state_schema_path = root.join("docs/state-schema.md");
    let session_example_path = root.join(".tmp/spiral_coder_session.json");
    let prefs_path = root.join(".spiral-coder/tui_prefs.json");
    let reflection_ledger_path = root.join(".spiral-coder/reflection_ledger.json");
    let harness_queue_path = root.join(".spiral-coder/policy_patch_queue.json");
    let harness_overlay_path = root.join(".spiral-coder/governor_contract.overlay.json");
    let harness_promotion_path = root.join(".spiral-coder/governor_contract.promotion.json");
    let harness_gate_path = root.join(".spiral-coder/governor_contract.promotion_gate.json");
    let runtime_spec_path = root.join(".spiral-coder/runtime_eval.json");
    let replay_spec_path = root.join(".spiral-coder/tui_replay.json");

    let layers = vec![
        json!({
            "key": "runtime_config",
            "owner": "src/config.rs",
            "lifetime": "process / launch",
            "backing_store": "CLI args + env",
            "persisted": false,
            "doc_present": state_schema_path.exists(),
            "owner_present": root.join("src/config.rs").exists(),
            "status": if root.join("src/config.rs").exists() { "implemented" } else { "missing" },
        }),
        json!({
            "key": "tui_prefs",
            "owner": "src/tui/prefs.rs",
            "lifetime": "cross-session",
            "backing_store": ".spiral-coder/tui_prefs.json",
            "persisted": true,
            "doc_present": state_schema_path.exists(),
            "owner_present": root.join("src/tui/prefs.rs").exists(),
            "backing_present": prefs_path.exists(),
            "status": if root.join("src/tui/prefs.rs").exists() && prefs_path.exists() { "implemented" } else if root.join("src/tui/prefs.rs").exists() { "partial" } else { "missing" },
        }),
        json!({
            "key": "session_persistence",
            "owner": "src/agent_session.rs",
            "lifetime": "resumable run",
            "backing_store": "session.json",
            "persisted": true,
            "doc_present": state_schema_path.exists(),
            "owner_present": root.join("src/agent_session.rs").exists(),
            "backing_present": session_example_path.exists(),
            "status": if root.join("src/agent_session.rs").exists() { "implemented" } else { "missing" },
        }),
        json!({
            "key": "reflection_ledger",
            "owner": "src/reflection_ledger.rs",
            "lifetime": "cross-session",
            "backing_store": ".spiral-coder/reflection_ledger.json",
            "persisted": true,
            "doc_present": state_schema_path.exists(),
            "owner_present": root.join("src/reflection_ledger.rs").exists(),
            "backing_present": reflection_ledger_path.exists(),
            "status": if root.join("src/reflection_ledger.rs").exists() && reflection_ledger_path.exists() { "implemented" } else if root.join("src/reflection_ledger.rs").exists() { "partial" } else { "missing" },
        }),
        json!({
            "key": "harness_evolution_queue",
            "owner": "src/tui/agent/harness_evolution.rs",
            "lifetime": "cross-session",
            "backing_store": ".spiral-coder/policy_patch_queue.json",
            "persisted": true,
            "doc_present": state_schema_path.exists(),
            "owner_present": root.join("src/tui/agent/harness_evolution.rs").exists(),
            "backing_present": harness_queue_path.exists(),
            "status": if root.join("src/tui/agent/harness_evolution.rs").exists() && harness_queue_path.exists() { "implemented" } else if root.join("src/tui/agent/harness_evolution.rs").exists() { "partial" } else { "missing" },
        }),
        json!({
            "key": "promoted_governor_overlay",
            "owner": "src/tui/agent/harness_evolution.rs",
            "lifetime": "cross-session",
            "backing_store": ".spiral-coder/governor_contract.overlay.json",
            "persisted": true,
            "doc_present": state_schema_path.exists(),
            "owner_present": root.join("src/tui/agent/harness_evolution.rs").exists(),
            "backing_present": harness_overlay_path.exists(),
            "status": if root.join("src/tui/agent/harness_evolution.rs").exists() && harness_overlay_path.exists() { "implemented" } else if root.join("src/tui/agent/harness_evolution.rs").exists() { "partial" } else { "missing" },
        }),
        json!({
            "key": "governor_contract_promotion",
            "owner": "src/harness_promotion.rs",
            "lifetime": "generated candidate artifact",
            "backing_store": ".spiral-coder/governor_contract.promotion.json",
            "persisted": true,
            "doc_present": state_schema_path.exists(),
            "owner_present": root.join("src/harness_promotion.rs").exists(),
            "backing_present": harness_promotion_path.exists(),
            "status": if root.join("src/harness_promotion.rs").exists() && harness_promotion_path.exists() { "implemented" } else if root.join("src/harness_promotion.rs").exists() { "partial" } else { "missing" },
        }),
        json!({
            "key": "governor_contract_promotion_gate",
            "owner": "src/harness_gate.rs",
            "lifetime": "cross-session",
            "backing_store": ".spiral-coder/governor_contract.promotion_gate.json",
            "persisted": true,
            "doc_present": state_schema_path.exists(),
            "owner_present": root.join("src/harness_gate.rs").exists(),
            "backing_present": harness_gate_path.exists(),
            "status": if root.join("src/harness_gate.rs").exists() && harness_gate_path.exists() { "implemented" } else if root.join("src/harness_gate.rs").exists() { "partial" } else { "missing" },
        }),
        json!({
            "key": "in_memory_app",
            "owner": "src/tui/app.rs",
            "lifetime": "live TUI session",
            "backing_store": "memory only",
            "persisted": false,
            "doc_present": state_schema_path.exists(),
            "owner_present": root.join("src/tui/app.rs").exists(),
            "status": if root.join("src/tui/app.rs").exists() { "implemented" } else { "missing" },
        }),
        json!({
            "key": "intent_anchor",
            "owner": "src/tui/intent.rs",
            "lifetime": "live session",
            "backing_store": "memory only today",
            "persisted": false,
            "doc_present": state_schema_path.exists(),
            "owner_present": root.join("src/tui/intent.rs").exists(),
            "status": if root.join("src/tui/intent.rs").exists() { "implemented" } else { "missing" },
        }),
        json!({
            "key": "runtime_eval_fixture",
            "owner": "src/runtime_eval.rs",
            "lifetime": "versioned test input/output",
            "backing_store": ".spiral-coder/runtime_eval.json + .tmp/runtime_eval_*",
            "persisted": true,
            "doc_present": state_schema_path.exists(),
            "owner_present": root.join("src/runtime_eval.rs").exists(),
            "backing_present": runtime_spec_path.exists(),
            "status": if root.join("src/runtime_eval.rs").exists() && runtime_spec_path.exists() { "implemented" } else if root.join("src/runtime_eval.rs").exists() { "partial" } else { "missing" },
        }),
        json!({
            "key": "tui_replay_fixture",
            "owner": "src/tui_replay.rs",
            "lifetime": "versioned test input/output",
            "backing_store": ".spiral-coder/tui_replay.json + .tmp/tui_replay_*",
            "persisted": true,
            "doc_present": state_schema_path.exists(),
            "owner_present": root.join("src/tui_replay.rs").exists(),
            "backing_present": replay_spec_path.exists(),
            "status": if root.join("src/tui_replay.rs").exists() && replay_spec_path.exists() { "implemented" } else if root.join("src/tui_replay.rs").exists() { "partial" } else { "missing" },
        }),
    ];

    let implemented = layers
        .iter()
        .filter(|entry| entry.get("status").and_then(|v| v.as_str()) == Some("implemented"))
        .count();
    let partial = layers
        .iter()
        .filter(|entry| entry.get("status").and_then(|v| v.as_str()) == Some("partial"))
        .count();
    let missing = layers
        .iter()
        .filter(|entry| entry.get("status").and_then(|v| v.as_str()) == Some("missing"))
        .count();
    let persisted = layers
        .iter()
        .filter(|entry| {
            entry
                .get("persisted")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
        })
        .count();

    let gaps = layers
        .iter()
        .filter_map(|entry| {
            let status = entry.get("status").and_then(|v| v.as_str()).unwrap_or("unknown");
            if status == "implemented" {
                return None;
            }
            let key = entry.get("key").and_then(|v| v.as_str()).unwrap_or("unknown");
            let next = match key {
                "tui_prefs" => "load/save a project-local .spiral-coder/tui_prefs.json at least once to confirm persistence",
                "harness_evolution_queue" => "run a loop-triggering eval case to seed .spiral-coder/policy_patch_queue.json",
                "promoted_governor_overlay" => "run `spiral-coder eval` on a loop-triggering case so promoted overlays are written",
                "governor_contract_promotion" => "run `spiral-coder promote-harness --json` to generate a UI-ready promotion candidate",
                "governor_contract_promotion_gate" => "review a promotion candidate in TUI/GUI or call the harness gate API so .spiral-coder/governor_contract.promotion_gate.json is created",
                "runtime_eval_fixture" => "keep .spiral-coder/runtime_eval.json in sync with current regression cases",
                "tui_replay_fixture" => "keep .spiral-coder/tui_replay.json aligned with observer stuck-case coverage",
                _ => "add missing owner or backing store",
            };
            Some(json!({
                "key": key,
                "status": status,
                "next_action": next,
            }))
        })
        .collect::<Vec<_>>();

    Ok(json!({
        "schema_doc": {
            "path": inventory_rel(root, &state_schema_path),
            "exists": state_schema_path.exists(),
        },
        "summary": {
            "implemented": implemented,
            "partial": partial,
            "missing": missing,
            "persisted_layers": persisted,
            "total": layers.len(),
        },
        "health": {
            "prefs_file_present": prefs_path.exists(),
            "runtime_eval_spec_present": runtime_spec_path.exists(),
            "tui_replay_spec_present": replay_spec_path.exists(),
            "session_example_present": session_example_path.exists(),
        },
        "layers": layers,
        "gaps": gaps,
    }))
}

fn inventory_latest_report_with_prefix(tmp_dir: &Path, prefix: &str) -> Option<serde_json::Value> {
    let mut best: Option<(PathBuf, SystemTime)> = None;
    for entry in std::fs::read_dir(tmp_dir).ok()? {
        let entry = entry.ok()?;
        let path = entry.path();
        let name = path.file_name()?.to_str()?;
        if !name.starts_with(prefix) {
            continue;
        }
        let report_path = path.join("report.json");
        if !report_path.exists() {
            continue;
        }
        let modified = report_path.metadata().ok()?.modified().ok()?;
        if best
            .as_ref()
            .map(|(_, current)| modified > *current)
            .unwrap_or(true)
        {
            best = Some((report_path, modified));
        }
    }
    let (report_path, modified) = best?;
    let report: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&report_path).ok()?).ok()?;
    Some(json!({
        "path": report_path,
        "modified_ms": modified.duration_since(SystemTime::UNIX_EPOCH).ok().map(|d| d.as_millis()),
        "age_ms": modified.elapsed().ok().map(|d| d.as_millis()),
        "freshness": inventory_modified_freshness(modified),
        "summary": report.get("summary").cloned().unwrap_or_else(|| json!({})),
    }))
}

fn inventory_report_is_green(report: Option<&serde_json::Value>) -> bool {
    let Some(report) = report else {
        return false;
    };
    let passed = report
        .get("summary")
        .and_then(|summary| summary.get("passed"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let total = report
        .get("summary")
        .and_then(|summary| summary.get("total"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    total > 0 && passed == total
}

fn inventory_report_freshness(report: Option<&serde_json::Value>) -> &'static str {
    match report
        .and_then(|report| report.get("freshness"))
        .and_then(|v| v.as_str())
    {
        Some("fresh") => "fresh",
        Some("recent") => "recent",
        Some("stale") => "stale",
        Some("old") => "old",
        Some("unknown") => "unknown",
        _ => "missing",
    }
}

fn inventory_modified_freshness(modified: SystemTime) -> &'static str {
    let age = modified.elapsed().ok();
    let Some(age) = age else {
        return "unknown";
    };
    let secs = age.as_secs();
    if secs <= 6 * 60 * 60 {
        "fresh"
    } else if secs <= 3 * 24 * 60 * 60 {
        "recent"
    } else if secs <= 7 * 24 * 60 * 60 {
        "stale"
    } else {
        "old"
    }
}

fn inventory_toml_value(text: &str, key: &str) -> Option<String> {
    let mut in_package = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_package = trimmed == "[package]";
            continue;
        }
        if !in_package || !trimmed.starts_with(key) {
            continue;
        }
        let (_, value) = trimmed.split_once('=')?;
        let value = value.trim().trim_matches('"');
        if !value.is_empty() {
            return Some(value.to_string());
        }
    }
    None
}

fn inventory_rel(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
}

fn print_inventory_manifest(value: &serde_json::Value) {
    println!("Manifest");
    if let Some(root) = value.get("root").and_then(|v| v.as_str()) {
        println!("root: {root}");
    }
    if let Some(pkg) = value.get("package") {
        let name = pkg
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let version = pkg
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        println!("package: {name} {version}");
    }
    println!(
        "providers: {} | modes: {} | personas: {}",
        value
            .get("providers")
            .and_then(|v| v.as_array())
            .map(|v| v.len())
            .unwrap_or(0),
        value
            .get("modes")
            .and_then(|v| v.as_array())
            .map(|v| v.len())
            .unwrap_or(0),
        value
            .get("personas")
            .and_then(|v| v.as_array())
            .map(|v| v.len())
            .unwrap_or(0),
    );
    if let Some(specs) = value.get("specs") {
        println!(
            "specs: runtime_eval={} | tui_replay={}",
            specs
                .get("runtime_eval_cases")
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            specs
                .get("tui_replay_cases")
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
        );
    }
}

fn print_inventory_health(value: &serde_json::Value) {
    println!(
        "Health: {}",
        value
            .get("overall")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
    );
    if let Some(freshness) = value.get("freshness") {
        println!(
            "freshness: runtime_eval={} | tui_replay={}",
            freshness
                .get("runtime_eval")
                .and_then(|v| v.as_str())
                .unwrap_or("missing"),
            freshness
                .get("tui_replay")
                .and_then(|v| v.as_str())
                .unwrap_or("missing"),
        );
    }
    if let Some(checks) = value.get("checks").and_then(|v| v.as_array()) {
        for check in checks {
            println!(
                "- {:<18} {:<5} {}{}",
                check
                    .get("key")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown"),
                if check.get("ok").and_then(|v| v.as_bool()).unwrap_or(false) {
                    "ok"
                } else {
                    "bad"
                },
                check.get("detail").and_then(|v| v.as_str()).unwrap_or(""),
                check
                    .get("freshness")
                    .and_then(|v| v.as_str())
                    .map(|f| format!(" [{f}]"))
                    .unwrap_or_default(),
            );
        }
    }
    if let Some(actions) = value.get("next_actions").and_then(|v| v.as_array()) {
        if !actions.is_empty() {
            println!("\nNext Actions");
            for action in actions {
                println!(
                    "- {}",
                    action.get("action").and_then(|v| v.as_str()).unwrap_or("")
                );
            }
        }
    }
}

fn print_inventory_commands(value: &serde_json::Value) {
    println!("CLI Commands");
    if let Some(cli) = value.get("cli").and_then(|v| v.as_array()) {
        for item in cli {
            println!(
                "- {}: {}",
                item.get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown"),
                item.get("about").and_then(|v| v.as_str()).unwrap_or("")
            );
        }
    }
    println!("\nSlash Commands");
    if let Some(slash) = value.get("slash").and_then(|v| v.as_array()) {
        for item in slash {
            println!(
                "- {}: {}",
                item.get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown"),
                item.get("about").and_then(|v| v.as_str()).unwrap_or("")
            );
        }
    }
}

fn print_inventory_tools(value: &serde_json::Value) {
    println!("Tools");
    if let Some(tools) = value.get("tools").and_then(|v| v.as_array()) {
        for tool in tools {
            let props = tool
                .get("property_keys")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_default();
            println!(
                "- {} [{}]",
                tool.get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown"),
                props
            );
        }
    }
}

fn print_inventory_replay_status(value: &serde_json::Value) {
    println!("Replay Status");
    for key in ["runtime_eval", "tui_replay"] {
        let section = &value[key];
        let case_count = section
            .get("cases")
            .and_then(|v| v.as_array())
            .map(|v| v.len())
            .unwrap_or(0);
        println!("- {key}: {case_count} cases");
        if let Some(latest) = section.get("latest_report") {
            let path = latest.get("path").and_then(|v| v.as_str()).unwrap_or("-");
            let passed = latest
                .get("summary")
                .and_then(|summary| summary.get("passed"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let total = latest
                .get("summary")
                .and_then(|summary| summary.get("total"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let freshness = latest
                .get("freshness")
                .and_then(|v| v.as_str())
                .unwrap_or("missing");
            println!("  latest: {path} ({passed}/{total} passed, freshness={freshness})");
        }
    }
}

fn print_inventory_parity(value: &serde_json::Value) {
    println!("Parity");
    if let Some(summary) = value.get("summary") {
        let implemented = summary
            .get("implemented")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let partial = summary.get("partial").and_then(|v| v.as_u64()).unwrap_or(0);
        let missing = summary.get("missing").and_then(|v| v.as_u64()).unwrap_or(0);
        let total = summary.get("total").and_then(|v| v.as_u64()).unwrap_or(0);
        println!(
            "summary: implemented={implemented} partial={partial} missing={missing} total={total}"
        );
    }
    if let Some(health) = value.get("health") {
        println!(
            "health: runtime_eval={} | tui_replay={} | split_modules={}",
            health
                .get("latest_runtime_eval_green")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            health
                .get("latest_tui_replay_green")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            health
                .get("split_modules_present")
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
        );
    }
    if let Some(entries) = value.get("entries").and_then(|v| v.as_array()) {
        for entry in entries {
            println!(
                "- {:<22} {:<11} {}",
                entry
                    .get("key")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown"),
                entry
                    .get("status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown"),
                entry.get("detail").and_then(|v| v.as_str()).unwrap_or("")
            );
        }
    }
    if let Some(gaps) = value.get("gaps").and_then(|v| v.as_array()) {
        if !gaps.is_empty() {
            println!("\nNext Gaps");
            for gap in gaps {
                println!(
                    "- {} ({}) -> {}",
                    gap.get("key").and_then(|v| v.as_str()).unwrap_or("unknown"),
                    gap.get("status")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown"),
                    gap.get("next_action")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                );
            }
        }
    }
}

fn print_inventory_state(value: &serde_json::Value) {
    println!("State");
    if let Some(schema) = value.get("schema_doc") {
        println!(
            "schema: {} ({})",
            schema.get("path").and_then(|v| v.as_str()).unwrap_or("-"),
            if schema
                .get("exists")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            {
                "present"
            } else {
                "missing"
            }
        );
    }
    if let Some(summary) = value.get("summary") {
        println!(
            "summary: implemented={} partial={} missing={} persisted={} total={}",
            summary
                .get("implemented")
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            summary.get("partial").and_then(|v| v.as_u64()).unwrap_or(0),
            summary.get("missing").and_then(|v| v.as_u64()).unwrap_or(0),
            summary
                .get("persisted_layers")
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            summary.get("total").and_then(|v| v.as_u64()).unwrap_or(0),
        );
    }
    if let Some(health) = value.get("health") {
        println!(
            "health: prefs={} runtime_eval_spec={} tui_replay_spec={} session_example={}",
            health
                .get("prefs_file_present")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            health
                .get("runtime_eval_spec_present")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            health
                .get("tui_replay_spec_present")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            health
                .get("session_example_present")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
        );
    }
    if let Some(layers) = value.get("layers").and_then(|v| v.as_array()) {
        for layer in layers {
            println!(
                "- {:<22} {:<11} {} -> {}",
                layer
                    .get("key")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown"),
                layer
                    .get("status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown"),
                layer.get("owner").and_then(|v| v.as_str()).unwrap_or("-"),
                layer
                    .get("backing_store")
                    .and_then(|v| v.as_str())
                    .unwrap_or("-"),
            );
        }
    }
    if let Some(gaps) = value.get("gaps").and_then(|v| v.as_array()) {
        if !gaps.is_empty() {
            println!("\nState Gaps");
            for gap in gaps {
                println!(
                    "- {} ({}) -> {}",
                    gap.get("key").and_then(|v| v.as_str()).unwrap_or("unknown"),
                    gap.get("status")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown"),
                    gap.get("next_action")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                );
            }
        }
    }
}

async fn run_chat(prompt: String, common: CommonArgs) -> Result<()> {
    let mut user_input = prompt;

    if common.stdin && common.log_file.is_none() {
        let stdin_text = read_stdin_to_string().context("failed to read stdin")?;
        if !stdin_text.trim().is_empty() {
            user_input = format!("{user_input}\n\n[stdin]\n{stdin_text}");
        }
    }

    let diff_text = match &common.diff_file {
        Some(path) => Some(
            std::fs::read_to_string(path)
                .with_context(|| format!("failed to read diff file: {}", path.display()))?,
        ),
        None => None,
    };

    let cfg = common.to_partial_config().resolve()?;
    let client = reqwest::Client::new();
    let provider = providers::build_provider(client, &cfg);
    let bot = ChatBot::new(provider);

    let log_text = match (&cfg.mode, &common.log_file) {
        (modes::Mode::LogAnalysis, Some(p)) => Some(
            std::fs::read_to_string(p)
                .with_context(|| format!("failed to read log file: {}", p.display()))?,
        ),
        (modes::Mode::LogAnalysis, None) if common.stdin => {
            Some(read_stdin_to_string().context("failed to read stdin")?)
        }
        _ => None,
    };

    let resp = bot
        .run(
            &user_input,
            &[],
            &cfg.mode,
            &cfg.persona,
            None,
            "brief",
            cfg.temperature,
            cfg.max_tokens,
            diff_text.as_deref(),
            log_text.as_deref(),
        )
        .await?;

    println!("{}", resp.content);
    Ok(())
}

fn truncate_middle(s: &str, max_chars: usize) -> String {
    let s = s.trim_end();
    let char_count = s.chars().count();
    if char_count <= max_chars {
        return s.to_string();
    }
    let head_len = max_chars / 2;
    let tail_len = max_chars.saturating_sub(head_len);
    let head: String = s.chars().take(head_len).collect();
    let tail: String = s
        .chars()
        .skip(char_count.saturating_sub(tail_len))
        .collect();
    let total_lines = s.lines().count();
    format!(
        "{head}\n...[truncated — {total_lines} lines total, {char_count} chars total]...\n{tail}"
    )
}

fn git_cmd_output(root: &str, args: &[&str]) -> Result<(String, String, i32)> {
    let out = std::process::Command::new("git")
        .args(["-C", root])
        .args(args)
        .output()
        .with_context(|| format!("failed to run git: git -C {root} {}", args.join(" ")))?;

    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    let exit = out.status.code().unwrap_or(-1);
    Ok((stdout, stderr, exit))
}

fn git_change_signal_since_base(root: &str, base: &str) -> Result<bool> {
    let (status_out, status_err, status_exit) =
        git_cmd_output(root, &["status", "--porcelain=v1"])?;
    if status_exit != 0 {
        anyhow::bail!("git status failed (exit {status_exit}).\n{status_err}");
    }

    let (diff_out, diff_err, diff_exit) =
        git_cmd_output(root, &["diff", "--no-color", "--name-only", base])?;
    if diff_exit != 0 {
        anyhow::bail!("git diff --name-only failed (exit {diff_exit}).\n{diff_err}");
    }

    Ok(!(status_out.trim().is_empty() && diff_out.trim().is_empty()))
}

async fn review_git_diff(
    tool_root: &str,
    staged: bool,
    unstaged: bool,
    base: Option<&str>,
    max_diff_chars: usize,
    guidance: &str,
    common: &CommonArgs,
    lang: Option<&str>,
) -> Result<String> {
    // Auto-scan project context (stack/git/tree + .spiral-coder.md/AGENTS.md) for better reviews.
    let (project_context, agents_md) =
        if let Some(ctx) = crate::project::ProjectContext::scan(tool_root).await {
            (Some(ctx.to_context_text()), ctx.agents_md)
        } else {
            (None, None)
        };

    // Git status and diff payload.
    let (status_out, status_err, status_exit) =
        git_cmd_output(tool_root, &["status", "--porcelain=v1"])?;
    if status_exit != 0 {
        anyhow::bail!("git status failed (exit {status_exit}).\n{status_err}");
    }

    let base = base.map(|s| s.trim()).filter(|s| !s.is_empty());
    let max_chars = max_diff_chars.max(2_000).min(200_000);

    // Build diff + stat.
    let (diff_label, stat_args, diff_args): (&str, Vec<&str>, Vec<&str>) = if staged {
        (
            "staged",
            vec!["diff", "--no-color", "--stat", "--staged"],
            vec!["diff", "--no-color", "--staged"],
        )
    } else if unstaged {
        (
            "unstaged",
            vec!["diff", "--no-color", "--stat"],
            vec!["diff", "--no-color"],
        )
    } else if let Some(b) = base {
        (
            "combined",
            vec!["diff", "--no-color", "--stat", b],
            vec!["diff", "--no-color", b],
        )
    } else {
        // Prefer a combined diff vs HEAD when it exists; otherwise fall back to staged+unstaged sections.
        let (_h_out, _h_err, h_exit) =
            git_cmd_output(tool_root, &["rev-parse", "--verify", "HEAD"])?;
        if h_exit == 0 {
            (
                "combined",
                vec!["diff", "--no-color", "--stat", "HEAD"],
                vec!["diff", "--no-color", "HEAD"],
            )
        } else {
            ("mixed", vec![], vec![])
        }
    };

    let mut git_context = String::new();
    git_context.push_str(&format!("[repo]\nroot: {tool_root}\n"));
    if let Some(b) = base {
        git_context.push_str(&format!("base: {b}\n"));
    }
    git_context.push_str("\n[git status --porcelain=v1]\n");
    git_context.push_str(status_out.trim_end());
    if !status_out.trim().is_empty() {
        git_context.push('\n');
    }

    let mut diff_payload = git_context.clone();
    let mut diff_patch = String::new();

    if diff_label == "mixed" {
        // Empty repo / no HEAD: show staged + unstaged separately.
        let (stat_s, err_s, exit_s) =
            git_cmd_output(tool_root, &["diff", "--no-color", "--stat", "--staged"])?;
        if exit_s == 0 && !stat_s.trim().is_empty() {
            diff_payload.push_str("\n[git diff --staged --stat]\n");
            diff_payload.push_str(stat_s.trim_end());
            diff_payload.push('\n');

            git_context.push_str("\n[git diff --staged --stat]\n");
            git_context.push_str(stat_s.trim_end());
            git_context.push('\n');
        } else if exit_s != 0 && !err_s.trim().is_empty() {
            diff_payload.push_str("\n[git diff --staged --stat ERROR]\n");
            diff_payload.push_str(err_s.trim_end());
            diff_payload.push('\n');
        }

        let (diff_s, err_sd, exit_sd) =
            git_cmd_output(tool_root, &["diff", "--no-color", "--staged"])?;
        if exit_sd == 0 && !diff_s.trim().is_empty() {
            diff_payload.push_str("\n[git diff --staged]\n");
            let t = truncate_middle(&diff_s, max_chars);
            diff_payload.push_str(&t);
            diff_payload.push('\n');

            diff_patch.push_str("[git diff --staged]\n");
            diff_patch.push_str(&t);
            diff_patch.push('\n');
        } else if exit_sd != 0 && !err_sd.trim().is_empty() {
            diff_payload.push_str("\n[git diff --staged ERROR]\n");
            diff_payload.push_str(err_sd.trim_end());
            diff_payload.push('\n');
        }

        let (stat_u, err_u, exit_u) = git_cmd_output(tool_root, &["diff", "--no-color", "--stat"])?;
        if exit_u == 0 && !stat_u.trim().is_empty() {
            diff_payload.push_str("\n[git diff --stat]\n");
            diff_payload.push_str(stat_u.trim_end());
            diff_payload.push('\n');

            git_context.push_str("\n[git diff --stat]\n");
            git_context.push_str(stat_u.trim_end());
            git_context.push('\n');
        } else if exit_u != 0 && !err_u.trim().is_empty() {
            diff_payload.push_str("\n[git diff --stat ERROR]\n");
            diff_payload.push_str(err_u.trim_end());
            diff_payload.push('\n');
        }

        let (diff_u, err_ud, exit_ud) = git_cmd_output(tool_root, &["diff", "--no-color"])?;
        if exit_ud == 0 && !diff_u.trim().is_empty() {
            diff_payload.push_str("\n[git diff]\n");
            let t = truncate_middle(&diff_u, max_chars);
            diff_payload.push_str(&t);
            diff_payload.push('\n');

            diff_patch.push_str("[git diff]\n");
            diff_patch.push_str(&t);
            diff_patch.push('\n');
        } else if exit_ud != 0 && !err_ud.trim().is_empty() {
            diff_payload.push_str("\n[git diff ERROR]\n");
            diff_payload.push_str(err_ud.trim_end());
            diff_payload.push('\n');
        }
    } else {
        let (stat, stat_err, stat_exit) = git_cmd_output(tool_root, &stat_args)?;
        if stat_exit == 0 && !stat.trim().is_empty() {
            diff_payload.push_str(&format!("\n[git diff --stat] ({diff_label})\n"));
            diff_payload.push_str(stat.trim_end());
            diff_payload.push('\n');

            git_context.push_str(&format!("\n[git diff --stat] ({diff_label})\n"));
            git_context.push_str(stat.trim_end());
            git_context.push('\n');
        } else if stat_exit != 0 && !stat_err.trim().is_empty() {
            diff_payload.push_str("\n[git diff --stat ERROR]\n");
            diff_payload.push_str(stat_err.trim_end());
            diff_payload.push('\n');
        }

        let (diff, diff_err, diff_exit) = git_cmd_output(tool_root, &diff_args)?;
        if diff_exit == 0 && !diff.trim().is_empty() {
            diff_payload.push_str(&format!("\n[git diff] ({diff_label})\n"));
            let t = truncate_middle(&diff, max_chars);
            diff_payload.push_str(&t);
            diff_payload.push('\n');

            diff_patch.push_str(&t);
            diff_patch.push('\n');
        } else if diff_exit != 0 {
            anyhow::bail!("git diff failed (exit {diff_exit}).\n{diff_err}");
        }
    }

    // Resolve config. Default review mode to Observer unless the user explicitly set one.
    let mut partial = common.to_partial_config();
    if partial.mode.is_none() {
        partial.mode = Some(crate::modes::Mode::Observer);
    }
    let cfg = partial.resolve()?;

    let client = reqwest::Client::new();
    let provider = providers::build_provider(client, &cfg);
    let bot = ChatBot::new(provider);

    // For diff批評 mode, pass diff via diff_text so compose_user_text wraps it in ```diff```.
    // For Observer mode, include the diff in the user input (diff_text injection is ignored there).
    let user_input = if matches!(cfg.mode, crate::modes::Mode::DiffReview) {
        let mut s = String::new();
        if let Some(ctx) = project_context.as_deref() {
            if !ctx.trim().is_empty() {
                s.push_str(ctx.trim_end());
                s.push_str("\n\n");
            }
        }
        if let Some(a) = agents_md.as_deref() {
            if !a.trim().is_empty() {
                s.push_str("[Project Instructions]\n");
                s.push_str(a.trim_end());
                s.push_str("\n\n");
            }
        }
        if !git_context.trim().is_empty() {
            s.push_str("[git context]\n");
            s.push_str(git_context.trim_end());
            s.push_str("\n\n");
        }
        s.push_str(guidance);
        s
    } else {
        let mut s = String::new();
        if let Some(ctx) = project_context.as_deref() {
            if !ctx.trim().is_empty() {
                s.push_str(ctx.trim_end());
                s.push_str("\n\n");
            }
        }
        if let Some(a) = agents_md.as_deref() {
            if !a.trim().is_empty() {
                s.push_str("[Project Instructions]\n");
                s.push_str(a.trim_end());
                s.push_str("\n\n");
            }
        }
        s.push_str(guidance);
        s.push_str("\n\n[git diff payload]\n");
        s.push_str(diff_payload.trim_end());
        s
    };

    let cot = if matches!(cfg.mode, crate::modes::Mode::Observer) {
        "off"
    } else {
        "brief"
    };

    let resp = bot
        .run(
            &user_input,
            &[],
            &cfg.mode,
            &cfg.persona,
            lang,
            cot,
            cfg.temperature,
            cfg.max_tokens,
            if matches!(cfg.mode, crate::modes::Mode::DiffReview) {
                Some(diff_patch.as_str())
            } else {
                None
            },
            None,
        )
        .await?;

    Ok(resp.content)
}

async fn run_review(args: ReviewArgs, common: CommonArgs) -> Result<()> {
    let tool_root = args.tool_root.clone().or_else(|| {
        std::env::current_dir()
            .ok()
            .map(|p| p.to_string_lossy().into_owned())
    });
    let tool_root = tool_root
        .as_deref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| ".".to_string());

    if args.base.is_some() && (args.staged || args.unstaged) {
        anyhow::bail!("--base cannot be combined with --staged/--unstaged");
    }
    if args.staged && args.unstaged {
        anyhow::bail!(
            "--staged and --unstaged are mutually exclusive (omit both for combined review)"
        );
    }

    // Read optional prompt guidance from stdin.
    let mut guidance = args.prompt.unwrap_or_default();
    if common.stdin {
        let stdin_text = read_stdin_to_string().context("failed to read stdin")?;
        if !stdin_text.trim().is_empty() {
            if guidance.trim().is_empty() {
                guidance = stdin_text;
            } else {
                guidance = format!("{guidance}\n\n[stdin]\n{stdin_text}");
            }
        }
    }
    if guidance.trim().is_empty() {
        guidance = "Review this git diff and critique it ruthlessly and concretely.".to_string();
    }

    let max_chars = args
        .max_diff_chars
        .unwrap_or(24_000)
        .max(2_000)
        .min(200_000);
    let out = review_git_diff(
        &tool_root,
        args.staged,
        args.unstaged,
        args.base.as_deref(),
        max_chars,
        &guidance,
        &common,
        None,
    )
    .await?;

    println!("{out}");
    Ok(())
}

fn parse_at_refs(text: &str) -> Vec<String> {
    let mut refs: Vec<String> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for word in text.split_whitespace() {
        if !word.starts_with('@') {
            continue;
        }
        let path = word.trim_start_matches('@');
        let path = path.trim_end_matches(|c: char| matches!(c, ',' | ')' | ']' | ';' | ':' | '.'));
        if path.is_empty() {
            continue;
        }
        if seen.insert(path.to_string()) {
            refs.push(path.to_string());
        }
    }
    refs
}

fn normalize_path_for_compare(s: &str) -> String {
    let mut out = s.trim().replace('\\', "/");
    while out.ends_with('/') && out.len() > 1 {
        out.pop();
    }
    if cfg!(target_os = "windows") {
        out = out.to_ascii_lowercase();
    }
    out
}

fn normalize_tool_root(tool_root: Option<String>) -> Option<String> {
    let raw = tool_root?;
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }

    let pb = std::path::PathBuf::from(raw);
    let abs = if pb.is_absolute() {
        pb
    } else {
        std::env::current_dir().ok()?.join(pb)
    };

    // Ensure the directory exists so canonicalize can remove `..` components.
    let _ = std::fs::create_dir_all(&abs);
    let abs = std::fs::canonicalize(&abs).unwrap_or(abs);

    Some(abs.to_string_lossy().into_owned())
}

fn resolve_session_path(session_path: PathBuf, tool_root: Option<&str>) -> PathBuf {
    if session_path.is_absolute() {
        return session_path;
    }
    let Some(root) = tool_root.map(|s| s.trim()).filter(|s| !s.is_empty()) else {
        return session_path;
    };
    std::path::PathBuf::from(root).join(session_path)
}

fn resolve_promote_path(path: PathBuf, cwd: &Path, tool_root: Option<&str>) -> PathBuf {
    if path.is_absolute() {
        return path;
    }

    let cwd_path = cwd.join(&path);
    if cwd_path.exists()
        || cwd_path
            .parent()
            .map(|parent| parent.exists())
            .unwrap_or(false)
    {
        return cwd_path;
    }

    resolve_session_path(path, tool_root)
}

async fn run_agent(args: AgentArgs, common: CommonArgs) -> Result<()> {
    run_agent_with_behavior(args, common, AgentRunBehavior::default()).await
}

#[derive(Clone, Copy)]
struct AgentRunBehavior {
    stream_deltas: bool,
    print_git_diff_summary: bool,
}

impl Default for AgentRunBehavior {
    fn default() -> Self {
        Self {
            stream_deltas: true,
            print_git_diff_summary: true,
        }
    }
}

async fn run_agent_with_behavior(
    args: AgentArgs,
    common: CommonArgs,
    behavior: AgentRunBehavior,
) -> Result<()> {
    let AgentArgs {
        prompt,
        tool_root: tool_root_arg,
        lang,
        max_iters,
        yes,
        no_approval,
        no_command_approval,
        no_edit_approval,
        session: session_path,
        new_session,
        autofix,
        trace_out,
        json_out,
        graph_out,
    } = args;

    let session_path = session_path.map(|sp| resolve_session_path(sp, tool_root_arg.as_deref()));

    let mut loaded_session: Option<crate::agent_session::AgentSession> = None;
    let mut start_messages_json: Option<Vec<serde_json::Value>> = None;
    let mut start_checkpoint: Option<String> = None;
    let mut start_cwd: Option<String> = None;
    let mut start_observation_cache: Option<crate::agent_session::ObservationCache> = None;
    let mut start_session_bridge: Option<crate::agent_session::SessionBridge> = None;
    let mut create_checkpoint = true;
    let mut resuming = false;

    if let Some(ref sp) = session_path {
        if sp.exists() && !new_session {
            let mut sess = crate::agent_session::AgentSession::load(sp)?;
            if let Some(warn) = sess.repair_for_resume() {
                eprintln!("[session] WARN: {warn}");
            }
            let msg_count = sess.messages.len();
            let ckpt_short = sess
                .checkpoint
                .as_deref()
                .map(|h| &h[..h.len().min(8)])
                .unwrap_or("-");
            eprintln!(
                "[session] resuming: {} (messages={msg_count}, checkpoint={ckpt_short})",
                sp.display()
            );
            start_checkpoint = sess.checkpoint.clone();
            start_cwd = sess.cur_cwd.clone();
            start_observation_cache = sess.observation_cache.clone();
            start_session_bridge = sess.session_bridge.clone();
            create_checkpoint = sess.checkpoint.is_none();
            start_messages_json = Some(std::mem::take(&mut sess.messages));
            loaded_session = Some(sess);
            resuming = true;
        } else if new_session && sp.exists() {
            eprintln!(
                "[session] --new-session: starting fresh and overwriting {}",
                sp.display()
            );
        } else if new_session {
            eprintln!(
                "[session] --new-session: starting fresh at {}",
                sp.display()
            );
        }
    }

    let saved_tool_root = loaded_session.as_ref().and_then(|s| s.tool_root.clone());
    let saved_tool_root_norm = normalize_tool_root(saved_tool_root.clone());

    let tool_root = normalize_tool_root(
        tool_root_arg
            .clone()
            .or_else(|| saved_tool_root.clone())
            .or_else(|| {
                std::env::current_dir()
                    .ok()
                    .map(|p| p.to_string_lossy().into_owned())
            }),
    );

    if resuming && tool_root_arg.is_some() {
        if let (Some(ref chosen), Some(ref saved)) =
            (tool_root.as_ref(), saved_tool_root_norm.as_ref())
        {
            if normalize_path_for_compare(chosen) != normalize_path_for_compare(saved) {
                eprintln!(
                    "WARN: --root/--tool-root ({}) differs from saved session tool_root ({}). Continuing with provided tool root.",
                    chosen,
                    saved
                );
            }
        }
    }

    let trace_out_path = trace_out.map(|p| resolve_session_path(p, tool_root.as_deref()));
    let json_out_path = json_out.map(|p| resolve_session_path(p, tool_root.as_deref()));
    let graph_out_path = graph_out.map(|p| resolve_session_path(p, tool_root.as_deref()));

    // Build the user prompt text.
    fn default_continue_prompt(lang: &str) -> String {
        let l = lang.trim().to_ascii_lowercase();
        if l == "fr" {
            "Continue depuis l’état précédent. Reprends la tâche et vérifie avec des commandes/tests.".to_string()
        } else if l == "en" {
            "Continue from the previous state. Resume the task and verify with commands/tests."
                .to_string()
        } else {
            "前回の状態から続けて。作業を再開して、コマンド/テストで検証して。".to_string()
        }
    }
    let pending_user_turn = resuming
        && start_messages_json
            .as_ref()
            .and_then(|m| m.last())
            .and_then(|v| v.get("role").and_then(|r| r.as_str()))
            == Some("user");
    let mut used_stdin_as_prompt = false;
    let mut append_user_message = true;
    let mut user_input = match (prompt, common.stdin) {
        (Some(p), _) => p,
        (None, true) => {
            used_stdin_as_prompt = true;
            read_stdin_to_string().context("failed to read stdin")?
        }
        (None, false) if pending_user_turn => {
            // Resume by answering the pending user message in the session. Do not append
            // a synthetic "continue" prompt that could override the unfinished turn.
            append_user_message = false;
            String::new()
        }
        (None, false) if resuming => default_continue_prompt(&lang),
        (None, false) => {
            anyhow::bail!("missing prompt. Provide a prompt argument or pass --stdin.")
        }
    };

    // If --stdin and a prompt was provided, append stdin as extra context (like `run_chat`).
    if common.stdin && !used_stdin_as_prompt && !user_input.trim().is_empty() {
        let stdin_text = read_stdin_to_string().context("failed to read stdin")?;
        if !stdin_text.trim().is_empty() {
            user_input = format!("{user_input}\n\n[stdin]\n{stdin_text}");
        }
    }

    // Expand @file references into injected system messages.
    let at_refs = parse_at_refs(&user_input);
    let mut at_ref_messages_json: Vec<serde_json::Value> = Vec::new();
    for ref_path in &at_refs {
        let (content, is_err) = crate::file_tools::tool_read_file(ref_path, tool_root.as_deref());
        if is_err {
            eprintln!("[@{ref_path}] not found");
        } else {
            let header = content
                .lines()
                .next()
                .unwrap_or(ref_path.as_str())
                .to_string();
            eprintln!("📎 injected: {header}");
            at_ref_messages_json.push(json!({
                "role": "system",
                "content": format!("[@{ref_path}]\n{content}"),
            }));
        }
    }

    // Resolve config. For the headless coding agent, default to a code-like mode
    // so `--code-model` is selected when users set different models per mode.
    let mut partial = common.to_partial_config();
    if !partial.vibe && partial.mode.is_none() {
        partial.mode = Some(crate::modes::Mode::Vibe);
    }
    let cfg = partial.resolve()?;

    let trace: Option<crate::trace_writer::TraceWriter> = match trace_out_path.clone() {
        Some(p) => {
            eprintln!("[trace] writing: {}", p.display());
            Some(crate::trace_writer::TraceWriter::new(p)?)
        }
        None => None,
    };
    if let Some(ref tw) = trace {
        let _ = tw.event(
            "agent_start",
            json!({
                "resuming": resuming,
                "lang": lang.as_str(),
                "max_iters": max_iters.unwrap_or(crate::tui::agent::DEFAULT_MAX_ITERS).max(1).min(64),
                "tool_root": tool_root.as_deref(),
                "session": session_path.as_ref().map(|p| p.display().to_string()),
                "json_out": json_out_path.as_ref().map(|p| p.display().to_string()),
                "graph_out": graph_out_path.as_ref().map(|p| p.display().to_string()),
                "cfg": {
                    "provider": cfg.provider.key(),
                    "base_url": cfg.base_url.as_str(),
                    "mode": cfg.mode.label(),
                    "persona": cfg.persona.as_str(),
                    "model": cfg.model.as_str(),
                    "chat_model": cfg.chat_model.as_str(),
                    "code_model": cfg.code_model.as_str(),
                    "temperature": cfg.temperature,
                    "max_tokens": cfg.max_tokens,
                    "timeout_seconds": cfg.timeout_seconds,
                }
            }),
        );
    }

    // Build initial message list (OpenAI-compatible JSON).
    let mut messages_json: Vec<serde_json::Value> = if let Some(m) = start_messages_json.take() {
        m
    } else {
        // New session: build system prompt (same as TUI Coder).
        let persona_prompt = personas::resolve_persona(&cfg.persona)
            .map(|p| p.prompt)
            .unwrap_or("");
        let lang_instruction = crate::modes::language_instruction(Some(&lang), &cfg.mode);
        let system = crate::tui::agent::coder_system(persona_prompt, lang_instruction, None);
        vec![json!({"role":"system","content": system})]
    };
    messages_json.extend(at_ref_messages_json);
    if append_user_message {
        messages_json.push(json!({"role":"user","content": user_input}));
    }

    // Scan project context (stack/git/tree + .spiral-coder.md/AGENTS.md + test_cmd).
    let (project_context, agents_md, test_cmd) = if let Some(ref root) = tool_root {
        if let Some(ctx) = project::ProjectContext::scan(root).await {
            (Some(ctx.to_context_text()), ctx.agents_md, ctx.test_cmd)
        } else {
            (None, None, None)
        }
    } else {
        (None, None, None)
    };

    if let Some(ref tw) = trace {
        let _ = tw.event(
            "verification_config",
            json!({ "test_command": test_cmd.as_deref() }),
        );
    }

    let autofix_rounds = match autofix {
        Some(n) => n.max(1).min(8),
        None => 0usize,
    };

    // Stream tokens to stdout.
    let max_iters = max_iters
        .unwrap_or(crate::tui::agent::DEFAULT_MAX_ITERS)
        .max(1)
        .min(64);
    let command_approval = !no_command_approval && !no_approval;
    let edit_approval = !no_edit_approval && !no_approval;
    let approver = std::sync::Arc::new(crate::approvals::CliApprover::new(
        command_approval,
        edit_approval,
        yes,
    ));

    use std::io::Write;
    let mut stdout = std::io::stdout();
    let mut checkpoint: Option<String> = start_checkpoint.clone();
    let mut cur_cwd: Option<String> = start_cwd.clone();
    let mut create_checkpoint_round = create_checkpoint;
    let mut result: Result<()> = Ok(());

    let autosaver = session_path.as_ref().map(|sp| {
        std::sync::Arc::new(crate::agent_session::SessionAutoSaver::new(
            sp.clone(),
            loaded_session.as_ref(),
        ))
    });
    if let Some(ref saver) = autosaver {
        if !resuming {
            if let Some(ref sp) = session_path {
                eprintln!("[session] saving: {} (autosave enabled)", sp.display());
            }
        }
        saver.save_or_error(
            tool_root.as_deref(),
            checkpoint.as_deref(),
            cur_cwd.as_deref(),
            &messages_json,
        )?;
    }

    let total_rounds = 1usize + autofix_rounds;
    let mut interrupted = false;
    for round in 0..total_rounds {
        if let Some(ref tw) = trace {
            let _ = tw.event(
                "round_start",
                json!({
                    "round": round,
                    "total_rounds": total_rounds,
                    "autofix": round > 0,
                }),
            );
        }
        if round > 0 {
            eprintln!(
                "\n\n[autofix] round {round}/{autofix_rounds} — applying Observer proposals\n"
            );
        }

        let mut saw_error = false;
        let (tx, mut rx) = mpsc::channel::<streaming::StreamToken>(128);

        let start_state = crate::tui::agent::AgenticStartState {
            messages: messages_json.clone(),
            checkpoint: checkpoint.clone(),
            cur_cwd: cur_cwd.clone(),
            observation_cache: start_observation_cache.clone(),
            session_bridge: start_session_bridge.clone(),
            create_checkpoint: create_checkpoint_round,
        };
        create_checkpoint_round = false;

        let tool_root_for_task = tool_root.clone();
        let cfg_for_task = cfg.clone();
        let project_context_for_task = project_context.clone();
        let agents_md_for_task = agents_md.clone();
        let test_cmd_for_task = test_cmd.clone();
        let autosaver_for_task = autosaver.clone();
        let approver_for_task = approver.clone();

        let handle = tokio::spawn(async move {
            crate::tui::agent::run_agentic_json(
                start_state,
                &cfg_for_task,
                tool_root_for_task.as_deref(),
                max_iters,
                tx,
                project_context_for_task,
                agents_md_for_task,
                test_cmd_for_task,
                command_approval,
                None,
                autosaver_for_task,
                approver_for_task.as_ref(),
            )
            .await
        });

        let ctrl_c = tokio::signal::ctrl_c();
        tokio::pin!(ctrl_c);
        loop {
            tokio::select! {
                token = rx.recv() => {
                    let Some(token) = token else { break };
                    match token {
                        streaming::StreamToken::Delta(s) => {
                            if behavior.stream_deltas {
                                stdout.write_all(s.as_bytes()).ok();
                                stdout.flush().ok();
                            }
                        }
                        streaming::StreamToken::ToolCall(tc) => {
                            if let Some(ref tw) = trace {
                                let _ = tw.event(
                                    "tool_call",
                                    json!({
                                        "id": tc.id,
                                        "name": tc.name,
                                        "arguments": tc.arguments,
                                    }),
                                );
                            }
                        }
                        streaming::StreamToken::GovernorState(s) => {
                            if let Some(ref tw) = trace {
                                let _ = tw.event("governor_state", json!(s));
                            }
                        }
                        streaming::StreamToken::RealizeState(s) => {
                            if let Some(ref tw) = trace {
                                let _ = tw.event("realize_state", json!(s));
                            }
                        }
                        streaming::StreamToken::Telemetry(ev) => {
                            if let Some(ref tw) = trace {
                                let _ = tw.event(&ev.event, ev.data);
                            }
                        }
                        streaming::StreamToken::Checkpoint(hash) => {
                            checkpoint = Some(hash.clone());
                            if let Some(ref tw) = trace {
                                let _ = tw.event("checkpoint", json!({"hash": hash}));
                            }
                        }
                        streaming::StreamToken::Done => {
                            if let Some(ref tw) = trace {
                                let _ = tw.event("done", json!({}));
                            }
                            break
                        },
                        streaming::StreamToken::Error(e) => {
                            saw_error = true;
                            eprintln!("ERROR: {e}");
                            if let Some(ref tw) = trace {
                                let _ = tw.event("error", json!({"message": e}));
                            }
                        }
                    }
                }
                _ = &mut ctrl_c => {
                    interrupted = true;
                    eprintln!("\n\n[agent] interrupted (Ctrl+C). Session can be resumed.\n");
                    if let Some(ref tw) = trace {
                        let _ = tw.event("interrupted", json!({}));
                    }
                    handle.abort();
                    break;
                }
            }
        }

        if interrupted {
            // abort() requests cancellation; join before returning so an active
            // save cannot race with a resumed process. The task owns the latest
            // transcript, so retain its autosave instead of our pre-round copy.
            let _ = handle.await;
            result = Err(anyhow::anyhow!("interrupted"));
            break;
        }

        let end_state = match handle.await {
            Err(e) => {
                result = Err(anyhow::anyhow!("agent task panicked: {e}"));
                break;
            }
            Ok(Ok(s)) => s,
            Ok(Err(e)) => {
                result = Err(e);
                break;
            }
        };

        messages_json = end_state.messages;
        cur_cwd = end_state.cur_cwd;
        checkpoint = checkpoint.or(end_state.checkpoint);
        start_observation_cache = end_state.observation_cache;
        start_session_bridge = crate::agent_session::session_bridge_from_messages(&messages_json);
        if let Some(ref saver) = autosaver {
            saver.save_or_error(
                tool_root.as_deref(),
                checkpoint.as_deref(),
                cur_cwd.as_deref(),
                &messages_json,
            )?;
        }
        if let Some(ref tw) = trace {
            let last_reflection =
                crate::agent_session::last_reflection_summary_from_messages(&messages_json);
            let _ = tw.event(
                "round_end",
                json!({
                    "round": round,
                    "messages_len": messages_json.len(),
                    "checkpoint": checkpoint.as_deref(),
                    "cur_cwd": cur_cwd.as_deref(),
                    "last_reflection": last_reflection,
                }),
            );
        }

        if saw_error {
            result = Err(anyhow::anyhow!("agent finished with errors"));
            break;
        }

        if round + 1 == total_rounds {
            break;
        }

        // ── Observer diff review → feed back into Coder (auto-fix loop) ───
        let Some(ref root) = tool_root else {
            eprintln!("[autofix] skipped: no tool_root set");
            break;
        };
        let Some(ref base_hash) = checkpoint else {
            eprintln!("[autofix] skipped: no git checkpoint available (not a git repo?)");
            break;
        };

        match git_change_signal_since_base(root, base_hash) {
            Ok(false) => {
                eprintln!("[autofix] no changes since checkpoint; stopping.");
                break;
            }
            Err(e) => {
                eprintln!("[autofix] skipped: {e:#}");
                break;
            }
            Ok(true) => {}
        }

        let short = &base_hash[..base_hash.len().min(8)];
        eprintln!("\n\n[autofix] Observer review (base {short})\n");
        let guidance = "Review changes since the session checkpoint. Output concrete, actionable proposals for the Coder to implement.";

        let review = match review_git_diff(
            root,
            false,
            false,
            Some(base_hash.as_str()),
            24_000,
            guidance,
            &common,
            Some(&lang),
        )
        .await
        {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[autofix] Observer review failed: {e:#}");
                result = Err(e);
                break;
            }
        };

        eprintln!("\n[Observer review]\n{review}\n");

        let next_prompt = format!(
            "[Auto-fix requested]\n\
The Observer has reviewed the code and identified the following issues. Fix ALL of them.\n\
For each proposal you address, verify with commands/tests. When finished, call done.\n\n\
{review}"
        );
        messages_json.push(json!({"role":"user","content": next_prompt}));
        if let Some(ref saver) = autosaver {
            if let Some(warn) = saver.save_best_effort(
                tool_root.as_deref(),
                checkpoint.as_deref(),
                cur_cwd.as_deref(),
                &messages_json,
            ) {
                eprintln!("[autosave] WARN: {warn}");
            }
        }
    }

    let export_session = if json_out_path.is_none() && graph_out_path.is_none() {
        None
    } else if result.is_err() {
        match crate::agent_session::AgentSession::load_for_failed_export(session_path.as_deref()) {
            Ok(session) => {
                eprintln!("[export] incomplete run: exporting the latest saved session snapshot");
                Some(session)
            }
            Err(error) => {
                eprintln!("[export] skipped JSON/graph output: {error:#}");
                None
            }
        }
    } else {
        let mut session = crate::agent_session::AgentSession::new(
            tool_root.clone(),
            checkpoint.clone(),
            cur_cwd.clone(),
            start_observation_cache.clone(),
            messages_json.clone(),
        );
        if let Some(ref loaded) = loaded_session {
            session.created_at_ms = loaded.created_at_ms;
        }
        Some(session)
    };

    // Optional: write a final or explicitly labeled durable session snapshot.
    if let (Some(out_path), Some(session)) = (json_out_path.as_ref(), export_session.as_ref()) {
        match crate::agent_session::AgentSession::save_atomic(out_path, session) {
            Ok(_) => eprintln!("[json_out] wrote: {}", out_path.display()),
            Err(e) => {
                eprintln!("[json_out] ERROR: {e:#}");
                if result.is_ok() {
                    result = Err(e);
                }
            }
        }
    }

    // Optional: write an execution graph derived from the final messages.
    if let (Some(out_path), Some(session)) = (graph_out_path.as_ref(), export_session.as_ref()) {
        let graph = crate::task_graph::TaskGraph::from_session_messages(
            session.tool_root.clone(),
            session.checkpoint.clone(),
            session.cur_cwd.clone(),
            &session.messages,
        );
        match crate::task_graph::save_graph_atomic(out_path, &graph) {
            Ok(_) => eprintln!("[graph_out] wrote: {}", out_path.display()),
            Err(e) => {
                eprintln!("[graph_out] ERROR: {e:#}");
                if result.is_ok() {
                    result = Err(e);
                }
            }
        }
    }

    // CLI nicety: show a compact git diff summary from the auto-created checkpoint.
    if behavior.print_git_diff_summary && result.is_ok() {
        let checkpoint_final = checkpoint.as_deref().map(|s| s.to_string());
        if let (Some(ref root), Some(ref hash)) = (tool_root, checkpoint_final.as_deref()) {
            let stat = std::process::Command::new("git")
                .args(["-C", root, "diff", hash, "--stat"])
                .output();
            let names = std::process::Command::new("git")
                .args(["-C", root, "diff", hash, "--name-status"])
                .output();
            if let (Ok(s), Ok(n)) = (stat, names) {
                if s.status.success() && n.status.success() {
                    let s_txt = String::from_utf8_lossy(&s.stdout).trim().to_string();
                    let n_txt = String::from_utf8_lossy(&n.stdout).trim().to_string();
                    if !s_txt.is_empty() || !n_txt.is_empty() {
                        println!(
                            "\n\n[git diff from checkpoint]\n{}\n\nFiles:\n{}",
                            if s_txt.is_empty() {
                                "(no changes)".to_string()
                            } else {
                                s_txt
                            },
                            if n_txt.is_empty() {
                                "(no files)".to_string()
                            } else {
                                n_txt
                            },
                        );
                    }
                }
            }
        }
    }

    if let Some(ref tw) = trace {
        let ok = result.is_ok();
        let err = result.as_ref().err().map(|e| format!("{e:#}"));
        let _ = tw.event(
            "agent_end",
            json!({
                "ok": ok,
                "error": err,
                "messages_len": messages_json.len(),
                "checkpoint": checkpoint.as_deref(),
                "cur_cwd": cur_cwd.as_deref(),
            }),
        );
    }

    result
}

fn resolve_eval_path(path: PathBuf, base_dir: &std::path::Path) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        base_dir.join(path)
    }
}

fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> Result<()> {
    std::fs::create_dir_all(dst)
        .with_context(|| format!("failed to create eval case dir: {}", dst.display()))?;
    let rd = std::fs::read_dir(src)
        .with_context(|| format!("failed to read eval fixture dir: {}", src.display()))?;
    for entry in rd {
        let entry =
            entry.with_context(|| format!("failed to read entry under {}", src.display()))?;
        let file_type = entry
            .file_type()
            .with_context(|| format!("failed to read file type for {}", entry.path().display()))?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else if file_type.is_file() {
            std::fs::copy(&from, &to).with_context(|| {
                format!(
                    "failed to copy eval fixture file {} -> {}",
                    from.display(),
                    to.display()
                )
            })?;
        }
    }
    Ok(())
}

fn resolve_eval_case_root(
    case_dir: &std::path::Path,
    base_root: &std::path::Path,
    defaults: &crate::runtime_eval::RuntimeEvalDefaults,
    case: &crate::runtime_eval::RuntimeEvalCase,
) -> Result<String> {
    let raw = case
        .tool_root
        .as_deref()
        .or(defaults.tool_root.as_deref())
        .unwrap_or(".");
    let root = {
        let pb = std::path::PathBuf::from(raw);
        if pb.is_absolute() {
            pb
        } else {
            base_root.join(pb)
        }
    };
    let should_copy = case.copy_tool_root.unwrap_or(defaults.copy_tool_root);
    let effective_root = if should_copy {
        let copied_root = case_dir.join("tool_root");
        if root.exists() {
            copy_dir_recursive(&root, &copied_root)?;
        } else {
            std::fs::create_dir_all(&copied_root).with_context(|| {
                format!(
                    "failed to create copied eval tool_root: {}",
                    copied_root.display()
                )
            })?;
        }
        copied_root
    } else {
        root
    };
    Ok(
        normalize_tool_root(Some(effective_root.to_string_lossy().into_owned()))
            .unwrap_or_else(|| effective_root.to_string_lossy().into_owned()),
    )
}

fn resolve_eval_session_seed(case_root: &str, seed: &str) -> std::path::PathBuf {
    let path = std::path::PathBuf::from(seed);
    if path.is_absolute() {
        path
    } else {
        std::path::PathBuf::from(case_root).join(path)
    }
}

fn maybe_promote_harness_overlay(
    case_root: &str,
    case_id: &str,
) -> Result<Option<std::path::PathBuf>> {
    let queue_path = crate::tui::agent::harness_evolution_queue_path_for_root(case_root);
    if !queue_path.exists() {
        return Ok(None);
    }
    let queue = crate::tui::agent::PersistentHarnessEvolutionQueue::load(&queue_path)?;
    let overlay_path = crate::tui::agent::harness_evolution_overlay_path_for_root(case_root);
    let mut overlay = crate::tui::agent::HarnessGovernorContractOverlay::load(&overlay_path)?;
    if !overlay.promote_from_queue(&queue, case_id) {
        return Ok(None);
    }
    overlay.save_atomic(&overlay_path)?;
    Ok(Some(overlay_path))
}

async fn run_eval(args: EvalArgs, common: CommonArgs) -> Result<()> {
    let EvalArgs {
        tool_root,
        spec,
        out_dir,
        report_out,
        filter,
        max_cases,
        continue_on_error,
    } = args;

    let cwd = std::env::current_dir().context("failed to get current directory")?;
    let base_root =
        normalize_tool_root(tool_root).unwrap_or_else(|| cwd.to_string_lossy().into_owned());
    let base_root_path = std::path::PathBuf::from(&base_root);
    let spec_path = resolve_eval_path(spec, &cwd);
    let spec_data = crate::runtime_eval::load_spec(&spec_path)?;

    let out_dir = match out_dir {
        Some(p) => resolve_eval_path(p, &base_root_path),
        None => base_root_path.join(format!(
            ".tmp/runtime_eval_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
        )),
    };
    std::fs::create_dir_all(&out_dir).with_context(|| {
        format!(
            "failed to create runtime eval out dir: {}",
            out_dir.display()
        )
    })?;
    let report_path = report_out
        .map(|p| resolve_eval_path(p, &base_root_path))
        .unwrap_or_else(|| out_dir.join("report.json"));

    let filter_lc = filter.as_deref().map(|s| s.trim().to_ascii_lowercase());
    let mut selected: Vec<crate::runtime_eval::RuntimeEvalCase> = spec_data
        .cases
        .iter()
        .filter(|case| {
            if let Some(f) = filter_lc.as_deref() {
                case.id.to_ascii_lowercase().contains(f)
                    || case
                        .tags
                        .iter()
                        .any(|tag| tag.to_ascii_lowercase().contains(f))
            } else {
                true
            }
        })
        .cloned()
        .collect();
    if let Some(limit) = max_cases.map(|n| n.max(1)) {
        selected.truncate(limit);
    }
    if selected.is_empty() {
        anyhow::bail!("runtime eval selected 0 cases");
    }

    eprintln!(
        "[eval] spec={} cases={} out={}",
        spec_path.display(),
        selected.len(),
        out_dir.display()
    );

    let mut reports: Vec<crate::runtime_eval::RuntimeEvalCaseReport> = Vec::new();
    let mut promoted_overlays: std::collections::BTreeMap<String, std::path::PathBuf> =
        std::collections::BTreeMap::new();
    for (idx, case) in selected.iter().enumerate() {
        let case_dir = out_dir.join(format!(
            "{:03}-{}",
            idx + 1,
            crate::runtime_eval::sanitize_case_id(&case.id)
        ));
        std::fs::create_dir_all(&case_dir).with_context(|| {
            format!("failed to create case artifact dir: {}", case_dir.display())
        })?;
        let trace_path = case_dir.join("trace.jsonl");
        let session_path = case_dir.join("session.json");
        let json_path = case_dir.join("final.json");
        let graph_path = case_dir.join("graph.json");
        let case_root =
            resolve_eval_case_root(&case_dir, &base_root_path, &spec_data.defaults, case)?;
        if let Some(seed) = case.session_seed.as_deref() {
            let seed_path = resolve_eval_session_seed(&case_root, seed);
            std::fs::copy(&seed_path, &session_path).with_context(|| {
                format!(
                    "failed to copy runtime eval session seed {} -> {}",
                    seed_path.display(),
                    session_path.display()
                )
            })?;
        }
        let case_lang = case
            .lang
            .clone()
            .or_else(|| spec_data.defaults.lang.clone())
            .unwrap_or_else(|| "ja".to_string());
        let case_max_iters = case.max_iters.or(spec_data.defaults.max_iters);
        let case_autofix = case.autofix.or(spec_data.defaults.autofix);

        eprintln!("[eval] case {}/{}: {}", idx + 1, selected.len(), case.id);

        let started = std::time::Instant::now();
        let run_result = run_agent_with_behavior(
            AgentArgs {
                prompt: Some(case.prompt.clone()),
                tool_root: Some(case_root.clone()),
                lang: case_lang,
                max_iters: case_max_iters,
                yes: false,
                no_approval: true,
                no_command_approval: false,
                no_edit_approval: false,
                session: Some(session_path.clone()),
                new_session: case.session_seed.is_none(),
                autofix: case_autofix,
                trace_out: Some(trace_path.clone()),
                json_out: Some(json_path.clone()),
                graph_out: Some(graph_path.clone()),
            },
            common.clone(),
            AgentRunBehavior {
                stream_deltas: false,
                print_git_diff_summary: false,
            },
        )
        .await;
        let duration_ms = started.elapsed().as_millis();
        let run_error = run_result.err().map(|e| format!("{e:#}"));
        let precheck_case = crate::runtime_eval::pre_promotion_case(case);

        let precheck_report = crate::runtime_eval::evaluate_case(
            &precheck_case,
            &case_root,
            crate::runtime_eval::RuntimeEvalArtifacts {
                case_dir: case_dir.clone(),
                trace_path: trace_path.clone(),
                session_path: session_path.clone(),
                json_path: json_path.clone(),
                graph_path: graph_path.clone(),
            },
            duration_ms,
            run_error.clone(),
        )?;
        if precheck_report.ok {
            if let Some(path) = maybe_promote_harness_overlay(&case_root, &case.id)? {
                eprintln!(
                    "[eval] promoted harness overlay for {} -> {}",
                    case.id,
                    path.display()
                );
                promoted_overlays.insert(case.id.clone(), path);
            }
        }
        let report = crate::runtime_eval::evaluate_case(
            case,
            &case_root,
            crate::runtime_eval::RuntimeEvalArtifacts {
                case_dir,
                trace_path,
                session_path,
                json_path,
                graph_path,
            },
            duration_ms,
            run_error.clone(),
        )?;

        let passed = report.ok;
        let tools = report.metrics.tool_call_count;
        let msgs = report.metrics.messages_len;
        let suffix = if let Some(err) = run_error.as_deref() {
            format!("error={err}")
        } else {
            format!("tools={tools} messages={msgs}")
        };
        eprintln!(
            "[eval] {} {} ({suffix})",
            if passed { "PASS" } else { "FAIL" },
            report.id
        );
        reports.push(report);

        if !passed && !continue_on_error {
            eprintln!("[eval] stopping on first failure (use --continue-on-error to keep going)");
            break;
        }
    }

    let report = crate::runtime_eval::build_report(spec_path.clone(), out_dir.clone(), reports);
    crate::runtime_eval::save_report(&report_path, &report)?;
    let merge_gate_path = crate::eval_merge_gate::default_path_for_report(&report_path);
    let merge_gate =
        crate::eval_merge_gate::build_merge_gate_report(&report, &report_path, &promoted_overlays);
    crate::eval_merge_gate::save_merge_gate_report(&merge_gate_path, &merge_gate)?;

    eprintln!(
        "[eval] summary: {}/{} passed, report={}",
        report.summary.passed,
        report.summary.total,
        report_path.display()
    );
    eprintln!(
        "[eval] merge gate: ready={} rollback_required={} promoted_overlays={} path={}",
        merge_gate.merge_ready,
        merge_gate.rollback_required,
        merge_gate.promoted_overlay_count,
        merge_gate_path.display()
    );

    if report.summary.failed > 0 {
        anyhow::bail!(
            "runtime eval failed: {}/{} case(s) failed",
            report.summary.failed,
            report.summary.total
        );
    }

    Ok(())
}

async fn run_init(args: InitArgs, _common: CommonArgs) -> Result<()> {
    let root = args.tool_root.clone().or_else(|| {
        std::env::current_dir()
            .ok()
            .map(|p| p.to_string_lossy().into_owned())
    });
    let root = root.unwrap_or_else(|| ".".to_string());
    let root_p = std::path::Path::new(&root);
    let spiral_coder_path = root_p.join(".spiral-coder.md");

    if spiral_coder_path.exists() && !args.force {
        anyhow::bail!(
            ".spiral-coder.md already exists at {} (use --force to overwrite)",
            spiral_coder_path.display()
        );
    }

    // Detect stack and test_cmd synchronously (same logic as TUI /init).
    let stack = crate::project::detect_stack_labels(&root_p).join(", ");
    let test_cmd = crate::project::detect_test_command(&root_p, None)
        .unwrap_or_else(|| "# add your test command here".to_string());
    let stack_line = if stack.is_empty() {
        "# auto-detected: unknown".to_string()
    } else {
        stack.clone()
    };
    let content = format!(
        "# .spiral-coder.md — Project Instructions for Spiral-Coder Coder
#
# This file is automatically injected into the Coder's system prompt.
# Edit it to set project rules, test commands, and coding conventions.

## Stack
{stack_line}

## Test Command
test_cmd: {test_cmd}

## Development Rules
- Always run tests after modifying source files
- Use patch_file or apply_diff for targeted edits (safer than exec+sed)
- Keep git commits small and focused
- Check for compilation errors before marking a task done

## Forbidden Commands
# List commands that should never be run automatically:
# - git push --force
# - rm -rf /
# - DROP TABLE

## Notes
# Add any project-specific context, architecture notes, or constraints here.
"
    );

    std::fs::create_dir_all(root_p)
        .with_context(|| format!("failed to create tool_root: {}", root_p.display()))?;
    std::fs::write(&spiral_coder_path, content.as_bytes())
        .with_context(|| format!("failed to write {}", spiral_coder_path.display()))?;

    println!("✓ wrote {}", spiral_coder_path.display());
    Ok(())
}

fn read_stdin_to_string() -> Result<String> {
    use std::io::Read;
    let mut buf = String::new();
    std::io::stdin().read_to_string(&mut buf)?;
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::{
        resolve_merge_gate_path, resolve_promote_path, CliProviderPreset, CommonArgs, MergeGateArgs,
    };
    use crate::config::ProviderKind;
    use std::path::{Path, PathBuf};

    fn temp_dir() -> PathBuf {
        static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let mut path = std::env::temp_dir();
        path.push(format!(
            "spiral_coder_main_tests_{}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis(),
            SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).expect("create temp dir");
        path
    }

    #[test]
    fn resolve_promote_path_prefers_existing_cwd_path() {
        let cwd = temp_dir();
        let root = cwd.join("tool_root");
        let overlay = cwd.join("tool_root/.spiral-coder/governor_contract.overlay.json");
        std::fs::create_dir_all(overlay.parent().unwrap()).expect("create overlay parent");
        std::fs::write(&overlay, b"{}").expect("write overlay");

        let resolved = resolve_promote_path(
            PathBuf::from("tool_root/.spiral-coder/governor_contract.overlay.json"),
            &cwd,
            Some(root.to_str().expect("root utf8")),
        );
        assert_eq!(resolved, overlay);
    }

    #[test]
    fn resolve_promote_path_falls_back_to_tool_root_for_new_output() {
        let cwd = temp_dir();
        let root = cwd.join("tool_root");
        std::fs::create_dir_all(root.join(".spiral-coder")).expect("create tool_root");

        let resolved = resolve_promote_path(
            PathBuf::from(".spiral-coder/governor_contract.promotion.json"),
            &cwd,
            Some(root.to_str().expect("root utf8")),
        );
        assert_eq!(
            resolved,
            Path::new(&root).join(".spiral-coder/governor_contract.promotion.json")
        );
    }

    #[test]
    fn resolve_merge_gate_path_derives_from_report_path() {
        let cwd = temp_dir();
        let root = cwd.join("tool_root");
        let report = root.join(".tmp/runtime_eval_1/report.json");
        std::fs::create_dir_all(report.parent().unwrap()).expect("create report parent");
        std::fs::write(&report, b"{}").expect("write report");
        let args = MergeGateArgs {
            tool_root: Some(root.to_string_lossy().to_string()),
            gate: None,
            report: Some(PathBuf::from(".tmp/runtime_eval_1/report.json")),
            json: false,
            ci: false,
        };

        let resolved = resolve_merge_gate_path(&args, &cwd, &root).expect("resolve gate");

        assert_eq!(resolved, root.join(".tmp/runtime_eval_1/merge_gate.json"));
    }

    #[test]
    fn resolve_merge_gate_path_rejects_gate_and_report_together() {
        let cwd = temp_dir();
        let root = cwd.join("tool_root");
        let args = MergeGateArgs {
            tool_root: Some(root.to_string_lossy().to_string()),
            gate: Some(PathBuf::from("merge_gate.json")),
            report: Some(PathBuf::from("report.json")),
            json: false,
            ci: false,
        };

        let err = resolve_merge_gate_path(&args, &cwd, &root).expect_err("reject both");

        assert!(err.to_string().contains("either --gate or --report"));
    }

    #[test]
    fn common_args_to_partial_config_applies_gemini_preset() {
        let args = CommonArgs {
            vibe: false,
            provider: Some(CliProviderPreset::Gemini),
            model: None,
            chat_model: None,
            code_model: None,
            api_key: None,
            base_url: None,
            mode: None,
            persona: None,
            temperature: 0.4,
            max_tokens: 1024,
            timeout_seconds: 120,
            diff_file: None,
            log_file: None,
            device: None,
            hf_local_only: false,
            stdin: false,
        };

        let partial = args.to_partial_config();
        assert_eq!(partial.provider, Some(ProviderKind::OpenAiCompatible));
        assert_eq!(
            partial.base_url.as_deref(),
            Some("https://generativelanguage.googleapis.com/v1beta/openai")
        );
    }
}
