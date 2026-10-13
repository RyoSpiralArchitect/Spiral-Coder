use super::*;
use serde_json::Value;

fn record(messages: &mut Vec<Value>, name: &str, args: Value, result: &str) {
    let id = format!("call-{}", messages.len());
    messages.push(json!({"role":"assistant","tool_calls":[{"id":id,"function":{"name":name,"arguments":args.to_string()}}]}));
    messages.push(json!({"role":"tool","tool_call_id":id,"content":result}));
}

fn read(path: &str) -> ToolCallData {
    ToolCallData {
        thought_signature: None,
        id: "next".into(),
        name: "read_file".into(),
        arguments: json!({"path":path}).to_string(),
    }
}

#[test]
fn diagnosis_takes_precedence_over_trace_derived_mutation_pressure() {
    let harness = TaskHarness::infer("Fix src/lib.rs and run cargo test", false);
    let mut messages = Vec::new();
    for _ in 0..3 {
        record(
            &mut messages,
            "read_file",
            json!({"path":"src/lib.rs"}),
            "[src/lib.rs] (1 lines, 5 bytes)\nbroken",
        );
    }
    record(
        &mut messages,
        "exec",
        json!({"command":"cargo test"}),
        "FAILED (exit_code: 1)\nstderr: compile error",
    );
    let mut recovery = RecoveryGovernor::default();
    recovery.on_exec_result(ExecKind::Verify, Some(VerificationLevel::Behavioral), false);
    assert!(!recovery.allows_progress_pressure());
    let meta = MetaHarness::analyze(harness, &messages, recovery.stage, Some("cargo test"), None);
    let evaluator = EvaluatorLoop::analyze(
        harness,
        &meta,
        &crate::reflection_ledger::ReflectionLedger::default(),
        None,
        Some("cargo test"),
        Some(1),
        0,
    );
    assert!(
        meta.prompt().is_none(),
        "diagnosis must not also demand mutation"
    );
    assert!(evaluator.prompt().is_none());
    assert!(evaluator
        .build_violation_block(&read("src/lib.rs"))
        .is_none());
    assert!(recovery
        .maybe_block_tool(&read("src/lib.rs"), Some("cargo test"), harness, false)
        .is_none());
    // A real successful diagnostic advances the phase; observation pressure
    // still applies to unchanged repetitions after diagnosis completes.
    recovery.on_diagnostic_result(true);
    let meta = MetaHarness::analyze(harness, &messages, recovery.stage, Some("cargo test"), None);
    let evaluator = EvaluatorLoop::analyze(
        harness,
        &meta,
        &crate::reflection_ledger::ReflectionLedger::default(),
        None,
        Some("cargo test"),
        Some(1),
        0,
    );
    assert!(evaluator
        .build_violation_block(&read("src/lib.rs"))
        .is_some());
    assert_eq!(recovery.stage, Some(RecoveryStage::Fix));
    assert!(recovery.allows_progress_pressure());
}

#[test]
fn progress_does_not_reuse_pre_mutation_reads_to_block_followup_diagnosis() {
    let harness = TaskHarness::infer("Fix src/lib.rs and update docs/state-schema.md", false);
    let mut messages = Vec::new();
    for path in ["src/lib.rs", "docs/state-schema.md", "docs/state-schema.md"] {
        record(
            &mut messages,
            "read_file",
            json!({"path":path}),
            &format!("[{path}] (1 lines, 3 bytes)\nold"),
        );
    }
    let blocked = |messages: &[Value]| {
        build_progress_gate_block(
            harness,
            &read("docs/state-schema.md"),
            messages,
            Some(RecoveryStage::Fix),
            Some("cargo test"),
            Some(4),
            0,
        )
    };
    assert!(
        blocked(&messages).is_some(),
        "unchanged repetitions remain limited"
    );
    record(
        &mut messages,
        "patch_file",
        json!({"path":"src/lib.rs"}),
        "GOVERNOR BLOCKED\nOK: patched 'src/lib.rs'",
    );
    assert!(
        blocked(&messages).is_some(),
        "rejected attempts do not reset history"
    );
    record(
        &mut messages,
        "patch_file",
        json!({"path":"src/lib.rs"}),
        "OK: patched 'src/lib.rs'\n[auto-test] ✗ FAILED (exit 1)\nmissing documentation follow-up",
    );
    assert!(
        blocked(&messages).is_none(),
        "allow the first read after the source changed"
    );
    for _ in 0..2 {
        record(
            &mut messages,
            "read_file",
            json!({"path":"docs/state-schema.md"}),
            "[docs/state-schema.md] (1 lines, 3 bytes)\nold",
        );
    }
    assert!(
        blocked(&messages).is_some(),
        "new observation epoch still has a bound"
    );
}

#[test]
fn rejected_exec_cannot_reset_observation_pressure_with_embedded_success_text() {
    let mut messages = Vec::new();
    for _ in 0..2 {
        record(
            &mut messages,
            "read_file",
            json!({"path":"src/lib.rs"}),
            "[src/lib.rs] (1 lines, 3 bytes)\nold",
        );
    }
    for result in [
        "GOVERNOR BLOCKED\nOK (exit_code: 0)",
        "REJECTED BY USER\nOK (exit_code: 0)",
        "FAILED (exit_code: 0)",
    ] {
        record(
            &mut messages,
            "exec",
            json!({"command":"touch generated.txt"}),
            result,
        );
        let window =
            meta_harness::observation_window_since_last_mutation(&messages, Some("cargo test"));
        assert_eq!(window.total_successes_since_mutation, 2, "{result}");
    }
    record(
        &mut messages,
        "exec",
        json!({"command":"touch generated.txt"}),
        "OK (exit_code: 0)",
    );
    assert_eq!(
        meta_harness::observation_window_since_last_mutation(&messages, Some("cargo test"))
            .total_successes_since_mutation,
        0
    );
}
