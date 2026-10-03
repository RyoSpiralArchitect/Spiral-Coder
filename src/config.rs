use anyhow::{anyhow, Context, Result};
use clap::ValueEnum;
use reqwest::Url;
use serde::{Deserialize, Serialize};

use crate::modes::Mode;
use crate::personas;

#[derive(Clone, Debug, PartialEq, ValueEnum, Serialize, Deserialize)]
pub enum ProviderKind {
    #[serde(rename = "openai-compatible")]
    #[value(name = "openai-compatible", alias = "openai", alias = "openai_compat")]
    OpenAiCompatible,

    #[serde(rename = "mistral")]
    #[value(name = "mistral")]
    Mistral,

    #[serde(rename = "anthropic")]
    #[value(name = "anthropic")]
    Anthropic,

    #[serde(rename = "hf")]
    #[value(name = "hf", alias = "huggingface")]
    Hf,
}

impl ProviderKind {
    pub fn key(&self) -> &'static str {
        match self {
            ProviderKind::OpenAiCompatible => "openai-compatible",
            ProviderKind::Mistral => "mistral",
            ProviderKind::Anthropic => "anthropic",
            ProviderKind::Hf => "hf",
        }
    }
}

impl std::fmt::Display for ProviderKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.key())
    }
}

impl std::str::FromStr for ProviderKind {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        parse_provider(s).ok_or_else(|| format!("unsupported provider: {s}"))
    }
}

pub fn supported_providers() -> Vec<&'static str> {
    provider_preset_keys(false)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProviderPreset {
    OpenAi,
    Gemini,
    AnthropicCompat,
    OpenAiCompatibleCustom,
    Mistral,
    Anthropic,
    HfLocal,
}

