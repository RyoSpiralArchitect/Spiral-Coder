use super::*;
use crate::tui::agent::{self, RecoveryGovernor, RecoveryStage, VerificationLevel};

const CHECK: &str = "cargo test --offline --color never";

fn call(id: &str, name: &str, args: Value) -> ToolCallData {
    ToolCallData {
        id: id.into(),
        name: name.into(),
        arguments: args.to_string(),
        thought_signature: None,
    }
}

fn append(messages: &mut Vec<Value>, tc: &ToolCallData, result: Value) {
    messages.push(json!({"role":"assistant","tool_calls":[{"id":tc.id,"function":{"name":tc.name,"arguments":tc.arguments}}]}));
    messages.push(result);
}

fn output(root: &Path, content: &str) -> String {
    agent::inject_cwd(content, &format!("cwd: {}", root.display()), None)
}

fn check(root: &Path) -> (String, bool) {
    let result = std::process::Command::new("cargo")
        .args(["test", "--offline", "--color", "never"])
        .current_dir(root)
        .env("CARGO_TARGET_DIR", root.join("target"))
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&result.stdout);
    let stderr = String::from_utf8_lossy(&result.stderr);
    let success = result.status.success();
    let content = if success {
        agent::build_ok_tool_output(&stdout)
    } else {
        agent::build_failed_tool_output(&stdout, &stderr, result.status.code().unwrap_or(-1))
    };
    (output(root, &content), success)
}

