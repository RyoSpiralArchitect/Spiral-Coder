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
        // Meter at the provider boundary so Observer assists and their repair
        // calls cannot bypass the shared task cap through this non-HTTP path.
        let meter = crate::work_budget::begin(&payload)?;

        let mut child = Command::new(&self.python)
            .arg(&self.script_path)
            .kill_on_drop(true)
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

        if let Some(meter) = &meter {
            meter.status(&format!(
                "subprocess_exit_{}",
                out.status.code().unwrap_or(-1)
            ));
        }

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

        if let Some(meter) = &meter {
            if let Some(usage) = data.get("usage") {
                meter.usage(usage);
            }
            meter.status("completed");
        }

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

    #[cfg(unix)]
    #[tokio::test]
    async fn hf_observer_and_repair_reserve_before_subprocess_and_share_coder_cap() {
        let root = tempfile::tempdir().unwrap();
        let script = root.path().join("inference.sh");
        std::fs::write(&script, "cat >/dev/null\nprintf 'attempt\\n' >> \"$0.count\"\nprintf '%s\\n' '{\"content\":\"advice\",\"usage\":{\"completion_tokens\":3}}'\n").unwrap();
        let provider = HuggingFaceSubprocessProvider {
            model: "test".into(),
            device: "cpu".into(),
            local_only: true,
            timeout: Duration::from_secs(5),
            python: "sh".into(),
            script_path: script.clone(),
        };
        let path = root.path().join("work.json");
        let budget = crate::work_budget::WorkBudget::new("original task", 3, Some(path.clone()));
        drop(budget.begin("coder", &json!({})).unwrap());
        let request = ChatRequest {
            messages: vec![],
            temperature: None,
            max_tokens: None,
            metadata: None,
        };
        for _ in 0..2 {
            crate::work_budget::scope(
                Some(crate::work_budget::Context {
                    budget: budget.clone(),
                    role: "observer",
                }),
                async {
                    assert_eq!(provider.chat(&request).await.unwrap().content, "advice");
                },
            )
            .await;
        }
        crate::work_budget::scope(
            Some(crate::work_budget::Context {
                budget: budget.clone(),
                role: "observer",
            }),
            async {
                assert!(provider
                    .chat(&request)
                    .await
                    .unwrap_err()
                    .to_string()
                    .contains("budget exhausted"));
            },
        )
        .await;
        assert_eq!(
            std::fs::read_to_string(script.with_extension("sh.count"))
                .unwrap()
                .lines()
                .count(),
            2,
            "no process is launched after exhaustion"
        );
        let restored = crate::work_budget::WorkBudget::load("original task", path)
            .unwrap()
            .snapshot();
        assert_eq!(restored.requests.len(), 3);
        for receipt in &restored.requests[1..] {
            assert_eq!(receipt.role, "observer");
            assert_eq!(receipt.status, "completed");
            assert_eq!(receipt.provider_usage, Some(json!({"completion_tokens":3})));
            assert!(receipt.duration_ms.is_some());
        }
        assert!(restored.requests[0].provider_usage.is_none());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn failed_hf_subprocess_consumes_its_reserved_request() {
        let root = tempfile::tempdir().unwrap();
        let script = root.path().join("failure.sh");
        std::fs::write(&script, "cat >/dev/null\nexit 7\n").unwrap();
        let provider = HuggingFaceSubprocessProvider {
            model: "test".into(),
            device: "cpu".into(),
            local_only: true,
            timeout: Duration::from_secs(5),
            python: "sh".into(),
            script_path: script,
        };
        let budget = crate::work_budget::WorkBudget::new("task", 1, None);
        crate::work_budget::scope(
            Some(crate::work_budget::Context {
                budget: budget.clone(),
                role: "observer",
            }),
            async {
                let request = ChatRequest {
                    messages: vec![],
                    temperature: None,
                    max_tokens: None,
                    metadata: None,
                };
                assert!(provider.chat(&request).await.is_err());
                assert!(provider
                    .chat(&request)
                    .await
                    .unwrap_err()
                    .to_string()
                    .contains("budget exhausted"));
            },
        )
        .await;
        assert_eq!(budget.remaining(), 0);
        let receipt = &budget.snapshot().requests[0];
        assert_eq!(receipt.status, "subprocess_exit_7");
        assert!(receipt.duration_ms.is_some());
        assert!(receipt.provider_usage.is_none());
    }

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
