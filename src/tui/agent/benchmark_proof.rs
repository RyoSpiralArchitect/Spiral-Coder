//! Explicit command evidence for approved Observer benchmark plans.
//!
//! Keep this separate from diagnostic command signatures: shell command case,
//! quoting, and internal whitespace can change what a check actually verifies.
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn commands_match(actual: &str, required: &str) -> bool {
    !actual.trim().is_empty() && actual.trim() == required.trim()
}

fn required_commands(root_user_text: &str) -> Vec<String> {
    let mut commands = Vec::new();
    let mut in_plan = false;
    let mut in_checks = false;
    for line in root_user_text.lines() {
        let line = line.trim();
        if line.starts_with("<observer_benchmark_plan") {
            in_plan = true;
            in_checks = false;
            continue;
        }
        if line.starts_with("</observer_benchmark_plan>") {
            in_plan = false;
            in_checks = false;
            continue;
        }
        if !in_plan {
            continue;
        }
        if line.eq_ignore_ascii_case("required_checks:") {
            in_checks = true;
            continue;
        }
        if !in_checks {
            continue;
        }
        if let Some(item) = line.strip_prefix('-') {
            let item = item.trim();
            let command = item
                .strip_prefix('`')
                .and_then(|item| item.strip_suffix('`'))
                .unwrap_or(item)
                .trim();
            if !command.is_empty() && !commands.iter().any(|seen| seen == command) {
                commands.push(command.to_string());
            }
        } else if !line.is_empty() {
            in_checks = false;
        }
    }
    commands
}

fn exec_succeeded(content: &str) -> bool {
    let header = content.lines().next().unwrap_or("");
    let Some(suffix) = header.strip_prefix("OK (exit_code: 0)") else {
        return false;
    };
    suffix.is_empty()
        || suffix
            .strip_prefix(" [pruned ")
            .and_then(|suffix| suffix.strip_suffix("L]"))
            .is_some_and(|count| {
                !count.is_empty() && count.bytes().all(|byte| byte.is_ascii_digit())
            })
}

/// Keep proof and invalidation exchanges together when the message window shrinks.
pub(super) fn protected_call_ids(messages: &[Value]) -> BTreeSet<String> {
    let root_user_text = messages
        .iter()
        .rev()
        .find(|message| message["role"].as_str() == Some("user"))
        .and_then(|message| message["content"].as_str())
        .unwrap_or("");
    let required = required_commands(root_user_text);
    if required.is_empty() {
        return BTreeSet::new();
    }
    let mut ids = BTreeSet::new();
    for message in messages
        .iter()
        .filter(|message| message["role"].as_str() == Some("assistant"))
    {
        for call in message["tool_calls"].as_array().into_iter().flatten() {
            let name = call["function"]["name"].as_str().unwrap_or("");
            let is_check = name == "exec"
                && call["function"]["arguments"]
                    .as_str()
                    .and_then(|args| serde_json::from_str::<Value>(args).ok())
                    .and_then(|args| args["command"].as_str().map(str::to_string))
                    .is_some_and(|command| {
                        required
                            .iter()
                            .any(|required| commands_match(&command, required))
                    });
            if matches!(name, "write_file" | "patch_file" | "apply_diff") || is_check {
                if let Some(id) = call["id"].as_str() {
                    ids.insert(id.to_string());
                }
            }
        }
    }
    ids
}

