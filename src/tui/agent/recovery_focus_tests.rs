use super::*;
use crate::tui::agent::{self, RecoveryGovernor, RecoveryStage, VerificationLevel};

const CHECK: &str = "spiral-coder --tui-replay .spiral-coder/tui_replay.json";
const TARGET: &str = ".spiral-coder/tui_replay.json";

fn call(id: &str, name: &str, arguments: Value) -> ToolCallData {
    ToolCallData {
        id: id.into(),
        name: name.into(),
        arguments: arguments.to_string(),
    }
}

fn append(messages: &mut Vec<Value>, tc: &ToolCallData, result: Value) {
    messages.push(json!({"role":"assistant","tool_calls":[{"id":tc.id,"function":{"name":tc.name,"arguments":tc.arguments}}]}));
    messages.push(result);
}

fn context() -> ExecVerificationContext<'static> {
    ExecVerificationContext::from_root(Some(CHECK), "Repair the existing replay")
}

fn exec_output(status: &str, root: &Path) -> String {
    agent::inject_cwd(status, &format!("cwd: {}", root.display()), None)
}

fn failed_json(root: &Path) -> (ToolCallData, String) {
    let tc = call(
        "bad-edit",
        "write_file",
        json!({"path":TARGET,"content":"{\"a\":1,}"}),
    );
    let (mut result, error) =
        crate::file_tools::tool_write_file(TARGET, "{\"a\":1,}", root.to_str());
    assert!(!error);
    let error = serde_json::from_str::<Value>("{\"a\":1,}").unwrap_err();
    result.push_str(&format!("\n[auto-test] ✗ FAILED (exit 1)\nError: failed to parse JSON: expected key at line {} column {}", error.line(), error.column()));
    (tc, result)
}

fn focused(root: &Path) -> (RecoveryGovernor, Vec<Value>) {
    let (tc, result) = failed_json(root);
    let mut g = RecoveryGovernor {
        required_verification: VerificationLevel::Behavioral,
        ..Default::default()
    };
    g.focus
        .record_result(&tc, &result, root.to_str(), Some(CHECK), &context());
    g.on_successful_edit(AutoTestOutcome::Failed, None);
    assert!(g.focus.requires_read());
    let mut messages = vec![json!({"role":"user","content":"Repair the existing replay"})];
    append(&mut messages, &tc, g.focus.tool_message(&tc, result));
    (g, messages)
}

fn read_target(g: &mut RecoveryGovernor, root: &Path) -> (ToolCallData, String) {
    let read = call(
        "read-target",
        "read_file",
        json!({"path":format!("./{TARGET}")}),
    );
    let (output, error) = crate::file_tools::tool_read_file(TARGET, root.to_str());
    assert!(!error);
    g.focus
        .record_result(&read, &output, root.to_str(), Some(CHECK), &context());
    g.on_diagnostic_result(true);
    (read, output)
}

#[test]
fn automatic_failure_keeps_exact_diagnosis_through_unrelated_successes_and_allows_read_backed_repair(
) {
    let root = tempfile::tempdir().unwrap();
    let (mut g, _) = focused(root.path());
    let harness = agent::TaskHarness::infer("Repair the existing replay", false);
    let repair = call(
        "repair",
        "patch_file",
        json!({"path":TARGET,"search":",}","replace":"}"}),
    );
    let block = g
        .maybe_block_tool(&repair, Some(CHECK), harness, false)
        .unwrap();
    assert!(block.contains(TARGET) && block.contains("read_file"));
    assert!(!block.contains("e.g. `pwd`"));
    for (tc, output) in [
        (
            call("pwd", "exec", json!({"command":"pwd"})),
            "OK (exit_code: 0)\n/workspace",
        ),
        (
            call("other", "read_file", json!({"path":"README.md"})),
            "[README.md] (1 lines, 8 bytes)\nREADME",
        ),
        (
            call("search", "search_files", json!({"pattern":"replay"})),
            "matches found",
        ),
    ] {
        g.focus
            .record_result(&tc, output, root.path().to_str(), Some(CHECK), &context());
        g.on_diagnostic_result(true);
        assert_eq!(g.stage, Some(RecoveryStage::Diagnose));
        assert!(g.focus.requires_read());
    }
    read_target(&mut g, root.path());
    assert_eq!(g.stage, Some(RecoveryStage::Fix));
    assert!(g
        .maybe_block_tool(&repair, Some(CHECK), harness, false)
        .is_none());
    let (result, error) =
        crate::file_tools::tool_patch_file(TARGET, ",}", "}", root.path().to_str());
    assert!(!error, "{result}");
    g.focus.record_result(
        &repair,
        &result,
        root.path().to_str(),
        Some(CHECK),
        &context(),
    );
    g.on_successful_edit(AutoTestOutcome::NotRun, None);
    assert_eq!(g.stage, Some(RecoveryStage::Verify));
    assert!(g.focus.repaired());
}

