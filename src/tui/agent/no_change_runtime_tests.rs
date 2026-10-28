use super::*;

fn add(messages: &mut Vec<serde_json::Value>, name: &str, args: serde_json::Value, output: &str) {
    let id = format!("call-{}", messages.len());
    messages.push(json!({"role":"assistant","tool_calls":[{"id":id,"function":{"name":name,"arguments":args.to_string()}}]}));
    messages.push(json!({"role":"tool","tool_call_id":id,"content":output}));
}

#[test]
fn no_change_preserves_pending_edits_and_current_proof_after_roundtrip() {
    let root = "Repair file.txt. Run exactly: cargo test";
    let mut messages = vec![json!({"role":"user","content":root})];
    add(
        &mut messages,
        "exec",
        json!({"command":"cargo test"}),
        "OK (exit_code: 0)",
    );
    add(
        &mut messages,
        "patch_file",
        json!({"path":"file.txt","search":"missing","replace":"new"}),
        "ERROR: search string not found",
    );
    let before = restore_done_gate_from_messages(&messages, Some("cargo test"));
    for name in ["write_file", "patch_file", "apply_diff"] {
        add(&mut messages, name, json!({"path":"file.txt"}), "OK: unchanged 'file.txt' (identical contents; no write performed)\n[auto-test] ✓ PASSED (exit 0)");
    }
    let messages: Vec<serde_json::Value> =
        serde_json::from_str(&serde_json::to_string(&messages).unwrap()).unwrap();
    let after = restore_done_gate_from_messages(&messages, Some("cargo test"));
    assert_eq!((before.1, before.2, before.3), (after.1, after.2, after.3));
    let context =
        exec_verification::ExecVerificationContext::from_messages(Some("cargo test"), &messages);
    assert_eq!(
        EditFailureMemory::from_messages(&messages, &context).count(),
        1
    );
    assert_eq!(
        acceptance_report::current_commands(&messages, Some("cargo test")),
        ["cargo test"]
    );
    let packet = evidence_packet::project(&messages, Some("cargo test"));
    assert_eq!(packet.mutation_step, None);
    assert!(packet.pending_exact_checks.is_empty());
    let progress = serde_json::to_value(work_progress::project(root, &packet)).unwrap();
    assert_eq!(progress["native_file_edits"]["succeeded"], 0);
    assert_eq!(progress["native_file_edits"]["unchanged"], 3);
    for action in packet.actions.iter().skip(2) {
        assert_eq!(
            action.edit_outcome,
            Some(crate::execution_evidence::FileEditOutcome::Unchanged)
        );
        assert_eq!(action.succeeded, Some(true));
    }
}

#[test]
fn no_change_neither_satisfies_a_missing_check_nor_clears_a_newer_failure() {
    let root = "Run exactly: cargo test";
    let mut messages = vec![json!({"role":"user","content":root})];
    add(
        &mut messages,
        "exec",
        json!({"command":"cargo test"}),
        "FAILED (exit_code: 1)",
    );
    add(
        &mut messages,
        "patch_file",
        json!({"path":"source.rs"}),
        "OK: unchanged 'source.rs'",
    );
    let packet = evidence_packet::project(&messages, Some("cargo test"));
    assert_eq!(packet.pending_exact_checks, ["cargo test"]);
    assert_eq!(packet.verification_step, None);
    assert_eq!(packet.mutation_step, None);
    assert!(acceptance_report::current_commands(&messages, Some("cargo test")).is_empty());
}

#[cfg(not(target_os = "windows"))]
#[tokio::test]
async fn live_tui_no_change_does_not_run_automatic_tests_or_complete_a_step() {
    use httpmock::prelude::*;
    for (name, args) in [
        (
            "write_file",
            json!({"path":"fixture.txt","content":"before\n"}),
        ),
        (
            "patch_file",
            json!({"path":"fixture.txt","search":"before","replace":"before"}),
        ),
        (
            "apply_diff",
            json!({"path":"fixture.txt","diff":"@@\n-before\n+before\n"}),
        ),
    ] {
        let server = MockServer::start();
        let prelude = format!("<plan>\ngoal: Update fixture.txt and verify\nsteps:\n1) Apply the requested edit\n2) Verify the result\nacceptance: fixture.txt has requested content and tests pass\nrisks: wrong content\nassumptions: fixture.txt exists\n</plan>\n<think>\ngoal: Apply requested edit\nstep: 1\ntool: {name}\nrisk: wrong content\ndoubt: result may be unchanged\nnext: {name} fixture.txt\nverify: inspect the result\n</think>\n<evidence>\ntarget_files: 1) fixture.txt\ntarget_symbols: 1) file contents\nevidence: The native read of fixture.txt returned before\nopen_questions: none\nnext_probe: {name} fixture.txt\n</evidence>");
        let response = server.mock(|when, then| {
            when.method(POST).path("/chat/completions");
            then.status(200).header("content-type","text/event-stream").body(format!("data: {}\n\ndata: [DONE]\n\n", json!({"choices":[{"delta":{"content":prelude,"tool_calls":[{"index":0,"id":"noop","function":{"name":name,"arguments":args.to_string()}}]},"finish_reason":"tool_calls"}]})));
        });
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("fixture.txt"), "before\n").unwrap();
        let goal = "Update fixture.txt to the requested contents and verify the result.";
        let mut messages = vec![crate::task_origin::user_message(
            goal,
            crate::task_origin::MessageOrigin::User,
        )];
        let (read, failed) = crate::file_tools::tool_read_file("fixture.txt", root.path().to_str());
        assert!(!failed);
        add(
            &mut messages,
            "read_file",
            json!({"path":"fixture.txt"}),
            &read,
        );
        let start = AgenticStartState {
            messages,
            checkpoint: None,
            cur_cwd: None,
            observation_cache: None,
            session_bridge: None,
            create_checkpoint: false,
        };
        let config = super::check_catalog_tests::mock_config(&server);
        let (tx, _rx) = mpsc::channel(512);
        let result = run_agentic_json(
            start,
            &config,
            root.path().to_str(),
            1,
            tx,
            None,
            None,
            Some("printf reran > unexpected-test.txt".into()),
            false,
            None,
            None,
            &crate::approvals::AutoApprover,
        )
        .await
        .unwrap();
        response.assert_hits(1);
        let output = result
            .messages
            .iter()
            .find(|m| m["tool_call_id"] == "noop")
            .and_then(|m| m["content"].as_str())
            .unwrap_or("");
        assert!(output.starts_with("OK: unchanged '"), "{name}: {output}");
        assert!(!root.path().join("unexpected-test.txt").exists());
        assert_eq!(
            evidence_packet::project(&result.messages, None).mutation_step,
            None
        );
        assert!(WorkingMemory::from_messages(&result.messages, None)
            .completed_steps
            .is_empty());
        assert!(!result.messages.iter().any(|m| m["content"]
            .as_str()
            .is_some_and(|s| s.contains("successful mutation via"))));
    }
}
