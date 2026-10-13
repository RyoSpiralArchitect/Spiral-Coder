use super::*;
use crate::streaming::tool_calls::ToolCallAccumulator;

#[test]
fn blocked_signed_exchange_survives_compaction_save_and_resume() {
    let mut streamed = ToolCallAccumulator::default();
    streamed.push(&json!([{"index":0,"id":"signed-read","function":{"name":"read_file","arguments":"{\"path\":\"src/lib.rs\"}"},
        "extra_content":{"google":{"thought_signature":"opaque-test-value"}}}])).unwrap();
    let call = streamed.drain().pop().unwrap();
    let mut messages = vec![json!({"role":"user","content":"Repair src/lib.rs and verify it."})];
    let text = "<plan>\ngoal: repair\nsteps:\n- read\nacceptance: tests pass\n</plan>\n".repeat(24);
    push_blocked_tool_exchange(&mut messages, &text, &call, "Missing think block");
    let signed_message = messages[1].clone();
    for index in 0..55 {
        messages.push(
            json!({"role":"assistant","content":format!("<think>goal: observe {index}</think>")}),
        );
    }
    prune_old_assistant_messages(&mut messages);
    prune_old_tool_results(&mut messages);
    message_window::prune_message_window(&mut messages);
    assert!(messages.iter().any(|message| message == &signed_message));
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("session.json");
    let session = crate::agent_session::AgentSession::new(None, None, None, None, messages);
    crate::agent_session::AgentSession::save_atomic(&path, &session).unwrap();
    let mut resumed = crate::agent_session::AgentSession::load(&path).unwrap();
    assert_eq!(resumed.repair_for_resume(), None);
    let prepared = crate::streaming::provider_metadata::prepare_messages(
        &resumed.messages,
        "https://generativelanguage.googleapis.com/v1beta/openai",
    );
    let index = prepared
        .iter()
        .position(|message| message == &signed_message)
        .unwrap();
    assert_eq!(prepared[index + 1]["tool_call_id"], "signed-read");
    assert!(prepared[index + 1]["content"]
        .as_str()
        .unwrap()
        .starts_with("GOVERNOR BLOCKED"));
    assert_eq!(prepared[index]["tool_calls"][0], call.to_json());
}
