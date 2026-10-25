use super::*;
use crate::{config::ProviderKind, CliProviderPreset};
use clap::Parser;
use serde_json::json;

fn coder() -> RunConfig {
    RunConfig {
        provider: ProviderKind::OpenAiCompatible,
        model: "coder-code-model".into(),
        chat_model: "different-chat-model".into(),
        code_model: "coder-code-model".into(),
        api_key: Some("coder-secret".into()),
        base_url: "https://api.openai.com/v1".into(),
        mode: Mode::Vibe,
        persona: "default".into(),
        temperature: 0.4,
        max_tokens: 2048,
        timeout_seconds: 5,
        hf_device: "auto".into(),
        hf_local_only: true,
    }
}

fn mistral() -> ObserverArgs {
    ObserverArgs {
        provider: Some(CliProviderPreset::Mistral),
        model: Some("mistral-large-4".into()),
        ..Default::default()
    }
}

#[test]
fn defaults_keep_the_actual_coder_model_endpoint_and_credentials() {
    let cfg = coder();
    let roles = resolve_with_env(cfg.clone(), &ObserverArgs::default(), false, |_| {
        panic!("same-model defaults must not re-resolve credentials")
    })
    .unwrap();
    assert!(roles.same_model_both_roles());
    let observer = roles.observer.as_ref().unwrap();
    assert!(matches!(observer.mode, Mode::Observer));
    assert_eq!(observer.model, cfg.model);
    assert_eq!(observer.chat_model, cfg.chat_model);
    assert_eq!(observer.code_model, cfg.code_model);
    assert_eq!(observer.api_key, cfg.api_key);
    assert_eq!(
        serde_json::to_value(roles.receipts()).unwrap(),
        json!({
            "coder":{"provider":"openai","transport":"openai-compatible","model":"coder-code-model",
                "endpoint_origin":"https://api.openai.com","temperature":0.4,"max_tokens":2048,"timeout_seconds":5},
            "observer":{"provider":"openai","transport":"openai-compatible","model":"coder-code-model",
                "endpoint_origin":"https://api.openai.com","temperature":0.2,"max_tokens":1200,"timeout_seconds":5}
        })
    );
}

#[test]
fn cross_family_resolution_uses_only_the_observers_credential_source() {
    let roles = resolve_with_env(coder(), &mistral(), false, |name| {
        assert_eq!(name, "MISTRAL_API_KEY");
        Some("observer-secret".into())
    })
    .unwrap();
    assert!(!roles.same_model_both_roles());
    let observer = roles.observer.unwrap();
    assert_eq!(observer.provider, ProviderKind::Mistral);
    assert_eq!(observer.model, "mistral-large-4");
    assert_eq!(observer.chat_model, observer.model);
    assert_eq!(observer.code_model, observer.model);
    assert_eq!(observer.api_key.as_deref(), Some("observer-secret"));
    assert_eq!(observer.base_url, "https://api.mistral.ai/v1");
    // An available generic/Coder key must not rescue a missing Observer key.
    assert!(resolve_with_env(coder(), &mistral(), false, |name| {
        (name != "MISTRAL_API_KEY").then(|| "coder-secret".into())
    })
    .is_err());
}

#[test]
fn explicit_key_name_and_gemini_aliases_do_not_fall_back_to_other_providers() {
    let mut args = ObserverArgs {
        provider: Some(CliProviderPreset::Gemini),
        model: Some("gemini-4-argon".into()),
        ..Default::default()
    };
    let roles = resolve_with_env(coder(), &args, false, |name| match name {
        "GEMINI_API_KEY" => Some("  ".into()),
        "GOOGLE_API_KEY" => Some("gemini-secret".into()),
        other => panic!("unexpected credential lookup: {other}"),
    })
    .unwrap();
    assert_eq!(
        roles.observer.unwrap().api_key.as_deref(),
        Some("gemini-secret")
    );
    args.api_key_env = Some("CUSTOM_OBSERVER_KEY".into());
    args.base_url = Some("http://localhost:1234/v1".into());
    let roles = resolve_with_env(coder(), &args, false, |name| {
        assert_eq!(name, "CUSTOM_OBSERVER_KEY");
        Some("custom-secret".into())
    })
    .unwrap();
    assert_eq!(
        roles.observer.unwrap().api_key.as_deref(),
        Some("custom-secret")
    );
    assert!(resolve_with_env(coder(), &args, false, |_| None).is_err());
}

