//! Read-only status for a bounded `coupled-run` output directory.
//!
//! Reconciles the native `iterations.json` manifest with each round's durable
//! request ledger (`<iteration>/state/tui-work.json`, falling back to the
//! round `report.json` work snapshot). The command never calls providers,
//! never writes, and needs no credentials. Unknown or pending usage stays
//! `null`; it is never reported as zero. In-flight and transport-charged
//! attempts each occupy exactly one wire receipt.
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

#[cfg(test)]
mod tests;

#[derive(clap::Args, Clone, Debug)]
pub(crate) struct Args {
    /// Existing `coupled-run --out-dir` directory containing iterations.json
    #[arg(long)]
    pub run_dir: PathBuf,
}

const IN_FLIGHT: &str = "in_flight_or_interrupted";

struct Ledger {
    source: &'static str,
    task: usize,
    transport: usize,
    wire: usize,
    pending: usize,
    usage_unknown: usize,
    roles: Value,
}

fn read_json(path: &Path) -> Result<Value> {
    let data = std::fs::read(path).with_context(|| format!("cannot read {}", path.display()))?;
    serde_json::from_slice(&data).with_context(|| format!("cannot parse {}", path.display()))
}

/// What the per-round ledger lookup found. A file that exists but cannot be
/// parsed is `Unreadable`: the round's usage is unknown rather than absent,
/// and that is a reconciliation issue, not a hard failure for the whole run.
enum LedgerLoad {
    Ledger(Ledger),
    Missing,
    Unreadable { source: &'static str, error: String },
}

/// `state/tui-work.json` is the authoritative per-round ledger; the round
/// report embeds the same work snapshot and substitutes only when the durable
/// file is absent (e.g. a round that died before its first persist).
fn load_ledger(round: &Path) -> Result<LedgerLoad> {
    let durable = round.join("state/tui-work.json");
    let (work, source) = if durable.is_file() {
        match read_json(&durable) {
            Ok(work) => (work, "state/tui-work.json"),
            Err(error) => {
                return Ok(LedgerLoad::Unreadable {
                    source: "state/tui-work.json",
                    error: error.to_string(),
                })
            }
        }
    } else {
        let report = round.join("report.json");
        if !report.is_file() {
            return Ok(LedgerLoad::Missing);
        }
        let report = match read_json(&report) {
            Ok(report) => report,
            Err(error) => {
                return Ok(LedgerLoad::Unreadable {
                    source: "report.json",
                    error: error.to_string(),
                })
            }
        };
        match report.get("work") {
            Some(work) => (work.clone(), "report.json"),
            None => return Ok(LedgerLoad::Missing),
        }
    };
    let requests = work
        .get("requests")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut task = 0;
    let mut transport = 0;
    let mut pending = 0;
    let mut usage_unknown = 0;
    let mut role_wire: std::collections::BTreeMap<String, usize> = Default::default();
    let mut role_task: std::collections::BTreeMap<String, usize> = Default::default();
    for request in &requests {
        // One receipt per wire attempt; the charge decides the task pool only.
        let is_task = request
            .get("charge")
            .and_then(Value::as_str)
            .unwrap_or("task")
            == "task";
        if is_task {
            task += 1;
        } else {
            transport += 1;
        }
        if request.get("status").and_then(Value::as_str) == Some(IN_FLIGHT) {
            pending += 1;
        }
        if request.get("provider_usage").is_none_or(Value::is_null) {
            usage_unknown += 1;
        }
        let role = request
            .get("role")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_owned();
        *role_wire.entry(role.clone()).or_default() += 1;
        if is_task {
            *role_task.entry(role).or_default() += 1;
        }
    }
    let roles = role_wire
        .into_iter()
        .map(|(role, wire)| {
            (
                role.clone(),
                json!({"task_requests": role_task.get(&role).copied().unwrap_or(0),
                    "wire_requests": wire}),
            )
        })
        .collect::<serde_json::Map<_, _>>();
    Ok(LedgerLoad::Ledger(Ledger {
        source,
        task,
        transport,
        wire: requests.len(),
        pending,
        usage_unknown,
        roles: Value::Object(roles),
    }))
}

fn ledger_json(ledger: &LedgerLoad) -> Value {
    match ledger {
        LedgerLoad::Ledger(l) => json!({
            "present": true,
            "source": l.source,
            "task_requests": l.task,
            "transport_requests": l.transport,
            "wire_requests": l.wire,
            "pending_requests": l.pending,
            "usage_unknown": l.usage_unknown,
            "roles": l.roles,
        }),
        LedgerLoad::Unreadable { source, error } => json!({
            "present": false,
            "source": source,
            "unreadable": true,
            "error": error,
            "task_requests": null,
            "transport_requests": null,
            "wire_requests": null,
            "pending_requests": null,
            "usage_unknown": null,
            "roles": {},
        }),
        // A missing ledger means usage is unknown, never zero.
        LedgerLoad::Missing => json!({
            "present": false,
            "source": null,
            "task_requests": null,
            "transport_requests": null,
            "wire_requests": null,
            "pending_requests": null,
            "usage_unknown": null,
            "roles": {},
        }),
    }
}

fn reconcile_counts(
    label: &str,
    recorded_task: Option<i64>,
    recorded_wire: Option<i64>,
    ledger: &LedgerLoad,
    issues: &mut Vec<String>,
) {
    match ledger {
        LedgerLoad::Ledger(l) => {
            if recorded_task.is_some_and(|v| v != l.task as i64)
                || (recorded_task.is_none() && l.task > 0)
            {
                issues.push(format!(
                    "{label}: recorded task_requests {recorded_task:?} but ledger holds {}",
                    l.task
                ));
            }
            if recorded_wire.is_some_and(|v| v != l.wire as i64)
                || (recorded_wire.is_none() && l.wire > 0)
            {
                issues.push(format!(
                    "{label}: recorded wire_requests {recorded_wire:?} but ledger holds {}",
                    l.wire
                ));
            }
        }
        _ => {
            if recorded_task.is_some_and(|v| v != 0) || recorded_wire.is_some_and(|v| v != 0) {
                issues.push(format!(
                    "{label}: recorded usage but no request ledger survives; actual usage unknown"
                ));
            }
        }
    }
}

/// How the manifest knows this round. `Row` and `Interrupted` carry recorded
/// counts to reconcile; `Active` has no row yet, so a growing ledger is
/// expected rather than a mismatch; `Unrecorded` is on disk only.
enum ManifestEntry<'a> {
    Row(&'a Value),
    Interrupted(&'a Value),
    Active,
    Unrecorded,
}

impl ManifestEntry<'_> {
    fn counts(&self) -> Option<&Value> {
        match self {
            ManifestEntry::Row(row) | ManifestEntry::Interrupted(row) => Some(row),
            _ => None,
        }
    }

    fn active(&self) -> bool {
        matches!(self, ManifestEntry::Active)
    }
}

fn inspect_iteration(run_dir: &Path, directory: &str, entry: ManifestEntry) -> Result<Value> {
    let round = run_dir.join(directory);
    let ledger = load_ledger(&round)?;
    let recorded_task = entry
        .counts()
        .and_then(|r| r.get("task_requests").and_then(Value::as_i64));
    let recorded_wire = entry
        .counts()
        .and_then(|r| r.get("wire_requests").and_then(Value::as_i64));
    let mut issues = Vec::new();
    if !round.is_dir() {
        issues.push(format!("{directory}: recorded round directory is missing"));
    }
    // The active round's manifest row lands only when it finishes; an
    // in-flight ledger with no recorded counts is expected, not a mismatch.
    if !entry.active() {
        reconcile_counts(
            directory,
            recorded_task,
            recorded_wire,
            &ledger,
            &mut issues,
        );
    }
    match &ledger {
        LedgerLoad::Missing if round.is_dir() && !entry.active() => issues.push(format!(
            "{directory}: no state/tui-work.json or report.json work snapshot"
        )),
        LedgerLoad::Unreadable { source, error } => issues.push(format!(
            "{directory}: unreadable ledger {source} ({error}); usage unknown"
        )),
        _ => {}
    }
    let recorded = match entry.counts() {
        Some(r) => json!({
            "termination": r.get("termination").cloned().unwrap_or(Value::Null),
            "completed": r.get("completed").cloned().unwrap_or(Value::Null),
            "task_requests": r.get("task_requests").cloned().unwrap_or(Value::Null),
            "wire_requests": r.get("wire_requests").cloned().unwrap_or(Value::Null),
            "progress": r.get("progress").cloned().unwrap_or(Value::Null),
        }),
        None => json!({
            "termination": null,
            "completed": null,
            "task_requests": null,
            "wire_requests": null,
            "progress": null,
            "manifest_row": false,
        }),
    };
    Ok(json!({
        "directory": directory,
        "active": entry.active(),
        "recorded": recorded,
        "ledger": ledger_json(&ledger),
        "reconciled": issues.is_empty(),
        "issues": issues,
    }))
}

fn iteration_dirs(run_dir: &Path) -> Result<BTreeSet<String>> {
    let mut dirs = BTreeSet::new();
    for entry in
        std::fs::read_dir(run_dir).with_context(|| format!("cannot list {}", run_dir.display()))?
    {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with("iteration-") {
                dirs.insert(name);
            }
        }
    }
    Ok(dirs)
}

