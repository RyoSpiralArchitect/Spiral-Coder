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
