use anyhow::{anyhow, Result};
use serde::Serialize;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct PendingCommandStore {
    items: Arc<Mutex<Vec<PendingCommandItem>>>,
    seq: Arc<AtomicU64>,
}

#[derive(Clone)]
struct PendingCommandItem {
    id: String,
    status: String, // "pending" | "approved" | "rejected"
    command: String,
    cwd: Option<String>,
    preview: String,
    created_at_ms: u128,
    result: Option<serde_json::Value>,
}

#[derive(Serialize, Clone)]
pub struct PendingCommandView {
    pub id: String,
    pub status: String,
    pub command: String,
    pub cwd: Option<String>,
    pub preview: String,
    pub created_at: u128,
}

#[derive(Serialize)]
pub struct PendingCommandResolveResponse {
    pub ok: bool,
    pub item: PendingCommandResolvedItem,
}

#[derive(Serialize, Clone)]
pub struct PendingCommandResolvedItem {
    pub id: String,
    pub status: String,
    pub command: String,
    pub cwd: Option<String>,
    pub result: Option<serde_json::Value>,
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn make_id(seq: u64) -> String {
    format!("pc_{seq}")
}

fn truncate_preview(s: &str, max_chars: usize, max_lines: usize) -> String {
    let mut out = String::new();
    let mut lines = 0usize;
    for line in s.lines() {
        if lines >= max_lines {
            out.push_str("\n...truncated...\n");
            break;
        }
        if out.len() + line.len() + 1 > max_chars {
            out.push_str("\n...truncated...\n");
            break;
        }
        out.push_str(line);
        out.push('\n');
        lines += 1;
    }
    out.trim_end().to_string()
}

impl PendingCommandStore {
    pub fn new() -> Self {
        Self {
            items: Arc::new(Mutex::new(Vec::new())),
            seq: Arc::new(AtomicU64::new(1)),
        }
    }

    pub async fn list(&self) -> Vec<PendingCommandView> {
        let items = self.items.lock().await;
        items
            .iter()
            .map(|it| PendingCommandView {
                id: it.id.clone(),
                status: it.status.clone(),
                command: it.command.clone(),
                cwd: it.cwd.clone(),
                preview: it.preview.clone(),
                created_at: it.created_at_ms,
            })
            .collect()
    }

    pub async fn queue(&self, command: &str, cwd: Option<String>) -> Result<String> {
        let cmd = command.trim();
        if cmd.is_empty() {
            return Err(anyhow!("command is empty"));
        }

        let preview = truncate_preview(cmd, 600, 12);
        let seq = self.seq.fetch_add(1, Ordering::Relaxed);
        let id = make_id(seq);

        let mut items = self.items.lock().await;
        items.push(PendingCommandItem {
            id: id.clone(),
            status: "pending".to_string(),
            command: cmd.to_string(),
            cwd,
            preview,
            created_at_ms: now_ms(),
            result: None,
        });

        Ok(id)
    }

    pub async fn reject(&self, id: &str) -> Result<PendingCommandResolvedItem> {
        let mut items = self.items.lock().await;
        let it = items
            .iter_mut()
            .find(|x| x.id == id)
            .ok_or_else(|| anyhow!("pending command not found: {id}"))?;

        it.status = "rejected".to_string();
        Ok(PendingCommandResolvedItem {
            id: it.id.clone(),
            status: it.status.clone(),
            command: it.command.clone(),
            cwd: it.cwd.clone(),
            result: it.result.clone(),
        })
    }

    pub async fn approve(&self, id: &str) -> Result<PendingCommandResolvedItem> {
        // Don't hold the mutex across an `await` (commands can take a long time).
        let (command, cwd) = {
            let mut items = self.items.lock().await;
            let it = items
                .iter_mut()
                .find(|x| x.id == id)
                .ok_or_else(|| anyhow!("pending command not found: {id}"))?;

            if it.status != "pending" {
                return Ok(PendingCommandResolvedItem {
                    id: it.id.clone(),
                    status: it.status.clone(),
                    command: it.command.clone(),
                    cwd: it.cwd.clone(),
                    result: it.result.clone(),
                });
            }

            // Mark as approved first to prevent double-execution from concurrent UI clicks.
            it.status = "approved".to_string();
            (it.command.clone(), it.cwd.clone())
        };

        if let Some(cwd) = cwd.as_deref().filter(|s| !s.trim().is_empty()) {
            if let Err(err) = crate::exec::validate_cwd(cwd) {
                let res = serde_json::json!({
                    "ok": false,
                    "error": format!("invalid cwd: {err}"),
                });
                let mut items = self.items.lock().await;
                if let Some(it) = items.iter_mut().find(|x| x.id == id) {
                    it.result = Some(res);
                    return Ok(PendingCommandResolvedItem {
                        id: it.id.clone(),
                        status: it.status.clone(),
                        command: it.command.clone(),
                        cwd: it.cwd.clone(),
                        result: it.result.clone(),
                    });
                }
                return Err(anyhow!("pending command disappeared: {id}"));
            }
            if let Err(err) = std::fs::create_dir_all(std::path::Path::new(cwd)) {
                let res = serde_json::json!({
                    "ok": false,
                    "error": format!("invalid cwd (create_dir_all failed): {err}"),
                });
                let mut items = self.items.lock().await;
                if let Some(it) = items.iter_mut().find(|x| x.id == id) {
                    it.result = Some(res);
                    return Ok(PendingCommandResolvedItem {
                        id: it.id.clone(),
                        status: it.status.clone(),
                        command: it.command.clone(),
                        cwd: it.cwd.clone(),
                        result: it.result.clone(),
                    });
                }
                return Err(anyhow!("pending command disappeared: {id}"));
            }
        }

        let r = crate::exec::run_command(&command, cwd.as_deref()).await;
        let res = match r {
            Ok(r) => serde_json::json!({
                "ok": true,
                "stdout": r.stdout,
                "stderr": r.stderr,
                "exit_code": r.exit_code,
                "cwd": r.cwd,
            }),
            Err(e) => serde_json::json!({
                "ok": false,
                "stdout": "",
                "stderr": format!("spawn failed: {e:#}"),
                "exit_code": -1,
            }),
        };

        let mut items = self.items.lock().await;
        let it = items
            .iter_mut()
            .find(|x| x.id == id)
            .ok_or_else(|| anyhow!("pending command not found (after exec): {id}"))?;

        it.result = Some(res);
        Ok(PendingCommandResolvedItem {
            id: it.id.clone(),
            status: it.status.clone(),
            command: it.command.clone(),
            cwd: it.cwd.clone(),
            result: it.result.clone(),
        })
    }
}
