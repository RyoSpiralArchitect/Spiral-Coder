use super::*;
use httpmock::prelude::*;

fn config(server: &MockServer) -> RunConfig {
    RunConfig {
        provider: ProviderKind::OpenAiCompatible,
        model: "test".into(),
        chat_model: "test".into(),
        code_model: "test".into(),
        api_key: None,
        base_url: server.base_url(),
        mode: crate::modes::Mode::Vibe,
        persona: String::new(),
        temperature: 0.0,
        max_tokens: 512,
        timeout_seconds: 5,
        hf_device: "cpu".into(),
        hf_local_only: true,
    }
}
fn start(goal: &str) -> AgenticStartState {
    AgenticStartState {
        messages: vec![crate::task_origin::user_message(
            goal,
            crate::task_origin::MessageOrigin::User,
        )],
        checkpoint: None,
        cur_cwd: None,
        observation_cache: None,
        session_bridge: None,
        create_checkpoint: false,
    }
}
fn sse(content: &str, calls: serde_json::Value) -> String {
    format!(
        "data: {}\n\ndata: [DONE]\n\n",
        json!({"choices":[{"delta":{"content":content,"tool_calls":calls},"finish_reason":"tool_calls"}]})
    )
}

#[tokio::test]
async fn rejected_signed_batch_reaches_next_request_with_every_correlated_refusal() {
    let server = MockServer::start();
    let calls = json!([
        {"index":0,"id":"first","function":{"name":"write_file","arguments":"{\"path\":\"sentinel.txt\",\"content\":\"must not run\"}"},"extra_content":{"google":{"thought_signature":"opaque-first"}}},
        {"index":1,"id":"second","function":{"name":"read_file","arguments":"{\"path\":\"fixture.txt\"}"},"extra_content":{"google":{"thought_signature":"opaque-second"}}}
    ]);
    let first = server.mock(|when, then| {
        when.method(POST).path("/chat/completions").matches(|r| {
            let b: serde_json::Value = serde_json::from_slice(r.body.as_deref().unwrap()).unwrap();
            !b["messages"]
                .as_array()
                .unwrap()
                .iter()
                .any(|m| m["role"] == "tool")
        });
        then.status(200)
            .header("content-type", "text/event-stream")
            .body(sse("", calls.clone()));
    });
    let second=server.mock(|when,then| {
        when.method(POST).path("/chat/completions").matches(|r| {
            let b:serde_json::Value=serde_json::from_slice(r.body.as_deref().unwrap()).unwrap();
            let ms=b["messages"].as_array().unwrap();
            let signed=ms.iter().find(|m| m["tool_calls"][0]["id"]=="first");
            signed.is_some_and(|m| m["tool_calls"][0]["function"]["name"] == "write_file"
                && m["tool_calls"][1]["id"] == "second"
                && m["tool_calls"][0].get("extra_content").is_none())
                && ["first","second"].iter().all(|id| ms.iter().any(|m| m["tool_call_id"]==*id && m["content"].as_str().is_some_and(|s| s.starts_with("GOVERNOR BLOCKED") && s.contains("No tool"))))
        });
        let content="<plan>\ngoal: Inspect fixture.txt and report its contents\nsteps:\n1. Read fixture.txt\n2. Report findings\nacceptance: fixture.txt is inspected\nrisks: wrong path\nassumptions: fixture.txt exists\n</plan>\n<think>\ngoal: Inspect fixture.txt\nstep: 1\ntool: read_file\nrisk: wrong path\ndoubt: none\nnext: read fixture.txt\nverify: inspect returned contents\n</think>";
        then.status(200).header("content-type","text/event-stream").body(sse(content,json!([{"index":0,"id":"actual-read","function":{"name":"read_file","arguments":"{\"path\":\"fixture.txt\"}"}}])));
    });
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("fixture.txt"), "retained-native-read").unwrap();
    let (tx, _rx) = mpsc::channel(512);
    let goal = "Inspect fixture.txt and report the content; do not edit files.";
    let result = run_agentic_json(
        start(goal),
        &config(&server),
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
    let signed = result
        .messages
        .iter()
        .find(|m| m["tool_calls"][0]["id"] == "first")
        .unwrap();
    for (index, signature) in ["opaque-first", "opaque-second"].iter().enumerate() {
        assert_eq!(
            signed["tool_calls"][index]["extra_content"]["google"]["thought_signature"],
            *signature
        );
    }
    let session =
        crate::agent_session::AgentSession::new(None, None, None, None, result.messages.clone());
    let path = root.path().join("session.json");
    crate::agent_session::AgentSession::save_atomic(&path, &session).unwrap();
    let mut resumed = crate::agent_session::AgentSession::load(&path).unwrap();
    assert_eq!(resumed.repair_for_resume(), None);
    let google = crate::streaming::provider_metadata::prepare_messages(
        &resumed.messages,
        "https://generativelanguage.googleapis.com/v1beta/openai",
    );
    assert!(google.iter().any(|m| m == signed));
    first.assert_hits(1);
    second.assert_hits(1);
    assert!(!root.path().join("sentinel.txt").exists());
    assert_eq!(crate::task_origin::root_user_text(&result.messages), goal);
    assert!(result
        .messages
        .iter()
        .any(|m| m["tool_call_id"] == "actual-read"
            && m["content"]
                .as_str()
                .unwrap_or("")
                .contains("retained-native-read")));
}

