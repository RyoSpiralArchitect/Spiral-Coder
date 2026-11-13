//! Advisory links from declared test files to the requirements they exercise.
use super::{ArtifactKind, Contract};
use crate::execution_evidence::FileEditOutcome;
use crate::tui::agent::evidence_packet::ActionEvidence;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

mod review_source;

const MAX_DESCRIPTION_BYTES: usize = 4096;

#[derive(Debug, Serialize)]
pub(crate) struct AuthoredTests {
    targets: Vec<Target>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    test_only_requirements: Vec<TestRequirement>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    shared_requirements: Vec<TestRequirement>,
    #[serde(skip_serializing_if = "Option::is_none")]
    review_source: Option<review_source::ReviewSource>,
}

impl AuthoredTests {
    pub(in crate::tui::agent) fn contains(&self, path: &str) -> bool {
        let path = path.trim_start_matches("./");
        self.targets.iter().any(|target| target.path == path)
    }

    pub(in crate::tui::agent) fn include_review_source(
        &mut self,
        messages: &[serde_json::Value],
        context: &crate::tui::agent::exec_verification::ExecVerificationContext<'_>,
    ) {
        self.review_source = review_source::project(self, messages, context);
    }
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
    let mut linked = Vec::new();
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
        let test_only = requirement
            .artifacts
            .iter()
            .all(|artifact| matches!(artifact.kind, ArtifactKind::Test));
        linked.push((requirement, test_only));
    }
    if by_path.is_empty() {
        return None;
    }
    // Preserve dedicated test scenarios first, then carry the behavioral rules
    // that the same test is expected to exercise against its paired artifacts.
    linked.sort_by_key(|(_, test_only)| !test_only);
    let mut description_budget = MAX_DESCRIPTION_BYTES;
    let mut test_only_requirements = Vec::new();
    let mut shared_requirements = Vec::new();
    for (requirement, test_only) in linked {
        let Some(remaining) = description_budget.checked_sub(requirement.description.len()) else {
            continue;
        };
        description_budget = remaining;
        let target = if test_only {
            &mut test_only_requirements
        } else {
            &mut shared_requirements
        };
        target.push(TestRequirement {
            id: requirement.id.clone(),
            description: requirement.description.clone(),
        });
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
        shared_requirements,
        review_source: None,
    })
}

#[cfg(test)]
mod tests {
    use crate::tui::agent::task_obligations;
    use serde_json::{json, Value};

    const DESCRIPTION: &str =
        "From a valid control, change the parent field and assert the public error.";
    const SHARED_RULE: &str = "The first ledger row must have a null parent.";

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
                {"id":"ledger","description":SHARED_RULE,
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
        assert_eq!(
            authored["shared_requirements"],
            json!([{"id":"ledger","description":SHARED_RULE}])
        );
        let coder = task_obligations::prompt(&messages, None, Some("/workspace")).unwrap();
        assert!(coder.contains(DESCRIPTION) && coder.contains(SHARED_RULE));
        let observer = crate::tui::events::observer_assist::build_tui_next_action_prompt(
            &json!({"task_obligations":view,"work_budget":{"next_coder_turn":"work","next_coder_requests":8}}),
            "en",
            "incomplete_task",
        );
        assert!(
            observer.contains(DESCRIPTION)
                && observer.contains(SHARED_RULE)
                && observer.contains("no-tool clarify with the case recipe")
        );
    }

