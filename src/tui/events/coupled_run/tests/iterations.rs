use super::*;
use crate::agent_session::AgentSession;
use serde_json::Value;

fn response(tool: &str, id: &str, args: Value, text: &str) -> String {
    let mut delta = json!({"content":text});
    if !tool.is_empty() {
        delta["tool_calls"] = json!([{"index":0,"id":id,"type":"function","function":{"name":tool,"arguments":args.to_string()}}]);
    }
    format!(
        "data: {}\n\ndata: [DONE]\n\n",
        json!({"choices":[{"delta":delta,"finish_reason":"stop"}]})
    )
}

#[test]
fn iteration_cli_keeps_a_finite_explicit_upper_bound() {
    for value in ["0", "1", "4", "6", "8", "9", "255"] {
        let parsed = crate::Cli::try_parse_from([
            "spiral-coder",
            "coupled-run",
            "--tool-root",
            ".",
            "--prompt-file",
            "task.txt",
            "--out-dir",
            "run",
            "--iterations",
            value,
        ]);
        assert_eq!(
            parsed.is_ok(),
            matches!(value, "1" | "4" | "6" | "8"),
            "{value}"
        );
    }
}

#[tokio::test]
async fn a_new_iteration_keeps_the_edit_and_validated_observer_note_but_owns_its_budget() {
    iteration_case(FirstRound::Exhausted).await;
}

#[tokio::test]
async fn a_progressed_early_blocked_round_resumes_and_no_progress_still_stops() {
    iteration_case(FirstRound::Blocked).await;
}

#[derive(Clone, Copy)]
enum FirstRound {
    Exhausted,
    Blocked,
}

