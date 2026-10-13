use super::*;

fn call(name: &str, args: serde_json::Value) -> ToolCallData {
    ToolCallData {
        id: "test-call".into(),
        name: name.into(),
        arguments: args.to_string(),
        thought_signature: None,
    }
}

#[test]
fn reflection_identity_distinguishes_revised_edits_on_the_same_file() {
    for (name, before, after) in [
        (
            "patch_file",
            json!({"path":"a.txt","search":"missing","replace":"after"}),
            json!({"path":"a.txt","search":"before","replace":"after"}),
        ),
        (
            "patch_file",
            json!({"path":"a.txt","search":"before","replace":"after1"}),
            json!({"path":"a.txt","search":"before","replace":"after2"}),
        ),
        (
            "write_file",
            json!({"path":"a.txt","content":"after1"}),
            json!({"path":"a.txt","content":"after2"}),
        ),
        (
            "apply_diff",
            json!({"path":"a.txt","diff":"@@ -1 +1 @@\n-missing\n+after\n"}),
            json!({"path":"a.txt","diff":"@@ -1 +1 @@\n-before\n+after\n"}),
        ),
    ] {
        assert_ne!(
            tool_call_action_sig(&call(name, before)),
            tool_call_action_sig(&call(name, after)),
            "{name}: edited payload is a new action"
        );
    }
}

#[test]
fn reflection_identity_preserves_executable_bytes_and_full_paths() {
    for (name, before, after) in [
        (
            "exec",
            json!({"command":"grep -fx 'a  b' File1"}),
            json!({"command":"grep -Fx 'a  b' File1"}),
        ),
        (
            "exec",
            json!({"command":"printf 'a  b'"}),
            json!({"command":"printf 'a b'"}),
        ),
        (
            "exec",
            json!({"command":"printf 1\nprintf 2"}),
            json!({"command":"printf 1\nprintf 3"}),
        ),
        (
            "exec",
            json!({"command":"cargo test","cwd":"crate1"}),
            json!({"command":"cargo test","cwd":"crate2"}),
        ),
        (
            "read_file",
            json!({"path":"File1"}),
            json!({"path":"file1"}),
        ),
        (
            "read_file",
            json!({"path":"file1"}),
            json!({"path":"file2"}),
        ),
        (
            "read_file",
            json!({"path":format!("{}a", "long/".repeat(40))}),
            json!({"path":format!("{}b", "long/".repeat(40))}),
        ),
    ] {
        assert_ne!(
            tool_call_action_sig(&call(name, before)),
            tool_call_action_sig(&call(name, after)),
            "{name}: meaningful bytes must not be normalized away"
        );
    }
}

#[test]
fn reflection_identity_ignores_wire_format_and_unused_fields() {
    let original = call(
        "patch_file",
        json!({"path":"a.txt","search":"before","replace":"after"}),
    );
    let mut retry = original.clone();
    retry.id = "new-call-id".into();
    retry.thought_signature = crate::streaming::provider_metadata::ThoughtSignature::from_call(
        &json!({"extra_content":{"google":{"thought_signature":"new-opaque-signature"}}}),
    )
    .unwrap();
    retry.arguments =
        "{ \"replace\": \"after\", \"unused\": 9, \"search\": \"before\", \"path\": \"a.txt\" }"
            .into();
    assert_eq!(
        tool_call_action_sig(&original),
        tool_call_action_sig(&retry)
    );
}

