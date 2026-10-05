use crossterm::event::{
    Event, EventStream, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc;

use crate::modes::{language_instruction, mode_prompt};
use crate::personas::resolve_persona;
use crate::providers;
use crate::streaming::StreamToken;
use crate::types::{ChatMessage, ChatRequest};

use super::agent;
use super::app::{App, Focus, Message, RightTab, Role, Task, TaskPhase, TaskTarget};
use super::input::{self, HistoryPane, Scroll};
use super::intent;
use super::merge_gate;
use super::prefs;
use super::promotion_gate;
use super::suggestion;

// ── Clipboard ─────────────────────────────────────────────────────────────────

#[cfg(target_os = "windows")]
fn copy_to_clipboard(text: &str) -> bool {
    use clipboard_win::{formats, set_clipboard};
    set_clipboard(formats::Unicode, text).is_ok()
}

#[cfg(target_os = "macos")]
fn copy_to_clipboard(text: &str) -> bool {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let Ok(mut child) = Command::new("pbcopy").stdin(Stdio::piped()).spawn() else {
        return false;
    };
    let Some(mut stdin) = child.stdin.take() else {
        let _ = child.wait();
        return false;
    };
    if stdin.write_all(text.as_bytes()).is_err() {
        let _ = child.wait();
        return false;
    }
    matches!(child.wait(), Ok(status) if status.success())
}

#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
fn copy_to_clipboard(text: &str) -> bool {
    use std::io::Write;
    use std::process::{Command, Stdio};

    fn try_pipe(cmd: &str, args: &[&str], input: &str) -> bool {
        let Ok(mut child) = Command::new(cmd).args(args).stdin(Stdio::piped()).spawn() else {
            return false;
        };
        let Some(mut stdin) = child.stdin.take() else {
            let _ = child.wait();
            return false;
        };
        if stdin.write_all(input.as_bytes()).is_err() {
            let _ = child.wait();
            return false;
        }
        matches!(child.wait(), Ok(status) if status.success())
    }

    try_pipe("wl-copy", &[], text)
        || try_pipe("xclip", &["-selection", "clipboard"], text)
        || try_pipe("xsel", &["--clipboard", "--input"], text)
}

async fn fake_stream_text(tx: &mpsc::Sender<StreamToken>, text: &str) {
    const CHUNK_CHARS: usize = 32;
    let mut buf = String::new();
    let mut n = 0usize;

    for ch in text.chars() {
        buf.push(ch);
        n += 1;
        if n >= CHUNK_CHARS {
            let out = std::mem::take(&mut buf);
            n = 0;
            let _ = tx.send(StreamToken::Delta(out)).await;
        }
    }

    if !buf.is_empty() {
        let _ = tx.send(StreamToken::Delta(buf)).await;
    }
    let _ = tx.send(StreamToken::Done).await;
}

fn realize_state_summary_line(state: &crate::streaming::RealizeState) -> String {
    let pending = if state.pending { "yes" } else { "no" };
    format!(
        "[realize summary] pending={pending} drift={:.2} latency={:.1} leakage={} missing={}",
        state.mean_drift, state.mean_realize_latency, state.early_leakage, state.missing
    )
}

fn should_report_realize_summary(state: &crate::streaming::RealizeState) -> bool {
    state.pending
        || state.mean_drift > 0.0
        || state.mean_realize_latency > 0.0
        || state.early_leakage > 0
        || state.missing > 0
}

// ── Slash command handler ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PaneId {
    Coder,
    Observer,
    Chat,
}

fn save_current_tui_prefs(app: &App) -> anyhow::Result<std::path::PathBuf> {
    let saved = prefs::snapshot_app_prefs(app);
    prefs::save_prefs(app.prefs_root.as_deref(), &saved)
}

fn load_tui_prefs_into_app(app: &mut App) -> anyhow::Result<Option<agent::RealizePreset>> {
    let saved = prefs::load_prefs(app.prefs_root.as_deref())?;
    let preset = saved.coder_realize();
    prefs::apply_prefs_to_app(app, &saved);
    Ok(preset)
}

fn pane_label(pane: PaneId) -> &'static str {
    match pane {
        PaneId::Coder => "coder",
        PaneId::Observer => "observer",
        PaneId::Chat => "chat",
    }
}

fn api_key_env_hint(cfg: &crate::config::RunConfig) -> &'static str {
    crate::config::provider_preset_for_run(cfg).api_key_env_hint()
}

fn pane_key_status(label: &str, cfg: &crate::config::RunConfig) -> String {
    let needs_key = !matches!(cfg.provider, crate::config::ProviderKind::Hf);
    let key_state = if needs_key {
        if cfg.api_key.is_some() {
            "set"
        } else {
            "missing"
        }
    } else {
        "not-needed"
    };
    let preset = crate::config::provider_preset_for_run(cfg);
    format!(
        "- {label}: provider={}  model={}  key={}  env={}",
        preset.label(),
        cfg.model,
        key_state,
        api_key_env_hint(cfg)
    )
}

fn right_tab_label(tab: RightTab) -> &'static str {
    match tab {
        RightTab::Observer => "observer",
        RightTab::Chat => "chat",
        RightTab::Tasks => "tasks",
        RightTab::Promotions => "promotions",
        RightTab::MergeGate => "merge-gate",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InlinePicker {
    Provider,
    Model,
}

fn focused_pane_id(app: &App) -> PaneId {
    match app.focus {
        Focus::Coder => PaneId::Coder,
        Focus::Right => match app.right_tab {
            RightTab::Observer => PaneId::Observer,
            RightTab::Chat => PaneId::Chat,
            RightTab::Tasks | RightTab::Promotions | RightTab::MergeGate => PaneId::Observer,
        },
    }
}

fn pane_input_text(app: &App, pane: PaneId) -> String {
    match pane {
        PaneId::Coder => app.coder.textarea.lines().join("\n"),
        PaneId::Observer => app.observer.textarea.lines().join("\n"),
        PaneId::Chat => app.chat.textarea.lines().join("\n"),
    }
}

fn pane_cfg<'a>(app: &'a App, pane: PaneId) -> &'a crate::config::RunConfig {
    match pane {
        PaneId::Coder => &app.coder_cfg,
        PaneId::Observer => &app.observer_cfg,
        PaneId::Chat => &app.chat_cfg,
    }
}

fn pane_mut<'a>(app: &'a mut App, pane: PaneId) -> &'a mut super::app::Pane {
    match pane {
        PaneId::Coder => &mut app.coder,
        PaneId::Observer => &mut app.observer,
        PaneId::Chat => &mut app.chat,
    }
}

fn active_inline_picker(app: &App, pane: PaneId) -> Option<InlinePicker> {
    match pane_input_text(app, pane).trim() {
        "/provider" => Some(InlinePicker::Provider),
        "/model" => Some(InlinePicker::Model),
        _ => None,
    }
}

fn inline_picker_items(app: &App, pane: PaneId, picker: InlinePicker) -> Vec<String> {
    match picker {
        InlinePicker::Provider => crate::config::provider_preset_keys(pane == PaneId::Coder)
            .into_iter()
            .map(str::to_string)
            .collect(),
        InlinePicker::Model => crate::config::representative_models_for_run(pane_cfg(app, pane))
            .iter()
            .map(|s| s.to_string())
            .collect(),
    }
}

fn adjust_inline_picker(app: &mut App, pane: PaneId, delta: isize) -> bool {
    let Some(picker) = active_inline_picker(app, pane) else {
        return false;
    };
    let items = inline_picker_items(app, pane, picker);
    if items.is_empty() {
        return false;
    }
    let pane = pane_mut(app, pane);
    let max = items.len().saturating_sub(1);
    let next = if delta.is_negative() {
        pane.picker_index.saturating_sub(delta.unsigned_abs())
    } else {
        (pane.picker_index + delta as usize).min(max)
    };
    pane.picker_index = next;
    true
}

fn set_pane_input_text(app: &mut App, pane: PaneId, text: &str) {
    let pane = pane_mut(app, pane);
    pane.textarea = tui_textarea::TextArea::default();
    if !text.is_empty() {
        pane.textarea.insert_str(text);
    }
    pane.welcome_dismissed = true;
}

fn apply_inline_picker(app: &mut App, pane: PaneId) -> bool {
    let Some(picker) = active_inline_picker(app, pane) else {
        return false;
    };
    let items = inline_picker_items(app, pane, picker);
    if items.is_empty() {
        return false;
    }
    let selected = {
        let pane_ref = pane_mut(app, pane);
        pane_ref.picker_index.min(items.len().saturating_sub(1))
    };
    match picker {
        InlinePicker::Provider => {
            let cmd = format!("/provider {}", items[selected]);
            let handled = handle_slash_command(&cmd, app, pane);
            set_pane_input_text(app, pane, "");
            pane_mut(app, pane).picker_index = 0;
            handled
        }
        InlinePicker::Model => {
            let selected_model = items[selected].clone();
            if selected_model == "other" {
                set_pane_input_text(app, pane, "/model ");
                pane_mut(app, pane).picker_index = 0;
                return true;
            }
            let cmd = format!("/model {selected_model}");
            let handled = handle_slash_command(&cmd, app, pane);
            set_pane_input_text(app, pane, "");
            pane_mut(app, pane).picker_index = 0;
            handled
        }
    }
}

fn validate_pane_ready(app: &App, pane: PaneId) -> Option<String> {
    let cfg = pane_cfg(app, pane);
    if !matches!(cfg.provider, crate::config::ProviderKind::Hf) && cfg.api_key.is_none() {
        return Some(format!(
            "missing API key for {}. Run /keys and set {}.",
            pane_label(pane),
            api_key_env_hint(cfg)
        ));
    }
    if cfg.model.trim().is_empty() {
        return Some(
            "model is missing. Choose one with /model after selecting the provider.".to_string(),
        );
    }
    None
}

