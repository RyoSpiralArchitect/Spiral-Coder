use super::*;

#[test]
fn resumed_action_then_verification_restores_a_validated_impact_plan() {
    let prompt = "[Observer benchmark plan approved]\n<observer_benchmark_plan required=\"true\">\nlane: runtime_eval\nobjective: Finish prepared regression artifacts for src/tui/agent/benchmark_proof.rs.\ntarget_files:\n- .spiral-coder/runtime_eval.json\n- docs/runtime-architecture.md\nrequired_checks:\n- grep -q proof docs/runtime-architecture.md\nsuccess_criteria:\n- Current checks pass after the latest edit.\n</observer_benchmark_plan>\nResume the work and complete the required verification.";
    let messages = vec![
        json!({"role":"user","content":prompt}),
        json!({"role":"assistant","tool_calls":[{"id":"edit","function":{"name":"exec","arguments":"{\"command\":\"printf proof > docs/runtime-architecture.md\"}"}}]}),
        json!({"role":"tool","tool_call_id":"edit","content":"OK (exit_code: 0)"}),
        json!({"role":"assistant","tool_calls":[{"id":"verify","function":{"name":"exec","arguments":"{\"command\":\"cargo check\"}"}}]}),
        json!({"role":"tool","tool_call_id":"verify","content":"OK (exit_code: 0)"}),
    ];
    let (_, mutation, verified, _, _) = restore_done_gate_from_messages(&messages, None);
    assert!(
        verified > mutation,
        "a later check does not replace the required impact review"
    );
    let pending =
        UnreviewedMutation::from_steps(mutation, last_impact_step_from_messages(&messages));
    assert!(pending.is_some());
    let harness = TaskHarness::infer(prompt, false);
    let tc = ToolCallData {
        id: "pending-check".into(),
        name: "exec".into(),
        arguments: json!({"command":"grep -q proof docs/runtime-architecture.md"}).to_string(),
    };
    let plan = benchmark_resume_plan(
        pending,
        harness,
        &tc,
        prompt,
        VerificationLevel::Build,
        None,
    )
    .expect("restore missing plan from approved task contract");
    let contract = derive_task_contract(prompt, false, true, VerificationLevel::Build);
    let resolver = InstructionResolver::new(&contract.task_summary, false, true);
    validate_plan_for_task_contract(&plan, false, &contract, &resolver)
        .expect("task and instruction contract still apply");
    for reason in [
        "recent mutation has not been evaluated for goal impact",
        "localized or revised prompt wording",
    ] {
        let impact = rescue_missing_impact_for_tool_turn(
            reason,
            &tc,
            false,
            true,
            ProviderKind::Mistral,
            Some(&plan),
            pending,
        )
        .expect("typed pending mutation authorizes compatibility rescue");
        validate_impact(&impact, Some(&plan)).expect("same impact validation remains mandatory");
        assert!(impact.changed.contains("recorded workspace mutation"));
    }
    for absent in [
        UnreviewedMutation::from_steps(None, None),
        UnreviewedMutation::from_steps(mutation, mutation),
    ] {
        assert!(rescue_missing_impact_for_tool_turn(
            "successful mutation",
            &tc,
            false,
            true,
            ProviderKind::Mistral,
            Some(&plan),
            absent
        )
        .is_none());
        assert!(benchmark_resume_plan(
            absent,
            harness,
            &tc,
            prompt,
            VerificationLevel::Build,
            None
        )
        .is_none());
    }
    assert!(
        rescue_missing_impact_for_tool_turn(
            "recent mutation",
            &tc,
            false,
            true,
            ProviderKind::Mistral,
            None,
            pending
        )
        .is_none(),
        "a missing plan never bypasses impact validation"
    );
}

#[test]
fn rejection_telemetry_is_bounded_and_omits_raw_responses_and_arguments() {
    let calls = (0..8)
        .map(|_| ToolCallData {
            id: "secret-id".into(),
            name: "x".repeat(100),
            arguments: "secret-arguments".into(),
        })
        .collect::<Vec<_>>();
    let data = rejection_data(
        "impact",
        &"界".repeat(800),
        &calls,
        "private model response",
    );
    assert_eq!(data["reason"].as_str().unwrap().chars().count(), 480);
    assert_eq!(data["tool_count"], 8);
    assert_eq!(data["tool_names"].as_array().unwrap().len(), 4);
    assert_eq!(data["tool_names"][0].as_str().unwrap().len(), 64);
    let serialized = data.to_string();
    assert!(!serialized.contains("secret"));
    assert!(!serialized.contains("private model"));
}