#[test]
fn cached_read_observes_but_blocked_and_failed_reads_do_not_claim_diagnosis() {
    let root = tempfile::tempdir().unwrap();
    for suffix in [
        "",
        " [⚡ cached — unchanged since last read]",
        " [pruned 4L]",
    ] {
        let (mut g, _) = focused(root.path());
        let read = call("read", "read_file", json!({"path":TARGET}));
        g.focus.record_result(
            &read,
            "GOVERNOR BLOCKED",
            root.path().to_str(),
            Some(CHECK),
            &context(),
        );
        assert!(g.focus.requires_read());
        let output = format!("[{TARGET}] (1 lines, 8 bytes){suffix}\n{{\"a\":1,}}");
        g.focus.record_result(
            &read,
            &output,
            root.path().to_str(),
            Some(CHECK),
            &context(),
        );
        g.on_diagnostic_result(true);
        assert_eq!(g.stage, Some(RecoveryStage::Fix), "{suffix}");
    }
    let (mut g, _) = focused(root.path());
    std::fs::remove_file(root.path().join(TARGET)).unwrap();
    let read = call("missing", "read_file", json!({"path":TARGET}));
    let (output, error) = crate::file_tools::tool_read_file(TARGET, root.path().to_str());
    assert!(error);
    g.focus.record_result(
        &read,
        &output,
        root.path().to_str(),
        Some(CHECK),
        &context(),
    );
    g.on_diagnostic_result(false);
    assert!(!g.focus.requires_read());
    assert!(g.focus.is_pending());
    assert!(g.focus.hint().unwrap().contains("read failed"));
    g.on_diagnostic_result(true);
    assert_eq!(g.stage, Some(RecoveryStage::Fix));
}

#[test]
fn failed_patch_auto_test_reads_actual_artifact_instead_of_raw_mutation_cache() {
    let root = tempfile::tempdir().unwrap();
    crate::file_tools::tool_write_file(TARGET, "{\"a\":1}", root.path().to_str());
    let edit = call(
        "patch",
        "patch_file",
        json!({"path":TARGET,"search":"}","replace":",}"}),
    );
    let (mut output, error) =
        crate::file_tools::tool_patch_file(TARGET, "}", ",}", root.path().to_str());
    assert!(!error);
    let content = std::fs::read_to_string(root.path().join(TARGET)).unwrap();
    let mut cache = std::collections::HashMap::new();
    cache.insert(
        root.path().join(TARGET).to_string_lossy().into_owned(),
        content,
    );
    output.push_str(&format!(
        "\n[auto-test] ✗ FAILED (exit 1)\nError: parse failed; path={TARGET}"
    ));
    let mut g = RecoveryGovernor::default();
    g.focus.record_result(
        &edit,
        &output,
        root.path().to_str(),
        Some(CHECK),
        &context(),
    );
    g.on_successful_edit(AutoTestOutcome::Failed, None);
    let read = call("read", "read_file", json!({"path":TARGET}));
    let (output, error) = g
        .focus
        .read_for_diagnosis(&read, root.path().to_str(), &mut cache)
        .unwrap();
    assert!(!error);
    assert!(output.starts_with(&format!("[{TARGET}] (")));
    assert_eq!(
        cache.get(&root.path().join(TARGET).to_string_lossy().into_owned()),
        Some(&output),
        "the fresh read receipt must also satisfy write_file's prior-read gate"
    );
    g.focus.record_result(
        &read,
        &output,
        root.path().to_str(),
        Some(CHECK),
        &context(),
    );
    g.on_diagnostic_result(true);
    assert_eq!(g.stage, Some(RecoveryStage::Fix));
}

