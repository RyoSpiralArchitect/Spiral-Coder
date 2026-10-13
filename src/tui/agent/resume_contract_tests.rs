use super::task_harness::ArtifactMode;
use super::*;
use crate::agent_session::AgentSession;
use crate::task_origin::{continuation_prompt, root_user_text, user_message, MessageOrigin};

fn resumed_messages(root: &str) -> Vec<serde_json::Value> {
    let mut messages = vec![json!({"role":"user", "content":root})];
    messages.extend([
        json!({"role":"assistant", "tool_calls":[{"id":"read", "function":{"name":"read_file", "arguments":"{\"path\":\"src/lib.rs\"}"}}]}),
        json!({"role":"tool", "tool_call_id":"read", "content":"[src/lib.rs]\nsource"}),
        user_message("[implied_exec]\nOK (exit_code: 0)", MessageOrigin::Runtime),
        user_message("[goal_check] current progress", MessageOrigin::Runtime),
        user_message(continuation_prompt("en"), MessageOrigin::Runtime),
    ]);
    let session = AgentSession::new(None, None, None, None, messages);
    let mut loaded: AgentSession =
        serde_json::from_str(&serde_json::to_string(&session).unwrap()).unwrap();
    assert_eq!(loaded.repair_for_resume(), None);
    loaded.messages
}

#[test]
fn resumed_read_only_contract_does_not_become_a_mutating_continuation() {
    let root = "Read only: locate the slash handler without editing. Final answer must include `inspection complete`.";
    let messages = resumed_messages(root);
    let selected = root_user_text(&messages);
    assert!(is_root_read_only_observation_task(selected));
    assert_eq!(
        TaskHarness::infer(selected, true).artifact_mode,
        ArtifactMode::ObserveOnly
    );
    assert!(requires_authored_final_answer(selected));
    assert!(validate_authored_done_summary(
        selected,
        "Located handler",
        "Located handler",
        &messages,
        None
    )
    .is_err());
    assert!(validate_authored_done_summary(
        selected,
        "inspection complete",
        "inspection complete",
        &messages,
        None
    )
    .is_ok());
}

#[test]
fn resumed_exact_bytes_and_final_literal_remain_independent_gates() {
    let root = "Create `receipt.txt` containing exactly `verified`. Verify it. Final answer must include `review ready`.";
    let messages = resumed_messages(root);
    let selected = root_user_text(&messages);
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path().to_str().unwrap();
    std::fs::write(temp.path().join("receipt.txt"), "verified\n").unwrap();
    assert!(exact_content::validate_exact_content_task(selected, Some(dir)).is_err());
    std::fs::write(temp.path().join("receipt.txt"), "verified").unwrap();
    assert!(exact_content::validate_exact_content_task(selected, Some(dir)).is_ok());
    assert!(validate_authored_done_summary(
        selected,
        "Created receipt.txt",
        "Created receipt.txt",
        &messages,
        None
    )
    .is_err());
    assert!(validate_authored_done_summary(
        selected,
        "review ready",
        "review ready",
        &messages,
        None
    )
    .is_ok());
}

#[test]
fn resumed_benchmark_command_authority_survives_feedback_and_window_pruning() {
    let check = "custom-check && printf verified > receipt.txt";
    let root = format!(
        "<observer_benchmark_plan>\nrequired_checks:\n- {check}\n</observer_benchmark_plan>"
    );
    let mut messages = resumed_messages(&root);
    messages.extend([
        json!({"role":"assistant", "tool_calls":[{"id":"check", "function":{"name":"exec", "arguments":json!({"command":check}).to_string()}}]}),
        json!({"role":"tool", "tool_call_id":"check", "content":"OK (exit_code: 0)"}),
    ]);
    for index in 0..30 {
        let id = format!("noise-{index}");
        messages.extend([
            json!({"role":"assistant", "tool_calls":[{"id":id, "function":{"name":"exec", "arguments":"{\"command\":\"git status --short\"}"}}]}),
            json!({"role":"tool", "tool_call_id":id, "content":"OK (exit_code: 0)"}),
        ]);
    }
    messages.push(user_message(
        continuation_prompt("ja"),
        MessageOrigin::Runtime,
    ));
    prune_message_window(&mut messages);
    assert!(messages
        .iter()
        .any(|message| message["tool_call_id"] == "check"));
    let context = exec_verification::ExecVerificationContext::from_messages(None, &messages);
    assert_eq!(context.classify(check).kind, ExecKind::Verify);
    assert_eq!(
        context.classify(check).verification,
        Some(VerificationLevel::Behavioral)
    );
    let (_, mutation, _, verified, _) = restore_done_gate_from_messages(&messages, None);
    assert!(mutation.is_none());
    assert!(verified.is_some());
    assert!(!benchmark_plan_missing_required_exec_proof(
        root_user_text(&messages),
        &messages,
        None
    ));
}

#[test]
fn new_human_steering_replaces_old_contract_after_runtime_feedback() {
    let mut messages = resumed_messages(
        "Create `old.txt` containing exactly `old`. Final answer must include `old label`.",
    );
    messages.push(user_message(
        "Read only: locate the new handler without editing.",
        MessageOrigin::User,
    ));
    messages.push(user_message(
        continuation_prompt("en"),
        MessageOrigin::Runtime,
    ));
    let selected = root_user_text(&messages);
    assert!(is_root_read_only_observation_task(selected));
    assert!(!requires_authored_final_answer(selected));
    assert!(exact_content::validate_exact_content_task(selected, None).is_ok());
}