#[tokio::test]
async fn final_shared_request_offers_only_done_and_cannot_execute_an_unoffered_edit() {
    check_final_report_rejection(false).await;
}

#[tokio::test]
async fn final_shared_request_cannot_turn_text_into_a_synthetic_edit() {
    check_final_report_rejection(true).await;
}

async fn check_final_report_rejection(text_only: bool) {
    let server = MockServer::start();
    let wire=server.mock(|when,then| {
        when.method(POST).path("/chat/completions").matches(|r| {
            let b:serde_json::Value=serde_json::from_slice(r.body.as_deref().unwrap()).unwrap();
            b["tools"].as_array().unwrap().len()==1 && b["tools"][0]["function"]["name"]=="done"
                && b["tools"][0]["function"]["parameters"]["properties"]["completed_acceptance"]["items"]["enum"] == json!(["acceptance 1"])
        });
        then.status(200).header("content-type","text/event-stream").body(if text_only { sse("File: sentinel.txt\n```text\nmust not run\n```", json!([])) } else { sse("",json!([{"index":0,"id":"unoffered","function":{"name":"write_file","arguments":"{\"path\":\"sentinel.txt\",\"content\":\"must not run\"}"}}])) });
    });
    let root = tempfile::tempdir().unwrap();
    let (tx, _rx) = mpsc::channel(512);
    let goal = "Complete the declared repair.\n<task_requirements>\n{\"version\":1,\"requirements\":[{\"id\":\"repair\",\"description\":\"Repair sentinel.txt\",\"artifacts\":[{\"path\":\"sentinel.txt\",\"kind\":\"source\"}],\"checks\":[{\"command\":\"test -f sentinel.txt\"}]}]}\n</task_requirements>";
    let budget = crate::work_budget::WorkBudget::new(goal, 1, None);
    let mut seeded = start(goal);
    seeded.messages.push(json!({"role":"assistant","content":"<plan>\ngoal: Repair sentinel.txt\nsteps:\n1. Inspect sentinel.txt\n2. Repair and verify sentinel.txt\nacceptance: sentinel.txt is repaired and verified\nrisks: incorrect contents\nassumptions: workspace is available\n</plan>"}));
    let context = crate::work_budget::Context {
        budget: budget.clone(),
        role: "coder",
    };
    let result = crate::work_budget::scope(
        Some(context),
        run_agentic_json(
            seeded,
            &config(&server),
            root.path().to_str(),
            1,
            tx,
            None,
            None,
            None,
            true,
            Some(RealizePreset::Off),
            None,
            &crate::approvals::AutoApprover,
        ),
    )
    .await
    .unwrap();
    wire.assert_hits(1);
    assert!(!root.path().join("sentinel.txt").exists());
    assert_eq!(budget.snapshot().requests.len(), 1);
    if text_only {
        assert!(result
            .messages
            .iter()
            .all(|m| m["role"] != "tool" && m.get("tool_calls").is_none()));
    } else {
        assert!(result
            .messages
            .iter()
            .any(|m| m["tool_call_id"] == "unoffered"
                && m["content"]
                    .as_str()
                    .unwrap_or("")
                    .starts_with("GOVERNOR BLOCKED")));
    }
}
