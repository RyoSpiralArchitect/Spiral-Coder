//! Shared UI projection of native execution evidence. Never infer execution
//! from assistant prose or expose opaque provider metadata in advisory packets.
use super::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ActionEvidence {
    pub call_id: String,
    pub tool: String,
    pub arguments: serde_json::Value,
    pub result: Option<String>,
    pub executed: Option<bool>,
    pub succeeded: Option<bool>,
    /// Tool-result order, not assistant prose or call scheduling order.
    pub result_step: Option<usize>,
    pub freshness: super::current_evidence::Freshness,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ExecutionPacket {
    pub actions: Vec<ActionEvidence>,
    pub mutation_step: Option<usize>,
    pub verification_step: Option<usize>,
    pub pending_exact_checks: Vec<String>,
}

impl ExecutionPacket {
    pub(crate) fn current_actions(&self) -> Vec<&ActionEvidence> {
        self.actions
            .iter()
            .filter(|action| action.freshness == super::current_evidence::Freshness::Current)
            .collect()
    }

    pub(crate) fn approval_rejections(&self) -> Vec<&ActionEvidence> {
        self.actions
            .iter()
            .filter(|action| {
                action
                    .result
                    .as_deref()
                    .is_some_and(|result| result.starts_with("REJECTED BY USER"))
            })
            .collect()
    }
}

pub(crate) fn project(messages: &[serde_json::Value], test_cmd: Option<&str>) -> ExecutionPacket {
    let messages = crate::task_origin::current_task_messages(messages);
    let mut actions = Vec::<ActionEvidence>::new();
    let mut result_step = 0;
    for message in messages {
        if let Some(calls) = message["tool_calls"].as_array() {
            for call in calls {
                if let (Some(id), Some(name)) =
                    (call["id"].as_str(), call["function"]["name"].as_str())
                {
                    actions.push(ActionEvidence {
                        call_id: id.into(),
                        tool: name.into(),
                        arguments: call["function"]["arguments"]
                            .as_str()
                            .and_then(|s| serde_json::from_str(s).ok())
                            .unwrap_or(serde_json::Value::Null),
                        result: None,
                        executed: None,
                        succeeded: None,
                        result_step: None,
                        freshness: super::current_evidence::Freshness::Pending,
                    });
                }
            }
        }
        if message["role"] == "tool" {
            result_step += 1;
            let Some(action) = actions
                .iter_mut()
                .rev()
                .find(|a| message["tool_call_id"] == a.call_id && a.result.is_none())
            else {
                continue;
            };
            let Some(result) = message["content"].as_str() else {
                continue;
            };
            let rejected =
                result.starts_with("GOVERNOR BLOCKED") || result.starts_with("REJECTED BY USER");
            let pending = result.starts_with("Awaiting approval");
            action.executed = Some(!rejected && !pending);
            action.succeeded = if pending {
                None
            } else if rejected {
                Some(false)
            } else {
                match action.tool.as_str() {
                    "exec" => Some(crate::execution_evidence::exec_succeeded(result)),
                    "write_file" | "patch_file" | "apply_diff" => Some(
                        crate::execution_evidence::file_edit_succeeded(&action.tool, result),
                    ),
                    // Read/search formats are not uniform. A returned result is
                    // evidence of execution, not by itself evidence of success.
                    _ => None,
                }
            };
            action.result = Some(result.chars().take(2400).collect());
            action.result_step = Some(result_step);
        }
    }
    let (_, mutation_step, build, behavioral, _) =
        restore_done_gate_from_messages(messages, test_cmd);
    super::current_evidence::mark(&mut actions, messages, test_cmd, mutation_step);
    ExecutionPacket {
        actions,
        mutation_step,
        verification_step: build.max(behavioral),
        pending_exact_checks: task_harness::benchmark_proof::pending_commands(
            messages,
            crate::task_origin::root_user_text(messages),
            test_cmd,
        ),
    }
}

pub(crate) fn completion_issue(
    messages: &[serde_json::Value],
    summary: &str,
    test_cmd: Option<&str>,
) -> Option<String> {
    let root = crate::task_origin::root_user_text(messages);
    let evidence = project(messages, test_cmd);
    if !evidence.pending_exact_checks.is_empty() {
        return Some(format!(
            "Missing current native exec proof: {}",
            evidence.pending_exact_checks.join("; ")
        ));
    }
    validate_authored_done_summary(root, summary, summary, messages, test_cmd)
        .err()
        .map(|e| e.to_string())
}

pub(crate) fn artifact_guidance(messages: &[serde_json::Value]) -> Option<String> {
    task_harness::benchmark_replay::guidance(crate::task_origin::root_user_text(messages))
}
