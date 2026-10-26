use super::*;
use crate::tui::agent::{self, RecoveryGovernor, RecoveryStage, VerificationLevel};

const FIRST: &str = "cargo test visible_failure";
const SECOND: &str = "cargo test --locked --bin spiral-coder -q";

fn call(id: &str, name: &str, args: Value) -> ToolCallData {
    ToolCallData {
        id: id.into(),
        name: name.into(),
        arguments: args.to_string(),
        thought_signature: None,
    }
}

fn record(
    focus: &mut RecoveryFocus,
    messages: &mut Vec<Value>,
    root: &Path,
    tc: &ToolCallData,
    output: &str,
) {
    let context = ExecVerificationContext::from_messages(Some(SECOND), messages);
    focus.record_result(tc, output, root.to_str(), Some(SECOND), &context);
    messages.push(json!({"role":"assistant","tool_calls":[{"id":tc.id,"function":{"name":tc.name,"arguments":tc.arguments}}]}));
    messages.push(focus.tool_message(tc, output.into()));
}

fn fixture() -> (tempfile::TempDir, RecoveryFocus, Vec<Value>) {
    let root = tempfile::tempdir().unwrap();
    for path in ["visible.rs", "later.rs", "source.rs"] {
        std::fs::write(root.path().join(path), "source\n").unwrap();
    }
    let mut focus = RecoveryFocus::default();
    let mut messages =
        vec![json!({"role":"user","content":"Repair the source; preserve supplied tests"})];
    let tc = call("first", "exec", json!({"command":FIRST}));
    let output = exec_failure(root.path(), "visible.rs");
    record(&mut focus, &mut messages, root.path(), &tc, &output);
    read(&mut focus, &mut messages, root.path(), "visible.rs");
    assert!(focus.observed());
    (root, focus, messages)
}

fn exec_failure(root: &Path, path: &str) -> String {
    format!(
        "FAILED (exit_code: 101)\ncwd: {}\nthread 'test' panicked at {path}:1:1:\nassertion failed",
        root.display()
    )
}

fn read(focus: &mut RecoveryFocus, messages: &mut Vec<Value>, root: &Path, path: &str) {
    let tc = call(
        &format!("read-{}", messages.len()),
        "read_file",
        json!({"path":path}),
    );
    let (output, error) = crate::file_tools::tool_read_file(path, root.to_str());
    assert!(!error);
    record(focus, messages, root, &tc, &output);
}

fn automatic_failure(
    focus: &mut RecoveryFocus,
    messages: &mut Vec<Value>,
    root: &Path,
    body: &str,
) {
    let tc = call("repair", "patch_file", json!({"path":"source.rs"}));
    record(
        focus,
        messages,
        root,
        &tc,
        &format!("OK: patched 'source.rs'\n[auto-test] ✗ FAILED (exit 101)\n{body}"),
    );
}

#[test]
fn newest_failure_routes_both_roles_without_discharging_the_older_check() {
    let (root, mut focus, mut messages) = fixture();
    automatic_failure(
        &mut focus,
        &mut messages,
        root.path(),
        "thread 'regression' panicked at later.rs:1:1:\nassertion failed",
    );
    let read_target = focus.required_read().unwrap();
    assert_eq!(
        (read_target.path.as_str(), read_target.failed_check.as_str()),
        ("later.rs", SECOND)
    );
    assert!(focus.hint().unwrap().contains(FIRST));
    read(&mut focus, &mut messages, root.path(), "visible.rs");
    assert!(
        focus.requires_read(),
        "old failure's read cannot diagnose the new failure"
    );
    let shared = agent::task_obligations::project(&messages, Some(SECOND));
    assert_eq!(shared.required_diagnostic_read.unwrap().path, "later.rs");
    read(&mut focus, &mut messages, root.path(), "later.rs");
    let mut resumed = RecoveryGovernor {
        stage: Some(RecoveryStage::Diagnose),
        ..Default::default()
    };
    resumed.restore_focus(&messages, root.path().to_str(), Some(SECOND));
    assert_eq!(resumed.stage, Some(RecoveryStage::Fix));
    assert!(focus.is_pending(), "diagnosis is not verification");

    let tc = call("new-pass", "exec", json!({"command":SECOND}));
    record(
        &mut focus,
        &mut messages,
        root.path(),
        &tc,
        &format!("OK (exit_code: 0)\ncwd: {}", root.path().display()),
    );
    assert!(
        focus.is_pending(),
        "passing the new check leaves the original one pending"
    );
    assert!(focus.check_matches(FIRST));
    assert!(
        !focus.requires_read(),
        "an unrelated failure did not invalidate the completed original diagnosis"
    );
    let tc = call("original-pass", "exec", json!({"command":FIRST}));
    record(
        &mut focus,
        &mut messages,
        root.path(),
        &tc,
        &format!("OK (exit_code: 0)\ncwd: {}", root.path().display()),
    );
    assert!(!focus.is_pending());
    assert!(!RecoveryFocus::from_messages(&messages, None, None).is_pending());
}

