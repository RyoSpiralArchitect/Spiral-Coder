use super::*;

#[test]
fn exec_accepts_a_declared_check_selection_without_retyping_the_command() {
    let tool = exec_tool_def();
    assert_eq!(
        tool["function"]["parameters"]["properties"]["check_id"]["type"],
        "string"
    );
    assert!(!tool["function"]["parameters"]["required"]
        .as_array()
        .is_some_and(|items| items.iter().any(|v| v == "command")));
}

#[cfg(not(target_os = "windows"))]
pub(super) fn mock_config(server: &httpmock::MockServer) -> RunConfig {
    RunConfig {
        provider: ProviderKind::OpenAiCompatible,
        model: "test".into(),
        chat_model: "test".into(),
        code_model: "test".into(),
        api_key: None,
        base_url: server.base_url(),
        mode: crate::modes::Mode::Vibe,
        persona: String::new(),
        temperature: 0.0,
        max_tokens: 512,
        timeout_seconds: 5,
        hf_device: "cpu".into(),
        hf_local_only: true,
    }
}
fn call(args: serde_json::Value) -> ToolCallData {
    ToolCallData {
        id: "select".into(),
        name: "exec".into(),
        arguments: args.to_string(),
        thought_signature: None,
    }
}
fn root() -> String {
    format!(
        "Run exactly: cargo test --locked -q\n<task_checks>\n{}\n</task_checks>",
        json!({"version":1,"checks":[{"id":"reproduce","command":"cargo test --locked Repro -- --exact"}]})
    )
}

#[test]
fn selections_preserve_exact_commands_and_do_not_add_proof_obligations() {
    let root = root();
    let catalog = check_catalog::Catalog::from_root(&root);
    assert_eq!(catalog.checks.len(), 2);
    assert_eq!(
        task_harness::benchmark_proof::required_commands(&root),
        ["cargo test --locked -q"]
    );
    let mut selected = call(json!({"check_id":"reproduce"}));
    let receipt = catalog
        .resolve(&mut selected, Some("/workspace"))
        .unwrap()
        .unwrap();
    assert_eq!(receipt["command"], "cargo test --locked Repro -- --exact");
    assert_eq!(receipt["cwd"], "/workspace");
    assert!(!check_catalog::allows_coercion(&selected));
    assert_eq!(
        parse_exec_command_from_args(&selected.arguments).as_deref(),
        Some("cargo test --locked Repro -- --exact")
    );
    let mut tools = json!([exec_tool_def()]);
    assert!(catalog
        .bind(&mut tools, "https://api.mistral.ai/v1")
        .unwrap()
        .contains("reproduce"));
    assert_eq!(
        tools[0]["function"]["parameters"]["properties"]["check_id"]["enum"],
        json!(["required-1", "reproduce"])
    );
}

#[test]
fn unknown_ids_overrides_and_signed_calls_never_execute_or_change_the_call() {
    let catalog = check_catalog::Catalog::from_root(&root());
    for args in [
        json!({"check_id":"missing"}),
        json!({"check_id":1}),
        json!({"check_id":"reproduce","command":"cargo test | tail -60"}),
        json!({"check_id":"reproduce","cwd":"elsewhere"}),
    ] {
        let mut selected = call(args);
        let before = selected.to_json();
        assert!(catalog.resolve(&mut selected, Some("/workspace")).is_err());
        assert_eq!(selected.to_json(), before);
    }
    let mut selected = call(json!({"check_id":"reproduce"}));
    assert!(catalog.resolve(&mut selected, None).is_err());
    selected.thought_signature = crate::streaming::provider_metadata::ThoughtSignature::from_call(
        &json!({"extra_content":{"google":{"thought_signature":"opaque"}}}),
    )
    .unwrap();
    let original = selected.to_json();
    assert!(catalog.resolve(&mut selected, Some("/workspace")).is_err());
    assert_eq!(selected.to_json(), original);
    let mut tools = json!([exec_tool_def()]);
    assert!(catalog
        .bind(
            &mut tools,
            "https://generativelanguage.googleapis.com/v1beta/openai"
        )
        .is_none());
    assert!(tools[0]["function"]["parameters"]["properties"]
        .get("check_id")
        .is_none());
}

