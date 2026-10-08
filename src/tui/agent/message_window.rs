//! Bounded context windows preserve complete tool-call exchanges.
use std::collections::HashSet;

use serde_json::Value;

use super::{
    assistant_message_is_compactable, parse_evidence_block, parse_impact_block, parse_plan_block,
    parse_reflection_block, parse_think_block,
};

const KEEP_RECENT_MESSAGE_WINDOW: usize = 24;
const MAX_CONTEXT_MESSAGES: usize = 48;

fn tool_message_is_drop_safe(msg: &Value) -> bool {
    let content = msg["content"].as_str().unwrap_or("").trim_start();
    content.starts_with("OK (exit_code: 0)")
        || content.starts_with("OK: wrote '")
        || content.starts_with("OK: patched '")
        || content.starts_with("OK: applied ")
        || content.starts_with("OK write_file")
}

/// A removable exchange must contain every requested result, exactly once,
/// immediately after its assistant message. Incomplete or malformed exchanges
/// remain available to resume repair; pruning must not create new corruption.
fn removable_exchange_end(messages: &[Value], start: usize) -> Option<usize> {
    let calls = messages[start]["tool_calls"].as_array()?;
    if calls.is_empty() {
        return None;
    }
    let content = messages[start]["content"].as_str().unwrap_or("");
    if content.trim_start().starts_with("[DONE]")
        || content.contains("[error]")
        || content.contains("GOVERNOR BLOCKED")
    {
        return None;
    }
    let mut pending = HashSet::new();
    for call in calls {
        let id = call["id"].as_str().filter(|id| !id.is_empty())?;
        let name = call["function"]["name"].as_str()?;
        if matches!(name, "read_file" | "search_files" | "list_dir" | "glob") || !pending.insert(id)
        {
            return None;
        }
    }
    let end = start.checked_add(calls.len() + 1)?;
    for result in messages.get(start + 1..end)? {
        if result["role"].as_str() != Some("tool")
            || !tool_message_is_drop_safe(result)
            || !pending.remove(result["tool_call_id"].as_str()?)
        {
            return None;
        }
    }
    // An unexpected extra result makes this exchange malformed too.
    if messages.get(end).and_then(|msg| msg["role"].as_str()) == Some("tool") {
        return None;
    }
    pending.is_empty().then_some(end)
}

#[cfg(test)]
pub(super) fn prune_message_window(messages: &mut Vec<Value>) {
    let context = super::exec_verification::ExecVerificationContext::from_messages(None, messages);
    prune_message_window_with_context(messages, &context);
}

