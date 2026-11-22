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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::tui::agent) ordered_record_requirement: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    test_only_requirements: Vec<TestRequirement>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    shared_requirements: Vec<TestRequirement>,
    #[serde(skip_serializing_if = "Option::is_none")]
    review_source: Option<review_source::ReviewSource>,
}

impl AuthoredTests {
    pub(in crate::tui::agent) fn has_explicit_cases(&self) -> bool {
        self.test_only_requirements
            .iter()
            .any(|r| !r.explicit_cases.is_empty())
    }

    pub(in crate::tui::agent) fn source_line(&self, path: &str, line: usize) -> Option<&str> {
        self.review_source.as_ref()?.line(path, line)
    }

    pub(in crate::tui::agent) fn contains(&self, path: &str) -> bool {
        let path = path.trim_start_matches("./");
        self.targets.iter().any(|target| target.path == path)
    }

    pub(in crate::tui::agent) fn include_review_source(
        &mut self,
        messages: &[serde_json::Value],
        context: &crate::tui::agent::exec_verification::ExecVerificationContext<'_>,
        tool_root: Option<&str>,
    ) {
        self.review_source = review_source::project(self, messages, context, tool_root);
    }

    pub(in crate::tui::agent) fn include_workspace_review_source(
        &mut self,
        messages: &[serde_json::Value],
        workspace: &str,
    ) {
        if self.review_source.is_none() {
            self.review_source = review_source::readback(self, messages, workspace);
        }
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
    #[serde(skip_serializing_if = "Vec::is_empty")]
    explicit_cases: Vec<String>,
}

pub(super) fn project(contract: &Contract, actions: &[ActionEvidence]) -> Option<AuthoredTests> {
    let mut by_path: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    let mut linked = Vec::new();
    let mut ordered_record_requirement = None;
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
        if ordered_record_requirement.is_none() && names_record_order(&requirement.description) {
            ordered_record_requirement = Some(requirement.id.clone());
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
            explicit_cases: requirement
                .explicit_test_cases()
                .into_iter()
                .map(str::to_owned)
                .collect(),
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
        ordered_record_requirement,
        test_only_requirements,
        shared_requirements,
        review_source: None,
    })
}

fn names_record_order(description: &str) -> bool {
    let lower = description.to_ascii_lowercase();
    let mut previous = "";
    for word in lower
        .split(|ch: char| !ch.is_alphanumeric())
        .filter(|w| !w.is_empty())
    {
        if (word == "order" && matches!(previous, "row" | "record" | "array" | "item" | "entry"))
            || (previous == "ordered"
                && matches!(word, "rows" | "records" | "array" | "items" | "entries"))
        {
            return true;
        }
        previous = word;
    }
    false
}

#[cfg(test)]
mod tests {
    use crate::tui::agent::task_obligations;
    use serde_json::{json, Value};

