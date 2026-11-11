//! Operator reference, never copied into the live candidate.
use anyhow::{ensure, Context, Result};
use clap::Args;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use crate::work_budget::WorkState;

#[derive(Args, Debug, Clone)]
pub(crate) struct StatusArgs {
    #[arg(long)]
    run_dir: PathBuf,
}

#[derive(Deserialize)]
struct Progress { changed_files: Vec<String>, newly_verified: Vec<String> }
#[derive(Deserialize)]
struct Row {
    iteration: usize, directory: String, task_id: String, parent_task_id: Option<String>,
    termination: String, completed: bool, task_requests: usize, wire_requests: usize,
    progress: Progress,
}
#[derive(Deserialize)]
struct Interrupted { directory: String, task_requests: Option<usize>, wire_requests: Option<usize> }
#[derive(Deserialize)]
struct Manifest {
    version: u32, max_iterations: usize, max_task_requests_per_iteration: usize,
    max_task_requests: usize, max_wire_requests: usize, completed: bool,
    stop_reason: Option<String>, active_iteration: Option<String>,
    interrupted_iteration: Option<Interrupted>, iterations: Vec<Row>,
    task_requests: Option<usize>, wire_requests: Option<usize>,
}

fn read(root: &Path, relative: &Path, optional: bool) -> Result<Option<Vec<u8>>> {
    let mut path = root.to_path_buf();
    for component in relative.components() {
        ensure!(matches!(component, std::path::Component::Normal(_)), "invalid record path");
        path.push(component);
        match std::fs::symlink_metadata(&path) {
            Ok(meta) => ensure!(!meta.file_type().is_symlink(), "symlink record: {}", path.display()),
            Err(e) if optional && e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e).with_context(|| format!("missing record {}", path.display())),
        }
    }
    Ok(Some(std::fs::read(&path).with_context(|| format!("read {}", path.display()))?))
}

fn ledger(root: &Path, directory: &str, optional: bool) -> Result<Option<(String, usize, usize, usize)>> {
    let path = PathBuf::from(directory).join("state/tui-work.json");
    let Some(bytes) = read(root, &path, optional)? else { return Ok(None) };
    let work: WorkState = serde_json::from_slice(&bytes).context("invalid work ledger")?;
    ensure!(!work.task_id.is_empty(), "empty task ID");
    let mut ids = BTreeSet::new();
    for r in &work.requests { ensure!(ids.insert(r.id), "duplicate request ID"); }
    Ok(Some((work.task_id.clone(), work.task_requests(), work.requests.len(),
        work.requests.iter().filter(|r| r.status == "in_flight_or_interrupted").count())))
}

fn plus(a: usize, b: usize) -> Result<usize> { a.checked_add(b).context("request count overflow") }

