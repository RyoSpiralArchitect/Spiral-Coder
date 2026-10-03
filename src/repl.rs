use anyhow::{anyhow, Context, Result};
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;

use crate::chatbot::ChatBot;
use crate::config::{PartialConfig, ProviderKind};
use crate::modes::Mode;
use crate::personas;
use crate::providers;
use crate::types::ChatMessage;

pub async fn run(mut partial: PartialConfig) -> Result<()> {
    let client = reqwest::Client::new();
    let mut history: Vec<ChatMessage> = Vec::new();

    let mut rl = DefaultEditor::new().context("failed to initialize line editor")?;
    let history_path = std::path::PathBuf::from(".spiral_coder_history");
    let _ = rl.load_history(&history_path);

    println!("Spiral-Coder REPL");
    println!("  /help  show commands");
    println!("  /exit  quit");

    loop {
        let cfg = partial.clone().resolve()?;
        let prompt = format!(
            "spiral-coder[{}|{}|{}]> ",
            cfg.mode.label(),
            cfg.persona,
            cfg.provider
        );

        let line = match rl.readline(&prompt) {
            Ok(line) => line,
            Err(ReadlineError::Interrupted) => {
                eprintln!("(interrupted; use /exit to quit)");
                continue;
            }
            Err(ReadlineError::Eof) => break,
            Err(err) => return Err(anyhow!(err).context("readline failed")),
        };

        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        rl.add_history_entry(line).ok();

        if line.starts_with('/') {
            if handle_command(line, &mut partial, &mut history)? {
                break;
            }
            continue;
        }

        let user_msg = ChatMessage {
            role: "user".to_string(),
            content: line.to_string(),
        };

        let provider = providers::build_provider(client.clone(), &cfg);
        let bot = ChatBot::new(provider);

        match bot
            .run(
                &user_msg.content,
                &history,
                &cfg.mode,
                &cfg.persona,
                None,
                "brief",
                cfg.temperature,
                cfg.max_tokens,
                None,
                None,
            )
            .await
        {
            Ok(resp) => {
                println!("\n{}\n", resp.content);
                history.push(user_msg);
                history.push(ChatMessage {
                    role: "assistant".to_string(),
                    content: resp.content,
                });
            }
            Err(err) => {
                eprintln!("Error: {err}");
            }
        }
    }

    let _ = rl.save_history(&history_path);
    Ok(())
}

// Returns true if the REPL should exit.
fn handle_command(
    cmd: &str,
    partial: &mut PartialConfig,
    history: &mut Vec<ChatMessage>,
) -> Result<bool> {
    let mut parts = cmd.split_whitespace();
    let head = parts.next().unwrap_or("");
    let rest = parts.collect::<Vec<_>>().join(" ");

    match head {
        "/help" => {
            println!("Commands:");
            println!("  /help");
            println!("  /exit | /quit");
            println!("  /reset                    clear conversation history");
            println!("  /config                   show effective config (no API keys)");
            println!("  /vibe                     apply VIBE preset defaults");
            println!("  /provider <name>          openai-compatible | mistral | anthropic | hf");
            println!("  /model <name>");
            println!("  /chat-model <name>");
            println!("  /code-model <name>");
            println!("  /base-url <url>");
            println!("  /mode <name>              実況 | 壁打ち | diff批評 | VIBE | ログ解析");
            println!(
                "  /persona <name>           default | novelist | cynical | cheerful | thoughtful"
            );
            println!("  /temperature <0..2>");
            println!("  /max-tokens <n>");
        }
        "/exit" | "/quit" => return Ok(true),
        "/reset" => {
            history.clear();
            println!("(history cleared)");
        }
        "/config" => {
            let cfg = partial.clone().resolve()?;
            println!("provider        = {}", cfg.provider);
            println!("chat_model      = {}", cfg.chat_model);
            println!("code_model      = {}", cfg.code_model);
            println!("model(selected) = {}", cfg.model);
            println!("base_url        = {}", cfg.base_url);
            println!("mode            = {}", cfg.mode.label());
            println!("persona         = {}", cfg.persona);
            println!("temperature     = {}", cfg.temperature);
            println!("max_tokens      = {}", cfg.max_tokens);
            println!("timeout_seconds = {}", cfg.timeout_seconds);
        }
        "/vibe" => {
            partial.vibe = true;
            partial.provider = None;
            partial.model = None;
            partial.chat_model = None;
            partial.code_model = None;
            partial.base_url = None;
            partial.mode = None;
            println!("(VIBE preset enabled: provider=mistral, model=devstral-2, mode=VIBE)");
        }
        "/provider" => {
            let p = parse_provider(&rest)?;
            partial.provider = Some(p);
            partial.model = None;
            partial.chat_model = None;
            partial.code_model = None;
            partial.base_url = None;
            partial.api_key = None;
            println!("(provider set; model/base_url/api_key reset to defaults/env)");
        }
        "/model" => {
            if rest.trim().is_empty() {
                return Err(anyhow!("usage: /model <name>"));
            }
            partial.model = Some(rest);
        }
        "/chat-model" => {
            if rest.trim().is_empty() {
                return Err(anyhow!("usage: /chat-model <name>"));
            }
            partial.chat_model = Some(rest);
        }
        "/code-model" => {
            if rest.trim().is_empty() {
                return Err(anyhow!("usage: /code-model <name>"));
            }
            partial.code_model = Some(rest);
        }
        "/base-url" => {
            if rest.trim().is_empty() {
                return Err(anyhow!("usage: /base-url <url>"));
            }
            partial.base_url = Some(rest);
        }
        "/mode" => {
            let m = parse_mode(&rest)?;
            partial.mode = Some(m);
        }
        "/persona" => {
            let p = parse_persona(&rest)?;
            partial.persona = Some(p);
        }
        "/temperature" => {
            let v: f64 = rest.trim().parse().context("usage: /temperature <0..2>")?;
            partial.temperature = Some(v);
        }
        "/max-tokens" => {
            let v: u32 = rest.trim().parse().context("usage: /max-tokens <n>")?;
            partial.max_tokens = Some(v);
        }
        other => return Err(anyhow!("unknown command: {other} (try /help)")),
    }

    Ok(false)
}