#[test]
fn unrelated_checks_cannot_clear_focus_or_certify_completion() {
    let root = tempfile::tempdir().unwrap();
    let (mut g, mut messages) = focused(root.path());
    read_target(&mut g, root.path());
    let approved = "python approved_other_check.py";
    let task = format!(
        "<observer_benchmark_plan>\nrequired_checks:\n- {approved}\n</observer_benchmark_plan>"
    );
    let ctx = ExecVerificationContext::from_root(Some(CHECK), &task);
    for command in ["pwd", "cargo test", approved] {
        let tc = call(command, "exec", json!({"command":command}));
        let output = "OK (exit_code: 0)";
        g.focus
            .record_result(&tc, output, root.path().to_str(), Some(CHECK), &ctx);
        let classification = ctx.classify(command);
        g.on_exec_result(classification.kind, classification.verification, true);
        append(&mut messages, &tc, g.focus.tool_message(&tc, output.into()));
        assert!(g.focus.is_pending());
        assert_ne!(g.stage, None);
        let mut outcome = agent::outcome::TaskOutcome::default();
        outcome.accept_done(&[]);
        assert!(!outcome.completed(&messages));
    }
    let tc = call("exact", "exec", json!({"command":CHECK}));
    g.focus.record_result(
        &tc,
        &exec_output("FAILED (exit_code: 1)", root.path()),
        root.path().to_str(),
        Some(CHECK),
        &ctx,
    );
    assert!(g.focus.is_pending());
    assert!(
        !g.focus.requires_read(),
        "a fresh failure without location needs broader diagnosis"
    );
    g.focus.record_result(
        &tc,
        "GOVERNOR BLOCKED",
        root.path().to_str(),
        Some(CHECK),
        &ctx,
    );
    assert!(g.focus.is_pending());
    g.focus.record_result(
        &tc,
        &exec_output("OK (exit_code: 0)", root.path()),
        root.path().to_str(),
        Some(CHECK),
        &ctx,
    );
    let classification = ctx.classify(CHECK);
    g.on_exec_result(classification.kind, classification.verification, true);
    assert!(!g.focus.is_pending());
    assert_eq!(g.stage, None);
}

#[test]
fn repair_snapshot_survives_valid_disk_pruning_resume_and_later_reads() {
    let root = tempfile::tempdir().unwrap();
    let (mut g, mut messages) = focused(root.path());
    let (tc, output) = read_target(&mut g, root.path());
    append(&mut messages, &tc, g.focus.tool_message(&tc, output));
    let repair = call(
        "shell-repair",
        "exec",
        json!({"command":"python repair_fixture.py"}),
    );
    std::fs::write(root.path().join(TARGET), "{\"a\":1}").unwrap();
    g.focus.record_result(
        &repair,
        "OK (exit_code: 0)",
        root.path().to_str(),
        Some(CHECK),
        &context(),
    );
    g.on_exec_result(ExecKind::Action, None, true);
    append(
        &mut messages,
        &repair,
        g.focus.tool_message(&repair, "OK (exit_code: 0)".into()),
    );
    assert!(g.focus.repaired());
    let (tc, output) = read_target(&mut g, root.path());
    append(&mut messages, &tc, g.focus.tool_message(&tc, output));
    assert!(g.focus.repaired());
    for index in 0..35 {
        let tc = call(&format!("later-{index}"), "exec", json!({"command":"pwd"}));
        append(
            &mut messages,
            &tc,
            json!({"role":"tool","tool_call_id":tc.id,"content":"OK (exit_code: 0)"}),
        );
    }
    agent::message_window::prune_message_window_with_context(&mut messages, &context());
    messages.push(crate::task_origin::user_message(
        "Resume",
        crate::task_origin::MessageOrigin::Runtime,
    ));
    let mut resumed = RecoveryGovernor::default();
    resumed.restore_focus(&messages, root.path().to_str(), Some(CHECK));
    assert_eq!(resumed.stage, Some(RecoveryStage::Verify));
    assert_eq!(resumed.focus, g.focus);
    let verify = call("verify", "exec", json!({"command":CHECK}));
    resumed.focus.record_result(
        &verify,
        &exec_output("OK (exit_code: 0)", root.path()),
        root.path().to_str(),
        Some(CHECK),
        &context(),
    );
    append(
        &mut messages,
        &verify,
        resumed
            .focus
            .tool_message(&verify, "OK (exit_code: 0)".into()),
    );
    for index in 0..35 {
        let tc = call(&format!("clear-{index}"), "exec", json!({"command":"pwd"}));
        append(
            &mut messages,
            &tc,
            json!({"role":"tool","tool_call_id":tc.id,"content":"OK (exit_code: 0)"}),
        );
    }
    agent::message_window::prune_message_window_with_context(&mut messages, &context());
    assert!(
        !RecoveryFocus::from_messages(&messages, root.path().to_str(), Some(CHECK)).is_pending()
    );
}

