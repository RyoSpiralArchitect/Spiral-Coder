use super::*;

fn sse(content: serde_json::Value) -> String {
    format!("data: {content}\n\ndata: [DONE]\n\n")
}

fn action(id: &str, tool: &str, args: serde_json::Value, reflect: bool) -> String {
    let reflection = if reflect {
        "<reflect>\nlast_outcome: failure\ngoal_delta: same\nwrong_assumption: the check would pass\nstrategy_change: adjust\nnext_minimal_action: read test_failure.py at the failed assertion\n</reflect>\n"
    } else {
        ""
    };
    let content = format!("{reflection}<think>\ngoal: diagnose the failing test\nstep: 1\ntool: {tool}\nrisk: none\ndoubt: failure remains unresolved\nnext: inspect the exact failed assertion\nverify: python3 -m unittest -q\n</think>\n[Observer response]\ndecision: accept\nreason: follow the proposed diagnostic step");
    sse(
        json!({"choices":[{"delta":{"content":content,"tool_calls":[{"index":0,"id":id,"type":"function","function":{"name":tool,"arguments":args.to_string()}}]},"finish_reason":"stop"}]}),
    )
}

#[tokio::test]
async fn later_verification_failure_returns_to_observer_before_another_coder_request() {
    let coder_server = MockServer::start();
    let observer_server = MockServer::start();
    let initial = coder_server.mock(|when, then| {
        when.method(POST).path("/chat/completions").matches(|req| {
            let body = String::from_utf8_lossy(req.body.as_deref().unwrap_or_default());
            !body.contains("Perform the first exact check.") && !body.contains("Read the actual failure location.")
        });
        then.status(200).header("content-type", "text/event-stream")
            .body(sse(json!({"choices":[{"delta":{"content":"The investigation remains unfinished."},"finish_reason":"stop"}]})));
    });
    let verification = coder_server.mock(|when, then| {
        when.method(POST)
            .path("/chat/completions")
            .body_contains("Perform the first exact check.")
            .matches(|req| {
                !String::from_utf8_lossy(req.body.as_deref().unwrap_or_default())
                    .contains("failed-check")
            });
        then.status(200)
            .header("content-type", "text/event-stream")
            .body(action(
                "failed-check",
                "exec",
                json!({"command":"python3 -m unittest -q"}),
                false,
            ));
    });
    let skipped_handoff = coder_server.mock(|when, then| {
        when.method(POST)
            .path("/chat/completions")
            .body_contains("failed-check")
            .matches(|req| {
                !String::from_utf8_lossy(req.body.as_deref().unwrap_or_default())
                    .contains("Read the actual failure location.")
            });
        then.status(500)
            .body("Coder requested again before Observer saw the new failure");
    });
    let diagnostic = coder_server.mock(|when, then| {
        when.method(POST)
            .path("/chat/completions")
            .body_contains("Read the actual failure location.")
            .matches(|req| {
                let body: serde_json::Value =
                    serde_json::from_slice(req.body.as_deref().unwrap()).unwrap();
                let tools = body["tools"].as_array().unwrap();
                !body.to_string().contains("current-diagnostic-read")
                    && tools.len() == 2
                    && tools.iter().any(|t| t["function"]["name"] == "done")
                    && tools.iter().any(|t| {
                        t["function"]["name"] == "read_file"
                            && t["function"]["parameters"]["properties"]["path"]["enum"]
                                == json!(["test_failure.py"])
                    })
            });
        then.status(200)
            .header("content-type", "text/event-stream")
            .body(action(
                "current-diagnostic-read",
                "read_file",
                json!({"path":"test_failure.py"}),
                true,
            ));
    });
    let report = coder_server.mock(|when, then| {
        when.method(POST).path("/chat/completions").body_contains("current-diagnostic-read");
        then.status(200).header("content-type", "text/event-stream")
            .body(action("incomplete-report", "done", json!({"summary":"unfixed: test_failure.py still fails", "completed_acceptance":[], "remaining_acceptance":["tests pass"],"acceptance_evidence":[]}), false));
    });
    let advice = |text: &str, kind: &str, tool: &str, args: serde_json::Value| {
        json!({"summary":text,"response_contract":{"required":true,"focus_axes":["correctness"]},
            "suggestions":[{"kind":kind,"reason":text,"confidence":0.95,"suggested_tool":tool,"suggested_args":args}]}).to_string()
    };
    let first_observer = observer_server.mock(|when, then| {
        when.method(POST).path("/chat/completions").matches(|req| {
            !String::from_utf8_lossy(req.body.as_deref().unwrap_or_default())
                .contains("failed-check")
        });
        let raw = advice(
            "Perform the first exact check.",
            "verify",
            "exec",
            json!({"command":"python3 -m unittest -q"}),
        );
        then.status(200)
            .header("content-type", "text/event-stream")
            .body(sse(
                json!({"choices":[{"delta":{"content":raw},"finish_reason":"stop"}]}),
            ));
    });
    let second_observer = observer_server.mock(|when, then| {
        when.method(POST)
            .path("/chat/completions")
            .body_contains("failed-check");
        // Exercise opener recovery on the real consumer without another wire call.
        let raw = advice(
            "Read the actual failure location.",
            "read",
            "read_file",
            json!({"path":"test_failure.py"}),
        );
        then.status(200)
            .header("content-type", "text/event-stream")
            .body(sse(
                json!({"choices":[{"delta":{"content":&raw[1..]},"finish_reason":"stop"}]}),
            ));
    });
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("workspace");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("test_failure.py"), "import unittest\nclass TestFailure(unittest.TestCase):\n    def test_fail(self):\n        self.assertEqual(1, 2)\n").unwrap();
    let goal = "Diagnose and fix the failing test_failure.py.\nRun exactly: python3 -m unittest -q\nFinal answer must include unfixed if the failure remains.";
    let prompt = directory.path().join("task.txt");
    std::fs::write(&prompt, goal).unwrap();
    let seed = directory.path().join("seed.json");
    let session = crate::agent_session::AgentSession::new(
        None,
        None,
        None,
        None,
        vec![
            crate::task_origin::user_message(goal, crate::task_origin::MessageOrigin::User),
            json!({"role":"assistant","accepted_plan":{"goal":"Diagnose the failing test","steps":["Inspect test_failure.py","Run the exact test","Repair and report"],"acceptance_criteria":["tests pass"],"risks":"preserve test coverage","assumptions":"test exists"}}),
        ],
    );
    crate::agent_session::AgentSession::save_atomic(&seed, &session).unwrap();
    let cli = crate::Cli::try_parse_from([
        "spiral-coder",
        "--provider",
        "openai",
        "--model",
        "offline",
        "--base-url",
        &coder_server.base_url(),
        "--api-key",
        "offline",
        "--timeout-seconds",
        "3",
    ])
    .unwrap();
    let coder = cli.common.to_partial_config().resolve().unwrap();
    let mut observer = coder.clone();
    observer.base_url = observer_server.base_url();
    observer.mode = crate::modes::Mode::Observer;
    let output = directory.path().join("run");
    let storage = crate::runtime_storage::RuntimeStorage::create(&root, &output).unwrap();
    let args = Args {
        tool_root: root,
        prompt_file: prompt,
        out_dir: output.clone(),
        max_requests: 9,
        transport_repair_requests: 0,
        observer_reserved_requests: 0,
        seed_session: Some(seed),
        without_observer: false,
        observer: roles::ObserverArgs::default(),
        coder_chunk_requests: Some(4),
        case_spec: None,
        evaluate_only: false,
    };
    tokio::time::timeout(
        std::time::Duration::from_secs(30),
        crate::runtime_storage::scope(
            Some(storage),
            run_owned(
                args,
                roles::ResolvedRoles {
                    coder,
                    observer: Some(observer),
                    observer_max_tokens: None,
                },
            ),
        ),
    )
    .await
    .unwrap()
    .unwrap();
    let retained: serde_json::Value =
        serde_json::from_slice(&std::fs::read(output.join("report.json")).unwrap()).unwrap();
    initial.assert_hits(4);
    verification.assert_hits(1);
    skipped_handoff.assert_hits(0);
    first_observer.assert_hits(1);
    second_observer.assert_hits(1);
    diagnostic.assert_hits(1);
    report.assert_hits(1);
    assert_eq!(retained["completed"], false);
    assert_eq!(retained["role_requests"], json!({"coder":7,"observer":2}));
    let order: Vec<_> = retained["work"]["requests"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["role"].as_str().unwrap())
        .collect();
    assert_eq!(
        order,
        ["coder", "coder", "coder", "coder", "observer", "coder", "observer", "coder", "coder"]
    );
    assert!(retained["execution"]["pending_exact_checks"]
        .as_array()
        .unwrap()
        .contains(&json!("python3 -m unittest -q")));
    let trace = std::fs::read_to_string(output.join("trace.jsonl")).unwrap();
    assert!(trace.contains("observer_verification_handoff"));
    assert!(retained["observer_display"]
        .as_array()
        .unwrap()
        .iter()
        .any(|m| m["raw_content"]
            .as_str()
            .is_some_and(|raw| raw.starts_with('"'))));
    assert!(retained["observer_display"]
        .to_string()
        .contains("OpeningBrace"));
}
