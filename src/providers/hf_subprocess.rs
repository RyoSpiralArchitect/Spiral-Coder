use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use crate::types::{ChatRequest, ChatResponse};

use super::ChatProvider;

pub struct HuggingFaceSubprocessProvider {
    model: String,
    device: String,
    local_only: bool,
    timeout: Duration,
    python: String,
    script_path: PathBuf,
}

impl HuggingFaceSubprocessProvider {
    pub fn new(model: String, device: String, local_only: bool, timeout: Duration) -> Self {
        let python =
            std::env::var("SPIRAL_CODER_HF_PYTHON").unwrap_or_else(|_| "python".to_string());
        let script_path = PathBuf::from("scripts").join("hf_infer.py");
        Self {
            model,
            device,
            local_only,
            timeout,
            python,
            script_path,
        }
    }
}

#[async_trait]
impl ChatProvider for HuggingFaceSubprocessProvider {
    async fn chat(&self, request: &ChatRequest) -> Result<ChatResponse> {
        let payload = json!({
            "model": self.model,
            "messages": request.messages,
            "max_new_tokens": request.max_tokens.unwrap_or(256),
            "temperature": request.temperature.unwrap_or(0.4),
            "device": self.device,
            "local_only": self.local_only,
        });
        let input = serde_json::to_vec(&payload).context("failed to serialize request")?;

        let mut child = Command::new(&self.python)
            .arg(&self.script_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("failed to spawn hf subprocess")?;

        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("failed to open stdin"))?;
        stdin
            .write_all(&input)
            .await
            .context("failed to write request to hf subprocess")?;
        stdin
            .write_all(b"\n")
            .await
            .context("failed to write newline to hf subprocess")?;
        drop(stdin);

        let out = tokio::time::timeout(self.timeout, child.wait_with_output())
            .await
            .context("hf subprocess timed out")?
            .context("failed to wait for hf subprocess")?;

        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            return Err(anyhow!("hf subprocess failed: {stderr}"));
        }

        let data: Value =
            serde_json::from_slice(&out.stdout).context("invalid JSON from hf subprocess")?;
        let content = data
            .get("content")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();
        let model = data
            .get("model")
            .and_then(|x| x.as_str())
            .unwrap_or(&self.model)
            .to_string();

        Ok(ChatResponse {
            content,
            model,
            raw: Some(data),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ChatMessage;

    // Requires a Python environment with `transformers` installed and a valid
    // local model. Run with: cargo test -- --ignored hf_subprocess
    #[tokio::test]
    #[ignore]
    async fn hf_subprocess_roundtrip() {
        let provider = HuggingFaceSubprocessProvider::new(
            "gpt2".to_string(),
            "cpu".to_string(),
            false,
            Duration::from_secs(120),
        );

        let request = ChatRequest {
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: "Hello, world!".to_string(),
            }],
            temperature: Some(0.4),
            max_tokens: Some(32),
            metadata: None,
        };

        let resp = provider.chat(&request).await.unwrap();
        assert!(
            !resp.content.is_empty(),
            "expected non-empty content from HF subprocess"
        );
    }
}
