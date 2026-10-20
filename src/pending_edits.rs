use anyhow::{anyhow, Context, Result};
use serde::Serialize;
use std::path::{Component, Path};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct PendingEditStore {
    items: Arc<Mutex<Vec<PendingEditItem>>>,
    seq: Arc<AtomicU64>,
}

#[derive(Clone)]
struct PendingEditItem {
    id: String,
    action: String,
    status: String, // "pending" | "approved" | "rejected"
    path: String,   // relative path (as provided by client)
    preview: String,
    diff: String,
    created_at_ms: u128,

    // Internal payload (not sent to UI).
    content: String,
    original: Option<String>,
    budget: Option<crate::work_budget::WorkBudget>,
    message: String,
    result: Option<serde_json::Value>,
}

struct PreparedEdit {
    action: String,
    path: String,
    content: String,
    original: Option<String>,
    message: String,
}

fn read_optional(path: &Path) -> Result<Option<String>> {
    match std::fs::read_to_string(path) {
        Ok(content) => Ok(Some(content)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

#[derive(Serialize, Clone)]
pub struct PendingEditView {
    pub id: String,
    pub action: String,
    pub status: String,
    pub path: String,
    pub preview: String,
    pub diff: String,
    pub created_at: u128,
}

#[derive(Serialize)]
pub struct PendingEditResolveResponse {
    pub ok: bool,
    pub item: PendingEditResolvedItem,
}

#[derive(Serialize, Clone)]
pub struct PendingEditResolvedItem {
    pub id: String,
    pub action: String,
    pub status: String,
    pub path: String,
    pub result: Option<serde_json::Value>,
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn make_id(seq: u64) -> String {
    format!("pe_{seq}")
}

fn is_safe_rel_path(p: &Path) -> bool {
    if p.as_os_str().is_empty() {
        return false;
    }
    if p.is_absolute() {
        return false;
    }
    for c in p.components() {
        match c {
            Component::CurDir => {}
            Component::Normal(_) => {}
            // Reject ParentDir, RootDir, Prefix (e.g., C:\).
            _ => return false,
        }
    }
    true
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

fn simple_diff(old: &str, new: &str) -> String {
    // Minimal, safe diff: show a small head/tail of both sides.
    // (We avoid introducing a diff dependency here.)
    if old == new {
        return String::new();
    }
    let old_p = truncate_preview(old, 1200, 40);
    let new_p = truncate_preview(new, 1200, 40);
    format!("--- before ---\n{old_p}\n\n--- after ---\n{new_p}\n")
        .trim_end()
        .to_string()
}

impl PendingEditStore {
    /// Prepare a native edit without mutating the workspace. Approval commits
    /// this exact content only while the original file still matches.
    pub async fn queue_tool_edit(
        &self,
        workspace_root: &Path,
        request: &serde_json::Value,
    ) -> Result<(String, String)> {
        let field = |name: &str| {
            request[name]
                .as_str()
                .ok_or_else(|| anyhow!("missing {name}"))
        };
        let action = request["action"].as_str().unwrap_or("write_file");
        let path = field("path")?;
        let abs = crate::file_tools::resolve_safe_path(path, workspace_root.to_str())?;
        let path = abs
            .strip_prefix(workspace_root)?
            .to_string_lossy()
            .into_owned();
        let original = read_optional(&abs)?;
        let existing = || {
            original
                .as_deref()
                .ok_or_else(|| anyhow!("file does not exist: {path}"))
        };
        let (content, message) = match action {
            "write_file" => (
                field("content")?.to_string(),
                format!("OK: wrote '{path}' (approved edit)"),
            ),
            "patch_file" => (
                crate::file_tools::json_patch::prepare(&path, existing()?, request)?,
                format!("OK: patched '{path}' (approved edit)"),
            ),
            "apply_diff" => {
                crate::file_tools::edit_preview::diff(&path, existing()?, field("diff")?)?
            }
            _ => return Err(anyhow!("unsupported edit action: {action}")),
        };
        let id = self
            .queue_prepared(
                workspace_root,
                PreparedEdit {
                    action: action.into(),
                    path: path.clone(),
                    content,
                    original,
                    message,
                },
            )
            .await?;
        Ok((id, path))
    }

    pub fn new() -> Self {
        Self {
            items: Arc::new(Mutex::new(Vec::new())),
            seq: Arc::new(AtomicU64::new(1)),
        }
    }

    pub async fn list(&self) -> Vec<PendingEditView> {
        let items = self.items.lock().await;
        items
            .iter()
            .map(|it| PendingEditView {
                id: it.id.clone(),
                action: it.action.clone(),
                status: it.status.clone(),
                path: it.path.clone(),
                preview: it.preview.clone(),
                diff: it.diff.clone(),
                created_at: it.created_at_ms,
            })
            .collect()
    }

    pub async fn queue_write_file(
        &self,
        workspace_root: &Path,
        rel_path: &str,
        content: &str,
        action: &str,
    ) -> Result<String> {
        anyhow::ensure!(
            is_safe_rel_path(Path::new(rel_path)),
            "unsafe relative path: {rel_path}"
        );
        self.queue_prepared(
            workspace_root,
            PreparedEdit {
                action: action.into(),
                path: rel_path.into(),
                content: content.into(),
                original: read_optional(&workspace_root.join(rel_path))?,
                message: format!("OK: wrote '{rel_path}' ({} bytes)", content.len()),
            },
        )
        .await
    }

    async fn queue_prepared(&self, workspace_root: &Path, edit: PreparedEdit) -> Result<String> {
        let rel = Path::new(&edit.path);
        anyhow::ensure!(
            is_safe_rel_path(rel),
            "unsafe path (must be relative, no '..'): {}",
            edit.path
        );
        let abs = workspace_root.join(rel);
        let budget = crate::work_budget::current().map(|c| c.budget);
        if let Some(budget) = &budget {
            budget.check_write(&abs, &edit.content)?;
        }
        let preview = truncate_preview(&edit.content, 2200, 80);
        let diff = simple_diff(edit.original.as_deref().unwrap_or_default(), &edit.content);

        let seq = self.seq.fetch_add(1, Ordering::Relaxed);
        let id = make_id(seq);

        let mut items = self.items.lock().await;
        items.push(PendingEditItem {
            id: id.clone(),
            action: edit.action,
            status: "pending".to_string(),
            path: edit.path,
            preview,
            diff,
            created_at_ms: now_ms(),
            content: edit.content,
            original: edit.original,
            budget,
            message: edit.message,
            result: None,
        });

        Ok(id)
    }

    pub async fn reject(&self, id: &str) -> Result<PendingEditResolvedItem> {
        let mut items = self.items.lock().await;
        let it = items
            .iter_mut()
            .find(|x| x.id == id)
            .ok_or_else(|| anyhow!("pending edit not found: {id}"))?;

        it.status = "rejected".to_string();
        Ok(PendingEditResolvedItem {
            id: it.id.clone(),
            action: it.action.clone(),
            status: it.status.clone(),
            path: it.path.clone(),
            result: it.result.clone(),
        })
    }

    pub async fn approve(
        &self,
        workspace_root: &Path,
        id: &str,
    ) -> Result<PendingEditResolvedItem> {
        let mut items = self.items.lock().await;
        let it = items
            .iter_mut()
            .find(|x| x.id == id)
            .ok_or_else(|| anyhow!("pending edit not found: {id}"))?;

        if it.status != "pending" {
            return Ok(PendingEditResolvedItem {
                id: it.id.clone(),
                action: it.action.clone(),
                status: it.status.clone(),
                path: it.path.clone(),
                result: it.result.clone(),
            });
        }

        let rel = Path::new(&it.path);
        if !is_safe_rel_path(rel) {
            return Err(anyhow!(
                "unsafe path (must be relative, no '..'): {}",
                it.path
            ));
        }
        let abs = workspace_root.join(rel);
        let current = read_optional(&abs)?;
        anyhow::ensure!(
            current == it.original,
            "file changed since approval was requested; queue a fresh edit"
        );
        if let Some(budget) = &it.budget {
            budget.check_write(&abs, &it.content)?;
        }
        if let Some(parent) = abs.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create_dir_all failed: {}", parent.display()))?;
        }

        std::fs::write(&abs, it.content.as_bytes())
            .with_context(|| format!("write failed: {}", abs.display()))?;

        it.status = "approved".to_string();
        it.result = Some(serde_json::json!({
            "ok": true,
            "bytes_written": it.content.as_bytes().len(),
            "message": it.message,
        }));

        Ok(PendingEditResolvedItem {
            id: it.id.clone(),
            action: it.action.clone(),
            status: it.status.clone(),
            path: it.path.clone(),
            result: it.result.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn structured_json_edit_waits_for_approval_and_keeps_its_original_baseline() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("spec.json");
        let original = r#"{"cases":[{"id":"old","unknown":{"keep":true}}]}"#;
        std::fs::write(&path, original).unwrap();
        let store = PendingEditStore::new();
        let request = json!({"action":"patch_file","path":"spec.json","json_pointer":"/cases/-","json_value":"{\"id\":\"new\"}"});
        let (id, _) = store.queue_tool_edit(dir.path(), &request).await.unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
        std::fs::write(&path, r#"{"cases":[{"id":"concurrent"}]}"#).unwrap();
        assert!(store.approve(dir.path(), &id).await.is_err());
        std::fs::write(&path, original).unwrap();
        let receipt = store.approve(dir.path(), &id).await.unwrap();
        assert!(receipt.result.unwrap()["message"]
            .as_str()
            .unwrap()
            .starts_with("OK: patched '"));
        let actual: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(
            actual,
            json!({"cases":[{"id":"old","unknown":{"keep":true}},{"id":"new"}]})
        );
    }

    #[tokio::test]
    async fn all_native_edits_wait_for_approval_and_reject_stale_previews() {
        for action in ["write_file", "patch_file", "apply_diff"] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("note.txt");
            std::fs::write(&path, "old\n").unwrap();
            let store = PendingEditStore::new();
            let request = json!({"action":action,"path":path,"content":"new\n",
                "search":"old","replace":"new","diff":"@@ -1 +1 @@\n-old\n+new\n"});
            let (id, relative) = store.queue_tool_edit(dir.path(), &request).await.unwrap();
            assert_eq!(relative, "note.txt");
            assert_eq!(std::fs::read_to_string(&path).unwrap(), "old\n");
            std::fs::write(&path, "concurrent change\n").unwrap();
            assert!(store.approve(dir.path(), &id).await.is_err());
            assert_eq!(
                std::fs::read_to_string(&path).unwrap(),
                "concurrent change\n"
            );
            std::fs::write(&path, "old\n").unwrap();
            let receipt = store.approve(dir.path(), &id).await.unwrap();
            let result = receipt.result.unwrap();
            assert!(crate::execution_evidence::file_edit_succeeded(
                action,
                result["message"].as_str().unwrap()
            ));
            assert_eq!(std::fs::read_to_string(&path).unwrap(), "new\n");
        }
    }
}
