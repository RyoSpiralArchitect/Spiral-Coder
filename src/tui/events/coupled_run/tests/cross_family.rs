use super::*;

fn sse(content: serde_json::Value) -> String {
    format!("data: {content}\n\ndata: [DONE]\n\n")
}

#[tokio::test]
async fn distinct_role_transports_share_one_cap_and_keep_native_proof() {
    assert_cross_family_run(None, false).await;
}

#[tokio::test]
async fn explicit_observer_output_cap_reaches_initial_repair_and_receipts() {
    assert_cross_family_run(Some(8192), true).await;
}

async fn assert_cross_family_run(observer_max_tokens: Option<u32>, repair: bool) {
    let coder_server = MockServer::start();
    let observer_server = MockServer::start();
    let initial = coder_server.mock(|when, then| {
        when.method(POST).path("/chat/completions")
            .header("authorization", "Bearer coder-secret")
            .body_contains("gpt-5.6-luna")
            .matches(|req| !String::from_utf8_lossy(req.body.as_deref().unwrap_or_default()).contains("Finish from retained native proof now."));
        then.status(200).header("content-type", "text/event-stream")
            .body(sse(json!({"choices":[{"delta":{"content":"The inspection remains unfinished."},"finish_reason":"stop"}]})));
    });
    let done_args = json!({"summary":"inspection complete: note.txt contains retained evidence", "completed_acceptance":["inspection complete"], "remaining_acceptance":[], "acceptance_evidence":[{"criterion":"inspection complete","command":"read_file note.txt","evidence":"retained evidence"}]});
    let effective_observer_max = observer_max_tokens.unwrap_or(1024);
    let initial_observer = repair.then(|| {
        observer_server.mock(|when, then| {
            when.method(POST)
                .path("/chat/completions")
                .header("authorization", "Bearer observer-secret")
                .json_body_partial(
                    json!({"max_tokens":effective_observer_max,"temperature":0.2}).to_string(),
                )
                .matches(|req| {
                    !String::from_utf8_lossy(req.body.as_deref().unwrap_or_default())
                        .contains("[Observer action repair:")
                });
            then.status(200)
                .header("content-type", "text/event-stream")
                .body(sse(
                    json!({"choices":[{"delta":{"content":""},"finish_reason":"length"}]}),
                ));
        })
    });
    let observer = observer_server.mock(|when, then| {
        let when = when.method(POST).path("/chat/completions")
            .header("authorization", "Bearer observer-secret")
            .body_contains("mistral-large-4")
            .body_contains("intervention mode")
            .json_body_partial(json!({"max_tokens":effective_observer_max,"temperature":0.2}).to_string());
        if repair { when.body_contains("[Observer action repair:"); }
        let advice = json!({"summary":"Finish from retained native proof now.",
            "primary_blocker":"missing_verification", "response_contract":{"required":true,"focus_axes":["correctness"]},
            "suggestions":[{"kind":"done","reason":"Finish from retained native proof now.","confidence":0.95,"suggested_tool":"done","suggested_args":done_args,"based_on":["execution_evidence"]}]});
        then.status(200).header("content-type", "text/event-stream")
            .body(sse(json!({"choices":[{"delta":{"content":advice.to_string()},"finish_reason":"stop"}]})));
    });
    let continuation = coder_server.mock(|when, then| {
        when.method(POST).path("/chat/completions")
            .header("authorization", "Bearer coder-secret")
            .body_contains("gpt-5.6-luna")
            .body_contains("retained-read")
            .body_contains("Finish from retained native proof now.");
        let content = "<think>\ngoal: report inspection\nstep: 2\ntool: done\nrisk: none\ndoubt: none\nnext: report retained evidence\nverify: prior native read\n</think>\n[Observer response]\ndecision: accept\nreason: native evidence is already available";
        then.status(200).header("content-type", "text/event-stream")
            .body(sse(json!({"choices":[{"delta":{"content":content,"tool_calls":[{"index":0,"id":"final-report","type":"function","function":{"name":"done","arguments":done_args.to_string()}}]},"finish_reason":"stop"}]})));
    });
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("workspace");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("note.txt"), "retained evidence").unwrap();
    let goal = "Read only: inspect note.txt. Final answer must include `inspection complete`.";
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
            json!({"role":"assistant","accepted_plan":{"goal":"Report inspected note.txt","steps":["Inspect note.txt", "Report retained evidence"],"acceptance_criteria":["inspection complete"],"risks":"none","assumptions":"note.txt was read"},"tool_calls":[{"id":"retained-read","type":"function","function":{"name":"read_file","arguments":"{\"path\":\"note.txt\"}"}}]}),
            json!({"role":"tool","tool_call_id":"retained-read","content":"[note.txt]\nretained evidence"}),
        ],
    );
    crate::agent_session::AgentSession::save_atomic(&seed, &session).unwrap();
    let cli = crate::Cli::try_parse_from([
        "spiral-coder",
        "--provider",
        "openai",
        "--model",
        "gpt-5.6-luna",
        "--base-url",
        &coder_server.base_url(),
        "--api-key",
        "coder-secret",
        "--timeout-seconds",
        "3",
        "--max-tokens",
        "1024",
    ])
    .unwrap();
    let coder = cli.common.to_partial_config().resolve().unwrap();
    let mut obs = coder.clone();
    obs.provider = crate::config::ProviderKind::Mistral;
    obs.model = "mistral-large-4".into();
    obs.base_url = observer_server.base_url();
    obs.api_key = Some("observer-secret".into());
    obs.mode = crate::modes::Mode::Observer;
    let output = directory.path().join("run");
    let storage = crate::runtime_storage::RuntimeStorage::create(&root, &output).unwrap();
    let args = Args {
        tool_root: root.clone(),
        prompt_file: prompt,
        out_dir: output.clone(),
        max_requests: 6 + usize::from(repair),
        seed_session: Some(seed),
        without_observer: false,
        observer: roles::ObserverArgs::default(),
        coder_chunk_requests: Some(4),
        case_spec: None,
        evaluate_only: false,
    };
    tokio::time::timeout(
        std::time::Duration::from_secs(20),
        crate::runtime_storage::scope(
            Some(storage),
            run_owned(
                args,
                roles::ResolvedRoles {
                    coder,
                    observer: Some(obs),
                    observer_max_tokens,
                },
            ),
        ),
    )
    .await
    .unwrap()
    .unwrap();
    initial.assert_hits(4);
    if let Some(first) = initial_observer {
        first.assert_hits(1);
    }
    observer.assert_hits(1);
    continuation.assert_hits(1);
    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(output.join("report.json")).unwrap()).unwrap();
    assert_eq!(report["completed"], true, "{report:#}");
    assert_eq!(report["same_model_both_roles"], false);
    assert_eq!(report["coder_chunk_requests"], 4);
    assert_eq!(
        report["role_requests"],
        json!({"coder":5,"observer":1 + usize::from(repair)})
    );
    assert_eq!(report["roles"]["coder"]["model"], "gpt-5.6-luna");
    assert_eq!(report["roles"]["observer"]["model"], "mistral-large-4");
    assert_eq!(report["roles"]["observer"]["transport"], "mistral");
    assert_eq!(report["roles"]["coder"]["max_tokens"], 1024);
    assert_eq!(
        report["roles"]["observer"]["max_tokens"],
        effective_observer_max
    );
    assert_eq!(report["work"]["request_limit"], 6 + usize::from(repair));
    assert_eq!(
        report["work"]["requests"].as_array().unwrap().len(),
        6 + usize::from(repair)
    );
    let trace = std::fs::read_to_string(output.join("trace.jsonl")).unwrap();
    let config: serde_json::Value = trace
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .find(|event| event["event"] == "agent_config")
        .unwrap();
    assert_eq!(config["data"]["roles"], report["roles"]);
    assert!(!report.to_string().contains("coder-secret"));
    assert!(!report.to_string().contains("observer-secret"));
    assert!(!root.join(".spiral-coder").exists());
}

