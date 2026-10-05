use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::config::{PartialConfig, ProviderKind, RunConfig};
use crate::modes::parse_mode;
use crate::personas::resolve_persona;

use super::agent::RealizePreset;
use super::app::{App, RightTab};

const PREFS_REL_PATH: &str = ".spiral-coder/tui_prefs.json";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PanePrefs {
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub persona: Option<String>,
    #[serde(default)]
    pub temperature: Option<f64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TuiPrefs {
    #[serde(default)]
    pub coder_realize_preset: Option<String>,
    /// Legacy flat field kept for backward-compatible loads.
    #[serde(default)]
    pub coder_persona: Option<String>,
    #[serde(default)]
    pub ui_lang: Option<String>,
    #[serde(default)]
    pub coder: PanePrefs,
    #[serde(default)]
    pub observer: PanePrefs,
    #[serde(default)]
    pub chat: PanePrefs,
    #[serde(default)]
    pub auto_observe: Option<bool>,
    #[serde(default)]
    pub auto_fix_mode: Option<bool>,
    #[serde(default)]
    pub ui_right_tab: Option<String>,
}

impl TuiPrefs {
    pub fn coder_realize(&self) -> Option<RealizePreset> {
        self.coder_realize_preset
            .as_deref()
            .and_then(|raw| raw.parse::<RealizePreset>().ok())
    }

    pub fn set_coder_realize(&mut self, preset: RealizePreset) {
        self.coder_realize_preset = Some(preset.label().to_string());
    }

    pub fn set_coder_persona(&mut self, persona: impl Into<String>) {
        let persona = persona.into();
        self.coder_persona = Some(persona.clone());
        self.coder.persona = Some(persona);
    }

    pub fn set_ui_lang(&mut self, lang: impl Into<String>) {
        self.ui_lang = Some(lang.into());
    }

    pub fn effective_coder_persona(&self) -> Option<&str> {
        self.coder
            .persona
            .as_deref()
            .or(self.coder_persona.as_deref())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PrefPane {
    Coder,
    Observer,
    Chat,
}

impl TuiPrefs {
    fn pane(&self, pane: PrefPane) -> &PanePrefs {
        match pane {
            PrefPane::Coder => &self.coder,
            PrefPane::Observer => &self.observer,
            PrefPane::Chat => &self.chat,
        }
    }

    fn pane_mut(&mut self, pane: PrefPane) -> &mut PanePrefs {
        match pane {
            PrefPane::Coder => &mut self.coder,
            PrefPane::Observer => &mut self.observer,
            PrefPane::Chat => &mut self.chat,
        }
    }
}

fn snapshot_run_config(cfg: &RunConfig) -> PanePrefs {
    PanePrefs {
        provider: Some(cfg.provider.to_string()),
        mode: Some(cfg.mode.label().to_string()),
        model: Some(cfg.model.clone()),
        base_url: Some(cfg.base_url.clone()),
        persona: Some(cfg.persona.clone()),
        temperature: Some(cfg.temperature),
    }
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

fn parse_right_tab(raw: &str) -> Option<RightTab> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "observer" | "observe" => Some(RightTab::Observer),
        "chat" => Some(RightTab::Chat),
        "tasks" | "task" => Some(RightTab::Tasks),
        "promotions" | "promotion" | "promote" => Some(RightTab::Promotions),
        "merge-gate" | "merge_gate" | "merge" | "gate" => Some(RightTab::MergeGate),
        _ => None,
    }
}

fn apply_pane_prefs(
    pane: PrefPane,
    saved: &PanePrefs,
    cfg: &mut RunConfig,
    legacy_persona: Option<&str>,
) {
    let original = cfg.clone();
    if let Some(raw) = saved.provider.as_deref() {
        if let Ok(provider) = raw.parse::<ProviderKind>() {
            let allowed = match pane {
                PrefPane::Coder => {
                    matches!(
                        provider,
                        ProviderKind::OpenAiCompatible | ProviderKind::Mistral
                    )
                }
                PrefPane::Observer | PrefPane::Chat => true,
            };
            if allowed {
                cfg.provider = provider;
            }
        }
    }
    if let Some(raw) = saved.mode.as_deref() {
        if let Some(mode) = parse_mode(raw) {
            cfg.mode = mode;
        }
    }
    if let Some(model) = saved.model.as_deref() {
        let trimmed = model.trim();
        if !trimmed.is_empty() {
            cfg.model = trimmed.to_string();
        }
    }
    if let Some(base_url) = saved.base_url.as_deref() {
        cfg.base_url = base_url.trim().trim_end_matches('/').to_string();
    }
    if let Some(temp) = saved.temperature {
        cfg.temperature = temp.clamp(0.0, 2.0);
    }
    let persona = saved.persona.as_deref().or(legacy_persona);
    if let Some(persona) = persona {
        if resolve_persona(persona).is_ok() {
            cfg.persona = persona.to_string();
        }
    }
    if cfg.provider != original.provider || cfg.base_url != original.base_url {
        // Saved settings never carry credentials. A different target must resolve
        // its own environment key instead of inheriting the launch-time key.
        let provider_changed = cfg.provider != original.provider;
        let model = if provider_changed && saved.model.is_none() {
            String::new()
        } else {
            cfg.model.clone()
        };
        let base_url = if provider_changed && saved.base_url.is_none() {
            String::new()
        } else {
            cfg.base_url.clone()
        };
        let resolved = PartialConfig {
            vibe: matches!(cfg.mode, crate::modes::Mode::Vibe),
            provider: Some(cfg.provider.clone()),
            model: Some(model.clone()),
            chat_model: Some(model.clone()),
            code_model: Some(model),
            api_key: None,
            base_url: Some(base_url),
            mode: Some(cfg.mode.clone()),
            persona: Some(cfg.persona.clone()),
            temperature: Some(cfg.temperature),
            max_tokens: Some(cfg.max_tokens),
            timeout_seconds: Some(cfg.timeout_seconds),
            hf_device: Some(cfg.hf_device.clone()),
            hf_local_only: Some(cfg.hf_local_only),
        }
        .resolve_for_interactive_setup();
        *cfg = resolved.unwrap_or(original);
    }
}

pub fn snapshot_app_prefs(app: &App) -> TuiPrefs {
    let mut out = TuiPrefs::default();
    out.set_coder_realize(app.coder_realize_preset);
    out.set_ui_lang(app.lang.clone());
    out.set_coder_persona(app.coder_cfg.persona.clone());
    *out.pane_mut(PrefPane::Coder) = snapshot_run_config(&app.coder_cfg);
    *out.pane_mut(PrefPane::Observer) = snapshot_run_config(&app.observer_cfg);
    *out.pane_mut(PrefPane::Chat) = snapshot_run_config(&app.chat_cfg);
    out.auto_observe = Some(app.auto_observe);
    out.auto_fix_mode = Some(app.auto_fix_mode);
    out.ui_right_tab = Some(right_tab_label(app.right_tab).to_string());
    out
}

pub fn apply_prefs_to_app(app: &mut App, prefs: &TuiPrefs) {
    if let Some(lang) = prefs.ui_lang.as_deref() {
        let v = lang.trim().to_ascii_lowercase();
        if matches!(v.as_str(), "ja" | "en" | "fr") {
            app.lang = v;
        }
    }
    if let Some(preset) = prefs.coder_realize() {
        app.coder_realize_preset = preset;
    }
    if let Some(auto_observe) = prefs.auto_observe {
        app.auto_observe = auto_observe;
    }
    if let Some(auto_fix_mode) = prefs.auto_fix_mode {
        app.auto_fix_mode = auto_fix_mode;
    }
    if let Some(tab) = prefs.ui_right_tab.as_deref().and_then(parse_right_tab) {
        app.right_tab = tab;
    }
    apply_pane_prefs(
        PrefPane::Coder,
        prefs.pane(PrefPane::Coder),
        &mut app.coder_cfg,
        prefs.effective_coder_persona(),
    );
    apply_pane_prefs(
        PrefPane::Observer,
        prefs.pane(PrefPane::Observer),
        &mut app.observer_cfg,
        None,
    );
    apply_pane_prefs(
        PrefPane::Chat,
        prefs.pane(PrefPane::Chat),
        &mut app.chat_cfg,
        None,
    );
}

pub fn default_prefs_root() -> Option<String> {
    std::env::current_dir()
        .ok()
        .map(|p| p.to_string_lossy().into_owned())
}

pub fn prefs_path(root: Option<&str>) -> Option<PathBuf> {
    let base = root
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(|| default_prefs_root().map(PathBuf::from))?;
    Some(base.join(PREFS_REL_PATH))
}

pub fn load_prefs(root: Option<&str>) -> Result<TuiPrefs> {
    let Some(path) = prefs_path(root) else {
        return Ok(TuiPrefs::default());
    };
    if !path.is_file() {
        return Ok(TuiPrefs::default());
    }
    let raw = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read prefs file: {}", path.display()))?;
    let prefs = serde_json::from_str::<TuiPrefs>(&raw)
        .with_context(|| format!("failed to parse prefs file: {}", path.display()))?;
    Ok(prefs)
}

pub fn save_prefs(root: Option<&str>, prefs: &TuiPrefs) -> Result<PathBuf> {
    let Some(path) = prefs_path(root) else {
        anyhow::bail!("no prefs root available");
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create prefs dir: {}", parent.display()))?;
    }
    let json = serde_json::to_string_pretty(prefs)?;
    std::fs::write(&path, json)
        .with_context(|| format!("failed to write prefs file: {}", path.display()))?;
    Ok(path)
}

pub fn root_label(root: Option<&str>) -> String {
    prefs_path(root)
        .as_deref()
        .and_then(Path::parent)
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "(no prefs root)".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restored_provider_and_endpoint_do_not_inherit_another_explicit_key() {
        const CHILD: &str = "SPIRAL_CODER_PREFS_CREDENTIAL_TEST_CHILD";
        if std::env::var_os(CHILD).is_none() {
            let result = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "tui::prefs::tests::restored_provider_and_endpoint_do_not_inherit_another_explicit_key", "--nocapture"])
                .env(CHILD, "1")
                .env("ANTHROPIC_API_KEY", "anthropic-target-placeholder")
                .env("GEMINI_API_KEY", "gemini-target-placeholder")
                .env_remove("SPIRAL_CODER_API_KEY")
                .env_remove("MISTRAL_API_KEY")
                .output().unwrap();
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            return;
        }
        let original = crate::config::PartialConfig {
            provider: Some(ProviderKind::OpenAiCompatible),
            base_url: Some("https://api.openai.com/v1".into()),
            model: Some("gpt-4.1-mini".into()),
            api_key: Some("original-explicit-placeholder".into()),
            ..crate::config::PartialConfig::default()
        }
        .resolve_for_interactive_setup()
        .unwrap();
        for (provider, base_url, model, expected_key) in [
            (
                "anthropic",
                "https://api.anthropic.com",
                "test-model",
                "anthropic-target-placeholder",
            ),
            (
                "openai-compatible",
                "https://generativelanguage.googleapis.com/v1beta/openai",
                "test-model",
                "gemini-target-placeholder",
            ),
        ] {
            let mut cfg = original.clone();
            let saved = PanePrefs {
                provider: Some(provider.into()),
                base_url: Some(base_url.into()),
                model: Some(model.into()),
                ..PanePrefs::default()
            };
            apply_pane_prefs(PrefPane::Observer, &saved, &mut cfg, None);
            assert_eq!(cfg.base_url, base_url);
            assert_eq!(cfg.api_key.as_deref(), Some(expected_key));
        }
        let mut cfg = original.clone();
        apply_pane_prefs(
            PrefPane::Observer,
            &snapshot_run_config(&original),
            &mut cfg,
            None,
        );
        assert_eq!(
            cfg.api_key.as_deref(),
            Some("original-explicit-placeholder")
        );
        let mut cfg = original.clone();
        apply_pane_prefs(
            PrefPane::Observer,
            &PanePrefs {
                provider: Some("anthropic".into()),
                ..PanePrefs::default()
            },
            &mut cfg,
            None,
        );
        assert_eq!(cfg.base_url, "https://api.anthropic.com/v1");
        assert_eq!(cfg.api_key.as_deref(), Some("anthropic-target-placeholder"));
        let mut cfg = original.clone();
        apply_pane_prefs(
            PrefPane::Observer,
            &PanePrefs {
                provider: Some("mistral".into()),
                ..PanePrefs::default()
            },
            &mut cfg,
            None,
        );
        assert_eq!(cfg.base_url, "https://api.mistral.ai/v1");
        assert!(cfg.api_key.is_none());
        let mut cfg = original.clone();
        apply_pane_prefs(
            PrefPane::Observer,
            &PanePrefs {
                base_url: Some("file:///tmp/settings".into()),
                ..PanePrefs::default()
            },
            &mut cfg,
            None,
        );
        assert_eq!(cfg.base_url, original.base_url);
        assert_eq!(cfg.api_key, original.api_key);
    }

    #[test]
    fn prefs_roundtrip_realize_preset() {
        let td = tempfile::tempdir().expect("tempdir");
        let root = td.path().to_string_lossy().into_owned();
        let mut prefs = TuiPrefs::default();
        prefs.set_coder_realize(RealizePreset::High);
        prefs.set_coder_persona("thoughtful");
        prefs.set_ui_lang("en");
        prefs.coder.mode = Some("VIBE".to_string());
        prefs.observer.persona = Some("skeptical".to_string());
        prefs.chat.temperature = Some(0.9);
        prefs.auto_observe = Some(true);
        prefs.auto_fix_mode = Some(false);
        prefs.ui_right_tab = Some("chat".to_string());
        let path = save_prefs(Some(&root), &prefs).expect("save prefs");
        assert!(path.is_file());

        let loaded = load_prefs(Some(&root)).expect("load prefs");
        assert_eq!(loaded.coder_realize(), Some(RealizePreset::High));
        assert_eq!(loaded.effective_coder_persona(), Some("thoughtful"));
        assert_eq!(loaded.ui_lang.as_deref(), Some("en"));
        assert_eq!(loaded.coder.mode.as_deref(), Some("VIBE"));
        assert_eq!(loaded.observer.persona.as_deref(), Some("skeptical"));
        assert_eq!(loaded.chat.temperature, Some(0.9));
        assert_eq!(loaded.auto_observe, Some(true));
        assert_eq!(loaded.auto_fix_mode, Some(false));
        assert_eq!(loaded.ui_right_tab.as_deref(), Some("chat"));
    }

    #[test]
    fn missing_prefs_file_returns_default() {
        let td = tempfile::tempdir().expect("tempdir");
        let root = td.path().to_string_lossy().into_owned();
        let loaded = load_prefs(Some(&root)).expect("load prefs");
        assert_eq!(loaded.coder_realize(), None);
    }
}
