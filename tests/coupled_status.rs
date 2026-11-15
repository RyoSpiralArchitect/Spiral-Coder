use serde_json::{json, Value};
use std::{fs, path::Path, process::Output};

fn status(root: &Path, run_dir: &Path) -> Output {
    std::process::Command::new(env!("CARGO_BIN_EXE_spiral-coder"))
        .current_dir(root)
        .env_remove("OPENAI_API_KEY")
        .env_remove("MISTRAL_API_KEY")
        .env_remove("GEMINI_API_KEY")
        .env_remove("GOOGLE_API_KEY")
        .env_remove("ANTHROPIC_API_KEY")
        .arg("coupled-status")
        .arg("--run-dir")
        .arg(run_dir)
        .output()
        .unwrap()
}

fn write(path: &Path, value: &Value) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}

fn receipt(charge: &str, status: &str) -> Value {
    json!({"role":"coder","estimated_input_tokens":4,"provider_usage":null,
        "duration_ms":1,"status":status,"charge":charge})
}

#[test]
fn missing_manifest_fails_without_touching_the_directory() {
    let root = tempfile::tempdir().unwrap();
    let run = tempfile::tempdir().unwrap();
    let out = status(root.path(), run.path());
    assert!(!out.status.success());
    assert!(out.stdout.is_empty());
    assert!(String::from_utf8_lossy(&out.stderr).contains("iterations.json"));
    // Read-only: the inspected directory gains no files.
    assert_eq!(fs::read_dir(run.path()).unwrap().count(), 0);
}

#[test]
fn reconciles_a_finished_run_and_qualifies_its_recorded_completion() {
    let root = tempfile::tempdir().unwrap();
    let run = tempfile::tempdir().unwrap();
    let ledger = json!({"task_id":"t","root":"/w","started_at_ms":0,"request_limit":32,
        "requests":[
            receipt("task","completed"),
            receipt("task","completed"),
            receipt("transport","completed"),
        ],"assists":[]});
    write(
        &run.path().join("iteration-01/state/tui-work.json"),
        &ledger,
    );
    write(
        &run.path().join("iteration-02/report.json"),
        &json!({
        "version":1,"completed":true,"termination":"completed","work":ledger}),
    );
    write(
        &run.path().join("iterations.json"),
        &json!({"version":1,"max_iterations":2,"max_task_requests_per_iteration":32,
            "max_task_requests":64,"max_wire_requests":72,"completed":true,
            "stop_reason":"completed",
            "iterations":[
                {"iteration":1,"directory":"iteration-01","termination":"budget_exhausted",
                 "completed":false,"task_requests":2,"wire_requests":3,
                 "progress":{"changed_files":["src/lib.rs"],"newly_verified":["cargo test"]}},
                {"iteration":2,"directory":"iteration-02","termination":"completed",
                 "completed":true,"task_requests":2,"wire_requests":3,
                 "progress":{"changed_files":["src/main.rs"],"newly_verified":[]}}],
            "task_requests":4,"wire_requests":6}),
    );
    let out = status(root.path(), run.path());
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let value: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value["version"], 1);
    assert_eq!(value["totals"]["task_requests"], 4);
    assert_eq!(value["totals"]["wire_requests"], 6);
    assert_eq!(value["iterations"][1]["ledger"]["source"], "report.json");
    assert_eq!(value["qualification"]["qualified_complete"], true);
}

#[test]
fn missing_ledger_reports_null_usage_not_zero() {
    let root = tempfile::tempdir().unwrap();
    let run = tempfile::tempdir().unwrap();
    fs::create_dir_all(run.path().join("iteration-01")).unwrap();
    write(
        &run.path().join("iterations.json"),
        &json!({"version":1,"completed":false,"stop_reason":"interrupted",
            "iterations":[{"iteration":1,"directory":"iteration-01",
             "termination":"interrupted","completed":false,
             "task_requests":null,"wire_requests":null,"progress":{}}],
            "task_requests":null,"wire_requests":null}),
    );
    let out = status(root.path(), run.path());
    assert!(out.status.success());
    let value: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value["totals"]["task_requests"], Value::Null);
    assert_eq!(value["totals"]["wire_requests"], Value::Null);
    assert_eq!(value["qualification"]["qualified_complete"], false);
}
