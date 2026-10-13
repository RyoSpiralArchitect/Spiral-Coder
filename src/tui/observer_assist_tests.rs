fn suggestion_response(kind: &str, tool: &str, args: serde_json::Value) -> String {
    json!({
        "summary":"One concrete next step.",
        "primary_blocker":"missing_verification",
        "response_contract":{"required":true,"focus_axes":["correctness"]},
        "suggestions":[{"kind":kind,"reason":"Complete the remaining obligation.","confidence":0.95,"suggested_tool":tool,"suggested_args":args}],
        "quickest_check":"exec(command=touch unrelated-file)",
    }).to_string()
}

#[test]
fn observer_replay_does_not_forward_unknown_tools_or_invalid_arguments() {
    for (kind, tool, args) in [
        ("clarify", "cargo", json!({"command":"cargo test"})),
        ("verify", "run_command", json!({"command":"cargo test"})),
        ("read", "exec", json!({"command":"cargo test"})),
        ("read", "read_file", json!({})),
        ("read", "read_file", json!({"path":42})),
        ("verify", "exec", json!({"command":"  "})),
        ("verify", "exec", json!(["cargo test"])),
        ("edit", "patch_file", json!({"path":"src/lib.rs","search":"before"})),
    ] {
        let (mut app, _root) = interaction_app();
        app.coder.messages.push(msg(Role::Assistant, "[GOVERNOR BLOCK] Required verification missing."));
        let result = replay_observer_next_action_case(&mut app, "last-fail", "missing_verification", &suggestion_response(kind, tool, args)).unwrap();
        assert!(result.parsed_suggestion.is_some(), "preserve visible invalid suggestion");
        assert!(result.pending_observer_hint.is_none(), "{kind}/{tool}");
        assert!(result.pending_observer_contract.is_none());
        assert!(!result.coder_preview_messages.iter().any(|message| message.content.contains("<observer_advisory") || message.content.contains("<observer_response_contract")));
        assert!(app.observer.messages.iter().any(|message| message.content.contains("prevented automatic forwarding")));
    }
}

#[test]
fn observer_verify_handoff_uses_one_typed_action_not_free_text_quickest_check() {
    let (mut app, root) = interaction_app();
    app.coder.messages.push(msg(Role::Assistant, "[GOVERNOR BLOCK] Missing current verification."));
    let command = "cargo test -q Parser::Case42 -- --exact";
    let result = replay_observer_next_action_case(&mut app, "last-fail", "missing_verification", &suggestion_response("verify", "exec", json!({"command":command}))).unwrap();
    assert!(result.pending_observer_hint.as_ref().unwrap().contains(command));
    assert!(result.pending_observer_contract.is_some());
    let forwarded = result.coder_preview_messages.iter().filter(|message| message.content.contains("<observer_")).collect::<Vec<_>>();
    assert_eq!(forwarded.len(), 2);
    for message in forwarded {
        assert!(message.content.contains(command));
        assert!(!message.content.contains("touch unrelated-file"));
    }
    assert!(!root.path().join("unrelated-file").exists());
}

#[test]
fn observer_no_tool_handoff_does_not_promote_free_text_to_an_action() {
    let (mut app, _root) = interaction_app();
    app.coder.messages.push(msg(Role::Assistant, "[GOVERNOR BLOCK] Need task clarification."));
    let response = json!({
        "summary":"The target is ambiguous.",
        "primary_blocker":"ambiguous_target",
        "response_contract":{"required":true,"focus_axes":["correctness"]},
        "suggestions":[{"kind":"clarify","reason":"Ask which parser behavior to preserve.","confidence":0.95,"suggested_tool":null,"suggested_args":{}}],
        "quickest_check":"exec(command=touch unrelated-file)",
    }).to_string();
    let result = replay_observer_next_action_case(&mut app, "last-fail", "ambiguous_target", &response).unwrap();
    assert!(result.pending_observer_hint.is_some());
    assert!(result.pending_observer_contract.is_some());
    let forwarded = result.coder_preview_messages.iter().filter(|message| message.content.contains("<observer_")).collect::<Vec<_>>();
    assert_eq!(forwarded.len(), 2);
    for message in forwarded {
        assert!(message.content.contains("Ask which parser behavior to preserve."));
        assert!(!message.content.contains("touch unrelated-file"));
    }
}