#[test]
fn incomplete_and_unsafe_role_configuration_is_rejected_without_environment_lookup() {
    let mut variants = vec![
        ObserverArgs {
            provider: Some(CliProviderPreset::Mistral),
            ..Default::default()
        },
        ObserverArgs {
            model: Some("model".into()),
            ..Default::default()
        },
        ObserverArgs {
            model: Some("  ".into()),
            ..mistral()
        },
        ObserverArgs {
            base_url: Some("https://custom.example/v1".into()),
            ..mistral()
        },
        ObserverArgs {
            base_url: Some("  ".into()),
            ..mistral()
        },
        ObserverArgs {
            api_key_env: Some("bad=name".into()),
            ..mistral()
        },
        ObserverArgs {
            provider: Some(CliProviderPreset::OpenAiCompatible),
            ..mistral()
        },
    ];
    for url in [
        "file:///tmp/model",
        "https://secret@api.example/v1",
        "https://api.example/v1?key=secret",
        "https://api.example/v1#secret",
    ] {
        variants.push(ObserverArgs {
            base_url: Some(url.into()),
            api_key_env: Some("KEY".into()),
            ..mistral()
        });
    }
    for args in variants {
        assert!(
            resolve_with_env(coder(), &args, false, |_| panic!(
                "invalid role reached credential lookup"
            ))
            .is_err(),
            "{args:?}"
        );
    }
    assert!(resolve_with_env(coder(), &mistral(), true, |_| panic!(
        "disabled role looked up credentials"
    ))
    .is_err());
    let roles =
        resolve_with_env(coder(), &ObserverArgs::default(), true, |_| unreachable!()).unwrap();
    assert!(!roles.same_model_both_roles());
    assert!(roles.observer.is_none());
}

#[test]
fn receipt_redacts_url_path_and_credentials_and_identity_keeps_the_full_endpoint() {
    let mut cfg = coder();
    cfg.base_url = "https://login:password@api.example/private-secret?api_key=query-secret".into();
    let mut roles =
        resolve_with_env(cfg, &ObserverArgs::default(), false, |_| unreachable!()).unwrap();
    let receipt = serde_json::to_string(&roles.receipts()).unwrap();
    for secret in [
        "login",
        "password",
        "private-secret",
        "query-secret",
        "coder-secret",
    ] {
        assert!(!receipt.contains(secret));
    }
    assert!(receipt.contains("https://api.example"));
    roles.observer.as_mut().unwrap().base_url = "https://api.example/other".into();
    assert!(!roles.same_model_both_roles());
}

#[test]
fn cli_observer_flags_do_not_collide_with_global_coder_flags() {
    let base = [
        "spiral-coder",
        "--provider",
        "openai",
        "--model",
        "gpt-5.6-luna",
        "coupled-run",
        "--tool-root",
        "/tmp/workspace",
        "--prompt-file",
        "/tmp/task",
        "--out-dir",
        "/tmp/run",
    ];
    let mut argv = base.to_vec();
    argv.extend([
        "--observer-provider",
        "mistral",
        "--observer-model",
        "mistral-large-4",
        "--coder-chunk-requests",
        "4",
    ]);
    let cli = crate::Cli::try_parse_from(argv).unwrap();
    assert!(matches!(
        cli.common.provider,
        Some(CliProviderPreset::OpenAi)
    ));
    assert_eq!(cli.common.model.as_deref(), Some("gpt-5.6-luna"));
    let mut incomplete = base.to_vec();
    incomplete.extend(["--observer-provider", "mistral"]);
    assert!(crate::Cli::try_parse_from(incomplete).is_err());
}
