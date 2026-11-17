use super::*;

fn live() -> serde_json::Value {
    serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/plan-revision/live-plans.json"
    )))
    .unwrap()
}

#[test]
fn recorded_seven_step_plan_does_not_count_parenthesized_exit_codes_as_steps() {
    let fixture = live();
    let plan = parse_plan_block(fixture["seven_step_text"].as_str().unwrap()).unwrap();
    assert_eq!(plan.steps.len(), 7, "{:?}", plan.steps);
    assert!(plan.steps[0].contains("FAILED (exit 1) recorded"));
    validate_plan(&plan).unwrap();
}

#[test]
fn recorded_pending_steps_keep_the_accepted_goal_and_acceptance_on_resume() {
    let fixture = live();
    let root = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/benchmarks/examples-v1/cases/event-summary/task.txt"
    ));
    let contract = derive_task_contract(root, false, true, VerificationLevel::Behavioral);
    let resolver = InstructionResolver::new(root, false, false);
    let previous: PlanBlock = serde_json::from_value(fixture["previous_plan"].clone()).unwrap();
    let revised = parse_plan_block(fixture["pending_steps_text"].as_str().unwrap()).unwrap();
    let messages = vec![
        json!({"role":"user","content":root}),
        json!({"role":"assistant","content":"","accepted_plan":previous}),
    ];
    validate_plan_for_task_contract(&previous, false, &contract, &resolver, &messages).unwrap();
    assert_eq!(revised.goal, previous.goal);
    assert_eq!(revised.acceptance_criteria, previous.acceptance_criteria);
    validate_plan_for_task_contract(&revised, false, &contract, &resolver, &messages).unwrap();

    let snapshot = plan_revision::snapshot(Some(&revised), &messages, &contract);
    assert!(snapshot["alignment_anchor"].is_object());
    // The old plan exchange can disappear: the newest admission owns its anchor.
    let mut resumed = vec![
        messages[0].clone(),
        json!({"role":"assistant","content":"","accepted_plan":snapshot}),
    ];
    for _ in 0..60 {
        resumed.push(json!({"role":"assistant","content":"Continuing"}));
    }
    resumed.push(crate::task_origin::user_message(
        "Continue",
        crate::task_origin::MessageOrigin::Runtime,
    ));
    message_window::prune_message_window(&mut resumed);
    let resumed: Vec<serde_json::Value> =
        serde_json::from_str(&serde_json::to_string(&resumed).unwrap()).unwrap();
    let restored = plan_resume::latest_valid(&resumed, |plan| {
        validate_plan_for_task_contract(plan, false, &contract, &resolver, &resumed).is_ok()
    })
    .unwrap();
    assert_eq!(
        serde_json::to_value(restored).unwrap(),
        serde_json::to_value(&revised).unwrap()
    );
    assert!(crate::task_origin::provider_messages(&resumed)
        .iter()
        .all(|m| m.get("accepted_plan").is_none()));

    for corrupt in ["goal", "acceptance_criteria"] {
        let mut bad = snapshot.clone();
        if corrupt == "goal" {
            bad["alignment_anchor"][corrupt] = json!("Unrelated goal");
        } else {
            bad["alignment_anchor"][corrupt] = json!(["Unrelated acceptance"]);
        }
        let messages = vec![
            messages[0].clone(),
            json!({"role":"assistant","accepted_plan":bad}),
        ];
        assert!(
            validate_plan_for_task_contract(&revised, false, &contract, &resolver, &messages)
                .is_err()
        );
    }
    let mut new_task = messages.clone();
    new_task.push(json!({"role":"user","content":root}));
    assert!(
        validate_plan_for_task_contract(&revised, false, &contract, &resolver, &new_task).is_err()
    );
    let prose_only = vec![
        messages[0].clone(),
        json!({"role":"assistant","content":fixture["seven_step_text"]}),
    ];
    assert!(
        validate_plan_for_task_contract(&revised, false, &contract, &resolver, &prose_only)
            .is_err()
    );

    let mut oversized = revised.clone();
    oversized.steps = vec!["Read source".into(); 8];
    assert!(
        validate_plan_for_task_contract(&oversized, false, &contract, &resolver, &messages)
            .is_err()
    );
    let read_only_resolver = InstructionResolver::new(root, true, false);
    let mut mutating = revised;
    mutating.steps = vec!["patch unrelated files".into(), "Report results".into()];
    assert!(validate_plan_for_task_contract(
        &mutating,
        true,
        &contract,
        &read_only_resolver,
        &messages
    )
    .is_err());
}

#[test]
fn eight_real_steps_still_fail_and_inline_lists_remain_supported() {
    assert_eq!(
        parse_plan_items("1)Read source 2)Verify result"),
        ["Read source", "Verify result"]
    );
    let list = (1..=8)
        .map(|i| format!("{i}) Action with (exit 1) data\n"))
        .collect::<String>();
    assert_eq!(parse_plan_items(&list).len(), 8);
}