fn parse_provider(s: &str) -> Result<ProviderKind> {
    let s = s.trim().to_ascii_lowercase();
    match s.as_str() {
        "openai-compatible" | "openai" | "openai_compat" => Ok(ProviderKind::OpenAiCompatible),
        "mistral" => Ok(ProviderKind::Mistral),
        "anthropic" => Ok(ProviderKind::Anthropic),
        "hf" | "huggingface" => Ok(ProviderKind::Hf),
        _ => Err(anyhow!(
            "unsupported provider: {s}. Available: openai-compatible, mistral, anthropic, hf"
        )),
    }
}

fn parse_mode(s: &str) -> Result<Mode> {
    let t = s.trim();
    if t.is_empty() {
        return Err(anyhow!("usage: /mode <name>"));
    }
    match t {
        "実況" | "jikkyo" | "live" => Ok(Mode::Jikkyo),
        "壁打ち" | "kabeuchi" | "ideation" => Ok(Mode::Kabeuchi),
        "diff批評" | "diff" | "review" => Ok(Mode::DiffReview),
        "VIBE" | "vibe" => Ok(Mode::Vibe),
        "ログ解析" | "log" | "analyze" => Ok(Mode::LogAnalysis),
        _ => Err(anyhow!(
            "unsupported mode: {t}. Available: 実況, 壁打ち, diff批評, VIBE, ログ解析"
        )),
    }
}

fn parse_persona(s: &str) -> Result<String> {
    let key = personas::normalize_persona(s);
    personas::resolve_persona(&key)?;
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_provider_accepts_aliases() {
        assert!(matches!(
            parse_provider("openai").unwrap(),
            ProviderKind::OpenAiCompatible
        ));
        assert!(matches!(
            parse_provider("openai_compat").unwrap(),
            ProviderKind::OpenAiCompatible
        ));
        assert!(matches!(
            parse_provider("mistral").unwrap(),
            ProviderKind::Mistral
        ));
        assert!(parse_provider("nope").is_err());
    }

    #[test]
    fn parse_mode_accepts_ja_and_aliases() {
        assert!(matches!(parse_mode("実況").unwrap(), Mode::Jikkyo));
        assert!(matches!(parse_mode("live").unwrap(), Mode::Jikkyo));
        assert!(matches!(parse_mode("壁打ち").unwrap(), Mode::Kabeuchi));
        assert!(matches!(parse_mode("diff").unwrap(), Mode::DiffReview));
        assert!(matches!(parse_mode("vibe").unwrap(), Mode::Vibe));
        assert!(parse_mode("").is_err());
    }

    #[test]
    fn handle_reset_clears_history() {
        let mut partial = PartialConfig::default();
        let mut history = vec![ChatMessage {
            role: "user".to_string(),
            content: "hi".to_string(),
        }];
        let exit = handle_command("/reset", &mut partial, &mut history).unwrap();
        assert!(!exit);
        assert!(history.is_empty());
    }

    #[test]
    fn handle_provider_resets_model_base_url_api_key() {
        let mut partial = PartialConfig::default();
        partial.model = Some("x".to_string());
        partial.chat_model = Some("chat".to_string());
        partial.code_model = Some("code".to_string());
        partial.base_url = Some("http://localhost:8000/v1".to_string());
        partial.api_key = Some("k".to_string());
        let mut history: Vec<ChatMessage> = Vec::new();

        handle_command("/provider mistral", &mut partial, &mut history).unwrap();
        assert!(matches!(partial.provider, Some(ProviderKind::Mistral)));
        assert!(partial.model.is_none());
        assert!(partial.chat_model.is_none());
        assert!(partial.code_model.is_none());
        assert!(partial.base_url.is_none());
        assert!(partial.api_key.is_none());
    }
}