#[test]
fn changed_configured_auto_test_cannot_clear_original_check_and_restore_preserves_newer_failure() {
    let root = tempfile::tempdir().unwrap();
    let (mut g, mut messages) = focused(root.path());
    let (read, output) = read_target(&mut g, root.path());
    append(&mut messages, &read, g.focus.tool_message(&read, output));
    let edit = call("new-check-edit", "patch_file", json!({"path":TARGET}));
    let output = "OK: patched 'fixture.json'\n[auto-test] ✓ PASSED (exit 0)";
    g.focus.record_result(
        &edit,
        output,
        root.path().to_str(),
        Some("cargo check"),
        &context(),
    );
    g.on_successful_edit(AutoTestOutcome::Passed, Some(VerificationLevel::Build));
    assert!(g.focus.is_pending());
    append(
        &mut messages,
        &edit,
        g.focus.tool_message(&edit, output.into()),
    );
    let mut after_edit = RecoveryGovernor::default();
    after_edit.restore_focus(&messages, root.path().to_str(), Some("cargo check"));
    assert_eq!(after_edit.stage, Some(RecoveryStage::Verify));
    let different_failure = format!("OK: patched '{TARGET}'\n[auto-test] ✗ FAILED (exit 1)\nError: changed check failed; path={TARGET}");
    g.focus.record_result(
        &edit,
        &different_failure,
        root.path().to_str(),
        Some("cargo check"),
        &context(),
    );
    assert!(g.focus.requires_read());
    assert!(g.focus.check_matches(CHECK));
    assert!(!g.focus.check_matches("cargo check"));
    g.focus.record_result(
        &edit,
        output,
        root.path().to_str(),
        Some("cargo check"),
        &context(),
    );
    assert!(g.focus.is_pending());
    assert!(g.focus.check_matches(CHECK));
    let failure = call("failed-repair", "patch_file", json!({"path":TARGET}));
    append(
        &mut messages,
        &failure,
        g.focus.tool_message(&failure, "ERROR: no matches".into()),
    );
    let mut resumed = RecoveryGovernor {
        stage: Some(RecoveryStage::Diagnose),
        ..Default::default()
    };
    resumed.restore_focus(&messages, root.path().to_str(), Some(CHECK));
    assert_eq!(resumed.stage, Some(RecoveryStage::Diagnose));
    g.focus
        .record_result(&edit, output, root.path().to_str(), Some(CHECK), &context());
    assert!(!g.focus.is_pending());
}

#[test]
fn exact_check_needs_runtime_root_cwd_not_another_project_or_printed_location() {
    let root = tempfile::tempdir().unwrap();
    let nested = root.path().join("passing-subproject");
    std::fs::create_dir(&nested).unwrap();
    let (mut g, _) = focused(root.path());
    let tc = call("check", "exec", json!({"command":CHECK}));
    for output in [
        "OK (exit_code: 0)".to_string(),
        exec_output("OK (exit_code: 0)", &nested),
        format!("OK (exit_code: 0)\nstdout:\ncwd: {}", root.path().display()),
        exec_output(
            &format!("OK (exit_code: 0)\nstdout:\ncwd: {}", root.path().display()),
            &nested,
        ),
    ] {
        g.focus
            .record_result(&tc, &output, root.path().to_str(), Some(CHECK), &context());
        assert!(g.focus.is_pending(), "{output}");
    }
    g.focus.record_result(
        &tc,
        &exec_output("OK (exit_code: 0)", root.path()),
        root.path().to_str(),
        Some(CHECK),
        &context(),
    );
    assert!(!g.focus.is_pending());
}

