//! A declared artifact that does not exist yet is created, not diagnosed.
use super::*;
use httpmock::prelude::*;
use serde_json::Value;

const CHECK: &str = "python3 -B -S check.py";
const TESTS: &str = "tests/test_status.py";

fn sse(content: &str, id: &str, name: &str, arguments: Value) -> String {
    let call =
        json!({"index":0,"id":id,"function":{"name":name,"arguments":arguments.to_string()}});
    format!(
        "data: {}\n\ndata: [DONE]\n\n",
        json!({"choices":[{"delta":{"content":content,"tool_calls":[call]},"finish_reason":"tool_calls"}]})
    )
}

fn think(tool: &str, next: &str) -> String {
    format!("<think>\ngoal: Implement status.py and its tests\nstep: 1\ntool: {tool}\nrisk: wrong fixture\ndoubt: none\nnext: {next}\nverify: {CHECK}\n</think>")
}

fn body(request: &HttpMockRequest) -> String {
    String::from_utf8_lossy(request.body.as_deref().unwrap_or_default()).into_owned()
}

#[tokio::test]
async fn s31_reading_a_declared_test_file_before_creating_it_does_not_hold_the_write() {
    // Sealed S31 iteration 6 shape: the Coder read the declared test file
    // before creating it. The "No such file" read error entered diagnose, and
    // three write_file calls creating that file were held by the Recovery Gate.
    let server = MockServer::start();
    let read = server.mock(|when, then| {
        when.method(POST)
            .path("/chat/completions")
            .matches(|r| !body(r).contains("ERROR reading"));
        then.status(200)
            .header("content-type", "text/event-stream")
            .body(sse(
                &think("read_file", "read tests/test_status.py"),
                "read-missing",
                "read_file",
                json!({"path":TESTS}),
            ));
    });
    let create = server.mock(|when, then| {
        when.method(POST)
            .path("/chat/completions")
            .matches(|r| body(r).contains("ERROR reading"));
        then.status(200)
            .header("content-type", "text/event-stream")
            .body(sse(
                &think("write_file", "create tests/test_status.py"),
                "create-tests",
                "write_file",
                json!({"path":TESTS,"content":"def test_valid_record():\n    pass\n"}),
            ));
    });
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("check.py"), "pass\n").unwrap();
    let requirements = json!({"version":1,"requirements":[
        {"id":"status-tests","description":"Cover a missing record and a duplicate identifier.",
         "artifacts":[{"path":"status.py","kind":"source"},{"path":TESTS,"kind":"test"}],
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
        "acceptance_criteria":["status.py works and the declared check passes"],
        "risks":"wrong fixture", "assumptions":"python3 is available"
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
        2,
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
    let mut display = String::new();
    while let Ok(token) = rx.try_recv() {
        if let StreamToken::Delta(text) = token {
            display.push_str(&text);
        }
    }
    assert_eq!(
        (read.hits(), create.hits()),
        (1, 1),
        "{display}\n{:?}",
        result.messages
    );
    let content = |id: &str| -> String {
        result
            .messages
            .iter()
            .find(|m| m["role"] == "tool" && m["tool_call_id"] == id)
            .and_then(|m| m["content"].as_str())
            .unwrap_or_default()
            .to_owned()
    };
    assert!(content("read-missing").starts_with("ERROR reading 'tests/test_status.py'"));
    let created = content("create-tests");
    assert!(
        created.starts_with("OK: wrote 'tests/test_status.py'"),
        "{created}"
    );
    assert!(root.path().join(TESTS).is_file());
    assert!(display.contains(
        "tests/test_status.py does not exist yet. The original task declares it as an artifact to produce"
    ));
    assert!(!display.contains("[Recovery Gate]"), "{display}");
}

#[test]
fn only_absent_declared_artifacts_skip_diagnosis() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("tests")).unwrap();
    std::fs::write(root.path().join("status.py"), "print('ok')\n").unwrap();
    let requirements = json!({"version":1,"requirements":[
        {"id":"status-tests","description":"Cover a missing record.",
         "artifacts":[{"path":"status.py","kind":"source"},{"path":TESTS,"kind":"test"}],
         "checks":[{"command":CHECK}]}]});
    let messages = vec![crate::task_origin::user_message(
        &format!("Implement it.\n<task_requirements>\n{requirements}\n</task_requirements>"),
        crate::task_origin::MessageOrigin::User,
    )];
    let workspace = root.path().to_str();
    let absent = |path: &str| {
        requirement_proof::absent_declared_artifact(&messages, Some(CHECK), path, workspace)
    };
    assert!(absent(TESTS));
    assert!(absent("./tests/test_status.py"));
    // An existing declared artifact, an undeclared path and a missing root are
    // ordinary read failures.
    assert!(!absent("status.py"));
    assert!(!absent("tests/other.py"));
    assert!(!requirement_proof::absent_declared_artifact(
        &messages,
        Some(CHECK),
        TESTS,
        None
    ));
    // A dangling link at the artifact path exists; it is not an absent artifact.
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(root.path().join("nowhere"), root.path().join(TESTS)).unwrap();
        assert!(!absent(TESTS));
    }
}
