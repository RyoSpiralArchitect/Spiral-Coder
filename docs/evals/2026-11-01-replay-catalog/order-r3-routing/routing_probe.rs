#[test]
fn catalog_retained_verification_route() {
    let session: Value = serde_json::from_slice(
        &std::fs::read(std::env::var("CATALOG_SAVED_SESSION").unwrap()).unwrap(),
    )
    .unwrap();
    let calls: Vec<String> =
        serde_json::from_str(&std::env::var("CATALOG_CALL_IDS").unwrap()).unwrap();
    let workspace = std::env::var("CATALOG_CANDIDATE_ROOT").unwrap();
    let messages = session["messages"].as_array().unwrap();
    for id in calls {
        let index = messages
            .iter()
            .position(|m| {
                m["tool_calls"].as_array().is_some_and(|calls| {
                    calls.iter().any(|c| c["id"].as_str() == Some(id.as_str()))
                })
            })
            .expect("retained native call");
        let prefix = &messages[..index];
        let root = crate::task_origin::root_user_text(prefix);
        assert!(super::super::requirement_proof::parse(root)
            .unwrap()
            .is_some());
        let authored = call("exec", json!({"command":"cargo test 2>&1"}));
        let canonical = |c: &ToolCallData| {
            super::super::canonicalize_tool_call_command(&c.name, &c.arguments).unwrap()
        };
        let followup = coerce_existing_followup_tool_call(prefix, &authored, root, None);
        let effective = followup.as_ref().map(|v| &v.0).unwrap_or(&authored);
        let focus = super::super::coerce_fix_existing_tool_call(
            TaskHarness::infer(root, false),
            prefix,
            effective,
            Some("cargo test 2>&1"),
            Some(&workspace),
        );
        let last = focus.as_ref().map(|v| &v.0).unwrap_or(effective);
        println!(
            "ROUTE {}",
            json!({"call_id":id,"prefix_messages":index,"proposed":canonical(&authored),
            "after_followup":canonical(effective),"after_focus":canonical(last)})
        );
    }
}