#[tokio::test]
async fn impossible_chunk_reservations_fail_before_network_or_storage() {
    let server = MockServer::start();
    let wire = server.mock(|when, then| {
        when.method(POST);
        then.status(500);
    });
    let directory = tempfile::tempdir().unwrap();
    for (limit, cap, disabled) in [(3, 6, false), (5, 6, false), (4, 5, false), (4, 6, true)] {
        let cli = crate::Cli::try_parse_from([
            "spiral-coder",
            "--provider",
            "openai",
            "--model",
            "offline",
            "--base-url",
            &server.base_url(),
            "--api-key",
            "offline",
        ])
        .unwrap();
        let output = directory.path().join(format!("{limit}-{cap}-{disabled}"));
        let args = Args {
            tool_root: directory.path().join("missing-workspace"),
            prompt_file: directory.path().join("missing-prompt"),
            out_dir: output.clone(),
            max_requests: cap,
            seed_session: None,
            without_observer: disabled,
            observer: roles::ObserverArgs::default(),
            coder_chunk_requests: Some(limit),
            case_spec: None,
            evaluate_only: false,
        };
        let error = run(args, cli.common).await.unwrap_err().to_string();
        assert!(error.contains("coder-chunk-requests"), "{error}");
        assert!(!output.exists());
    }
    wire.assert_hits(0);
}

#[tokio::test]
async fn a_compatibility_retry_cannot_spend_the_reserved_observer_requests() {
    let server = MockServer::start();
    let wire = server.mock(|when, then| {
        when.method(POST).path("/chat/completions");
        then.status(400)
            .body("Unsupported parameter: 'max_tokens'. Use 'max_completion_tokens' instead.");
    });
    let cli = crate::Cli::try_parse_from([
        "spiral-coder",
        "--provider",
        "openai",
        "--model",
        "offline",
        "--base-url",
        &server.base_url(),
        "--api-key",
        "offline",
    ])
    .unwrap();
    let cfg = cli.common.to_partial_config().resolve().unwrap();
    let budget = crate::work_budget::WorkBudget::new("task", 6, None);
    let chunk = budget.with_request_allowance(4);
    for _ in 0..3 {
        drop(chunk.begin("coder", &json!({})).unwrap());
    }
    let (tx, _rx) = mpsc::channel(16);
    let result = crate::work_budget::scope(
        Some(crate::work_budget::Context {
            budget: chunk,
            role: "coder",
        }),
        crate::streaming::stream_openai_compat_json(&reqwest::Client::new(), &cfg, &[], None, tx),
    )
    .await;
    assert!(result
        .err()
        .unwrap()
        .is::<crate::work_budget::ChunkExhausted>());
    wire.assert_hits(1);
    assert_eq!(budget.remaining(), 2);
    drop(budget.begin("observer", &json!({})).unwrap());
    drop(budget.begin("coder", &json!({})).unwrap());
    assert_eq!(budget.remaining(), 0);
}
