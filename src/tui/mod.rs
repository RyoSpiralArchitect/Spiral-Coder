pub mod agent;
pub mod app;
pub mod events;
mod input;
pub mod intent;
mod layout;
pub mod merge_gate;
pub mod prefs;
pub mod promotion_gate;
pub mod suggestion;
pub mod ui;

use anyhow::{Context, Result};
use crossterm::{
    event::{DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::config::{PartialConfig, ProviderKind};
use crate::modes::Mode;

#[derive(clap::Args, Debug, Clone)]
pub struct TuiArgs {
    /// Model to use for the Observer pane (defaults to same as Coder).
    #[arg(long)]
    pub observer_model: Option<String>,

    /// Provider to use for the Observer pane (defaults to same as Coder).
    #[arg(long, value_enum)]
    pub observer_provider: Option<ProviderKind>,

    /// Provider base URL for the Observer pane. When omitted and the provider differs
    /// from the Coder, the provider default is used (not SPIRAL_CODER_BASE_URL).
    #[arg(long)]
    pub observer_base_url: Option<String>,

    /// API key for the Observer pane (prefer env vars over CLI flags).
    #[arg(long)]
    pub observer_api_key: Option<String>,

    /// Provider to use for the Chat pane (defaults to same as Coder).
    #[arg(long, value_enum)]
    pub chat_provider: Option<ProviderKind>,

    /// Provider base URL for the Chat pane. When omitted and the provider differs
    /// from the Coder, the provider default is used (not SPIRAL_CODER_BASE_URL).
    #[arg(long)]
    pub chat_base_url: Option<String>,

    /// API key for the Chat pane (prefer env vars over CLI flags).
    #[arg(long)]
    pub chat_api_key: Option<String>,

    /// UI / response language hint (ja/en/fr).
    #[arg(long, default_value = "en")]
    pub lang: String,

    /// Working directory for exec tool calls.
    #[arg(long, short = 'C', alias = "root")]
    pub tool_root: Option<String>,

    /// Max iterations for the Coder agent loop (default: 12). Increase for long runs.
    #[arg(long)]
    pub max_iters: Option<usize>,

    /// Enable auto-observe (Observer auto-fires after each Coder response).
    #[arg(long)]
    pub auto_observe: bool,
}

#[cfg(target_os = "windows")]
#[link(name = "kernel32")]
extern "system" {
    fn SetConsoleOutputCP(wCodePageID: u32) -> i32;
    fn SetConsoleCP(wCodePageID: u32) -> i32;
}

fn default_tui_tool_root() -> String {
    // Isolate each TUI session to avoid collisions and nested git repo disasters.
    // Keep it relative so it stays under the user's workspace root by default.
    let epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!(".tmp/tui_{epoch}")
}

pub async fn run(args: TuiArgs, partial_cfg: PartialConfig) -> Result<()> {
    // Resolve the Coder config from the shared partial config.
    let mut coder_partial = partial_cfg.clone();
    if coder_partial.mode.is_none() {
        coder_partial.mode = Some(Mode::Vibe);
    }
    let coder_cfg = coder_partial
        .resolve_for_interactive_setup()
        .context("failed to resolve coder config")?;

    // Coder is the agentic tool loop; it requires an OpenAI-compatible Chat Completions API
    // with tool calling. Anthropic and HF (local subprocess) are supported for Chat/Observer only.
    if matches!(
        coder_cfg.provider,
        ProviderKind::Anthropic | ProviderKind::Hf
    ) {
        anyhow::bail!(
            "TUI Coder requires a tool-calling provider: openai-compatible or mistral.\n\
You selected: {}\n\
Fix: pass `--provider openai-compatible` (or `--provider mistral`).\n\
Tip: you can still use Anthropic/HF for other panes via `--observer-provider` / `--chat-provider`.",
            coder_cfg.provider
        );
    }

    // Chat uses Chat mode (no tools) and should default to chat_model (not code_model).
    let chat_cfg = {
        let mut chat_partial = partial_cfg.clone();
        chat_partial.mode = Some(Mode::Chat);
        if let Some(p) = args.chat_provider.clone() {
            chat_partial.provider = Some(p.clone());

            // If the Chat provider differs from the Coder, do not inherit global base_url/model/api_key
            // (they are almost always wrong across provider families). Prefer provider defaults/env.
            if p != coder_cfg.provider {
                chat_partial.base_url = args.chat_base_url.clone().or_else(|| Some(String::new()));
                chat_partial.api_key = args.chat_api_key.clone();

                // If the user did not explicitly set --chat-model, default to the provider model.
                if chat_partial.chat_model.is_none() {
                    chat_partial.model = Some(String::new());
                    chat_partial.chat_model = Some(String::new());
                }
            } else {
                if args.chat_base_url.is_some() {
                    chat_partial.base_url = args.chat_base_url.clone();
                }
                if args.chat_api_key.is_some() {
                    chat_partial.api_key = args.chat_api_key.clone();
                }
            }
        } else {
            if args.chat_base_url.is_some() {
                chat_partial.base_url = args.chat_base_url.clone();
            }
            if args.chat_api_key.is_some() {
                chat_partial.api_key = args.chat_api_key.clone();
            }
        }
        chat_partial
            .resolve_for_interactive_setup()
            .context("failed to resolve chat config")?
    };

    // For the Observer we allow a different model via --observer-model.
    let observer_cfg = {
        let mut obs_partial = partial_cfg.clone();
        obs_partial.mode = Some(crate::modes::Mode::Observer);

        if let Some(p) = args.observer_provider.clone() {
            obs_partial.provider = Some(p.clone());

            if p != coder_cfg.provider {
                obs_partial.base_url = args
                    .observer_base_url
                    .clone()
                    .or_else(|| Some(String::new()));
                obs_partial.api_key = args.observer_api_key.clone();

                // Default to provider model unless explicitly overridden by --observer-model.
                if args.observer_model.is_none() {
                    obs_partial.model = Some(String::new());
                    obs_partial.chat_model = Some(String::new());
                    obs_partial.code_model = Some(String::new());
                }
            } else {
                if args.observer_base_url.is_some() {
                    obs_partial.base_url = args.observer_base_url.clone();
                }
                if args.observer_api_key.is_some() {
                    obs_partial.api_key = args.observer_api_key.clone();
                }
            }
        } else {
            if args.observer_base_url.is_some() {
                obs_partial.base_url = args.observer_base_url.clone();
            }
            if args.observer_api_key.is_some() {
                obs_partial.api_key = args.observer_api_key.clone();
            }
        }

        if let Some(obs_model) = &args.observer_model {
            obs_partial.model = Some(obs_model.clone());
            // Block env overrides and force chat_model/code_model to follow base model.
            obs_partial.chat_model = Some(String::new());
            obs_partial.code_model = Some(String::new());
        }
        // Observer always uses Observer mode.
        obs_partial
            .resolve_for_interactive_setup()
            .context("failed to resolve observer config")?
    };

    let tool_root = args
        .tool_root
        .clone()
        .or_else(|| Some(default_tui_tool_root()));
    if let Some(ref r) = tool_root {
        if !r.trim().is_empty() {
            std::fs::create_dir_all(r).context("failed to create tool_root")?;
        }
    }
    let auto_observe = args.auto_observe;
    let lang = args.lang.clone();
    let max_iters = args.max_iters;
    let prefs_root = args
        .tool_root
        .clone()
        .filter(|s| !s.trim().is_empty())
        .or_else(prefs::default_prefs_root);

    // Windows: force UTF-8 so Japanese/French and box-drawing characters don't mojibake.
    #[cfg(target_os = "windows")]
    unsafe {
        const CP_UTF8: u32 = 65001;
        let _ = SetConsoleOutputCP(CP_UTF8);
        let _ = SetConsoleCP(CP_UTF8);
    }

    // ── Panic hook — restore terminal before printing the panic message ──────
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // Best-effort restore; ignore errors (we're already panicking).
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = crossterm::execute!(
            std::io::stderr(),
            crossterm::terminal::LeaveAlternateScreen,
            crossterm::event::DisableMouseCapture,
            crossterm::event::DisableBracketedPaste,
        );
        let _ = crossterm::execute!(std::io::stderr(), crossterm::cursor::Show);
        default_hook(info);
    }));

    // ── Terminal setup ────────────────────────────────────────────────────────
    enable_raw_mode().context("failed to enable raw mode")?;
    let mut stdout = std::io::stdout();
    execute!(
        stdout,
        EnterAlternateScreen,
        EnableMouseCapture,
        EnableBracketedPaste
    )
    .context("failed to enter alternate screen")?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).context("failed to create terminal")?;

    // ── Run ───────────────────────────────────────────────────────────────────
    let mut app = app::App::new(
        coder_cfg,
        observer_cfg,
        chat_cfg,
        tool_root,
        prefs_root.clone(),
        auto_observe,
        lang,
        max_iters,
    );
    if let Ok(prefs) = prefs::load_prefs(prefs_root.as_deref()) {
        prefs::apply_prefs_to_app(&mut app, &prefs);
    }
    if let Err(err) = promotion_gate::refresh_promotions(&mut app) {
        app.harness_promotions_status = Some(format!("promotions refresh failed: {err:#}"));
    }
    if let Err(err) = merge_gate::refresh_merge_gate(&mut app) {
        app.merge_gate_status = Some(format!("merge gate refresh failed: {err:#}"));
    }
    let result = events::run_event_loop(&mut app, &mut terminal).await;

    // ── Restore terminal ──────────────────────────────────────────────────────
    // Always restore even if run errored.
    let _ = disable_raw_mode();
    let _ = execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture,
        DisableBracketedPaste
    );
    let _ = terminal.show_cursor();

    result
}
