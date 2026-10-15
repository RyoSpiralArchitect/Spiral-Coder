//! Durable server-side budget for browser tasks. The client cannot refund wire
//! retries by reloading or resend a different limit for an existing task ID.
use crate::work_budget::{Context, WorkBudget};
use anyhow::{Context as _, Result};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

#[derive(Deserialize)]
struct Request {
    task_id: String,
    root: String,
    limit: usize,
    role: String,
    tool_root: Option<String>,
}
static BUDGETS: OnceLock<Mutex<BTreeMap<std::path::PathBuf, WorkBudget>>> = OnceLock::new();

pub(super) fn from_body(body: &[u8], workspace: &Path) -> Result<Option<Context>> {
    let value: serde_json::Value = serde_json::from_slice(body).unwrap_or_default();
    let Some(raw) = value.get("work_budget") else {
        return Ok(None);
    };
    let req: Request = serde_json::from_value(raw.clone()).context("invalid work budget")?;
    anyhow::ensure!(
        !req.task_id.is_empty()
            && req.task_id.len() <= 100
            && req
                .task_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'),
        "invalid task id"
    );
    anyhow::ensure!((1..=64).contains(&req.limit), "invalid request limit");
    let role = match req.role.as_str() {
        "coder" => "coder",
        "observer" => "observer",
        "observer_repair" => "observer_repair",
        _ => anyhow::bail!("invalid work role"),
    };
    let path = workspace
        .join(".spiral-coder/web-budgets")
        .join(format!("{}.json", req.task_id));
    let mut budgets = BUDGETS.get_or_init(Default::default).lock().unwrap();
    let budget = if let Some(budget) = budgets.get(&path) {
        budget.clone()
    } else {
        let budget = if path.exists() {
            WorkBudget::load(&req.root, path.clone())
                .context("saved budget cannot be restored; do not reset it")?
        } else {
            WorkBudget::new(&req.root, req.limit, Some(path.clone()))
        };
        budgets.insert(path, budget.clone());
        budget
    };
    let saved = budget.snapshot();
    anyhow::ensure!(
        saved.root == req.root && saved.request_limit == req.limit,
        "task budget identity changed"
    );
    let root = req.tool_root.as_deref().map(Path::new).unwrap_or(workspace);
    budget.protect_replay(&workspace.join(root))?;
    Ok(Some(Context { budget, role }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn browser_reload_cannot_refund_budget_or_change_workspace() {
        let workspace = tempfile::tempdir().unwrap();
        let request =
            json!({"work_budget":{"task_id":"reload-test","root":"task","limit":1,"role":"coder"}});
        let context = from_body(&serde_json::to_vec(&request).unwrap(), workspace.path())
            .unwrap()
            .unwrap();
        drop(context.budget.begin("coder", &json!({})).unwrap());
        let resumed = from_body(&serde_json::to_vec(&request).unwrap(), workspace.path())
            .unwrap()
            .unwrap();
        assert_eq!(resumed.budget.remaining(), 0);
        assert!(resumed.budget.begin("observer", &json!({})).is_err());
        let mut changed = request.clone();
        changed["work_budget"]["limit"] = json!(64);
        assert!(from_body(&serde_json::to_vec(&changed).unwrap(), workspace.path()).is_err());
        let other = tempfile::tempdir().unwrap();
        changed = request;
        changed["work_budget"]["tool_root"] = json!(other.path());
        assert!(from_body(&serde_json::to_vec(&changed).unwrap(), workspace.path()).is_err());
    }
}