/// Returns true if `text` was a slash command (caller should NOT send to AI).
fn handle_slash_command(text: &str, app: &mut App, pane: PaneId) -> bool {
    if !text.starts_with('/') {
        return false;
    }

    let (cmd, arg) = match text.find(' ') {
        Some(i) => (&text[..i], text[i + 1..].trim()),
        None => (text, ""),
    };
    let cmd_lc = cmd.to_ascii_lowercase();

    macro_rules! push {
        ($msg:expr) => {
            match pane {
                PaneId::Coder => app.coder.push_tool($msg),
                PaneId::Observer => app.observer.push_tool($msg),
                PaneId::Chat => app.chat.push_tool($msg),
            }
        };
    }

    match cmd_lc.as_str() {
        "/provider" => {
            use crate::config::{
                parse_provider_preset, provider_preset_for_run, provider_preset_keys,
                PartialConfig, ProviderKind, ProviderPreset, RunConfig,
            };

            fn partial_from_run_config(cfg: &RunConfig) -> PartialConfig {
                PartialConfig {
                    vibe: matches!(cfg.mode, crate::modes::Mode::Vibe),
                    provider: Some(cfg.provider.clone()),
                    model: Some(cfg.model.clone()),
                    chat_model: Some(cfg.chat_model.clone()),
                    code_model: Some(cfg.code_model.clone()),
                    api_key: cfg.api_key.clone(),
                    base_url: Some(cfg.base_url.clone()),
                    mode: Some(cfg.mode.clone()),
                    persona: Some(cfg.persona.clone()),
                    temperature: Some(cfg.temperature),
                    max_tokens: Some(cfg.max_tokens),
                    timeout_seconds: Some(cfg.timeout_seconds),
                    hf_device: Some(cfg.hf_device.clone()),
                    hf_local_only: Some(cfg.hf_local_only),
                }
            }

            fn resolve_with_provider_preset(
                cfg: &RunConfig,
                preset: ProviderPreset,
            ) -> anyhow::Result<RunConfig> {
                let mut partial = partial_from_run_config(cfg);
                partial.provider = Some(preset.provider_kind());
                partial.api_key = None;
                partial.base_url = Some(
                    preset
                        .default_base_url()
                        .map(str::to_string)
                        .or_else(|| {
                            (cfg.provider == ProviderKind::OpenAiCompatible)
                                .then(|| cfg.base_url.clone())
                        })
                        .unwrap_or_default(),
                );
                partial.model = Some(String::new());
                partial.chat_model = Some(String::new());
                partial.code_model = Some(String::new());
                partial.resolve_for_interactive_setup()
            }

            let cur = match pane {
                PaneId::Coder => &app.coder_cfg,
                PaneId::Observer => &app.observer_cfg,
                PaneId::Chat => &app.chat_cfg,
            };

            if arg.is_empty() {
                let preset = provider_preset_for_run(cur);
                push!(format!(
                    "provider: {} ({})  base_url: {}  model: {}  (type `/provider` then use Up/Down+Enter to pick)",
                    preset.label(), preset.key(), cur.base_url, cur.model
                ));
                return true;
            }

            let preset = match parse_provider_preset(arg.trim()) {
                Some(preset) => preset,
                None => {
                    let values = provider_preset_keys(pane == PaneId::Coder).join("|");
                    push!(format!("usage: /provider <{values}>"));
                    return true;
                }
            };

            if pane == PaneId::Coder && !preset.coder_supported() {
                push!(
                    "Coder requires a tool-calling preset: openai, gemini, anthropic-compat, or mistral. (Use Chat/Observer for anthropic/hf.)"
                        .to_string()
                );
                return true;
            }

            let cur_clone = cur.clone();
            match resolve_with_provider_preset(&cur_clone, preset) {
                Ok(new_cfg) => {
                    match pane {
                        PaneId::Coder => app.coder_cfg = new_cfg,
                        PaneId::Observer => app.observer_cfg = new_cfg,
                        PaneId::Chat => app.chat_cfg = new_cfg,
                    }
                    match save_current_tui_prefs(app) {
                        Ok(path) => push!(format!(
                            "{:?} provider <- {} ({})  [saved {}]",
                            pane,
                            preset.label(),
                            preset.key(),
                            path.display()
                        )),
                        Err(e) => push!(format!(
                            "{:?} provider <- {} ({})  [save_warn: {e}]",
                            pane,
                            preset.label(),
                            preset.key()
                        )),
                    }
                }
                Err(e) => push!(format!("error: {e}")),
            }
        }
        "/base_url" | "/baseurl" => {
            use crate::config::{PartialConfig, RunConfig};

            fn partial_from_run_config(cfg: &RunConfig) -> PartialConfig {
                PartialConfig {
                    vibe: matches!(cfg.mode, crate::modes::Mode::Vibe),
                    provider: Some(cfg.provider.clone()),
                    model: Some(cfg.model.clone()),
                    chat_model: Some(cfg.chat_model.clone()),
                    code_model: Some(cfg.code_model.clone()),
                    api_key: cfg.api_key.clone(),
                    base_url: Some(cfg.base_url.clone()),
                    mode: Some(cfg.mode.clone()),
                    persona: Some(cfg.persona.clone()),
                    temperature: Some(cfg.temperature),
                    max_tokens: Some(cfg.max_tokens),
                    timeout_seconds: Some(cfg.timeout_seconds),
                    hf_device: Some(cfg.hf_device.clone()),
                    hf_local_only: Some(cfg.hf_local_only),
                }
            }

            fn resolve_with_base_url(cfg: &RunConfig, base_url: &str) -> anyhow::Result<RunConfig> {
                let mut partial = partial_from_run_config(cfg);
                partial.base_url = Some(base_url.to_string());
                partial.api_key = None;
                let mut resolved = partial.resolve_for_interactive_setup()?;
                if resolved.provider == cfg.provider && resolved.base_url == cfg.base_url {
                    resolved.api_key = cfg.api_key.clone();
                }
                Ok(resolved)
            }

            let cur = match pane {
                PaneId::Coder => &app.coder_cfg,
                PaneId::Observer => &app.observer_cfg,
                PaneId::Chat => &app.chat_cfg,
            };

            if arg.is_empty() {
                push!(format!("base_url: {}", cur.base_url));
                return true;
            }

            let base = arg.trim();
            let base_url = if base.eq_ignore_ascii_case("default") {
                String::new()
            } else {
                let parsed = match reqwest::Url::parse(base) {
                    Ok(u) => u,
                    Err(e) => {
                        push!(format!("error: invalid URL: {e}"));
                        return true;
                    }
                };
                match parsed.scheme() {
                    "http" | "https" => {}
                    other => {
                        push!(format!(
                            "error: unsupported scheme: {other} (expected http/https)"
                        ));
                        return true;
                    }
                }
                if parsed.host_str().is_none() {
                    push!("error: base_url missing host".to_string());
                    return true;
                }
                base.trim_end_matches('/').to_string()
            };

            let cur_clone = cur.clone();
            match resolve_with_base_url(&cur_clone, &base_url) {
                Ok(new_cfg) => {
                    match pane {
                        PaneId::Coder => app.coder_cfg = new_cfg,
                        PaneId::Observer => app.observer_cfg = new_cfg,
                        PaneId::Chat => app.chat_cfg = new_cfg,
                    }
                    let label = if base_url.is_empty() {
                        "(default)".to_string()
                    } else {
                        base_url.clone()
                    };
                    match save_current_tui_prefs(app) {
                        Ok(path) => push!(format!(
                            "{:?} base_url <- {}  [saved {}]",
                            pane,
                            label,
                            path.display()
                        )),
                        Err(e) => push!(format!(
                            "{:?} base_url <- {}  [save_warn: {e}]",
                            pane, label
                        )),
                    }
                }
                Err(e) => push!(format!("error: {e}")),
            }
        }
        "/model" => {
            if arg.is_empty() {
                let m = match pane {
                    PaneId::Coder => app.coder_cfg.model.clone(),
                    PaneId::Observer => app.observer_cfg.model.clone(),
                    PaneId::Chat => app.chat_cfg.model.clone(),
                };
                push!(format!(
                    "model: {m}  (type `/model` then use Up/Down+Enter to pick, or `/model <name>` to enter manually)"
                ));
            } else {
                match pane {
                    PaneId::Coder => {
                        app.coder_cfg.model = arg.to_string();
                    }
                    PaneId::Observer => {
                        app.observer_cfg.model = arg.to_string();
                    }
                    PaneId::Chat => {
                        app.chat_cfg.model = arg.to_string();
                    }
                }
                let label = match pane {
                    PaneId::Coder => "coder",
                    PaneId::Observer => "observer",
                    PaneId::Chat => "chat",
                };
                match save_current_tui_prefs(app) {
                    Ok(path) => push!(format!(
                        "{label} model <- {arg}  [saved {}]",
                        path.display()
                    )),
                    Err(e) => push!(format!("{label} model <- {arg}  [save_warn: {e}]")),
                }
            }
        }
        "/mode" => {
            if arg.is_empty() {
                let m = match pane {
                    PaneId::Coder => app.coder_cfg.mode.label(),
                    PaneId::Observer => app.observer_cfg.mode.label(),
                    PaneId::Chat => app.chat_cfg.mode.label(),
                };
                push!(format!("mode: {m}"));
            } else if let Some(mode) = crate::modes::parse_mode(arg) {
                let label = mode.label().to_string();
                match pane {
                    PaneId::Coder => app.coder_cfg.mode = mode,
                    PaneId::Observer => app.observer_cfg.mode = mode,
                    PaneId::Chat => app.chat_cfg.mode = mode,
                }
                match save_current_tui_prefs(app) {
                    Ok(path) => push!(format!("mode <- {label}  [saved {}]", path.display())),
                    Err(e) => push!(format!("mode <- {label}  [save_warn: {e}]")),
                }
            } else {
                let values = crate::modes::supported_modes().join("|");
                push!(format!("usage: /mode <{values}>"));
            }
        }
        "/persona" => {
            if arg.is_empty() {
                let p = match pane {
                    PaneId::Coder => app.coder_cfg.persona.as_str(),
                    PaneId::Observer => app.observer_cfg.persona.as_str(),
                    PaneId::Chat => app.chat_cfg.persona.as_str(),
                };
                push!(format!("persona: {p}"));
            } else {
                match resolve_persona(arg) {
                    Ok(p) => {
                        let key = p.key.to_string();
                        match pane {
                            PaneId::Coder => app.coder_cfg.persona = key.clone(),
                            PaneId::Observer => app.observer_cfg.persona = key.clone(),
                            PaneId::Chat => app.chat_cfg.persona = key.clone(),
                        }
                        match save_current_tui_prefs(app) {
                            Ok(path) => {
                                push!(format!("persona <- {key}  [saved {}]", path.display()))
                            }
                            Err(e) => push!(format!("persona <- {key}  [save_warn: {e}]")),
                        }
                    }
                    Err(e) => push!(format!("error: {e}")),
                }
            }
        }
        "/temp" | "/temperature" => {
            if arg.is_empty() {
                let cfg = match pane {
                    PaneId::Coder => &app.coder_cfg,
                    PaneId::Observer => &app.observer_cfg,
                    PaneId::Chat => &app.chat_cfg,
                };
                let mut msg = format!("temperature: {:.2}", cfg.temperature);
                if !crate::config::should_send_temperature_for_run(cfg) {
                    msg.push_str("  (ignored by this GPT-5 OpenAI-compatible endpoint)");
                }
                push!(msg);
            } else if let Ok(t0) = arg.parse::<f64>() {
                let t = t0.clamp(0.0, 2.0);
                match pane {
                    PaneId::Coder => app.coder_cfg.temperature = t,
                    PaneId::Observer => app.observer_cfg.temperature = t,
                    PaneId::Chat => app.chat_cfg.temperature = t,
                }
                let cfg = match pane {
                    PaneId::Coder => &app.coder_cfg,
                    PaneId::Observer => &app.observer_cfg,
                    PaneId::Chat => &app.chat_cfg,
                };
                let suffix = if !crate::config::should_send_temperature_for_run(cfg) {
                    "  [note: ignored by this GPT-5 OpenAI-compatible endpoint]"
                } else {
                    ""
                };
                match save_current_tui_prefs(app) {
                    Ok(path) => push!(format!(
                        "temperature <- {t:.2}{suffix}  [saved {}]",
                        path.display()
                    )),
                    Err(e) => push!(format!("temperature <- {t:.2}{suffix}  [save_warn: {e}]")),
                }
            } else {
                push!("usage: /temp 0.0-2.0".to_string());
            }
        }
        "/lang" => {
            if arg.is_empty() {
                push!(format!("lang: {}", app.lang));
            } else {
                let v = arg.trim().to_ascii_lowercase();
                if v == "ja" || v == "en" || v == "fr" {
                    app.lang = v.clone();
                    match save_current_tui_prefs(app) {
                        Ok(path) => push!(format!("lang <- {v}  [saved {}]", path.display())),
                        Err(e) => push!(format!("lang <- {v}  [save_warn: {e}]")),
                    }
                } else {
                    push!("usage: /lang ja|en|fr".to_string());
                }
            }
        }
        "/tab" => {
            if arg.is_empty() {
                push!(format!(
                    "right tab: {}  (usage: /tab <observer|chat|tasks|promotions|merge|next>)",
                    right_tab_label(app.right_tab)
                ));
            } else {
                let next = match arg.trim().to_ascii_lowercase().as_str() {
                    "observer" | "obs" => Some(RightTab::Observer),
                    "chat" => Some(RightTab::Chat),
                    "tasks" | "task" => Some(RightTab::Tasks),
                    "promotions" | "promotion" | "promote" => Some(RightTab::Promotions),
                    "merge-gate" | "merge_gate" | "merge" | "gate" => Some(RightTab::MergeGate),
                    "next" => {
                        app.cycle_right_tab();
                        None
                    }
                    _ => {
                        push!("usage: /tab <observer|chat|tasks|promotions|merge|next>".to_string());
                        return true;
                    }
                };
                if let Some(tab) = next {
                    app.right_tab = tab;
                }
                match save_current_tui_prefs(app) {
                    Ok(path) => push!(format!(
                        "right tab <- {}  [saved {}]",
                        right_tab_label(app.right_tab),
                        path.display()
                    )),
                    Err(e) => push!(format!(
                        "right tab <- {}  [save_warn: {e}]",
                        right_tab_label(app.right_tab)
                    )),
                }
            }
        }
        "/keys" => {
            let mut lines = vec![
                "API key setup".to_string(),
                pane_key_status("coder", &app.coder_cfg),
                pane_key_status("observer", &app.observer_cfg),
                pane_key_status("chat", &app.chat_cfg),
                String::new(),
                "CLI flags:".to_string(),
                "- spiral-coder tui --api-key <key>".to_string(),
                "- spiral-coder tui --observer-api-key <key>".to_string(),
                "- spiral-coder tui --chat-api-key <key>".to_string(),
                String::new(),
                "Environment variables:".to_string(),
                "- OpenAI: OPENAI_API_KEY or SPIRAL_CODER_API_KEY".to_string(),
                "- Google Gemini: GEMINI_API_KEY or GOOGLE_API_KEY".to_string(),
                "- Anthropic-compatible: ANTHROPIC_API_KEY".to_string(),
                "- Mistral: MISTRAL_API_KEY or SPIRAL_CODER_API_KEY".to_string(),
                "- Anthropic: ANTHROPIC_API_KEY".to_string(),
                "- HF local: no key required".to_string(),
                String::new(),
                format!(
                    "Current pane: {}  right tab: {}",
                    pane_label(pane),
                    right_tab_label(app.right_tab)
                ),
                "Tip: use /provider first, then /keys to confirm the right env var.".to_string(),
            ];
            if matches!(
                app.coder_cfg.provider,
                crate::config::ProviderKind::OpenAiCompatible
            ) && app.coder_cfg.api_key.is_none()
            {
                lines.push(
                    "Note: custom OpenAI-style local endpoints may not require a key, but hosted presets do."
                        .to_string(),
                );
            }
            push!(lines.join("\n"));
        }
        "/realize" => {
            if arg.is_empty() {
                let mut msg = format!(
                    "coder realize: {}  prefs: {}  (usage: /realize <off|low|mid|high>)",
                    app.coder_realize_preset.summary(),
                    prefs::root_label(app.prefs_root.as_deref())
                );
                if let Some(ref state) = app.coder_realize_state {
                    msg.push_str(&format!("\n{}", realize_state_summary_line(state)));
                }
                push!(msg);
            } else {
                match arg.parse::<agent::RealizePreset>() {
                    Ok(preset) => {
                        app.coder_realize_preset = preset;
                        if matches!(preset, agent::RealizePreset::Off) {
                            app.coder_realize_state = None;
                        }
                        match save_current_tui_prefs(app) {
                            Ok(path) => push!(format!(
                                "coder realize <- {}  [saved {}]",
                                app.coder_realize_preset.summary(),
                                path.display()
                            )),
                            Err(e) => push!(format!(
                                "coder realize <- {}  [save_warn: {e}]",
                                app.coder_realize_preset.summary()
                            )),
                        }
                    }
                    Err(_) => push!("usage: /realize <off|low|mid|high>".to_string()),
                }
            }
        }
        "/root" | "/wd" => {
            if arg.is_empty() {
                let r = app.tool_root.as_deref().unwrap_or("(default: current dir)");
                push!(format!("tool_root: {r}"));
            } else {
                app.tool_root = Some(arg.to_string());
                app.prefs_root = Some(arg.to_string());
                let _ = std::fs::create_dir_all(arg);
                match load_tui_prefs_into_app(app) {
                    Ok(Some(preset)) => push!(format!(
                        "tool_root <- {arg}  [prefs loaded: realize={}]",
                        preset.label()
                    )),
                    Ok(None) => push!(format!(
                        "tool_root <- {arg}  [prefs root: {}]",
                        prefs::root_label(app.prefs_root.as_deref())
                    )),
                    Err(e) => push!(format!("tool_root <- {arg}  [prefs_warn: {e}]")),
                }
            }
        }
        "/find" => {
            let q = arg.to_string();
            match pane {
                PaneId::Coder => app.coder.find_query = q.clone(),
                PaneId::Observer => app.observer.find_query = q.clone(),
                PaneId::Chat => app.chat.find_query = q.clone(),
            }
            if q.trim().is_empty() {
                push!("find: off".to_string());
            } else {
                push!(format!("find: {q}"));
            }
        }
        "/autofix" => {
            app.auto_fix_mode = !app.auto_fix_mode;
            let status = format!(
                "auto-fix mode: {} — Observer reviews will {} be forwarded to Coder",
                if app.auto_fix_mode { "ON" } else { "OFF" },
                if app.auto_fix_mode {
                    "automatically"
                } else {
                    "NOT"
                }
            );
            match save_current_tui_prefs(app) {
                Ok(path) => push!(format!("{status}  [saved {}]", path.display())),
                Err(e) => push!(format!("{status}  [save_warn: {e}]")),
            }
        }
        "/diff" => {
            let root = app
                .tool_root
                .as_ref()
                .cloned()
                .unwrap_or_else(|| ".".to_string());
            let base = app.last_git_checkpoint.as_deref().unwrap_or("HEAD~1");
            let stat = std::process::Command::new("git")
                .args(["-C", &root, "diff", base, "--stat"])
                .output();
            let diff = std::process::Command::new("git")
                .args(["-C", &root, "diff", base, "--name-status"])
                .output();
            match (stat, diff) {
                (Ok(s), Ok(d)) => {
                    let stat_text = String::from_utf8_lossy(&s.stdout);
                    let diff_text = String::from_utf8_lossy(&d.stdout);
                    if stat_text.trim().is_empty() {
                        push!("no changes since session start".to_string());
                    } else {
                        push!(format!(
                            "[Session diff from checkpoint]\n{}\n\nFiles changed:\n{}",
                            stat_text.trim(),
                            diff_text.trim()
                        ));
                    }
                }
                _ => push!("git diff failed — is tool_root a git repo?".to_string()),
            }
        }
        "/init" => {
            let root = app
                .tool_root
                .as_ref()
                .cloned()
                .unwrap_or_else(|| ".".to_string());
            let spiral_coder_path = std::path::Path::new(&root).join(".spiral-coder.md");
            if spiral_coder_path.exists() {
                push!(format!(
                    ".spiral-coder.md already exists at {}",
                    spiral_coder_path.display()
                ));
            } else {
                // Detect stack and test_cmd synchronously.
                let root_path = std::path::Path::new(&root);
                let stack = crate::project::detect_stack_labels(root_path).join(", ");
                let test_cmd = crate::project::detect_test_command(root_path, None)
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
                match std::fs::write(&spiral_coder_path, &content) {
                    Ok(_) => push!(format!(
                        "✓ created .spiral-coder.md at {} — edit it to customize",
                        spiral_coder_path.display()
                    )),
                    Err(e) => push!(format!("✗ failed to create .spiral-coder.md: {e}")),
                }
            }
        }
        "/rollback" => {
            match (&app.last_git_checkpoint, &app.tool_root) {
                (Some(hash), Some(root)) => {
                    let hash = hash.clone();
                    let root = root.clone();
                    let short = &hash[..hash.len().min(8)];
                    // Run git reset --hard synchronously (blocking is fine here — TUI is paused on input).
                    match std::process::Command::new("git")
                        .args(["-C", &root, "reset", "--hard", &hash])
                        .output()
                    {
                        Ok(out) if out.status.success() => {
                            push!(format!("✓ rolled back to checkpoint {short}"));
                            app.last_git_checkpoint = None;
                        }
                        Ok(out) => {
                            let stderr = String::from_utf8_lossy(&out.stderr);
                            push!(format!("✗ rollback failed: {}", stderr.trim()));
                        }
                        Err(e) => push!(format!("✗ rollback error: {e}")),
                    }
                }
                (None, _) => push!("no checkpoint available (run Coder first)".to_string()),
                (_, None) => push!("no tool_root set — use /root <path> first".to_string()),
            }
        }
        "/help" | "/?" => {
            push!(
                "pane-scoped & persisted: /provider /base_url /mode /model /persona /temp\n\
/model <name>       set model\n\
/provider <name>    set provider (or show current; exact `/provider` opens picker)\n\
/base_url <url>     set base_url (or show; use `default` to reset)\n\
/persona <name>     set persona\n\
/mode <name>        set mode\n\
/temp <0.0-2.0>     set temperature\n\
/lang <ja|en|fr>    set UI + prompt language\n\
/tab <name>         switch right pane (observer|chat|tasks|promotions|merge|next)\n\
/keys               show API key status and setup help\n\
/model              exact `/model` opens vendor-aware model picker\n\
/realize <mode>     set coder latent-plan mode (off|low|mid|high)\n\
/root <path>        set tool_root\n\
/find <text>        filter history\n\
/meta-diagnose [...] send coder failure to Observer\n\
/autofix            toggle Observer->Coder auto-fix pipeline\n\
/diff               show session diff from git checkpoint\n\
/init               generate .spiral-coder.md template\n\
/rollback           restore git checkpoint from session start\n\
Ctrl+R              cycle right pane tab\n"
                    .to_string()
            );
        }
        _ => push!(format!("unknown command: {cmd} (try /help)")),
    }

    true
}

pub enum AppEvent {
    Key(KeyEvent),
    Mouse(MouseEvent),
    Paste(String),
    CoderToken(StreamToken),
    ObserverToken(StreamToken),
    ChatToken(StreamToken),
    TasksPlanned(Vec<Task>),
    TaskPlanError(String),
    Tick,
}

fn terminal_event(event: Event) -> Option<AppEvent> {
    match event {
        Event::Key(key) => Some(AppEvent::Key(key)),
        Event::Mouse(mouse) => Some(AppEvent::Mouse(mouse)),
        Event::Paste(text) => Some(AppEvent::Paste(text)),
        _ => None,
    }
}

#[derive(Debug, Deserialize)]
struct PlannedTasks {
    tasks: Vec<PlannedTask>,
}

#[derive(Debug, Deserialize)]
struct PlannedTask {
    target: String,
    title: String,
    body: String,
    phase: Option<String>,
    priority: Option<u8>,
}

pub async fn run_event_loop(
    app: &mut App,
    terminal: &mut ratatui::Terminal<impl ratatui::backend::Backend>,
) -> anyhow::Result<()> {
    let (coder_tx, mut coder_rx) = mpsc::channel::<StreamToken>(256);
    let (observer_tx, mut observer_rx) = mpsc::channel::<StreamToken>(256);
    let (chat_tx, mut chat_rx) = mpsc::channel::<StreamToken>(256);
    // Internal app events (background planners, etc.).
    let (internal_tx, mut internal_rx) = mpsc::channel::<AppEvent>(64);
    let mut event_stream = EventStream::new();
    let mut tick = tokio::time::interval(std::time::Duration::from_millis(100));

    loop {
        terminal.draw(|f| super::ui::render(f, app))?;

        let ev = tokio::select! {
            maybe_key = event_stream.next() => {
                match maybe_key {
                    Some(Ok(event)) => match terminal_event(event) {
                        Some(event) => event,
                        None => continue,
                    },
                    Some(Err(e)) => return Err(e.into()),
                    _ => continue,
                }
            }
            Some(token) = coder_rx.recv() => AppEvent::CoderToken(token),
            Some(token) = observer_rx.recv() => AppEvent::ObserverToken(token),
            Some(token) = chat_rx.recv() => AppEvent::ChatToken(token),
            Some(ev2) = internal_rx.recv() => ev2,
            _ = tick.tick() => AppEvent::Tick,
        };

        match ev {
            AppEvent::Key(key) => {
                if handle_key(key, app, &coder_tx, &observer_tx, &chat_tx, &internal_tx).await? {
                    break;
                }
            }
            AppEvent::Mouse(m) => handle_mouse(m, app),
            AppEvent::Paste(text) => input::paste(app, &text),
            AppEvent::CoderToken(token) => handle_coder_token(token, app),
            AppEvent::ObserverToken(token) => handle_observer_token(token, app),
            AppEvent::ChatToken(token) => handle_chat_token(token, app),
            AppEvent::TasksPlanned(tasks) => handle_tasks_planned(tasks, app),
            AppEvent::TaskPlanError(e) => handle_task_plan_error(e, app),
            AppEvent::Tick => {
                app.tick_count = app.tick_count.wrapping_add(1);
                maybe_auto_next_action_assist(app, &observer_tx).await;
                maybe_auto_observe(app, &observer_tx).await;
                maybe_observer_lang_retry(app, &observer_tx).await;
                maybe_observer_loop_retry(app, &observer_tx).await;
                // A — consume pending auto-fix (set by handle_observer_token on Done).
                if let Some(fix_text) = app.pending_auto_fix.take() {
                    if !app.coder.streaming {
                        send_coder_with_text(app, &coder_tx, fix_text).await;
                    }
                }
                if app.pending_observer_hint.is_some() && !app.coder.streaming {
                    send_coder_with_text(app, &coder_tx, "Continue.".to_string()).await;
                }
            }
        }

        if app.quit {
            break;
        }
    }

    // Abort any in-flight tasks on clean exit.
    if let Some(t) = app.coder_task.take() {
        t.abort();
    }
    if let Some(t) = app.observer_task.take() {
        t.abort();
    }
    if let Some(t) = app.chat_task.take() {
        t.abort();
    }

    Ok(())
}

// ── Mouse handler ─────────────────────────────────────────────────────────────

fn handle_mouse(mouse: MouseEvent, app: &mut App) {
    // Query terminal dimensions for hit-testing (fall back to 80×24).
    let (term_w, term_h) = crossterm::terminal::size().unwrap_or((80, 24));
    handle_mouse_at(mouse, app, term_w, term_h);
}

