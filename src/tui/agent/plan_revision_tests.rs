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
