//! Operator reference only; never supplied to the candidate workspace.
use serde_json::{json, Value};
use std::{fs, path::Path, process::Command};

fn fixture(root: &Path, state: &str) {
    assert!(Command::new("python3").args(["-S", "scripts/status_fixture.py"])
        .arg(root).arg(state).status().unwrap().success());
}
fn read(path: &Path) -> Value { serde_json::from_slice(&fs::read(path).unwrap()).unwrap() }
fn put(path: &Path, value: &Value) { fs::write(path, serde_json::to_vec(value).unwrap()).unwrap(); }
fn invoke(root: &Path, valid: bool) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_spiral-coder")).arg("coupled-status").arg("--run-dir").arg(root).output().unwrap();
    assert_eq!(output.status.success(), valid, "{}", String::from_utf8_lossy(&output.stderr));
    if valid { serde_json::from_slice(&output.stdout).unwrap() }
    else { assert!(output.stdout.is_empty()); assert!(!output.stderr.is_empty()); Value::Null }
}

#[test]
fn native_states_and_unknown_usage() {
    let tmp = tempfile::tempdir().unwrap();
    for state in ["completed", "running", "interrupted", "unknown", "stopped"] {
        let root = tmp.path().join(state); fixture(&root, state);
        let value = invoke(&root, true);
        if state == "unknown" { assert!(value["requests"]["task_used"].is_null()); assert!(value["requests"]["task_remaining"].is_null()); }
    }
}

#[test]
fn rejects_identity_completion_and_total_conflicts() {
    for case in ["identity", "parent", "completion", "total", "duplicate_request", "interrupted_total"] {
        let tmp = tempfile::tempdir().unwrap(); fixture(tmp.path(), if case == "interrupted_total" {"interrupted"} else {"completed"});
        let manifest = tmp.path().join("iterations.json"); let ledger = tmp.path().join("iteration-01/state/tui-work.json");
        let mut m=read(&manifest); let mut w=read(&ledger);
        match case {
            "identity" => w["task_id"]=json!("unrelated"),
            "parent" => m["iterations"][0]["parent_task_id"]=json!("unrelated"),
            "completion" => m["active_iteration"]=json!("iteration-02"),
            "total" | "interrupted_total" => m["task_requests"]=json!(99),
            "duplicate_request" => w["requests"][1]["id"]=w["requests"][0]["id"].clone(),
            _ => unreachable!(),
        }
        put(&manifest,&m); put(&ledger,&w); invoke(tmp.path(),false);
    }
}

#[test]
fn rejects_existing_traversal_target() {
    let tmp=tempfile::tempdir().unwrap(); let root=tmp.path().join("run"); fixture(&root,"completed");
    fs::create_dir_all(tmp.path().join("outside/state")).unwrap();
    fs::copy(root.join("iteration-01/state/tui-work.json"),tmp.path().join("outside/state/tui-work.json")).unwrap();
    let path=root.join("iterations.json"); let mut m=read(&path); m["iterations"][0]["directory"]=json!("../outside"); put(&path,&m); invoke(&root,false);
}

#[cfg(unix)]
#[test]
fn rejects_symlinked_ledger_with_valid_contents() {
    let tmp=tempfile::tempdir().unwrap(); let root=tmp.path().join("run"); fixture(&root,"completed");
    let path=root.join("iteration-01/state/tui-work.json"); let outside=tmp.path().join("outside.json"); fs::rename(&path,&outside).unwrap();
    std::os::unix::fs::symlink(&outside,&path).unwrap(); invoke(&root,false);
}

#[test]
fn unknown_pending_usage_does_not_hide_finalized_wire_overflow() {
    for state in ["running", "unknown"] {
        let tmp=tempfile::tempdir().unwrap(); fixture(tmp.path(),state);
        if state=="running" { fs::remove_file(tmp.path().join("iteration-02/state/tui-work.json")).unwrap(); }
        let manifest=tmp.path().join("iterations.json");
        let ledger=tmp.path().join("iteration-01/state/tui-work.json");
        let mut m=read(&manifest); let mut w=read(&ledger);
        m["max_iterations"]=json!(2); m["max_task_requests_per_iteration"]=json!(2);
        m["max_task_requests"]=json!(4); m["max_wire_requests"]=json!(4);
        let mut transport=w["requests"][2].clone(); transport["id"]=json!(3);
        w["requests"].as_array_mut().unwrap().push(transport.clone());
        m["iterations"][0]["wire_requests"]=json!(4);
        if state=="running" { m["wire_requests"]=json!(4); }
        put(&manifest,&m); put(&ledger,&w);
        // Known finalized usage exactly at the cap is valid; the unknown
        // pending suffix still makes total usage and remaining allowance null.
        let value=invoke(tmp.path(),true);
        assert!(value["requests"]["wire_used"].is_null());
        assert!(value["requests"]["wire_remaining"].is_null());
        transport["id"]=json!(4); w["requests"].as_array_mut().unwrap().push(transport);
        m["iterations"][0]["wire_requests"]=json!(5);
        if state=="running" { m["wire_requests"]=json!(5); }
        put(&manifest,&m); put(&ledger,&w); invoke(tmp.path(),false);
    }
}