impl ProviderPreset {
    pub fn key(self) -> &'static str {
        match self {
            ProviderPreset::OpenAi => "openai",
            ProviderPreset::Gemini => "gemini",
            ProviderPreset::AnthropicCompat => "anthropic-compat",
            ProviderPreset::OpenAiCompatibleCustom => "openai-compatible",
            ProviderPreset::Mistral => "mistral",
            ProviderPreset::Anthropic => "anthropic",
            ProviderPreset::HfLocal => "hf",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ProviderPreset::OpenAi => "OpenAI",
            ProviderPreset::Gemini => "Google Gemini",
            ProviderPreset::AnthropicCompat => "Anthropic (OpenAI-compatible)",
            ProviderPreset::OpenAiCompatibleCustom => "OpenAI-compatible (custom)",
            ProviderPreset::Mistral => "Mistral",
            ProviderPreset::Anthropic => "Anthropic",
            ProviderPreset::HfLocal => "HF local",
        }
    }

    pub fn provider_kind(self) -> ProviderKind {
        match self {
            ProviderPreset::OpenAi
            | ProviderPreset::Gemini
            | ProviderPreset::AnthropicCompat
            | ProviderPreset::OpenAiCompatibleCustom => ProviderKind::OpenAiCompatible,
            ProviderPreset::Mistral => ProviderKind::Mistral,
            ProviderPreset::Anthropic => ProviderKind::Anthropic,
            ProviderPreset::HfLocal => ProviderKind::Hf,
        }
    }

    pub fn default_base_url(self) -> Option<&'static str> {
        match self {
            ProviderPreset::OpenAi => Some("https://api.openai.com/v1"),
            ProviderPreset::Gemini => {
                Some("https://generativelanguage.googleapis.com/v1beta/openai")
            }
            ProviderPreset::AnthropicCompat => Some("https://api.anthropic.com/v1"),
            ProviderPreset::OpenAiCompatibleCustom => None,
            ProviderPreset::Mistral => Some("https://api.mistral.ai/v1"),
            ProviderPreset::Anthropic => Some("https://api.anthropic.com/v1"),
            ProviderPreset::HfLocal => Some("http://localhost"),
        }
    }

    pub fn api_key_env_hint(self) -> &'static str {
        match self {
            ProviderPreset::OpenAi => "OPENAI_API_KEY or SPIRAL_CODER_API_KEY",
            ProviderPreset::Gemini => "GEMINI_API_KEY or GOOGLE_API_KEY",
            ProviderPreset::AnthropicCompat | ProviderPreset::Anthropic => "ANTHROPIC_API_KEY",
            ProviderPreset::OpenAiCompatibleCustom => {
                "provider-specific key (or SPIRAL_CODER_API_KEY for OpenAI-style endpoints)"
            }
            ProviderPreset::Mistral => "MISTRAL_API_KEY or SPIRAL_CODER_API_KEY",
            ProviderPreset::HfLocal => "(none; hf/local does not use an API key)",
        }
    }

    pub fn coder_supported(self) -> bool {
        matches!(
            self,
            ProviderPreset::OpenAi
                | ProviderPreset::Gemini
                | ProviderPreset::AnthropicCompat
                | ProviderPreset::Mistral
                | ProviderPreset::OpenAiCompatibleCustom
        )
    }

    pub fn representative_models(self) -> &'static [&'static str] {
        match self {
            ProviderPreset::OpenAi => &[
                "gpt-5-mini",
                "gpt-5",
                "gpt-4.1-mini",
                "gpt-4.1",
                "gpt-4o-mini",
                "other",
            ],
            ProviderPreset::Gemini => &[
                "gemini-2.5-flash",
                "gemini-2.5-pro",
                "gemini-2.5-flash-lite",
                "gemini-2.0-flash",
                "other",
            ],
            ProviderPreset::AnthropicCompat | ProviderPreset::Anthropic => &[
                "claude-sonnet-4-6",
                "claude-haiku-4-5",
                "claude-opus-4-6",
                "claude-sonnet-4-5",
                "other",
            ],
            ProviderPreset::OpenAiCompatibleCustom => &[
                "gpt-5-mini",
                "gemini-2.5-flash",
                "claude-sonnet-4-6",
                "other",
            ],
            ProviderPreset::Mistral => &[
                "devstral-small-latest",
                "codestral-latest",
                "devstral-medium-latest",
                "mistral-small-latest",
                "mistral-large-latest",
                "other",
            ],
            ProviderPreset::HfLocal => &["local", "other"],
        }
    }

    pub fn default_model(self, vibe: bool) -> &'static str {
        match self {
            ProviderPreset::OpenAi => "gpt-5-mini",
            ProviderPreset::Gemini => "gemini-2.5-flash",
            ProviderPreset::AnthropicCompat | ProviderPreset::Anthropic => "claude-sonnet-4-6",
            ProviderPreset::OpenAiCompatibleCustom => "gpt-5-mini",
            ProviderPreset::Mistral => {
                if vibe {
                    "devstral-small-latest"
                } else {
                    "mistral-small-latest"
                }
            }
            ProviderPreset::HfLocal => "local",
        }
    }
}

pub fn supported_provider_presets(coder_only: bool) -> Vec<ProviderPreset> {
    [
        ProviderPreset::OpenAi,
        ProviderPreset::Gemini,
        ProviderPreset::AnthropicCompat,
        ProviderPreset::Mistral,
        ProviderPreset::Anthropic,
        ProviderPreset::HfLocal,
    ]
    .into_iter()
    .filter(|preset| !coder_only || preset.coder_supported())
    .collect()
}

pub fn provider_preset_keys(coder_only: bool) -> Vec<&'static str> {
    supported_provider_presets(coder_only)
        .into_iter()
        .map(ProviderPreset::key)
        .collect()
}

pub fn parse_provider_preset(s: &str) -> Option<ProviderPreset> {
    match normalize_provider(s).as_str() {
        "openai" => Some(ProviderPreset::OpenAi),
        "gemini" | "google" | "google-gemini" => Some(ProviderPreset::Gemini),
        "anthropic-compat" | "claude-compat" | "anthropic_openai" => {
            Some(ProviderPreset::AnthropicCompat)
        }
        "openai-compatible" | "openai-compat" | "openai_compat" => {
            Some(ProviderPreset::OpenAiCompatibleCustom)
        }
        "mistral" => Some(ProviderPreset::Mistral),
        "anthropic" | "claude" => Some(ProviderPreset::Anthropic),
        "hf" | "huggingface" | "hf-local" => Some(ProviderPreset::HfLocal),
        _ => None,
    }
}