#[test]
fn the_same_check_can_report_a_new_file_after_a_repair() {
    let (root, mut focus, mut messages) = fixture();
    let tc = call("failed-again", "exec", json!({"command":FIRST}));
    record(
        &mut focus,
        &mut messages,
        root.path(),
        &tc,
        &exec_failure(root.path(), "later.rs"),
    );
    assert_eq!(focus.required_read().unwrap().path, "later.rs");
    assert_eq!(focus.required_read().unwrap().failed_check, FIRST);
    read(&mut focus, &mut messages, root.path(), "visible.rs");
    assert!(focus.requires_read());
}

#[test]
fn an_unlocalized_new_failure_does_not_borrow_an_older_path() {
    let (root, mut focus, mut messages) = fixture();
    automatic_failure(
        &mut focus,
        &mut messages,
        root.path(),
        "Error: test worker exited before reporting a location",
    );
    assert!(focus.required_read().is_none());
    assert!(focus.check_matches(SECOND));
    assert!(focus.hint().unwrap().contains("test worker exited"));
    read(&mut focus, &mut messages, root.path(), "visible.rs");
    assert!(!focus.observed());
    let tc = call("original-pass", "exec", json!({"command":FIRST}));
    record(
        &mut focus,
        &mut messages,
        root.path(),
        &tc,
        &format!("OK (exit_code: 0)\ncwd: {}", root.path().display()),
    );
    assert!(focus.check_matches(SECOND));
    assert!(focus.is_pending());
}

#[test]
fn current_failure_survives_pruning_and_an_older_success_does_not_clear_it() {
    let (root, mut focus, mut messages) = fixture();
    let tc = call("later", "exec", json!({"command":SECOND}));
    record(
        &mut focus,
        &mut messages,
        root.path(),
        &tc,
        &exec_failure(root.path(), "later.rs"),
    );
    let tc = call("original-pass", "exec", json!({"command":FIRST}));
    record(
        &mut focus,
        &mut messages,
        root.path(),
        &tc,
        &format!("OK (exit_code: 0)\ncwd: {}", root.path().display()),
    );
    for _ in 0..35 {
        read(&mut focus, &mut messages, root.path(), "visible.rs");
    }
    let context = ExecVerificationContext::from_root(Some(SECOND), "Repair the source");
    agent::message_window::prune_message_window_with_context(&mut messages, &context);
    let messages: Vec<Value> =
        serde_json::from_str(&serde_json::to_string(&messages).unwrap()).unwrap();
    let restored = RecoveryFocus::from_messages(&messages, root.path().to_str(), Some(SECOND));
    assert_eq!(restored, focus);
    assert_eq!(restored.required_read().unwrap().path, "later.rs");
    assert!(!agent::outcome::TaskOutcome::default().completed(&messages));
    let mut governor = RecoveryGovernor {
        required_verification: VerificationLevel::Behavioral,
        ..Default::default()
    };
    governor.restore_focus(&messages, root.path().to_str(), Some(SECOND));
    assert_eq!(governor.stage, Some(RecoveryStage::Diagnose));
}

#[test]
fn resolving_the_newest_check_restores_an_older_unread_diagnosis_in_the_live_governor() {
    let (root, mut focus, mut messages) = fixture();
    let original = call("original-failed-again", "exec", json!({"command":FIRST}));
    record(
        &mut focus,
        &mut messages,
        root.path(),
        &original,
        &exec_failure(root.path(), "visible.rs"),
    );
    let newest = call("newest", "exec", json!({"command":SECOND}));
    record(
        &mut focus,
        &mut messages,
        root.path(),
        &newest,
        &exec_failure(root.path(), "later.rs"),
    );
    read(&mut focus, &mut messages, root.path(), "later.rs");
    record(
        &mut focus,
        &mut messages,
        root.path(),
        &newest,
        &format!("OK (exit_code: 0)\ncwd: {}", root.path().display()),
    );
    let mut g = RecoveryGovernor {
        stage: Some(RecoveryStage::Verify),
        focus,
        ..Default::default()
    };
    g.on_exec_result(ExecKind::Verify, Some(VerificationLevel::Behavioral), true);
    assert_eq!(g.stage, Some(RecoveryStage::Diagnose));
    assert_eq!(g.focus.required_read().unwrap().path, "visible.rs");
    let harness = agent::TaskHarness::infer("Repair the source", false);
    assert!(g
        .maybe_block_tool(&original, Some(SECOND), harness, false)
        .is_some());
}

