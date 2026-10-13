//! Regressions drawn from several providers' interrupted repair trajectories.
use super::*;

#[test]
fn suggested_verification_command_keeps_the_exact_configured_identity() {
    for command in [
        "test -f notes/todo.txt && grep -Fx \"ship it\" notes/todo.txt",
        "test -f 'Mixed Case/Result.txt' && grep -Fx 'Ship  It' 'Mixed Case/Result.txt'",
    ] {
        let hint = verification_requirement_hint(VerificationLevel::Behavioral, Some(command));
        let suggested = hint.split('`').nth(1).expect("preferred command");
        let context = exec_verification::ExecVerificationContext::from_root(
            Some(command),
            "verify the created file",
        );
        let classification = context.classify(suggested);
        assert_eq!(
            classification.kind,
            ExecKind::Verify,
            "the gate must accept its own suggestion: {hint}"
        );
        assert_eq!(
            classification.verification,
            Some(VerificationLevel::Behavioral)
        );
        assert_eq!(
            suggested, command,
            "case and quoted whitespace carry shell semantics"
        );
    }
}

#[test]
fn preliminary_checks_preserve_failed_edits_and_do_not_satisfy_required_checks_on_resume() {
    let required = "spiral-coder tui-replay --spec replay.json --filter new-case";
    let preliminary = "spiral-coder tui-replay --spec replay.json --filter existing-case";
    let root = format!(
        "<observer_benchmark_plan>\nrequired_checks:\n- {required}\n</observer_benchmark_plan>"
    );
    let context = exec_verification::ExecVerificationContext::from_root(None, &root);
    let mut messages = vec![json!({"role":"user","content":root})];
    let mut failures = EditFailureMemory::default();
    for (id, name, args, output) in [
        (
            "patch",
            "patch_file",
            json!({"path":"replay.json","search":"old","replace":"new"}),
            "ERROR: search string not found",
        ),
        (
            "check",
            "exec",
            json!({"command":preliminary}),
            "OK (exit_code: 0)",
        ),
    ] {
        let call = ToolCallData {
            id: id.into(),
            name: name.into(),
            arguments: args.to_string(),
            thought_signature: None,
        };
        failures.on_result(&call, output, &context);
        messages.push(json!({"role":"assistant","tool_calls":[call.to_json()]}));
        messages.push(json!({"role":"tool","tool_call_id":id,"content":output}));
    }
    assert_eq!(failures.count(), 1);
    assert_eq!(
        EditFailureMemory::from_messages(&messages, &context),
        failures
    );
    assert_eq!(
        benchmark_plan_pending_required_exec_command(&root, &messages, None),
        Some(required.into())
    );
    let (_, mutation, _, behavioral, _) = restore_done_gate_from_messages(&messages, None);
    assert_eq!((mutation, behavioral), (None, Some(2)));
    assert!(edit_failure::protected_reset_call_id(&messages, &context).is_none());
}

#[test]
fn incomplete_protocol_reports_all_independent_repairs_in_one_turn() {
    let plan = parse_plan_block(
        "<plan>\ngoal: repair\nsteps:\n1. inspect\n2. repair and test\nacceptance: tests pass\nrisks: regression\nassumptions: source exists\n</plan>",
    ).unwrap();
    let think =
        parse_think_block("<think>\ngoal: find files\nstep: 1\ntool: glob\n</think>").unwrap();
    let call = ToolCallData {
        id: "inspect".into(),
        name: "glob".into(),
        arguments: json!({"pattern":"src/**"}).to_string(),
        thought_signature: None,
    };
    let error = validate_think(&think, &plan, &call)
        .unwrap_err()
        .to_string();
    for field in ["risk", "doubt", "next", "verify"] {
        assert!(error.contains(&format!("think missing {field}")), "{error}");
    }
    let incomplete = parse_plan_block("<plan>\ngoal: repair\n</plan>").unwrap();
    let error = validate_plan(&incomplete).unwrap_err().to_string();
    for field in ["steps", "acceptance", "risks", "assumptions"] {
        assert!(error.contains(field), "{error}");
    }
}

#[test]
fn preliminary_replay_does_not_create_a_mutation_or_block_the_intended_edit() {
    let command = "spiral-coder --provider openai tui-replay --spec .spiral-coder/tui_replay.json --filter existing-case";
    let context = exec_verification::ExecVerificationContext::from_root(None, "Add a replay case");
    let classification = context.classify(command);
    assert_eq!(classification.kind, ExecKind::Verify);
    assert_eq!(
        classification.verification,
        Some(VerificationLevel::Behavioral)
    );
    let mut recovery = RecoveryGovernor {
        stage: Some(RecoveryStage::Fix),
        required_verification: VerificationLevel::Behavioral,
        ..Default::default()
    };
    recovery.on_exec_result(classification.kind, classification.verification, true);
    let edit = ToolCallData {
        id: "edit".into(),
        name: "patch_file".into(),
        arguments: "{}".into(),
        thought_signature: None,
    };
    assert!(recovery
        .maybe_block_tool(
            &edit,
            None,
            TaskHarness::infer("Add a replay case", false),
            false
        )
        .is_none());
    let (mut mutation, mut build, mut behavioral) = (None, None, None);
    exec_proof::record_result(
        command,
        &context,
        exec_proof::ExecProofResult {
            content: "OK (exit_code: 0)",
            succeeded: true,
        },
        1,
        &mut mutation,
        &mut build,
        &mut behavioral,
    );
    assert_eq!((mutation, build, behavioral), (None, None, Some(1)));
}

#[test]
fn schema_repair_keeps_the_failing_check_shape_through_auto_test_compaction() {
    let spec = json!({"cases":[{
        "id":"broken", "prompt":"inspect", "observer_response":{},
        "checks":[{"kind":"target_message_contains"}]
    }]});
    let source = spec.to_string();
    let error = serde_json::from_str::<crate::tui_replay::TuiReplaySpec>(&source).unwrap_err();
    let diagnostic = crate::tui_replay::diagnostics::parse_error_for_test(&source, error);
    let output = failure_diagnostics::automatic_test_output(
        "",
        &format!("Error: {diagnostic}"),
        1,
        "/workspace",
    );
    // Exercise the real old-result pruning path, not just the CLI rendering.
    let compacted = compact_success_tool_result_for_history(
        "patch_file",
        &format!("OK: patched 'replay.json'\n{output}"),
    );
    let mut messages = vec![json!({"role":"tool","content":compacted})];
    for _ in 0..KEEP_RECENT_TOOL_TURNS + 1 {
        messages.push(json!({"role":"tool","content":"read complete"}));
    }
    prune_old_tool_results(&mut messages);
    let retained = messages[0]["content"].as_str().unwrap();
    for required in [
        "cases[0].checks[0]",
        "missing field `value`",
        "\"kind\":\"target_message_contains\"",
        "\"value\":\"coder-0\"",
    ] {
        assert!(
            retained.contains(required),
            "missing {required}: {retained}"
        );
    }
    assert_eq!(
        crate::execution_evidence::auto_test_outcome("patch_file", retained),
        crate::execution_evidence::AutoTestOutcome::Failed
    );
}