#[test]
fn localization_rejects_unrelated_paths_unknown_outputs_and_unconfirmed_json_positions() {
    let root = tempfile::tempdir().unwrap();
    let (tc, _) = failed_json(root.path());
    for diagnostic in [
        "running 2 tests",
        "Error: other/.spiral-coder/tui_replay.json:9:1 invalid",
        "Error: failed to parse JSON at line 99 column 99",
        "Error: src/unrelated.rs:9:1 invalid",
    ] {
        let mut focus = RecoveryFocus::default();
        let result = format!("OK: wrote '{TARGET}'\n[auto-test] ✗ FAILED (exit 1)\n{diagnostic}");
        focus.record_result(&tc, &result, root.path().to_str(), Some(CHECK), &context());
        assert!(!focus.is_pending(), "{diagnostic}");
    }
    let mut focus = RecoveryFocus::default();
    focus.record_result(&tc, &format!("OK: wrote '{TARGET}'\n+[auto-test] source marker\n+Error: parse failed; path={TARGET}\n[auto-test] ✗ FAILED (exit 1)\nrunning 2 tests"), root.path().to_str(), Some(CHECK), &context());
    assert!(
        !focus.is_pending(),
        "diff-owned diagnostics are not automatic-test output"
    );
    for diagnostic in [
        format!("Error: parse failed; path={TARGET}"),
        format!(" --> {TARGET}:3:1"),
        format!("  File \"{TARGET}\", line 3, in main"),
    ] {
        let mut focus = RecoveryFocus::default();
        let result = format!("OK: wrote '{TARGET}'\n[auto-test] ✗ FAILED (exit 1)\n{diagnostic}");
        focus.record_result(&tc, &result, root.path().to_str(), Some(CHECK), &context());
        assert!(focus.requires_read(), "{diagnostic}");
    }
}

#[test]
fn snapshots_require_correlated_current_task_results_and_preserve_local_only_transport() {
    let root = tempfile::tempdir().unwrap();
    let (g, messages) = focused(root.path());
    let snapshot = messages.last().unwrap().clone();
    assert!(!RecoveryFocus::from_messages(&[snapshot], None, None).is_pending());
    let mut new_task = messages.clone();
    new_task.push(json!({"role":"user","content":"A different task"}));
    assert!(!RecoveryFocus::from_messages(&new_task, None, None).is_pending());
    let mut session = crate::agent_session::AgentSession::new(None, None, None, None, messages);
    session = serde_json::from_str(&serde_json::to_string(&session).unwrap()).unwrap();
    assert_eq!(
        RecoveryFocus::from_messages(&session.messages, None, None),
        g.focus
    );
    assert!(crate::task_origin::provider_messages(&session.messages)
        .iter()
        .all(|message| message.get(METADATA_KEY).is_none()));
}

#[cfg(unix)]
#[test]
fn pathless_json_fallback_never_follows_an_outside_root_symlink() {
    let outside = tempfile::tempdir().unwrap();
    let (tc, output) = failed_json(outside.path());
    let root = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(
        outside.path().join(".spiral-coder"),
        root.path().join(".spiral-coder"),
    )
    .unwrap();
    let mut focus = RecoveryFocus::default();
    focus.record_result(&tc, &output, root.path().to_str(), Some(CHECK), &context());
    assert!(!focus.is_pending());
}

