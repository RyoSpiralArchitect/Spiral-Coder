//! Curated tool-capable models, checked against provider docs on 2026-10-24.
//! This is a picker, not an allowlist: custom IDs and saved choices still work.
//! Sources and transport limits live in docs/model-catalog.md.
use super::{detect_provider_preset, ProviderKind, ProviderPreset};
use serde_json::{json, Value};

pub(super) fn models(preset: ProviderPreset) -> &'static [&'static str] {
    match preset {
        ProviderPreset::OpenAi => &[
            "gpt-6-luna",
            "gpt-6-sol",
            "gpt-5.6-luna",
            "gpt-5.4-mini",
            "gpt-5.4",
            "gpt-5-mini",
            "gpt-4.1-mini",
            "other",
        ],
        ProviderPreset::Gemini => &[
            "gemini-3.8-flash",
            "gemini-3.5-flash-lite",
            "gemini-4-argon",
            "gemini-2.5-flash",
            "other",
        ],
        ProviderPreset::AnthropicCompat | ProviderPreset::Anthropic => &[
            "claude-sonnet-5-5",
            "claude-haiku-5-5",
            "claude-opus-5-5",
            "claude-fable-5-1",
            "claude-sonnet-4-6",
            "claude-haiku-4-5",
            "other",
        ],
        ProviderPreset::Mistral => &[
            "mistral-small-2603",
            "mistral-medium-3-5",
            "mistral-large-4",
            "devstral-small-latest",
            "codestral-latest",
            "devstral-medium-latest",
            "mistral-small-latest",
            "mistral-large-latest",
            "other",
        ],
        // A custom gateway may use deployment IDs rather than upstream names.
        ProviderPreset::OpenAiCompatibleCustom => &["other"],
        ProviderPreset::HfLocal => &["local", "other"],
    }
}

pub(super) fn supports_temperature(provider: &ProviderKind, base_url: &str, model: &str) -> bool {
    let model = model.trim().to_ascii_lowercase();
    match detect_provider_preset(provider, base_url) {
        ProviderPreset::OpenAi | ProviderPreset::OpenAiCompatibleCustom
            if model.starts_with("gpt-5") || model.starts_with("gpt-6") =>
        {
            false
        }
        ProviderPreset::Anthropic
        | ProviderPreset::AnthropicCompat
        | ProviderPreset::OpenAiCompatibleCustom => ![
            "claude-sonnet-5-",
            "claude-haiku-5-",
            "claude-opus-5-",
            "claude-fable-5-",
        ]
        .iter()
        .any(|prefix| model.starts_with(prefix)),
        _ => true,
    }
}

/// GPT-5.6 Luna and GPT-6 Luna/Sol permit Chat Completions tools with reasoning off.
/// Use the same setting for Coder, Observer and Chat; do not spend a request on
/// a known-invalid max_tokens/temperature payload before retrying.
pub(crate) fn prepare_chat_payload(
    payload: &mut Value,
    provider: &ProviderKind,
    base_url: &str,
    model: &str,
) {
    if !supports_temperature(provider, base_url, model) {
        if let Some(obj) = payload.as_object_mut() {
            obj.remove("temperature");
            if model.trim().to_ascii_lowercase().starts_with("claude-") {
                obj.remove("top_p");
                obj.remove("top_k");
            }
        }
    }
    let model = model.trim().to_ascii_lowercase();
    if matches!(
        detect_provider_preset(provider, base_url),
        ProviderPreset::OpenAi | ProviderPreset::OpenAiCompatibleCustom
    ) && (model.starts_with("gpt-5") || model.starts_with("gpt-6"))
    {
        if let Some(obj) = payload.as_object_mut() {
            if let Some(limit) = obj.remove("max_tokens") {
                obj.entry("max_completion_tokens").or_insert(limit);
            }
        }
    }
    if matches!(
        detect_provider_preset(provider, base_url),
        ProviderPreset::OpenAi | ProviderPreset::OpenAiCompatibleCustom
    ) && ["gpt-5.6-luna", "gpt-6-luna", "gpt-6-sol"]
        .iter()
        .any(|id| model == *id || model.starts_with(&format!("{id}-")))
    {
        payload["reasoning_effort"] = json!("none");
    }
    // Large 4 rejects its default nucleus setting when temperature is zero.
    // Keep other models and non-greedy requests unchanged.
    if detect_provider_preset(provider, base_url) == ProviderPreset::Mistral
        && (model == "mistral-large-4" || model.starts_with("mistral-large-4-"))
        && payload.get("temperature").and_then(Value::as_f64) == Some(0.0)
    {
        payload["top_p"] = json!(1.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn luna_uses_the_same_tool_compatible_payload_with_or_without_tools() {
        for tools in [
            None,
            Some(json!([{"type":"function","function":{"name":"read_file"}}])),
        ] {
            let mut payload = json!({"model":"gpt-6-luna", "max_tokens":2048, "temperature":0.4});
            if let Some(tools) = tools {
                payload["tools"] = tools;
            }
            prepare_chat_payload(
                &mut payload,
                &ProviderKind::OpenAiCompatible,
                "https://api.openai.com/v1",
                "gpt-6-luna",
            );
            assert_eq!(payload["reasoning_effort"], "none");
            assert_eq!(payload["max_completion_tokens"], 2048);
            assert!(payload.get("max_tokens").is_none());
            assert!(payload.get("temperature").is_none());
        }
    }

    #[test]
    fn current_claude_sampling_is_omitted_on_native_and_compatible_routes() {
        for provider in [ProviderKind::Anthropic, ProviderKind::OpenAiCompatible] {
            for model in [
                "claude-sonnet-5-5",
                "claude-haiku-5-5",
                "claude-opus-5-5",
                "claude-fable-5-1",
            ] {
                assert!(!supports_temperature(
                    &provider,
                    "https://api.anthropic.com/v1",
                    model
                ));
            }
            assert!(supports_temperature(
                &provider,
                "https://api.anthropic.com/v1",
                "claude-sonnet-4-6"
            ));
        }
    }

    #[test]
    fn existing_weak_models_keep_their_request_parameters() {
        for (provider, base, model) in [
            (
                ProviderKind::Mistral,
                "https://api.mistral.ai/v1",
                "mistral-small-2603",
            ),
            (
                ProviderKind::OpenAiCompatible,
                "https://api.openai.com/v1",
                "gpt-4.1-mini",
            ),
            (
                ProviderKind::OpenAiCompatible,
                "https://generativelanguage.googleapis.com/v1beta/openai",
                "gemini-3.5-flash-lite",
            ),
        ] {
            let expected = json!({"model":model,"max_tokens":2048,"temperature":0.0});
            let mut actual = expected.clone();
            prepare_chat_payload(&mut actual, &provider, base, model);
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn large_four_greedy_sampling_sets_top_p_without_changing_other_temperatures() {
        for temperature in [0.0, 0.2] {
            let mut payload =
                json!({"model":"mistral-large-4", "temperature":temperature, "max_tokens":64});
            prepare_chat_payload(
                &mut payload,
                &ProviderKind::Mistral,
                "https://api.mistral.ai/v1",
                "mistral-large-4",
            );
            let mut expected =
                json!({"model":"mistral-large-4", "temperature":temperature, "max_tokens":64});
            if temperature == 0.0 {
                expected["top_p"] = json!(1.0);
            }
            assert_eq!(payload, expected);
        }
    }
}
