use super::*;
use crate::tui::events::tests::{closeout_args, closeout_messages};

fn sse(content: serde_json::Value) -> String {
    format!("data: {content}\n\ndata: [DONE]\n\n")
}

#[tokio::test]
async fn headless_output_limit_handoff_uses_the_same_one_report_consumer() {
    for valid in [true, false] {
        headless_case(valid).await;
    }
}

async fn headless_case(valid: bool) {
    let coder = MockServer::start();
    let observer = MockServer::start();
    let initial = coder.mock(|when, then| {
        when.method(POST).path("/chat/completions").matches(|r| !String::from_utf8_lossy(r.body.as_deref().unwrap_or_default()).contains("one report-only request"));
        then.status(200).header("content-type", "text/event-stream").body(sse(json!({"choices":[{"delta":{"content":"The final authored report remains."},"finish_reason":"stop"}]})));
    });
    let mut done_args = closeout_args();
    if !valid {
        done_args["acceptance_evidence"]
            .as_array_mut()
            .unwrap()
            .truncate(2);
    }
    let report_call = coder.mock(|when, then| {
        when.method(POST).path("/chat/completions").body_contains("one report-only request").matches(|r| {
            let v: serde_json::Value=serde_json::from_slice(r.body.as_deref().unwrap()).unwrap();
            v["tools"].as_array().is_some_and(|t| t.len()==1 && t[0]["function"]["name"]=="done")
        });
        then.status(200).header("content-type", "text/event-stream").body(sse(json!({"choices":[{"delta":{"content":"<impact>\nchanged: source.py and docs/usage.md repaired\nprogress: step 2 exact check passed\nremaining_gap: final report\n</impact>\n<think>\ngoal: Report current proof\nstep: 2\ntool: done\nrisk: inaccurate claims\ndoubt: none\nnext: finish report\nverify: retained native check\n</think>","tool_calls":[{"index":0,"id":"closeout","function":{"name":"done","arguments":done_args.to_string()}}]},"finish_reason":"stop"}]})));
    });
    let invalid = observer.mock(|when, then| {
        when.method(POST).path("/chat/completions").matches(|r| !String::from_utf8_lossy(r.body.as_deref().unwrap_or_default()).contains("Observer action repair: attempt"));
        let mut args=closeout_args();
        args["acceptance_evidence"].as_array_mut().unwrap().truncate(2);
        let raw=json!({"summary":"Report retained proof","suggestions":[{"kind":"done","confidence":0.95,"suggested_tool":"done","suggested_args":args}]}).to_string();
        then.status(200).header("content-type", "text/event-stream").body(sse(json!({"choices":[{"delta":{"content":raw},"finish_reason":"stop"}]})));
    });
    let limited = observer.mock(|when, then| {
        when.method(POST)
            .path("/chat/completions")
            .body_contains("Observer action repair: attempt");
        then.status(200)
            .header("content-type", "text/event-stream")
            .body(sse(
                json!({"choices":[{"delta":{"content":""},"finish_reason":"length"}]}),
            ));
    });
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("workspace");
    std::fs::create_dir(&root).unwrap();
    let messages = closeout_messages();
    let prompt = dir.path().join("prompt.txt");
    std::fs::write(&prompt, messages[0]["content"].as_str().unwrap()).unwrap();
    let seed = dir.path().join("seed.json");
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
                transport_repair_requests: 4,
                seed_session: Some(seed),
                without_observer: false,
                observer: roles::ObserverArgs::default(),
                coder_chunk_requests: Some(1),
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
    initial.assert_hits(1);
    report_call.assert_hits(1);
    invalid.assert_hits(1);
    limited.assert_hits(1);
    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(output.join("report.json")).unwrap()).unwrap();
    assert_eq!(report["completed"], valid, "{report}");
    assert_eq!(
        report["termination"],
        if valid {
            "completed"
        } else {
            "report_incomplete"
        }
    );
    assert_eq!(report["report_closeout_attempted"], true);
    assert_eq!(report["role_requests"], json!({"coder":2,"observer":2}));
    assert_eq!(report["work"]["requests"][3]["purpose"], "report_closeout");
    assert!(
        report["work"]["assists"].as_array().unwrap().is_empty(),
        "rejected Observer advice is not an accepted assist"
    );
}
