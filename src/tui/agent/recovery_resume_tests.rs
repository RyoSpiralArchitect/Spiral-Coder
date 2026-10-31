//! Cross-owner continuation regressions. All receipts are model-free except the
//! explicitly ignored test, which consumes the retained r11 session unchanged.
use super::*;
use crate::tui::agent::exec_verification::ExecVerificationContext;
use crate::tui::agent::recovery_focus::RecoveryFocus;
use serde_json::Value;

const FIRST: &str = "cargo test visible_failure";
const SECOND: &str = "cargo test newer_failure";

fn call(id: &str, name: &str, arguments: Value) -> ToolCallData {
    ToolCallData {
        id: id.into(),
        name: name.into(),
        arguments: arguments.to_string(),
        thought_signature: None,
    }
}

struct Session {
    root: tempfile::TempDir,
    focus: RecoveryFocus,
    messages: Vec<Value>,
}

impl Session {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        for path in ["visible.rs", "later.rs", "source.rs", "other.rs"] {
            std::fs::write(root.path().join(path), "source\n").unwrap();
        }
        Self {
            root,
            focus: RecoveryFocus::default(),
            messages: vec![crate::task_origin::user_message(
                "Repair existing source, preserving supplied tests",
                crate::task_origin::MessageOrigin::User,
            )],
        }
    }

    fn record(&mut self, name: &str, arguments: Value, output: &str) {
        let tc = call(&format!("call-{}", self.messages.len()), name, arguments);
        let context = ExecVerificationContext::from_messages(Some(FIRST), &self.messages);
        self.focus.record_result(
            &tc,
            output,
            self.root.path().to_str(),
            Some(FIRST),
            &context,
        );
        self.messages.push(json!({"role":"assistant","tool_calls":[{
            "id":tc.id,"type":"function",
            "function":{"name":tc.name,"arguments":tc.arguments}
        }]}));
        self.messages
            .push(self.focus.tool_message(&tc, output.into()));
    }

    fn fail_check(&mut self, command: &str, path: &str) {
        let output = format!(
            "FAILED (exit_code: 101)\ncwd: {}\nthread 'test' panicked at {path}:1:1:\nassertion failed",
            self.root.path().display()
        );
        self.record("exec", json!({"command":command}), &output);
    }

    fn pass_check(&mut self, command: &str) {
        let output = format!("OK (exit_code: 0)\ncwd: {}", self.root.path().display());
        self.record("exec", json!({"command":command}), &output);
    }

    fn fail_edit(&mut self, path: &str) {
        let (output, failed) = crate::file_tools::tool_patch_file(
            path,
            "absent anchor",
            "replacement",
            self.root.path().to_str(),
        );
        assert!(failed);
        self.record(
            "patch_file",
            json!({"path":path,"search":"absent anchor","replace":"replacement"}),
            &output,
        );
    }

    fn read(&mut self, path: &str) {
        let (output, failed) =
            crate::file_tools::tool_read_file(path, self.root.path().to_str());
        assert!(!failed, "{output}");
        self.record("read_file", json!({"path":path}), &output);
    }

    fn restore(&self) -> RecoveryGovernor {
        let mut saved: Vec<Value> =
            serde_json::from_str(&serde_json::to_string(&self.messages).unwrap()).unwrap();
        saved.push(crate::task_origin::user_message(
            "Continue.",
            crate::task_origin::MessageOrigin::Runtime,
        ));
        let context = ExecVerificationContext::from_messages(Some(FIRST), &saved);
        let edits = EditFailureMemory::from_messages(&saved, &context);
        let memory = FailureMemory::from_recent_messages(&saved, Some(FIRST));
        let mut governor =
            RecoveryGovernor::restore_from_session(&memory, &saved, VerificationLevel::Behavioral);
        governor.restore_pending_edits(&edits);
        governor.restore_focus(&saved, self.root.path().to_str(), Some(FIRST));
        governor
    }

    fn assert_stage(&self, stage: RecoveryStage) -> RecoveryGovernor {
        let governor = self.restore();
        assert_eq!(governor.stage, Some(stage));
        let repair = call(
            "next-repair",
            "patch_file",
            json!({"path":"source.rs","search":"source","replace":"repaired"}),
        );
        assert_eq!(
            governor
                .maybe_block_tool(
                    &repair,
                    Some(FIRST),
                    TaskHarness::infer("Repair existing source", false),
                    false,
                )
                .is_some(),
            stage != RecoveryStage::Fix,
        );
        assert!(!crate::tui::agent::outcome::TaskOutcome::default().completed(&self.messages));
        governor
    }
}

fn diagnosed() -> Session {
    let mut session = Session::new();
    session.fail_check(FIRST, "visible.rs");
    session.read("visible.rs");
    session.fail_edit("source.rs");
    session.read("source.rs");
    session
}