#[test]
fn observer_optional_display_shape_cannot_discard_or_replace_a_typed_action() {
    for display in [json!({"tool":"exec","command":"touch unrelated-file"}), json!(["exec"]), json!(null), json!(42)] {
        let (mut app, root) = interaction_app();
        app.coder.messages.push(msg(Role::Assistant, "[GOVERNOR BLOCK] Need a source read."));
        let mut response: serde_json::Value = serde_json::from_str(&suggestion_response("read", "read_file", json!({"path":"src/parser.rs"}))).unwrap();
        response["quickest_check"] = display.clone();
        let result = replay_observer_next_action_case(&mut app, "last-fail", "missing_read", &response.to_string()).unwrap();
        assert!(result.parsed_suggestion.as_ref().unwrap().quickest_check.is_empty());
        assert!(result.pending_observer_hint.as_ref().unwrap().contains("read_file(path=src/parser.rs)"));
        let forwarded = result.coder_preview_messages.iter().filter(|message| message.content.contains("<observer_")).collect::<Vec<_>>();
        assert_eq!(forwarded.len(), 2);
        for message in forwarded {
            assert!(message.content.contains("read_file(path=src/parser.rs)"));
            assert!(!message.content.contains("touch unrelated-file"));
        }
        assert!(!root.path().join("unrelated-file").exists());
        response["suggestions"][0]["suggested_args"] = json!({"path":42});
        let invalid = replay_observer_next_action_case(&mut app, "last-fail", "missing_read", &response.to_string()).unwrap();
        assert!(invalid.pending_observer_hint.is_none());
        assert!(invalid.pending_observer_contract.is_none());
    }
}

#[test]
fn observer_edit_advisory_does_not_execute_or_grant_proof() {
    let (mut app, root) = interaction_app();
    std::fs::write(root.path().join("artifact.txt"), "before").unwrap();
    app.coder.messages.push(msg(Role::Assistant, "[GOVERNOR BLOCK] Need a revised patch."));
    let result = replay_observer_next_action_case(&mut app, "last-fail", "bad_edit", &suggestion_response("edit", "patch_file", json!({"path":"artifact.txt","search":"before","replace":"after"}))).unwrap();
    assert!(result.pending_observer_hint.is_some());
    assert_eq!(std::fs::read_to_string(root.path().join("artifact.txt")).unwrap(), "before");
    assert!(app.coder_governor.is_none());
    assert!(app.active_observer_contract.is_none());
}

#[test]
fn observer_packet_retains_task_and_late_diagnostic_without_inventing_execution() {
    let (mut app, _root) = interaction_app();
    let goal = "Repair only .spiral-coder/tui_replay.json and verify the intended case.";
    app.coder_intent_anchor = Some(intent::apply_intent_update(None, intent::normalize_intent_update(goal, None), goal));
    app.coder.messages.push(msg(Role::User, goal));
    for _ in 0..12 { app.coder.messages.push(msg(Role::Assistant, "Continuing the existing task.")); }
    app.coder.messages.push(msg(Role::User, "Continue."));
    app.coder.messages.push(msg(Role::Tool, &format!("OK: patched '.spiral-coder/tui_replay.json'\n{}[auto-test] ✗ FAILED (exit 1)\nError: input spec=.spiral-coder/tui_replay.json; missing assistant target", "[diff] source context\n".repeat(30))));
    app.coder.messages.push(msg(Role::Assistant, "[GOVERNOR BLOCK] Diagnose the failed replay."));
    let packet = build_tui_meta_failure_packet_for_selector(&app, "last-fail").unwrap();
    assert_eq!(packet["task_summary"], goal);
    assert!(packet["recent_tool_results"][0]["diagnostics"].to_string().contains("missing assistant target"));
    assert!(packet["available_tools"].as_array().unwrap().contains(&json!("exec")));
    let catalog = packet["tool_argument_schemas"].as_array().unwrap();
    assert_eq!(catalog.iter().find(|schema| schema["name"] == "exec").unwrap()["parameters"], agent::exec_tool_def()["function"]["parameters"]);
    assert!(packet["recent_tool_calls"].as_array().unwrap().is_empty());
    assert!(packet["recovery_stage"].is_null());
}