fn handle_mouse_at(mouse: MouseEvent, app: &mut App, term_w: u16, term_h: u16) {
    use super::layout::{contains, right_tab_labels, ScreenLayout};
    let screen = ratatui::layout::Rect::new(0, 0, term_w, term_h);
    let layout = ScreenLayout::new(screen, app);
    let over_coder = contains(layout.coder, mouse.column, mouse.row);
    let over_right = contains(layout.right_content, mouse.column, mouse.row)
        || contains(layout.right_tabs, mouse.column, mouse.row);

    match mouse.kind {
        // Scroll wheel: scroll whichever pane the cursor is over.
        MouseEventKind::ScrollUp if over_coder || over_right => {
            if over_coder {
                input::scroll_history(app, HistoryPane::Coder, Scroll::Up(3), screen);
            } else {
                match app.right_tab {
                    RightTab::Observer => {
                        input::scroll_history(app, HistoryPane::Observer, Scroll::Up(3), screen)
                    }
                    RightTab::Chat => {
                        input::scroll_history(app, HistoryPane::Chat, Scroll::Up(3), screen)
                    }
                    RightTab::Tasks => app.tasks_cursor = app.tasks_cursor.saturating_sub(1),
                    RightTab::Promotions => {
                        app.harness_promotions_cursor =
                            app.harness_promotions_cursor.saturating_sub(1)
                    }
                    RightTab::MergeGate => {
                        app.merge_gate_cursor = app.merge_gate_cursor.saturating_sub(1)
                    }
                }
            }
        }
        MouseEventKind::ScrollDown if over_coder || over_right => {
            if over_coder {
                input::scroll_history(app, HistoryPane::Coder, Scroll::Down(3), screen);
            } else {
                match app.right_tab {
                    RightTab::Observer => {
                        input::scroll_history(app, HistoryPane::Observer, Scroll::Down(3), screen)
                    }
                    RightTab::Chat => {
                        input::scroll_history(app, HistoryPane::Chat, Scroll::Down(3), screen)
                    }
                    RightTab::Tasks => {
                        if !app.tasks.is_empty() {
                            app.tasks_cursor =
                                (app.tasks_cursor + 1).min(app.tasks.len().saturating_sub(1));
                        }
                    }
                    RightTab::Promotions => {
                        if !app.harness_promotions.entries.is_empty() {
                            app.harness_promotions_cursor = (app.harness_promotions_cursor + 1)
                                .min(app.harness_promotions.entries.len().saturating_sub(1));
                        }
                    }
                    RightTab::MergeGate => {
                        if !app.merge_gate.entries.is_empty() {
                            app.merge_gate_cursor = (app.merge_gate_cursor + 1)
                                .min(app.merge_gate.entries.len().saturating_sub(1));
                        }
                    }
                }
            }
        }

        // Left-click in the body: focus that pane.
        MouseEventKind::Down(MouseButton::Left) => {
            if over_coder || over_right {
                app.focus = if over_coder {
                    Focus::Coder
                } else {
                    Focus::Right
                };
                if contains(layout.right_tabs, mouse.column, mouse.row) {
                    if let Some(label) = right_tab_labels(layout.right_tabs, app)
                        .into_iter()
                        .find(|label| contains(label.area, mouse.column, mouse.row))
                    {
                        app.right_tab = label.tab;
                        let _ = save_current_tui_prefs(app);
                    }
                } else if over_right {
                    let inner = ratatui::widgets::Block::default()
                        .borders(ratatui::widgets::Borders::ALL)
                        .inner(layout.right_content);
                    if contains(inner, mouse.column, mouse.row) {
                        let offset = (mouse.row - inner.y) as usize;
                        match app.right_tab {
                            RightTab::Promotions => {
                                let _ = promotion_gate::select_visible_row(
                                    app,
                                    inner.height as usize,
                                    offset,
                                );
                            }
                            RightTab::MergeGate => {
                                let _ = merge_gate::select_visible_row(
                                    app,
                                    inner.height as usize,
                                    offset,
                                );
                            }
                            _ => {}
                        }
                    }
                }
                match app.focus {
                    Focus::Coder => app.coder.welcome_dismissed = true,
                    Focus::Right => match app.right_tab {
                        RightTab::Observer => app.observer.welcome_dismissed = true,
                        RightTab::Chat => app.chat.welcome_dismissed = true,
                        RightTab::Tasks | RightTab::Promotions | RightTab::MergeGate => {}
                    },
                }
            } else if contains(layout.input, mouse.column, mouse.row) {
                // There is one full-width composer for the current pane. Clicking
                // its right half must not redirect a draft to another recipient.
                match app.focus {
                    Focus::Coder => app.coder.welcome_dismissed = true,
                    Focus::Right => match app.right_tab {
                        RightTab::Observer => app.observer.welcome_dismissed = true,
                        RightTab::Chat => app.chat.welcome_dismissed = true,
                        RightTab::Tasks | RightTab::Promotions | RightTab::MergeGate => {}
                    },
                }
            }
        }

        _ => {}
    }
}

// ── Key handler ───────────────────────────────────────────────────────────────

/// Returns true if the app should quit.
async fn handle_key(
    key: KeyEvent,
    app: &mut App,
    coder_tx: &mpsc::Sender<StreamToken>,
    observer_tx: &mpsc::Sender<StreamToken>,
    chat_tx: &mpsc::Sender<StreamToken>,
    internal_tx: &mpsc::Sender<AppEvent>,
) -> anyhow::Result<bool> {
    use crossterm::event::KeyEventKind;
    if key.kind == KeyEventKind::Release {
        return Ok(false);
    }

    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);
    let active_pane = focused_pane_id(app);

    match key.code {
        // Quit
        KeyCode::Char('c') if ctrl => return Ok(true),
        KeyCode::Esc => return Ok(true),

        // Switch focus
        KeyCode::Tab => app.toggle_focus(),

        // Cycle right-side tab (Observer/Chat/Tasks/Promotions/Merge)
        KeyCode::Char('r') if ctrl => {
            app.cycle_right_tab();
            let _ = save_current_tui_prefs(app);
        }

        // Yank (copy) last assistant message to clipboard
        KeyCode::Char('y')
            if ctrl && app.focus == Focus::Right && app.right_tab == RightTab::Promotions =>
        {
            if let Some(text) = promotion_gate::selected_entry_clipboard_text(app) {
                if copy_to_clipboard(&text) {
                    app.harness_promotions_status =
                        Some("promotion summary copied to clipboard".to_string());
                } else {
                    app.harness_promotions_status = Some(
                        "clipboard copy failed (macOS: pbcopy / Linux: wl-copy|xclip|xsel)"
                            .to_string(),
                    );
                }
            }
        }
        KeyCode::Char('y')
            if ctrl && app.focus == Focus::Right && app.right_tab == RightTab::MergeGate =>
        {
            if let Some(text) = merge_gate::selected_entry_clipboard_text(app) {
                if copy_to_clipboard(&text) {
                    app.merge_gate_status =
                        Some("merge gate summary copied to clipboard".to_string());
                } else {
                    app.merge_gate_status = Some(
                        "clipboard copy failed (macOS: pbcopy / Linux: wl-copy|xclip|xsel)"
                            .to_string(),
                    );
                }
            }
        }
        KeyCode::Char('y') if ctrl => {
            let content = {
                let pane = match app.focus {
                    Focus::Coder => &app.coder,
                    Focus::Right => match app.right_tab {
                        RightTab::Observer => &app.observer,
                        RightTab::Chat => &app.chat,
                        RightTab::Tasks | RightTab::Promotions | RightTab::MergeGate => {
                            &app.observer
                        }
                    },
                };
                pane.messages
                    .iter()
                    .rev()
                    .find(|m| matches!(m.role, Role::Assistant) && m.complete)
                    .map(|m| m.content.clone())
            };
            if let Some(text) = content {
                if copy_to_clipboard(&text) {
                    app.focused_pane_mut()
                        .push_tool("copied to clipboard".to_string());
                } else {
                    app.focused_pane_mut().push_tool(
                        "clipboard copy failed (macOS: pbcopy / Linux: wl-copy|xclip|xsel)"
                            .to_string(),
                    );
                }
            }
        }

        // Toggle auto-observe
        KeyCode::Char('a') if ctrl => {
            app.auto_observe = !app.auto_observe;
            let _ = save_current_tui_prefs(app);
        }

        // Trigger Observer manually
        KeyCode::Char('o') if ctrl => {
            app.right_tab = RightTab::Observer;
            app.focus = Focus::Right;
            if let Some(prompt) = input::manual_review_prompt(app) {
                send_observer_message(app, observer_tx, Some(prompt)).await;
            } else {
                app.observer
                    .push_tool("No completed Coder output to review yet.".to_string());
            }
        }

        // Clear current pane
        KeyCode::Char('l')
            if ctrl && app.focus == Focus::Right && app.right_tab == RightTab::Promotions =>
        {
            app.harness_promotions_status = None;
        }
        KeyCode::Char('l')
            if ctrl && app.focus == Focus::Right && app.right_tab == RightTab::MergeGate =>
        {
            app.merge_gate_status = None;
        }
        KeyCode::Char('l') if ctrl => {
            let pane = app.focused_pane_mut();
            pane.messages.clear();
            pane.scroll = 0;
        }

        // Stop streaming (Ctrl+K)
        KeyCode::Char('k')
            if ctrl && app.focus == Focus::Right && app.right_tab == RightTab::Chat =>
        {
            if let Some(handle) = app.chat_task.take() {
                handle.abort();
            }
            app.ignore_chat_tokens = true;
            app.chat.finish_stream();
            app.chat.push_tool("(stream canceled)".to_string());
        }
        KeyCode::Char('k') if ctrl => match app.focus {
            Focus::Coder => {
                if let Some(handle) = app.coder_task.take() {
                    handle.abort();
                }
                app.ignore_coder_tokens = true;
                app.coder.finish_stream();
                app.coder.push_tool("(stream canceled)".to_string());
            }
            Focus::Right => {
                if let Some(handle) = app.observer_task.take() {
                    handle.abort();
                }
                app.observer_meta_mode = false;
                app.observer_next_action_mode = false;
                app.ignore_observer_tokens = true;
                app.observer.finish_stream();
                app.observer.push_tool("(stream canceled)".to_string());
            }
        },

        // Scroll (lines-from-bottom semantics: 0 = pinned, N = above)
        KeyCode::PageUp => {
            if app.focus == Focus::Right && app.right_tab == RightTab::Tasks {
                app.tasks_cursor = app.tasks_cursor.saturating_sub(5);
            } else if app.focus == Focus::Right && app.right_tab == RightTab::Promotions {
                app.harness_promotions_cursor = app.harness_promotions_cursor.saturating_sub(5);
            } else if app.focus == Focus::Right && app.right_tab == RightTab::MergeGate {
                app.merge_gate_cursor = app.merge_gate_cursor.saturating_sub(5);
            } else {
                input::scroll_focused_history(app, Scroll::Up(5));
            }
        }
        KeyCode::PageDown => {
            if app.focus == Focus::Right && app.right_tab == RightTab::Tasks {
                if !app.tasks.is_empty() {
                    app.tasks_cursor =
                        (app.tasks_cursor + 5).min(app.tasks.len().saturating_sub(1));
                }
            } else if app.focus == Focus::Right && app.right_tab == RightTab::Promotions {
                if !app.harness_promotions.entries.is_empty() {
                    app.harness_promotions_cursor = (app.harness_promotions_cursor + 5)
                        .min(app.harness_promotions.entries.len().saturating_sub(1));
                }
            } else if app.focus == Focus::Right && app.right_tab == RightTab::MergeGate {
                if !app.merge_gate.entries.is_empty() {
                    app.merge_gate_cursor = (app.merge_gate_cursor + 5)
                        .min(app.merge_gate.entries.len().saturating_sub(1));
                }
            } else {
                input::scroll_focused_history(app, Scroll::Down(5));
            }
        }
        KeyCode::Home => {
            if app.focus == Focus::Right && app.right_tab == RightTab::Tasks {
                app.tasks_cursor = 0;
            } else if app.focus == Focus::Right && app.right_tab == RightTab::Promotions {
                app.harness_promotions_cursor = 0;
            } else if app.focus == Focus::Right && app.right_tab == RightTab::MergeGate {
                app.merge_gate_cursor = 0;
            } else {
                input::scroll_focused_history(app, Scroll::Top);
            }
        }
        KeyCode::End => {
            if app.focus == Focus::Right && app.right_tab == RightTab::Tasks {
                if !app.tasks.is_empty() {
                    app.tasks_cursor = app.tasks.len().saturating_sub(1);
                }
            } else if app.focus == Focus::Right && app.right_tab == RightTab::Promotions {
                if !app.harness_promotions.entries.is_empty() {
                    app.harness_promotions_cursor =
                        app.harness_promotions.entries.len().saturating_sub(1);
                }
            } else if app.focus == Focus::Right && app.right_tab == RightTab::MergeGate {
                if !app.merge_gate.entries.is_empty() {
                    app.merge_gate_cursor = app.merge_gate.entries.len().saturating_sub(1);
                }
            } else {
                input::scroll_focused_history(app, Scroll::Bottom);
            }
        }

        // Tasks selection
        KeyCode::Up if adjust_inline_picker(app, active_pane, -1) => {}
        KeyCode::Down if adjust_inline_picker(app, active_pane, 1) => {}
        KeyCode::Up if app.focus == Focus::Right && app.right_tab == RightTab::Tasks => {
            app.tasks_cursor = app.tasks_cursor.saturating_sub(1);
        }
        KeyCode::Down if app.focus == Focus::Right && app.right_tab == RightTab::Tasks => {
            if !app.tasks.is_empty() {
                app.tasks_cursor = (app.tasks_cursor + 1).min(app.tasks.len().saturating_sub(1));
            }
        }
        KeyCode::Up if app.focus == Focus::Right && app.right_tab == RightTab::Promotions => {
            app.harness_promotions_cursor = app.harness_promotions_cursor.saturating_sub(1);
        }
        KeyCode::Down if app.focus == Focus::Right && app.right_tab == RightTab::Promotions => {
            if !app.harness_promotions.entries.is_empty() {
                app.harness_promotions_cursor = (app.harness_promotions_cursor + 1)
                    .min(app.harness_promotions.entries.len().saturating_sub(1));
            }
        }
        KeyCode::Up if app.focus == Focus::Right && app.right_tab == RightTab::MergeGate => {
            app.merge_gate_cursor = app.merge_gate_cursor.saturating_sub(1);
        }
        KeyCode::Down if app.focus == Focus::Right && app.right_tab == RightTab::MergeGate => {
            if !app.merge_gate.entries.is_empty() {
                app.merge_gate_cursor =
                    (app.merge_gate_cursor + 1).min(app.merge_gate.entries.len().saturating_sub(1));
            }
        }
        KeyCode::Char(' ') if app.focus == Focus::Right && app.right_tab == RightTab::Tasks => {
            if let Some(t) = app.tasks.get_mut(app.tasks_cursor) {
                t.done = !t.done;
            }
        }
        KeyCode::Char('a')
            if app.focus == Focus::Right && app.right_tab == RightTab::Promotions =>
        {
            let msg = match promotion_gate::approve_selected(app) {
                Ok(msg) => msg,
                Err(err) => format!("promotions approve failed: {err:#}"),
            };
            app.harness_promotions_status = Some(msg);
        }
        KeyCode::Char('h')
            if app.focus == Focus::Right && app.right_tab == RightTab::Promotions =>
        {
            let msg = match promotion_gate::hold_selected(app) {
                Ok(msg) => msg,
                Err(err) => format!("promotions hold failed: {err:#}"),
            };
            app.harness_promotions_status = Some(msg);
        }
        KeyCode::Char('p')
            if app.focus == Focus::Right && app.right_tab == RightTab::Promotions =>
        {
            let msg = match promotion_gate::apply_selected(app) {
                Ok(msg) => msg,
                Err(err) => format!("promotions apply failed: {err:#}"),
            };
            app.harness_promotions_status = Some(msg);
        }
        KeyCode::Char('r')
            if app.focus == Focus::Right && app.right_tab == RightTab::Promotions =>
        {
            let msg = match promotion_gate::refresh_promotions(app) {
                Ok(msg) => msg,
                Err(err) => format!("promotions refresh failed: {err:#}"),
            };
            app.harness_promotions_status = Some(msg);
        }
        KeyCode::Char('a') if app.focus == Focus::Right && app.right_tab == RightTab::MergeGate => {
            let msg = match merge_gate::approve_selected(app) {
                Ok(msg) => msg,
                Err(err) => format!("merge gate approve failed: {err:#}"),
            };
            app.merge_gate_status = Some(msg);
        }
        KeyCode::Char('h') if app.focus == Focus::Right && app.right_tab == RightTab::MergeGate => {
            let msg = match merge_gate::hold_selected(app) {
                Ok(msg) => msg,
                Err(err) => format!("merge gate hold failed: {err:#}"),
            };
            app.merge_gate_status = Some(msg);
        }
        KeyCode::Char('r') if app.focus == Focus::Right && app.right_tab == RightTab::MergeGate => {
            let msg = match merge_gate::refresh_merge_gate(app) {
                Ok(msg) => msg,
                Err(err) => format!("merge gate refresh failed: {err:#}"),
            };
            app.merge_gate_status = Some(msg);
        }

        // Send message
        KeyCode::Enter if !shift && apply_inline_picker(app, active_pane) => {}
        KeyCode::Enter if !shift => match app.focus {
            Focus::Coder => send_coder_message(app, coder_tx).await,
            Focus::Right => match app.right_tab {
                RightTab::Observer => send_observer_message(app, observer_tx, None).await,
                RightTab::Chat => send_chat_message(app, chat_tx, internal_tx).await,
                RightTab::Tasks => dispatch_selected_task(app, coder_tx, observer_tx).await,
                RightTab::Promotions => {
                    let msg = match promotion_gate::run_primary_action(app) {
                        Ok(msg) => msg,
                        Err(err) => format!("promotions primary action blocked: {err:#}"),
                    };
                    app.harness_promotions_status = Some(msg);
                }
                RightTab::MergeGate => {
                    let msg = match merge_gate::run_primary_action(app) {
                        Ok(msg) => msg,
                        Err(err) => format!("merge gate primary action blocked: {err:#}"),
                    };
                    app.merge_gate_status = Some(msg);
                }
            },
        },

        // Insert newline
        KeyCode::Enter if shift => match app.focus {
            Focus::Coder => app.coder.textarea.insert_newline(),
            Focus::Right => match app.right_tab {
                RightTab::Observer => app.observer.textarea.insert_newline(),
                RightTab::Chat => app.chat.textarea.insert_newline(),
                RightTab::Tasks | RightTab::Promotions | RightTab::MergeGate => {}
            },
        },

        // Pass everything else to tui-textarea
        _ => match app.focus {
            Focus::Coder => {
                app.coder.welcome_dismissed = true;
                app.coder.textarea.input(key);
            }
            Focus::Right => match app.right_tab {
                RightTab::Observer => {
                    app.observer.welcome_dismissed = true;
                    app.observer.textarea.input(key);
                }
                RightTab::Chat => {
                    app.chat.welcome_dismissed = true;
                    app.chat.textarea.input(key);
                }
                RightTab::Tasks | RightTab::Promotions | RightTab::MergeGate => {}
            },
        },
    }

    Ok(false)
}

// ── Token handlers ────────────────────────────────────────────────────────────

fn handle_coder_token(token: StreamToken, app: &mut App) {
    if app.ignore_coder_tokens {
        return;
    }
    match token {
        StreamToken::Delta(s) => {
            app.coder.push_delta(&s);
            // scroll = 0 means pinned; don't disturb if user has scrolled up.
        }
        StreamToken::ToolCall(call) => {
            app.coder_iter = app.coder_iter.saturating_add(1);
            if app.active_observer_contract.is_some() && app.active_observer_first_tool.is_none() {
                app.active_observer_first_tool = Some((call.name, call.arguments));
            }
        }
        StreamToken::GovernorState(s) => {
            app.coder_governor = Some(s);
        }
        StreamToken::RealizeState(s) => {
            app.coder_realize_state = Some(s);
        }
        StreamToken::Telemetry(_) => {}
        StreamToken::Done => {
            app.coder.finish_stream();
            finalize_active_observer_contract(app);
            if let Some(ref state) = app.coder_realize_state {
                if should_report_realize_summary(state) {
                    app.coder.push_tool(realize_state_summary_line(state));
                }
            }
        }
        StreamToken::Error(e) => {
            app.coder.push_tool(format!("ERROR: {e}"));
            app.coder.finish_stream();
        }
        StreamToken::Checkpoint(hash) => {
            app.last_git_checkpoint = Some(hash);
        }
    }
}

fn explicit_observer_response_decision(text: &str) -> Option<suggestion::ObserverResponseMode> {
    let lower = text.to_ascii_lowercase();
    for line in lower.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("decision:") {
            return parse_observer_response_mode(rest.trim());
        }
        if let Some(rest) = trimmed.strip_prefix("observer_response:") {
            return parse_observer_response_mode(rest.trim());
        }
        if trimmed.starts_with("[observer response]") {
            if trimmed.contains("override") {
                return Some(suggestion::ObserverResponseMode::Override);
            }
            if trimmed.contains("defer") {
                return Some(suggestion::ObserverResponseMode::Defer);
            }
            if trimmed.contains("accept") {
                return Some(suggestion::ObserverResponseMode::Accept);
            }
        }
    }
    None
}

