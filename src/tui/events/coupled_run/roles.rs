//! Resolve each role independently, and expose only non-secret run identity.
use crate::config::{PartialConfig, ProviderPreset, RunConfig};
use crate::modes::Mode;
use anyhow::{ensure, Context, Result};
use serde::Serialize;

#[derive(clap::Args, Debug, Default)]
pub(crate) struct ObserverArgs {
    /// Observer provider; requires --observer-model. Defaults to the Coder config.
    #[arg(
        long = "observer-provider",
        id = "observer_provider",
        value_enum,
        requires = "observer_model"
    )]
    pub provider: Option<crate::CliProviderPreset>,
    /// Observer model; requires --observer-provider.
    #[arg(
        long = "observer-model",
        id = "observer_model",
        requires = "observer_provider"
    )]
    pub model: Option<String>,
    /// Observer endpoint; a custom endpoint also requires --observer-api-key-env.
    #[arg(long = "observer-base-url", id = "observer_base_url", requires_all = ["observer_provider", "observer_model"])]
    pub base_url: Option<String>,
    /// Name of the Observer credential variable, never the secret itself.
    #[arg(long = "observer-api-key-env", id = "observer_api_key_env", requires_all = ["observer_provider", "observer_model"])]
    pub api_key_env: Option<String>,
}

pub(crate) struct ResolvedRoles {
    pub coder: RunConfig,
    pub observer: Option<RunConfig>,
}

#[derive(Debug, Serialize)]
pub(crate) struct RoleReceipt {
    provider: &'static str,
    transport: &'static str,
    model: String,
    endpoint_origin: String,
    temperature: f64,
    max_tokens: u32,
    timeout_seconds: u64,
}

#[derive(Debug, Serialize)]
pub(crate) struct RoleReceipts {
    coder: RoleReceipt,
    observer: Option<RoleReceipt>,
}

impl ResolvedRoles {
    pub(crate) fn receipts(&self) -> RoleReceipts {
        RoleReceipts {
            coder: receipt(&self.coder),
            observer: self
                .observer
                .as_ref()
                .map(|cfg| receipt(&super::super::observer_request::next_action_config(cfg))),
        }
    }

    pub(crate) fn same_model_both_roles(&self) -> bool {
        self.observer.as_ref().is_some_and(|observer| {
            self.coder.provider == observer.provider
                && self.coder.model == observer.model
                && self.coder.base_url == observer.base_url
        })
    }
}

fn receipt(cfg: &RunConfig) -> RoleReceipt {
    RoleReceipt {
        provider: crate::config::provider_preset_for_run(cfg).key(),
        transport: cfg.provider.key(),
        model: cfg.model.clone(),
        // Custom URLs may contain auth in their path or query. Never persist it.
        endpoint_origin: reqwest::Url::parse(&cfg.base_url)
            .map(|url| url.origin().ascii_serialization())
            .unwrap_or_default(),
        temperature: cfg.temperature,
        max_tokens: cfg.max_tokens,
        timeout_seconds: cfg.timeout_seconds,
    }
}

pub(crate) fn resolve(
    coder: RunConfig,
    args: &ObserverArgs,
    without_observer: bool,
) -> Result<ResolvedRoles> {
    resolve_with_env(coder, args, without_observer, |name| {
        std::env::var(name).ok()
    })
}

fn resolve_with_env(
    coder: RunConfig,
    args: &ObserverArgs,
    without_observer: bool,
    get: impl Fn(&str) -> Option<String>,
) -> Result<ResolvedRoles> {
    let explicit = args.provider.is_some()
        || args.model.is_some()
        || args.base_url.is_some()
        || args.api_key_env.is_some();
    ensure!(
        !without_observer || !explicit,
        "Observer configuration conflicts with --without-observer"
    );
    if without_observer {
        return Ok(ResolvedRoles {
            coder,
            observer: None,
        });
    }
    if !explicit {
        let mut observer = coder.clone();
        observer.mode = Mode::Observer;
        return Ok(ResolvedRoles {
            coder,
            observer: Some(observer),
        });
    }
    let preset: ProviderPreset = args
        .provider
        .clone()
        .context("explicit Observer configuration requires --observer-provider")?
        .into();
    let model = args
        .model
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .context("explicit Observer configuration requires a nonempty --observer-model")?;
    let base_url = match args.base_url.as_deref() {
        Some(value) => {
            ensure!(
                !value.trim().is_empty(),
                "Observer endpoint must not be empty"
            );
            value.trim()
        }
        None => preset
            .default_base_url()
            .context("custom Observer provider requires --observer-base-url")?,
    };
    let url = reqwest::Url::parse(base_url).context("invalid Observer endpoint")?;
    ensure!(
        matches!(url.scheme(), "http" | "https") && url.host_str().is_some(),
        "Observer endpoint must be an HTTP(S) URL with a host"
    );
    ensure!(url.username().is_empty() && url.password().is_none() && url.query().is_none() && url.fragment().is_none(), "Observer endpoint must not contain credentials, query or fragment; use --observer-api-key-env");
    let custom_endpoint = preset
        .default_base_url()
        .is_none_or(|default| default.trim_end_matches('/') != base_url.trim_end_matches('/'));
    ensure!(
        !custom_endpoint || preset == ProviderPreset::HfLocal || args.api_key_env.is_some(),
        "custom Observer endpoint requires an explicit --observer-api-key-env"
    );
    let key = if preset == ProviderPreset::HfLocal {
        ensure!(
            args.api_key_env.is_none(),
            "HF Observer does not use an API key variable"
        );
        None
    } else {
        let names = if let Some(name) = args.api_key_env.as_deref() {
            ensure!(
                valid_env_name(name),
                "invalid Observer credential variable name"
            );
            vec![name]
        } else {
            match preset {
                ProviderPreset::OpenAi => vec!["OPENAI_API_KEY"],
                ProviderPreset::Gemini => vec!["GEMINI_API_KEY", "GOOGLE_API_KEY"],
                ProviderPreset::Mistral => vec!["MISTRAL_API_KEY"],
                ProviderPreset::Anthropic | ProviderPreset::AnthropicCompat => {
                    vec!["ANTHROPIC_API_KEY"]
                }
                _ => unreachable!("custom endpoint requires an explicit credential variable"),
            }
        };
        Some(
            names
                .iter()
                .find_map(|name| {
                    get(name)
                        .map(|key| key.trim().to_owned())
                        .filter(|key| !key.is_empty())
                })
                .with_context(|| {
                    format!(
                        "Observer credential unavailable; set {}",
                        names.join(" or ")
                    )
                })?,
        )
    };
    // Populate every model and endpoint field so global SPIRAL_CODER_* values
    // cannot select the other role's model, URL or credential during resolution.
    let observer = PartialConfig {
        provider: Some(preset.provider_kind()),
        model: Some(model.into()),
        chat_model: Some(model.into()),
        code_model: Some(model.into()),
        api_key: key,
        base_url: Some(base_url.into()),
        mode: Some(Mode::Observer),
        persona: Some(coder.persona.clone()),
        temperature: Some(coder.temperature),
        max_tokens: Some(coder.max_tokens),
        timeout_seconds: Some(coder.timeout_seconds),
        hf_device: Some(coder.hf_device.clone()),
        hf_local_only: Some(coder.hf_local_only),
        ..Default::default()
    }
    .resolve()?;
    Ok(ResolvedRoles {
        coder,
        observer: Some(observer),
    })
}

fn valid_env_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    bytes
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == b'_')
        && bytes.all(|c| c.is_ascii_alphanumeric() || c == b'_')
}

#[cfg(test)]
mod tests;