#[tokio::test]
async fn reflection_gate_allows_a_revised_patch_but_blocks_an_identical_retry() {
    use httpmock::prelude::*;
    fn tool_count(request: &HttpMockRequest) -> usize {
        let body: serde_json::Value =
            serde_json::from_slice(request.body.as_deref().unwrap_or_default()).unwrap();
        body["messages"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|m| m["role"] == "tool")
            .count()
    }
    for revised in [false, true] {
        let server = MockServer::start();
        let plan = "<plan>\ngoal: Fix fixture.txt and verify it\nsteps:\n1. Read fixture.txt\n2. Patch fixture.txt and verify it\nacceptance: fixture.txt contains after\nrisks: wrong edit\nassumptions: fixture.txt exists\n</plan>";
        let reflection = "<reflect>\nlast_outcome: failure\ngoal_delta: same\nwrong_assumption: search text matched the file\nstrategy_change: adjust\nnext_minimal_action: correct the search text and patch fixture.txt\n</reflect>";
        let mut mocks = Vec::new();
        let mut calls = vec![
            (0, "read_file", json!({"path":"fixture.txt"})),
            (
                1,
                "patch_file",
                json!({"path":"fixture.txt","search":"absent","replace":"after"}),
            ),
            (
                2,
                "patch_file",
                json!({"path":"fixture.txt","search":if revised {"before"} else {"absent"},"replace":"after"}),
            ),
        ];
        if revised {
            calls.extend([
                (3, "read_file", json!({"path":"fixture.txt"})),
                (
                    4,
                    "patch_file",
                    json!({"path":"fixture.txt","search":"before","replace":"after"}),
                ),
            ]);
        }
        let turns = calls.len();
        for (index, name, args) in calls {
            let content = format!("{plan}\n{}\n<think>\ngoal: fix fixture.txt\nstep: {}\ntool: {name}\nrisk: wrong match\ndoubt: search may mismatch\nnext: {name} fixture.txt\nverify: inspect tool result\n</think>", if index >= 2 {reflection} else {""}, if name == "read_file" {1} else {2});
            let content = if name == "patch_file" {
                format!("{content}\n<evidence>\ntarget_files: 1) fixture.txt\ntarget_symbols: 1) file content\nevidence: read_file showed before in fixture.txt\nopen_questions: none\nnext_probe: patch_file fixture.txt\n</evidence>")
            } else {
                content
            };
            let response = format!(
                "data: {}\n\ndata: [DONE]\n\n",
                json!({"choices":[{"delta":{"content":content,"tool_calls":[{"index":0,"id":format!("call-{index}"),"function":{"name":name,"arguments":args.to_string()}}]},"finish_reason":"tool_calls"}]})
            );
            mocks.push(server.mock(move |when, then| {
                let matcher: fn(&HttpMockRequest) -> bool = match index {
                    0 => |r| tool_count(r) == 0,
                    1 => |r| tool_count(r) == 1,
                    2 => |r| tool_count(r) == 2,
                    3 => |r| tool_count(r) == 3,
                    _ => |r| tool_count(r) == 4,
                };
                when.method(POST).path("/chat/completions").matches(matcher);
                then.status(200)
                    .header("content-type", "text/event-stream")
                    .body(response);
            }));
        }
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("fixture.txt"), "before\n").unwrap();
        let cfg = RunConfig {
            provider: ProviderKind::OpenAiCompatible,
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
        let start = AgenticStartState {
            messages: vec![
                json!({"role":"user","content":"Fix the existing fixture.txt by replacing before with after, then verify it."}),
            ],
            checkpoint: None,
            cur_cwd: None,
            observation_cache: None,
            session_bridge: None,
            create_checkpoint: false,
        };
        let (tx, mut rx) = mpsc::channel(512);
        let result = run_agentic_json(
            start,
            &cfg,
            root.path().to_str(),
            turns,
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
        for mock in mocks {
            mock.assert_hits(1);
        }
        while let Some(token) = rx.recv().await {
            match token {
                StreamToken::Telemetry(event) if event.event == "pre_tool_gate_rejected" => {
                    panic!("mock response must satisfy the protocol: {:?}", event.data);
                }
                StreamToken::GovernorState(state) => assert!(state.last_verify_ok_step.is_none()),
                _ => {}
            }
        }
        let history = serde_json::to_string(&result.messages).unwrap();
        assert!(history.contains("search string not found"), "{history}");
        assert_eq!(
            history.contains("Reflection/action mismatch blocked"),
            !revised,
            "{history}"
        );
        if revised {
            assert!(
                history.contains("[Recovery Gate] stage=diagnose"),
                "the corrected patch still requires observation of its failed target"
            );
        }
        assert_eq!(
            std::fs::read_to_string(root.path().join("fixture.txt")).unwrap(),
            if revised { "after\n" } else { "before\n" },
            "{history}"
        );
    }
}

#[test]
fn reflection_feedback_offers_parseable_outcomes() {
    let invalid = "<reflect>\nlast_outcome: observed result\ngoal_delta: same\nwrong_assumption: search matched\nstrategy_change: adjust\nnext_minimal_action: reread the target\n</reflect>";
    let error = validate_reflection(
        &parse_reflection_block(invalid).unwrap(),
        &FailureMemory::default(),
        1,
    )
    .unwrap_err()
    .to_string();
    let choices = error
        .lines()
        .find_map(|line| line.strip_prefix("last_outcome: "))
        .unwrap();
    assert_eq!(
        choices.split('|').collect::<Vec<_>>(),
        governor_contract::field_allowed_values("reflect", "last_outcome")
    );
    for choice in choices.split('|') {
        let repaired = parse_reflection_block(&invalid.replace("observed result", choice)).unwrap();
        assert_eq!(repaired.last_outcome, choice);
        validate_reflection(&repaired, &FailureMemory::default(), 1).unwrap();
    }
}
