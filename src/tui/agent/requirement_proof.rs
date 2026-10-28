//! Human-task requirements, independent of the model's editable plan.
//! Derived entirely from the root request and correlated native exchanges.
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Contract {
    version: u32,
    pub requirements: Vec<Requirement>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Requirement {
    id: String,
    description: String,
    pub artifacts: Vec<Artifact>,
    pub checks: Vec<Check>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Artifact {
    pub path: String,
    kind: ArtifactKind,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum ArtifactKind {
    Source,
    Test,
    Documentation,
    Spec,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Check {
    pub command: String,
    #[serde(default)]
    baseline_failure: bool,
}

#[derive(Debug, Default, Serialize)]
pub(crate) struct Status {
    pub declared: bool,
    pub pending: Vec<String>,
    pub pending_baselines: Vec<String>,
    pub pending_artifact_paths: Vec<String>,
    requirements: Vec<Value>,
}

// Only a standalone block in the original human task is authoritative. Quoted
// examples, model plans, runtime continuations and Observer advice cannot add one.
pub(super) fn parse(root: &str) -> Result<Option<Contract>> {
    let mut fence = None;
    let mut body = None;
    let mut closed = false;
    let mut text = String::new();
    for line in root.lines() {
        let line = line.trim();
        if (body.is_none() || closed) && super::prose_context::is_fence_line(line) {
            let marker = super::prose_context::fence_marker(line);
            if fence == Some(marker) {
                fence = None;
            } else if fence.is_none() {
                fence = Some(marker);
            }
            continue;
        }
        if fence.is_some() {
            continue;
        }
        if line == "<task_requirements>" {
            ensure!(
                body.is_none(),
                "only one task_requirements block is allowed"
            );
            body = Some(());
        } else if line == "</task_requirements>" {
            ensure!(
                body.is_some() && !closed,
                "unexpected task_requirements closing tag"
            );
            closed = true;
        } else if body.is_some() && !closed {
            text.push_str(line);
            text.push('\n');
        }
    }
    if body.is_none() {
        return Ok(None);
    }
    ensure!(closed, "unclosed task_requirements block");
    let contract: Contract =
        serde_json::from_str(&text).context("invalid task_requirements JSON")?;
    ensure!(
        contract.version == 1,
        "unsupported task_requirements version"
    );
    ensure!(
        (1..=32).contains(&contract.requirements.len()),
        "declare 1..32 requirements"
    );
    let mut ids = BTreeSet::new();
    for requirement in &contract.requirements {
        ensure!(
            !requirement.id.trim().is_empty() && ids.insert(&requirement.id),
            "requirement ids must be nonempty and unique"
        );
        ensure!(
            !requirement.description.trim().is_empty(),
            "requirement description is required"
        );
        ensure!(
            !requirement.artifacts.is_empty() && !requirement.checks.is_empty(),
            "each requirement needs artifacts and checks"
        );
        for artifact in &requirement.artifacts {
            ensure!(
                !artifact.path.is_empty()
                    && !std::path::Path::new(&artifact.path).is_absolute()
                    && artifact
                        .path
                        .split('/')
                        .all(|p| !p.is_empty() && p != "." && p != ".."),
                "artifact paths must be normalized relative paths"
            );
        }
        for check in &requirement.checks {
            ensure!(
                !check.command.trim().is_empty() && !check.command.contains(['\n', '\r']),
                "checks must be nonempty single-line commands"
            );
        }
    }
    Ok(Some(contract))
}

pub(super) fn declared(root: &str) -> bool {
    !matches!(parse(root), Ok(None))
}

pub(super) fn required_commands(root: &str) -> Vec<String> {
    parse(root)
        .ok()
        .flatten()
        .into_iter()
        .flat_map(|c| c.requirements)
        .flat_map(|r| r.checks)
        .map(|c| c.command)
        .collect()
}

pub(super) fn source_paths(root: &str) -> Vec<String> {
    parse(root)
        .ok()
        .flatten()
        .into_iter()
        .flat_map(|c| c.requirements)
        .flat_map(|r| r.artifacts)
        .filter(|artifact| matches!(artifact.kind, ArtifactKind::Source))
        .map(|artifact| artifact.path)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

pub(super) fn project(messages: &[Value], test_cmd: Option<&str>) -> Status {
    let messages = crate::task_origin::current_task_messages(messages);
    let root = crate::task_origin::root_user_text(messages);
    let contract = match parse(root) {
        Ok(None) => return Status::default(),
        Ok(Some(contract)) => contract,
        Err(error) => {
            return Status {
                declared: true,
                pending: vec![format!("Invalid task requirements: {error}")],
                pending_baselines: vec![],
                pending_artifact_paths: vec![],
                requirements: vec![],
            }
        }
    };
    let evidence = super::evidence_packet::project(messages, test_cmd);
    let context =
        super::exec_verification::ExecVerificationContext::from_messages(test_cmd, messages);
    let is_edit = |a: &&super::evidence_packet::ActionEvidence| {
        crate::execution_evidence::file_edit_succeeded(&a.tool, a.result.as_deref().unwrap_or(""))
    };
    let first_mutation = evidence
        .actions
        .iter()
        .filter(|a| {
            is_edit(a)
                || (a.tool == "exec"
                    && a.executed == Some(true)
                    && a.arguments["command"]
                        .as_str()
                        .is_some_and(|cmd| context.classify(cmd).kind == super::ExecKind::Action))
        })
        .filter_map(|a| a.result_step)
        .min();
    let mut status = Status {
        declared: true,
        ..Status::default()
    };
    for requirement in contract.requirements {
        let mut missing = Vec::new();
        for artifact in &requirement.artifacts {
            if !evidence.actions.iter().filter(is_edit).any(|a| {
                a.arguments["path"]
                    .as_str()
                    .map(|p| p.trim_start_matches("./"))
                    == Some(artifact.path.as_str())
            }) {
                if !status.pending_artifact_paths.contains(&artifact.path) {
                    status.pending_artifact_paths.push(artifact.path.clone());
                }
                missing.push(format!(
                    "native file edit for {:?} {}",
                    artifact.kind, artifact.path
                ));
            }
        }
        for check in &requirement.checks {
            if evidence.pending_exact_checks.contains(&check.command) {
                missing.push(format!("current successful check: {}", check.command));
            }
            if check.baseline_failure {
                let baseline = evidence.actions.iter().rev().find(|a| {
                    a.tool == "exec"
                        && a.executed == Some(true)
                        && a.arguments["command"].as_str() == Some(check.command.as_str())
                        && a.result_step
                            .is_some_and(|step| first_mutation.is_none_or(|first| step < first))
                });
                // The correlated outcome is parsed before the advisory result
                // is shortened; diagnostic prefixes may exceed that text cap.
                if !baseline.is_some_and(|a| a.succeeded == Some(false)) {
                    if !status.pending_baselines.contains(&check.command) {
                        status.pending_baselines.push(check.command.clone());
                    }
                    missing.push(format!(
                        "failing baseline before the first mutation: {}",
                        check.command
                    ));
                }
            }
        }
        status.pending.extend(
            missing
                .iter()
                .map(|gap| format!("{}: {gap}", requirement.id)),
        );
        status
            .requirements
            .push(serde_json::json!({"requirement":requirement,"pending":missing}));
    }
    status
}

pub(super) fn is_pending_baseline(
    tc: &crate::streaming::ToolCallData,
    messages: &[Value],
    test_cmd: Option<&str>,
) -> bool {
    tc.name == "exec"
        && super::parse_exec_command_from_args(&tc.arguments).is_some_and(|command| {
            project(messages, test_cmd)
                .pending_baselines
                .contains(&command)
        })
}

/// A protocol refusal has not observed a workspace failure. Permit the first
/// human-declared baseline check after that refusal, before any mutation. A
/// real attempt (success or failure) or any edit closes this exception.
pub(super) fn is_unattempted_baseline(
    tc: &crate::streaming::ToolCallData,
    messages: &[Value],
    test_cmd: Option<&str>,
) -> bool {
    if !is_pending_baseline(tc, messages, test_cmd) {
        return false;
    }
    let evidence = super::evidence_packet::project(messages, test_cmd);
    let command = super::parse_exec_command_from_args(&tc.arguments);
    evidence.mutation_step.is_none()
        && !evidence.actions.iter().any(|action| {
            action.tool == "exec"
                && action.executed == Some(true)
                && action.arguments["command"].as_str() == command.as_deref()
        })
}

pub(super) fn mutation_issue(
    tc: &crate::streaming::ToolCallData,
    messages: &[Value],
    test_cmd: Option<&str>,
) -> Option<String> {
    let context =
        super::exec_verification::ExecVerificationContext::from_messages(test_cmd, messages);
    let mutates = matches!(tc.name.as_str(), "write_file" | "patch_file" | "apply_diff")
        || (tc.name == "exec"
            && super::parse_exec_command_from_args(&tc.arguments)
                .is_some_and(|command| context.classify(&command).kind == super::ExecKind::Action));
    if !mutates {
        return None;
    }
    let status = project(messages, test_cmd);
    if !status.pending_baselines.is_empty() {
        return Some(format!(
            "[Requirement Gate] Before editing, run these exact baseline checks on the unchanged workspace:\n{}\nA failing baseline is expected. Keep the check unchanged and then implement the declared artifacts. No edit was executed.",
            status.pending_baselines.join("\n")
        ));
    }
    None
}

pub(super) fn issue(messages: &[Value], test_cmd: Option<&str>) -> Option<String> {
    let status = project(messages, test_cmd);
    (!status.pending.is_empty()).then(|| format!("[Requirement Gate]\n{}\nRequirements come from the original task, not your plan. Run required failing baselines before editing. Use native file tools for declared artifacts, then execute each exact check after the last edit. A passing old test or a claimed plan criterion cannot replace these receipts.", status.pending.join("\n")))
}

#[cfg(test)]
#[path = "requirement_baseline_tests.rs"]
mod baseline_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn root() -> String {
        format!(
            "Implement selection.\n<task_requirements>\n{}\n</task_requirements>",
            json!({
            "version":1,"requirements":[{"id":"selection","description":"selection is wired and documented",
            "artifacts":[{"path":"src/runner.py","kind":"source"},{"path":"tests/test_runner.py","kind":"test"},{"path":"docs/usage.md","kind":"documentation"}],
            "checks":[{"command":"python3 acceptance.py","baseline_failure":true}]}]})
        )
    }
    fn exchange(messages: &mut Vec<Value>, id: &str, tool: &str, args: Value, output: &str) {
        messages.extend([json!({"role":"assistant","tool_calls":[{"id":id,"function":{"name":tool,"arguments":args.to_string()}}]}),json!({"role":"tool","tool_call_id":id,"content":output})]);
    }
    #[test]
    fn old_test_pass_and_self_declared_plan_cannot_complete_missing_artifacts() {
        let mut messages = vec![json!({"role":"user","content":root()})];
        exchange(
            &mut messages,
            "old",
            "exec",
            json!({"command":"python3 old_tests.py"}),
            "OK (exit_code: 0)",
        );
        exchange(
            &mut messages,
            "partial",
            "patch_file",
            json!({"path":"src/runner.py"}),
            "OK: patched 'src/runner.py'",
        );
        exchange(
            &mut messages,
            "check",
            "exec",
            json!({"command":"python3 acceptance.py"}),
            "OK (exit_code: 0)",
        );
        let issue = issue(&messages, None).unwrap();
        assert_eq!(
            serde_json::to_value(project(&messages, None)).unwrap()["pending_artifact_paths"],
            json!(["tests/test_runner.py", "docs/usage.md"])
        );
        assert!(issue.contains("tests/test_runner.py"));
        assert!(issue.contains("docs/usage.md"));
        assert!(issue.contains("failing baseline"));
        let mut outcome = super::super::outcome::TaskOutcome::default();
        outcome.accept_done(&[]);
        assert!(!outcome.completed(&messages));
    }

    #[test]
    fn editing_cannot_destroy_an_unrecorded_baseline() {
        let mut messages = vec![json!({"role":"user","content":root()})];
        let mut call = crate::streaming::ToolCallData {
            id: "edit".into(),
            name: "patch_file".into(),
            thought_signature: None,
            arguments: json!({"path":"src/runner.py","search":"old","replace":"new"}).to_string(),
        };
        let harness = super::super::task_harness::TaskHarness::infer(&root(), false);
        let blocked = super::super::task_harness::build_progress_gate_block(
            harness, &call, &messages, None, None, None, 0,
        )
        .unwrap();
        assert!(blocked.contains("Before editing"));
        call.name = "exec".into();
        call.arguments = json!({"command":"python3 acceptance.py"}).to_string();
        assert!(is_pending_baseline(&call, &messages, None));
        assert!(mutation_issue(&call, &messages, None).is_none());
        exchange(
            &mut messages,
            "red",
            "exec",
            json!({"command":"python3 acceptance.py"}),
            "FAILED (exit_code: 1)",
        );
        assert!(!is_pending_baseline(&call, &messages, None));
        call.name = "patch_file".into();
        assert!(mutation_issue(&call, &messages, None).is_none());
    }
    #[test]
    fn red_then_edits_then_green_survives_resume_and_invalidates_after_another_edit_or_failed_rerun(
    ) {
        let mut messages = vec![json!({"role":"user","content":root()})];
        exchange(
            &mut messages,
            "baseline",
            "exec",
            json!({"command":"python3 acceptance.py"}),
            "FAILED (exit_code: 1)",
        );
        for (id, path) in [
            ("s", "src/runner.py"),
            ("t", "tests/test_runner.py"),
            ("d", "docs/usage.md"),
        ] {
            exchange(
                &mut messages,
                id,
                "patch_file",
                json!({"path":path}),
                &format!("OK: patched '{path}'"),
            );
        }
        exchange(
            &mut messages,
            "green",
            "exec",
            json!({"command":"python3 acceptance.py"}),
            "OK (exit_code: 0)",
        );
        assert!(issue(&messages, None).is_none());
        messages = serde_json::from_str(&serde_json::to_string(&messages).unwrap()).unwrap();
        for index in 0..60 {
            exchange(
                &mut messages,
                &format!("pwd-{index}"),
                "exec",
                json!({"command":"pwd"}),
                "OK (exit_code: 0)\n/workspace",
            );
        }
        super::super::message_window::prune_message_window(&mut messages);
        assert!(
            messages.len() < 100,
            "old inspection commands should be pruned"
        );
        assert!(
            issue(&messages, None).is_none(),
            "keep baseline, edits and green proof"
        );
        messages.push(crate::task_origin::user_message(
            "Continue.",
            crate::task_origin::MessageOrigin::Runtime,
        ));
        assert!(issue(&messages, None).is_none());
        exchange(
            &mut messages,
            "again",
            "patch_file",
            json!({"path":"src/runner.py"}),
            "OK: patched 'src/runner.py'",
        );
        assert!(issue(&messages, None)
            .unwrap()
            .contains("current successful check"));
        exchange(
            &mut messages,
            "green2",
            "exec",
            json!({"command":"python3 acceptance.py"}),
            "OK (exit_code: 0)",
        );
        exchange(
            &mut messages,
            "red",
            "exec",
            json!({"command":"python3 acceptance.py"}),
            "FAILED (exit_code: 1)",
        );
        assert!(issue(&messages, None).is_some());
        messages.push(json!({"role":"user","content":"Explain a different task"}));
        assert!(!project(&messages, None).declared);
    }
    #[test]
    fn malformed_contract_fails_closed_and_examples_or_model_text_cannot_add_requirements() {
        assert!(parse("<task_requirements>\n{}").is_err());
        assert!(parse(&format!("```text\n{}\n```", root()))
            .unwrap()
            .is_none());
        let messages = vec![
            json!({"role":"user","content":"Explain"}),
            json!({"role":"assistant","content":root()}),
        ];
        assert!(!project(&messages, None).declared);
    }
}
