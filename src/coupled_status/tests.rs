use super::*;
use serde_json::json;
use std::path::Path;

fn write(path: &Path, value: &Value) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}

fn expect_ledger(round: &Path) -> Ledger {
    match load_ledger(round).unwrap() {
        LedgerLoad::Ledger(ledger) => ledger,
        other => panic!("expected ledger, got {:?}", std::mem::discriminant(&other)),
    }
}

fn request(role: &str, charge: &str, status: &str, usage: bool) -> Value {
    json!({
        "id": 0, "role": role, "estimated_input_tokens": 10,
        "provider_usage": usage.then(|| json!({"total_tokens": 5})),
        "duration_ms": 3, "status": status, "charge": charge,
    })
}

fn work(requests: Vec<Value>) -> Value {
    json!({
        "task_id": "t", "root": "/tmp/ws", "started_at_ms": 0,
        "request_limit": 32, "requests": requests, "assists": [],
    })
}

fn report(work: Value, completed: bool) -> Value {
    json!({"version":1,"completed":completed,"termination":"budget_exhausted",
        "work":work,"iteration_progress":{}})
}

fn manifest(rows: Vec<Value>) -> Value {
    json!({"version":1,"max_iterations":6,"max_task_requests_per_iteration":32,
        "max_task_requests":192,"max_wire_requests":216,"completed":false,
        "stop_reason":null,"iterations":rows,"task_requests":0,"wire_requests":0})
}

fn row(n: usize, task: usize, wire: usize) -> Value {
    json!({"iteration":n,"directory":format!("iteration-{n:02}"),
        "termination":"budget_exhausted","completed":false,
        "task_requests":task,"wire_requests":wire,"progress":{}})
}

#[test]
fn ledger_counts_transport_and_in_flight_exactly_once() {
    let dir = tempfile::tempdir().unwrap();
    let round = dir.path().join("iteration-01");
    write(
        &round.join("state/tui-work.json"),
        &work(vec![
            request("coder", "task", "completed", true),
            request("coder", "transport", "completed", true),
            request("observer", "task", "in_flight_or_interrupted", false),
        ]),
    );
    let ledger = expect_ledger(&round);
    assert_eq!(ledger.task, 2);
    assert_eq!(ledger.transport, 1);
    assert_eq!(ledger.wire, 3);
    assert_eq!(ledger.pending, 1);
    assert_eq!(ledger.usage_unknown, 1);
    assert_eq!(ledger.roles["coder"]["wire_requests"], 2);
    assert_eq!(ledger.roles["observer"]["task_requests"], 1);
}

#[test]
fn report_snapshot_substitutes_only_when_durable_ledger_is_absent() {
    let dir = tempfile::tempdir().unwrap();
    let round = dir.path().join("iteration-01");
    write(
        &round.join("report.json"),
        &report(
            work(vec![request("coder", "task", "completed", true)]),
            false,
        ),
    );
    let ledger = expect_ledger(&round);
    assert_eq!(ledger.source, "report.json");
    // The durable ledger wins when both exist.
    write(
        &round.join("state/tui-work.json"),
        &work(vec![
            request("coder", "task", "completed", true),
            request("observer", "task", "completed", true),
        ]),
    );
    let ledger = expect_ledger(&round);
    assert_eq!(ledger.source, "state/tui-work.json");
    assert_eq!(ledger.task, 2);
}

#[test]
fn missing_ledger_is_unknown_never_zero() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("iteration-01")).unwrap();
    let row = row(1, 5, 5);
    let inspected =
        inspect_iteration(dir.path(), "iteration-01", ManifestEntry::Row(&row)).unwrap();
    assert_eq!(inspected["ledger"]["task_requests"], Value::Null);
    assert_eq!(inspected["ledger"]["wire_requests"], Value::Null);
    assert_eq!(inspected["reconciled"], false);
    let missing_dir =
        inspect_iteration(dir.path(), "iteration-09", ManifestEntry::Unrecorded).unwrap();
    assert_eq!(missing_dir["ledger"]["present"], false);
}

#[test]
fn reconcile_flags_mismatch_and_recorded_null_with_real_usage() {
    let dir = tempfile::tempdir().unwrap();
    let round = dir.path().join("iteration-02");
    write(
        &round.join("state/tui-work.json"),
        &work(vec![request("coder", "task", "completed", true)]),
    );
    let mut issues = Vec::new();
    let ledger = load_ledger(&round).unwrap();
    reconcile_counts("iteration-02", Some(3), Some(3), &ledger, &mut issues);
    assert_eq!(issues.len(), 2);
    issues.clear();
    // Manifest recorded null but the ledger proves spend: still a discrepancy.
    reconcile_counts("iteration-02", None, None, &ledger, &mut issues);
    assert_eq!(issues.len(), 2);
}

#[test]
fn unrecorded_iteration_directories_are_flagged_not_counted() {
    let dir = tempfile::tempdir().unwrap();
    write(
        &dir.path().join("iterations.json"),
        &manifest(vec![row(1, 1, 1)]),
    );
    write(
        &dir.path().join("iteration-01/state/tui-work.json"),
        &work(vec![request("coder", "task", "completed", true)]),
    );
    write(
        &dir.path().join("iteration-07/state/tui-work.json"),
        &work(vec![
            request("coder", "task", "completed", true),
            request("coder", "transport", "completed", true),
        ]),
    );
    let dirs = iteration_dirs(dir.path()).unwrap();
    assert!(dirs.contains("iteration-07"));
    let inspected =
        inspect_iteration(dir.path(), "iteration-07", ManifestEntry::Unrecorded).unwrap();
    assert_eq!(inspected["ledger"]["wire_requests"], 2);
    assert_eq!(inspected["recorded"]["task_requests"], Value::Null);
}

#[test]
fn active_iteration_ledger_is_not_a_pending_mismatch() {
    let dir = tempfile::tempdir().unwrap();
    let round = dir.path().join("iteration-01");
    write(
        &round.join("state/tui-work.json"),
        &work(vec![request(
            "coder",
            "task",
            "in_flight_or_interrupted",
            false,
        )]),
    );
    // In-flight round: no manifest row yet, so recorded counts stay null and
    // the growing ledger is expected rather than a discrepancy.
    let inspected = inspect_iteration(dir.path(), "iteration-01", ManifestEntry::Active).unwrap();
    assert_eq!(inspected["active"], true);
    assert_eq!(inspected["reconciled"], true);
    assert_eq!(inspected["ledger"]["pending_requests"], 1);
}

#[test]
fn corrupt_ledger_is_unreadable_unknown_not_fatal() {
    let dir = tempfile::tempdir().unwrap();
    let round = dir.path().join("iteration-01");
    std::fs::create_dir_all(round.join("state")).unwrap();
    std::fs::write(round.join("state/tui-work.json"), "{not json").unwrap();
    match load_ledger(&round).unwrap() {
        LedgerLoad::Unreadable { source, .. } => assert_eq!(source, "state/tui-work.json"),
        _ => panic!("corrupt durable ledger must be unreadable"),
    }
    // The round still reports; its usage is null and flagged, never zero.
    let row = row(1, 2, 2);
    let inspected =
        inspect_iteration(dir.path(), "iteration-01", ManifestEntry::Row(&row)).unwrap();
    assert_eq!(inspected["ledger"]["unreadable"], true);
    assert_eq!(inspected["ledger"]["wire_requests"], Value::Null);
    assert_eq!(inspected["reconciled"], false);
}
