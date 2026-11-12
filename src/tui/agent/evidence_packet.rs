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
    /// Runtime execution directory from the full correlated result.
    #[serde(skip)]
    pub(crate) exec_cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cargo_inputs: Option<crate::cargo_inputs::Receipt>,
    pub executed: Option<bool>,
    /// None when execution returned no recognized runtime status. Parsed from
    /// the full correlated result, independently of the bounded display text.
    pub succeeded: Option<bool>,
    pub edit_outcome: Option<crate::execution_evidence::FileEditOutcome>,
    /// Tool-result order, not assistant prose or call scheduling order.
    pub result_step: Option<usize>,
    pub freshness: super::current_evidence::Freshness,
}

impl ActionEvidence {
    pub(crate) fn matches_suggestion(&self, tool: &str, args: &serde_json::Value) -> bool {
        if self.tool != tool {
            return false;
        }
        if self.arguments == *args {
            return true;
        }
        // A native check selection retains the ID with its resolved command and
        // root. Other suggestions require exact argument equality.
        self.tool == "exec"
            && args.as_object().is_some_and(|object| object.len() == 1)
            && args["check_id"]
                .as_str()
                .is_some_and(|id| self.arguments["check_id"] == id)
            && self.arguments.as_object().is_some_and(|object| {
                object.len() == 3
                    && object
                        .get("command")
                        .is_some_and(serde_json::Value::is_string)
                    && object.get("cwd").is_some_and(serde_json::Value::is_string)
            })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CargoInputHandoff {
    tool_call_id: Option<String>,
    inputs: crate::cargo_inputs::Receipt,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ExecutionPacket {
    pub actions: Vec<ActionEvidence>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cargo_input_handoff: Option<CargoInputHandoff>,
    pub mutation_step: Option<usize>,
    pub verification_step: Option<usize>,
    pub pending_exact_checks: Vec<String>,
    pub available_checks: Vec<super::check_catalog::Check>,
}

impl ExecutionPacket {
    pub(crate) fn current_actions(&self) -> Vec<&ActionEvidence> {
        self.actions
            .iter()
            .filter(|action| action.freshness == super::current_evidence::Freshness::Current)
            .collect()
    }

    pub(crate) fn has_approval_barrier(&self) -> bool {
        self.actions.iter().any(|a| {
            a.result.as_deref().is_some_and(|s| {
                s.starts_with("REJECTED BY USER") || s.starts_with("Awaiting approval")
            })
        })
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
    let mut cargo_input_handoff = None;
    let mut result_step = 0;
    for message in messages {
        if message["role"] == "user"
            && message["origin"] == "runtime"
            && message["content"].as_str().is_some_and(|text| {
                text.starts_with("[implied_exec]\n") || text.starts_with("[implied_write_file]\n")
            })
        {
            // An implied action has no native call ID. It can supersede a previous
            // observation, but it never becomes an action or verification receipt.
            cargo_input_handoff =
                crate::cargo_inputs::Receipt::from_message(message).map(|inputs| {
                    CargoInputHandoff {
                        tool_call_id: None,
                        inputs,
                    }
                });
        }
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
                        exec_cwd: None,
                        cargo_inputs: None,
                        executed: None,
                        succeeded: None,
                        edit_outcome: None,
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
            if action.tool == "exec" && action.executed == Some(true) {
                action.exec_cwd = crate::execution_evidence::exec_cwd(result).map(str::to_owned);
            }
            action.edit_outcome =
                crate::execution_evidence::file_edit_outcome(&action.tool, result);
            action.succeeded = if pending {
                None
            } else if rejected {
                Some(false)
            } else {
                match action.tool.as_str() {
                    "exec" => crate::execution_evidence::exec_outcome(result),
                    "write_file" | "patch_file" | "apply_diff" => {
                        action.edit_outcome.map(|outcome| {
                            outcome != crate::execution_evidence::FileEditOutcome::Failed
                        })
                    }
                    // Read/search formats are not uniform. A returned result is
                    // evidence of execution, not by itself evidence of success.
                    _ => None,
                }
            };
            action.result = Some(
                if action.tool == "exec"
                    && action.executed == Some(true)
                    && action.succeeded == Some(false)
                {
                    super::failure_diagnostics::failure_excerpt(result, 2400)
                } else {
                    result.chars().take(2400).collect()
                },
            );
            action.cargo_inputs = crate::cargo_inputs::Receipt::from_message(message);
            if action.executed == Some(true)
                && (action.tool == "exec"
                    || action.edit_outcome
                        == Some(crate::execution_evidence::FileEditOutcome::Changed))
            {
                cargo_input_handoff = action.cargo_inputs.clone().map(|inputs| CargoInputHandoff {
                    tool_call_id: Some(action.call_id.clone()),
                    inputs,
                });
            }
            action.result_step = Some(result_step);
        }
    }
    let (_, mutation_step, build, behavioral, _) =
        restore_done_gate_from_messages(messages, test_cmd);
    super::current_evidence::mark(&mut actions, messages, test_cmd, mutation_step);
    ExecutionPacket {
        actions,
        cargo_input_handoff,
        mutation_step,
        verification_step: build.max(behavioral),
        available_checks: super::check_catalog::Catalog::from_root(
            crate::task_origin::root_user_text(messages),
        )
        .checks,
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
    let evidence = project(messages, test_cmd);
    if !evidence.pending_exact_checks.is_empty() {
        return Some(format!(
            "Missing current native exec proof: {}",
            evidence.pending_exact_checks.join("; ")
        ));
    }
    if let Some(issue) = super::requirement_proof::issue(messages, test_cmd) {
        return Some(issue);
    }
    report_issue(messages, summary, test_cmd)
}

/// Shared authored-report check for completion and high-confidence done advice.
/// An incomplete but truthful report remains subject to the same row checks;
/// this does not promote advice into completed work or invent missing proof.
pub(crate) fn report_issue(
    messages: &[serde_json::Value],
    summary: &str,
    test_cmd: Option<&str>,
) -> Option<String> {
    let root = crate::task_origin::root_user_text(messages);
    let report = super::final_handoff::report_context(root, messages, test_cmd);
    let authored = super::report_surface::authored(summary, &report);
    validate_authored_done_summary(root, authored, authored, messages, test_cmd)
        .err()
        .map(|e| e.to_string())
}

pub(crate) fn artifact_guidance(messages: &[serde_json::Value]) -> Option<String> {
    task_harness::benchmark_replay::guidance(crate::task_origin::root_user_text(messages))
}

/// Check an advisory completion against the same acceptance contract as Coder.
/// Display packets and Observer prose cannot add a plan or a native receipt.
pub(crate) fn acceptance_issue(
    messages: &[serde_json::Value],
    args: &serde_json::Value,
    test_cmd: Option<&str>,
    tool_root: Option<&str>,
) -> Option<String> {
    let messages = crate::task_origin::current_task_messages(messages);
    let plan = super::plan_resume::latest_valid(messages, |plan| validate_plan(plan).is_ok());
    let acceptance =
        super::acceptance_report::CurrentEvidence::from_native(messages, test_cmd, tool_root);
    let commands = &acceptance.commands;
    let rows =
        match super::acceptance_report::resolve_evidence(&args["acceptance_evidence"], &acceptance)
        {
            Ok(rows) => rows,
            Err(error) => {
                return Some(format!(
                    "Suggested done has invalid acceptance evidence: {error}"
                ))
            }
        };
    let observations = collect_observation_evidence(messages);
    // Advice may precede Coder's first valid plan. Check its citations now, but
    // leave plan admission to the real done gate on the ensuing Coder turn.
    if plan.is_none() {
        return rows
            .iter()
            .any(|row| {
                super::done_gate::preferred_known_command_for_evidence(
                    &row.command,
                    commands,
                    &observations,
                )
                .is_none()
            })
            .then(|| {
                format!(
                    "Suggested done has invalid acceptance evidence: {}",
                    governor_contract::done_evidence_unknown_command_message()
                )
            });
    }
    validate_done_acceptance(
        plan.as_ref(),
        &parse_string_list_arg(&args["completed_acceptance"]),
        &parse_string_list_arg(&args["remaining_acceptance"]),
        &rows,
        commands,
        &observations,
    )
    .err()
    .map(|error| format!("Suggested done has invalid acceptance evidence: {error}"))
}
