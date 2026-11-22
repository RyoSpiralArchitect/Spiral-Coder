//! The authored-test coverage plan through native file dispatch.
use super::*;
use httpmock::prelude::*;
use serde_json::Value;

const CHECK: &str = "python3 -B -S check.py";
const PLAN: &str = "[Test coverage plan]";

fn sse(content: &str, id: &str, name: &str, arguments: Value) -> String {
    let call =
        json!({"index":0,"id":id,"function":{"name":name,"arguments":arguments.to_string()}});
    format!(
        "data: {}\n\ndata: [DONE]\n\n",
        json!({"choices":[{"delta":{"content":content,"tool_calls":[call]},"finish_reason":"tool_calls"}]})
    )
}

fn write(content: &str, id: &str, path: &str, body: &str) -> String {
    sse(
        content,
        id,
        "write_file",
        json!({"path":path,"content":body}),
    )
}

fn think(tool: &str, next: &str) -> String {
    format!("<think>\ngoal: Implement status.py and its tests\nstep: 1\ntool: {tool}\nrisk: missing cases\ndoubt: none\nnext: {next}\nverify: {CHECK}\n</think>")
}

fn impact(changed: &str) -> String {
    format!("<impact>\nchanged: {changed}\nprogress: step 1\nremaining_gap: run the declared check\n</impact>\n")
}

fn body(request: &HttpMockRequest) -> String {
    String::from_utf8_lossy(request.body.as_deref().unwrap_or_default()).into_owned()
}

#[tokio::test]
async fn declared_cases_return_the_first_funded_test_plan_to_the_observer() {
    let server = MockServer::start();
    let authored = server.mock(|when, then| {
        when.method(POST).path("/chat/completions");
        then.status(200)
            .header("content-type", "text/event-stream")
            .body(write(
                &think("write_file", "write tests/test_status.py"),
                "first-tests",
                "tests/test_status.py",
                "def test_valid_record():\n    pass\n",
            ));
    });
    let root = tempfile::tempdir().unwrap();
    let requirements = json!({"version":1,"requirements":[{"id":"record-tests",
        "description":"Cover empty input, absent names and repeated values.",
        "artifacts":[{"path":"tests/test_status.py","kind":"test"}],
        "checks":[{"command":CHECK}]}]});
    let task = format!("Implement status.py and its tests.\n<task_requirements>\n{requirements}\n</task_requirements>");
    let messages = vec![
        crate::task_origin::user_message(&task, crate::task_origin::MessageOrigin::User),
        json!({"role":"assistant","accepted_plan":{
            "goal":"Implement status.py and its tests","steps":["Write tests/test_status.py","Run the declared check"],
            "acceptance_criteria":["The public tests pass"],"risks":"missing cases","assumptions":"python3 is available"}}),
    ];
    let budget = crate::work_budget::WorkBudget::new(&task, 8, None);
    let session = AgenticStartState {
        messages,
        checkpoint: None,
        cur_cwd: None,
        observation_cache: None,
        session_bridge: None,
        create_checkpoint: false,
    };
    let (tx, mut rx) = mpsc::channel(256);
    let result = crate::work_budget::scope(
        Some(crate::work_budget::Context {
            budget: budget.with_observer_return(3),
            role: "coder",
        }),
        Box::pin(run_agentic_json(
            session,
            &super::check_catalog_tests::mock_config(&server),
            root.path().to_str(),
            3,
            tx,
            None,
            None,
            None,
            true,
            Some(RealizePreset::Off),
            None,
            &crate::approvals::AutoApprover,
        )),
    )
    .await
    .unwrap();
    authored.assert_hits(1);
    let mut review = false;
    let mut yielded = false;
    while let Ok(token) = rx.try_recv() {
        if let StreamToken::Telemetry(e) = token {
            review |= e.event == "observer_authored_test_review"
                && e.data["changed_call_id"] == "first-tests";
            yielded |= e.data["yield_reason"] == "authored_test_review";
        }
    }
    assert!(review && yielded);
    assert_eq!(budget.snapshot().task_requests(), 1);
    assert!(evidence_packet::project(&result.messages, None)
        .pending_exact_checks
        .contains(&CHECK.to_owned()));
    // The same native history cannot trigger a second case review.
    assert!(requirement_proof::test_plan(
        &result.messages,
        root.path().to_str(),
        "tests/test_status.py",
        Some(7)
    )
    .is_none());
    let packet = json!({"work_budget":{"next_coder_turn":"work","next_coder_requests":6},
        "task_obligations":task_obligations::project_advisory(&result.messages,None,root.path().to_str())});
    let prompt = crate::tui::events::observer_assist::build_tui_next_action_prompt(
        &packet,
        "en",
        "authored_test_review",
    );
    assert!(prompt.starts_with("Review the Coder's current test assertions"));
    assert!(prompt.contains("absent names") && prompt.contains(CHECK));
    assert_eq!(prompt.matches("def test_valid_record()").count(), 1);
    assert!(!prompt.contains("two distinguishable") && !prompt.contains("symbolic link"));
    for (pointer, value) in [
        ("/work_budget/next_coder_turn", json!("report_only")),
        ("/work_budget/next_coder_requests", json!(2)),
        ("/task_obligations/required_recheck", json!({"command":CHECK})),
        ("/task_obligations/required_diagnostic_read", json!({"path":"check.py"})),
        ("/task_obligations/report_repair", json!({"reason":"missing receipt"})),
        ("/task_obligations/task_requirements/pending_baselines", json!([CHECK])),
        ("/task_obligations/task_requirements/authored_tests/review_source/path", json!("tests/other.py")),
        ("/task_obligations/task_requirements/authored_tests/review_source", Value::Null),
        ("/task_obligations/task_requirements/authored_tests/test_only_requirements/0/explicit_cases", json!([])),
    ] {
        let mut blocked = packet.clone();
        let (parent, key) = pointer.rsplit_once('/').unwrap();
        blocked.pointer_mut(parent).unwrap()[key] = value;
        assert!(crate::observer_action::test_review::prompt(&blocked, "English").is_none(), "{pointer}");
    }
    // At the floor, the Coder still gets the plan, with no handoff that would take its last action.
    let first = crate::task_origin::user_message(&task, crate::task_origin::MessageOrigin::User);
    let tight = crate::work_budget::WorkBudget::new(&task, 3, None);
    crate::work_budget::scope(
        Some(crate::work_budget::Context {
            budget: tight.with_observer_return(2),
            role: "coder",
        }),
        async {
            assert!(
                requirement_proof::test_plan(&[first], None, "tests/test_status.py", Some(3))
                    .unwrap()
                    .review_floor
                    .is_none()
            );
        },
    )
    .await;
}

