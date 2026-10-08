use super::super::{
    validate_reflection, FailureMemory, GoalDelta, RecoveryGovernor, RecoveryStage,
    ReflectionBlock, StrategyChange, TaskHarness,
};
use super::*;
use serde_json::json;

fn context() -> ExecVerificationContext<'static> {
    ExecVerificationContext::from_root(None, "")
}

fn restore(messages: &[Value]) -> EditFailureMemory {
    EditFailureMemory::from_messages(
        messages,
        &ExecVerificationContext::from_messages(None, messages),
    )
}

fn call(name: &str, args: Value) -> ToolCallData {
    ToolCallData {
        thought_signature: None,
        id: "edit".into(),
        name: name.into(),
        arguments: args.to_string(),
    }
}

fn record(messages: &mut Vec<Value>, call: &ToolCallData, output: &str) {
    let id = format!("call_{}", messages.len());
    messages.push(json!({"role":"assistant","tool_calls":[{
        "id":id,"type":"function","function":{"name":call.name,"arguments":call.arguments}
    }]}));
    messages.push(json!({"role":"tool","tool_call_id":id,"content":output}));
}

#[test]
fn failed_json_edit_read_retry_preserves_recovery_gate_and_resume() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().to_str().unwrap();
    let original = "{\n  \"id\": \"case\",\n  \"based_on\": [\"recent_tool_results\"]\n}\n";
    std::fs::write(dir.path().join("replay.json"), original).unwrap();
    // Attempt 08's failure shape: a large guessed search omitted an existing field.
    let search = "{\n  \"id\": \"case\"\n}";
    let edit = call(
        "patch_file",
        json!({"path":"replay.json","search":search,"replace":"{}"}),
    );
    let read = call("read_file", json!({"path":"replay.json"}));
    let mut memory = EditFailureMemory::default();
    let mut recovery = RecoveryGovernor::default();
    let mut messages = vec![json!({"role":"user","content":"Repair the replay case"})];
    for expected in 1..=3 {
        let (output, failed) =
            crate::file_tools::tool_patch_file("replay.json", search, "{}", Some(base));
        assert!(failed);
        memory.on_result(&edit, &output, &context());
        record(&mut messages, &edit, &output);
        recovery.on_fix_result(false, None);
        assert_eq!(recovery.stage, Some(RecoveryStage::Diagnose));

        let (output, failed) = crate::file_tools::tool_read_file("replay.json", Some(base));
        assert!(!failed);
        memory.on_result(&read, &output, &context());
        record(&mut messages, &read, &output);
        recovery.on_diagnostic_result(true);
        assert_eq!(recovery.stage, Some(RecoveryStage::Fix));
        assert_eq!(memory.count(), expected);
        let restored: Vec<Value> =
            serde_json::from_str(&serde_json::to_string(&messages).unwrap()).unwrap();
        assert_eq!(restore(&restored), memory);

        if expected >= 2 {
            let hint = memory
                .strategy_hint()
                .expect("specific recovery guidance survives read");
            assert!(hint.contains("smallest unique anchor"));
            assert!(hint.contains("preserve surrounding fields"));
            assert!(hint.contains(&format!(
                "identical patch_file arguments failed {expected} times"
            )));
            let mut reflection = ReflectionBlock {
                last_outcome: "failure".into(),
                goal_delta: GoalDelta::Closer,
                wrong_assumption: "The guessed JSON block matched the file".into(),
                strategy_change: StrategyChange::Keep,
                next_minimal_action: "repeat the same patch".into(),
            };
            assert!(
                validate_reflection(&reflection, &FailureMemory::default(), memory.count())
                    .is_err()
            );
            reflection.strategy_change = StrategyChange::Adjust;
            reflection.next_minimal_action = "patch the exact id field, preserving metadata".into();
            assert!(
                validate_reflection(&reflection, &FailureMemory::default(), memory.count()).is_ok()
            );
            assert!(recovery
                .maybe_block_tool(
                    &edit,
                    None,
                    TaskHarness::infer("Fix existing replay", false),
                    false
                )
                .is_none());
        }
    }
    assert_eq!(
        std::fs::read_to_string(dir.path().join("replay.json")).unwrap(),
        original
    );
}

