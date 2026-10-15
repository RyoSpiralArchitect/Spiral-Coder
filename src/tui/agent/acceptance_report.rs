//! Lossless handoff for authored completion: stable criterion references plus
//! currently citable native receipts. Never guess which criterion a check proves.
use super::*;

pub(super) fn criteria(plan: Option<&PlanBlock>) -> Vec<serde_json::Value> {
    plan.into_iter().flat_map(|plan| plan.acceptance_criteria.iter().enumerate()
        .map(|(idx, criterion)| json!({"reference":format!("acceptance {}", idx + 1),"criterion":criterion})))
        .collect()
}

pub(super) fn current_commands(
    messages: &[serde_json::Value],
    test_cmd: Option<&str>,
) -> Vec<String> {
    let messages = crate::task_origin::current_task_messages(messages);
    let evidence = evidence_packet::project(messages, test_cmd);
    let mut commands = Vec::new();
    for action in evidence.current_actions() {
        let result = action.result.as_deref().unwrap_or("");
        let command = if action.tool == "exec" && action.succeeded == Some(true) {
            action.arguments["command"].as_str().map(str::to_owned)
        } else if action.tool != "exec"
            && action.executed == Some(true)
            && non_exec_tool_succeeded(result)
        {
            canonicalize_tool_call_command(&action.tool, &action.arguments.to_string())
        } else {
            None
        };
        if let Some(command) = command.filter(|c| !c.is_empty()) {
            if !commands.contains(&command) {
                commands.push(command);
            }
        }
    }
    // Automatic tests are separate receipts and retain their configured command;
    // accept them only if the existing proof restorer still considers them fresh.
    let (_, mutation, build, behavioral, _) = restore_done_gate_from_messages(messages, test_cmd);
    if build
        .max(behavioral)
        .is_some_and(|step| step >= mutation.unwrap_or(0))
    {
        if let Some(command) = test_cmd {
            let fresh_auto = evidence.current_actions().iter().any(|action| {
                action
                    .result_step
                    .is_some_and(|step| step >= mutation.unwrap_or(0))
                    && messages.iter().any(|message| {
                        message["role"] == "tool"
                            && message["tool_call_id"] == action.call_id
                            && crate::execution_evidence::auto_test_succeeded(
                                &action.tool,
                                message["content"].as_str().unwrap_or(""),
                            )
                    })
            });
            if fresh_auto && !commands.iter().any(|known| known == command) {
                commands.push(command.to_string());
            }
        }
    }
    commands
}

pub(super) fn hint(plan: Option<&PlanBlock>, commands: &[String]) -> String {
    format!("\n[Authored acceptance mapping]\nKeep your own completed/remaining decisions and map evidence to the exact criterion reference below. Supply exactly one evidence row for each completed criterion, using a current command exactly as recorded. Do not invent a shorter shell command. If one criterion needs multiple checks, describe that in summary and cite the decisive recorded check; all required exact checks remain independently enforced. The runtime will not reassign evidence or silently move a criterion to remaining. Repair this report using retained proof; do not rerun passed checks just to repair its format.\n{}\n", json!({"criteria":criteria(plan),"current_commands":commands}))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn exchange(
        messages: &mut Vec<serde_json::Value>,
        id: &str,
        name: &str,
        args: serde_json::Value,
        output: &str,
    ) {
        messages.extend([json!({"role":"assistant","tool_calls":[{"id":id,"function":{"name":name,"arguments":args.to_string()}}]}), json!({"role":"tool","tool_call_id":id,"content":output})]);
    }
    #[test]
    fn reporting_commands_exclude_old_reads_failed_reruns_and_previous_tasks() {
        let mut messages = vec![json!({"role":"user","content":"Repair files and verify"})];
        exchange(
            &mut messages,
            "old",
            "read_file",
            json!({"path":"receipt.txt"}),
            "[receipt.txt] (1 lines, 5 bytes)\nstale",
        );
        exchange(
            &mut messages,
            "edit",
            "patch_file",
            json!({"path":"source.txt","search":"old","replace":"new"}),
            "OK: patched 'source.txt'",
        );
        exchange(
            &mut messages,
            "pass",
            "exec",
            json!({"command":"cargo test"}),
            "OK (exit_code: 0)",
        );
        exchange(
            &mut messages,
            "fail",
            "exec",
            json!({"command":"cargo test"}),
            "FAILED (exit_code: 1)",
        );
        let commands = current_commands(&messages, None);
        assert!(!commands
            .iter()
            .any(|c| c.contains("receipt.txt") || c == "cargo test"));
        exchange(
            &mut messages,
            "fresh",
            "exec",
            json!({"command":"cargo test"}),
            "OK (exit_code: 0)",
        );
        assert!(current_commands(&messages, None).contains(&"cargo test".into()));
        messages.push(json!({"role":"user","content":"Different task"}));
        assert!(current_commands(&messages, None).is_empty());
    }
    #[test]
    fn exact_commands_survive_parsing_without_truncation() {
        let command = format!(
            "grep -q '{}' file.json && printf done > receipt.txt",
            "A".repeat(250)
        );
        let rows = parse_done_acceptance_evidence(
            &json!([{"criterion":"acceptance 1","command":command}]),
        );
        assert_eq!(rows[0].command, command);
    }

    #[test]
    fn shell_proof_cannot_be_shortened_or_case_folded_to_another_command() {
        let plan = PlanBlock {
            goal: "Report proof".into(),
            steps: vec!["Verify".into(), "Report".into()],
            acceptance_criteria: vec!["Receipt is current".into()],
            risks: "stale proof".into(),
            assumptions: "native evidence".into(),
        };
        let command = "grep -q 'Case' spec.json && printf current > receipt.txt";
        for candidate in [
            "grep -q 'Case' spec.json",
            "grep -q 'case' spec.json && printf current > receipt.txt",
            command,
        ] {
            let result = validate_done_acceptance(
                Some(&plan),
                &["acceptance 1".into()],
                &[],
                &[DoneAcceptanceEvidence {
                    criterion: "acceptance 1".into(),
                    command: candidate.into(),
                }],
                &[command.into()],
                &ObservationEvidence::default(),
            );
            assert_eq!(result.is_ok(), candidate == command, "{candidate}");
        }
    }
}