#[test]
fn catalog_uses_only_the_current_human_task_and_rejects_ambiguous_declarations() {
    for root in [
        format!("```text\n{}\n```", root()),
        root().lines().map(|l| format!("> {l}\n")).collect(),
    ] {
        assert!(check_catalog::Catalog::from_root(&root).checks.is_empty());
    }
    for body in [
        json!({"version":1,"checks":[{"id":"required-1","command":"cargo check"}]}),
        json!({"version":1,"checks":[{"id":"bad","command":"printf changed > source.rs"}]}),
        json!({"version":2,"checks":[]}),
    ] {
        let root = format!("Run exactly: cargo test\n<task_checks>\n{body}\n</task_checks>");
        assert!(check_catalog::Catalog::from_root(&root).checks.is_empty());
        assert_eq!(
            task_harness::benchmark_proof::required_commands(&root),
            ["cargo test"]
        );
    }
    let messages = vec![
        crate::task_origin::user_message(
            "Inspect the source",
            crate::task_origin::MessageOrigin::User,
        ),
        crate::task_origin::user_message(root(), crate::task_origin::MessageOrigin::Runtime),
    ];
    assert!(evidence_packet::project(&messages, None)
        .available_checks
        .is_empty());
}

#[test]
fn a_resolved_check_needs_actual_fresh_execution_and_keeps_command_identity() {
    let root = root();
    let catalog = check_catalog::Catalog::from_root(&root);
    let mut selected = call(json!({"check_id":"required-1"}));
    catalog.resolve(&mut selected, Some("/workspace")).unwrap();
    let mut messages = vec![
        json!({"role":"user","content":root}),
        json!({"role":"assistant","tool_calls":[selected.to_json()]}),
    ];
    let pending = |messages: &[serde_json::Value]| {
        task_harness::benchmark_proof::pending_commands(messages, &root, None)
    };
    assert_eq!(pending(&messages), ["cargo test --locked -q"]);
    messages.push(json!({"role":"tool","tool_call_id":"select","content":"OK (exit_code: 0)"}));
    assert!(pending(&messages).is_empty());
    let resumed: Vec<serde_json::Value> =
        serde_json::from_str(&serde_json::to_string(&messages).unwrap()).unwrap();
    assert!(pending(&resumed).is_empty());
    messages.extend([json!({"role":"assistant","tool_calls":[{"id":"edit","function":{"name":"patch_file","arguments":"{}"}}]}),json!({"role":"tool","tool_call_id":"edit","content":"OK: patched 'src/main.rs'"})]);
    assert_eq!(pending(&messages), ["cargo test --locked -q"]);
    let mut altered = selected.clone();
    altered.id = "altered".into();
    altered.arguments = json!({"command":"cargo test --locked -q 2>&1 | tail -60"}).to_string();
    messages.extend([
        json!({"role":"assistant","tool_calls":[altered.to_json()]}),
        json!({"role":"tool","tool_call_id":"altered","content":"OK (exit_code: 0)"}),
    ]);
    assert_eq!(pending(&messages), ["cargo test --locked -q"]);
}

