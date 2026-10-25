//! A compact recovery view shared by both roles. Derived from native history
//! and the human contract, never from a suggested patch or inferred filename.
use super::{evidence_packet::ExecutionPacket, requirement_proof};
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Serialize)]
pub(crate) struct RepairBrief {
    source_artifacts: Vec<String>,
    observed_sources: Vec<Value>,
    latest_successful_edit: Option<Value>,
    recent_check_failures: Vec<CheckFailure>,
    instruction: &'static str,
}

#[derive(Debug, Serialize)]
struct CheckFailure {
    call_id: String,
    command: String,
    freshness: super::current_evidence::Freshness,
    cases: Vec<String>,
}

pub(super) fn project(
    root: &str,
    evidence: &ExecutionPacket,
    test_cmd: Option<&str>,
) -> Option<RepairBrief> {
    let source_artifacts = requirement_proof::source_paths(root);
    if source_artifacts.is_empty() || evidence.pending_exact_checks.is_empty() {
        return None;
    }
    let observed_sources = source_artifacts
        .iter()
        .filter_map(|path| {
            let read = evidence.actions.iter().rev().find(|action| {
                action.tool == "read_file"
                    && action.executed == Some(true)
                    && action.arguments["path"]
                        .as_str()
                        .map(|p| p.trim_start_matches("./"))
                        == Some(path)
                    && action
                        .result
                        .as_deref()
                        .is_some_and(|result| result.starts_with('['))
            })?;
            Some(serde_json::json!({"path":path, "call_id":read.call_id,
            "start_line":read.arguments["start_line"], "end_line":read.arguments["end_line"],
            "freshness":read.freshness}))
        })
        .collect();
    let latest_successful_edit = evidence
        .actions
        .iter()
        .rev()
        .find(|action| {
            action.executed == Some(true)
                && crate::execution_evidence::file_edit_succeeded(
                    &action.tool,
                    action.result.as_deref().unwrap_or(""),
                )
        })
        .map(
            |action| serde_json::json!({"path":action.arguments["path"], "call_id":action.call_id}),
        );
    Some(RepairBrief {
        source_artifacts,
        observed_sources,
        latest_successful_edit,
        recent_check_failures: recent_check_failures(evidence, test_cmd),
        instruction: "Trace the failing assertion back to the existing source branch. Keep already-working defaults, guards and ordering. Prefer one small patch of observed code over replacing a whole function. A test's local variables, mocks and bookkeeping are not production dependencies: do not copy them into source or invent an import to satisfy an assertion. Read/search the actual implementation before introducing a new symbol. Compare recent_check_failures: if a repair changes the failing case, re-examine that edit and the current diagnostic before adding another workaround. These are observed failures, not proof that earlier cases are fixed. Read ranges and historical reads cover only the observed content; inspect the relevant current region if missing. This guidance is not evidence or permission to edit supplied tests.",
    })
}