#[test]
fn real_baseline_failure_survives_pruning_and_handoff_until_the_original_check_passes() {
    let root = tempfile::tempdir().unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/runtime-resume-damaged-exchange");
    std::fs::create_dir(root.path().join("src")).unwrap();
    for file in ["Cargo.toml", "src/lib.rs"] {
        std::fs::copy(source.join(file), root.path().join(file)).unwrap();
    }
    let configured = Some("cargo test 2>&1");
    let context = ExecVerificationContext::from_root(configured, "Fix the failing greet test");
    let mut messages = vec![json!({"role":"user","content":"Fix the failing greet test"})];
    let mut g = RecoveryGovernor {
        required_verification: VerificationLevel::Behavioral,
        ..Default::default()
    };
    let read = call("read", "read_file", json!({"path":"src/lib.rs"}));
    let (before, failed) = crate::file_tools::tool_read_file("src/lib.rs", root.path().to_str());
    assert!(!failed);
    append(&mut messages, &read, g.focus.tool_message(&read, before));
    let exec = call("baseline", "exec", json!({"command":CHECK}));
    let (failed_output, success) = check(root.path());
    assert!(!success);
    g.focus.record_result(
        &exec,
        &failed_output,
        root.path().to_str(),
        configured,
        &context,
    );
    g.on_exec_result(ExecKind::Verify, Some(VerificationLevel::Behavioral), false);
    assert!(
        g.focus.requires_read(),
        "the read before failure is not a fresh diagnosis: {failed_output}"
    );
    assert!(g.focus.hint().unwrap().contains("src/lib.rs"));
    append(
        &mut messages,
        &exec,
        g.focus.tool_message(&exec, failed_output),
    );
    let mut immediate_handoff = messages.clone();
    let repair = call(
        "repair",
        "patch_file",
        json!({"path":"src/lib.rs","search":"Hello, {}?","replace":"Hello, {}!"}),
    );
    let harness = agent::TaskHarness::infer("Fix the failing greet test", false);
    assert!(g
        .maybe_block_tool(&repair, configured, harness, false)
        .unwrap()
        .contains("src/lib.rs"));
    for i in 0..35 {
        let tc = call(&format!("pwd-{i}"), "exec", json!({"command":"pwd"}));
        let result = output(root.path(), "OK (exit_code: 0)");
        g.focus
            .record_result(&tc, &result, root.path().to_str(), configured, &context);
        g.on_diagnostic_result(true);
        append(&mut messages, &tc, g.focus.tool_message(&tc, result));
    }
    assert_eq!(g.stage, Some(RecoveryStage::Diagnose));
    agent::message_window::prune_message_window_with_context(&mut messages, &context);
    assert_eq!(
        agent::task_obligations::project(&messages, configured).pending_recovery,
        vec![g.focus.hint().unwrap()]
    );
    let mut resumed = RecoveryGovernor::default();
    resumed.restore_focus(&messages, root.path().to_str(), configured);
    assert_eq!(resumed.focus, g.focus);
    let (diagnosed, error) = crate::file_tools::tool_read_file("src/lib.rs", root.path().to_str());
    assert!(!error);
    resumed.focus.record_result(
        &read,
        &diagnosed,
        root.path().to_str(),
        configured,
        &context,
    );
    resumed.on_diagnostic_result(true);
    append(
        &mut messages,
        &read,
        resumed.focus.tool_message(&read, diagnosed),
    );
    append(
        &mut immediate_handoff,
        &read,
        messages.last().unwrap().clone(),
    );
    let mem = agent::FailureMemory::from_recent_messages(&immediate_handoff);
    assert!(mem.consecutive_failures > 0);
    let mut direct = RecoveryGovernor::restore_from_session(
        &mem,
        &immediate_handoff,
        VerificationLevel::Behavioral,
    );
    direct.restore_focus(&immediate_handoff, root.path().to_str(), configured);
    assert_eq!(
        direct.stage,
        Some(RecoveryStage::Fix),
        "completed diagnosis must survive the failed exec still in failure memory"
    );

    // A different failure must still be diagnosed, even if the old target is read again.
    let unrelated = call("new-failure", "exec", json!({"command":"git status"}));
    let new_failure = output(
        root.path(),
        "FAILED (exit_code: 128)\nstderr:\nfatal: not a git repository",
    );
    append(
        &mut immediate_handoff,
        &unrelated,
        json!({"role":"tool","tool_call_id":unrelated.id,"content":new_failure}),
    );
    append(
        &mut immediate_handoff,
        &read,
        messages.last().unwrap().clone(),
    );
    let mem = agent::FailureMemory::from_recent_messages(&immediate_handoff);
    let mut new_failure = RecoveryGovernor::restore_from_session(
        &mem,
        &immediate_handoff,
        VerificationLevel::Behavioral,
    );
    new_failure.restore_focus(&immediate_handoff, root.path().to_str(), configured);
    assert_eq!(new_failure.stage, Some(RecoveryStage::Diagnose));
    messages.push(crate::task_origin::user_message(
        "Continue",
        crate::task_origin::MessageOrigin::Runtime,
    ));
    let mut handoff = RecoveryGovernor::default();
    handoff.restore_focus(&messages, root.path().to_str(), configured);
    assert_eq!(handoff.stage, Some(RecoveryStage::Fix));
    assert!(handoff
        .maybe_block_tool(&repair, configured, harness, false)
        .is_none());
    let (patched, error) = crate::file_tools::tool_patch_file(
        "src/lib.rs",
        "Hello, {}?",
        "Hello, {}!",
        root.path().to_str(),
    );
    assert!(!error);
    handoff.focus.record_result(
        &repair,
        &patched,
        root.path().to_str(),
        configured,
        &context,
    );
    handoff.on_successful_edit(AutoTestOutcome::NotRun, None);
    assert_eq!(handoff.stage, Some(RecoveryStage::Verify));
    let different = call("different-check", "exec", json!({"command":"cargo check"}));
    handoff.focus.record_result(
        &different,
        &output(root.path(), "OK (exit_code: 0)"),
        root.path().to_str(),
        configured,
        &context,
    );
    assert!(
        handoff.focus.is_pending(),
        "another successful check cannot discharge the original failure"
    );
    let (passed, success) = check(root.path());
    assert!(success, "{passed}");
    handoff
        .focus
        .record_result(&exec, &passed, root.path().to_str(), configured, &context);
    handoff.on_exec_result(ExecKind::Verify, Some(VerificationLevel::Behavioral), true);
    assert!(!handoff.focus.is_pending());
    assert_eq!(handoff.stage, None);
}

