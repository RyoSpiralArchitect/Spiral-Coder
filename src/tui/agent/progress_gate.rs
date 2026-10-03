use super::{
    canonicalize_tool_call_command, classify_verify_level, compact_one_line,
    infer_fix_existing_focus, is_observation_tool, is_src_read_file_tool_call, observation_history,
    parse_exec_command_from_args, ArtifactMode, FixExistingFocus, RecoveryStage, TaskHarness,
    TaskLane, ToolCallData, Value,
};
use crate::execution_evidence::exec_may_have_run;

pub(in super::super) fn build_progress_gate_block(
    harness: TaskHarness,
    tc: &ToolCallData,
    messages: &[Value],
    recovery_stage: Option<RecoveryStage>,
    test_cmd: Option<&str>,
    last_mutation_step: Option<usize>,
    file_tool_consec_failures: usize,
) -> Option<String> {
    if recovery_stage != Some(RecoveryStage::Fix) {
        return None;
    }
    if harness.artifact_mode == ArtifactMode::ObserveOnly {
        return None;
    }

    let history = observation_history(messages);
    if file_tool_consec_failures > 0 && is_src_read_file_tool_call(tc) {
        return None;
    }
    let verify_without_mutation = harness.artifact_mode == ArtifactMode::ExistingFiles
        && last_mutation_step.is_none()
        && tc.name == "exec"
        && parse_exec_command_from_args(tc.arguments.as_str())
            .and_then(|command| classify_verify_level(command.as_str(), test_cmd))
            .is_some();
    if !verify_without_mutation && !is_observation_tool(tc.name.as_str()) {
        return None;
    }
    if verify_without_mutation && !has_executed_command(messages, tc) {
        // The first baseline check can locate the failure. Unrelated successful
        // observations and rejected attempts cannot make it a verification rerun.
        return None;
    }

    let attempted = canonicalize_tool_call_command(tc.name.as_str(), tc.arguments.as_str())
        .unwrap_or_else(|| {
            format!(
                "{}({})",
                tc.name,
                compact_one_line(tc.arguments.as_str(), 120)
            )
        });
    if !verify_without_mutation {
        let same_successes = history.by_command.get(&attempted).copied().unwrap_or(0);
        let allow_first_target_read = harness.artifact_mode == ArtifactMode::ExistingFiles
            && tc.name == "read_file"
            && same_successes == 0;
        if allow_first_target_read {
            // Listing the project and reading its manifest do not inspect the
            // implementation. Focus inference is a hint, not proof it was read.
            return None;
        }
        if same_successes == 0 && history.total_successes < 2 {
            return None;
        }
    } else if history.total_successes < 2 {
        return None;
    }

    let fix_focus = infer_fix_existing_focus(messages);
    if let Some(FixExistingFocus::ReadImplementation(path)) = fix_focus.as_ref() {
        return Some(format!(
            "[Progress Gate]\n\
Task lane: {}\n\
Recovery stage is already `fix`.\n\
Attempted next action: {}\n\
Successful observation commands so far: {}\n\
This is stalled progress, not forward motion.\n\
Required now: read `{}` now to inspect the implementation before patching.\n\
Do NOT rerun verification or widen search until `{}` is read.\n\
Do NOT call the same observation tool on the same target again until the target changes or a mutation lands.",
            harness.lane_label(),
            compact_one_line(&attempted, 180),
            history.total_successes,
            compact_one_line(path, 140),
            compact_one_line(path, 140),
        ));
    }

    let next_action = match harness.artifact_mode {
        ArtifactMode::ExistingFiles => match harness.lane {
            TaskLane::BenchmarkPlan => {
                "patch the smallest benchmark spec/fixture update now with `patch_file` or `apply_diff`".to_string()
            }
            _ => match fix_focus.as_ref() {
                Some(FixExistingFocus::PatchImplementation(path)) => format!(
                    "apply the smallest edit now with `patch_file` or `apply_diff` on `{}`",
                    compact_one_line(path, 140)
                ),
                _ => "apply the smallest edit now with `patch_file` or `apply_diff`".to_string(),
            },
        },
        ArtifactMode::NewFiles => {
            "create the requested file now with `write_file` or a minimal `exec`".to_string()
        }
        ArtifactMode::NewRepo => {
            "create the requested repo/project artifact now with `write_file` or `exec`".to_string()
        }
        ArtifactMode::ObserveOnly => unreachable!(),
    };
    let verify_hint = if verify_without_mutation {
        test_cmd
            .filter(|cmd| !cmd.trim().is_empty())
            .map(|cmd| {
                format!(
                    "Do NOT run `{}` again before a mutation lands. Read the strongest target or patch now.\n",
                    compact_one_line(cmd, 140)
                )
            })
            .unwrap_or_else(|| {
                "Do NOT rerun verification before a mutation lands. Read the strongest target or patch now.\n".to_string()
            })
    } else {
        test_cmd
            .filter(|cmd| !cmd.trim().is_empty())
            .map(|cmd| format!("If the artifact is already present, run the configured verification command now: `{}`.\n", compact_one_line(cmd, 140)))
            .unwrap_or_else(|| {
                "If you believe the artifact is already present, run a real command that proves it before `done`.\n".to_string()
            })
    };

    Some(format!(
        "[Progress Gate]\n\
Task lane: {}\n\
Recovery stage is already `fix`.\n\
Attempted next action: {}\n\
Successful observation commands so far: {}\n\
This is stalled progress, not forward motion.\n\
Required now: {}.\n\
{}\
Do NOT call the same observation tool on the same target again until the target changes or a mutation lands.",
        harness.lane_label(),
        compact_one_line(&attempted, 180),
        history.total_successes,
        next_action,
        verify_hint
    ))
}