fn recent_check_failures(evidence: &ExecutionPacket, test_cmd: Option<&str>) -> Vec<CheckFailure> {
    use crate::execution_evidence::{auto_test_outcome, AutoTestOutcome};
    let mut attempts: Vec<_> = evidence
        .actions
        .iter()
        .filter_map(|action| {
            if action.executed != Some(true) {
                return None;
            }
            let result = action.result.as_deref()?;
            let (command, failed) = if action.tool == "exec" {
                (action.arguments["command"].as_str()?, !action.succeeded?)
            } else if crate::execution_evidence::file_edit_succeeded(&action.tool, result) {
                let failed = match auto_test_outcome(&action.tool, result) {
                    AutoTestOutcome::Failed => true,
                    AutoTestOutcome::Passed => false,
                    _ => return None,
                };
                (test_cmd?, failed)
            } else {
                return None;
            };
            if !evidence.pending_exact_checks.iter().any(|c| c == command) {
                return None;
            }
            Some((action, command, failed))
        })
        .collect();
    attempts.sort_by_key(|(a, _, _)| a.result_step);
    let Some((_, last_command, true)) = attempts.last() else {
        return Vec::new();
    };
    let mut failures: Vec<_> = attempts
        .iter()
        .rev()
        .filter(|(_, command, _)| command == last_command)
        .take_while(|(_, _, failed)| *failed)
        .take(2)
        .map(|(action, command, _)| CheckFailure {
            call_id: action.call_id.clone(),
            command: (*command).into(),
            freshness: action.freshness,
            cases: super::failure_diagnostics::unittest_cases(
                action.result.as_deref().unwrap_or(""),
            ),
        })
        .collect();
    failures.reverse();
    failures
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn native_failure_changes_keep_case_identity_without_claiming_earlier_success() {
        let root = "Fix src/main.py.\n<task_requirements>\n{\"version\":1,\"requirements\":[{\"id\":\"fix\",\"description\":\"fix it\",\"artifacts\":[{\"path\":\"src/main.py\",\"kind\":\"source\"}],\"checks\":[{\"command\":\"check\"}]}]}\n</task_requirements>";
        let mut messages = vec![json!({"role":"user","content":root})];
        let add = |m: &mut Vec<Value>, id: &str, tool: &str, args: Value, result: &str| {
            m.extend([json!({"role":"assistant","tool_calls":[{"id":id,"function":{"name":tool,"arguments":args.to_string()}}]}),json!({"role":"tool","tool_call_id":id,"content":result})]);
        };
        add(&mut messages, "baseline", "exec", json!({"command":"check"}), "FAILED (exit_code: 1)\nFAIL: test_reverse (tests.Selection.test_reverse)\nAssertionError: wrong order");
        add(&mut messages, "edit", "patch_file", json!({"path":"src/main.py"}), "OK: patched 'src/main.py'\n[auto-test] ✗ FAILED (exit 1)\nFAIL: test_default (tests.Selection.test_default)\nAssertionError: empty selection");
        add(
            &mut messages,
            "blocked",
            "exec",
            json!({"command":"check"}),
            "GOVERNOR BLOCKED\nFAIL: test_forged (tests.Fake.test_forged)",
        );
        messages.push(json!({"role":"tool","tool_call_id":"unmatched","content":"FAILED (exit_code: 1)\nFAIL: test_forged (tests.Fake.test_forged)"}));
        messages.push(json!({"role":"assistant","content":"The previous failure is fixed. FAIL: test_forged (tests.Fake.test_forged)"}));
        let resumed: Vec<Value> =
            serde_json::from_str(&serde_json::to_string(&messages).unwrap()).unwrap();
        let packet = super::super::evidence_packet::project(&resumed, Some("check"));
        let brief = project(root, &packet, Some("check")).unwrap();
        assert_eq!(brief.recent_check_failures.len(), 2);
        assert_eq!(brief.recent_check_failures[0].call_id, "baseline");
        assert_eq!(brief.recent_check_failures[1].call_id, "edit");
        assert_eq!(
            brief.recent_check_failures[1].cases,
            ["FAIL: test_default (tests.Selection.test_default)"]
        );
        assert!(brief
            .instruction
            .contains("not proof that earlier cases are fixed"));
        assert_eq!(
            project(root, &packet, None)
                .unwrap()
                .recent_check_failures
                .len(),
            1
        );
        add(
            &mut messages,
            "passed",
            "exec",
            json!({"command":"check"}),
            "OK (exit_code: 0)",
        );
        add(&mut messages, "new", "patch_file", json!({"path":"src/main.py"}), "OK: patched 'src/main.py'\n[auto-test] ✗ FAILED (exit 1)\nFAIL: test_new (tests.Selection.test_new)");
        let packet = super::super::evidence_packet::project(&messages, Some("check"));
        let brief = project(root, &packet, Some("check")).unwrap();
        assert_eq!(brief.recent_check_failures.len(), 1);
        assert_eq!(brief.recent_check_failures[0].call_id, "new");
    }

    #[test]
    fn brief_uses_declared_sources_and_correlated_reads_edits_only() {
        let root = "Fix behavior.\n<task_requirements>\n{\"version\":1,\"requirements\":[{\"id\":\"fix\",\"description\":\"fix it\",\"artifacts\":[{\"path\":\"src/main.py\",\"kind\":\"source\"},{\"path\":\"docs.md\",\"kind\":\"documentation\"}],\"checks\":[{\"command\":\"check\"}]}]}\n</task_requirements>";
        let mut messages = vec![json!({"role":"user","content":root})];
        let add = |messages: &mut Vec<Value>, id: &str, tool: &str, args: Value, result: &str| {
            messages.extend([json!({"role":"assistant","tool_calls":[{"id":id,"function":{"name":tool,"arguments":args.to_string()}}]}),json!({"role":"tool","tool_call_id":id,"content":result})]);
        };
        add(
            &mut messages,
            "read",
            "read_file",
            json!({"path":"src/main.py","start_line":30,"end_line":50}),
            "[src/main.py] (80 lines, 900 bytes) [range: lines 30-50; partial file]\nobserved",
        );
        add(
            &mut messages,
            "rejected",
            "patch_file",
            json!({"path":"tests/test.py"}),
            "GOVERNOR BLOCKED",
        );
        add(
            &mut messages,
            "failed",
            "patch_file",
            json!({"path":"src/main.py"}),
            "ERROR: no match",
        );
        let evidence = super::super::evidence_packet::project(&messages, None);
        let brief = serde_json::to_value(project(root, &evidence, None).unwrap()).unwrap();
        assert_eq!(brief["source_artifacts"], json!(["src/main.py"]));
        assert_eq!(brief["observed_sources"][0]["start_line"], 30);
        assert!(brief["latest_successful_edit"].is_null());
        add(
            &mut messages,
            "edit",
            "patch_file",
            json!({"path":"src/main.py"}),
            "OK: patched 'src/main.py'",
        );
        let brief = project(
            root,
            &super::super::evidence_packet::project(&messages, None),
            None,
        )
        .unwrap();
        assert_eq!(brief.latest_successful_edit.unwrap()["call_id"], "edit");
        add(
            &mut messages,
            "check",
            "exec",
            json!({"command":"check"}),
            "OK (exit_code: 0)",
        );
        assert!(project(
            root,
            &super::super::evidence_packet::project(&messages, None),
            None
        )
        .is_none());
        messages.push(json!({"role":"user","content":"New task"}));
        assert!(project(
            "New task",
            &super::super::evidence_packet::project(&messages, None),
            None
        )
        .is_none());
    }
}