#[test]
fn an_actual_edit_resets_failures_independently_of_automatic_test_failure() {
    for name in ["write_file", "patch_file", "apply_diff"] {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().to_str().unwrap();
        std::fs::write(dir.path().join("file.txt"), "before\n").unwrap();
        let failed = call(
            "patch_file",
            json!({"path":"file.txt","search":"absent","replace":"after"}),
        );
        let mut memory = EditFailureMemory::default();
        let mut messages = vec![json!({"role":"user","content":"Fix file"})];
        for _ in 0..2 {
            let (output, _) =
                crate::file_tools::tool_patch_file("file.txt", "absent", "after", Some(base));
            memory.on_result(&failed, &output, &context());
            record(&mut messages, &failed, &output);
        }
        let (mut output, failed) = match name {
            "write_file" => crate::file_tools::tool_write_file("file.txt", "after\n", Some(base)),
            "patch_file" => {
                crate::file_tools::tool_patch_file("file.txt", "before", "after", Some(base))
            }
            _ => crate::file_tools::tool_apply_diff(
                "file.txt",
                "@@ -1 +1 @@\n-before\n+after",
                Some(base),
            ),
        };
        assert!(!failed, "{output}");
        output.push_str("\n[auto-test] ✗ FAILED (exit 1)");
        let edit = call(name, json!({"path":"file.txt"}));
        memory.on_result(&edit, &output, &context());
        record(&mut messages, &edit, &output);
        assert_eq!(memory, EditFailureMemory::default());
        assert_eq!(restore(&messages), memory);
        assert_eq!(
            std::fs::read_to_string(dir.path().join("file.txt")).unwrap(),
            "after\n"
        );
    }
}

#[test]
fn diagnostic_errors_rejections_and_uncorrelated_outputs_do_not_change_edit_memory() {
    let edit = call("patch_file", json!({"path":"file.txt"}));
    let mut memory = EditFailureMemory::default();
    memory.on_result(&edit, "ERROR: search string not found", &context());
    let mut messages = vec![json!({"role":"user","content":"Fix file"})];
    record(&mut messages, &edit, "ERROR: search string not found");
    for (name, output) in [
        ("read_file", "ERROR: file absent"),
        ("read_file", "[file.txt] (1 lines)\nOK: patched 'file.txt'"),
        ("patch_file", "REJECTED BY USER\nERROR: denied"),
        (
            "write_file",
            "GOVERNOR BLOCKED: existing file requires read",
        ),
        ("patch_file", "unknown result\nERROR: not a header"),
    ] {
        let attempt = call(name, json!({"path":"file.txt"}));
        memory.on_result(&attempt, output, &context());
        record(&mut messages, &attempt, output);
    }
    messages.push(json!({"role":"tool","tool_call_id":"not-called","content":"ERROR: failed"}));
    assert_eq!(memory.count(), 1);
    assert_eq!(restore(&messages), memory);
    assert!(memory.strategy_hint().is_none());
}

#[test]
fn compacting_history_preserves_the_success_that_resolved_old_edit_failures() {
    let edit = call("patch_file", json!({"path":"file.txt"}));
    let mut messages = vec![json!({"role":"user","content":"Fix file"})];
    record(&mut messages, &edit, "ERROR: search string not found");
    record(&mut messages, &edit, "ERROR: search string not found");
    record(
        &mut messages,
        &edit,
        "OK: patched 'file.txt' (0 lines, 1 total)",
    );
    for _ in 0..35 {
        record(
            &mut messages,
            &call("exec", json!({"command":"cargo test"})),
            "OK (exit_code: 0)",
        );
    }
    super::super::message_window::prune_message_window(&mut messages);
    assert!(messages.len() < 77);
    assert_eq!(restore(&messages), EditFailureMemory::default());
    record(&mut messages, &edit, "ERROR: second unrelated issue");
    assert_eq!(restore(&messages).count(), 1);
}

#[test]
fn runtime_continuation_keeps_edit_failures_while_new_human_scope_starts_fresh() {
    use crate::task_origin::{user_message, MessageOrigin};
    let mut messages = vec![user_message("Repair replay", MessageOrigin::User)];
    let edit = call("patch_file", json!({"path":"replay.json"}));
    for _ in 0..2 {
        record(&mut messages, &edit, "ERROR: search string not found");
        messages.push(user_message(
            "Continue the unfinished task",
            MessageOrigin::Runtime,
        ));
    }
    let session = crate::agent_session::AgentSession::new(None, None, None, None, messages);
    let mut restored: crate::agent_session::AgentSession =
        serde_json::from_str(&serde_json::to_string(&session).unwrap()).unwrap();
    assert_eq!(restore(&restored.messages).count(), 2);
    restored
        .messages
        .push(user_message("Inspect another module", MessageOrigin::User));
    assert_eq!(restore(&restored.messages).count(), 0);
}