    #[test]
    fn native_test_bytes_reach_both_roles_and_expire_after_unobserved_changes() {
        const PATH: &str = "tests/ledger.rs";
        const CHECK: &str = "cargo test --locked --test ledger";
        const SOURCE: &str = "fn sample() { assert_eq!(requests.len(), count); }\n";
        let task = format!(
            "Add a ledger test.\n<task_requirements>\n{}\n</task_requirements>",
            json!({"version":1,"requirements":[{"id":"ledger","description":DESCRIPTION,
                "artifacts":[{"path":PATH,"kind":"test"}],"checks":[{"command":CHECK}]}]})
        );
        let mut messages = vec![json!({"role":"user","content":task})];
        let view = |messages: &[Value]| {
            serde_json::to_value(task_obligations::project_advisory(
                messages,
                None,
                Some("/workspace"),
            ))
            .unwrap()
        };
        let authored = |messages: &[Value]| {
            view(messages)["task_requirements"]["authored_tests"]["review_source"].clone()
        };
        result(
            &mut messages,
            "write",
            "write_file",
            json!({"path":PATH,"content":SOURCE}),
            &format!(
                "OK: wrote '{PATH}' (1 lines, {} bytes)\n[auto-test] ✓ PASSED",
                SOURCE.len()
            ),
        );
        assert_eq!(
            authored(&messages),
            json!({"path":PATH,"call_id":"write","origin":"write","source":SOURCE})
        );
        result(
            &mut messages,
            "verify",
            "exec",
            json!({"command":CHECK}),
            "OK (exit_code: 0)\ncwd: /workspace",
        );
        assert_eq!(authored(&messages)["call_id"], "write");
        let packet =
            json!({"task_obligations":view(&messages), "work_budget":{"next_coder_turn":"work"}});
        let observer = crate::tui::events::observer_assist::build_tui_next_action_prompt(
            &packet,
            "en",
            "incomplete_task",
        );
        let coder = task_obligations::prompt(&messages, None, Some("/workspace")).unwrap();
        assert_eq!(
            observer
                .matches("[Authored test source from a native receipt]")
                .count(),
            1
        );
        assert!(observer.contains("assert_eq!(requests.len(), count)"));
        assert!(coder.contains("assert_eq!(requests.len(), count)"));
        assert_eq!(view(&messages)["pending_exact_checks"], json!([]));
        result(
            &mut messages,
            "blocked",
            "patch_file",
            json!({"path":PATH,"search":SOURCE,"replace":""}),
            "GOVERNOR BLOCKED",
        );
        assert_eq!(authored(&messages)["call_id"], "write");
        result(
            &mut messages,
            "edit",
            "patch_file",
            json!({"path":PATH,"search":SOURCE,"replace":"after\n"}),
            &format!("OK: patched '{PATH}'"),
        );
        assert!(authored(&messages).is_null());
        result(
            &mut messages,
            "partial",
            "read_file",
            json!({"path":PATH,"start_line":2,"end_line":2}),
            &format!("[{PATH}] (3 lines, 100 bytes) [range: lines 2-2; partial file]\nafter\n"),
        );
        assert!(authored(&messages).is_null());
        result(
            &mut messages,
            "read",
            "read_file",
            json!({"path":PATH}),
            &format!("[{PATH}] (1 lines, 6 bytes)\nafter\n"),
        );
        assert_eq!(
            authored(&messages),
            json!({"path":PATH,"call_id":"read","origin":"read","source":"after\n"})
        );
        let mut switched = messages.clone();
        switched.push(json!({"role":"user","content":"Start another task."}));
        assert!(authored(&switched).is_null());
        result(
            &mut messages,
            "command",
            "exec",
            json!({"command":"cargo fmt --all"}),
            "FAILED (exit_code: 1)\ncwd: /workspace",
        );
        assert!(authored(&messages).is_null());
    }

    #[test]
    fn shared_text_budget_prioritizes_complete_test_only_cases() {
        let large = "x".repeat(super::MAX_DESCRIPTION_BYTES);
        let contract = serde_json::from_value(json!({"version":1,"requirements":[
            {"id":"large","description":large,"artifacts":[
                {"kind":"source","path":"src/ledger.rs"},{"kind":"test","path":"tests/ledger.rs"}
            ],"checks":[]},
            {"id":"parent","description":DESCRIPTION,
             "artifacts":[{"kind":"test","path":"tests/ledger.rs"}],"checks":[]},
            {"id":"root","description":SHARED_RULE,"artifacts":[
                {"kind":"source","path":"src/ledger.rs"},{"kind":"test","path":"tests/ledger.rs"}
            ],"checks":[]}
        ]}))
        .unwrap();
        let value = serde_json::to_value(super::project(&contract, &[]).unwrap()).unwrap();
        assert_eq!(
            value["targets"][0]["requirement_ids"],
            json!(["large", "parent", "root"])
        );
        assert_eq!(
            value["test_only_requirements"],
            json!([{"id":"parent","description":DESCRIPTION}])
        );
        assert_eq!(
            value["shared_requirements"],
            json!([{"id":"root","description":SHARED_RULE}])
        );
    }
}