pub fn apply_provider_preset(partial: &mut PartialConfig, preset: ProviderPreset) {
    partial.provider = Some(preset.provider_kind());
    let needs_default_base_url = partial
        .base_url
        .as_deref()
        .map(|v| v.trim().is_empty())
        .unwrap_or(true);
    if needs_default_base_url {
        partial.base_url = preset.default_base_url().map(str::to_string);
    }
}

pub fn provider_preset_for_run(cfg: &RunConfig) -> ProviderPreset {
    detect_provider_preset(&cfg.provider, &cfg.base_url)
}

pub fn representative_models_for_run(cfg: &RunConfig) -> &'static [&'static str] {
    provider_preset_for_run(cfg).representative_models()
}

pub fn should_send_temperature(provider: &ProviderKind, base_url: &str, model: &str) -> bool {
    let model = model.trim().to_ascii_lowercase();
    match detect_provider_preset(provider, base_url) {
        // GPT-5 family commonly rejects explicit temperature on OpenAI-compatible APIs,
        // including first-party OpenAI and custom/proxied OpenAI-compatible gateways.
        ProviderPreset::OpenAi | ProviderPreset::OpenAiCompatibleCustom => {
            !model.starts_with("gpt-5")
        }
        _ => true,
    }
}

pub fn should_send_temperature_for_run(cfg: &RunConfig) -> bool {
    should_send_temperature(&cfg.provider, &cfg.base_url, &cfg.model)
}

fn detect_provider_preset(provider: &ProviderKind, base_url: &str) -> ProviderPreset {
    match provider {
        ProviderKind::Mistral => ProviderPreset::Mistral,
        ProviderKind::Anthropic => ProviderPreset::Anthropic,
        ProviderKind::Hf => ProviderPreset::HfLocal,
        ProviderKind::OpenAiCompatible => {
            let base = base_url.trim().to_ascii_lowercase();
            if base.contains("generativelanguage.googleapis.com") {
                ProviderPreset::Gemini
            } else if base.contains("anthropic.com") {
                ProviderPreset::AnthropicCompat
            } else if base.contains("api.openai.com") {
                ProviderPreset::OpenAi
            } else {
                ProviderPreset::OpenAiCompatibleCustom
            }
        }
    }
}

pub fn normalize_provider(s: &str) -> String {
    s.trim().to_ascii_lowercase()
}

fn parse_provider(s: &str) -> Option<ProviderKind> {
    match normalize_provider(s).as_str() {
        "openai-compatible" | "openai" | "openai_compat" => Some(ProviderKind::OpenAiCompatible),
        "mistral" => Some(ProviderKind::Mistral),
        "anthropic" => Some(ProviderKind::Anthropic),
        "hf" | "huggingface" => Some(ProviderKind::Hf),
        _ => None,
    }
}

#[derive(Clone, Debug, Default)]
pub struct PartialConfig {
    pub vibe: bool,
    pub provider: Option<ProviderKind>,
    pub model: Option<String>,
    pub chat_model: Option<String>,
    pub code_model: Option<String>,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub mode: Option<Mode>,
    pub persona: Option<String>,
    pub temperature: Option<f64>,
    pub max_tokens: Option<u32>,
    pub timeout_seconds: Option<u64>,
    pub hf_device: Option<String>,
    pub hf_local_only: Option<bool>,
}

#[derive(Clone, Debug)]
pub struct RunConfig {
    pub provider: ProviderKind,
    pub model: String,
    pub chat_model: String,
    pub code_model: String,
    pub api_key: Option<String>,
    pub base_url: String,
    pub mode: Mode,
    pub persona: String,
    pub temperature: f64,
    pub max_tokens: u32,
    pub timeout_seconds: u64,
    pub hf_device: String,
    pub hf_local_only: bool,
}