#[tokio::test]
async fn paid_test_review_waits_for_the_recheck_and_survives_resume_once() {
    let path = "tests/test_status.py";
    let requirements = json!({"version":1,"requirements":[{"id":"cases",
        "description":"Cover valid records, missing fields and repeated values.",
        "artifacts":[{"path":path,"kind":"test"}],"checks":[{"command":CHECK}]}]});
    let request = format!(
        "Implement and verify status.\n<task_requirements>\n{requirements}\n</task_requirements>"
    );
    let mut history = vec![crate::task_origin::user_message(
        &request,
        crate::task_origin::MessageOrigin::User,
    )];
    let call = |id: &str, name: &str, args: Value| {
        json!({"role":"assistant","tool_calls":[{
        "id":id, "function":{"name":name,"arguments":args.to_string()}}]})
    };
    history.push(call(
        "authored",
        "write_file",
        json!({"path":path,"content":"def test_status(): pass"}),
    ));
    let pending = json!({"pending":{"check":CHECK,"diagnostic":"public status value differed",
        "path":"check.py","location":"current","observation":"repair_attempted"}});
    history.push(
        json!({"role":"tool","tool_call_id":"authored","content":"OK: wrote 'tests/test_status.py'",
        "recovery_focus":{"version":2,"state":pending,"test_review":{"floor":3,"path":path}}}),
    );
    let context = |cap| {
        Some(crate::work_budget::Context {
            budget: crate::work_budget::WorkBudget::new(&request, cap, None)
                .with_observer_return(3),
            role: "coder",
        })
    };
    crate::work_budget::scope(context(8), async {
        let facts = task_obligations::project(&history, Some(CHECK));
        assert_eq!(facts.required_recheck.as_deref(), Some(CHECK));
        assert!(observer_handoff::take_test_review(&mut history, Some(CHECK), None).is_none());
        history.push(call("red", "exec", json!({"command":CHECK})));
        history.push(
            json!({"role":"tool","tool_call_id":"red","content":"FAILED (exit_code: 1)",
            "recovery_focus":{"version":2,"state":pending}}),
        );
        assert!(observer_handoff::take_test_review(&mut history, Some(CHECK), None).is_none());
    })
    .await;
    let mut restored: Vec<Value> =
        serde_json::from_str(&serde_json::to_string(&history).unwrap()).unwrap();
    restored.push(crate::task_origin::user_message(
        "Continue",
        crate::task_origin::MessageOrigin::Runtime,
    ));
    restored.push(call("green", "exec", json!({"command":CHECK})));
    restored.push(
        json!({"role":"tool","tool_call_id":"green","content":"OK (exit_code: 0)",
        "recovery_focus":{"version":2,"state":{"pending":null}}}),
    );
    crate::work_budget::scope(context(3), async {
        assert!(observer_handoff::take_test_review(&mut restored, Some(CHECK), None).is_none());
    })
    .await;
    crate::work_budget::scope(context(8), async {
        let mut steered = restored.clone();
        steered.push(crate::task_origin::user_message(
            "Inspect a new task",
            crate::task_origin::MessageOrigin::User,
        ));
        assert!(observer_handoff::take_test_review(&mut steered, Some(CHECK), None).is_none());
        let sent = observer_handoff::take_test_review(&mut restored, Some(CHECK), None).unwrap();
        assert_eq!(
            sent,
            json!({"changed_call_id":"authored","changed_path":path,"dispatched_on":"green"})
        );
        assert!(observer_handoff::take_test_review(&mut restored, Some(CHECK), None).is_none());
        assert_eq!(
            restored[2]["recovery_focus"]["test_review"]["dispatched_on"],
            "green"
        );
        assert!(crate::task_origin::provider_messages(&restored)[2]
            .get("recovery_focus")
            .is_none());
    })
    .await;
}

