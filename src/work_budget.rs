//! Durable task and transport accounting. Every wire attempt has one receipt.
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

mod allocation;
pub(crate) use allocation::{BudgetPolicy, RequestCharge, TransportRejection};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RequestReceipt {
    pub id: usize,
    pub role: String,
    pub estimated_input_tokens: usize,
    pub provider_usage: Option<Value>,
    pub duration_ms: Option<u128>,
    pub status: String,
    #[serde(default)]
    pub charge: RequestCharge,
    #[serde(default)]
    pub transport_rejection: Option<TransportRejection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub purpose: Option<RequestPurpose>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_of: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream_interruption: Option<StreamInterruption>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RequestPurpose {
    ReportCloseout,
    ObserverReconnect,
}

/// Only native stream readers assign these causes. Error text and broad
/// `stream_interrupted` statuses cannot authorize reconnection on their own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum StreamInterruption {
    MissingTerminalFrame,
    ReadFailed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AssistReceipt {
    pub assist_id: u64,
    pub continuation_id: u64,
    pub decision: String,
    pub call_ids: Vec<String>,
    pub executed: bool,
    pub succeeded: Option<bool>,
    pub completed: Option<bool>,
    pub pending_exact_checks: Vec<String>,
    /// Exact suggested-action evidence, distinct from the continuation totals.
    /// Historical receipts and suggestions without a tool have no projection.
    #[serde(default)]
    pub suggestion_execution: Option<SuggestedActionReceipt>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct SuggestedActionReceipt {
    pub tool: String,
    pub arguments: Value,
    pub matching_call_ids: Vec<String>,
    /// None when matching calls exist but execution is still unconfirmed.
    pub executed: Option<bool>,
    /// Aggregate only confirmed executed matches, never completion proof.
    pub succeeded: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct WorkState {
    pub task_id: String,
    #[serde(default)]
    pub persistence_revision: u64,
    pub root: String,
    #[serde(default)]
    pub tool_root: Option<PathBuf>,
    pub started_at_ms: u128,
    pub request_limit: usize,
    #[serde(default)]
    pub policy: BudgetPolicy,
    pub requests: Vec<RequestReceipt>,
    pub assists: Vec<AssistReceipt>,
    #[serde(default)]
    pub protected_artifacts: std::collections::BTreeMap<PathBuf, Value>,
}

#[derive(Debug, Clone)]
pub(crate) struct WorkBudget {
    state: Arc<Mutex<WorkState>>,
    path: Option<PathBuf>,
    claimed: Arc<AtomicBool>,
    // Per-handle ceiling for one role's slice, never a change to durable cap.
    request_ceiling: Option<usize>,
    // Transient role-slice policy; not new durable requests or task authority.
    return_on_verification_failure: bool,
    // Per-handle wire ceiling includes transport reclassification/API retries.
    wire_ceiling: Option<usize>,
    purpose: Option<RequestPurpose>,
    retry_of: Option<usize>,
}

static NEXT_TASK: AtomicU64 = AtomicU64::new(0);
static PERSIST_LOCK: Mutex<()> = Mutex::new(());

impl Default for WorkBudget {
    fn default() -> Self {
        Self::new("", 32, None)
    }
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

impl WorkBudget {
    pub fn new(root: &str, request_limit: usize, path: Option<PathBuf>) -> Self {
        Self {
            state: Arc::new(Mutex::new(WorkState {
                task_id: format!(
                    "task-{}-{}-{}",
                    now_ms(),
                    std::process::id(),
                    NEXT_TASK.fetch_add(1, Ordering::Relaxed)
                ),
                persistence_revision: 0,
                root: root.into(),
                tool_root: None,
                started_at_ms: now_ms(),
                request_limit,
                policy: BudgetPolicy::default(),
                requests: Vec::new(),
                assists: Vec::new(),
                protected_artifacts: Default::default(),
            })),
            path,
            claimed: Arc::new(AtomicBool::new(false)),
            request_ceiling: None,
            return_on_verification_failure: false,
            wire_ceiling: None,
            purpose: None,
            retry_of: None,
        }
    }

    pub fn load(root: &str, path: PathBuf) -> Option<Self> {
        let state: WorkState = serde_json::from_slice(&std::fs::read(&path).ok()?).ok()?;
        (state.root == root).then(|| Self {
            state: Arc::new(Mutex::new(state)),
            path: Some(path),
            claimed: Arc::new(AtomicBool::new(true)),
            request_ceiling: None,
            return_on_verification_failure: false,
            wire_ceiling: None,
            purpose: None,
            retry_of: None,
        })
    }

    pub fn protect_replay(&self, root: &std::path::Path) -> Result<()> {
        let mut state = self.state.lock().unwrap();
        let root = root.canonicalize()?;
        if let Some(saved) = &state.tool_root {
            anyhow::ensure!(*saved == root, "task workspace changed; start a new task");
        }
        state.tool_root = Some(root.clone());
        self.persist(&mut state)?;
        let text = state.root.to_ascii_lowercase();
        if !(text.contains("tui_replay.json")
            && text.contains("preserve")
            && text.contains("existing case"))
        {
            return Ok(());
        }
        let path = root.join(".spiral-coder/tui_replay.json");
        if path.exists() && !state.protected_artifacts.contains_key(&path) {
            let value: Value = serde_json::from_slice(&std::fs::read(&path)?)?;
            // Capture only the explicit append-only replay obligation.
            if value["cases"].is_array() {
                state.protected_artifacts.insert(path, value);
            }
            self.persist(&mut state)?;
        }
        Ok(())
    }

    pub fn check_write(&self, path: &std::path::Path, body: &str) -> Result<()> {
        let state = self.state.lock().unwrap();
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        if let Some(baseline) = state.protected_artifacts.iter().find_map(|(key, value)| {
            (key.canonicalize().unwrap_or_else(|_| key.clone()) == canonical).then_some(value)
        }) {
            let candidate: Value = serde_json::from_str(body)?;
            crate::artifact_preservation::check(baseline, &candidate)?;
        }
        Ok(())
    }

    pub fn preservation_issue(&self) -> Option<String> {
        let baselines = self.snapshot().protected_artifacts;
        for (path, baseline) in baselines {
            let result = std::fs::read(&path)
                .map_err(anyhow::Error::from)
                .and_then(|body| {
                    serde_json::from_slice::<Value>(&body).map_err(anyhow::Error::from)
                })
                .and_then(|value| crate::artifact_preservation::check(&baseline, &value));
            if let Err(error) = result {
                return Some(format!("{}: {error}", path.display()));
            }
        }
        None
    }

    pub fn snapshot(&self) -> WorkState {
        self.state.lock().unwrap().clone()
    }
    pub fn remaining(&self) -> usize {
        let s = self.state.lock().unwrap();
        s.request_limit.saturating_sub(s.task_requests())
    }

    pub(crate) fn with_request_allowance(&self, allowance: usize) -> Self {
        let mut bounded = self.clone();
        let ceiling = self.snapshot().task_requests().saturating_add(allowance);
        bounded.request_ceiling =
            Some(self.request_ceiling.map_or(ceiling, |old| old.min(ceiling)));
        bounded
    }

    pub(crate) fn with_observer_return(&self, allowance: usize) -> Self {
        let mut bounded = self.with_request_allowance(allowance);
        bounded.return_on_verification_failure = true;
        bounded
    }

    pub(crate) fn can_return_to_observer(&self) -> bool {
        self.return_on_verification_failure
            && self.remaining_for("observer")
                >= if self.snapshot().policy.observer_reserved_requests > 0 {
                    1
                } else {
                    2
                }
    }

    fn with_one_wire_attempt(&self) -> Self {
        let mut bounded = self.with_request_allowance(1);
        let ceiling = self.snapshot().requests.len().saturating_add(1);
        bounded.wire_ceiling = Some(self.wire_ceiling.map_or(ceiling, |old| old.min(ceiling)));
        bounded.return_on_verification_failure = false;
        bounded
    }

    pub(crate) fn with_report_closeout(&self) -> Self {
        let mut bounded = self.with_one_wire_attempt();
        bounded.purpose = Some(RequestPurpose::ReportCloseout);
        bounded.retry_of = None;
        bounded
    }

    pub(crate) fn with_observer_reconnect(&self, interrupted_request: usize) -> Self {
        let mut bounded = self.with_one_wire_attempt();
        bounded.purpose = Some(RequestPurpose::ObserverReconnect);
        bounded.retry_of = Some(interrupted_request);
        bounded
    }

    pub(crate) fn is_report_closeout(&self) -> bool {
        self.purpose == Some(RequestPurpose::ReportCloseout)
    }

    fn persist(&self, state: &mut WorkState) -> Result<()> {
        if let Some(path) = &self.path {
            let _write = PERSIST_LOCK.lock().unwrap();
            if self.claimed.load(Ordering::Relaxed) {
                let current: WorkState = serde_json::from_slice(&std::fs::read(path)?)?;
                anyhow::ensure!(
                    current.task_id == state.task_id
                        && current.persistence_revision == state.persistence_revision,
                    "task budget was replaced; stale request cannot persist or continue"
                );
            }
            state.persistence_revision += 1;
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let tmp = path.with_extension("json.tmp");
            std::fs::write(&tmp, serde_json::to_vec_pretty(state)?)?;
            std::fs::rename(tmp, path)?;
            self.claimed.store(true, Ordering::Relaxed);
        }
        Ok(())
    }

    pub fn record_assist(&self, receipt: AssistReceipt) -> Result<()> {
        let mut state = self.state.lock().unwrap();
        state.assists.push(receipt);
        self.persist(&mut state)
    }

    pub fn begin(&self, role: &str, payload: &Value) -> Result<RequestGuard> {
        let mut state = self.state.lock().unwrap();
        if state.task_requests() >= state.request_limit
            || state.requests.len()
                >= state
                    .request_limit
                    .saturating_add(state.policy.transport_repair_limit)
        {
            bail!("Shared task request budget exhausted; work remains incomplete. Send a new scoped task to allocate another budget.");
        }
        if (role == "coder" && state.coder_remaining() == 0)
            || self
                .request_ceiling
                .is_some_and(|ceiling| state.task_requests() >= ceiling)
            || self
                .wire_ceiling
                .is_some_and(|ceiling| state.requests.len() >= ceiling)
        {
            return Err(ChunkExhausted.into());
        }
        let id = state.requests.len();
        state.requests.push(RequestReceipt {
            id,
            role: role.into(),
            estimated_input_tokens: payload.to_string().chars().count().div_ceil(4),
            provider_usage: None,
            duration_ms: None,
            status: "in_flight_or_interrupted".into(),
            charge: RequestCharge::Task,
            transport_rejection: None,
            purpose: self.purpose,
            retry_of: self.retry_of,
            stream_interruption: None,
        });
        // Reserve durably before any network I/O, including compatibility retries.
        self.persist(&mut state)?;
        Ok(RequestGuard {
            budget: self.clone(),
            id,
            started: Instant::now(),
        })
    }
}

#[derive(Debug)]
pub(crate) struct ChunkExhausted;
impl std::fmt::Display for ChunkExhausted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(
            "Coder request chunk exhausted; shared requests remain for Observer and continuation",
        )
    }
}
impl std::error::Error for ChunkExhausted {}

pub(crate) struct RequestGuard {
    budget: WorkBudget,
    id: usize,
    started: Instant,
}
impl RequestGuard {
    pub fn interrupted(&self, cause: StreamInterruption) {
        let mut state = self.budget.state.lock().unwrap();
        state.requests[self.id].status = "stream_interrupted".into();
        state.requests[self.id].stream_interruption = Some(cause);
        let _ = self.budget.persist(&mut state);
    }

    pub fn usage(&self, usage: &Value) {
        let mut state = self.budget.state.lock().unwrap();
        state.requests[self.id].provider_usage = Some(usage.clone());
        let _ = self.budget.persist(&mut state);
    }
    pub fn status(&self, status: &str) {
        let mut state = self.budget.state.lock().unwrap();
        state.requests[self.id].status = status.into();
        let _ = self.budget.persist(&mut state);
    }
}
impl Drop for RequestGuard {
    fn drop(&mut self) {
        let mut state = self.budget.state.lock().unwrap();
        state.requests[self.id].duration_ms = Some(self.started.elapsed().as_millis());
        let _ = self.budget.persist(&mut state);
    }
}

#[derive(Clone)]
pub(crate) struct Context {
    pub budget: WorkBudget,
    pub role: &'static str,
}
tokio::task_local! { static CURRENT: Context; }
pub(crate) fn current() -> Option<Context> {
    CURRENT.try_with(Clone::clone).ok()
}
pub(crate) async fn scope<F: std::future::Future>(
    context: Option<Context>,
    future: F,
) -> F::Output {
    match context {
        Some(context) => CURRENT.scope(context, future).await,
        None => future.await,
    }
}
pub(crate) fn begin(payload: &Value) -> Result<Option<RequestGuard>> {
    current()
        .map(|c| c.budget.begin(c.role, payload))
        .transpose()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn report_closeout_has_one_wire_attempt_even_after_transport_reclassification() {
        let budget = WorkBudget::new("task", 10, None);
        budget
            .configure(BudgetPolicy {
                observer_reserved_requests: 3,
                transport_repair_limit: 4,
            })
            .unwrap();
        let bounded = budget.with_report_closeout();
        let attempt = bounded.begin("coder", &Value::Null).unwrap();
        attempt.status("completed");
        drop(attempt);
        bounded
            .reject_transport("coder", 0, TransportRejection::NativeArguments)
            .unwrap();
        assert_eq!(bounded.slice_remaining("coder"), 0);
        assert!(bounded
            .with_report_closeout()
            .begin("coder", &Value::Null)
            .err()
            .unwrap()
            .is::<ChunkExhausted>());
        assert_eq!(budget.remaining_for("coder"), 7);
        assert_eq!(budget.snapshot().request_limit, 10);
        let receipt = serde_json::to_value(&budget.snapshot().requests[0]).unwrap();
        assert_eq!(receipt["purpose"], "report_closeout");
        assert_eq!(receipt["charge"], "transport");
        assert!(!budget.is_report_closeout());
    }

    #[test]
    fn reconnect_wire_cap_and_receipt_roundtrip_keep_old_receipts_unclassified() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("work.json");
        let budget = WorkBudget::new("task", 10, Some(path.clone()));
        budget
            .configure(BudgetPolicy {
                observer_reserved_requests: 3,
                transport_repair_limit: 4,
            })
            .unwrap();
        let original = budget.begin("observer", &Value::Null).unwrap();
        original.interrupted(StreamInterruption::MissingTerminalFrame);
        drop(original);
        let bounded = budget.with_observer_reconnect(0);
        let attempt = bounded.begin("observer", &Value::Null).unwrap();
        attempt.status("completed");
        drop(attempt);
        bounded
            .reject_transport("observer", 1, TransportRejection::ObserverEnvelope)
            .unwrap();
        assert_eq!(bounded.slice_remaining("observer"), 0);
        assert!(bounded
            .begin("observer", &Value::Null)
            .err()
            .unwrap()
            .is::<ChunkExhausted>());
        assert!(!bounded.is_report_closeout());
        let saved: WorkState = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(
            saved.requests[0].stream_interruption,
            Some(StreamInterruption::MissingTerminalFrame)
        );
        assert_eq!(saved.requests[0].charge, RequestCharge::Task);
        assert_eq!(saved.requests[1].retry_of, Some(0));
        assert_eq!(
            saved.requests[1].purpose,
            Some(RequestPurpose::ObserverReconnect)
        );
        let mut old = serde_json::to_value(&saved.requests[0]).unwrap();
        old.as_object_mut().unwrap().remove("stream_interruption");
        let old: RequestReceipt = serde_json::from_value(old).unwrap();
        assert_eq!(old.stream_interruption, None);
        assert_eq!(old.retry_of, None);
        assert_eq!(old.purpose, None);
    }

    #[tokio::test]
    async fn role_allowance_counts_retries_without_changing_the_shared_cap() {
        let budget = WorkBudget::new("task", 6, None);
        let chunk = budget.with_request_allowance(4);
        scope(
            Some(Context {
                budget: chunk,
                role: "coder",
            }),
            async {
                for _ in 0..4 {
                    drop(begin(&Value::Null).unwrap());
                }
                assert!(begin(&Value::Null).err().unwrap().is::<ChunkExhausted>());
            },
        )
        .await;
        assert_eq!(budget.remaining(), 2);
        assert_eq!(budget.snapshot().request_limit, 6);
        drop(budget.begin("observer", &Value::Null).unwrap());
        drop(budget.begin("coder", &Value::Null).unwrap());
        assert_eq!(budget.remaining(), 0);
        assert!(budget.begin("observer", &Value::Null).is_err());
        assert_eq!(
            budget
                .snapshot()
                .requests
                .iter()
                .map(|r| r.role.as_str())
                .collect::<Vec<_>>(),
            ["coder", "coder", "coder", "coder", "observer", "coder"]
        );
    }

    #[test]
    fn canceled_request_cannot_overwrite_a_replacement_tasks_budget() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("work.json");
        let old = WorkBudget::new("old", 4, Some(path.clone()));
        let pending = old.begin("coder", &serde_json::json!({})).unwrap();
        let new = WorkBudget::new("new", 2, Some(path.clone()));
        drop(new.begin("coder", &serde_json::json!({})).unwrap());
        pending.usage(&serde_json::json!({"prompt_tokens":100}));
        drop(pending);
        assert!(old.begin("coder", &Value::Null).is_err());
        let restored = WorkBudget::load("new", path).unwrap();
        assert_eq!(restored.remaining(), 1);
        assert_eq!(restored.snapshot().task_id, new.snapshot().task_id);
        assert!(restored.snapshot().requests[0].provider_usage.is_none());
    }

    #[test]
    fn canceled_request_cannot_refund_requests_added_after_same_task_reload() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("work.json");
        let original = WorkBudget::new("task", 3, Some(path.clone()));
        let pending = original.begin("coder", &Value::Null).unwrap();
        let resumed = WorkBudget::load("task", path.clone()).unwrap();
        drop(resumed.begin("coder", &Value::Null).unwrap());
        drop(pending);
        let saved = WorkBudget::load("task", path).unwrap();
        assert_eq!(saved.remaining(), 1);
        assert_eq!(saved.snapshot().requests.len(), 2);
    }
    #[test]
    fn retries_and_interrupted_calls_survive_resume_without_refunding() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("work.json");
        let budget = WorkBudget::new("task", 2, Some(path.clone()));
        drop(
            budget
                .begin("coder", &serde_json::json!({"messages": ["a"]}))
                .unwrap(),
        );
        drop(
            budget
                .begin("observer_repair", &serde_json::json!({}))
                .unwrap(),
        );
        let resumed = WorkBudget::load("task", path).unwrap();
        assert_eq!(resumed.remaining(), 0);
        assert!(resumed.begin("coder", &Value::Null).is_err());
        assert_eq!(resumed.snapshot().requests[0].provider_usage, None);
    }
    #[tokio::test]
    async fn replay_preservation_survives_resume_and_blocks_loss_before_completion() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir(root.join(".spiral-coder")).unwrap();
        let path = root.join(".spiral-coder/tui_replay.json");
        let baseline = serde_json::json!({"version":1,"cases":[{"id":"old","checks":[{"value":"coder-1"}],"observer_response":{"suggestions":[{}, {"based_on":["failure_kind"]}]}}]});
        std::fs::write(&path, baseline.to_string()).unwrap();
        let goal = "Add to .spiral-coder/tui_replay.json; preserve every existing case.";
        let budget_path = root.join("budget.json");
        let budget = WorkBudget::new(goal, 4, Some(budget_path.clone()));
        budget.protect_replay(root).unwrap();
        let budget = WorkBudget::load(goal, budget_path).unwrap();
        scope(Some(Context {budget:budget.clone(), role:"coder"}), async {
            let (result, error) = crate::file_tools::tool_write_file(".spiral-coder/tui_replay.json", "{\"cases\":[]}", Some(root.to_str().unwrap()));
            assert!(error, "{result}");
            assert_eq!(serde_json::from_slice::<Value>(&std::fs::read(&path).unwrap()).unwrap(), baseline);
            // Shell edits bypass the pre-write hook; the next observation and
            // the completion gate must still expose the actual damage.
            std::fs::write(&path, "{\"cases\":[]}").unwrap();
            let request = serde_json::from_value(serde_json::json!({"packet":{},"messages":[{"role":"user","content":goal}],"final_summary":"done"})).unwrap();
            let observation = crate::observer_action::evaluate(request);
            assert!(observation.packet["artifact_preservation_issue"].as_str().unwrap().contains("removed"));
            assert!(observation.completion_issue.unwrap().contains("preservation"));
            let (result, error) = crate::file_tools::tool_write_file(".spiral-coder/tui_replay.json", &baseline.to_string(), Some(root.to_str().unwrap()));
            assert!(!error, "{result}");
            assert!(budget.preservation_issue().is_none());
        }).await;
    }
}