#[test]
fn observation_restores_the_admitted_goal_with_authored_steps_and_acceptance() {
    let root = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/benchmarks/examples-v1/cases/event-summary/task.txt"
    ));
    let contract = derive_task_contract(root, false, true, VerificationLevel::Behavioral);
    let resolver = InstructionResolver::new(root, false, false);
    let previous: PlanBlock = serde_json::from_value(live()["previous_plan"].clone()).unwrap();
    let mut proposal = parse_plan_block(live()["pending_steps_text"].as_str().unwrap()).unwrap();
    proposal.goal = "Inspect the immediate error and collect the comparison".into();
    let messages = [
        json!({"role":"user","content":root}),
        json!({"role":"assistant","accepted_plan":previous}),
    ];
    assert!(
        validate_plan_for_task_contract(&proposal, false, &contract, &resolver, &messages).is_err()
    );
    let restored =
        plan_revision::preserve_goal_for_observation(&proposal, &previous, &messages, &contract)
            .unwrap();
    assert_eq!(restored.goal, previous.goal);
    assert_eq!(restored.steps, proposal.steps);
    assert_eq!(restored.risks, proposal.risks);
    assert_eq!(restored.assumptions, proposal.assumptions);
    assert_eq!(restored.acceptance_criteria, proposal.acceptance_criteria);
    validate_plan_for_task_contract(&restored, false, &contract, &resolver, &messages).unwrap();
    assert!(plan_revision::preserve_goal_for_observation(
        &proposal,
        &previous,
        &messages[..1],
        &contract,
    )
    .is_none());
    proposal.acceptance_criteria[0] = "A different criterion".into();
    assert!(plan_revision::preserve_goal_for_observation(
        &proposal, &previous, &messages, &contract,
    )
    .is_none());
}

#[test]
fn real_search_rejection_quotes_only_the_bound_objective_for_a_fresh_attempt() {
    let sample: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/plan-revision/search-walk-h2.json"
    )))
    .unwrap();
    let root = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/benchmarks/self-dogfood-v2/search-walk-links/prompt.txt"
    ));
    let contract = derive_task_contract(root, false, true, VerificationLevel::Behavioral);
    let accepted: PlanBlock = serde_json::from_value(sample["accepted"].clone()).unwrap();
    let mut proposed = parse_plan_block(sample["rejected"].as_str().unwrap()).unwrap();
    let messages = vec![
        json!({"role":"user","content":root}),
        json!({"role":"assistant","accepted_plan":accepted}),
    ];
    let error = plan_revision::validate_alignment(&proposed, &messages, &contract).unwrap_err();
    let mut stage = None;
    let feedback = plan_revision::rejection(
        &mut stage,
        &error.to_string(),
        Some(&accepted),
        &messages,
        &contract,
    );
    assert_eq!(stage, None, "the refused check did not execute");
    assert!(feedback.contains(&format!(
        "exactly {} separate numbered lines",
        accepted.acceptance_criteria.len()
    )));
    for previous in [
        RecoveryStage::Diagnose,
        RecoveryStage::Fix,
        RecoveryStage::Verify,
    ] {
        stage = Some(previous);
        let _ = plan_revision::rejection(
            &mut stage,
            &error.to_string(),
            Some(&accepted),
            &messages,
            &contract,
        );
        assert_eq!(stage, Some(previous), "keep outstanding, genuine recovery");
    }
    let fields: serde_json::Value = serde_json::from_str(feedback.lines().last().unwrap()).unwrap();
    assert_eq!(
        fields,
        json!({"goal":accepted.goal,"acceptance":accepted.acceptance_criteria})
    );
    // Rendering feedback cannot admit the original proposal or its tool call.
    assert!(plan_revision::validate_alignment(&proposed, &messages, &contract).is_err());
    proposed.goal = fields["goal"].as_str().unwrap().to_owned();
    proposed.acceptance_criteria = serde_json::from_value(fields["acceptance"].clone()).unwrap();
    let resolver = InstructionResolver::new(root, false, false);
    validate_plan_for_task_contract(&proposed, false, &contract, &resolver, &messages).unwrap();
    let mut new_task = messages.clone();
    new_task.push(json!({"role":"user","content":"Another task."}));
    stage = None;
    let ordinary = governor_contract::invalid_plan_message(&error.to_string());
    assert_eq!(
        plan_revision::rejection(
            &mut stage,
            &error.to_string(),
            Some(&accepted),
            &new_task,
            &contract
        ),
        ordinary
    );
    assert_eq!(
        plan_revision::rejection(&mut stage, &error.to_string(), None, &messages, &contract),
        ordinary
    );
    assert_eq!(
        plan_revision::rejection(
            &mut stage,
            "wrong shape",
            Some(&accepted),
            &messages,
            &contract
        ),
        governor_contract::invalid_plan_message("wrong shape")
    );
    assert_eq!(stage, Some(RecoveryStage::Diagnose));
}

#[test]
fn recorded_flattened_acceptance_requires_numbered_boundaries() {
    let sample: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/docs/evals/2026-11-11-coupled-status/s5-checklist/plan-format-rejection.json"
    )))
    .unwrap();
    let proposed = sample["proposed"].as_str().unwrap();
    let fields: serde_json::Value = serde_json::from_str(
        sample["rejection"]
            .as_str()
            .unwrap()
            .lines()
            .last()
            .unwrap(),
    )
    .unwrap();
    let items: Vec<String> = serde_json::from_value(fields["acceptance"].clone()).unwrap();
    let flat = parse_plan_block(proposed).unwrap();
    assert_eq!(items.len(), 4);
    assert_eq!(flat.acceptance_criteria.len(), 1);
    let start = proposed.find("acceptance:").unwrap();
    let end = start + proposed[start..].find("risks:").unwrap();
    let numbered = items
        .iter()
        .enumerate()
        .map(|(i, s)| format!("{}. {}", i + 1, s))
        .collect::<Vec<_>>()
        .join("\n");
    let corrected = format!(
        "{}acceptance:\n{}\n{}",
        &proposed[..start],
        numbered,
        &proposed[end..]
    );
    let restored = parse_plan_block(&corrected).unwrap();
    assert_eq!(restored.goal, flat.goal);
    assert_eq!(restored.steps, flat.steps);
    assert_eq!(restored.acceptance_criteria, items);
}