fn has_executed_command(messages: &[Value], attempted: &ToolCallData) -> bool {
    let Some(command) = parse_exec_command_from_args(&attempted.arguments) else {
        return false;
    };
    let mut matching_ids = std::collections::HashSet::new();
    for message in messages {
        match message.get("role").and_then(Value::as_str) {
            Some("assistant") => {
                for call in message
                    .get("tool_calls")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    let function = &call["function"];
                    if function["name"].as_str() == Some("exec")
                        && function["arguments"]
                            .as_str()
                            .and_then(parse_exec_command_from_args)
                            .is_some_and(|previous| previous.trim() == command.trim())
                    {
                        if let Some(id) = call["id"].as_str() {
                            matching_ids.insert(id);
                        }
                    }
                }
            }
            Some("tool") => {
                if message["tool_call_id"]
                    .as_str()
                    .is_some_and(|id| matching_ids.remove(id))
                    && exec_may_have_run(message["content"].as_str().unwrap_or(""))
                {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn tool(messages: &mut Vec<Value>, name: &str, args: Value, content: &str) {
        let id = format!("call_{}", messages.len());
        messages.push(json!({"role":"assistant", "tool_calls":[{
            "id":id, "type":"function", "function": {
                "name":name, "arguments":args.to_string()
            }
        }]}));
        messages.push(json!({"role":"tool", "tool_call_id":id, "content":content}));
    }

    fn observed_project(manifest: &str) -> Vec<Value> {
        let mut messages = Vec::new();
        tool(
            &mut messages,
            "list_dir",
            json!({"dir":"."}),
            "[list_dir] src/",
        );
        tool(
            &mut messages,
            "read_file",
            json!({"path":manifest}),
            "[manifest] package metadata",
        );
        messages
    }

    fn blocked(messages: &[Value], name: &str, arguments: Value) -> Option<String> {
        build_progress_gate_block(
            TaskHarness {
                lane: TaskLane::FixExisting,
                artifact_mode: ArtifactMode::ExistingFiles,
            },
            &ToolCallData {
                id: "next".into(),
                name: name.into(),
                arguments: arguments.to_string(),
            },
            messages,
            Some(RecoveryStage::Fix),
            Some("cargo test 2>&1"),
            None,
            0,
        )
    }

    #[test]
    fn resumed_progress_allows_unread_implementation_after_project_orientation() {
        for (manifest, source) in [
            ("Cargo.toml", "src/lib.rs"),
            ("package.json", "lib/handler.js"),
            ("pyproject.toml", "app/handler.py"),
        ] {
            let mut messages = observed_project(manifest);
            assert!(
                blocked(&messages, "read_file", json!({"path":source})).is_none(),
                "first implementation read must not depend on a heuristic selecting the target"
            );
            tool(
                &mut messages,
                "read_file",
                json!({"path":source}),
                "[source] implementation",
            );
            assert!(
                blocked(&messages, "read_file", json!({"path":source})).is_some(),
                "unchanged repeated reads still need a different next action"
            );
        }
    }

    #[test]
    fn resumed_progress_allows_first_baseline_check_but_blocks_unchanged_rerun() {
        let command = "cargo test 2>&1";
        let mut messages = observed_project("Cargo.toml");
        assert!(
            blocked(&messages, "exec", json!({"command":command})).is_none(),
            "first baseline execution cannot be called a rerun"
        );
        tool(
            &mut messages,
            "exec",
            json!({"command":command}),
            "GOVERNOR BLOCKED [Plan Gate]",
        );
        assert!(
            blocked(&messages, "exec", json!({"command":command})).is_none(),
            "a pre-execution rejection is not a baseline run"
        );
        tool(
            &mut messages,
            "exec",
            json!({"command":command}),
            "FAILED (exit_code: 1)\nstdout:\ntest failed",
        );
        assert!(blocked(&messages, "exec", json!({"command":command})).is_some());
    }
}