#[cfg(not(target_os = "windows"))]
#[tokio::test]
async fn a_written_linked_test_file_carries_one_plan_on_the_first_change_whose_auto_test_passes() {
    // The first test write fails its automatic test, so it carries no plan. After
    // the diagnosis read, the source write that makes the automatic test pass
    // carries it; a second test write carries none.
    let server = MockServer::start();
    let first_tests = server.mock(|when, then| {
        when.method(POST)
            .path("/chat/completions")
            .matches(|r| !body(r).contains("OK: wrote '"));
        then.status(200)
            .header("content-type", "text/event-stream")
            .body(write(
                &think("write_file", "write tests/test_status.py"),
                "tests-write-1",
                "tests/test_status.py",
                "def test_valid_record():\n    pass\n",
            ));
    });
    let diagnose = server.mock(|when, then| {
        when.method(POST).path("/chat/completions").matches(|r| {
            let b = body(r);
            b.contains("OK: wrote 'tests/test_status.py'") && !b.contains("\"tests-read\"")
        });
        then.status(200)
            .header("content-type", "text/event-stream")
            .body(sse(
                &format!(
                    "{}{}",
                    impact("wrote tests/test_status.py"),
                    think("read_file", "read tests/test_status.py")
                ),
                "tests-read",
                "read_file",
                json!({"path":"tests/test_status.py"}),
            ));
    });
    let source = server.mock(|when, then| {
        when.method(POST).path("/chat/completions").matches(|r| {
            let b = body(r);
            b.contains("\"tests-read\"") && !b.contains("OK: wrote 'status.py'")
        });
        then.status(200)
            .header("content-type", "text/event-stream")
            .body(write(
                &think("write_file", "write status.py"),
                "status-write",
                "status.py",
                "print('ok')\n",
            ));
    });
    let second_tests = server.mock(|when, then| {
        when.method(POST)
            .path("/chat/completions")
            .matches(|r| body(r).contains("OK: wrote 'status.py'"));
        then.status(200)
            .header("content-type", "text/event-stream")
            .body(write(
                &format!(
                    "{}{}",
                    impact("wrote status.py"),
                    think("write_file", "add the missing record test")
                ),
                "tests-write-2",
                "tests/test_status.py",
                "def test_valid_record():\n    pass\n\ndef test_missing_record():\n    pass\n",
            ));
    });

    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("check.py"), "pass\n").unwrap();
    let requirements = json!({"version":1,"requirements":[
        {"id":"status-output","description":"Print the status summary for a valid record.",
         "artifacts":[{"path":"status.py","kind":"source"},{"path":"tests/test_status.py","kind":"test"}],
         "checks":[{"command":CHECK}]},
        {"id":"status-tests","description":"Cover a missing record, a dangling link and a duplicate identifier.",
         "artifacts":[{"path":"tests/test_status.py","kind":"test"}],
         "checks":[{"command":CHECK}]}]});
    let goal = format!(
        "Implement status.py and its tests.\n<task_requirements>\n{requirements}\n</task_requirements>"
    );
    let mut messages = vec![crate::task_origin::user_message(
        &goal,
        crate::task_origin::MessageOrigin::User,
    )];
    messages.push(json!({"role":"assistant","accepted_plan":{
        "goal":"Implement status.py and its tests",
        "steps":["Write status.py and tests/test_status.py", "Run the declared check"],
        "acceptance_criteria":["status.py prints the summary and the declared check passes"],
        "risks":"missing cases", "assumptions":"python3 is available"
    }}));
    let start = AgenticStartState {
        messages,
        checkpoint: None,
        cur_cwd: None,
        observation_cache: None,
        session_bridge: None,
        create_checkpoint: false,
    };
    let (tx, mut rx) = mpsc::channel(512);
    let result = run_agentic_json(
        start,
        &super::check_catalog_tests::mock_config(&server),
        root.path().to_str(),
        4,
        tx,
        None,
        None,
        // The automatic test fails until status.py exists.
        Some("test -f status.py".into()),
        true,
        Some(RealizePreset::Off),
        None,
        &crate::approvals::AutoApprover,
    )
    .await
    .unwrap();
    let mut display = String::new();
    while let Ok(token) = rx.try_recv() {
        if let StreamToken::Delta(text) = token {
            display.push_str(&text);
        }
    }
    assert_eq!(
        (
            first_tests.hits(),
            diagnose.hits(),
            source.hits(),
            second_tests.hits()
        ),
        (1, 1, 1, 1),
        "{display}\n{:?}",
        result.messages
    );
    let content = |id: &str| -> String {
        result
            .messages
            .iter()
            .find(|m: &&Value| m["role"] == "tool" && m["tool_call_id"] == id)
            .and_then(|m| m["content"].as_str())
            .unwrap_or_default()
            .to_owned()
    };
    let first = content("tests-write-1");
    assert!(
        first.starts_with("OK: wrote 'tests/test_status.py'"),
        "{first}"
    );
    assert!(first.contains("[auto-test] ✗ FAILED"), "{first}");
    assert!(!first.contains(PLAN), "{first}");

    let planned = content("status-write");
    assert!(planned.starts_with("OK: wrote 'status.py'"), "{planned}");
    let (result_text, plan) = planned
        .split_once(PLAN)
        .expect("the plan follows the result");
    assert!(
        result_text.contains("[auto-test] ✓ PASSED (exit 0)"),
        "{planned}"
    );
    assert!(plan.contains("tests/test_status.py is linked by the original task to 2 requirements"));
    assert!(plan.contains(
        "- status-tests: Cover a missing record, a dangling link and a duplicate identifier."
    ));
    assert!(plan.contains("Current test functions (1): test_valid_record"));
    assert!(plan.contains("Map the cases now:"));
    assert!(display.contains(PLAN));

    let second = content("tests-write-2");
    assert!(
        second.starts_with("OK: wrote 'tests/test_status.py'"),
        "{second}"
    );
    assert!(!second.contains(PLAN), "{second}");
    assert_eq!(
        result
            .messages
            .iter()
            .filter(
                |m| m["role"] == "tool" && m["content"].as_str().is_some_and(|c| c.contains(PLAN))
            )
            .count(),
        1
    );
    // The plan is advisory: it adds no edit evidence and leaves the check pending.
    let evidence = evidence_packet::project(&result.messages, Some(CHECK));
    assert!(evidence.pending_exact_checks.contains(&CHECK.to_string()));
}