#[test]
fn completed_source_diagnosis_survives_repeated_handoffs_without_verification() {
    let mut session = diagnosed();
    for _ in 0..3 {
        let governor = session.assert_stage(RecoveryStage::Fix);
        assert!(governor.focus.observed());
        assert!(governor.focus.check_matches(FIRST));
        assert!(!governor.focus.repaired());
        session.record(
            "patch_file",
            json!({"path":"source.rs"}),
            "GOVERNOR BLOCKED\nERROR: not executed",
        );
        session.read("source.rs");
    }
}

#[test]
fn absent_unrelated_stale_and_wrong_header_reads_do_not_diagnose_an_edit() {
    for variant in [
        "absent",
        "unrelated",
        "stale",
        "focused",
        "wrong-header",
        "orphan",
    ] {
        let mut session = Session::new();
        session.fail_check(FIRST, "visible.rs");
        session.read("visible.rs");
        if variant == "stale" {
            session.read("source.rs");
        }
        session.fail_edit("source.rs");
        match variant {
            "unrelated" => session.read("other.rs"),
            "focused" => session.read("visible.rs"),
            "wrong-header" => session.record(
                "read_file",
                json!({"path":"source.rs"}),
                "[other.rs] (1 lines, 7 bytes)\nsource\n",
            ),
            "orphan" => {
                session.read("source.rs");
                session.messages.last_mut().unwrap()["tool_call_id"] = json!("not-called");
            }
            _ => {}
        }
        session.assert_stage(RecoveryStage::Diagnose);
    }
}

#[test]
fn a_newer_check_keeps_its_own_diagnosis_and_older_check_ownership() {
    for command in [FIRST, SECOND] {
        let mut session = diagnosed();
        session.fail_check(command, "later.rs");
        session.read("source.rs");
        session.read("visible.rs");
        let governor = session.assert_stage(RecoveryStage::Diagnose);
        let required = governor.focus.required_read().unwrap();
        assert_eq!(required.path, "later.rs");
        assert_eq!(required.failed_check, command);
        session.read("later.rs");
        session.assert_stage(RecoveryStage::Fix);
        if command == SECOND {
            session.pass_check(SECOND);
            let governor = session.assert_stage(RecoveryStage::Fix);
            assert!(governor.focus.check_matches(FIRST));
            assert!(governor.focus.is_pending());
        }
    }
}

#[test]
fn a_newer_edit_requires_its_own_target_even_after_reading_the_old_source() {
    let mut session = diagnosed();
    session.fail_edit("other.rs");
    session.read("source.rs");
    session.read("visible.rs");
    session.assert_stage(RecoveryStage::Diagnose);
    session.read("other.rs");
    session.assert_stage(RecoveryStage::Fix);
}

#[test]
fn diagnosing_the_newest_edit_does_not_erase_an_older_unread_edit() {
    let mut session = Session::new();
    session.fail_check(FIRST, "visible.rs");
    session.read("visible.rs");
    session.fail_edit("source.rs");
    session.fail_edit("other.rs");
    session.read("other.rs");
    session.assert_stage(RecoveryStage::Diagnose);
    session.read("source.rs");
    session.assert_stage(RecoveryStage::Fix);
}

#[test]
fn a_failed_read_invalidates_only_its_own_diagnosis_and_must_be_retried() {
    for path in ["source.rs", "other.rs", "visible.rs"] {
        let mut session = diagnosed();
        session.record("read_file", json!({"path":path}), "ERROR: access denied");
        // The former implementation could reuse a source read after an
        // unrelated failed read because EditFailureMemory still said "read".
        if path != "source.rs" {
            session.read("source.rs");
        }
        session.assert_stage(RecoveryStage::Diagnose);
        session.read(path);
        session.assert_stage(RecoveryStage::Fix);
    }
}

#[test]
fn overlapping_read_edit_and_check_failures_keep_separate_diagnostic_obligations() {
    let mut session = diagnosed();
    session.record(
        "read_file",
        json!({"path":"other.rs"}),
        "ERROR: temporary read failure",
    );
    session.fail_edit("source.rs");
    session.fail_edit("other.rs");
    session.fail_check(SECOND, "later.rs");
    session.read("later.rs");
    session.assert_stage(RecoveryStage::Diagnose);
    session.read("other.rs");
    session.assert_stage(RecoveryStage::Diagnose);
    session.read("source.rs");
    let governor = session.assert_stage(RecoveryStage::Fix);
    assert!(governor.focus.check_matches(SECOND));
    assert!(governor.focus.hint().unwrap().contains(FIRST));
    session.pass_check(SECOND);
    assert!(session
        .assert_stage(RecoveryStage::Fix)
        .focus
        .check_matches(FIRST));
}

#[test]
fn a_read_after_a_failed_edit_never_reuses_an_older_repair_attempt() {
    let mut session = diagnosed();
    session.record(
        "patch_file",
        json!({"path":"source.rs"}),
        "OK: patched 'source.rs'",
    );
    assert!(session.focus.repaired());
    session.fail_edit("other.rs");
    session.read("other.rs");
    session.assert_stage(RecoveryStage::Fix);
}

