use super::*;

fn sse(text: &str, call: Option<serde_json::Value>) -> String {
    let mut delta = json!({"content":text});
    if let Some(call) = call {
        delta["tool_calls"] = json!([call]);
    }
    format!(
        "data: {}\n\ndata: [DONE]\n\n",
        json!({"choices":[{"delta":delta,"finish_reason":"stop"}]})
    )
}

fn advice() -> String {
    json!({"summary":"Inspect the source before repair", "suggestions":[{"kind":"read", "reason":"Locate the defect", "confidence":0.95,"suggested_tool":"read_file","suggested_args":{"path":"engine.rs"}}]}).to_string()
}

async fn episode(
    coder: &MockServer,
    observer: &MockServer,
    cap: usize,
    reserved: usize,
    repair: usize,
) -> serde_json::Value {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("workspace");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("engine.rs"), "fn answer() -> i32 { 0 }\n").unwrap();
    let prompt = dir.path().join("prompt.txt");
    std::fs::write(&prompt, "Repair engine.rs so answer returns 42, preserving all checks. Final report must describe the repair and actual verification.").unwrap();
    let output = dir.path().join("run");
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
    let storage = crate::runtime_storage::RuntimeStorage::create(&root, &output).unwrap();
    let outcome = crate::runtime_storage::scope(
        Some(storage),
        run_owned(
            Args {
                tool_root: root,
                prompt_file: prompt,
                out_dir: output.clone(),
                max_requests: cap,
                observer_reserved_requests: reserved,
                transport_repair_requests: repair,
                seed_session: None,
                without_observer: false,
                observer: roles::ObserverArgs::default(),
                coder_chunk_requests: (cap >= 6).then_some(4),
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
    .await;
    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(output.join("report.json")).unwrap()).unwrap();
    assert_eq!(outcome.is_err(), report["error"].is_string());
    report
}

#[tokio::test]
async fn reservation_drives_eight_observer_interventions_without_coder_borrowing() {
    let coder = MockServer::start();
    let observer = MockServer::start();
    let c = coder.mock(|when, then| {
        when.method(POST).path("/chat/completions");
        then.status(200)
            .header("content-type", "text/event-stream")
            .body(sse("The repair remains unfinished.", None));
    });
    let o = observer.mock(|when, then| {
        when.method(POST).path("/chat/completions");
        then.status(200)
            .header("content-type", "text/event-stream")
            .body(sse(&advice(), None));
    });
    let report = episode(&coder, &observer, 32, 8, 4).await;
    c.assert_hits(24);
    o.assert_hits(8);
    assert_eq!(report["completed"], false);
    assert_eq!(report["termination"], "budget_exhausted");
    for chunk in report["work"]["requests"].as_array().unwrap().chunks(4) {
        assert_eq!(
            chunk
                .iter()
                .map(|r| r["role"].as_str().unwrap())
                .collect::<Vec<_>>(),
            ["coder", "coder", "coder", "observer"]
        );
    }
}

#[tokio::test]
async fn pure_invalid_native_json_uses_four_transport_slots_then_task_budget() {
    let coder = MockServer::start();
    let observer = MockServer::start();
    let c = coder.mock(|when, then| {
        when.method(POST).path("/chat/completions");
        then.status(200).header("content-type", "text/event-stream").body(sse("", Some(json!({"index":0,"id":"bad","type":"function","function":{"name":"exec","arguments":"{"}}))));
    });
    let o = observer.mock(|when, then| {
        when.method(POST).path("/chat/completions");
        then.status(200)
            .header("content-type", "text/event-stream")
            .body(sse(&advice(), None));
    });
    let report = episode(&coder, &observer, 6, 2, 4).await;
    c.assert_hits(8);
    o.assert_hits(2);
    assert_eq!(report["completed"], false);
    let rows = report["work"]["requests"].as_array().unwrap();
    assert_eq!(rows.len(), 10);
    assert_eq!(
        rows.iter().filter(|r| r["charge"] == "transport").count(),
        4
    );
    assert!(report["execution"]["actions"]
        .as_array()
        .unwrap()
        .iter()
        .all(|a| a["executed"] != true));
    assert_eq!(report["execution"]["pending_exact_checks"], json!([]));
}

#[test]
fn semantic_advice_failure_and_local_opener_recovery_are_not_transport_rejections() {
    assert!(suggestion::transport_issue_for_raw("{unfinished").is_some());
    let valid = advice();
    assert!(suggestion::transport_issue_for_raw(&valid[1..]).is_none());
    let mut semantic: serde_json::Value = serde_json::from_str(&valid).unwrap();
    semantic["suggestions"][0]["kind"] = json!("verify");
    assert!(suggestion::validation_issue_for_raw(&semantic.to_string()).is_some());
    assert!(suggestion::transport_issue_for_raw(&semantic.to_string()).is_none());
}

#[tokio::test]
async fn observer_provider_failure_is_an_interrupted_episode_not_a_clean_exit() {
    let coder = MockServer::start();
    let observer = MockServer::start();
    coder.mock(|when, then| {
        when.method(POST).path("/chat/completions");
        then.status(200)
            .header("content-type", "text/event-stream")
            .body(sse("The repair remains unfinished.", None));
    });
    let failed = observer.mock(|when, then| {
        when.method(POST).path("/chat/completions");
        then.status(401).body("offline fixture unavailable");
    });
    let report = episode(&coder, &observer, 8, 2, 4).await;
    failed.assert_hits(1);
    assert_eq!(report["termination"], "interrupted");
    assert_eq!(report["completed"], false);
    assert!(report["error"].as_str().unwrap().contains("Observer"));
    assert!(report["work"]["requests"]
        .as_array()
        .unwrap()
        .iter()
        .all(|r| r["charge"] == "task"));
}

#[tokio::test]
async fn observer_output_limit_stays_in_task_budget_for_empty_and_partial_answers() {
    for content in ["", "{unfinished"] {
        let coder = MockServer::start();
        let observer = MockServer::start();
        coder.mock(|when, then| {
            when.method(POST).path("/chat/completions");
            then.status(200)
                .header("content-type", "text/event-stream")
                .body(sse("The repair remains unfinished.", None));
        });
        let limited = observer.mock(|when, then| {
            when.method(POST).path("/chat/completions");
            let payload =
                json!({"choices":[{"delta":{"content":content},"finish_reason":"length"}]});
            then.status(200)
                .header("content-type", "text/event-stream")
                .body(format!("data: {payload}\n\ndata: [DONE]\n\n"));
        });
        let report = episode(&coder, &observer, 2, 1, 4).await;
        let rows = report["work"]["requests"].as_array().unwrap();
        assert_eq!(rows.len(), 2, "{rows:?}");
        limited.assert_hits(1);
        assert!(rows.iter().all(|r| r["charge"] == "task"));
        assert_eq!(rows[1]["status"], "output_limited");
        assert_eq!(report["termination"], "budget_exhausted");
        assert_eq!(report["completed"], false);
    }
}

#[tokio::test]
async fn final_semantic_rejection_cannot_start_an_unfunded_repair() {
    let coder = MockServer::start();
    let observer = MockServer::start();
    coder.mock(|when, then| {
        when.method(POST).path("/chat/completions");
        then.status(200)
            .header("content-type", "text/event-stream")
            .body(sse("The repair remains unfinished.", None));
    });
    let rejected = observer.mock(|when, then| {
        when.method(POST).path("/chat/completions");
        let mut raw: serde_json::Value = serde_json::from_str(&advice()).unwrap();
        raw["suggestions"][0]["kind"] = json!("verify");
        then.status(200)
            .header("content-type", "text/event-stream")
            .body(sse(&raw.to_string(), None));
    });
    let report = episode(&coder, &observer, 2, 1, 4).await;
    rejected.assert_hits(1);
    assert_eq!(report["termination"], "budget_exhausted");
    assert_eq!(report["error"], serde_json::Value::Null);
    assert_eq!(report["work"]["requests"].as_array().unwrap().len(), 2);
    assert!(report["work"]["requests"]
        .as_array()
        .unwrap()
        .iter()
        .all(|r| r["charge"] == "task"));
}
