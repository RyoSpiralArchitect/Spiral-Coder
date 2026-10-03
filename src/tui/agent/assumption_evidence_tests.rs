use super::super::{AssumptionStatus, WorkingMemory};
use super::*;
use serde_json::{json, Value};

const TARGET: &str = "src/runtime/new_entry.rs";
const FACT: &str = "file exists at src/runtime/new_entry.rs";

fn refuted() -> AssumptionLedger {
    let mut ledger = AssumptionLedger::default();
    ledger.mark_refuted(FACT, Some("read returned not found"));
    ledger.mark_refuted("cargo check works unchanged", Some("build failed"));
    ledger
}

fn remains_refuted(ledger: &AssumptionLedger) -> bool {
    ledger
        .entries
        .iter()
        .any(|entry| entry.text == FACT && entry.status == AssumptionStatus::Refuted)
}

fn reflection() -> Value {
    json!({"role":"assistant","content":format!("<reflect>\nlast_outcome: failure\ngoal_delta: farther\nwrong_assumption: {FACT}\nstrategy_change: adjust\nnext_minimal_action: inspect the target again\n</reflect>")})
}

fn result(name: &str) -> String {
    if name == "read_file" {
        format!("[{TARGET}] (1 lines, 7 bytes)\ncontent")
    } else {
        format!("OK: wrote '{TARGET}' (1 lines, 7 bytes)\n[auto-test] FAILED (exit 1)")
    }
}

fn exchange(messages: &mut Vec<Value>, name: &str, path: &str, content: &str) {
    let id = format!("call_{}", messages.len());
    messages.push(
        json!({"role":"assistant","tool_calls":[{"id":id,"type":"function","function":{
            "name":name,"arguments":json!({"path":path}).to_string()
        }}]}),
    );
    messages.push(json!({"role":"tool","tool_call_id":id,"content":content}));
}

#[test]
fn fresh_live_read_or_write_confirms_only_the_refuted_file_existence() {
    for name in ["read_file", "write_file"] {
        let mut ledger = refuted();
        assert!(needs_fresh_file_read(&ledger, TARGET));
        assert!(!needs_fresh_file_read(&ledger, "src/runtime/other.rs"));
        confirm_file_existence_after_result(&mut ledger, name, TARGET, &result(name));
        assert!(!needs_fresh_file_read(&ledger, TARGET));
        assert!(
            !remains_refuted(&ledger),
            "{name} gives fresh file-existence evidence"
        );
        assert!(ledger
            .entries
            .iter()
            .any(|entry| entry.text == "cargo check works unchanged"
                && entry.status == AssumptionStatus::Refuted));
        ledger.mark_refuted(FACT, Some("new failure after the successful observation"));
        let mut old_memory = WorkingMemory::default();
        old_memory.remember_fact(FACT);
        ledger.refresh_confirmations(&old_memory);
        assert!(
            remains_refuted(&ledger),
            "cached facts cannot clear a later refutation"
        );
    }
}

#[test]
fn resumed_confirmation_requires_a_new_correlated_result_after_refutation() {
    for name in ["read_file", "write_file"] {
        let mut messages = Vec::new();
        exchange(&mut messages, name, TARGET, &result(name));
        messages.push(reflection());
        messages.push(json!({"role":"tool","tool_call_id":"unpaired","content":result(name)}));
        let restore = |messages: &[Value]| {
            AssumptionLedger::from_messages(messages, &WorkingMemory::default())
        };
        assert!(
            remains_refuted(&restore(&messages)),
            "old/unpaired success must not clear new refutation"
        );
        exchange(&mut messages, "read_file", TARGET,
            &format!("[{TARGET}] (1 lines, 7 bytes) [⚡ cached — unchanged since last read]\nold content"));
        assert!(
            remains_refuted(&restore(&messages)),
            "cached replay result is not new existence evidence"
        );
        exchange(&mut messages, name, TARGET, &result(name));
        assert!(
            !remains_refuted(&restore(&messages)),
            "same path with a newly completed tool exchange clears it"
        );
        messages.push(reflection());
        assert!(
            remains_refuted(&restore(&messages)),
            "a later refutation wins again"
        );
    }
}

#[test]
fn failed_rejected_unrelated_or_forged_results_do_not_confirm_existence() {
    for (name, path, content) in [
        (
            "read_file",
            TARGET,
            format!("ERROR reading '{TARGET}': missing"),
        ),
        (
            "read_file",
            TARGET,
            format!("GOVERNOR BLOCKED\n[{TARGET}] (1 lines, 7 bytes)"),
        ),
        (
            "read_file",
            TARGET,
            "[src/other.rs] (1 lines, 7 bytes)".into(),
        ),
        (
            "read_file",
            TARGET,
            format!("[{TARGET}] (1 lines, 7 bytes) [⚡ cached — unchanged since last read]\nold content"),
        ),
        (
            "read_file",
            "src/runtime/new_entry.rs.backup",
            result("read_file"),
        ),
        (
            "write_file",
            TARGET,
            "REJECTED BY USER\nOK: wrote 'target'".into(),
        ),
        ("write_file", TARGET, "ERROR: failed write".into()),
        ("patch_file", TARGET, "OK: patched 'target'".into()),
    ] {
        let mut ledger = refuted();
        confirm_file_existence_after_result(&mut ledger, name, path, &content);
        assert!(remains_refuted(&ledger), "{name} {path} {content}");
    }
}