#[test]
fn baseline_focus_requires_actual_failed_verification_at_root_and_native_correlation() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("lib.rs"), "broken").unwrap();
    let context = ExecVerificationContext::from_root(None, "Fix tests");
    let diagnostic = "thread 'test' (123) panicked at lib.rs:1:2:\nassertion failed";
    for (command, header, cwd) in [
        ("cargo test", "GOVERNOR BLOCKED", true),
        ("cargo test", "REJECTED BY USER", true),
        ("cargo test", "OK (exit_code: 0)", true),
        ("cargo test", "FAILED (exit_code: 1)", false),
        ("echo cargo test", "FAILED (exit_code: 1)", true),
        ("cd src && cargo test", "FAILED (exit_code: 1)", true),
        ("cargo test || false", "FAILED (exit_code: 1)", true),
    ] {
        let tc = call("failed", "exec", json!({"command":command}));
        let content = format!("{header}\n{diagnostic}");
        let content = if cwd {
            output(root.path(), &content)
        } else {
            content
        };
        let mut focus = RecoveryFocus::default();
        focus.record_result(&tc, &content, root.path().to_str(), None, &context);
        assert!(!focus.is_pending(), "{command}: {header}, cwd={cwd}");
    }
    let tc = call("failed", "exec", json!({"command":"cargo test"}));
    let content = output(root.path(), &format!("FAILED (exit_code: 1)\n{diagnostic}"));
    let orphan = json!({"role":"tool","tool_call_id":"failed","content":content});
    assert!(
        !RecoveryFocus::from_messages(&[orphan.clone()], root.path().to_str(), None).is_pending()
    );
    let mut messages = Vec::new();
    append(&mut messages, &tc, orphan);
    assert!(RecoveryFocus::from_messages(&messages, root.path().to_str(), None).requires_read());
    messages.push(json!({"role":"user","content":"New task"}));
    assert!(!RecoveryFocus::from_messages(&messages, root.path().to_str(), None).is_pending());
}

#[test]
fn initial_location_rejects_ambiguous_missing_or_ordinary_text_paths() {
    let root = tempfile::tempdir().unwrap();
    for file in ["a.rs", "b.rs"] {
        std::fs::write(root.path().join(file), "source").unwrap();
    }
    let context = ExecVerificationContext::from_root(None, "Fix tests");
    for body in [
        "Error: see a.rs for documentation",
        " --> missing.rs:1:2",
        " --> ../a.rs:1:2",
        " --> a.rs:0:2",
        " --> a.rs:1:no-column",
        " --> a.rs:1:2\n --> b.rs:2:3",
        "example: thread 'test' panicked at a.rs:1:2:",
    ] {
        assert!(
            localize(
                "cargo test",
                &format!("FAILED (exit_code: 1)\n{body}"),
                root.path().to_str(),
                &context
            )
            .is_none(),
            "{body}"
        );
    }
}

#[test]
fn compiler_and_python_failures_retain_a_unique_existing_location() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("a.rs"), "source").unwrap();
    std::fs::write(root.path().join("a b.py"), "source").unwrap();
    let context = ExecVerificationContext::from_root(Some("python a.py"), "Fix tests");
    for (command, body, expected) in [
        ("cargo check", "error[E0001]: broken\n --> a.rs:1:2".into(), "a.rs"),
        ("python a.py", format!("Traceback (most recent call last):\n  File \"caller.py\", line 1, in main\n  File \"{}\", line 2, in main\nAssertionError: broken", root.path().join("a b.py").display()), "a b.py"),
    ] {
        let focus = localize(command, &format!("FAILED (exit_code: 1)\n{body}"), root.path().to_str(), &context).unwrap();
        assert_eq!((focus.path.as_str(), focus.check.as_str(), focus.observation), (expected, command, Observation::AwaitingRead));
    }
}

#[cfg(unix)]
#[test]
fn baseline_diagnostics_cannot_follow_a_symlink_outside_the_workspace() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("a.rs"), "source").unwrap();
    std::os::unix::fs::symlink(outside.path().join("a.rs"), root.path().join("a.rs")).unwrap();
    let context = ExecVerificationContext::from_root(None, "Fix tests");
    assert!(localize(
        "cargo test",
        "FAILED (exit_code: 1)\n --> a.rs:1:2",
        root.path().to_str(),
        &context
    )
    .is_none());
}