#[test]
fn repaired_schema_error_gives_way_to_current_pathless_cause_through_resume_and_pruning() {
    let root = tempfile::tempdir().unwrap();
    let edit = call("schema-edit", "write_file", json!({"path":TARGET}));
    let content = json!({"version":1,"cases":[{"id":"review-panel-replay-sensitive","prompt":"Test review panel replay","checks":[{"kind":"replay_spec_includes"}],"coder_messages":[{"role":"user","content":"Test review panel replay"}],"observer_response":{"summary":"Review panel replay","suggestions":[]}}]}).to_string();
    let (mut output, error) =
        crate::file_tools::tool_write_file(TARGET, &content, root.path().to_str());
    assert!(!error);
    output.push_str(&format!("\n[auto-test] ✗ FAILED (exit 1)\nError: failed to parse tui replay spec: unknown variant `replay_spec_includes` at line 1 column 40; path={TARGET}"));
    let mut g = RecoveryGovernor::default();
    g.focus.record_result(
        &edit,
        &output,
        root.path().to_str(),
        Some(CHECK),
        &context(),
    );
    g.on_successful_edit(AutoTestOutcome::Failed, None);
    let mut messages = vec![json!({"role":"user","content":"Repair the replay fixture"})];
    append(&mut messages, &edit, g.focus.tool_message(&edit, output));
    read_target(&mut g, root.path());
    let repair = call("schema-repair", "patch_file", json!({"path":TARGET}));
    let (mut output, error) = crate::file_tools::tool_patch_file(
        TARGET,
        "replay_spec_includes",
        "suggestion_parsed",
        root.path().to_str(),
    );
    assert!(!error);
    assert!(serde_json::from_str::<crate::tui_replay::TuiReplaySpec>(
        &std::fs::read_to_string(root.path().join(TARGET)).unwrap()
    )
    .is_ok());
    output.push_str("\n[auto-test] ✗ FAILED (exit 1)\nError: could not infer a stuck target from coder_messages; {\"roles\":{\"assistant\":0,\"tool\":0,\"user\":1}}\nError: tui-replay target requires a completed failure-like assistant message in top-level coder_messages.");
    g.focus.record_result(
        &repair,
        &output,
        root.path().to_str(),
        Some(CHECK),
        &context(),
    );
    g.on_successful_edit(AutoTestOutcome::Failed, None);
    append(
        &mut messages,
        &repair,
        g.focus.tool_message(&repair, output),
    );
    assert!(g.focus.is_pending());
    assert!(!g.focus.requires_read());
    let hint = g.focus.hint().unwrap();
    assert!(hint.contains("could not infer a stuck target"));
    assert!(hint.contains("last_confirmed_path"));
    assert!(!hint.contains("replay_spec_includes") && !hint.contains("Read this exact path"));
    let (read, output) = read_target(&mut g, root.path());
    append(&mut messages, &read, g.focus.tool_message(&read, output));
    assert_eq!(
        g.focus.pending.as_ref().unwrap().observation,
        Observation::Unlocalized
    );
    assert_eq!(g.stage, Some(RecoveryStage::Fix));
    assert!(g.focus.hint().unwrap().contains("last_confirmed_path"));
    for index in 0..35 {
        let tc = call(
            &format!("diagnostic-{index}"),
            "exec",
            json!({"command":"pwd"}),
        );
        append(
            &mut messages,
            &tc,
            json!({"role":"tool","tool_call_id":tc.id,"content":"OK (exit_code: 0)"}),
        );
    }
    agent::message_window::prune_message_window_with_context(&mut messages, &context());
    let restored = RecoveryFocus::from_messages(&messages, root.path().to_str(), Some(CHECK));
    assert_eq!(restored, g.focus);
    let mut outcome = agent::outcome::TaskOutcome::default();
    outcome.accept_done(&[]);
    assert!(!outcome.completed(&messages));
    let action = call(
        "repair-attempt",
        "exec",
        json!({"command":"python repair_fixture.py"}),
    );
    g.focus.record_result(
        &action,
        "OK (exit_code: 0)",
        root.path().to_str(),
        Some(CHECK),
        &context(),
    );
    assert!(g.focus.repaired());
    assert!(g.focus.hint().unwrap().contains("last_confirmed_path"));
    append(
        &mut messages,
        &action,
        g.focus.tool_message(&action, "OK (exit_code: 0)".into()),
    );
    assert_eq!(RecoveryFocus::from_messages(&messages, None, None), g.focus);
}

