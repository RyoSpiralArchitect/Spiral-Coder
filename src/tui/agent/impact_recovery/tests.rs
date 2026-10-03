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

#[tokio::test]
async fn invalid_resumed_impact_retry_receives_the_validated_plan_labels() {
    use httpmock::prelude::*;
    let server = MockServer::start();
    let invalid_impact = "<impact>\nchanged: Updated the prepared docs.\nprogress: invented milestone outside the current plan\nremaining_gap: Run the required verification.\n</impact>";
    let response = format!(
        "data: {}\n\ndata: [DONE]\n\n",
        json!({"choices":[{
            "delta":{"content":invalid_impact,"tool_calls":[{"index":0,"id":"retryexec","function":{"name":"exec","arguments":"{\"command\":\"pwd\"}"}}]},
            "finish_reason":"tool_calls"
        }]})
    );
    let initial = server.mock(|when, then| {
        when.method(POST)
            .path("/chat/completions")
            .matches(|request| {
                !String::from_utf8_lossy(request.body.as_deref().unwrap_or_default())
                    .contains("[Plan Steps]")
            });
        then.status(200)
            .header("content-type", "text/event-stream")
            .body(&response);
    });
    let retry = server.mock(|when, then| {
        when.method(POST)
            .path("/chat/completions")
            .body_contains("[Plan Steps]")
            .body_contains("- step 1:")
            .body_contains("[Acceptance Criteria]")
            .body_contains("- acceptance 1:");
        then.status(200)
            .header("content-type", "text/event-stream")
            .body(&response);
    });
    let prompt = "[Observer benchmark plan approved]\n<observer_benchmark_plan required=\"true\">\nlane: runtime_eval\nobjective: Finish prepared regression artifacts for src/tui/agent/benchmark_proof.rs.\ntarget_files:\n- .spiral-coder/runtime_eval.json\n- docs/runtime-architecture.md\nrequired_checks:\n- grep -q proof docs/runtime-architecture.md\nsuccess_criteria:\n- Current checks pass after the latest edit.\n</observer_benchmark_plan>\nResume the work and complete the required verification.";
    let seed: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/runtime-benchmark-plan-exec-proof/session_seed.json"
    ))
    .unwrap();
    let mut messages = seed["messages"].as_array().unwrap().clone();
    messages.push(json!({"role":"user","content":prompt}));
    let start = AgenticStartState {
        messages,
        checkpoint: None,
        cur_cwd: None,
        observation_cache: None,
        session_bridge: None,
        create_checkpoint: false,
    };
    let cfg = RunConfig {
        provider: ProviderKind::Mistral,
        model: "local-mock".into(),
        chat_model: "local-mock".into(),
        code_model: "local-mock".into(),
        api_key: None,
        base_url: server.base_url(),
        mode: crate::modes::Mode::Vibe,
        persona: String::new(),
        temperature: 0.0,
        max_tokens: 512,
        timeout_seconds: 5,
        hf_device: "cpu".into(),
        hf_local_only: true,
    };
    let root = tempfile::tempdir().unwrap();
    let (tx, mut rx) = mpsc::channel(512);
    run_agentic_json(
        start,
        &cfg,
        root.path().to_str(),
        2,
        tx,
        None,
        None,
        None,
        true,
        Some(RealizePreset::Off),
        None,
        &crate::approvals::AutoApprover,
    )
    .await
    .unwrap();
    initial.assert_hits(1);
    retry.assert_hits(1);
    let mut rejected = 0;
    let mut restored = 0;
    while let Some(token) = rx.recv().await {
        if let StreamToken::Telemetry(event) = token {
            if event.event == "pre_tool_gate_rejected" && event.data["gate"] == "impact" {
                rejected += 1;
            }
            if event.event == "impact_resume_plan_restored" {
                restored += 1;
            }
        }
    }
    assert_eq!(
        rejected, 2,
        "invalid progress remains rejected even after the plan is visible"
    );
    assert_eq!(restored, 1, "the validated plan is retained for the retry");
}
