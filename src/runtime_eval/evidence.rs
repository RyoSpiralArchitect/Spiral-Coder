//! Evidence used by eval checks, distinct from historical success telemetry.
use crate::execution_evidence::{auto_test_succeeded, exec_succeeded, file_edit_succeeded};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Evidence {
    pub successful_commands: Vec<String>,
    pub verified_commands: Vec<String>,
    pub auto_test_pass_count: usize,
    pub fresh_auto_test_pass_count: usize,
}

pub(super) fn commands_match(actual: &str, required: &str) -> bool {
    !actual.trim().is_empty() && actual.trim() == required.trim()
}

pub(super) fn collect(
    messages: &[Value],
    required_commands: &[String],
    auto_test_command: Option<&str>,
) -> Evidence {
    let mut pending = BTreeMap::<String, (String, Option<String>)>::new();
    let mut verified = BTreeSet::<String>::new();
    let mut evidence = Evidence::default();
    for message in messages {
        match message["role"].as_str() {
            Some("assistant") => {
                for call in message["tool_calls"].as_array().into_iter().flatten() {
                    let Some(id) = call["id"].as_str().filter(|id| !id.is_empty()) else {
                        continue;
                    };
                    let name = call["function"]["name"].as_str().unwrap_or("");
                    let command = call["function"]["arguments"]
                        .as_str()
                        .and_then(|args| serde_json::from_str::<Value>(args).ok())
                        .and_then(|args| args["command"].as_str().map(str::to_string));
                    pending.insert(id.to_string(), (name.to_string(), command));
                }
            }
            Some("tool") => {
                let Some(id) = message["tool_call_id"].as_str() else {
                    continue;
                };
                let Some((name, command)) = pending.remove(id) else {
                    continue;
                };
                let content = message["content"].as_str().unwrap_or("").trim_start();
                if file_edit_succeeded(&name, content) {
                    verified.clear();
                    evidence.fresh_auto_test_pass_count = 0;
                    if auto_test_succeeded(&name, content) {
                        evidence.auto_test_pass_count += 1;
                        evidence.fresh_auto_test_pass_count = 1;
                    }
                } else if name == "exec" {
                    let Some(command) = command.filter(|command| !command.trim().is_empty()) else {
                        continue;
                    };
                    let succeeded = exec_succeeded(content);
                    if succeeded {
                        evidence.successful_commands.push(command.clone());
                    }
                    let required = required_commands
                        .iter()
                        .find(|required| commands_match(&command, required));
                    if let Some(required) = required {
                        if succeeded {
                            verified.insert(required.clone());
                        } else if crate::execution_evidence::exec_may_have_run(content) {
                            verified.remove(required);
                        }
                    } else if crate::tui::agent::exec_may_mutate_for_evaluation(
                        &command,
                        auto_test_command,
                    ) && crate::execution_evidence::exec_may_have_run(content)
                    {
                        // A failed shell action can still have changed files before failing.
                        verified.clear();
                        evidence.fresh_auto_test_pass_count = 0;
                    }
                    if !succeeded
                        && crate::execution_evidence::exec_may_have_run(content)
                        && auto_test_command.is_some_and(|test| commands_match(&command, test))
                    {
                        evidence.fresh_auto_test_pass_count = 0;
                    }
                }
            }
            _ => {}
        }
    }
    evidence.verified_commands = verified.into_iter().collect();
    evidence
}

#[cfg(test)]
mod tests;