#[test]
fn shell_mutation_reset_uses_the_same_context_in_live_replay_and_pruning() {
    let approved = "verify-contract && printf checked > receipt";
    let configured = "custom-project-check";
    let prompt = format!("<observer_benchmark_plan>\nrequired_checks:\n- {approved}\n</observer_benchmark_plan>\nRepair the file");
    let context = ExecVerificationContext::from_root(Some(configured), &prompt);
    let mut messages = vec![json!({"role":"user","content":prompt})];
    let mut memory = EditFailureMemory::default();
    let edit = call("patch_file", json!({"path":"file.txt"}));
    for _ in 0..2 {
        memory.on_result(&edit, "ERROR: search string not found", &context);
        record(&mut messages, &edit, "ERROR: search string not found");
    }
    for (command, output) in [
        ("pwd", "OK (exit_code: 0)"),
        ("cargo test", "OK (exit_code: 0)"),
        (approved, "OK (exit_code: 0)"),
        (configured, "OK (exit_code: 0)"),
        ("printf changed > file.txt", "FAILED (exit_code: 1)"),
        ("printf changed > file.txt", "GOVERNOR BLOCKED"),
        ("printf changed > file.txt", "REJECTED BY USER"),
        (
            "printf changed > file.txt",
            "NOTE: cwd escaped tool_root\nFAILED (exit_code: 1)",
        ),
    ] {
        let action = call("exec", json!({"command":command}));
        memory.on_result(&action, output, &context);
        record(&mut messages, &action, output);
        assert_eq!(memory.count(), 2, "{command}: {output}");
        assert_eq!(
            EditFailureMemory::from_messages(&messages, &context),
            memory
        );
    }
    let action = call("exec", json!({"command":"printf changed > file.txt"}));
    memory.on_result(&action, "OK (exit_code: 0)", &context);
    record(&mut messages, &action, "OK (exit_code: 0)");
    assert_eq!(memory.count(), 0);
    for _ in 0..35 {
        record(
            &mut messages,
            &call("exec", json!({"command":configured})),
            "OK (exit_code: 0)",
        );
    }
    super::super::message_window::prune_message_window_with_context(&mut messages, &context);
    assert_eq!(
        EditFailureMemory::from_messages(&messages, &context),
        memory
    );
    assert!(memory.strategy_hint().is_none());
}

#[test]
fn resumed_failed_edit_after_read_keeps_benchmark_repair_ahead_of_verification() {
    use super::super::{coerce_benchmark_plan_tool_call, VerificationLevel};
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().to_str().unwrap();
    let path = ".spiral-coder/tui_replay.json";
    let command = "spiral-coder --provider openai tui-replay --spec .spiral-coder/tui_replay.json";
    let prompt = format!("<observer_benchmark_plan>\nlane: tui_replay\ntarget_files:\n- {path}\nrequired_checks:\n- {command}\nsuccess_criteria:\n- {path} includes src/tui/review_panel.rs\n</observer_benchmark_plan>");
    let context = ExecVerificationContext::from_root(None, &prompt);
    let harness = TaskHarness::infer(&prompt, false);
    let invalid = "{\"target\":\"src/tui/review_panel.rs\",}";
    std::fs::create_dir(dir.path().join(".spiral-coder")).unwrap();
    std::fs::write(dir.path().join(path), invalid).unwrap();
    let mut messages = vec![json!({"role":"user","content":prompt})];
    let failed = call(
        "patch_file",
        json!({"path":path,"search":"absent","replace":"replacement"}),
    );
    let (output, error) =
        crate::file_tools::tool_patch_file(path, "absent", "replacement", Some(base));
    assert!(error);
    record(&mut messages, &failed, &output);
    let read = call("read_file", json!({"path":path}));
    let (output, error) = crate::file_tools::tool_read_file(path, Some(base));
    assert!(!error);
    record(&mut messages, &read, &output);
    let repair = call(
        "patch_file",
        json!({"path":path,"search":",}","replace":"}"}),
    );
    let edits = EditFailureMemory::from_messages(&messages, &context);
    let mut recovery = RecoveryGovernor::restore_from_session(
        &FailureMemory::default(),
        &messages,
        VerificationLevel::Behavioral,
    );
    let (wrong, _, _) = coerce_benchmark_plan_tool_call(
        harness,
        &messages,
        &repair,
        &prompt,
        Some(base),
        None,
        None,
    )
    .expect("matching text alone would prematurely request verification");
    assert_eq!(wrong.name, "exec");
    recovery.restore_pending_edits(&edits);
    assert_eq!(recovery.stage, Some(RecoveryStage::Diagnose));
    assert!(coerce_benchmark_plan_tool_call(
        harness,
        &messages,
        &read,
        &prompt,
        Some(base),
        recovery.stage,
        None
    )
    .is_none());
    recovery.on_diagnostic_result(true);
    assert_eq!(recovery.stage, Some(RecoveryStage::Fix));
    assert!(coerce_benchmark_plan_tool_call(
        harness,
        &messages,
        &repair,
        &prompt,
        Some(base),
        recovery.stage,
        None
    )
    .is_none());
    let (output, error) = crate::file_tools::tool_patch_file(path, ",}", "}", Some(base));
    assert!(!error, "{output}");
    let repaired = std::fs::read_to_string(dir.path().join(path)).unwrap();
    assert!(serde_json::from_str::<Value>(&repaired).is_ok());
}
