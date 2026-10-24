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
    instruction: &'static str,
}

pub(super) fn project(root: &str, evidence: &ExecutionPacket) -> Option<RepairBrief> {
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
        instruction: "Trace the failing assertion back to the existing source branch. Keep already-working defaults, guards and ordering. Prefer one small patch of observed code over replacing a whole function. A test's local variables, mocks and bookkeeping are not production dependencies: do not copy them into source or invent an import to satisfy an assertion. Read/search the actual implementation before introducing a new symbol. If a repair changes the failure, re-examine that edit and the current diagnostic before adding another workaround. Read ranges and historical reads cover only the observed content; inspect the relevant current region if missing. This guidance is not evidence or permission to edit supplied tests.",
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
        let brief = serde_json::to_value(project(root, &evidence).unwrap()).unwrap();
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
            &super::super::evidence_packet::project(&messages, None)
        )
        .is_none());
        messages.push(json!({"role":"user","content":"New task"}));
        assert!(project(
            "New task",
            &super::super::evidence_packet::project(&messages, None)
        )
        .is_none());
    }
}