pub(super) fn prune_message_window_with_context(
    messages: &mut Vec<Value>,
    context: &super::exec_verification::ExecVerificationContext<'_>,
) {
    if messages.len() <= MAX_CONTEXT_MESSAGES {
        return;
    }
    let mut protected = HashSet::new();
    let mut proof_ids = super::task_harness::benchmark_plan_protected_call_ids(messages);
    proof_ids.extend(super::edit_failure::protected_reset_call_id(
        messages, context,
    ));
    for (idx, msg) in messages.iter().enumerate() {
        if !matches!(msg["role"].as_str(), Some("assistant" | "tool")) {
            protected.insert(idx);
        }
        let proof_call = msg["role"].as_str() == Some("assistant")
            && msg["tool_calls"].as_array().is_some_and(|calls| {
                calls
                    .iter()
                    .any(|call| call["id"].as_str().is_some_and(|id| proof_ids.contains(id)))
            });
        let proof_result = msg["role"].as_str() == Some("tool")
            && msg["tool_call_id"]
                .as_str()
                .is_some_and(|id| proof_ids.contains(id));
        if proof_call || proof_result {
            protected.insert(idx);
        }
    }
    protected.extend(messages.len().saturating_sub(KEEP_RECENT_MESSAGE_WINDOW)..messages.len());
    let anchors: &[fn(&str) -> bool] = &[
        |content| parse_plan_block(content).is_some(),
        |content| parse_think_block(content).is_some(),
        |content| parse_reflection_block(content).is_some(),
        |content| parse_impact_block(content).is_some(),
        |content| parse_evidence_block(content).is_some(),
    ];
    for check in anchors {
        if let Some((idx, _)) = messages.iter().enumerate().rev().find(|(_, msg)| {
            msg["role"].as_str() == Some("assistant")
                && check(msg["content"].as_str().unwrap_or("").trim())
        }) {
            protected.insert(idx);
        }
    }

    let mut drop_indices = HashSet::new();
    let over = messages.len() - MAX_CONTEXT_MESSAGES;
    for (idx, msg) in messages.iter().enumerate() {
        if drop_indices.len() >= over {
            break;
        }
        if msg["role"].as_str() != Some("assistant") || protected.contains(&idx) {
            continue;
        }
        let has_calls = msg["tool_calls"]
            .as_array()
            .is_some_and(|calls| !calls.is_empty());
        if has_calls {
            if let Some(end) = removable_exchange_end(messages, idx) {
                if (idx..end).all(|index| !protected.contains(&index)) {
                    drop_indices.extend(idx..end);
                }
            }
        } else if assistant_message_is_compactable(msg) {
            drop_indices.insert(idx);
        }
    }
    // The target is a soft cap: failures, observations, incomplete exchanges,
    // and recent/anchor groups take precedence over the requested window size.
    let mut index = 0;
    messages.retain(|_| {
        let keep = !drop_indices.contains(&index);
        index += 1;
        keep
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_session::AgentSession;
    use serde_json::json;

    fn exchange(id: &str, tool: &str, output: &str) -> Vec<Value> {
        vec![
            json!({"role":"assistant","tool_calls":[{
                "id":id,"type":"function","function":{"name":tool,"arguments":"{}"}
            }]}),
            json!({"role":"tool","tool_call_id":id,"content":output}),
        ]
    }

    fn exec_history(count: usize) -> Vec<Value> {
        let mut messages = vec![json!({"role":"user","content":"fix and verify"})];
        for idx in 0..count {
            messages.extend(exchange(
                &format!("exec_{idx}"),
                "exec",
                "OK (exit_code: 0)",
            ));
        }
        messages
    }

    fn assert_resume_intact(messages: &[Value]) {
        let mut session = AgentSession::new(None, None, None, None, messages.to_vec());
        assert_eq!(session.repair_for_resume(), None);
        assert_eq!(session.messages, messages);
    }

    #[test]
    fn odd_overflow_drops_a_whole_exchange() {
        let mut messages = exec_history(24);
        assert_eq!(messages.len(), MAX_CONTEXT_MESSAGES + 1);
        let expected = [messages[..1].to_vec(), messages[3..].to_vec()].concat();
        prune_message_window(&mut messages);
        assert_eq!(messages, expected);
        assert_resume_intact(&messages);
    }

    #[test]
    fn drops_old_exec_exchanges_but_keeps_observations() {
        let mut messages = exec_history(20);
        let observed = exchange("read", "read_file", "OK (exit_code: 0)");
        messages.extend(observed.clone());
        for idx in 20..28 {
            messages.extend(exchange(
                &format!("exec_{idx}"),
                "exec",
                "OK (exit_code: 0)",
            ));
        }
        prune_message_window(&mut messages);
        assert!(messages.len() <= MAX_CONTEXT_MESSAGES);
        assert!(messages.windows(2).any(|pair| pair == observed));
        assert!(!messages.iter().any(|msg| msg["tool_call_id"] == "exec_0"));
        assert_resume_intact(&messages);
    }

    #[test]
    fn recent_result_protects_its_assistant_call() {
        // Only this exchange is otherwise removable; its result is the first
        // message inside the protected window, while its call is outside it.
        let mut messages = vec![json!({"role":"user","content":"retain"}); 30];
        let boundary = exchange("boundary", "exec", "OK (exit_code: 0)");
        messages.extend(boundary.clone());
        messages.extend(vec![json!({"role":"user","content":"recent"}); 23]);
        let expected = messages.clone();
        prune_message_window(&mut messages);
        assert_eq!(messages, expected);
        assert_resume_intact(&messages);
    }

    #[test]
    fn multi_tool_exchange_is_removed_together() {
        let mut messages = vec![json!({"role":"assistant","tool_calls":[
            {"id":"a","function":{"name":"exec","arguments":"{}"}},
            {"id":"b","function":{"name":"exec","arguments":"{}"}}
        ]})];
        messages.push(json!({"role":"tool","tool_call_id":"b","content":"OK (exit_code: 0)"}));
        messages.push(json!({"role":"tool","tool_call_id":"a","content":"OK (exit_code: 0)"}));
        let tail = vec![json!({"role":"user","content":"retain"}); 47];
        messages.extend(tail.clone());
        prune_message_window(&mut messages);
        assert_eq!(messages, tail);
        assert_resume_intact(&messages);
    }

    #[test]
    fn failed_or_incomplete_exchanges_are_preserved() {
        for output in [Some("FAILED (exit_code: 1)"), None] {
            let mut messages = exchange("failed", "exec", "OK (exit_code: 0)");
            if let Some(output) = output {
                messages[1]["content"] = json!(output);
            } else {
                messages.pop();
            }
            messages.extend(vec![json!({"role":"user","content":"retain"}); 50]);
            let expected = messages.clone();
            prune_message_window(&mut messages);
            assert_eq!(messages, expected);
        }
    }

    #[test]
    fn benchmark_window_preserves_checks_and_mutations_that_invalidate_them() {
        let plan =
            "<observer_benchmark_plan>\nrequired_checks:\n- cargo test\n</observer_benchmark_plan>";
        for verify_after_edit in [false, true] {
            let mut messages = vec![json!({"role":"user","content":plan})];
            let mut check = exchange(
                "before",
                "exec",
                "OK (exit_code: 0)\nstdout:\none\ntwo\nthree",
            );
            check[0]["tool_calls"][0]["function"]["arguments"] =
                json!("{\"command\":\"cargo test\"}");
            messages.extend(check.clone());
            let mutation = exchange(
                "edit",
                "patch_file",
                "OK: patched 'src/lib.rs'\n[hash] changed\n[auto-test] ✗ FAILED (exit 1)",
            );
            messages.extend(mutation);
            if verify_after_edit {
                check[0]["tool_calls"][0]["id"] = json!("after");
                check[1]["tool_call_id"] = json!("after");
                messages.extend(check);
            }
            for index in 0..35 {
                messages.extend(exchange(
                    &format!("noise_{index}"),
                    "exec",
                    "OK (exit_code: 0)",
                ));
            }
            let proof_missing =
                super::super::task_harness::benchmark_plan_missing_required_exec_proof(
                    plan, &messages, None,
                );
            assert_eq!(proof_missing, !verify_after_edit);
            super::super::prune_old_tool_results(&mut messages);
            prune_message_window(&mut messages);
            assert!(messages.len() <= MAX_CONTEXT_MESSAGES);
            assert!(messages
                .iter()
                .any(|message| message["tool_call_id"] == "before"));
            assert!(messages
                .iter()
                .any(|message| message["tool_call_id"] == "edit"));
            assert_eq!(
                super::super::task_harness::benchmark_plan_missing_required_exec_proof(
                    plan, &messages, None
                ),
                proof_missing
            );
            assert_resume_intact(&messages);
        }
    }

    #[test]
    fn benchmark_window_keeps_shell_mutation_between_required_checks() {
        let plan = "<observer_benchmark_plan>\nrequired_checks:\n- check A\n- check B\n</observer_benchmark_plan>";
        for output in [
            "OK (exit_code: 0)\nstdout:\none\ntwo\nthree",
            "FAILED (exit_code: 1)",
            "ERROR: process timed out",
        ] {
            for rerun_b in [false, true] {
                let mut messages = vec![json!({"role":"user","content":plan})];
                let mut add_exec = |id: &str, command: &str| {
                    let mut pair =
                        exchange(id, "exec", "OK (exit_code: 0)\nstdout:\none\ntwo\nthree");
                    pair[0]["tool_calls"][0]["function"]["arguments"] =
                        json!(json!({"command":command}).to_string());
                    messages.extend(pair);
                };
                add_exec("a_before", "check A");
                add_exec("b_before", "check B");
                add_exec("shell_edit", "sed -i 's/old/new/' src/lib.rs");
                add_exec("a_after", "check A");
                if rerun_b {
                    add_exec("b_after", "check B");
                }
                for index in 0..35 {
                    add_exec(&format!("noise_{index}"), "git status --short");
                }
                messages[6]["content"] = json!(output);
                let expected = (!rerun_b).then(|| "check B".to_string());
                assert_eq!(
                    super::super::task_harness::benchmark_plan_pending_required_exec_command(
                        plan, &messages, None
                    ),
                    expected
                );
                super::super::prune_old_tool_results(&mut messages);
                prune_message_window(&mut messages);
                assert!(messages.len() <= MAX_CONTEXT_MESSAGES);
                assert!(messages
                    .iter()
                    .any(|message| message["tool_call_id"] == "shell_edit"));
                assert_eq!(
                    super::super::task_harness::benchmark_plan_pending_required_exec_command(
                        plan, &messages, None
                    ),
                    expected
                );
                assert_resume_intact(&messages);
            }
        }
    }
}