impl PartialConfig {
    pub fn resolve(self) -> Result<RunConfig> {
        self.resolve_with_credentials(CredentialCheck::AtLaunch)
    }

    /// Resolve an interactive setup screen before credentials have been entered.
    /// Callers must validate pane readiness before starting a provider request.
    pub(crate) fn resolve_for_interactive_setup(self) -> Result<RunConfig> {
        self.resolve_with_credentials(CredentialCheck::BeforeSend)
    }

    fn resolve_with_credentials(mut self, credential_check: CredentialCheck) -> Result<RunConfig> {
        if self.provider.is_none() {
            if let Some(v) = env_trimmed("SPIRAL_CODER_PROVIDER") {
                if let Some(preset) = parse_provider_preset(&v) {
                    apply_provider_preset(&mut self, preset);
                } else {
                    self.provider = parse_provider(&v);
                }
            }
        }
        if self.model.is_none() {
            self.model = env_trimmed("SPIRAL_CODER_MODEL");
        }
        if self.chat_model.is_none() {
            self.chat_model = env_trimmed("SPIRAL_CODER_CHAT_MODEL");
        }
        if self.code_model.is_none() {
            self.code_model = env_trimmed("SPIRAL_CODER_CODE_MODEL");
        }
        if self.base_url.is_none() {
            self.base_url = env_trimmed("SPIRAL_CODER_BASE_URL");
        }
        if self.timeout_seconds.is_none() {
            self.timeout_seconds =
                env_trimmed("SPIRAL_CODER_TIMEOUT_SECONDS").and_then(|v| v.parse().ok());
        }
        if self.persona.is_none() {
            self.persona = env_trimmed("SPIRAL_CODER_PERSONA");
        }
        if self.hf_device.is_none() {
            self.hf_device = env_trimmed("SPIRAL_CODER_HF_DEVICE");
        }
        if self.hf_local_only.is_none() {
            self.hf_local_only =
                env_trimmed("SPIRAL_CODER_HF_LOCAL_ONLY").and_then(|v| parse_bool(&v));
        }

        // Provider defaulting:
        // - `--vibe` is a preset that implies Mistral/Codestral unless explicitly overridden.
        // - Otherwise, prefer an "it just works" default for local dev: infer from base_url/env keys,
        //   falling back to OpenAI-compatible (so users with only OPENAI_API_KEY don't error out).
        let provider = if let Some(p) = self.provider.clone() {
            p
        } else if self.vibe {
            ProviderKind::Mistral
        } else if let Some(u) = self
            .base_url
            .as_deref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            // Infer from explicit base_url when present.
            let low = u.to_ascii_lowercase();
            if low.contains("anthropic") {
                ProviderKind::Anthropic
            } else if low.contains("mistral.ai") {
                ProviderKind::Mistral
            } else {
                ProviderKind::OpenAiCompatible
            }
        } else if env_trimmed("MISTRAL_API_KEY").is_some() {
            ProviderKind::Mistral
        } else if env_trimmed("ANTHROPIC_API_KEY").is_some() {
            ProviderKind::Anthropic
        } else if env_trimmed("GEMINI_API_KEY").is_some() || env_trimmed("GOOGLE_API_KEY").is_some()
        {
            ProviderKind::OpenAiCompatible
        } else if env_trimmed("OPENAI_API_KEY").is_some()
            || env_trimmed("SPIRAL_CODER_API_KEY").is_some()
        {
            ProviderKind::OpenAiCompatible
        } else {
            ProviderKind::OpenAiCompatible
        };

        let mode = if self.vibe {
            self.mode.unwrap_or(Mode::Vibe)
        } else {
            self.mode.unwrap_or(Mode::Kabeuchi)
        };

        let base_url = match self.base_url {
            Some(u) if !u.trim().is_empty() => u,
            _ => default_base_url(&provider).to_string(),
        };
        let base_url = base_url.trim().trim_end_matches('/').to_string();
        validate_base_url(&base_url).context("invalid --base-url")?;