#[test]
fn original_exec_failures_refresh_cause_and_localization_without_other_checks_replacing_them() {
    let root = tempfile::tempdir().unwrap();
    let (mut g, _) = focused(root.path());
    let check = call("check", "exec", json!({"command":CHECK}));
    let output = exec_output(
        "FAILED (exit_code: 1)\nError: could not infer a stuck target from coder_messages",
        root.path(),
    );
    g.focus.record_result(
        &check,
        &output,
        root.path().to_str(),
        Some(CHECK),
        &context(),
    );
    assert!(!g.focus.requires_read());
    let pending = g.focus.pending.as_ref().unwrap();
    assert_eq!(pending.location, LocationEvidence::LastConfirmed);
    assert_eq!(pending.observation, Observation::Unlocalized);
    assert!(pending.diagnostic.contains("stuck target"));
    let unchanged = g.focus.clone();
    let other = call("other-check", "exec", json!({"command":"cargo check"}));
    g.focus.record_result(
        &other,
        &exec_output("FAILED (exit_code: 1)\nError: another cause", root.path()),
        root.path().to_str(),
        Some(CHECK),
        &context(),
    );
    assert_eq!(g.focus, unchanged);
    let edit = call(
        "different-config-edit",
        "patch_file",
        json!({"path":TARGET}),
    );
    g.focus.record_result(&edit, &format!("OK: patched '{TARGET}'\n[auto-test] ✗ FAILED (exit 1)\nError: another check; path={TARGET}"), root.path().to_str(), Some("cargo check"), &context());
    assert_eq!(g.focus, unchanged);
    let current = exec_output(
        &format!("FAILED (exit_code: 1)\nError: invalid replay data; path={TARGET}"),
        root.path(),
    );
    g.focus.record_result(
        &check,
        &current,
        root.path().to_str(),
        Some(CHECK),
        &context(),
    );
    assert!(g.focus.requires_read());
    assert_eq!(
        g.focus.pending.as_ref().unwrap().location,
        LocationEvidence::Current
    );
    assert!(g.focus.hint().unwrap().contains("invalid replay data"));
    assert!(!g.focus.hint().unwrap().contains("last_confirmed_path"));
}

#[test]
fn legacy_stale_snapshot_refreshes_only_with_matching_check_and_correlated_failure() {
    let root = tempfile::tempdir().unwrap();
    let (g, mut messages) = focused(root.path());
    let repair = call("legacy-repair", "patch_file", json!({"path":TARGET}));
    let (mut output, error) =
        crate::file_tools::tool_patch_file(TARGET, ",}", "}", root.path().to_str());
    assert!(!error);
    output.push_str("\n[auto-test] ✗ FAILED (exit 1)\nError: could not infer a stuck target from coder_messages");
    let mut legacy = g.focus.tool_message(&repair, output);
    legacy[METADATA_KEY]["state"]["pending"]
        .as_object_mut()
        .unwrap()
        .remove("location");
    append(&mut messages, &repair, legacy);
    let read = call("legacy-read", "read_file", json!({"path":TARGET}));
    let (output, error) = crate::file_tools::tool_read_file(TARGET, root.path().to_str());
    assert!(!error);
    let mut old_read = g.focus.tool_message(&read, output);
    old_read[METADATA_KEY]["state"]["pending"]
        .as_object_mut()
        .unwrap()
        .remove("location");
    append(&mut messages, &read, old_read);
    let action = call(
        "legacy-action",
        "exec",
        json!({"command":"python repair_fixture.py"}),
    );
    let mut old_action = g.focus.tool_message(&action, "OK (exit_code: 0)".into());
    old_action[METADATA_KEY]["state"]["pending"]
        .as_object_mut()
        .unwrap()
        .remove("location");
    append(&mut messages, &action, old_action);
    let migrated = RecoveryFocus::from_messages(&messages, root.path().to_str(), Some(CHECK));
    assert!(migrated.is_pending());
    assert!(!migrated.requires_read());
    let hint = migrated.hint().unwrap();
    assert!(
        hint.contains("could not infer a stuck target") && hint.contains("last_confirmed_path")
    );
    assert!(!hint.contains("expected key"));
    for check in [None, Some("another check")] {
        assert_eq!(
            RecoveryFocus::from_messages(&messages, root.path().to_str(), check),
            g.focus
        );
    }
    let mut uncorrelated = messages;
    uncorrelated.push(json!({"role":"tool","tool_call_id":"unknown","content":"OK: patched 'file'\n[auto-test] ✗ FAILED (exit 1)\nError: invented cause"}));
    assert_eq!(
        RecoveryFocus::from_messages(&uncorrelated, root.path().to_str(), Some(CHECK)),
        migrated
    );
}