#[cfg(not(target_os = "windows"))]
#[tokio::test]
async fn live_check_selection_uses_the_root_directory_and_honors_rejection() {
    use httpmock::prelude::*;
    struct Approval {
        allow: bool,
        seen: std::sync::Mutex<Vec<crate::approvals::ApprovalRequest>>,
    }
    #[async_trait::async_trait]
    impl crate::approvals::Approver for Approval {
        async fn approve(
            &self,
            request: crate::approvals::ApprovalRequest,
        ) -> anyhow::Result<crate::approvals::ApprovalOutcome> {
            self.seen.lock().unwrap().push(request);
            Ok(if self.allow {
                crate::approvals::ApprovalOutcome::Approved
            } else {
                crate::approvals::ApprovalOutcome::Rejected
            })
        }
    }
    for allow in [true, false] {
        let server = MockServer::start();
        let prelude="<plan>\ngoal: Verify fixture.txt\nsteps:\n1) Run the declared check\n2) Report its outcome\nacceptance: the declared check passes\nrisks: wrong directory\nassumptions: fixture.txt exists\n</plan>\n<think>\ngoal: Verify fixture.txt\nstep: 1\ntool: exec\nrisk: wrong directory\ndoubt: contents may differ\nnext: run required-1\nverify: exit status is zero\n</think>";
        let mock=server.mock(|when,then| {
            when.method(POST).path("/chat/completions");
            then.status(200).header("content-type","text/event-stream").body(format!("data: {}\n\ndata: [DONE]\n\n",json!({"choices":[{"delta":{"content":prelude,"tool_calls":[{"index":0,"id":"selection","function":{"name":"exec","arguments":"{\"check_id\":\"required-1\"}"}}]},"finish_reason":"tool_calls"}]})));
        });
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("fixture.txt"), "retained").unwrap();
        std::fs::create_dir(root.path().join("elsewhere")).unwrap();
        let goal="Verify fixture.txt.\n<observer_benchmark_plan>\nrequired_checks:\n- grep -q retained fixture.txt\n</observer_benchmark_plan>";
        let start = AgenticStartState {
            messages: vec![crate::task_origin::user_message(
                goal,
                crate::task_origin::MessageOrigin::User,
            )],
            checkpoint: None,
            cur_cwd: Some(root.path().join("elsewhere").to_string_lossy().into()),
            observation_cache: None,
            session_bridge: None,
            create_checkpoint: false,
        };
        let approver = Approval {
            allow,
            seen: Default::default(),
        };
        let (tx, _rx) = mpsc::channel(512);
        let result = run_agentic_json(
            start,
            &mock_config(&server),
            root.path().to_str(),
            1,
            tx,
            None,
            None,
            None,
            true,
            None,
            None,
            &approver,
        )
        .await
        .unwrap();
        mock.assert_hits(1);
        let requests = approver.seen.lock().unwrap();
        assert_eq!(requests.len(), 1, "{:?}", result.messages);
        match &requests[0] {
            crate::approvals::ApprovalRequest::Command { command, cwd } => {
                assert_eq!(command, "grep -q retained fixture.txt");
                assert_eq!(
                    std::path::Path::new(cwd.as_deref().unwrap())
                        .canonicalize()
                        .unwrap(),
                    root.path().canonicalize().unwrap()
                );
            }
            _ => panic!("expected command approval"),
        }
        let output = result
            .messages
            .iter()
            .find(|m| m["tool_call_id"] == "selection")
            .and_then(|m| m["content"].as_str())
            .unwrap();
        assert_eq!(
            crate::execution_evidence::exec_succeeded(output),
            allow,
            "{output}"
        );
        assert_eq!(
            task_harness::benchmark_proof::pending_commands(&result.messages, goal, None)
                .is_empty(),
            allow
        );
    }
}

#[test]
fn selected_check_then_failed_direct_or_alias_check_revokes_current_evidence_review_regression() {
    let root = root();
    for second in [
        json!({"command":"cargo test --locked -q"}),
        json!({"command":"cargo test --locked -q","cwd":"/workspace"}),
        json!({"check_id":"other","command":"cargo test --locked -q","cwd":"/workspace"}),
    ] {
        let mut first = call(json!({"check_id":"required-1"}));
        check_catalog::Catalog::from_root(&root)
            .resolve(&mut first, Some("/workspace"))
            .unwrap();
        let mut next = call(second);
        next.id = "later".into();
        let messages = vec![
            json!({"role":"user","content":root}),
            json!({"role":"assistant","tool_calls":[first.to_json()]}),
            json!({"role":"tool","tool_call_id":"select","content":"OK (exit_code: 0)\ncwd: /workspace"}),
            json!({"role":"assistant","tool_calls":[next.to_json()]}),
            json!({"role":"tool","tool_call_id":"later","content":"FAILED (exit_code: 1)\ncwd: /workspace"}),
        ];
        assert!(acceptance_report::current_commands(&messages, None).is_empty());
        let packet = evidence_packet::project(&messages, None);
        assert_eq!(
            packet.actions[0].freshness,
            current_evidence::Freshness::Superseded
        );
        assert_eq!(packet.pending_exact_checks, ["cargo test --locked -q"]);
    }
}