        let persona_in = self.persona.unwrap_or_else(|| "default".to_string());
        let persona_def = personas::resolve_persona(&persona_in).context("invalid persona")?;
        let persona = persona_def.key.to_string();

        let base_model = match self.model {
            Some(m) if !m.trim().is_empty() => m,
            _ => default_model(&provider, &base_url, self.vibe).to_string(),
        };

        let chat_model = match self.chat_model {
            Some(m) if !m.trim().is_empty() => m,
            _ => base_model.clone(),
        };

        let code_model = match self.code_model {
            Some(m) if !m.trim().is_empty() => m,
            _ => base_model.clone(),
        };

        let model = if mode.uses_code_model() {
            code_model.clone()
        } else {
            chat_model.clone()
        };

        if provider == ProviderKind::OpenAiCompatible {
            if let Ok(url) = Url::parse(&base_url) {
                if url.host_str() == Some("api.openai.com") && model.contains("devstral") {
                    return Err(anyhow!(
                        "invalid model for OpenAI base URL: {model}. If you want Devstral, use --provider mistral (base_url=https://api.mistral.ai/v1)."
                    ));
                }
            }
        }

        let temperature = self.temperature.unwrap_or(0.4);
        if !(0.0..=2.0).contains(&temperature) {
            return Err(anyhow!(
                "invalid temperature: {temperature} (expected 0.0..=2.0)"
            ));
        }

        let max_tokens = self.max_tokens.unwrap_or(1024);
        if max_tokens == 0 {
            return Err(anyhow!("invalid max_tokens: 0 (expected >= 1)"));
        }

        let timeout_seconds = self.timeout_seconds.unwrap_or(120);
        if timeout_seconds == 0 {
            return Err(anyhow!("invalid timeout_seconds: 0 (expected >= 1)"));
        }

        let api_key = self
            .api_key
            .and_then(|k| {
                let k = k.trim().to_string();
                if k.is_empty() {
                    None
                } else {
                    Some(k)
                }
            })
            .or_else(|| resolve_api_key_from_env(&provider, &base_url));

        match provider {
            ProviderKind::Mistral
                if credential_check == CredentialCheck::AtLaunch && api_key.is_none() =>
            {
                return Err(anyhow!(
                    "missing API key for mistral. Set MISTRAL_API_KEY (or SPIRAL_CODER_API_KEY), or pass --api-key."
                ));
            }
            ProviderKind::Anthropic
                if credential_check == CredentialCheck::AtLaunch && api_key.is_none() =>
            {
                return Err(anyhow!(
                    "missing API key for anthropic. Set ANTHROPIC_API_KEY (or pass --api-key)."
                ));
            }
            _ => {}
        }

        let hf_device = self.hf_device.unwrap_or_else(|| "auto".to_string());
        let hf_local_only = self.hf_local_only.unwrap_or(false);