async fn iteration_case(first: FirstRound) {
    let server = MockServer::start();
    let blocked_args = json!({"summary":"The greeting was edited, but independent verification remains unfinished.","completed_acceptance":[],"remaining_acceptance":["acceptance 1"],"acceptance_evidence":[],"next_steps":"Inspect guide.md and verify the greeting before reporting completion."});
    let blocked_text = "<impact>\nchanged: guide.md greeting\nprogress: edit recorded\nremaining_gap: verification\n</impact>\n<think>\ngoal: Update the greeting in guide.md\nstep: 2\ntool: done\nrisk: inaccurate claims\ndoubt: verification unfinished\nnext: hand off verification\nverify: inspect guide.md\n</think>";
    let coder_first = server.mock(|when,then| {
        when.method(POST).path("/chat/completions").matches(|r| {
            let v:Value=serde_json::from_slice(r.body.as_deref().unwrap_or_default()).unwrap();
            v["tools"].is_array() && !v.to_string().contains("observed-edit") && !v.to_string().contains("carry-greeting-forward")
        });
        then.status(200).header("content-type","text/event-stream").body(response("patch_file","observed-edit",json!({"path":"guide.md","search":"old","replace":"hello"}),"<evidence>\ntarget_files: 1) guide.md\ntarget_symbols: 1) greeting\nevidence: The retained read shows old in guide.md.\nopen_questions: none\nnext_probe: patch guide.md\n</evidence>\n<think>\ngoal: Update the greeting in guide.md\nstep: 1\ntool: patch_file\nrisk: stale anchor\ndoubt: none\nnext: replace the old greeting\nverify: inspect guide.md\n</think>"));
    });
    let early_blocked = server.mock(|when, then| {
        when.method(POST).path("/chat/completions").matches(|r| {
            let v: Value = serde_json::from_slice(r.body.as_deref().unwrap_or_default()).unwrap();
            v["tools"].is_array()
                && v.to_string().contains("observed-edit")
                && !v.to_string().contains("carry-greeting-forward")
        });
        then.status(200)
            .header("content-type", "text/event-stream")
            .body(response(
                "done",
                "early-blocked",
                blocked_args.clone(),
                blocked_text,
            ));
    });
    let resumed = server.mock(|when, then| {
        when.method(POST).path("/chat/completions").matches(|r| {
            let v: Value = serde_json::from_slice(r.body.as_deref().unwrap_or_default()).unwrap();
            v["tools"].is_array()
                && v.to_string().contains("observed-edit")
                && v.to_string().contains("carry-greeting-forward")
        });
        then.status(200)
            .header("content-type", "text/event-stream")
            .body(match first {
                FirstRound::Exhausted => {
                    response("", "", json!({}), "The current proof is incomplete.")
                }
                FirstRound::Blocked => {
                    response("done", "still-blocked", blocked_args.clone(), blocked_text)
                }
            });
    });
    let observer=server.mock(|when,then| {
        when.method(POST).path("/chat/completions").matches(|r| {
            let v:Value=serde_json::from_slice(r.body.as_deref().unwrap_or_default()).unwrap();
            !v["tools"].is_array()
        });
        then.status(200).header("content-type","text/event-stream").body(response("","",json!({}),
            &json!({"summary":"The edit was recorded.","suggestions":[{"kind":"clarify","suggested_tool":null,"suggested_args":{},"reason":"carry-greeting-forward","confidence":0.9}]}).to_string()));
    });
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("project");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("guide.md"), "old\n").unwrap();
    let task = "Update the greeting in guide.md from old to hello.";
    let prompt = dir.path().join("task.txt");
    std::fs::write(&prompt, task).unwrap();
    let seed = dir.path().join("seed.json");
    AgentSession::save_atomic(&seed,&AgentSession::new(None,None,None,None,vec![
        json!({"role":"user","content":task}),json!({"role":"assistant","accepted_plan":{"goal":"Update the greeting in guide.md","steps":["Edit the greeting","Report the result"],"acceptance_criteria":["guide.md greets the reader"],"risks":"incorrect anchor","assumptions":"guide.md exists"},"tool_calls":[{"id":"prior-read","type":"function","function":{"name":"read_file","arguments":r#"{"path":"guide.md"}"#}}]}),
        json!({"role":"tool","tool_call_id":"prior-read","content":"[guide.md] (1 lines, 4 bytes)\nold\n"})])).unwrap();
    let output = dir.path().join("chain");
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
        "--timeout-seconds",
        "3",
        "coupled-run",
        "--tool-root",
        root.to_str().unwrap(),
        "--prompt-file",
        prompt.to_str().unwrap(),
        "--seed-session",
        seed.to_str().unwrap(),
        "--out-dir",
        output.to_str().unwrap(),
        "--max-requests",
        match first {
            FirstRound::Exhausted => "2",
            FirstRound::Blocked => "5",
        },
        "--observer-reserved-requests",
        "1",
        "--iterations",
        "6",
    ])
    .unwrap();
    let Some(crate::Command::CoupledRun(args)) = cli.command else {
        panic!("coupled-run")
    };
    let result = run(args, cli.common).await;
    let manifest: Value =
        serde_json::from_slice(&std::fs::read(output.join("iterations.json")).unwrap()).unwrap();
    assert!(result.is_ok(), "{result:?} {manifest}");
    assert_eq!(
        manifest["iterations"].as_array().unwrap().len(),
        2,
        "{manifest}"
    );
    assert_eq!(manifest["stop_reason"], "no_new_edit_or_verification");
    assert_eq!(manifest["max_iterations"], 6);
    let (limit, spent) = match first {
        FirstRound::Exhausted => (12, 4),
        FirstRound::Blocked => (30, 5),
    };
    assert_eq!(manifest["max_task_requests"], limit);
    assert_eq!(manifest["max_wire_requests"], limit);
    assert_eq!(manifest["wire_requests"], spent);
    if matches!(first, FirstRound::Blocked) {
        assert_eq!(manifest["iterations"][0]["termination"], "blocked");
        assert_eq!(manifest["iterations"][0]["task_requests"], 3);
        assert_eq!(manifest["completed"], false);
        early_blocked.assert_hits(1);
    } else {
        early_blocked.assert_hits(0);
    }
    assert_eq!(
        manifest["iterations"][1]["parent_task_id"],
        manifest["iterations"][0]["task_id"]
    );
    assert_ne!(
        manifest["iterations"][1]["task_id"],
        manifest["iterations"][0]["task_id"]
    );
    assert_eq!(
        std::fs::read_to_string(root.join("guide.md")).unwrap(),
        "hello\n"
    );
    coder_first.assert_hits(1);
    resumed.assert_hits(1);
    observer.assert_hits(2);
}

#[test]
fn final_shared_request_can_be_a_report_only_after_observer_participation() {
    let (mut app, _dir) = crate::tui::events::tests::interaction_app();
    let messages = crate::tui::events::tests::closeout_messages();
    let task = crate::task_origin::root_user_text(&messages).to_owned();
    app.coder_session = Some(AgentSession::new(None, None, None, None, messages));
    app.work_budget = crate::work_budget::WorkBudget::new(&task, 3, None);
    assert!(!super::super::iterations::last_coder_report(&app));
    for role in ["coder", "observer"] {
        let request = app.work_budget.begin(role, &json!({})).unwrap();
        request.status("completed");
    }
    assert!(super::super::iterations::last_coder_report(&app));
    app.work_budget = crate::work_budget::WorkBudget::new(&task, 1, None);
    assert!(!super::super::iterations::last_coder_report(&app));
}

#[tokio::test]
async fn explicit_follow_up_keeps_conversation_and_requires_new_human_task_proof() {
    let server = MockServer::start();
    let call = server.mock(|when, then| {
        when.method(POST)
            .path("/chat/completions")
            .body_contains("seed-proof")
            .body_contains("pending_artifact_paths");
        then.status(400)
            .json_body(json!({"error":{"message":"stop after capturing first request"}}));
    });
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("project");
    std::fs::create_dir(&root).unwrap();
    let mut history = crate::tui::events::tests::closeout_messages();
    history.push(json!({"role":"assistant","content":"seed-proof"}));
    let task = crate::task_origin::root_user_text(&history).to_owned();
    let seed = dir.path().join("prior.json");
    AgentSession::save_atomic(
        &seed,
        &AgentSession::new(
            Some(root.canonicalize().unwrap().display().to_string()),
            None,
            None,
            None,
            history,
        ),
    )
    .unwrap();
    let prompt = dir.path().join("task.txt");
    std::fs::write(&prompt, &task).unwrap();
    let out = dir.path().join("out");
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
        "coupled-run",
        "--tool-root",
        root.to_str().unwrap(),
        "--prompt-file",
        prompt.to_str().unwrap(),
        "--follow-up-session",
        seed.to_str().unwrap(),
        "--out-dir",
        out.to_str().unwrap(),
        "--max-requests",
        "2",
    ])
    .unwrap();
    let Some(crate::Command::CoupledRun(args)) = cli.command else {
        unreachable!()
    };
    assert!(run(args, cli.common).await.is_err());
    let saved = AgentSession::load(&out.join("session.json")).unwrap();
    assert_eq!(
        saved
            .messages
            .iter()
            .filter(|m| crate::task_origin::is_human_request(m) && m["content"] == task)
            .count(),
        2
    );
    let view = crate::tui::agent::task_obligations::project(&saved.messages, None);
    assert!(
        !view.task_requirements.pending_artifact_paths.is_empty()
            && !view.pending_exact_checks.is_empty()
    );
    assert!(view.iteration_handoff.is_none() && view.previous_blocked_report.is_none());
    call.assert_hits(1);
}