fn parse_observer_response_mode(raw: &str) -> Option<suggestion::ObserverResponseMode> {
    let t = raw.trim().to_ascii_lowercase();
    match t.as_str() {
        "accept" => Some(suggestion::ObserverResponseMode::Accept),
        "override" => Some(suggestion::ObserverResponseMode::Override),
        "defer" => Some(suggestion::ObserverResponseMode::Defer),
        _ => None,
    }
}

fn tool_call_matches_observer_suggestion(
    call_name: &str,
    call_args_json: &str,
    primary: &suggestion::ObserverSuggestion,
) -> bool {
    let Some(tool) = primary.suggested_tool.as_deref() else {
        return false;
    };
    if tool != call_name {
        return false;
    }
    let expected = match primary.suggested_args.as_object() {
        Some(obj) if !obj.is_empty() => obj,
        _ => return true,
    };
    let parsed = serde_json::from_str::<serde_json::Value>(call_args_json).ok();
    let Some(actual) = parsed.as_ref().and_then(|v| v.as_object()) else {
        return false;
    };
    expected
        .iter()
        .all(|(key, value)| actual.get(key) == Some(value))
}

fn finalize_active_observer_contract(app: &mut App) {
    let Some(env) = app.active_observer_contract.take() else {
        app.active_observer_first_tool = None;
        return;
    };
    let explicit = app
        .coder
        .messages
        .iter()
        .rev()
        .find(|m| matches!(m.role, Role::Assistant) && m.complete && !m.content.trim().is_empty())
        .and_then(|m| explicit_observer_response_decision(&m.content));
    let inferred = match (
        suggestion::primary_suggestion(&env),
        app.active_observer_first_tool.as_ref(),
    ) {
        (Some(primary), Some((tool, args)))
            if tool_call_matches_observer_suggestion(tool, args, primary) =>
        {
            Some(suggestion::ObserverResponseMode::Accept)
        }
        (Some(_), Some(_)) => Some(suggestion::ObserverResponseMode::Override),
        _ => None,
    };
    let decision = explicit
        .or(inferred)
        .unwrap_or(suggestion::ObserverResponseMode::Defer);
    let focus = if env.response_contract.focus_axes.is_empty() {
        "current blocker".to_string()
    } else {
        env.response_contract.focus_axes.join(", ")
    };
    let note = match decision {
        suggestion::ObserverResponseMode::Accept => {
            "(observer contract) coder accepted the latest observer suggestion."
        }
        suggestion::ObserverResponseMode::Override => {
            "(observer contract) coder overrode the latest observer suggestion."
        }
        suggestion::ObserverResponseMode::Defer => {
            "(observer contract) coder deferred the latest observer suggestion."
        }
    };
    app.last_observer_contract_outcome = Some(format!(
        "{} focus={} blocker={}",
        decision.label(),
        focus,
        env.primary_blocker
    ));
    app.coder.push_tool(format!("{note} focus: {focus}"));
    app.active_observer_first_tool = None;
}

fn handle_observer_token(token: StreamToken, app: &mut App) {
    if app.ignore_observer_tokens {
        return;
    }
    match token {
        StreamToken::Delta(s) => {
            app.observer.push_delta(&s);
        }
        StreamToken::ToolCall(_)
        | StreamToken::Checkpoint(_)
        | StreamToken::GovernorState(_)
        | StreamToken::RealizeState(_)
        | StreamToken::Telemetry(_) => {}
        StreamToken::Done => {
            app.observer.finish_stream();
            if app.observer_meta_mode {
                app.observer_meta_mode = false;
                return;
            }
            let next_action_mode = app.observer_next_action_mode;
            if next_action_mode {
                app.observer_next_action_mode = false;
                finalize_observer_next_action_suggestion(app);
                return;
            }
            // A — auto-fix pipeline: queue the review text for Coder on next Tick.
            if app.auto_fix_mode && !app.coder.streaming {
                if let Some(review) = app
                    .observer
                    .messages
                    .iter()
                    .filter(|m| matches!(m.role, crate::tui::app::Role::Assistant) && m.complete)
                    .last()
                    .map(|m| m.content.trim().to_string())
                    .filter(|s| !s.is_empty())
                {
                    app.pending_auto_fix = Some(format!(
                        "[Auto-fix requested]\nThe Observer has reviewed the code and identified the following issues. Fix ALL of them:\n\n{review}"
                    ));
                }
            }

            // Language enforcement (Observer): if the model ignored the requested language (ja/fr),
            // schedule a one-shot rewrite retry. Streaming responses can't be rewritten mid-stream,
            // so we do it after Done and overwrite the last assistant message on the next Tick.
            let expected = app.lang.as_str();
            if expected != "en" && app.observer_lang_retry_budget > 0 {
                if let Some(last) = app
                    .observer
                    .messages
                    .iter()
                    .filter(|m| {
                        matches!(m.role, Role::Assistant)
                            && m.complete
                            && !m.content.trim().is_empty()
                    })
                    .last()
                {
                    if crate::lang_detect::needs_language_rewrite(expected, &last.content) {
                        app.observer_lang_retry_budget =
                            app.observer_lang_retry_budget.saturating_sub(1);
                        app.observer_lang_pending = Some(expected.to_string());
                        let note = match expected {
                            "fr" => "(lang fix) Observer a répondu dans la mauvaise langue — réécriture en français…",
                            "ja" => "(lang fix) Observerが指定言語で返していないため、書き直します…",
                            _ => "(lang fix) Observer language mismatch — rewriting…",
                        };
                        app.observer.push_tool(note.to_string());
                        // Skip loop detection on the pre-rewrite text.
                        return;
                    }
                }
            }
            // Detect repeated Observer replies and schedule a one-shot diff-only retry.
            // This prevents the common "template critique loop" when nothing new happened.
            if app.observer_loop_retry_budget > 0 {
                let asst: Vec<&Message> = app
                    .observer
                    .messages
                    .iter()
                    .filter(|m| {
                        matches!(m.role, Role::Assistant)
                            && m.complete
                            && !m.content.trim().is_empty()
                    })
                    .collect();
                if asst.len() >= 2 {
                    let last = asst[asst.len() - 1];
                    if !crate::loop_detect::is_skippable_for_loop(&last.content) {
                        let mut max_sim: f64 = 0.0;
                        for prev in asst[..asst.len() - 1].iter().rev().take(4) {
                            max_sim = max_sim
                                .max(crate::loop_detect::similarity(&last.content, &prev.content));
                        }
                        let detected = last.content.trim().len() >= 180 && max_sim >= 0.80;
                        if detected {
                            app.observer_loop_retry_budget =
                                app.observer_loop_retry_budget.saturating_sub(1);
                            app.observer_loop_pending = Some(max_sim);
                        }
                    }
                }
            }
        }
        StreamToken::Error(e) => {
            app.observer_meta_mode = false;
            app.observer_next_action_mode = false;
            app.observer.push_tool(format!("ERROR: {e}"));
            app.observer.finish_stream();
        }
    }
}

// ── Send helpers ──────────────────────────────────────────────────────────────

fn handle_chat_token(token: StreamToken, app: &mut App) {
    if app.ignore_chat_tokens {
        return;
    }
    match token {
        StreamToken::Delta(s) => {
            app.chat.push_delta(&s);
        }
        StreamToken::ToolCall(_)
        | StreamToken::Checkpoint(_)
        | StreamToken::GovernorState(_)
        | StreamToken::RealizeState(_)
        | StreamToken::Telemetry(_) => {}
        StreamToken::Done => {
            app.chat.finish_stream();
        }
        StreamToken::Error(e) => {
            app.chat.push_tool(format!("ERROR: {e}"));
            app.chat.finish_stream();
        }
    }
}

fn handle_tasks_planned(tasks: Vec<Task>, app: &mut App) {
    app.planning_tasks = false;
    if tasks.is_empty() {
        app.chat
            .push_tool("(task router) no tasks planned".to_string());
        return;
    }
    let n = tasks.len();
    app.tasks.extend(tasks);
    if app.tasks_cursor >= app.tasks.len() {
        app.tasks_cursor = app.tasks.len().saturating_sub(1);
    }
    app.chat.push_tool(format!(
        "(task router) planned {n} task(s) - Ctrl+R to view Tasks"
    ));
}

fn handle_task_plan_error(e: String, app: &mut App) {
    app.planning_tasks = false;
    app.chat.push_tool(format!("(task router error) {e}"));
}