pub(crate) fn run(args: Args) -> Result<()> {
    let run_dir = args
        .run_dir
        .canonicalize()
        .with_context(|| format!("run dir {} does not exist", args.run_dir.display()))?;
    let manifest_path = run_dir.join("iterations.json");
    if !manifest_path.is_file() {
        bail!(
            "{} is not a coupled-run output directory (no iterations.json)",
            run_dir.display()
        );
    }
    let manifest = read_json(&manifest_path)?;
    let rows = manifest
        .get("iterations")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let active_dir = manifest
        .get("active_iteration")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let recorded_dirs: BTreeSet<String> = rows
        .iter()
        .filter_map(|r| {
            r.get("directory")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .chain(
            manifest
                .get("interrupted_iteration")
                .and_then(|i| i.get("directory"))
                .and_then(Value::as_str)
                .map(str::to_owned),
        )
        .chain(active_dir.clone())
        .collect();
    let on_disk = iteration_dirs(&run_dir)?;
    let mut issues = Vec::new();
    let mut iterations = Vec::new();
    for row in &rows {
        let directory = row
            .get("directory")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let entry = if active_dir.as_deref() == Some(directory.as_str()) {
            ManifestEntry::Active
        } else {
            ManifestEntry::Row(row)
        };
        if directory.is_empty() {
            issues.push("manifest row without a directory name".to_string());
        } else {
            iterations.push(inspect_iteration(&run_dir, &directory, entry)?);
        }
    }
    // The in-flight round and an interrupted round are manifest-known but not
    // rows; their durable receipts still reconcile exactly once.
    if let Some(directory) = &active_dir {
        if !rows
            .iter()
            .any(|r| r.get("directory").and_then(Value::as_str) == Some(directory))
        {
            iterations.push(inspect_iteration(
                &run_dir,
                directory,
                ManifestEntry::Active,
            )?);
        }
    }
    if let Some(interrupted) = manifest.get("interrupted_iteration") {
        let directory = interrupted
            .get("directory")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        // A directory already covered by a row or the active round must not
        // double-count its receipts.
        if !directory.is_empty()
            && !iterations
                .iter()
                .any(|it| it["directory"].as_str() == Some(directory.as_str()))
        {
            iterations.push(inspect_iteration(
                &run_dir,
                &directory,
                ManifestEntry::Interrupted(interrupted),
            )?);
        }
    }
    for unrecorded in on_disk.difference(&recorded_dirs) {
        let inspected = inspect_iteration(&run_dir, unrecorded, ManifestEntry::Unrecorded)?;
        let spent = inspected["ledger"]["wire_requests"].clone();
        issues.push(format!(
            "{unrecorded}: iteration directory has no manifest row; its usage is reported but excluded from totals"
        ));
        let mut inspected = inspected;
        inspected["unrecorded"] = json!(true);
        inspected["excluded_wire_requests"] = spent;
        iterations.push(inspected);
    }

    let mut all_ledgers = true;
    let mut task_total = 0usize;
    let mut wire_total = 0usize;
    let mut pending_total = 0usize;
    for iteration in &iterations {
        let ledger = &iteration["ledger"];
        if ledger["present"].as_bool() == Some(true) && iteration["unrecorded"] != json!(true) {
            task_total += ledger["task_requests"].as_u64().unwrap_or(0) as usize;
            wire_total += ledger["wire_requests"].as_u64().unwrap_or(0) as usize;
            pending_total += ledger["pending_requests"].as_u64().unwrap_or(0) as usize;
        } else if iteration["unrecorded"] != json!(true) {
            all_ledgers = false;
        }
    }
    let recorded_task = manifest.get("task_requests").and_then(Value::as_i64);
    let recorded_wire = manifest.get("wire_requests").and_then(Value::as_i64);
    // Top-level totals are written when rounds finish; while a round is
    // in flight the manifest legitimately lags the live ledger.
    if all_ledgers && active_dir.is_none() {
        if recorded_task.is_some_and(|v| v != task_total as i64) {
            issues.push(format!(
                "manifest task_requests {recorded_task:?} but ledgers total {task_total}"
            ));
        }
        if recorded_wire.is_some_and(|v| v != wire_total as i64) {
            issues.push(format!(
                "manifest wire_requests {recorded_wire:?} but ledgers total {wire_total}"
            ));
        }
    } else if !all_ledgers && (recorded_task.is_some() || recorded_wire.is_some()) {
        issues.push(
            "manifest records usage but at least one round ledger is missing; totals are unknown"
                .into(),
        );
    }
    for iteration in &iterations {
        if iteration["reconciled"].as_bool() != Some(true) {
            issues.extend(
                iteration["issues"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(str::to_owned),
            );
        }
    }

    let recorded_complete = manifest.get("completed").and_then(Value::as_bool);
    // A completion claim without any recorded work receipt (no changed file,
    // no newly verified check, no reproduced baseline) cannot qualify: the
    // done report is the run's own assertion, not independent evidence.
    if recorded_complete == Some(true) {
        let any_progress = iterations.iter().any(|it| {
            let p = &it["recorded"]["progress"];
            [
                "changed_files",
                "newly_verified",
                "newly_reproduced_baselines",
            ]
            .iter()
            .any(|key| {
                p.get(key)
                    .and_then(Value::as_array)
                    .is_some_and(|a| !a.is_empty())
            })
        });
        if !any_progress {
            issues.push(
                "completion recorded but no round shows a changed file or verified check".into(),
            );
        }
    }
    // Recorded completion is the run's own claim; qualification is this
    // command's independent reconciliation of every durable receipt.
    let qualified_complete =
        recorded_complete == Some(true) && issues.is_empty() && !rows.is_empty();
    let status = json!({
        "version": 1,
        "run_dir": run_dir,
        "manifest": {
            "present": true,
            "version": manifest.get("version").cloned().unwrap_or(Value::Null),
            "max_iterations": manifest.get("max_iterations").cloned().unwrap_or(Value::Null),
            "max_task_requests_per_iteration": manifest.get("max_task_requests_per_iteration").cloned().unwrap_or(Value::Null),
            "max_task_requests": manifest.get("max_task_requests").cloned().unwrap_or(Value::Null),
            "max_wire_requests": manifest.get("max_wire_requests").cloned().unwrap_or(Value::Null),
        },
        "recorded": {
            "completed": manifest.get("completed").cloned().unwrap_or(Value::Null),
            "stop_reason": manifest.get("stop_reason").cloned().unwrap_or(Value::Null),
            "active_iteration": manifest.get("active_iteration").cloned().unwrap_or(Value::Null),
            "interrupted_iteration": manifest.get("interrupted_iteration").cloned().unwrap_or(Value::Null),
            "iterations": rows.len(),
            "task_requests": manifest.get("task_requests").cloned().unwrap_or(Value::Null),
            "wire_requests": manifest.get("wire_requests").cloned().unwrap_or(Value::Null),
        },
        "iterations": iterations,
        "totals": {
            "task_requests": all_ledgers.then_some(task_total),
            "wire_requests": all_ledgers.then_some(wire_total),
            "pending_requests": all_ledgers.then_some(pending_total),
        },
        "qualification": {
            "recorded_complete": recorded_complete,
            "qualified_complete": qualified_complete,
            "issues": issues,
        },
    });
    println!("{}", serde_json::to_string(&status)?);
    Ok(())
}