#[test]
fn repair_and_matching_native_verification_are_still_required_after_diagnosis() {
    let mut session = diagnosed();
    let mut governor = session.assert_stage(RecoveryStage::Fix);
    session.record(
        "patch_file",
        json!({"path":"source.rs"}),
        "OK: patched 'source.rs'",
    );
    governor.focus = session.focus.clone();
    governor.on_successful_edit(AutoTestOutcome::NotRun, None);
    assert_eq!(governor.stage, Some(RecoveryStage::Verify));
    session.pass_check(SECOND);
    governor.focus = session.focus.clone();
    governor.on_exec_result(ExecKind::Verify, Some(VerificationLevel::Behavioral), true);
    assert_eq!(governor.stage, Some(RecoveryStage::Verify));
    assert!(governor.focus.check_matches(FIRST));
    session.pass_check(FIRST);
    governor.focus = session.focus.clone();
    governor.on_exec_result(ExecKind::Verify, Some(VerificationLevel::Behavioral), true);
    assert_eq!(governor.stage, None);
    assert!(!governor.focus.is_pending());
}

#[test]
fn an_unlocalized_or_wrong_root_failure_cannot_borrow_edit_diagnosis() {
    for variant in ["unlocalized", "wrong-root", "diagnostic"] {
        let mut session = diagnosed();
        let command = if variant == "diagnostic" {
            "git status"
        } else {
            SECOND
        };
        let cwd = if variant == "wrong-root" {
            "/not-the-workspace".to_string()
        } else {
            session.root.path().display().to_string()
        };
        session.record(
            "exec",
            json!({"command":command}),
            &format!("FAILED (exit_code: 1)\ncwd: {cwd}\nworker exited without a location"),
        );
        session.read("source.rs");
        session.assert_stage(RecoveryStage::Diagnose);
    }
}

#[test]
fn edit_only_recovery_also_preserves_a_later_failed_read() {
    let mut session = Session::new();
    session.fail_edit("source.rs");
    session.read("source.rs");
    session.assert_stage(RecoveryStage::Fix);
    session.record("read_file", json!({"path":"other.rs"}), "ERROR: access denied");
    session.assert_stage(RecoveryStage::Diagnose);
    session.read("other.rs");
    session.assert_stage(RecoveryStage::Fix);
}

#[test]
fn a_new_human_task_does_not_inherit_the_previous_edit_obligations() {
    let mut session = diagnosed();
    session.messages.push(crate::task_origin::user_message(
        "Inspect a different repository",
        crate::task_origin::MessageOrigin::User,
    ));
    let governor = session.restore();
    assert_eq!(governor.stage, None);
    assert!(!governor.focus.is_pending());
}

#[test]
#[ignore = "requires SPIRAL_DIAGNOSTIC_SESSION and SPIRAL_DIAGNOSTIC_WORKSPACE from the retained r11 bundle"]
fn native_source_diagnosis_survives_a_restored_continuation() {
    let session = std::env::var("SPIRAL_DIAGNOSTIC_SESSION").expect("retained r11 session path");
    let root = std::env::var("SPIRAL_DIAGNOSTIC_WORKSPACE").expect("retained r11 workspace path");
    let saved: Value = serde_json::from_slice(&std::fs::read(session).unwrap()).unwrap();
    let messages = saved["messages"].as_array().unwrap();
    let check = "python3 -S -m unittest discover -s tests -p test_coupled_pilot.py -v";
    let source = "scripts/run_coupled_pilot.py";
    let mut diagnosed = Vec::new();
    for (index, message) in messages.iter().enumerate() {
        if message["role"] != "tool"
            || !message["content"]
                .as_str()
                .is_some_and(|s| s.starts_with(&format!("[{source}]")))
        {
            continue;
        }
        let mut prefix = messages[..=index].to_vec();
        prefix.push(crate::task_origin::user_message(
            "Continue.",
            crate::task_origin::MessageOrigin::Runtime,
        ));
        let context = ExecVerificationContext::from_messages(Some(check), &prefix);
        let edits = EditFailureMemory::from_messages(&prefix, &context);
        if edits.count() == 0 || !edits.target_was_read() {
            continue;
        }
        let failures = FailureMemory::from_recent_messages(&prefix, Some(check));
        let mut restored = RecoveryGovernor::restore_from_session(
            &failures,
            &prefix,
            VerificationLevel::Behavioral,
        );
        restored.restore_pending_edits(&edits);
        restored.restore_focus(&prefix, Some(&root), Some(check));
        assert!(restored.focus.is_pending() && restored.focus.check_matches(check));
        assert_eq!(restored.stage, Some(RecoveryStage::Fix));
        assert!(!restored.focus.repaired());
        assert!(!crate::tui::agent::outcome::TaskOutcome::default().completed(&prefix));
        diagnosed.push(message["tool_call_id"].as_str().unwrap().to_string());
    }
    assert_eq!(
        diagnosed,
        [
            "call_tbsQEus6nL6MkWq6gQTWYkv8",
            "call_bTsalwxB8uiU1YhDGRzhmIsv",
            "call_rO0f6iWvOg2tvcwq2qQgJY5q",
        ],
        "all three actual post-failure native reads must remain in the input",
    );
}
