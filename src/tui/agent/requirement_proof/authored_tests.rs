//! Advisory links from declared test files to the requirements they exercise.
use super::{ArtifactKind, Contract};
use crate::execution_evidence::FileEditOutcome;
use crate::tui::agent::evidence_packet::ActionEvidence;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

const MAX_DESCRIPTION_BYTES: usize = 4096;

#[derive(Debug, Serialize)]
pub(crate) struct AuthoredTests {
    targets: Vec<Target>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    test_only_requirements: Vec<TestRequirement>,
}

#[derive(Debug, Serialize)]
struct Target {
    path: String,
    requirement_ids: Vec<String>,
    latest_changed_call_id: Option<String>,
}

#[derive(Debug, Serialize)]
struct TestRequirement {
    id: String,
    description: String,
}

pub(super) fn project(contract: &Contract, actions: &[ActionEvidence]) -> Option<AuthoredTests> {
    let mut by_path: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    let mut test_only_requirements = Vec::new();
    let mut description_budget = MAX_DESCRIPTION_BYTES;
    for requirement in &contract.requirements {
        let paths: BTreeSet<_> = requirement
            .artifacts
            .iter()
            .filter(|artifact| matches!(artifact.kind, ArtifactKind::Test))
            .map(|artifact| artifact.path.as_str())
            .collect();
        if paths.is_empty() {
            continue;
        }
        for path in paths {
            by_path
                .entry(path)
                .or_default()
                .push(requirement.id.clone());
        }
        if requirement
            .artifacts
            .iter()
            .all(|artifact| matches!(artifact.kind, ArtifactKind::Test))
            && requirement.description.len() <= description_budget
        {
            description_budget -= requirement.description.len();
            test_only_requirements.push(TestRequirement {
                id: requirement.id.clone(),
                description: requirement.description.clone(),
            });
        }
    }
    if by_path.is_empty() {
        return None;
    }
    let targets = by_path
        .into_iter()
        .map(|(path, requirement_ids)| {
            let latest_changed_call_id = actions
                .iter()
                .filter(|action| {
                    action.executed == Some(true)
                        && action.edit_outcome == Some(FileEditOutcome::Changed)
                        && action.arguments["path"]
                            .as_str()
                            .is_some_and(|arg| arg.trim_start_matches("./") == path)
                })
                .max_by_key(|action| action.result_step)
                .map(|action| action.call_id.clone());
            Target {
                path: path.into(),
                requirement_ids,
                latest_changed_call_id,
            }
        })
        .collect();
    Some(AuthoredTests {
        targets,
        test_only_requirements,
    })
}

#[cfg(test)]
mod tests {
    use crate::tui::agent::task_obligations;
    use serde_json::{json, Value};

    const DESCRIPTION: &str =
        "From a valid control, change the parent field and assert the public error.";

    fn result(messages: &mut Vec<Value>, id: &str, tool: &str, args: Value, body: &str) {
        messages.extend([
            json!({"role":"assistant","tool_calls":[{"id":id,"function":{"name":tool,"arguments":args.to_string()}}]}),
            json!({"role":"tool","tool_call_id":id,"content":body}),
        ]);
    }

    #[test]
    fn retains_test_requirements_after_edit_and_passing_check() {
        let check = "cargo test --test ledger";
        let path = "tests/ledger.rs";
        let task = format!(
            "Implement and test the ledger.\n<task_requirements>\n{}\n</task_requirements>",
            json!({"version":1,"requirements":[
                {"id":"ledger","description":"Check the public ledger behavior.",
                 "artifacts":[{"path":"src/ledger.rs","kind":"source"},{"path":path,"kind":"test"}],
                 "checks":[{"command":check}]},
                {"id":"parent","description":DESCRIPTION,
                 "artifacts":[{"path":path,"kind":"test"}],"checks":[{"command":check}]}
            ]})
        );
        let mut messages = vec![json!({"role":"user","content":task})];
        let projected = |history: &[Value]| {
            serde_json::to_value(task_obligations::project_advisory(
                history,
                None,
                Some("/workspace"),
            ))
            .unwrap()
        };
        assert_eq!(
            projected(&messages)["task_requirements"]["authored_tests"]["targets"][0],
            json!({"path":path,"requirement_ids":["ledger","parent"],"latest_changed_call_id":null})
        );
        result(
            &mut messages,
            "source",
            "write_file",
            json!({"path":"src/ledger.rs","content":"source"}),
            "OK: wrote 'src/ledger.rs'",
        );
        result(
            &mut messages,
            "test",
            "write_file",
            json!({"path":path,"content":"test"}),
            "OK: wrote 'tests/ledger.rs'",
        );
        result(
            &mut messages,
            "unchanged",
            "patch_file",
            json!({"path":path,"search":"test","replace":"test"}),
            "OK: unchanged 'tests/ledger.rs'",
        );
        result(
            &mut messages,
            "check",
            "exec",
            json!({"command":check}),
            "OK (exit_code: 0)\ncwd: /workspace",
        );
        let view = projected(&messages);
        assert_eq!(view["task_requirements"]["pending"], json!([]));
        assert_eq!(view["pending_exact_checks"], json!([]));
        let authored = &view["task_requirements"]["authored_tests"];
        assert_eq!(authored["targets"][0]["latest_changed_call_id"], "test");
        assert_eq!(
            authored["test_only_requirements"],
            json!([{"id":"parent","description":DESCRIPTION}])
        );
        let coder = task_obligations::prompt(&messages, None, Some("/workspace")).unwrap();
        assert!(coder.contains(DESCRIPTION) && coder.contains("compare the actual test code"));
        let observer = crate::tui::events::observer_assist::build_tui_next_action_prompt(
            &json!({"task_obligations":view,"work_budget":{"next_coder_turn":"work","next_coder_requests":8}}),
            "en",
            "incomplete_task",
        );
        assert!(
            observer.contains(DESCRIPTION)
                && observer.contains("no-tool clarify with the case recipe")
        );
    }
}