        Ok(RunConfig {
            provider,
            model,
            chat_model,
            code_model,
            api_key,
            base_url,
            mode,
            persona,
            temperature,
            max_tokens,
            timeout_seconds,
            hf_device,
            hf_local_only,
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CredentialCheck {
    AtLaunch,
    BeforeSend,
}

fn default_base_url(provider: &ProviderKind) -> &'static str {
    match provider {
        ProviderKind::OpenAiCompatible => "https://api.openai.com/v1",
        ProviderKind::Mistral => "https://api.mistral.ai/v1",
        ProviderKind::Anthropic => "https://api.anthropic.com/v1",
        ProviderKind::Hf => "http://localhost",
    }
}

fn default_model(provider: &ProviderKind, base_url: &str, vibe: bool) -> &'static str {
    detect_provider_preset(provider, base_url).default_model(vibe)
}

fn validate_base_url(base_url: &str) -> Result<()> {
    let url = Url::parse(base_url)?;
    match url.scheme() {
        "http" | "https" => {}
        other => {
            return Err(anyhow!(
                "unsupported URL scheme: {other} (expected http or https)"
            ));
        }
    }
    if url.host_str().is_none() {
        return Err(anyhow!("base_url missing host"));
    }
    Ok(())
}

fn env_trimmed(key: &str) -> Option<String> {
    std::env::var(key).ok().and_then(|v| {
        let v = v.trim().to_string();
        if v.is_empty() {
            None
        } else {
            Some(v)
        }
    })
}

fn parse_bool(s: &str) -> Option<bool> {
    match s.trim() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn resolve_api_key_from_env(provider: &ProviderKind, base_url: &str) -> Option<String> {
    let get = |k: &str| env_trimmed(k);
    match detect_provider_preset(provider, base_url) {
        ProviderPreset::OpenAi => get("SPIRAL_CODER_API_KEY").or_else(|| get("OPENAI_API_KEY")),
        ProviderPreset::Gemini => get("GEMINI_API_KEY").or_else(|| get("GOOGLE_API_KEY")),
        ProviderPreset::AnthropicCompat | ProviderPreset::Anthropic => get("ANTHROPIC_API_KEY"),
        ProviderPreset::OpenAiCompatibleCustom => get("SPIRAL_CODER_API_KEY")
            .or_else(|| get("OPENAI_API_KEY"))
            .or_else(|| get("GEMINI_API_KEY"))
            .or_else(|| get("GOOGLE_API_KEY"))
            .or_else(|| get("ANTHROPIC_API_KEY")),
        ProviderPreset::Mistral => get("MISTRAL_API_KEY").or_else(|| get("SPIRAL_CODER_API_KEY")),
        ProviderPreset::HfLocal => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_env_var<T>(key: &str, value: Option<&str>, f: impl FnOnce() -> T) -> T {
        let prev = std::env::var(key).ok();
        match value {
            Some(v) => unsafe { std::env::set_var(key, v) },
            None => unsafe { std::env::remove_var(key) },
        }
        let out = f();
        match prev {
            Some(v) => unsafe { std::env::set_var(key, v) },
            None => unsafe { std::env::remove_var(key) },
        }
        out
    }

    #[test]
    fn detects_gemini_openai_compat_preset_from_base_url() {
        let cfg = RunConfig {
            provider: ProviderKind::OpenAiCompatible,
            model: "gemini-2.5-flash".to_string(),
            chat_model: "gemini-2.5-flash".to_string(),
            code_model: "gemini-2.5-flash".to_string(),
            api_key: Some("x".to_string()),
            base_url: "https://generativelanguage.googleapis.com/v1beta/openai".to_string(),
            mode: Mode::Chat,
            persona: "default".to_string(),
            temperature: 0.2,
            max_tokens: 256,
            timeout_seconds: 30,
            hf_device: "auto".to_string(),
            hf_local_only: false,
        };
        assert_eq!(provider_preset_for_run(&cfg), ProviderPreset::Gemini);
        assert_eq!(representative_models_for_run(&cfg)[0], "gemini-2.5-flash");
    }

    #[test]
    fn default_model_uses_vendor_specific_openai_compat_target() {
        assert_eq!(
            default_model(
                &ProviderKind::OpenAiCompatible,
                "https://generativelanguage.googleapis.com/v1beta/openai",
                false,
            ),
            "gemini-2.5-flash"
        );
        assert_eq!(
            default_model(
                &ProviderKind::OpenAiCompatible,
                "https://api.openai.com/v1",
                false
            ),
            "gpt-5-mini"
        );
    }

    #[test]
    fn resolve_api_key_uses_vendor_specific_env_for_gemini() {
        with_env_var("GEMINI_API_KEY", Some("gem-test"), || {
            with_env_var("GOOGLE_API_KEY", None, || {
                with_env_var("OPENAI_API_KEY", None, || {
                    let key = resolve_api_key_from_env(
                        &ProviderKind::OpenAiCompatible,
                        "https://generativelanguage.googleapis.com/v1beta/openai",
                    );
                    assert_eq!(key.as_deref(), Some("gem-test"));
                })
            })
        });
    }

    #[test]
    fn apply_provider_preset_sets_gemini_base_url() {
        let mut partial = PartialConfig::default();
        apply_provider_preset(&mut partial, ProviderPreset::Gemini);
        assert_eq!(partial.provider, Some(ProviderKind::OpenAiCompatible));
        assert_eq!(
            partial.base_url.as_deref(),
            Some("https://generativelanguage.googleapis.com/v1beta/openai")
        );
    }

    #[test]
    fn resolve_honors_obs_provider_gemini_preset() {
        with_env_var("SPIRAL_CODER_PROVIDER", Some("gemini"), || {
            with_env_var("SPIRAL_CODER_BASE_URL", None, || {
                with_env_var("GEMINI_API_KEY", Some("gem-test"), || {
                    let cfg = PartialConfig::default()
                        .resolve()
                        .expect("resolve gemini preset");
                    assert_eq!(cfg.provider, ProviderKind::OpenAiCompatible);
                    assert_eq!(
                        cfg.base_url,
                        "https://generativelanguage.googleapis.com/v1beta/openai"
                    );
                    assert_eq!(cfg.model, "gemini-2.5-flash");
                    assert_eq!(cfg.chat_model, "gemini-2.5-flash");
                    assert_eq!(cfg.code_model, "gemini-2.5-flash");
                })
            })
        });
    }

    #[test]
    fn openai_gpt5_omits_temperature() {
        assert!(!should_send_temperature(
            &ProviderKind::OpenAiCompatible,
            "https://api.openai.com/v1",
            "gpt-5-mini",
        ));
        assert!(!should_send_temperature(
            &ProviderKind::OpenAiCompatible,
            "https://azure.example.net/openai/v1",
            "gpt-5",
        ));
        assert!(should_send_temperature(
            &ProviderKind::OpenAiCompatible,
            "https://api.openai.com/v1",
            "gpt-4.1-mini",
        ));
        assert!(should_send_temperature(
            &ProviderKind::OpenAiCompatible,
            "https://generativelanguage.googleapis.com/v1beta/openai",
            "gemini-2.5-flash",
        ));
    }

    #[test]
    fn mistral_preset_lists_large_latest_model() {
        assert!(ProviderPreset::Mistral
            .representative_models()
            .contains(&"mistral-large-latest"));
    }

    #[test]
    fn interactive_setup_defers_only_missing_credentials() {
        // Credential environment changes belong to a child process, not the
        // process running other config tests concurrently.
        const CHILD: &str = "SPIRAL_CODER_CONFIG_SETUP_TEST_CHILD";
        if std::env::var_os(CHILD).is_none() {
            let result = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "config::tests::interactive_setup_defers_only_missing_credentials",
                    "--nocapture",
                ])
                .env(CHILD, "1")
                .env_remove("MISTRAL_API_KEY")
                .env_remove("ANTHROPIC_API_KEY")
                .env_remove("SPIRAL_CODER_API_KEY")
                .output()
                .unwrap();
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            return;
        }
        for provider in [ProviderKind::Mistral, ProviderKind::Anthropic] {
            let partial = PartialConfig {
                provider: Some(provider),
                model: Some("test-model".into()),
                base_url: Some("http://127.0.0.1:1".into()),
                mode: Some(Mode::Chat),
                persona: Some("default".into()),
                timeout_seconds: Some(1),
                ..PartialConfig::default()
            };
            let setup = partial.clone().resolve_for_interactive_setup().unwrap();
            assert!(setup.api_key.is_none());
            assert!(partial
                .clone()
                .resolve()
                .unwrap_err()
                .to_string()
                .contains("missing API key"));
            let mut invalid = partial.clone();
            invalid.base_url = Some("file:///tmp/config".into());
            assert!(invalid.clone().resolve_for_interactive_setup().is_err());
            assert!(invalid.resolve().is_err());
            let mut invalid = partial;
            invalid.temperature = Some(3.0);
            assert!(invalid.clone().resolve_for_interactive_setup().is_err());
            assert!(invalid.resolve().is_err());
        }
    }
}