/// Return the first check lacking successful evidence after the latest file edit.
/// Results are correlated by tool-call ID; failed reruns revoke older successes.
pub(super) fn pending_command(messages: &[Value], root_user_text: &str) -> Option<String> {
    let required = required_commands(root_user_text);
    let mut pending = BTreeMap::<String, (String, Option<String>)>::new();
    let mut passed = BTreeSet::<String>::new();
    for message in messages {
        match message.get("role").and_then(Value::as_str) {
            Some("assistant") => {
                let Some(calls) = message.get("tool_calls").and_then(Value::as_array) else {
                    continue;
                };
                for call in calls {
                    let Some(id) = call
                        .get("id")
                        .and_then(Value::as_str)
                        .filter(|id| !id.is_empty())
                    else {
                        continue;
                    };
                    let Some(function) = call.get("function") else {
                        continue;
                    };
                    let name = function.get("name").and_then(Value::as_str).unwrap_or("");
                    let command = function
                        .get("arguments")
                        .and_then(Value::as_str)
                        .and_then(|args| serde_json::from_str::<Value>(args).ok())
                        .and_then(|args| {
                            args.get("command")
                                .and_then(Value::as_str)
                                .map(str::to_string)
                        });
                    pending.insert(id.to_string(), (name.to_string(), command));
                }
            }
            Some("tool") => {
                let Some(id) = message.get("tool_call_id").and_then(Value::as_str) else {
                    continue;
                };
                let Some((name, command)) = pending.remove(id) else {
                    continue;
                };
                let content = message
                    .get("content")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .trim_start();
                if matches!(name.as_str(), "write_file" | "patch_file" | "apply_diff") {
                    // The edit remains a mutation even when its appended auto-test fails.
                    if content.starts_with("OK: wrote '")
                        || content.starts_with("OK: patched '")
                        || content.starts_with("OK: applied ")
                    {
                        passed.clear();
                    }
                } else if name == "exec" {
                    let Some(command) = command else { continue };
                    let Some(required_command) = required
                        .iter()
                        .find(|required| commands_match(&command, required))
                    else {
                        continue;
                    };
                    // Inspect the runtime's status header, not strings printed by the command.
                    if exec_succeeded(content) {
                        passed.insert(required_command.clone());
                    } else {
                        passed.remove(required_command);
                    }
                }
            }
            _ => {}
        }
    }
    required
        .into_iter()
        .find(|command| !passed.contains(command))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn plan(commands: &[&str]) -> String {
        format!("<observer_benchmark_plan>\nrequired_checks:\n{}\nsuccess_criteria:\n- verified\n</observer_benchmark_plan>",
            commands.iter().map(|command| format!("- {command}")).collect::<Vec<_>>().join("\n"))
    }

    fn result(messages: &mut Vec<Value>, name: &str, command: &str, content: &str) {
        let id = format!("call_{}", messages.len());
        messages.push(json!({"role": "assistant", "tool_calls": [{
            "id": id, "type": "function", "function": {
                "name": name, "arguments": json!({"command": command}).to_string()
            }
        }]}));
        messages.push(json!({"role": "tool", "tool_call_id": id, "content": content}));
    }

    #[test]
    fn every_required_check_needs_its_own_success() {
        let plan = plan(&["cargo fmt --check", "cargo test", "cargo test"]);
        let mut messages = vec![];
        assert_eq!(
            pending_command(&messages, &plan).as_deref(),
            Some("cargo fmt --check")
        );
        result(
            &mut messages,
            "exec",
            "cargo fmt --check",
            "OK (exit_code: 0)",
        );
        assert_eq!(
            pending_command(&messages, &plan).as_deref(),
            Some("cargo test")
        );
        result(&mut messages, "exec", "cargo test", "FAILED (exit_code: 1)");
        assert_eq!(
            pending_command(&messages, &plan).as_deref(),
            Some("cargo test")
        );
        result(&mut messages, "exec", "cargo test", "OK (exit_code: 0)");
        assert_eq!(pending_command(&messages, &plan), None);
        result(&mut messages, "exec", "cargo test", "FAILED (exit_code: 1)");
        assert_eq!(
            pending_command(&messages, &plan).as_deref(),
            Some("cargo test")
        );
    }

    #[test]
    fn edits_invalidate_all_earlier_checks_even_if_auto_test_fails() {
        for (tool, content) in [
            ("write_file", "OK: wrote 'src/lib.rs'"),
            (
                "patch_file",
                "OK: patched 'src/lib.rs'\n[auto-test] ✗ FAILED (exit 1)",
            ),
            ("apply_diff", "OK: applied diff to 'src/lib.rs'"),
        ] {
            let plan = plan(&["check A", "check B"]);
            let mut messages = vec![];
            result(&mut messages, "exec", "check A", "OK (exit_code: 0)");
            result(&mut messages, "exec", "check B", "OK (exit_code: 0)");
            result(&mut messages, tool, "", content);
            assert_eq!(
                pending_command(&messages, &plan).as_deref(),
                Some("check A")
            );
            result(&mut messages, "exec", "check A", "OK (exit_code: 0)");
            assert_eq!(
                pending_command(&messages, &plan).as_deref(),
                Some("check B")
            );
            result(&mut messages, "exec", "check B", "OK (exit_code: 0)");
            assert_eq!(pending_command(&messages, &plan), None);
        }
    }

    #[test]
    fn failed_edits_and_reads_do_not_discard_proof() {
        let plan = plan(&["cargo test"]);
        let mut messages = vec![];
        result(&mut messages, "exec", "cargo test", "OK (exit_code: 0)");
        result(
            &mut messages,
            "patch_file",
            "",
            "Error: search text not found",
        );
        result(
            &mut messages,
            "read_file",
            "",
            "OK: patched 'quoted output'",
        );
        result(&mut messages, "exec", "git status", "OK (exit_code: 0)");
        assert_eq!(pending_command(&messages, &plan), None);
    }

    #[test]
    fn preserves_command_case_and_quoted_whitespace() {
        let plan = plan(&["`grep -q 'A  B' Spec.json`"]);
        let mut messages = vec![];
        for command in [
            "grep -q 'a  b' Spec.json",
            "grep -q 'A B' Spec.json",
            "grep -q 'A  B' spec.json",
        ] {
            result(&mut messages, "exec", command, "OK (exit_code: 0)");
            assert!(pending_command(&messages, &plan).is_some());
        }
        result(
            &mut messages,
            "exec",
            "  grep -q 'A  B' Spec.json  ",
            "OK (exit_code: 0)",
        );
        assert_eq!(pending_command(&messages, &plan), None);
    }

    #[test]
    fn only_correlated_runtime_status_is_proof() {
        let plan = plan(&["cargo test"]);
        let mut messages =
            vec![json!({"role": "tool", "tool_call_id": "orphan", "content": "OK (exit_code: 0)"})];
        for content in [
            "GOVERNOR BLOCKED\nOK (exit_code: 0)",
            "FAILED (exit_code: 1)\nstdout:\nOK (exit_code: 0)",
            "OK: no exit status",
        ] {
            result(&mut messages, "exec", "cargo test", content);
            assert!(pending_command(&messages, &plan).is_some());
        }
        result(
            &mut messages,
            "exec",
            "cargo test",
            "OK (exit_code: 0)\nstdout:\nFAILED is an expected fixture string",
        );
        assert_eq!(pending_command(&messages, &plan), None);
    }

    #[test]
    fn reads_only_required_checks_inside_plan() {
        let text = "required_checks:\n- unrelated-before\n<observer_benchmark_plan required=\"true\">\nrequired_checks:\n- `cargo test`\n- \n- cargo fmt --check\n</observer_benchmark_plan>\n- unrelated-after";
        assert_eq!(required_commands(text), ["cargo test", "cargo fmt --check"]);
    }

    #[test]
    fn runtime_pruning_preserves_many_required_check_successes() {
        let commands = (0..8)
            .map(|index| format!("check {index}"))
            .collect::<Vec<_>>();
        let plan = plan(&commands.iter().map(String::as_str).collect::<Vec<_>>());
        let mut messages = vec![];
        for command in &commands {
            result(
                &mut messages,
                "exec",
                command,
                "OK (exit_code: 0)\nstdout:\nline one\nline two\nline three",
            );
        }
        super::super::super::prune_old_tool_results(&mut messages);
        assert!(messages.iter().any(|message| message["content"]
            .as_str()
            .is_some_and(|content| content.ends_with("[pruned 5L]"))));
        assert_eq!(pending_command(&messages, &plan), None);
        assert!(!exec_succeeded("OK (exit_code: 0) unexpected output"));
        assert!(!exec_succeeded("OK (exit_code: 0) [pruned forgedL]"));
    }
}
