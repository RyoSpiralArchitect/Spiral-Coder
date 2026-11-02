use super::*;

// Exercise the same native write -> automatic test -> Coder / Observer handoff as r4.
#[tokio::test]
async fn unregistered_rust_source_is_not_listed_even_when_the_native_auto_test_passes() {
    use httpmock::prelude::*;
    let server = MockServer::start();
    let root = tempfile::tempdir().unwrap();
    let dir = root.path();
    std::fs::create_dir(dir.join("src")).unwrap();
    std::fs::write(
        dir.join("Cargo.toml"),
        "[package]\nname='reach-fixture'\nversion='0.1.0'\nedition='2021'\n",
    )
    .unwrap();
    std::fs::write(dir.join("src/main.rs"), "fn main() {}\n").unwrap();
    let response = server.mock(|when, then| {
        when.method(POST).path("/chat/completions");
        let prelude = "<plan>\ngoal: Add a helper\nsteps:\n1) Create src/leaf.rs\n2) Verify the result\nacceptance: new helper exists and tests pass\nrisks: module may be unregistered\nassumptions: target is clear\n</plan>\n<think>\ngoal: Add the helper\nstep: 1\ntool: write_file\nrisk: missing module registration\ndoubt: compile reach\nnext: write_file src/leaf.rs\nverify: cargo test\n</think>";
        let args = json!({"path":"src/leaf.rs","content":"pub fn leaf() {}\n"});
        let delta = json!({"choices":[{"delta":{"content":prelude,"tool_calls":[{"index":0,"id":"leaf","function":{"name":"write_file","arguments":args.to_string()}}]},"finish_reason":"tool_calls"}]});
        then.status(200).header("content-type","text/event-stream").body(format!("data: {delta}\n\ndata: [DONE]\n\n"));
    });
    let start = AgenticStartState {
        messages: vec![crate::task_origin::user_message(
            "Create src/leaf.rs with a Rust helper and verify it.",
            crate::task_origin::MessageOrigin::User,
        )],
        checkpoint: None,
        cur_cwd: None,
        observation_cache: None,
        session_bridge: None,
        create_checkpoint: false,
    };
    let cmd = "cargo test --offline --target-dir isolated-target";
    let (tx, _rx) = mpsc::channel(512);
    let result = run_agentic_json(
        start,
        &super::check_catalog_tests::mock_config(&server),
        dir.to_str(),
        1,
        tx,
        None,
        None,
        Some(cmd.into()),
        false,
        None,
        None,
        &crate::approvals::AutoApprover,
    )
    .await
    .unwrap();
    response.assert_hits(1);
    let message = result
        .messages
        .iter()
        .find(|m| m["tool_call_id"] == "leaf")
        .unwrap();
    assert!(
        message["content"]
            .as_str()
            .unwrap()
            .contains("[auto-test] ✓ PASSED"),
        "{message}"
    );
    let packet =
        serde_json::to_value(evidence_packet::project(&result.messages, Some(cmd))).unwrap();
    let observer = current_evidence::observer_prompt_packet(&json!({"execution_evidence":packet}));
    assert_eq!(
        observer["execution_evidence"]["actions"][0]["cargo_inputs"]["files"]["src/leaf.rs"],
        "not_listed"
    );
    let wire = crate::task_origin::provider_messages(&result.messages);
    let content = wire.iter().find(|m| m["tool_call_id"] == "leaf").unwrap()["content"]
        .as_str()
        .unwrap();
    assert!(
        content.contains("not_listed") && content.contains("src/leaf.rs"),
        "{content}"
    );
}