#[tokio::test]
async fn the_reserved_last_wire_is_a_coder_report_and_can_finish_the_task() {
    let server = MockServer::start();
    let terminal=server.mock(|when,then|{
        when.method(POST).path("/chat/completions").body_contains("Use your last turn").matches(|r| {
            let v:Value=serde_json::from_slice(r.body.as_deref().unwrap_or_default()).unwrap();
            v["tools"].as_array().is_some_and(|t| t.len()==1 && t[0]["function"]["name"]=="done")
        });
        then.status(200).header("content-type","text/event-stream").body(response("done","last-report",crate::tui::events::tests::closeout_args(),"<impact>\nchanged: source.py and docs/usage.md repaired\nprogress: step 2 exact check passed\nremaining_gap: final report\n</impact>\n<think>\ngoal: Report current proof\nstep: 2\ntool: done\nrisk: inaccurate claims\ndoubt: none\nnext: finish report\nverify: retained native check\n</think>"));
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
        "--timeout-seconds",
        "2",
    ])
    .unwrap();
    let cfg = cli.common.to_partial_config().resolve().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("workspace");
    std::fs::create_dir(&root).unwrap();
    let output = temp.path().join("capture");
    let storage = crate::runtime_storage::RuntimeStorage::create(&root, &output).unwrap();
    let root = root.canonicalize().unwrap().display().to_string();
    let mut app = App::new(
        cfg.clone(),
        cfg.clone(),
        cfg,
        Some(root.clone()),
        Some(root.clone()),
        false,
        "en".into(),
        Some(3),
    );
    let messages = crate::tui::events::tests::closeout_messages();
    let task = crate::task_origin::root_user_text(&messages).to_owned();
    app.coder_session = Some(AgentSession::new(Some(root), None, None, None, messages));
    app.work_budget = crate::work_budget::WorkBudget::new(&task, 3, None);
    for role in ["coder", "observer"] {
        app.work_budget
            .begin(role, &json!({}))
            .unwrap()
            .status("completed");
    }
    let trace = crate::trace_writer::TraceWriter::new(output.join("trace.jsonl")).unwrap();
    crate::runtime_storage::scope(Some(storage), async {
        let (tx, mut rx) = mpsc::channel(256);
        assert!(
            super::super::iterations::send_last_report(&mut app, &tx, &trace)
                .await
                .unwrap()
        );
        drain(&mut app, &mut rx, PaneId::Coder, &trace)
            .await
            .unwrap();
    })
    .await;
    terminal.assert_hits(1);
    assert_eq!(app.coder_completed, Some(true));
    assert_eq!(
        app.work_budget.snapshot().requests.last().unwrap().role,
        "coder"
    );
    assert_eq!(app.work_budget.remaining(), 0);
}