#[test]
fn unrelated_failures_and_ambiguous_locations_cannot_reuse_a_completed_diagnosis() {
    let (root, mut focus, mut messages) = fixture();
    automatic_failure(
        &mut focus,
        &mut messages,
        root.path(),
        "thread 'regression' panicked at later.rs:1:1:",
    );
    read(&mut focus, &mut messages, root.path(), "later.rs");
    let tc = call("missing", "read_file", json!({"path":"absent.rs"}));
    record(
        &mut focus,
        &mut messages,
        root.path(),
        &tc,
        "ERROR: file not found",
    );
    read(&mut focus, &mut messages, root.path(), "later.rs");
    assert!(!focus.progress_survives(&messages, root.path().to_str(), Some(SECOND)));
    let mut resumed = RecoveryGovernor {
        stage: Some(RecoveryStage::Diagnose),
        ..Default::default()
    };
    resumed.restore_focus(&messages, root.path().to_str(), Some(SECOND));
    assert_eq!(resumed.stage, Some(RecoveryStage::Diagnose));
    for body in [
        "error: see source.rs\n --> later.rs:1:1\n --> source.rs:1:1",
        "error: see source.rs\n --> ../outside.rs:1:1",
        "error: see source.rs\n --> missing.rs:1:1",
    ] {
        automatic_failure(&mut focus, &mut messages, root.path(), body);
        assert!(focus.required_read().is_none(), "{body}");
        assert!(focus.contains_check(FIRST) && focus.contains_check(SECOND));
    }
}

#[test]
fn refusals_wrong_roots_and_non_verification_failures_do_not_replace_check_ownership() {
    let (root, focus, messages) = fixture();
    for (command, output) in [
        (SECOND, "GOVERNOR BLOCKED\nFAILED (exit_code: 1)".into()),
        (SECOND, "REJECTED BY USER".into()),
        (
            SECOND,
            "FAILED (exit_code: 1)\ncwd: /missing\n --> later.rs:1:1".into(),
        ),
        ("git status", exec_failure(root.path(), "later.rs")),
        (
            "cd nested && cargo test",
            exec_failure(root.path(), "later.rs"),
        ),
    ] {
        let mut actual = focus.clone();
        let mut history = messages.clone();
        let tc = call("refused", "exec", json!({"command":command}));
        record(&mut actual, &mut history, root.path(), &tc, &output);
        assert_eq!(actual, focus, "{command}: {output}");
    }
}