/// Extract `@path` tokens from a message (unique, ordered).
/// Accepts patterns like @src/main.rs, @README.md, @dir/sub/file.txt.
fn parse_at_refs(text: &str) -> Vec<String> {
    let mut refs = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for word in text.split_whitespace() {
        if !word.starts_with('@') {
            continue;
        }
        let path = word.trim_start_matches('@');
        // Strip common trailing punctuation.
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

async fn send_coder_with_text(app: &mut App, tx: &mpsc::Sender<StreamToken>, text: String) {
    let text = text.trim().to_string();
    if text.is_empty() || app.coder.streaming {
        return;
    }

    if let Some(selector) = parse_meta_diagnose_command(&text) {
        send_meta_diagnose(app, tx, &selector).await;
        return;
    }

    // Handle slash commands before sending to AI.
    if handle_slash_command(&text, app, PaneId::Coder) {
        return;
    }
    if let Some(problem) = validate_pane_ready(app, PaneId::Coder) {
        app.coder.push_tool(problem);
        return;
    }

    // Abort any previous task before starting a new one.
    if let Some(handle) = app.coder_task.take() {
        handle.abort();
    }

    // Reset state for the new send.
    app.coder_iter = 0;
    app.coder.scroll = 0; // pin to bottom for new output
    app.ignore_coder_tokens = false;
    app.coder_realize_state = None;

    // Resolve tool_root early (needed for @ref file reads below).
    let tool_root = app.tool_root.clone().or_else(|| {
        std::env::current_dir()
            .ok()
            .map(|p| p.to_string_lossy().into_owned())
    });

    let intent_update = intent::normalize_intent_update(&text, app.coder_intent_anchor.as_ref());
    let intent_anchor =
        intent::apply_intent_update(app.coder_intent_anchor.as_ref(), intent_update, &text);
    let intent_anchor_message = intent::render_intent_anchor(&intent_anchor);
    let observer_soft_hint = app.pending_observer_hint.take();
    let observer_contract = app.pending_observer_contract.take();
    if intent_anchor.requires_human_confirmation {
        app.coder.push_tool(
            "[intent] ambiguous update detected; keeping current scope until clarified."
                .to_string(),
        );
    } else if intent_anchor.last_update_no_op {
        app.coder
            .push_tool("[intent] continue update preserved current scope.".to_string());
    }
    app.coder_intent_anchor = Some(intent_anchor.clone());
    if observer_soft_hint.is_some() {
        app.coder.push_tool(
            "(observer suggestion) advisory applied to this coder continuation turn.".to_string(),
        );
    }
    if observer_contract.is_some() {
        app.coder.push_tool(
            "(observer contract) next coder turn should explicitly accept, override, or defer the latest high-confidence observer suggestion."
                .to_string(),
        );
    }
    app.active_observer_contract = observer_contract.clone();
    app.active_observer_first_tool = None;

    // Expand @file references: read files and collect system messages to inject.
    let at_refs = parse_at_refs(&text);
    let mut at_ref_messages: Vec<ChatMessage> = Vec::new();
    app.coder.push_user(text.clone());
    for ref_path in &at_refs {
        let (content, is_err) = crate::file_tools::tool_read_file(ref_path, tool_root.as_deref());
        if is_err {
            app.coder.push_tool(format!("📎 @{ref_path}: not found"));
        } else {
            // Header line is "[path] (N lines, B bytes)" — use it as the notification.
            let header = content
                .lines()
                .next()
                .unwrap_or(ref_path.as_str())
                .to_string();
            app.coder.push_tool(format!("📎 injected: {header}"));
            at_ref_messages.push(ChatMessage {
                role: "system".to_string(),
                content: format!("[@{ref_path}]\n{content}"),
            });
        }
    }
    app.coder.streaming = true;
    app.coder
        .messages
        .push(Message::new_streaming(Role::Assistant));

    let history = app.coder.chat_history();
    let cfg = app.coder_cfg.clone();
    let max_iters = app.coder_max_iters.unwrap_or(agent::DEFAULT_MAX_ITERS);
    let realize_preset = app.coder_realize_preset;

    let persona_prompt = resolve_persona(&cfg.persona)
        .map(|p| p.prompt)
        .unwrap_or("");
    let lang = language_instruction(Some(&app.lang), &cfg.mode);
    let messages = build_coder_request_messages(
        lang,
        realize_preset,
        &history,
        intent_anchor_message,
        observer_soft_hint,
        observer_contract
            .as_ref()
            .map(build_observer_response_contract_message),
        &at_ref_messages,
        &text,
        persona_prompt,
    );

    // Scan project context once per session (guarded by stack_label being None).
    let (project_context, agents_md): (Option<String>, Option<String>) =
        if app.project_stack_label.is_none() {
            if let Some(ref root) = tool_root {
                if let Some(ctx) = crate::project::ProjectContext::scan(root).await {
                    app.project_stack_label = Some(ctx.stack_label());
                    if app.project_test_cmd.is_none() {
                        app.project_test_cmd = ctx.test_cmd.clone();
                    }
                    let agents = ctx.agents_md.clone();
                    (Some(ctx.to_context_text()), agents)
                } else {
                    (None, None)
                }
            } else {
                (None, None)
            }
        } else {
            (None, None)
        };
    let test_cmd = app.project_test_cmd.clone();

    let tx = tx.clone();
    let handle = tokio::spawn(async move {
        let approver = crate::approvals::AutoApprover;
        if let Err(e) = agent::run_agentic(
            messages,
            &cfg,
            tool_root.as_deref(),
            max_iters,
            tx.clone(),
            project_context,
            agents_md,
            test_cmd,
            false,
            Some(realize_preset),
            &approver,
        )
        .await
        {
            let _ = tx.send(StreamToken::Error(format!("{e:#}"))).await;
        }
    });
    app.coder_task = Some(handle);
}

fn build_coder_request_messages(
    lang: &str,
    realize_preset: agent::RealizePreset,
    history: &[ChatMessage],
    intent_anchor_message: String,
    observer_soft_hint: Option<String>,
    observer_response_contract: Option<String>,
    at_ref_messages: &[ChatMessage],
    user_text: &str,
    persona_prompt: &str,
) -> Vec<ChatMessage> {
    let system = agent::coder_system(persona_prompt, lang, Some(realize_preset));
    let mut messages = vec![ChatMessage {
        role: "system".to_string(),
        content: system,
    }];
    messages.push(ChatMessage {
        role: "system".to_string(),
        content: intent_anchor_message,
    });
    if let Some(observer_hint) = observer_soft_hint {
        messages.push(ChatMessage {
            role: "system".to_string(),
            content: observer_hint,
        });
    }
    if let Some(observer_contract) = observer_response_contract {
        messages.push(ChatMessage {
            role: "system".to_string(),
            content: observer_contract,
        });
    }
    let hist_len = history.len();
    for m in history.iter().take(hist_len.saturating_sub(1)) {
        messages.push(m.clone());
    }
    messages.extend(at_ref_messages.iter().cloned());
    messages.push(ChatMessage {
        role: "user".to_string(),
        content: user_text.to_string(),
    });
    messages
}

async fn send_coder_message(app: &mut App, tx: &mpsc::Sender<StreamToken>) {
    let text = app.coder.textarea.lines().join("\n").trim().to_string();
    if text.is_empty() || app.coder.streaming {
        return;
    }

    if let Some(selector) = parse_meta_diagnose_command(&text) {
        if let Some(problem) = validate_pane_ready(app, PaneId::Observer) {
            app.coder.push_tool(problem);
            return;
        }
        app.coder.textarea = tui_textarea::TextArea::default();
        send_meta_diagnose(app, tx, &selector).await;
        return;
    }

    // Handle slash commands before sending to AI.
    if handle_slash_command(&text, app, PaneId::Coder) {
        app.coder.textarea = tui_textarea::TextArea::default();
        return;
    }

    if let Some(problem) = validate_pane_ready(app, PaneId::Coder) {
        app.coder.push_tool(problem);
        return;
    }

    app.coder.textarea = tui_textarea::TextArea::default();
    send_coder_with_text(app, tx, text).await;
}

fn truncate_chars(s: &str, max_chars: usize) -> String {
    let s = s.trim_end();
    let mut it = s.chars();
    let head: String = it.by_ref().take(max_chars).collect();
    if it.next().is_some() {
        format!("{head}…[truncated]")
    } else {
        head
    }
}

fn build_recent_tool_outputs(messages: &[Message]) -> String {
    const MAX_TOOL_MSGS: usize = 6;
    const MAX_TOOL_CHARS: usize = 1_200;

    fn is_failure_like(output: &str) -> bool {
        let o = output.trim_start();
        o.contains("FAILED (exit_code:")
            || o.contains("REJECTED BY USER")
            || o.contains("[auto-test] ✗ FAILED")
    }

    let tool_indices: Vec<usize> = messages
        .iter()
        .enumerate()
        .filter(|(_, m)| matches!(m.role, Role::Tool) && m.complete)
        .map(|(i, _)| i)
        .collect();

    if tool_indices.is_empty() {
        return String::new();
    }

    let mut selected: Vec<usize> = Vec::new();
    // Prefer recent failures (including auto-test failures), then fill with recency context.
    for &idx in tool_indices.iter().rev() {
        if selected.len() >= MAX_TOOL_MSGS {
            break;
        }
        if is_failure_like(&messages[idx].content) {
            selected.push(idx);
        }
    }
    for &idx in tool_indices.iter().rev() {
        if selected.len() >= MAX_TOOL_MSGS {
            break;
        }
        if !selected.contains(&idx) {
            selected.push(idx);
        }
    }
    selected.sort_unstable();

    let count = selected.len();
    let snippet = selected
        .into_iter()
        .enumerate()
        .map(|(i, idx)| {
            format!(
                "[tool {}]: {}",
                i + 1,
                truncate_chars(&messages[idx].content, MAX_TOOL_CHARS)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    format!("\n\n[Recent tool outputs — last {count}]\n{snippet}")
}

fn parse_meta_diagnose_command(text: &str) -> Option<String> {
    let raw = text.trim();
    let cmd = "/meta-diagnose";
    if raw == cmd {
        return Some("last-fail".to_string());
    }
    raw.strip_prefix(cmd).and_then(|rest| {
        if rest.starts_with(char::is_whitespace) {
            let arg = rest.trim();
            Some(if arg.is_empty() {
                "last-fail".to_string()
            } else {
                arg.to_string()
            })
        } else {
            None
        }
    })
}

fn parse_tui_meta_target_index(selector: &str) -> Result<Option<usize>, String> {
    let selector = selector.trim();
    if selector.is_empty() || selector.eq_ignore_ascii_case("last-fail") {
        return Ok(None);
    }
    let Some(raw_id) = selector.strip_prefix("msg:") else {
        return Err(
            "meta-diagnose: expected `/meta-diagnose`, `/meta-diagnose last-fail`, or `/meta-diagnose msg:coder-<index>`"
                .to_string(),
        );
    };
    let raw_id = raw_id.trim();
    if raw_id.is_empty() {
        return Err("meta-diagnose: missing message id after `msg:`".to_string());
    }
    let numeric = raw_id.strip_prefix("coder-").unwrap_or(raw_id);
    numeric.parse::<usize>().map(Some).map_err(|_| {
        format!("meta-diagnose: invalid message id `{raw_id}` (expected `coder-<index>`)")
    })
}

fn resolve_tui_meta_target(messages: &[Message], selector: &str) -> Result<usize, String> {
    if let Some(target_idx) = parse_tui_meta_target_index(selector)? {
        let Some(target_msg) = messages.get(target_idx) else {
            return Err(format!(
                "meta-diagnose: no coder message found for `coder-{target_idx}`"
            ));
        };
        if !matches!(target_msg.role, Role::Assistant) || !target_msg.complete {
            return Err(format!(
                "meta-diagnose: `coder-{target_idx}` is not a completed coder assistant message"
            ));
        }
        if !meta_failure_like(&target_msg.content) {
            return Err(format!(
                "meta-diagnose: `coder-{target_idx}` is not failure-like"
            ));
        }
        return Ok(target_idx);
    }

    messages
        .iter()
        .enumerate()
        .rev()
        .find(|(_, m)| {
            matches!(m.role, Role::Assistant) && m.complete && meta_failure_like(&m.content)
        })
        .map(|(idx, _)| idx)
        .ok_or_else(|| "meta-diagnose: no failed coder message found".to_string())
}

fn meta_digest_text(text: &str, max_chars: usize, max_lines: usize) -> String {
    let mut out = text
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .lines()
        .map(|line| line.trim_end().to_string())
        .filter(|line| !line.trim().is_empty())
        .take(max_lines.max(1))
        .collect::<Vec<_>>()
        .join("\n");
    if out.len() > max_chars.max(32) {
        out.truncate(max_chars.max(32) - 3);
        out.push_str("...");
    }
    out
}

fn meta_first_line(text: &str, max_chars: usize) -> String {
    meta_digest_text(text, max_chars, 2)
        .lines()
        .next()
        .unwrap_or("")
        .trim()
        .to_string()
}

fn meta_failure_like(text: &str) -> bool {
    let s = text.to_ascii_lowercase();
    s.contains("failed")
        || s.contains("[error]")
        || s.contains("[stop]")
        || s.contains("stderr:")
        || s.contains("error:")
        || s.contains("fatal:")
        || s.contains("traceback")
        || s.contains("exception")
        || s.contains("rejected by user")
        || s.contains("sandbox breach")
        || s.contains("governor block")
        || s.contains("missing valid <plan>")
        || s.contains("missing <think>")
        || s.contains("[goal_check]")
}

fn meta_failure_kind(text: &str) -> &'static str {
    let s = text.to_ascii_lowercase();
    if s.contains("[stop]") || s.contains("no tool call") || s.contains("[goal_check]") {
        "no_tool"
    } else if s.contains("write_file failed")
        || s.contains("patch_file failed")
        || s.contains("apply_diff failed")
        || s.contains("rejected by user")
        || s.contains("unsafe path")
    {
        "bad_edit"
    } else if s.contains("false success") {
        "false_success"
    } else if meta_failure_like(text) {
        "tool_error"
    } else {
        "unclear"
    }
}

fn now_iso_utc() -> String {
    fn civil_from_days(days: i64) -> (i32, u32, u32) {
        let z = days + 719_468;
        let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
        let y = yoe + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = mp + if mp < 10 { 3 } else { -9 };
        let year = y + if m <= 2 { 1 } else { 0 };
        (year as i32, m as u32, d as u32)
    }

    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let days = secs.div_euclid(86_400);
    let sod = secs.rem_euclid(86_400);
    let hour = sod / 3_600;
    let minute = (sod % 3_600) / 60;
    let second = sod % 60;
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

fn compact_meta_stamp(ts: &str) -> String {
    let digits: String = ts.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() >= 14 {
        format!("{}T{}Z", &digits[..8], &digits[8..14])
    } else {
        format!(
            "unix{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
        )
    }
}

fn meta_slug(value: &str, fallback: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for ch in value.chars() {
        let mapped = if ch.is_ascii_alphanumeric() {
            prev_dash = false;
            Some(ch.to_ascii_lowercase())
        } else if ch == '-' || ch == '_' {
            prev_dash = false;
            Some(ch)
        } else if prev_dash {
            None
        } else {
            prev_dash = true;
            Some('-')
        };
        if let Some(ch) = mapped {
            out.push(ch);
        }
    }
    let trimmed = out.trim_matches('-').trim_matches('_');
    let base = if trimmed.is_empty() {
        fallback
    } else {
        trimmed
    };
    base.chars().take(64).collect()
}

fn save_tui_meta_diagnose_artifact(
    tool_root: Option<&str>,
    artifact: &serde_json::Value,
) -> anyhow::Result<PathBuf> {
    let base = tool_root
        .map(PathBuf::from)
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."));
    let dir = base.join(".spiral-coder").join("meta-diagnose");
    let ts = artifact
        .get("ts")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let thread_id = artifact
        .get("thread_id")
        .and_then(|v| v.as_str())
        .unwrap_or("tui-session");
    let target_id = artifact
        .get("target_message_id")
        .and_then(|v| v.as_str())
        .unwrap_or("msg");
    let stem = format!(
        "{}__thread-{}__msg-{}",
        compact_meta_stamp(ts),
        meta_slug(thread_id, "thread"),
        meta_slug(target_id, "msg")
    );
    let mut path = dir.join(format!("{stem}.json"));
    if path.exists() {
        for idx in 1..1000u32 {
            let candidate = dir.join(format!("{stem}__{idx:02}.json"));
            if !candidate.exists() {
                path = candidate;
                break;
            }
        }
    }
    crate::trace_writer::safe_mkdir(&path)?;
    let json = serde_json::to_string_pretty(artifact)?;
    std::fs::write(&path, json.as_bytes())?;

    let index_path = dir.join("index.jsonl");
    crate::trace_writer::safe_mkdir(&index_path)?;
    let index_line = json!({
        "ts": artifact.get("ts").cloned().unwrap_or(serde_json::Value::Null),
        "thread_id": artifact.get("thread_id").cloned().unwrap_or(serde_json::Value::Null),
        "target_message_id": artifact.get("target_message_id").cloned().unwrap_or(serde_json::Value::Null),
        "path": path.to_string_lossy(),
        "parse_ok": artifact.get("parse_ok").cloned().unwrap_or(serde_json::Value::Null),
        "parse_error": artifact.get("parse_error").cloned().unwrap_or(serde_json::Value::Null),
        "provider": artifact.get("provider").cloned().unwrap_or(serde_json::Value::Null),
        "model": artifact.get("model").cloned().unwrap_or(serde_json::Value::Null),
        "primary_failure": artifact
            .get("diagnosis")
            .and_then(|v| v.get("primary_failure"))
            .cloned()
            .unwrap_or(serde_json::Value::Null),
    });
    let mut index = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&index_path)?;
    index.write_all(format!("{index_line}\n").as_bytes())?;
    Ok(path)
}

fn build_tui_meta_failure_packet_for_selector(
    app: &App,
    selector: &str,
) -> Result<serde_json::Value, String> {
    let target_idx = resolve_tui_meta_target(&app.coder.messages, selector)?;
    let Some(target_msg) = app.coder.messages.get(target_idx) else {
        return Err(format!(
            "meta-diagnose: no coder message found for `coder-{target_idx}`"
        ));
    };
    let start = target_idx.saturating_sub(8);
    let base = &app.coder.messages[start..=target_idx];
    let recent_users: Vec<String> = base
        .iter()
        .filter(|m| matches!(m.role, Role::User) && m.complete)
        .rev()
        .take(3)
        .map(|m| meta_digest_text(&m.content, 220, 4))
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let recent_assistant: Vec<String> = base
        .iter()
        .enumerate()
        .filter(|(i, m)| {
            *i + start != target_idx && matches!(m.role, Role::Assistant) && m.complete
        })
        .rev()
        .take(3)
        .map(|(_, m)| meta_digest_text(&m.content, 220, 4))
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let recent_tool_results: Vec<serde_json::Value> = base
        .iter()
        .filter(|m| matches!(m.role, Role::Tool) && m.complete)
        .rev()
        .take(4)
        .map(|m| {
            json!({
                "digest": meta_digest_text(&m.content, 260, 6),
                "first_line": meta_first_line(&m.content, 220),
            })
        })
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let intent_anchor = app
        .coder_intent_anchor
        .as_ref()
        .map(|anchor| {
            json!({
                "revision": anchor.revision,
                "goal": anchor.goal,
                "target": anchor.target,
                "constraints": anchor.constraints,
                "success_criteria": anchor.success_criteria,
                "optimization_hints": anchor.optimization_hints,
                "ambiguity": anchor.ambiguity,
                "confidence": anchor.confidence,
                "requires_human_confirmation": anchor.requires_human_confirmation,
                "baseline": intent::anchor_baseline(anchor),
            })
        })
        .unwrap_or(serde_json::Value::Null);
    let actual_outcome = meta_digest_text(&target_msg.content, 320, 8);
    let system_digest = meta_digest_text(
        &format!(
            "coder_mode={:?}\nobserver_mode={:?}\ncoder_provider={}\nobserver_provider={}",
            app.coder_cfg.mode,
            app.observer_cfg.mode,
            app.coder_cfg.provider,
            app.observer_cfg.provider
        ),
        320,
        8,
    );
    Ok(json!({
        "thread_id": app.tool_root.clone().unwrap_or_else(|| "tui-session".to_string()),
        "target_message_id": format!("coder-{}", target_idx),
        "task_summary": recent_users.last().cloned().unwrap_or_default(),
        "expected_outcome": recent_users.last().cloned().unwrap_or_else(|| "Complete the requested task without failure.".to_string()),
        "actual_outcome": actual_outcome,
        "failure_kind": meta_failure_kind(&target_msg.content),
        "coder_mode": format!("{:?}", app.coder_cfg.mode),
        "coder_provider": app.coder_cfg.provider.to_string(),
        "coder_model": app.coder_cfg.model.clone(),
        "observer_model": app.observer_cfg.model.clone(),
        "tool_root": app.tool_root.clone(),
        "cur_cwd": app.tool_root.clone(),
        "checkpoint": app.last_git_checkpoint.clone(),
        "system_prompt_digest": system_digest,
        "project_context_digest": app.project_stack_label.clone(),
        "intent_anchor": intent_anchor,
        "agents_md_digest": serde_json::Value::Null,
        "available_tools": Vec::<String>::new(),
        "recent_user_messages": recent_users,
        "recent_assistant_messages": recent_assistant,
        "recent_tool_calls": Vec::<serde_json::Value>::new(),
        "recent_tool_results": recent_tool_results,
        "last_error_digest": meta_first_line(&target_msg.content, 240),
        "loop_signals": {
            "same_command_repeats": 0,
            "same_error_repeats": 0,
            "same_output_repeats": 0,
            "ui_loop_depth": 0,
        },
        "approval_signals": Vec::<String>::new(),
        "packet_notes": vec![
            "tui meta-diagnose packet built from visible coder history".to_string(),
            "tui path does not preserve structured tool-call snapshots".to_string(),
            "intent_anchor and recent tool results are included when available".to_string(),
        ],
    }))
}

fn build_tui_meta_diagnose_prompt(packet: &serde_json::Value, lang: &str) -> String {
    let lang_name = match lang {
        "fr" => "French",
        "en" => "English",
        _ => "Japanese",
    };
    let schema = json!({
        "summary": format!("{lang_name} summary"),
        "primary_failure": "contract_ambiguity",
        "causes": [{
            "label": "contract_ambiguity",
            "why": format!("{lang_name} explanation"),
            "evidence": ["evidence 1", "evidence 2"],
            "fix_layer": "instruction",
            "minimal_patch": format!("{lang_name} minimal patch"),
            "confidence": 0.84
        }],
        "recommended_experiments": [{
            "change": format!("{lang_name} experiment change"),
            "verify": format!("{lang_name} verification"),
            "expected_signal": format!("{lang_name} expected signal")
        }],
        "do_not_change": ["repo code itself"]
    });
    format!(
        "This is meta analysis, not implementation.\n\
Tool calls, code changes, and diff application are forbidden.\n\
Your task is to diagnose the immediate failure by layer, using the failure packet only.\n\
Write summary/why/evidence/minimal_patch/experiments in {lang_name}.\n\
Keep fix_layer values in English enum form.\n\
Return JSON only. No markdown. No backticks. No commentary outside JSON.\n\n\
Requirements:\n\
- Identify up to 3 causes.\n\
- Each cause must include evidence.\n\
- Each cause must choose exactly one fix_layer.\n\
- Keep patches minimal and rerunnable.\n\
- Distinguish repo_code vs agent/harness issues.\n\
- Do not overclaim; use confidence 0.0..1.0.\n\n\
fix_layer enum:\n\
guideline | instruction | skill | harness | tool | index | schema_ci | repo_code | no_change\n\n\
Output schema:\n\
{}\n\n\
failure packet:\n\
<packet>\n\
{}\n\
</packet>",
        serde_json::to_string_pretty(&schema).unwrap_or_else(|_| "{}".to_string()),
        serde_json::to_string_pretty(packet).unwrap_or_else(|_| "{}".to_string())
    )
}

fn build_tui_next_action_prompt(
    packet: &serde_json::Value,
    lang: &str,
    reason_hint: &str,
) -> String {
    let lang_name = match lang {
        "fr" => "French",
        "en" => "English",
        _ => "Japanese",
    };
    let reason = reason_hint.trim();
    let schema = json!({
        "summary": format!("{lang_name} summary of the blocker"),
        "primary_blocker": "missing_concrete_next_step",
        "scores": {
            "correctness": 0.82,
            "security": 0.61,
            "efficiency": 0.77,
            "readability": 0.80
        },
        "response_contract": {
            "required": true,
            "focus_axes": ["security"],
            "note": format!("{lang_name} note about when to accept, override, or defer")
        },
        "suggestions": [{
            "kind": "read",
            "reason": format!("{lang_name} reason"),
            "confidence": 0.84,
            "suggested_tool": "read_file",
            "suggested_args": { "path": "src/tui/events.rs" },
            "based_on": ["intent_anchor", "recent_tool_results"]
        }],
        "quickest_check": "read_file(path=src/tui/events.rs)",
        "why_this_first": format!("{lang_name} why this is the smallest next step"),
        "fallback": format!("{lang_name} fallback if the first action fails")
    });
    format!(
        "This is intervention mode, not critique.\n\
Tool calls, code changes, and diff application are forbidden.\n\
Your task is to help the Coder take the next concrete step only.\n\
Write all free-text explanations in {lang_name}. Keep enum values, tool names, and file paths in English.\n\
Return JSON only. No markdown. No backticks. No prose outside JSON.\n\n\
Output schema:\n\
{}\n\n\
Rules:\n\
- Prefer small, local, reversible actions.\n\
- Mention exact files, commands, or symbols when possible.\n\
- Respect intent_anchor.goal/target when present; do not widen scope.\n\
- If recent_tool_results already contain useful evidence, continue from that evidence instead of restarting broad search.\n\
- If the blocker is repo code, say so directly.\n\
- If the blocker is instruction/harness/tooling, say so directly.\n\
- Do not broaden into a full review.\n\
- If evidence is weak, make quickest_check purely diagnostic.\n\
- scores must be normalized 0..1 floats for: correctness, security, efficiency, readability\n\
- response_contract.required should be true when the next coder turn must explicitly accept, override, or defer your primary suggestion\n\
- response_contract.focus_axes should name the dimensions that most need attention (correctness|security|efficiency|readability)\n\
- response_contract.note should briefly explain when override/defer is acceptable\n\
- suggestions.kind must be exactly one of: search | read | done | clarify | abandon_path\n\
- suggestions.suggested_args must be an object (or empty object)\n\
- suggestions.based_on should cite coarse facts only: intent_anchor, recent_tool_results, recovery_stage, failure_kind\n\
- Keep suggestions advisory-only; do not claim you already executed anything.\n\n\
reason_hint: {reason}\n\
stuck packet:\n\
<packet>\n\
{}\n\
</packet>",
        serde_json::to_string_pretty(&schema).unwrap_or_else(|_| "{}".to_string()),
        serde_json::to_string_pretty(packet).unwrap_or_else(|_| "{}".to_string())
    )
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct TuiNextActionReplayOutcome {
    pub selector: String,
    pub reason_hint: String,
    pub target_message_id: Option<String>,
    pub failure_kind: String,
    pub packet: serde_json::Value,
    pub observer_prompt: String,
    pub observer_raw_response: String,
    pub parsed_suggestion: Option<suggestion::ObserverSuggestionEnvelope>,
    pub pending_observer_hint: Option<String>,
    pub pending_observer_contract: Option<suggestion::ObserverSuggestionEnvelope>,
    pub coder_preview_messages: Vec<ChatMessage>,
}

fn finalize_observer_next_action_suggestion(app: &mut App) {
    let Some(idx) = app.observer.messages.iter().rposition(|m| {
        matches!(m.role, Role::Assistant) && m.complete && !m.content.trim().is_empty()
    }) else {
        app.last_observer_suggestion = None;
        return;
    };
    let raw = app
        .observer
        .messages
        .get(idx)
        .map(|m| m.content.clone())
        .unwrap_or_default();
    if raw.trim().is_empty() {
        app.last_observer_suggestion = None;
        return;
    }
    if let Some(parsed) = suggestion::parse_observer_suggestion_envelope(&raw) {
        let rendered = suggestion::format_observer_suggestion_envelope(&parsed);
        if let Some(msg) = app.observer.messages.get_mut(idx) {
            msg.content = rendered;
        }
        if let Some(hint) = build_observer_suggestion_soft_hint(&parsed) {
            app.pending_observer_hint = Some(hint);
            app.pending_observer_contract = build_observer_response_contract(&parsed);
            app.coder.push_tool(
                "(observer suggestion) queued an advisory next step for the next coder continuation."
                    .to_string(),
            );
        } else {
            app.pending_observer_hint = None;
            app.pending_observer_contract = None;
        }
        app.last_observer_suggestion = Some(parsed);
    } else {
        app.last_observer_suggestion = None;
        app.pending_observer_hint = None;
        app.pending_observer_contract = None;
        app.observer.push_tool(
            "(next-action) structured parse failed; raw Observer output was preserved.".to_string(),
        );
    }
}

pub(crate) fn latest_tui_next_action_target(app: &App) -> Option<(usize, String)> {
    latest_tui_next_action_target_impl(app)
}

pub(crate) fn replay_observer_next_action_case(
    app: &mut App,
    selector: &str,
    reason_hint: &str,
    observer_response: &str,
) -> Result<TuiNextActionReplayOutcome, String> {
    let packet = build_tui_meta_failure_packet_for_selector(app, selector)?;
    let observer_prompt = build_tui_next_action_prompt(&packet, app.lang.as_str(), reason_hint);
    let target_message_id = packet
        .get("target_message_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let failure_kind = packet
        .get("failure_kind")
        .and_then(|v| v.as_str())
        .unwrap_or("unclear")
        .to_string();

    app.observer_meta_mode = false;
    app.observer_next_action_mode = true;
    app.last_observer_suggestion = None;
    app.pending_observer_hint = None;
    app.pending_observer_contract = None;
    app.ignore_observer_tokens = false;
    app.observer.streaming = true;
    app.observer
        .messages
        .push(Message::new_streaming(Role::Assistant));
    if let Some(last) = app.observer.messages.last_mut() {
        last.content = observer_response.to_string();
    }
    handle_observer_token(StreamToken::Done, app);

    let preview_messages = preview_coder_continuation_messages(app, "Continue.");

    Ok(TuiNextActionReplayOutcome {
        selector: selector.to_string(),
        reason_hint: reason_hint.to_string(),
        target_message_id,
        failure_kind,
        packet,
        observer_prompt,
        observer_raw_response: observer_response.to_string(),
        parsed_suggestion: app.last_observer_suggestion.clone(),
        pending_observer_hint: app.pending_observer_hint.clone(),
        pending_observer_contract: app.pending_observer_contract.clone(),
        coder_preview_messages: preview_messages,
    })
}

fn preview_coder_continuation_messages(app: &App, text: &str) -> Vec<ChatMessage> {
    let history = app.coder.chat_history();
    let intent_update = intent::normalize_intent_update(text, app.coder_intent_anchor.as_ref());
    let intent_anchor =
        intent::apply_intent_update(app.coder_intent_anchor.as_ref(), intent_update, text);
    let intent_anchor_message = intent::render_intent_anchor(&intent_anchor);
    let persona_prompt = resolve_persona(&app.coder_cfg.persona)
        .map(|p| p.prompt)
        .unwrap_or("");
    let lang = language_instruction(Some(&app.lang), &app.coder_cfg.mode);
    build_coder_request_messages(
        lang,
        app.coder_realize_preset,
        &history,
        intent_anchor_message,
        app.pending_observer_hint.clone(),
        app.pending_observer_contract
            .as_ref()
            .map(build_observer_response_contract_message),
        &[],
        text,
        persona_prompt,
    )
}

fn build_observer_suggestion_soft_hint(
    env: &suggestion::ObserverSuggestionEnvelope,
) -> Option<String> {
    let primary = suggestion::primary_suggestion(env)?;
    if primary.confidence < 0.75 {
        return None;
    }
    let action = if let Some(tool) = primary.suggested_tool.as_deref() {
        let args = render_observer_suggestion_args(&primary.suggested_args);
        if args.is_empty() {
            tool.to_string()
        } else {
            format!("{tool}({args})")
        }
    } else if !env.quickest_check.is_empty() {
        env.quickest_check.clone()
    } else {
        primary.reason.clone()
    };
    let reason = if primary.reason.is_empty() {
        env.summary.as_str()
    } else {
        primary.reason.as_str()
    };
    Some(format!(
        "<observer_advisory priority=\"soft\" source=\"external_observer\">\n\
[Observer advisory — soft hint only]\n\
Stay within the current intent anchor and task scope.\n\
If you are still stuck, prefer this next step before widening search:\n\
{action}\n\
Why: {reason}\n\
</observer_advisory>"
    ))
}

fn build_observer_response_contract(
    env: &suggestion::ObserverSuggestionEnvelope,
) -> Option<suggestion::ObserverSuggestionEnvelope> {
    let primary = suggestion::primary_suggestion(env)?;
    if primary.confidence < 0.75 || !env.response_contract.required {
        return None;
    }
    Some(env.clone())
}

fn build_observer_response_contract_message(
    env: &suggestion::ObserverSuggestionEnvelope,
) -> String {
    let focus = if env.response_contract.focus_axes.is_empty() {
        "current blocker".to_string()
    } else {
        env.response_contract.focus_axes.join(", ")
    };
    let quickest = if env.quickest_check.trim().is_empty() {
        suggestion::primary_suggestion(env)
            .map(|primary| {
                if let Some(tool) = primary.suggested_tool.as_deref() {
                    let args = render_observer_suggestion_args(&primary.suggested_args);
                    if args.is_empty() {
                        tool.to_string()
                    } else {
                        format!("{tool}({args})")
                    }
                } else {
                    primary.reason.clone()
                }
            })
            .unwrap_or_else(|| "the primary observer suggestion".to_string())
    } else {
        env.quickest_check.clone()
    };
    let note = if env.response_contract.note.is_empty() {
        "If you do not follow the observer's next step on this turn, explicitly choose override or defer and give a short reason."
            .to_string()
    } else {
        env.response_contract.note.clone()
    };
    format!(
        "<observer_response_contract required=\"true\" source=\"external_observer\">\n\
[Observer response contract]\n\
On this coder turn, explicitly address the latest high-confidence observer suggestion before finishing.\n\
Choose one: accept | override | defer.\n\
If you accept, follow this next step: {quickest}\n\
If you override, justify the divergence briefly and keep scope narrow.\n\
If you defer, explain what missing evidence blocks action.\n\
In your next <plan> or visible planning text, name how you will handle this Observer input.\n\
Focus axes: {focus}\n\
Note: {note}\n\
Suggested plain-text block:\n\
[Observer response]\n\
decision: accept|override|defer\n\
reason: short justification\n\
</observer_response_contract>"
    )
}

fn render_observer_suggestion_args(args: &serde_json::Value) -> String {
    let Some(obj) = args.as_object() else {
        return String::new();
    };
    let mut keys: Vec<&String> = obj.keys().collect();
    keys.sort();
    keys.into_iter()
        .filter_map(|key| {
            let value = obj.get(key)?;
            Some(format!(
                "{key}={}",
                render_observer_suggestion_arg_value(value)
            ))
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn render_observer_suggestion_arg_value(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(items) => items
            .iter()
            .map(render_observer_suggestion_arg_value)
            .collect::<Vec<_>>()
            .join("|"),
        _ => value.to_string(),
    }
}

async fn send_meta_diagnose(app: &mut App, tx: &mpsc::Sender<StreamToken>, selector: &str) {
    if let Some(problem) = validate_pane_ready(app, PaneId::Observer) {
        app.observer.push_tool(problem);
        return;
    }

    app.right_tab = RightTab::Observer;
    app.observer.scroll = 0;
    let selector = selector.trim();
    let packet = match build_tui_meta_failure_packet_for_selector(app, selector) {
        Ok(packet) => packet,
        Err(msg) => {
            app.observer.push_tool(msg);
            return;
        }
    };

    if let Some(handle) = app.observer_task.take() {
        handle.abort();
    }

    let started_at = now_iso_utc();
    let prompt = build_tui_meta_diagnose_prompt(&packet, &app.lang);
    let cfg = app.observer_cfg.clone();
    let tool_root = app.tool_root.clone();
    let lang = app.lang.clone();
    let target_id = packet
        .get("target_message_id")
        .and_then(|v| v.as_str())
        .unwrap_or("msg")
        .to_string();
    let failure_kind = packet
        .get("failure_kind")
        .and_then(|v| v.as_str())
        .unwrap_or("unclear")
        .to_string();

    app.ignore_observer_tokens = false;
    app.observer_meta_mode = true;
    app.observer.push_user(format!(
        "[META-DIAGNOSE] target={target_id} kind={failure_kind}"
    ));
    app.observer.streaming = true;
    app.observer
        .messages
        .push(Message::new_streaming(Role::Assistant));

    let tx = tx.clone();
    let handle = tokio::spawn(async move {
        let client = reqwest::Client::new();
        let provider = providers::build_provider(client, &cfg);
        let system = format!(
            "{}\n\n[Language]\n{}",
            mode_prompt(&cfg.mode),
            language_instruction(Some(&lang), &cfg.mode)
        );
        let req = ChatRequest {
            messages: vec![
                ChatMessage {
                    role: "system".to_string(),
                    content: system,
                },
                ChatMessage {
                    role: "user".to_string(),
                    content: prompt.clone(),
                },
            ],
            temperature: Some(0.2),
            max_tokens: Some(cfg.max_tokens.min(1_800)),
            metadata: None,
        };

        let (raw_response, diagnosis, parse_ok, parse_error) = match provider.chat(&req).await {
            Ok(resp) => {
                let raw = resp.content.trim().to_string();
                match serde_json::from_str::<serde_json::Value>(&raw) {
                    Ok(v) if v.is_object() => (raw, v, true, serde_json::Value::Null),
                    Ok(_) => (
                        raw.clone(),
                        serde_json::Value::Null,
                        false,
                        serde_json::Value::String("json_root_not_object".to_string()),
                    ),
                    Err(e) => (
                        raw.clone(),
                        serde_json::Value::Null,
                        false,
                        serde_json::Value::String(format!("invalid_json: {e}")),
                    ),
                }
            }
            Err(e) => {
                let msg = format!("[error] {e}");
                (
                    msg.clone(),
                    serde_json::Value::Null,
                    false,
                    serde_json::Value::String(format!("request_failed: {e}")),
                )
            }
        };

        let config_digest = {
            let seed = json!({
                "thread_id": packet.get("thread_id").cloned().unwrap_or(serde_json::Value::Null),
                "coder_mode": packet.get("coder_mode").cloned().unwrap_or(serde_json::Value::Null),
                "observer_mode": format!("{:?}", cfg.mode),
                "provider": cfg.provider.to_string(),
                "model": cfg.model,
                "tool_root": tool_root,
            })
            .to_string();
            let mut hasher = DefaultHasher::new();
            seed.hash(&mut hasher);
            format!("{:016x}", hasher.finish())
        };

        let artifact = json!({
            "ts": started_at,
            "thread_id": packet.get("thread_id").cloned().unwrap_or(serde_json::Value::Null),
            "target_message_id": packet.get("target_message_id").cloned().unwrap_or(serde_json::Value::Null),
            "packet": packet,
            "observer_prompt": prompt,
            "raw_response": raw_response,
            "diagnosis": diagnosis,
            "parse_ok": parse_ok,
            "parse_error": if parse_ok { serde_json::Value::Null } else { parse_error },
            "provider": cfg.provider.to_string(),
            "model": cfg.model.clone(),
            "config_digest": config_digest,
        });
        let saved_note = match save_tui_meta_diagnose_artifact(tool_root.as_deref(), &artifact) {
            Ok(path) => format!("\n\n[saved] {}", path.display()),
            Err(e) => format!("\n\n[save_error] {e}"),
        };
        let display = if parse_ok {
            format!(
                "{}{}",
                serde_json::to_string_pretty(
                    artifact
                        .get("diagnosis")
                        .unwrap_or(&serde_json::Value::Null)
                )
                .unwrap_or_else(|_| raw_response.clone()),
                saved_note
            )
        } else {
            format!("{raw_response}{saved_note}")
        };
        fake_stream_text(&tx, &display).await;
    });
    app.observer_task = Some(handle);
}

fn latest_tui_next_action_target_impl(app: &App) -> Option<(usize, String)> {
    let (idx, msg) = app.coder.messages.iter().enumerate().rev().find(|(_, m)| {
        matches!(m.role, Role::Assistant) && m.complete && !m.content.trim().is_empty()
    })?;
    if !meta_failure_like(&msg.content) {
        return None;
    }
    let failure_kind = meta_failure_kind(&msg.content);
    let reason = if let Some(governor) = app.coder_governor.as_ref() {
        let stage = governor
            .recovery_stage
            .clone()
            .unwrap_or_else(|| "diagnose".to_string());
        format!(
            "{failure_kind}; recovery_stage={stage}; same_error_repeats={}; same_command_repeats={}",
            governor.same_error_repeats, governor.same_command_repeats
        )
    } else {
        failure_kind.to_string()
    };
    Some((idx, reason))
}

async fn send_next_action_assist(
    app: &mut App,
    tx: &mpsc::Sender<StreamToken>,
    selector: &str,
    reason_hint: &str,
) {
    if let Some(problem) = validate_pane_ready(app, PaneId::Observer) {
        app.observer.push_tool(problem);
        return;
    }

    app.right_tab = RightTab::Observer;
    app.observer.scroll = 0;
    let packet = match build_tui_meta_failure_packet_for_selector(app, selector) {
        Ok(packet) => packet,
        Err(msg) => {
            app.observer.push_tool(msg);
            return;
        }
    };
    let target_id = packet
        .get("target_message_id")
        .and_then(|v| v.as_str())
        .unwrap_or("coder");
    let failure_kind = packet
        .get("failure_kind")
        .and_then(|v| v.as_str())
        .unwrap_or("unclear");
    let prompt = build_tui_next_action_prompt(&packet, app.lang.as_str(), reason_hint);
    let label = format!("[NEXT-ACTION] target={target_id} kind={failure_kind}");

    if let Some(handle) = app.observer_task.take() {
        handle.abort();
    }

    app.observer_meta_mode = false;
    app.observer_next_action_mode = true;
    app.last_observer_suggestion = None;
    app.pending_observer_hint = None;
    app.pending_observer_contract = None;
    app.ignore_observer_tokens = false;
    app.observer_loop_retry_budget = 0;
    app.observer_loop_pending = None;
    app.observer_lang_retry_budget = 1;
    app.observer_lang_pending = None;
    app.observer.push_user(label);
    app.observer.streaming = true;
    app.observer
        .messages
        .push(Message::new_streaming(Role::Assistant));

    let cfg = {
        let mut cfg = app.observer_cfg.clone();
        cfg.temperature = 0.2;
        cfg.max_tokens = cfg.max_tokens.min(1200);
        cfg
    };
    let tx = tx.clone();
    let lang = app.lang.clone();
    let handle = tokio::spawn(async move {
        use crate::config::ProviderKind;
        use crate::streaming::{stream_anthropic, stream_openai_compat};
        let client = reqwest::Client::new();
        let system = format!(
            "You are Observer in next-action assist mode.\n\
Do not perform a broad critique.\n\
Focus only on the next concrete step.\n\n\
[Language]\n{}",
            language_instruction(Some(&lang), &cfg.mode)
        );
        let messages = vec![
            ChatMessage {
                role: "system".to_string(),
                content: system,
            },
            ChatMessage {
                role: "user".to_string(),
                content: prompt,
            },
        ];
        let result = match cfg.provider {
            ProviderKind::Anthropic => stream_anthropic(&client, &cfg, &messages, tx.clone()).await,
            ProviderKind::Hf => {
                let provider = providers::build_provider(client.clone(), &cfg);
                let req = ChatRequest {
                    messages: messages.clone(),
                    temperature: Some(0.2),
                    max_tokens: Some(cfg.max_tokens),
                    metadata: None,
                };
                match provider.chat(&req).await {
                    Ok(resp) => {
                        fake_stream_text(&tx, &resp.content).await;
                        Ok(())
                    }
                    Err(e) => Err(e),
                }
            }
            _ => stream_openai_compat(&client, &cfg, &messages, None, tx.clone()).await,
        };
        if let Err(e) = result {
            let prefix = match lang.as_str() {
                "fr" => "Observer next-action failed",
                "en" => "Observer next-action failed",
                _ => "Observer next-action failed",
            };
            let _ = tx.send(StreamToken::Error(format!("{prefix}: {e}"))).await;
        }
    });
    app.observer_task = Some(handle);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ProviderKind, RunConfig};
    use crate::modes::Mode;
    use std::time::{SystemTime, UNIX_EPOCH};

    include!("interaction_tests.rs");

    fn msg(role: Role, content: &str) -> Message {
        Message::new_complete(role, content.to_string())
    }

    fn test_cfg(mode: Mode) -> RunConfig {
        RunConfig {
            provider: ProviderKind::OpenAiCompatible,
            model: "test-model".to_string(),
            chat_model: "test-model".to_string(),
            code_model: "test-model".to_string(),
            api_key: None,
            base_url: "https://api.openai.com/v1".to_string(),
            mode,
            persona: "default".to_string(),
            temperature: 0.2,
            max_tokens: 1024,
            timeout_seconds: 30,
            hf_device: "cpu".to_string(),
            hf_local_only: false,
        }
    }

    fn isolated_prefs_root() -> String {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir()
            .join(format!("spiral_coder_tui_events_{stamp}"))
            .display()
            .to_string()
    }

    #[test]
    fn parses_meta_diagnose_message_selector() {
        assert_eq!(
            parse_meta_diagnose_command("/meta-diagnose msg:coder-7").as_deref(),
            Some("msg:coder-7")
        );
    }

    #[test]
    fn resolves_last_failed_coder_message() {
        let messages = vec![
            msg(Role::User, "first"),
            msg(Role::Assistant, "ok"),
            msg(Role::Assistant, "[error] failed once"),
            msg(Role::Assistant, "FAILED (exit_code: 1)"),
        ];
        assert_eq!(resolve_tui_meta_target(&messages, "last-fail").unwrap(), 3);
    }

    #[test]
    fn resolves_explicit_meta_message_id() {
        let messages = vec![
            msg(Role::User, "first"),
            msg(Role::Assistant, "ok"),
            msg(Role::Assistant, "[error] failed once"),
        ];
        assert_eq!(
            resolve_tui_meta_target(&messages, "msg:coder-2").unwrap(),
            2
        );
        assert!(resolve_tui_meta_target(&messages, "msg:coder-1").is_err());
    }

    #[test]
    fn latest_next_action_target_picks_recent_failure() {
        let mut app = App::new(
            test_cfg(Mode::Jikkyo),
            test_cfg(Mode::Observer),
            test_cfg(Mode::Chat),
            None,
            Some(isolated_prefs_root()),
            false,
            "en".to_string(),
            None,
        );
        app.coder.messages = vec![
            msg(Role::Assistant, "all good"),
            msg(Role::Assistant, "[GOVERNOR BLOCK]\nMissing <think>"),
        ];
        let target = latest_tui_next_action_target(&app).expect("should detect stuck message");
        assert_eq!(target.0, 1);
        assert!(target.1.contains("tool_error"));
    }

    #[test]
    fn latest_next_action_target_ignores_non_failure_tail() {
        let mut app = App::new(
            test_cfg(Mode::Jikkyo),
            test_cfg(Mode::Observer),
            test_cfg(Mode::Chat),
            None,
            Some(isolated_prefs_root()),
            false,
            "en".to_string(),
            None,
        );
        app.coder.messages = vec![
            msg(Role::Assistant, "[error] failed once"),
            msg(Role::Assistant, "done"),
        ];
        assert!(latest_tui_next_action_target(&app).is_none());
    }

    #[test]
    fn meta_failure_packet_includes_intent_anchor_and_recent_tool_results() {
        let mut app = App::new(
            test_cfg(Mode::Jikkyo),
            test_cfg(Mode::Observer),
            test_cfg(Mode::Chat),
            None,
            Some(isolated_prefs_root()),
            false,
            "en".to_string(),
            None,
        );
        app.coder_intent_anchor = Some(intent::apply_intent_update(
            None,
            intent::normalize_intent_update(
                "Find where pane-scoped TUI preferences are serialized and restored. Do not edit anything.",
                None,
            ),
            "Find where pane-scoped TUI preferences are serialized and restored. Do not edit anything.",
        ));
        app.coder.messages = vec![
            msg(Role::User, "Find prefs storage"),
            msg(
                Role::Tool,
                "[src/tui/prefs.rs] (123 lines)\nfn save_tui_prefs(...)",
            ),
            msg(Role::Assistant, "[GOVERNOR BLOCK]\nMissing <think>"),
        ];

        let packet = build_tui_meta_failure_packet_for_selector(&app, "last-fail").expect("packet");
        assert_eq!(
            packet
                .get("intent_anchor")
                .and_then(|v| v.get("goal"))
                .and_then(|v| v.as_str()),
            Some("Find where pane-scoped TUI preferences are serialized and restored. Do not edit anything.")
        );
        assert!(packet
            .get("recent_tool_results")
            .and_then(|v| v.as_array())
            .is_some_and(|rows| !rows.is_empty()));
    }

    #[test]
    fn next_action_prompt_requests_json_schema() {
        let mut app = App::new(
            test_cfg(Mode::Jikkyo),
            test_cfg(Mode::Observer),
            test_cfg(Mode::Chat),
            None,
            Some(isolated_prefs_root()),
            false,
            "en".to_string(),
            None,
        );
        app.coder_intent_anchor = Some(intent::apply_intent_update(
            None,
            intent::normalize_intent_update(
                "Find where pane-scoped TUI preferences are serialized and restored. Do not edit anything.",
                None,
            ),
            "Find where pane-scoped TUI preferences are serialized and restored. Do not edit anything.",
        ));
        app.coder.messages = vec![
            msg(Role::User, "Find prefs storage"),
            msg(
                Role::Tool,
                "[src/tui/prefs.rs] (123 lines)\nfn save_tui_prefs(...)",
            ),
            msg(Role::Assistant, "[GOVERNOR BLOCK]\nMissing <think>"),
        ];

        let packet = build_tui_meta_failure_packet_for_selector(&app, "last-fail").expect("packet");
        let prompt = build_tui_next_action_prompt(&packet, "en", "missing_think");
        assert!(prompt.contains("Return JSON only."));
        assert!(prompt.contains("\"suggestions\""));
        assert!(prompt.contains("\"scores\""));
        assert!(prompt.contains("\"response_contract\""));
        assert!(prompt.contains("search | read | done | clarify | abandon_path"));
    }

    #[test]
    fn handle_observer_token_parses_structured_next_action_suggestion() {
        let mut app = App::new(
            test_cfg(Mode::Jikkyo),
            test_cfg(Mode::Observer),
            test_cfg(Mode::Chat),
            None,
            Some(isolated_prefs_root()),
            false,
            "en".to_string(),
            None,
        );
        app.observer_next_action_mode = true;
        app.observer.streaming = true;
        let mut msg = Message::new_streaming(Role::Assistant);
        msg.content = serde_json::json!({
            "summary": "Coder has enough evidence and should read the prefs file directly.",
            "primary_blocker": "missing_concrete_next_step",
            "scores": {
                "correctness": 0.84,
                "security": 0.62,
                "efficiency": 0.79,
                "readability": 0.81
            },
            "response_contract": {
                "required": true,
                "focus_axes": ["security"],
                "note": "If you do not take the read step now, explicitly override or defer it."
            },
            "suggestions": [{
                "kind": "read",
                "reason": "The latest search already narrowed the scope to prefs storage.",
                "confidence": 0.91,
                "suggested_tool": "read_file",
                "suggested_args": { "path": "src/tui/prefs.rs" },
                "based_on": ["intent_anchor", "recent_tool_results"]
            }],
            "quickest_check": "read_file(path=src/tui/prefs.rs)",
            "why_this_first": "It confirms the storage file without widening scope.",
            "fallback": "If prefs.rs is unrelated, inspect events.rs next."
        })
        .to_string();
        app.observer.messages.push(msg);

        handle_observer_token(StreamToken::Done, &mut app);

        assert!(!app.observer_next_action_mode);
        let stored = app
            .last_observer_suggestion
            .as_ref()
            .expect("structured suggestion should be stored");
        assert_eq!(stored.primary_blocker, "missing_concrete_next_step");
        assert!(app.pending_observer_hint.is_some());
        assert!(app.pending_observer_contract.is_some());
        let last = app.observer.messages.last().expect("observer response");
        assert!(last.content.contains("--- blocker ---"));
        assert!(last.content.contains("--- scores ---"));
        assert!(last.content.contains("read_file(path=src/tui/prefs.rs)"));
    }

    #[test]
    fn build_observer_suggestion_soft_hint_requires_high_confidence() {
        let low = suggestion::ObserverSuggestionEnvelope {
            summary: "Need a better next step.".to_string(),
            primary_blocker: "missing_concrete_next_step".to_string(),
            scores: suggestion::ObserverScoreVector::default(),
            response_contract: suggestion::ObserverResponseContract::default(),
            suggestions: vec![suggestion::ObserverSuggestion {
                kind: suggestion::ObserverSuggestionKind::Read,
                reason: "Read the prefs file next.".to_string(),
                confidence: 0.60,
                suggested_tool: Some("read_file".to_string()),
                suggested_args: serde_json::json!({"path": "src/tui/prefs.rs"}),
                based_on: vec!["recent_tool_results".to_string()],
            }],
            quickest_check: String::new(),
            why_this_first: String::new(),
            fallback: String::new(),
        };
        assert!(build_observer_suggestion_soft_hint(&low).is_none());

        let high = suggestion::ObserverSuggestionEnvelope {
            summary: "The next step is clear.".to_string(),
            primary_blocker: "missing_concrete_next_step".to_string(),
            scores: suggestion::ObserverScoreVector {
                correctness: 0.84,
                security: 0.62,
                efficiency: 0.79,
                readability: 0.81,
            },
            response_contract: suggestion::ObserverResponseContract {
                required: true,
                focus_axes: vec!["security".to_string()],
                note: "Explicitly accept, override, or defer.".to_string(),
            },
            suggestions: vec![suggestion::ObserverSuggestion {
                kind: suggestion::ObserverSuggestionKind::Read,
                reason: "Read the prefs file next.".to_string(),
                confidence: 0.92,
                suggested_tool: Some("read_file".to_string()),
                suggested_args: serde_json::json!({"path": "src/tui/prefs.rs"}),
                based_on: vec!["recent_tool_results".to_string()],
            }],
            quickest_check: String::new(),
            why_this_first: String::new(),
            fallback: String::new(),
        };
        let hint = build_observer_suggestion_soft_hint(&high).expect("high-confidence hint");
        assert!(hint.contains("<observer_advisory"));
        assert!(hint.contains("[Observer advisory"));
        assert!(hint.contains("read_file(path=src/tui/prefs.rs)"));
        let contract = build_observer_response_contract(&high).expect("contract");
        let contract_msg = build_observer_response_contract_message(&contract);
        assert!(contract_msg.contains("<observer_response_contract"));
        assert!(contract_msg.contains("[Observer response contract]"));
        assert!(contract_msg.contains("accept | override | defer"));
        assert!(contract_msg.contains("next <plan>"));
    }

    #[test]
    fn build_coder_request_messages_includes_observer_soft_hint_as_system_message() {
        let cfg = test_cfg(Mode::Jikkyo);
        let lang = language_instruction(Some("en"), &cfg.mode);
        let messages = build_coder_request_messages(
            lang,
            crate::tui::agent::RealizePreset::tui_default(),
            &[ChatMessage {
                role: "user".to_string(),
                content: "Continue.".to_string(),
            }],
            "[Intent Anchor]\ngoal: keep current scope".to_string(),
            Some(
                "[Observer advisory — soft hint only]\nPrefer read_file(path=src/tui/prefs.rs)."
                    .to_string(),
            ),
            Some("[Observer response contract]\ndecision: accept|override|defer".to_string()),
            &[],
            "Continue.",
            "",
        );

        assert!(messages.iter().any(|m| {
            m.role == "system" && m.content.contains("[Observer advisory — soft hint only]")
        }));
        assert!(messages
            .iter()
            .any(|m| { m.role == "system" && m.content.contains("[Observer response contract]") }));
    }

    #[test]
    fn handle_coder_token_records_observer_contract_accept() {
        let mut app = App::new(
            test_cfg(Mode::Jikkyo),
            test_cfg(Mode::Observer),
            test_cfg(Mode::Chat),
            None,
            Some(isolated_prefs_root()),
            false,
            "en".to_string(),
            None,
        );
        app.active_observer_contract = Some(suggestion::ObserverSuggestionEnvelope {
            summary: "Read the prefs file next.".to_string(),
            primary_blocker: "missing_concrete_next_step".to_string(),
            scores: suggestion::ObserverScoreVector {
                correctness: 0.84,
                security: 0.62,
                efficiency: 0.79,
                readability: 0.81,
            },
            response_contract: suggestion::ObserverResponseContract {
                required: true,
                focus_axes: vec!["security".to_string()],
                note: "Explicitly respond on this turn.".to_string(),
            },
            suggestions: vec![suggestion::ObserverSuggestion {
                kind: suggestion::ObserverSuggestionKind::Read,
                reason: "Read the prefs file next.".to_string(),
                confidence: 0.92,
                suggested_tool: Some("read_file".to_string()),
                suggested_args: serde_json::json!({"path": "src/tui/prefs.rs"}),
                based_on: vec!["recent_tool_results".to_string()],
            }],
            quickest_check: "read_file(path=src/tui/prefs.rs)".to_string(),
            why_this_first: "It confirms the prefs storage path.".to_string(),
            fallback: "Inspect events.rs next.".to_string(),
        });
        app.coder.streaming = true;
        let mut msg = Message::new_streaming(Role::Assistant);
        msg.content =
            "[Observer response]\ndecision: accept\nreason: The read matches the current scope."
                .to_string();
        app.coder.messages.push(msg);

        handle_coder_token(
            StreamToken::ToolCall(crate::streaming::ToolCallData {
                id: "tool-1".to_string(),
                name: "read_file".to_string(),
                arguments: serde_json::json!({"path": "src/tui/prefs.rs"}).to_string(),
            }),
            &mut app,
        );
        handle_coder_token(StreamToken::Done, &mut app);

        assert!(app.active_observer_contract.is_none());
        assert_eq!(
            app.last_observer_contract_outcome.as_deref(),
            Some("accept focus=security blocker=missing_concrete_next_step")
        );
        assert!(app.coder.messages.iter().any(|m| {
            matches!(m.role, Role::Tool)
                && m.content
                    .contains("accepted the latest observer suggestion")
        }));
    }

    #[test]
    fn slash_tab_switches_right_pane() {
        let mut app = App::new(
            test_cfg(Mode::Jikkyo),
            test_cfg(Mode::Observer),
            test_cfg(Mode::Chat),
            None,
            Some(isolated_prefs_root()),
            false,
            "en".to_string(),
            None,
        );
        assert!(handle_slash_command(
            "/tab observer",
            &mut app,
            PaneId::Coder
        ));
        assert_eq!(app.right_tab, RightTab::Observer);
        assert!(handle_slash_command("/tab chat", &mut app, PaneId::Coder));
        assert_eq!(app.right_tab, RightTab::Chat);
        assert!(handle_slash_command(
            "/tab promotions",
            &mut app,
            PaneId::Coder
        ));
        assert_eq!(app.right_tab, RightTab::Promotions);
        assert!(handle_slash_command("/tab merge", &mut app, PaneId::Coder));
        assert_eq!(app.right_tab, RightTab::MergeGate);
    }

    #[test]
    fn slash_keys_reports_env_hint() {
        let mut app = App::new(
            test_cfg(Mode::Jikkyo),
            test_cfg(Mode::Observer),
            test_cfg(Mode::Chat),
            None,
            Some(isolated_prefs_root()),
            false,
            "en".to_string(),
            None,
        );
        assert!(handle_slash_command("/keys", &mut app, PaneId::Coder));
        let last = app.coder.messages.last().expect("tool message");
        assert!(last.content.contains("OPENAI_API_KEY"));
        assert!(last.content.contains("observer"));
    }

    #[test]
    fn validate_pane_ready_requires_key() {
        let app = App::new(
            test_cfg(Mode::Jikkyo),
            test_cfg(Mode::Observer),
            test_cfg(Mode::Chat),
            None,
            Some(isolated_prefs_root()),
            false,
            "en".to_string(),
            None,
        );
        let msg = validate_pane_ready(&app, PaneId::Coder).expect("missing key should block");
        assert!(msg.contains("missing API key"));
    }

    #[test]
    fn slash_provider_accepts_gemini_preset() {
        let mut app = App::new(
            test_cfg(Mode::Jikkyo),
            test_cfg(Mode::Observer),
            test_cfg(Mode::Chat),
            None,
            Some(isolated_prefs_root()),
            false,
            "en".to_string(),
            None,
        );
        assert!(handle_slash_command(
            "/provider gemini",
            &mut app,
            PaneId::Coder
        ));
        assert_eq!(
            app.coder_cfg.base_url,
            "https://generativelanguage.googleapis.com/v1beta/openai"
        );
    }

    #[test]
    fn slash_provider_blocks_native_anthropic_for_coder() {
        let mut app = App::new(
            test_cfg(Mode::Jikkyo),
            test_cfg(Mode::Observer),
            test_cfg(Mode::Chat),
            None,
            Some(isolated_prefs_root()),
            false,
            "en".to_string(),
            None,
        );
        assert!(handle_slash_command(
            "/provider anthropic",
            &mut app,
            PaneId::Coder
        ));
        let last = app.coder.messages.last().expect("tool message");
        assert!(last.content.contains("tool-calling preset"));
    }
}

async fn send_observer_message(
    app: &mut App,
    tx: &mpsc::Sender<StreamToken>,
    override_text: Option<String>,
) {
    if app.observer.streaming {
        return;
    }
    let from_composer = override_text.is_none();
    let text = match override_text {
        Some(t) => t,
        None => {
            let t = app.observer.textarea.lines().join("\n").trim().to_string();
            if t.is_empty() {
                return;
            }
            if let Some(selector) = parse_meta_diagnose_command(&t) {
                if let Some(problem) = validate_pane_ready(app, PaneId::Observer) {
                    app.observer.push_tool(problem);
                    return;
                }
                app.observer.textarea = tui_textarea::TextArea::default();
                send_meta_diagnose(app, tx, &selector).await;
                return;
            }
            // Handle slash commands before sending to AI.
            if handle_slash_command(&t, app, PaneId::Observer) {
                app.observer.textarea = tui_textarea::TextArea::default();
                return;
            }
            t
        }
    };
    if let Some(problem) = validate_pane_ready(app, PaneId::Observer) {
        app.observer.push_tool(problem);
        return;
    }
    if from_composer {
        app.observer.textarea = tui_textarea::TextArea::default();
    }

    if let Some(handle) = app.observer_task.take() {
        handle.abort();
    }

    app.observer_meta_mode = false;
    app.observer_next_action_mode = false;
    app.observer.scroll = 0;
    app.ignore_observer_tokens = false;
    // Each new Observer send gets a single anti-loop retry budget.
    app.observer_loop_retry_budget = 1;
    app.observer_loop_pending = None;
    // Each new Observer send gets a single language rewrite budget.
    app.observer_lang_retry_budget = 1;
    app.observer_lang_pending = None;
    app.observer.push_user(text.clone());
    app.observer.streaming = true;
    app.observer
        .messages
        .push(Message::new_streaming(Role::Assistant));

    let history = app.observer.chat_history();
    let coder_history = app.coder.chat_history();
    let cfg = app.observer_cfg.clone();
    let tool_root = app.tool_root.clone();
    let checkpoint = app.last_git_checkpoint.clone();

    // Build Observer system prompt. Persona is intentionally excluded: the Observer mode
    // has its own strict critique tone ("NO padding, ruthlessly honest") that must not be
    // softened by a cheerful/novelist/etc persona assigned to the Coder pane.
    let lang = language_instruction(Some(&app.lang), &cfg.mode);
    let obs_mode_prompt = mode_prompt(&cfg.mode);
    let obs_system = format!("{obs_mode_prompt}\n\n[Language]\n{lang}");
    let obs_system = obs_system.trim_end().to_string();
    // Inject recent Coder history. Use more messages and wider window than before so
    // Observer can see the full arc of what the Coder did, not just a narrow snippet.
    let coder_context = if !coder_history.is_empty() {
        let snippet = coder_history
            .iter()
            .rev()
            .take(10)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .map(|m| {
                format!(
                    "[{}]: {}",
                    m.role,
                    m.content.chars().take(800).collect::<String>()
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        format!("\n\n[Recent Coder activity — last 10 turns]\n{snippet}")
    } else {
        String::new()
    };

    let tool_context = build_recent_tool_outputs(&app.coder.messages);

    let system_base = format!("{obs_system}{coder_context}{tool_context}")
        .trim_end()
        .to_string();
    // Exclude the last entry in history (current user message, already pushed to pane)
    // to avoid sending a duplicate user message, matching the pattern in send_coder_message.
    let hist_len = history.len();
    let history_prefix: Vec<ChatMessage> = history
        .iter()
        .take(hist_len.saturating_sub(1))
        .cloned()
        .collect();

    let tx = tx.clone();
    let handle = tokio::spawn(async move {
        use crate::config::ProviderKind;
        use crate::streaming::{stream_anthropic, stream_openai_compat};
        let client = reqwest::Client::new();

        let mut system = system_base;

        // Add project context and AGENTS.md instructions (when tool_root is set).
        if let Some(ref root) = tool_root {
            if let Some(ctx) = crate::project::ProjectContext::scan(root).await {
                let ctx_text = ctx.to_context_text();
                if !ctx_text.trim().is_empty() {
                    system.push_str("\n\n");
                    system.push_str(ctx_text.trim_end());
                }
                if let Some(a) = ctx.agents_md.as_deref() {
                    if !a.trim().is_empty() {
                        system.push_str("\n\n[Project Instructions]\n");
                        system.push_str(a.trim_end());
                    }
                }
            }
        }

        // Add git diff payload (status + stat + patch) so Observer can quote real code.
        let mut user_text = text;
        if let Some(ref root) = tool_root {
            if let Some(payload) =
                build_git_diff_payload(root, checkpoint.as_deref(), OBS_GIT_DIFF_MAX_CHARS).await
            {
                user_text.push_str("\n\n[git diff payload]\n");
                user_text.push_str(payload.trim_end());
            }
        }

        let mut messages = vec![ChatMessage {
            role: "system".to_string(),
            content: system,
        }];
        messages.extend(history_prefix);
        messages.push(ChatMessage {
            role: "user".to_string(),
            content: user_text,
        });

        let result = match cfg.provider {
            ProviderKind::Anthropic => stream_anthropic(&client, &cfg, &messages, tx.clone()).await,
            ProviderKind::Hf => {
                let provider = providers::build_provider(client.clone(), &cfg);
                let req = ChatRequest {
                    messages: messages.clone(),
                    temperature: Some(cfg.temperature),
                    max_tokens: Some(cfg.max_tokens),
                    metadata: None,
                };
                match provider.chat(&req).await {
                    Ok(resp) => {
                        fake_stream_text(&tx, &resp.content).await;
                        Ok(())
                    }
                    Err(e) => Err(e),
                }
            }
            _ => stream_openai_compat(&client, &cfg, &messages, None, tx.clone()).await,
        };
        if let Err(e) = result {
            let _ = tx.send(StreamToken::Error(e.to_string())).await;
        }
    });
    app.observer_task = Some(handle);
}

const OBS_GIT_DIFF_MAX_CHARS: usize = 24_000;

fn truncate_middle(s: &str, max_chars: usize) -> String {
    let s = s.trim_end();
    if max_chars < 20 {
        return s.chars().take(max_chars).collect();
    }
    let total = s.chars().count();
    if total <= max_chars {
        return s.to_string();
    }

    let head_len = max_chars / 2;
    let tail_len = max_chars.saturating_sub(head_len);
    let head: String = s.chars().take(head_len).collect();
    let tail: String = s
        .chars()
        .rev()
        .take(tail_len)
        .collect::<Vec<char>>()
        .into_iter()
        .rev()
        .collect();
    format!("{head}\n[…truncated — middle removed, total {total} chars]\n{tail}")
}

async fn git_cmd_output(root: &str, args: &[&str]) -> (String, String, i32) {
    let fut = tokio::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output();

    match tokio::time::timeout(std::time::Duration::from_secs(5), fut).await {
        Ok(Ok(out)) => {
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
            let exit = out.status.code().unwrap_or(-1);
            (stdout, stderr, exit)
        }
        Ok(Err(e)) => (String::new(), e.to_string(), -1),
        Err(_) => (
            String::new(),
            "git command timed out after 5s".to_string(),
            -1,
        ),
    }
}

async fn build_git_diff_payload(
    root: &str,
    base: Option<&str>,
    max_chars: usize,
) -> Option<String> {
    let base = base.map(|s| s.trim()).filter(|s| !s.is_empty());

    let (status_out, _status_err, status_exit) =
        git_cmd_output(root, &["status", "--porcelain=v1"]).await;
    if status_exit != 0 {
        return None; // not a git repo or git unavailable
    }

    let mut out = String::new();
    out.push_str(&format!("[repo]\nroot: {root}\n"));
    if let Some(b) = base {
        out.push_str(&format!("base: {b}\n"));
    }
    out.push_str("\n[git status --porcelain=v1]\n");
    out.push_str(status_out.trim_end());
    out.push('\n');

    let mut stat_args: Vec<&str> = vec!["diff", "--no-color", "--stat"];
    if let Some(b) = base {
        stat_args.push(b);
    }
    let (stat_out, stat_err, stat_exit) = git_cmd_output(root, &stat_args).await;
    if stat_exit == 0 && !stat_out.trim().is_empty() {
        out.push_str("\n[git diff --stat]\n");
        out.push_str(stat_out.trim_end());
        out.push('\n');
    } else if stat_exit != 0 && !stat_err.trim().is_empty() {
        out.push_str("\n[git diff --stat ERROR]\n");
        out.push_str(stat_err.trim_end());
        out.push('\n');
    }

    let mut diff_args: Vec<&str> = vec!["diff", "--no-color"];
    if let Some(b) = base {
        diff_args.push(b);
    }
    let (diff_out, diff_err, diff_exit) = git_cmd_output(root, &diff_args).await;
    if diff_exit == 0 && !diff_out.trim().is_empty() {
        out.push_str("\n[git diff]\n");
        out.push_str(&truncate_middle(&diff_out, max_chars));
        out.push('\n');
    } else if diff_exit != 0 && !diff_err.trim().is_empty() {
        out.push_str("\n[git diff ERROR]\n");
        out.push_str(diff_err.trim_end());
        out.push('\n');
    }

    Some(out.trim_end().to_string())
}

// ── Auto-observe ──────────────────────────────────────────────────────────────

async fn send_chat_message(
    app: &mut App,
    tx: &mpsc::Sender<StreamToken>,
    internal_tx: &mpsc::Sender<AppEvent>,
) {
    let text = app.chat.textarea.lines().join("\n").trim().to_string();
    if text.is_empty() || app.chat.streaming {
        return;
    }

    // Handle slash commands before sending to AI.
    if handle_slash_command(&text, app, PaneId::Chat) {
        app.chat.textarea = tui_textarea::TextArea::default();
        return;
    }
    if let Some(problem) = validate_pane_ready(app, PaneId::Chat) {
        app.chat.push_tool(problem);
        return;
    }

    // Abort any previous chat task before starting a new one.
    if let Some(handle) = app.chat_task.take() {
        handle.abort();
    }

    // Reset state for the new send.
    app.chat.textarea = tui_textarea::TextArea::default();
    app.chat.scroll = 0;
    app.ignore_chat_tokens = false;

    app.chat.push_user(text.clone());
    app.chat.streaming = true;
    app.chat
        .messages
        .push(Message::new_streaming(Role::Assistant));

    // Start background task planning (TaskRouter) in parallel.
    spawn_task_planner(app, internal_tx);

    let history = app.chat.chat_history();
    let cfg = app.chat_cfg.clone();
    let persona_prompt = resolve_persona(&cfg.persona)
        .map(|p| p.prompt)
        .unwrap_or("");
    let lang = language_instruction(Some(&app.lang), &cfg.mode);
    let mode = mode_prompt(&cfg.mode);

    let mut system = mode.to_string();
    if !lang.trim().is_empty() {
        system.push_str("\n\n[Language]\n");
        system.push_str(lang);
    }
    if !persona_prompt.trim().is_empty() {
        system.push_str("\n\n[Persona]\n");
        system.push_str(persona_prompt);
    }

    let mut messages = vec![ChatMessage {
        role: "system".to_string(),
        content: system,
    }];
    let hist_len = history.len();
    for m in history.iter().take(hist_len.saturating_sub(1)) {
        messages.push(m.clone());
    }
    messages.push(ChatMessage {
        role: "user".to_string(),
        content: text,
    });

    let tx = tx.clone();
    let handle = tokio::spawn(async move {
        use crate::config::ProviderKind;
        use crate::streaming::{stream_anthropic, stream_openai_compat};
        let client = reqwest::Client::new();
        let result = match cfg.provider {
            ProviderKind::Anthropic => stream_anthropic(&client, &cfg, &messages, tx.clone()).await,
            ProviderKind::Hf => {
                let provider = providers::build_provider(client.clone(), &cfg);
                let req = ChatRequest {
                    messages: messages.clone(),
                    temperature: Some(cfg.temperature),
                    max_tokens: Some(cfg.max_tokens),
                    metadata: None,
                };
                match provider.chat(&req).await {
                    Ok(resp) => {
                        fake_stream_text(&tx, &resp.content).await;
                        Ok(())
                    }
                    Err(e) => Err(e),
                }
            }
            _ => stream_openai_compat(&client, &cfg, &messages, None, tx.clone()).await,
        };
        if let Err(e) = result {
            let _ = tx.send(StreamToken::Error(e.to_string())).await;
        }
    });
    app.chat_task = Some(handle);
}

fn spawn_task_planner(app: &mut App, internal_tx: &mpsc::Sender<AppEvent>) {
    if app.planning_tasks {
        return;
    }
    app.planning_tasks = true;

    let cfg = app.chat_cfg.clone();
    let lang = app.lang.clone();
    let chat_hist = app.chat.chat_history();
    let coder_hist = app.coder.chat_history();
    let observer_hist = app.observer.chat_history();
    let tx = internal_tx.clone();

    tokio::spawn(async move {
        let client = reqwest::Client::new();
        let provider = providers::build_provider(client, &cfg);

        let lang_instr = language_instruction(Some(&lang), &cfg.mode);
        let system = format!(
            "You are TaskRouter.\n\
Return STRICT JSON only (no markdown, no commentary).\n\
Schema: {{\"tasks\":[{{\"target\":\"coder|observer\",\"title\":\"...\",\"body\":\"...\",\"phase\":\"core|feature|polish|any\",\"priority\":0..100}}]}}\n\
Rules:\n\
- Prefer concrete next actions (files/commands) over abstract advice.\n\
- If there is no actionable work, return {{\"tasks\":[]}}.\n\n\
[Language]\n{lang_instr}"
        );

        let mut messages: Vec<ChatMessage> = Vec::new();
        messages.push(ChatMessage {
            role: "system".to_string(),
            content: system,
        });

        fn fmt_recent(label: &str, hist: &[ChatMessage], take: usize) -> String {
            let recent = hist
                .iter()
                .rev()
                .take(take)
                .cloned()
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<Vec<_>>();
            let joined = recent
                .iter()
                .map(|m| format!("[{}] {}", m.role, m.content))
                .collect::<Vec<_>>()
                .join("\n");
            format!("{label}:\n{joined}")
        }

        let state = format!(
            "{}\n\n{}\n\n{}",
            fmt_recent("Recent chat", &chat_hist, 6),
            fmt_recent("Recent coder", &coder_hist, 6),
            fmt_recent("Recent observer", &observer_hist, 3),
        );
        messages.push(ChatMessage {
            role: "user".to_string(),
            content: state,
        });

        let req = ChatRequest {
            messages,
            temperature: Some(0.2),
            max_tokens: Some(512),
            metadata: Some(json!({"response_format": {"type": "json_object"}})),
        };

        let resp = match provider.chat(&req).await {
            Ok(r) => r,
            Err(e) => {
                let _ = tx.send(AppEvent::TaskPlanError(e.to_string())).await;
                return;
            }
        };

        let raw = resp.content;
        let json_str = extract_first_json_object(&raw).unwrap_or(raw);
        let parsed: PlannedTasks = match serde_json::from_str(&json_str) {
            Ok(v) => v,
            Err(e) => {
                let _ = tx
                    .send(AppEvent::TaskPlanError(format!("invalid JSON: {e}")))
                    .await;
                return;
            }
        };

        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();

        let mut out: Vec<Task> = Vec::new();
        for (i, t) in parsed.tasks.into_iter().enumerate() {
            let target = match t.target.to_ascii_lowercase().as_str() {
                "observer" | "obs" => TaskTarget::Observer,
                _ => TaskTarget::Coder,
            };
            let phase = match t
                .phase
                .unwrap_or_else(|| "any".to_string())
                .to_ascii_lowercase()
                .as_str()
            {
                "core" => TaskPhase::Core,
                "feature" => TaskPhase::Feature,
                "polish" => TaskPhase::Polish,
                _ => TaskPhase::Any,
            };
            let prio = t.priority.unwrap_or(50).min(100);
            out.push(Task {
                id: format!("t{now_ms}-{i}"),
                target,
                title: t.title,
                body: t.body,
                phase,
                priority: prio,
                done: false,
            });
        }

        let _ = tx.send(AppEvent::TasksPlanned(out)).await;
    });
}

fn extract_first_json_object(s: &str) -> Option<String> {
    let start = s.find('{')?;
    let end = s.rfind('}')?;
    if end <= start {
        return None;
    }
    Some(s[start..=end].to_string())
}

async fn dispatch_selected_task(
    app: &mut App,
    coder_tx: &mpsc::Sender<StreamToken>,
    observer_tx: &mpsc::Sender<StreamToken>,
) {
    let Some(t) = app.tasks.get(app.tasks_cursor).cloned() else {
        return;
    };
    let msg = format!(
        "[TASK]\nid: {}\nphase: {:?}\npriority: {}\ntitle: {}\n\n{}",
        t.id, t.phase, t.priority, t.title, t.body
    );
    match t.target {
        TaskTarget::Coder => send_coder_with_text(app, coder_tx, msg).await,
        TaskTarget::Observer => send_observer_message(app, observer_tx, Some(msg)).await,
    }
}

async fn maybe_auto_next_action_assist(app: &mut App, observer_tx: &mpsc::Sender<StreamToken>) {
    if app.observer.streaming || app.coder.streaming {
        return;
    }
    let Some((idx, reason_hint)) = latest_tui_next_action_target(app) else {
        return;
    };
    let sig = format!("coder-{idx}:{reason_hint}");
    if app.last_auto_next_action_sig.as_deref() == Some(sig.as_str()) {
        return;
    }
    app.last_auto_next_action_sig = Some(sig);
    app.last_auto_obs_idx = Some(idx);
    let selector = format!("msg:coder-{idx}");
    send_next_action_assist(app, observer_tx, &selector, &reason_hint).await;
}

async fn maybe_auto_observe(app: &mut App, observer_tx: &mpsc::Sender<StreamToken>) {
    if let Some((idx, content)) = app.auto_observe_trigger() {
        app.last_auto_obs_idx = Some(idx);
        let snippet = content.chars().take(2000).collect::<String>();
        let prompt = match app.lang.as_str() {
            "fr" => format!(
                "[AUTO-OBSERVE] Le Coder vient de produire une nouvelle sortie. Fais une critique en mode commentaire live.\n\nDernière sortie du Coder:\n{snippet}"
            ),
            "en" => format!(
                "[AUTO-OBSERVE] The Coder produced new output. Commentate and critique live.\n\nLatest Coder output:\n{snippet}"
            ),
            _ => format!(
                "[AUTO-OBSERVE] コーダーが新しいアウトプットを生成した。実況しながら批評せよ。\n\n最新のコーダー出力:\n{snippet}"
            ),
        };
        send_observer_message(app, observer_tx, Some(prompt)).await;
    }
}

async fn maybe_observer_lang_retry(app: &mut App, observer_tx: &mpsc::Sender<StreamToken>) {
    let Some(expected) = app.observer_lang_pending.take() else {
        return;
    };
    if app.observer.streaming {
        return;
    }

    if let Some(problem) = validate_pane_ready(app, PaneId::Observer) {
        app.observer.push_tool(problem);
        return;
    }

    // Best-effort: abort the previous task handle (it should already be complete).
    if let Some(handle) = app.observer_task.take() {
        handle.abort();
    }

    let idx_opt = app.observer.messages.iter().rposition(|m| {
        matches!(m.role, Role::Assistant) && m.complete && !m.content.trim().is_empty()
    });
    let Some(idx) = idx_opt else {
        return;
    };
    let original = app
        .observer
        .messages
        .get(idx)
        .map(|m| m.content.clone())
        .unwrap_or_default();
    if original.trim().is_empty() {
        return;
    }

    // Overwrite the last assistant message if it's the tail; otherwise append a new streaming assistant.
    if idx + 1 == app.observer.messages.len() {
        if let Some(m) = app.observer.messages.get_mut(idx) {
            m.content.clear();
            m.complete = false;
        }
    } else {
        app.observer
            .messages
            .push(Message::new_streaming(Role::Assistant));
    }

    app.observer.scroll = 0;
    app.ignore_observer_tokens = false;
    app.observer.streaming = true;

    let system_fix = if expected.eq_ignore_ascii_case("fr") {
        "You are a strict translator.\n\
Rewrite the provided text into French ONLY.\n\
Do not add new content.\n\
Output ONLY the rewritten text.\n\
Keep proposals block keys in English (title/to_coder/severity/score/phase/impact/cost)."
    } else {
        // Default: Japanese.
        "You are a strict translator.\n\
Rewrite the provided text into Japanese ONLY.\n\
Do not add new content.\n\
Output ONLY the rewritten text.\n\
Keep proposals block keys in English (title/to_coder/severity/score/phase/impact/cost)."
    };
    let user_fix = format!("TEXT:\n```text\n{}\n```", original.trim_end());

    let cfg = app.observer_cfg.clone();
    let messages = vec![
        ChatMessage {
            role: "system".to_string(),
            content: system_fix.to_string(),
        },
        ChatMessage {
            role: "user".to_string(),
            content: user_fix,
        },
    ];

    let tx = observer_tx.clone();
    let handle = tokio::spawn(async move {
        let client = reqwest::Client::new();
        let provider = crate::providers::build_provider(client, &cfg);
        let req = crate::types::ChatRequest {
            messages,
            temperature: Some(0.0),
            max_tokens: Some(cfg.max_tokens.min(1024)),
            metadata: None,
        };
        match provider.chat(&req).await {
            Ok(resp) => {
                let mut chunk = String::new();
                let mut n = 0usize;
                const CHUNK_CHARS: usize = 28;
                for ch in resp.content.chars() {
                    chunk.push(ch);
                    n += 1;
                    if n >= CHUNK_CHARS {
                        let _ = tx.send(StreamToken::Delta(chunk.clone())).await;
                        chunk.clear();
                        n = 0;
                    }
                }
                if !chunk.is_empty() {
                    let _ = tx.send(StreamToken::Delta(chunk)).await;
                }
                let _ = tx.send(StreamToken::Done).await;
            }
            Err(e) => {
                let _ = tx.send(StreamToken::Error(e.to_string())).await;
            }
        }
    });
    app.observer_task = Some(handle);
}

async fn maybe_observer_loop_retry(app: &mut App, observer_tx: &mpsc::Sender<StreamToken>) {
    let Some(sim) = app.observer_loop_pending.take() else {
        return;
    };
    if app.observer.streaming {
        return;
    }

    if let Some(problem) = validate_pane_ready(app, PaneId::Observer) {
        app.observer.push_tool(problem);
        return;
    }

    // Best-effort: abort the previous task handle (it should already be complete).
    if let Some(handle) = app.observer_task.take() {
        handle.abort();
    }

    // Build the retry request using the already-visible history (including the repeated reply),
    // but do not add a visible user message for the loop fix.
    let history = app.observer.chat_history();
    let coder_history = app.coder.chat_history();
    let cfg = app.observer_cfg.clone();

    let lang = language_instruction(Some(&app.lang), &cfg.mode);
    let obs_mode_prompt = mode_prompt(&cfg.mode);
    let obs_system = format!("{obs_mode_prompt}\n\n[Language]\n{lang}");
    let obs_system = obs_system.trim_end().to_string();
    let coder_context = if !coder_history.is_empty() {
        let snippet = coder_history
            .iter()
            .rev()
            .take(10)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .map(|m| {
                format!(
                    "[{}]: {}",
                    m.role,
                    m.content.chars().take(800).collect::<String>()
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        format!("\n\n[Recent Coder activity — last 10 turns]\n{snippet}")
    } else {
        String::new()
    };

    let tool_context = build_recent_tool_outputs(&app.coder.messages);
    let system = format!("{obs_system}{coder_context}{tool_context}")
        .trim_end()
        .to_string();
    let mut messages = vec![ChatMessage {
        role: "system".to_string(),
        content: system,
    }];
    for m in &history {
        messages.push(m.clone());
    }

    let loop_fix = match app.lang.as_str() {
        "fr" => format!(
            "CORRECTION BOUCLE: Ton dernier message se répète (sim={}%). Fais une critique NOUVELLE basée UNIQUEMENT sur les informations NOUVELLES depuis le message précédent. Ne répète pas les mêmes proposals. S'il n'y a rien de nouveau, réponds exactement: [Observer] No new critique. Loop detected.",
            (sim * 100.0).round() as i64
        ),
        "en" => format!(
            "LOOP FIX: Your last message repeated the same critique (sim={}%). Write a NEW critique ONLY based on NEW information since your previous message. Do not restate the same proposals. If there is no new signal, reply exactly: [Observer] No new critique. Loop detected.",
            (sim * 100.0).round() as i64
        ),
        _ => format!(
            "LOOP FIX: 直前の批評がほぼ同一です (sim={}%)。前回から増えた情報に基づく「新しい」批評だけを書いてください。同じ提案の焼き直しは禁止。新しい指摘が無い場合は、次の1行だけを厳密に出力: [Observer] No new critique. Loop detected.",
            (sim * 100.0).round() as i64
        ),
    };
    messages.push(ChatMessage {
        role: "user".to_string(),
        content: loop_fix,
    });

    // Overwrite the last assistant message if it's the tail; otherwise append a new streaming assistant.
    if let Some(idx) = app
        .observer
        .messages
        .iter()
        .rposition(|m| matches!(m.role, Role::Assistant) && m.complete)
    {
        if idx + 1 == app.observer.messages.len() {
            if let Some(m) = app.observer.messages.get_mut(idx) {
                m.content.clear();
                m.complete = false;
            }
        } else {
            app.observer
                .messages
                .push(Message::new_streaming(Role::Assistant));
        }
    } else {
        app.observer
            .messages
            .push(Message::new_streaming(Role::Assistant));
    }

    app.observer.scroll = 0;
    app.ignore_observer_tokens = false;
    app.observer.streaming = true;

    let tx = observer_tx.clone();
    let handle = tokio::spawn(async move {
        use crate::config::ProviderKind;
        use crate::streaming::{stream_anthropic, stream_openai_compat};
        let client = reqwest::Client::new();
        let result = match cfg.provider {
            ProviderKind::Anthropic => stream_anthropic(&client, &cfg, &messages, tx.clone()).await,
            ProviderKind::Hf => {
                let provider = providers::build_provider(client.clone(), &cfg);
                let req = ChatRequest {
                    messages: messages.clone(),
                    temperature: Some(cfg.temperature),
                    max_tokens: Some(cfg.max_tokens),
                    metadata: None,
                };
                match provider.chat(&req).await {
                    Ok(resp) => {
                        fake_stream_text(&tx, &resp.content).await;
                        Ok(())
                    }
                    Err(e) => Err(e),
                }
            }
            _ => stream_openai_compat(&client, &cfg, &messages, None, tx.clone()).await,
        };
        if let Err(e) = result {
            let _ = tx.send(StreamToken::Error(e.to_string())).await;
        }
    });
    app.observer_task = Some(handle);
}