pub(crate) fn run(args: StatusArgs) -> Result<()> {
    let root = args.run_dir.canonicalize().context("run directory")?;
    let bytes = read(&root, Path::new("iterations.json"), false)?.context("manifest")?;
    let m: Manifest = serde_json::from_slice(&bytes).context("invalid iterations manifest")?;
    ensure!(m.version == 1, "unsupported manifest version");
    ensure!(m.max_iterations > 0 && m.max_task_requests_per_iteration > 0, "invalid limits");
    ensure!(m.max_iterations.checked_mul(m.max_task_requests_per_iteration) == Some(m.max_task_requests), "inconsistent task limit");
    ensure!(m.max_wire_requests >= m.max_task_requests && m.iterations.len() <= m.max_iterations, "invalid chain limits");
    ensure!(!(m.active_iteration.is_some() && m.interrupted_iteration.is_some()), "two pending records");
    let pending_dir = m.active_iteration.as_deref().or(m.interrupted_iteration.as_ref().map(|x| x.directory.as_str()));
    if let Some(directory) = pending_dir {
        ensure!(m.iterations.len() < m.max_iterations && directory == format!("iteration-{:02}", m.iterations.len()+1), "invalid pending directory");
        ensure!(!m.completed, "completed run has pending iteration");
    }
    if m.active_iteration.is_some() { ensure!(m.stop_reason.is_none(), "active run has stop reason"); }
    if m.interrupted_iteration.is_some() { ensure!(m.stop_reason.as_deref() == Some("interrupted"), "invalid interruption reason"); }
    ensure!(m.completed == (m.stop_reason.as_deref() == Some("completed")), "inconsistent completion");
    let mut tasks = 0;
    let mut wires = 0;
    let mut ids = BTreeSet::new();
    let mut parent: Option<&str> = None;
    let mut rows = Vec::new();
    for (index, r) in m.iterations.iter().enumerate() {
        ensure!(r.iteration == index+1 && r.directory == format!("iteration-{:02}", index+1), "invalid iteration order");
        ensure!(r.parent_task_id.as_deref() == parent && ids.insert(r.task_id.clone()), "invalid task chain");
        ensure!(r.completed == (r.termination == "completed"), "inconsistent row completion");
        ensure!(!r.completed || (index+1 == m.iterations.len() && m.completed), "nonterminal completion");
        let (task, t, w, _) = ledger(&root, &r.directory, false)?.context("missing final ledger")?;
        ensure!(task == r.task_id && t == r.task_requests && w == r.wire_requests && t <= m.max_task_requests_per_iteration, "row/ledger mismatch");
        tasks = plus(tasks, t)?; wires = plus(wires, w)?;
        parent = Some(&r.task_id);
        rows.push(json!({"iteration":r.iteration,"task_id":r.task_id,"termination":r.termination,
            "task_requests":t,"wire_requests":w,"changed_files":r.progress.changed_files,"newly_verified":r.progress.newly_verified}));
    }
    ensure!(!m.completed || m.iterations.last().is_some_and(|r| r.completed), "missing completed row");
    let mut total_t = Some(tasks);
    let mut total_w = Some(wires);
    let pending = if let Some(directory) = pending_dir {
        let pending_work = ledger(&root, directory, true)?;
        let (task, t, w, flight) = if let Some((task,t,w,f)) = pending_work {
            ensure!(ids.insert(task.clone()) && t <= m.max_task_requests_per_iteration, "invalid pending ledger");
            (Some(task),Some(t),Some(w),Some(f))
        } else { (None,None,None,None) };
        if let Some(interrupted) = &m.interrupted_iteration {
            ensure!(interrupted.task_requests == t && interrupted.wire_requests == w, "interruption/ledger mismatch");
        }
        total_t = t.map(|t| plus(tasks,t)).transpose()?;
        total_w = w.map(|w| plus(wires,w)).transpose()?;
        json!({"directory":directory,"task_id":task,"task_requests":t,"wire_requests":w,"in_flight_requests":flight})
    } else { Value::Null };
    let recorded = if m.interrupted_iteration.is_some() { (total_t,total_w) } else { (Some(tasks),Some(wires)) };
    ensure!((m.task_requests,m.wire_requests) == recorded, "manifest/ledger totals mismatch");
    ensure!(total_t.is_none_or(|n| n <= m.max_task_requests) && total_w.is_none_or(|n| n <= m.max_wire_requests), "budget exceeded");
    let status = if m.completed { "completed" } else if m.stop_reason.is_some() { "stopped" } else { "running" };
    let result = json!({"version":1,"status":status,"stop_reason":m.stop_reason,"iterations":rows,"pending":pending,
        "requests":{"task_used":total_t,"wire_used":total_w,"task_limit":m.max_task_requests,"wire_limit":m.max_wire_requests,
                     "task_remaining":total_t.map(|t| m.max_task_requests-t),"wire_remaining":total_w.map(|w| m.max_wire_requests-w)}});
    println!("{}",serde_json::to_string(&result)?);
    Ok(())
}