    #[test]
    fn distinguish_ordered_records_from_incidental_order_language() {
        for description in [
            "Preserve array order.",
            "Return ORDERED records with their identities.",
            "Preserve row order 1..N.",
        ] {
            assert!(super::names_record_order(description));
        }
        for description in [
            "Validate each record in order to count it.",
            "Return unordered records.",
            "Place an order.",
        ] {
            assert!(!super::names_record_order(description));
        }
    }

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
        assert!(observer.contains("preserve the recorded path"));
        assert!(coder.contains("preserve its recorded path"));
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
        let rendered_before = format!("[{PATH}] (1 lines, 6 bytes)\nafter\n");
        result(
            &mut messages,
            "read",
            "read_file",
            json!({"path":PATH}),
            &rendered_before,
        );
        assert_eq!(
            authored(&messages),
            json!({"path":PATH,"call_id":"read","origin":"read","source":"after\n"})
        );
        use crate::tui::agent::verification::{fmt_hash, hash_text};
        let fingerprint = |source: &str| fmt_hash(hash_text(source));
        let absolute = format!("/workspace/{PATH}");
        let args = json!({"path":absolute,"search":"after","replace":"latest"});
        let patch_header = format!("OK: patched '{absolute}' (+0 lines, 1 total)");
        let before = fingerprint("after\n");
        let after = fingerprint("latest\n");
        let bad = fingerprint("different\n");
        for hashes in [
            String::new(),
            format!("[hash] before={bad} after={after}"),
            format!("[hash] before={before} after={bad}"),
            format!("[hash] before={before} after={after}\n[hash] before={before} after={bad}"),
        ] {
            let mut uncertain = messages.clone();
            result(
                &mut uncertain,
                "unconfirmed",
                "patch_file",
                args.clone(),
                &format!("{patch_header}\n{hashes}"),
            );
            assert!(authored(&uncertain).is_null(), "{hashes}");
        }
        result(
            &mut messages,
            "confirmed",
            "patch_file",
            args,
            &format!(
                "{patch_header}\n[hash] before={} after={after}",
                fingerprint(&rendered_before)
            ),
        );
        assert_eq!(
            authored(&messages),
            json!({"path":PATH,"call_id":"confirmed","origin":"reconstructed_edit","source":"latest\n"})
        );
        let diff = "@@\n-latest\n+final";
        result(
            &mut messages,
            "diff",
            "apply_diff",
            json!({"path":PATH,"diff":diff}),
            &format!(
                "OK: applied 1/1 hunk(s) to '{PATH}'\n[hash] before={after} after={}",
                fingerprint("final\n")
            ),
        );
        assert_eq!(authored(&messages)["source"], "final\n");
        assert_eq!(authored(&messages)["call_id"], "diff");
        assert_eq!(view(&messages)["pending_exact_checks"], json!([CHECK]));
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
    fn local_review_refreshes_a_declared_test_when_the_formatter_invalidates_history() {
        const PATH: &str = "tests/ledger.rs";
        const CHECK: &str = "cargo test --locked --test ledger";
        let root = tempfile::tempdir().unwrap();
        let workspace = root.path().to_str().unwrap();
        let target = root.path().join(PATH);
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        let source = "#[test]\nfn duplicate_preserves_row_count() {}\n";
        std::fs::write(&target, source).unwrap();
        let task = format!(
            "Add an integration test.\n<task_requirements>\n{}\n</task_requirements>",
            json!({"version":1,"requirements":[{"id":"ledger","description":DESCRIPTION,
                "artifacts":[{"path":PATH,"kind":"test"}],"checks":[{"command":CHECK}]}]})
        );
        let mut messages = vec![json!({"role":"user","content":task})];
        let local = |history: &[Value]| {
            serde_json::to_value(task_obligations::project_at(history, None, Some(workspace)))
                .unwrap()
        };
        let key =
            |view: &Value| view["task_requirements"]["authored_tests"]["review_source"].clone();
        assert!(key(&local(&messages)).is_null());
        result(
            &mut messages,
            "edit",
            "patch_file",
            json!({"path":target,"search":"before","replace":"after"}),
            &format!("OK: patched '{}' (+0 lines, 2 total)", target.display()),
        );
        result(
            &mut messages,
            "format",
            "exec",
            json!({"command":"cargo fmt --all"}),
            "OK (exit_code: 0)",
        );
        let advisory = serde_json::to_value(task_obligations::project_advisory(
            &messages,
            None,
            Some(workspace),
        ))
        .unwrap();
        let mut review = local(&messages);
        assert!(key(&advisory).is_null());
        assert_eq!(
            key(&review),
            json!({"path":PATH,"call_id":"edit","origin":"workspace_readback","source":source})
        );
        let coder = task_obligations::prompt(&messages, None, Some(workspace)).unwrap();
        let observer = crate::tui::events::observer_assist::build_tui_next_action_prompt(
            &json!({"task_obligations":review,"work_budget":{"next_coder_turn":"work"}}),
            "en",
            "incomplete_task",
        );
        assert!(coder.contains("fn duplicate_preserves_row_count()"));
        assert!(observer.contains("Authored test source read from the workspace"));
        assert!(observer.contains("fn duplicate_preserves_row_count()"));
        review["task_requirements"]["authored_tests"]
            .as_object_mut()
            .unwrap()
            .remove("review_source");
        assert_eq!(review, advisory);

        std::fs::write(&target, "x".repeat(8 * 1024 + 1)).unwrap();
        assert!(key(&local(&messages)).is_null());
        std::fs::remove_file(&target).unwrap();
        #[cfg(unix)]
        {
            let elsewhere = tempfile::NamedTempFile::new().unwrap();
            std::fs::write(elsewhere.path(), "external contents\n").unwrap();
            std::os::unix::fs::symlink(elsewhere.path(), &target).unwrap();
            assert!(key(&local(&messages)).is_null());
        }
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

    #[test]
    fn frozen_status_rules_survive_final_closeout_and_source_mutation() {
        let task = include_str!("../../../../benchmarks/self-dogfood-v2/coupled-status/prompt.txt");
        let contract = super::super::parse(task).unwrap().unwrap();
        let mut messages = vec![json!({"role":"user","content":task})];
        let checks = &contract.requirements[0].checks;
        for check in checks.iter().filter(|check| check.baseline_failure) {
            result(&mut messages, "baseline", "exec", json!({"command":check.command}),
                "FAILED (exit_code: 1)\ncwd: /workspace\nAssertionError: coupled-status command missing from --help");
        }
        for (i, artifact) in contract.requirements[0].artifacts.iter().enumerate() {
            let path = &artifact.path;
            let (tool, args, receipt) = if path == "docs/state-schema.md" {
                (
                    "patch_file",
                    json!({"path":path,"search":"old\n","replace":"old\nfixture\n"}),
                    format!("OK: patched '{path}'"),
                )
            } else {
                (
                    "write_file",
                    json!({"path":path,"content":"fixture\n"}),
                    format!("OK: wrote '{path}' (1 lines, 8 bytes)"),
                )
            };
            result(&mut messages, &format!("edit-{i}"), tool, args, &receipt);
        }
        for (i, check) in checks.iter().enumerate() {
            result(
                &mut messages,
                &format!("check-{i}"),
                "exec",
                json!({"command":check.command}),
                "OK (exit_code: 0)\ncwd: /workspace",
            );
        }
        for mutated in [false, true] {
            if mutated {
                result(
                    &mut messages,
                    "source-repair",
                    "patch_file",
                    json!({"path":"src/coupled_status.rs","search":"fixture\n","replace":"repaired\n"}),
                    "OK: patched 'src/coupled_status.rs'",
                );
            }
            let view = serde_json::to_value(task_obligations::project_advisory(
                &messages,
                None,
                Some("/workspace"),
            ))
            .unwrap();
            let expected_checks: Vec<_> =
                checks.iter().map(|check| check.command.clone()).collect();
            assert_eq!(
                view["pending_exact_checks"],
                if mutated {
                    json!(expected_checks)
                } else {
                    json!([])
                }
            );
            assert_eq!(
                view["task_requirements"]["pending"]
                    .as_array()
                    .unwrap()
                    .is_empty(),
                !mutated
            );
            let authored = &view["task_requirements"]["authored_tests"];
            assert_eq!(
                authored["targets"][0]["requirement_ids"],
                json!([
                    "status",
                    "behavior-1",
                    "behavior-2",
                    "behavior-3",
                    "behavior-4",
                    "behavior-5",
                    "behavior-6",
                    "behavior-7",
                    "native-tests"
                ])
            );
            let coder = task_obligations::prompt(&messages, None, Some("/workspace")).unwrap();
            let observer = crate::tui::events::observer_assist::build_tui_next_action_prompt(
                &json!({"task_obligations":view,"work_budget":{"next_coder_turn":"work","next_coder_requests":8}}),
                "en",
                "incomplete_task",
            );
            for id in ["behavior-3", "behavior-4", "behavior-6", "native-tests"] {
                let original = contract.requirements.iter().find(|r| r.id == id).unwrap();
                let key = if id == "native-tests" {
                    "test_only_requirements"
                } else {
                    "shared_requirements"
                };
                let row = authored[key]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|r| r["id"] == id)
                    .unwrap();
                assert_eq!(row["description"], original.description);
                for prompt in [&coder, &observer] {
                    assert!(
                        prompt.contains(id) && prompt.contains(&original.description),
                        "missing {id}, mutated={mutated}"
                    );
                }
            }
        }
    }
}