#[test]
fn automatic_success_restores_the_remaining_explicit_check_through_handoff() {
    let (root, mut focus, mut messages) = fixture();
    automatic_failure(
        &mut focus,
        &mut messages,
        root.path(),
        "thread 'regression' panicked at later.rs:1:1:",
    );
    read(&mut focus, &mut messages, root.path(), "later.rs");
    messages.extend([
        json!({"role":"assistant","tool_calls":[{"id":"blocked","function":{"name":"exec","arguments":"{\"command\":\"cargo test --locked --bin spiral-coder -q\"}"}}]}),
        json!({"role":"tool","tool_call_id":"blocked","content":"GOVERNOR BLOCKED"}),
    ]);
    let tc = call("repair-passes", "patch_file", json!({"path":"source.rs"}));
    record(
        &mut focus,
        &mut messages,
        root.path(),
        &tc,
        "OK: patched 'source.rs'\n[auto-test] ✓ PASSED (exit 0)",
    );
    assert!(focus.check_matches(FIRST) && focus.repaired());
    let mut live = RecoveryGovernor {
        stage: Some(RecoveryStage::Fix),
        focus: focus.clone(),
        ..Default::default()
    };
    live.on_successful_edit(AutoTestOutcome::Passed, Some(VerificationLevel::Behavioral));
    assert_eq!(live.stage, Some(RecoveryStage::Verify));
    let context = ExecVerificationContext::from_messages(Some(SECOND), &messages);
    for prune in [false, true] {
        let mut saved = messages.clone();
        if prune {
            agent::message_window::prune_message_window_with_context(&mut saved, &context);
        }
        let saved: Vec<Value> =
            serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap();
        let memory = agent::FailureMemory::from_recent_messages(&saved, Some(SECOND));
        assert!(memory.consecutive_failures > 0);
        let mut resumed =
            RecoveryGovernor::restore_from_session(&memory, &saved, VerificationLevel::Behavioral);
        resumed.restore_focus(&saved, root.path().to_str(), Some(SECOND));
        assert_eq!(resumed.focus, focus);
        assert_eq!(resumed.stage, Some(RecoveryStage::Verify));
        let verify = call("remaining", "exec", json!({"command":FIRST}));
        let harness = agent::TaskHarness::infer("Repair source.rs", false);
        assert!(resumed
            .maybe_block_tool(&verify, Some(SECOND), harness, false)
            .is_none());
    }
    let failed_read = call("missing", "read_file", json!({"path":"absent.rs"}));
    record(
        &mut focus,
        &mut messages,
        root.path(),
        &failed_read,
        "ERROR: file not found",
    );
    read(&mut focus, &mut messages, root.path(), "visible.rs");
    assert!(
        !focus.progress_survives(&messages, root.path().to_str(), Some(SECOND)),
        "resolving a check cannot waive a newer unrelated read failure"
    );
}

#[test]
fn actual_pruning_preserves_the_success_that_resolved_a_different_check() {
    for automatic in [false, true] {
        let (root, mut focus, mut messages) = fixture();
        let check = call("second-failed", "exec", json!({"command":SECOND}));
        record(
            &mut focus,
            &mut messages,
            root.path(),
            &check,
            &exec_failure(root.path(), "later.rs"),
        );
        read(&mut focus, &mut messages, root.path(), "later.rs");
        let passed = if automatic {
            call("second-passed", "patch_file", json!({"path":"source.rs"}))
        } else {
            call("second-passed", "exec", json!({"command":SECOND}))
        };
        let result = if automatic {
            "OK: patched 'source.rs'\n[auto-test] ✓ PASSED (exit 0)".into()
        } else {
            format!("OK (exit_code: 0)\ncwd: {}", root.path().display())
        };
        record(&mut focus, &mut messages, root.path(), &passed, &result);
        let edit = call("repair-only", "patch_file", json!({"path":"source.rs"}));
        let context = ExecVerificationContext::from_root(None, "Repair source.rs");
        focus.record_result(
            &edit,
            "OK: patched 'source.rs'",
            root.path().to_str(),
            None,
            &context,
        );
        messages.extend([
            json!({"role":"assistant","tool_calls":[{"id":edit.id,"function":{"name":edit.name,"arguments":edit.arguments}}]}),
            focus.tool_message(&edit, "OK: patched 'source.rs'".into()),
        ]);
        let disposable = call("disposable-pwd", "exec", json!({"command":"pwd"}));
        record(
            &mut focus,
            &mut messages,
            root.path(),
            &disposable,
            &format!("OK (exit_code: 0)\ncwd: {}", root.path().display()),
        );
        for _ in 0..30 {
            read(&mut focus, &mut messages, root.path(), "source.rs");
        }
        let before = messages.len();
        assert!(before > 48);
        agent::message_window::prune_message_window_with_context(&mut messages, &context);
        assert!(
            messages.len() < before,
            "pruning must really remove an exchange"
        );
        assert!(!messages
            .iter()
            .any(|m| m["tool_call_id"] == "disposable-pwd"));
        assert!(messages
            .iter()
            .any(|m| m["tool_call_id"] == "second-failed"));
        assert!(messages
            .iter()
            .any(|m| m["tool_call_id"] == "second-passed"));
        let saved: Vec<Value> =
            serde_json::from_str(&serde_json::to_string(&messages).unwrap()).unwrap();
        let memory = agent::FailureMemory::from_recent_messages(&saved, None);
        let mut resumed =
            RecoveryGovernor::restore_from_session(&memory, &saved, VerificationLevel::Behavioral);
        resumed.restore_focus(&saved, root.path().to_str(), None);
        assert_eq!(resumed.focus, focus);
        assert_eq!(resumed.stage, Some(RecoveryStage::Verify));
    }
}
