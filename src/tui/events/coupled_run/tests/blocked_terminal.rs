use super::*;

async fn blocked_case(seed_plan: bool) {
    let coder = MockServer::start();
    let observer = MockServer::start();
    let done = coder.mock(|when, then| {
        when.method(POST).path("/chat/completions");
        let args = json!({"summary":"The exact test still fails; source diagnosis is unfinished.", "completed_acceptance":[], "remaining_acceptance":["acceptance 1"], "acceptance_evidence":[], "next_steps":"Read test_failure.py at the failed assertion."});
        let payload = json!({"choices":[{"delta":{"content":"<plan>\ngoal: Repair the failure\nsteps:\n1. Read failure\n2. Repair source\n3. Verify and report\nacceptance:\n1. tests pass\nrisks: preserve coverage\nassumptions: test exists\n</plan>","tool_calls":[{"index":0,"id":"incomplete","type":"function","function":{"name":"done","arguments":args.to_string()}}]},"finish_reason":"stop"}]});
        then.status(200).header("content-type", "text/event-stream").body(format!("data: {payload}\n\ndata: [DONE]\n\n"));
    });
    let diagnosis = observer.mock(|when, then| {
        when.method(POST).path("/chat/completions").body_contains("[Blocked terminal diagnosis]");
        let raw = json!({"summary":"The failing test has not been read; the repair has no diagnostic evidence yet.", "suggestions":[{"kind":"clarify","reason":"Read the exact failed assertion next.","confidence":0.8,"suggested_args":{}}]}).to_string();
        let payload = json!({"choices":[{"delta":{"content":raw},"finish_reason":"stop"}]});
        then.status(200).header("content-type", "text/event-stream").body(format!("data: {payload}\n\ndata: [DONE]\n\n"));
    });
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("workspace");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("test_failure.py"), "assert 1 == 2\n").unwrap();
    let goal = "Repair test_failure.py.\nRun exactly: python3 -m unittest -q\nFinal answer must describe unfinished work.";
    let prompt = dir.path().join("prompt.txt");
    std::fs::write(&prompt, goal).unwrap();
    let seed = dir.path().join("seed.json");
    let mut messages = vec![
        crate::task_origin::user_message(goal, crate::task_origin::MessageOrigin::User),
        json!({"role":"assistant","accepted_plan":{"goal":"Repair the failure","steps":["Read failure","Repair source","Verify and report"],"acceptance_criteria":["tests pass"],"risks":"preserve coverage","assumptions":"test exists"}}),
        json!({"role":"assistant","tool_calls":[{"id":"failed-check","type":"function","function":{"name":"exec","arguments":"{\"command\":\"python3 -m unittest -q\"}"}}]}),
        json!({"role":"tool","tool_call_id":"failed-check","content":format!("FAILED (exit_code: 1)\ncwd: {}\nstderr:\n  File \"test_failure.py\", line 1\nAssertionError: 1 != 2", root.display())}),
    ];
    if !seed_plan {
        messages.truncate(1);
    }
    crate::agent_session::AgentSession::save_atomic(
        &seed,
        &crate::agent_session::AgentSession::new(None, None, None, None, messages),
    )
    .unwrap();
    let cli = crate::Cli::try_parse_from([
        "spiral-coder",
        "--provider",
        "openai",
        "--model",
        "offline",
        "--base-url",
        &coder.base_url(),
        "--api-key",
        "offline",
        "--timeout-seconds",
        "2",
    ])
    .unwrap();
    let cfg = cli.common.to_partial_config().resolve().unwrap();
    let mut obs = cfg.clone();
    obs.base_url = observer.base_url();
    obs.mode = crate::modes::Mode::Observer;
    let output = dir.path().join("run");
    let storage = crate::runtime_storage::RuntimeStorage::create(&root, &output).unwrap();
    crate::runtime_storage::scope(
        Some(storage),
        run_owned(
            Args {
                tool_root: root,
                prompt_file: prompt,
                out_dir: output.clone(),
                max_requests: 10,
                observer_reserved_requests: 3,
                transport_repair_requests: 2,
                iterations: 1,
                seed_session: Some(seed),
                follow_up_session: None,
                without_observer: false,
                observer: roles::ObserverArgs::default(),
                coder_chunk_requests: Some(4),
                case_spec: None,
                evaluate_only: false,
            },
            roles::ResolvedRoles {
                coder: cfg,
                observer: Some(obs),
                observer_max_tokens: None,
            },
        ),
    )
    .await
    .unwrap();
    done.assert_hits(1);
    diagnosis.assert_hits(1);
    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(output.join("report.json")).unwrap()).unwrap();
    assert_eq!(report["completed"], false);
    assert_eq!(report["termination"], "blocked");
    assert_eq!(report["role_requests"], json!({"coder":1,"observer":1}));
    assert!(report["execution"]["pending_exact_checks"]
        .as_array()
        .unwrap()
        .contains(&json!("python3 -m unittest -q")));
    assert!(report["execution"]["actions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|a| a["result"] == "OK: blocked"));
    assert!(!report["execution"]["actions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|a| a["result"] == "OK: done"));
    let session: serde_json::Value =
        serde_json::from_slice(&std::fs::read(output.join("session.json")).unwrap()).unwrap();
    assert!(
        crate::task_origin::provider_messages(session["messages"].as_array().unwrap())
            .iter()
            .all(|m| m.get("blocked_report").is_none())
    );
    assert!(session["messages"]
        .as_array()
        .unwrap()
        .iter()
        .any(|m| m.get("blocked_report").is_some()));
}

#[tokio::test]
async fn incomplete_done_preserves_failure_and_gets_one_observer_diagnosis() {
    blocked_case(true).await;
}

#[tokio::test]
async fn first_turn_blocked_report_can_validate_its_own_plan() {
    blocked_case(false).await;
}

/// The budget here could pay for a second return; only the once-per-round rule
/// stops it.
#[tokio::test]
async fn a_chain_returns_one_blocked_report_per_round_before_stopping() {
    let coder = MockServer::start();
    let observer = MockServer::start();
    let report = |id: &str| {
        let args = json!({"summary":"The exact test still fails; source diagnosis is unfinished.", "completed_acceptance":[], "remaining_acceptance":["acceptance 1"], "acceptance_evidence":[], "next_steps":"Read test_failure.py at the failed assertion."});
        let payload = json!({"choices":[{"delta":{"content":"<plan>\ngoal: Repair the failure\nsteps:\n1. Read failure\n2. Repair source\n3. Verify and report\nacceptance:\n1. tests pass\nrisks: preserve coverage\nassumptions: test exists\n</plan>","tool_calls":[{"index":0,"id":id,"type":"function","function":{"name":"done","arguments":args.to_string()}}]},"finish_reason":"stop"}]});
        format!("data: {payload}\n\ndata: [DONE]\n\n")
    };
    let first = coder.mock(|when, then| {
        when.method(POST).path("/chat/completions").matches(|r| {
            !String::from_utf8_lossy(r.body.as_deref().unwrap_or_default())
                .contains("[Blocked report returned]")
        });
        then.status(200)
            .header("content-type", "text/event-stream")
            .body(report("first-report"));
    });
    let returned = coder.mock(|when, then| {
        when.method(POST)
            .path("/chat/completions")
            .body_contains("[Blocked report returned]")
            .body_contains("Read test_failure.py at the failed assertion.");
        then.status(200)
            .header("content-type", "text/event-stream")
            .body(report("second-report"));
    });
    let diagnosis = observer.mock(|when, then| {
        when.method(POST).path("/chat/completions").body_contains("[Blocked terminal diagnosis]");
        let raw = json!({"summary":"The failing test has not been read.", "suggestions":[{"kind":"clarify","reason":"Read the exact failed assertion next.","confidence":0.8,"suggested_args":{}}]}).to_string();
        let payload = json!({"choices":[{"delta":{"content":raw},"finish_reason":"stop"}]});
        then.status(200).header("content-type", "text/event-stream").body(format!("data: {payload}\n\ndata: [DONE]\n\n"));
    });
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("workspace");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("test_failure.py"), "assert 1 == 2\n").unwrap();
    let goal = "Repair test_failure.py.\nRun exactly: python3 -m unittest -q\nFinal answer must describe unfinished work.";
    let prompt = dir.path().join("prompt.txt");
    std::fs::write(&prompt, goal).unwrap();
    let seed = dir.path().join("seed.json");
    crate::agent_session::AgentSession::save_atomic(
        &seed,
        &crate::agent_session::AgentSession::new(None, None, None, None, vec![
            crate::task_origin::user_message(goal, crate::task_origin::MessageOrigin::User),
            json!({"role":"assistant","accepted_plan":{"goal":"Repair the failure","steps":["Read failure","Repair source","Verify and report"],"acceptance_criteria":["tests pass"],"risks":"preserve coverage","assumptions":"test exists"}}),
            json!({"role":"assistant","tool_calls":[{"id":"failed-check","type":"function","function":{"name":"exec","arguments":"{\"command\":\"python3 -m unittest -q\"}"}}]}),
            json!({"role":"tool","tool_call_id":"failed-check","content":format!("FAILED (exit_code: 1)\ncwd: {}\nstderr:\n  File \"test_failure.py\", line 1\nAssertionError: 1 != 2", root.display())}),
        ]),
    )
    .unwrap();
    let cli = crate::Cli::try_parse_from([
        "spiral-coder",
        "--provider",
        "openai",
        "--model",
        "offline",
        "--base-url",
        &coder.base_url(),
        "--api-key",
        "offline",
        "--timeout-seconds",
        "2",
    ])
    .unwrap();
    let cfg = cli.common.to_partial_config().resolve().unwrap();
    let mut obs = cfg.clone();
    obs.base_url = observer.base_url();
    obs.mode = crate::modes::Mode::Observer;
    let output = dir.path().join("chain");
    std::fs::create_dir_all(&output).unwrap();
    super::super::iterations::run(
        Args {
            tool_root: root,
            prompt_file: prompt,
            out_dir: output.clone(),
            max_requests: 10,
            observer_reserved_requests: 3,
            transport_repair_requests: 2,
            iterations: 2,
            seed_session: Some(seed),
            follow_up_session: None,
            without_observer: false,
            observer: roles::ObserverArgs::default(),
            coder_chunk_requests: Some(4),
            case_spec: None,
            evaluate_only: false,
        },
        roles::ResolvedRoles {
            coder: cfg,
            observer: Some(obs),
            observer_max_tokens: None,
        },
    )
    .await
    .unwrap();
    first.assert_hits(1);
    returned.assert_hits(1);
    diagnosis.assert_hits(2);
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(output.join("iterations.json")).unwrap()).unwrap();
    assert_eq!(manifest["stop_reason"], "no_new_edit_or_verification");
    assert_eq!(manifest["iterations"].as_array().unwrap().len(), 1);
    assert_eq!(manifest["iterations"][0]["termination"], "blocked");
    assert_eq!(manifest["iterations"][0]["task_requests"], 4);
    let trace = std::fs::read_to_string(output.join("iteration-01/trace.jsonl")).unwrap();
    assert_eq!(trace.matches("blocked_report_returned_work").count(), 1);
}
