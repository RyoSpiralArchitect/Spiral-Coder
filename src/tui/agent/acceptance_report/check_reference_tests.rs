use super::*;
use serde_json::{json, Value};

const WORKSPACE: &str = "/workspace";
const SUCCESS: &str = "OK (exit_code: 0)\ncwd: /workspace";
const FAILED: &str = "FAILED (exit_code: 1)\ncwd: /workspace";

fn task(command: &str) -> String {
    format!(
        "Repair source.py and verify it.\n<task_requirements>\n{}\n</task_requirements>",
        json!({"version":1,"requirements":[{
            "id":"verification", "description":"Verify the repair",
            "artifacts":[{"path":"source.py","kind":"source"}], "checks":[{"command":command}]
        }]})
    )
}

fn result(messages: &mut Vec<Value>, name: &str, args: Value, output: &str) {
    let id = format!("receipt-{}", messages.len());
    messages.extend([
        json!({"role":"assistant","tool_calls":[{"id":id,"function":{"name":name,"arguments":args.to_string()}}]}),
        json!({"role":"tool","tool_call_id":id,"content":output}),
    ]);
}

#[test]
fn check_references_select_only_current_success_and_preserve_authored_rows() {
    let command = format!("python3 -S -m unittest test_{}", "x".repeat(250));
    let root = task(&command);
    let rows = json!([{"criterion":"acceptance 2","command":"@required-1"}]);
    let mut messages = vec![json!({"role":"user","content":root})];
    let resolve = |messages: &[Value]| {
        resolve_evidence(
            &rows,
            &CurrentEvidence::from_native(messages, None, Some(WORKSPACE)),
        )
    };
    assert!(
        resolve(&messages).is_err(),
        "an ID without execution is not proof"
    );
    result(
        &mut messages,
        "exec",
        json!({"command":command}),
        "OK (exit_code: 0)\n[history digest — kept 10/20 lines, 1024 chars]\ncwd: /workspace\nstdout:\nchecks passed",
    );
    for shape in [rows.clone(), json!(["acceptance 2 | @required-1"])] {
        let original = shape.clone();
        let resolved = resolve_evidence(
            &shape,
            &CurrentEvidence::from_native(&messages, None, Some(WORKSPACE)),
        )
        .unwrap();
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].criterion, "acceptance 2");
        assert_eq!(resolved[0].command, command);
        assert_eq!(shape, original, "provider arguments remain as authored");
    }
    result(&mut messages, "exec", json!({"command":command}), FAILED);
    assert!(
        resolve(&messages).is_err(),
        "a failed rerun supersedes the pass"
    );
    result(&mut messages, "exec", json!({"command":command}), SUCCESS);
    result(
        &mut messages,
        "patch_file",
        json!({"path":"source.py"}),
        "OK: patched 'source.py'",
    );
    assert!(
        resolve(&messages).is_err(),
        "edits invalidate the selected receipt"
    );
    result(&mut messages, "exec", json!({"command":command}), SUCCESS);
    messages.push(json!({"role":"user","content":"A new task. Run exactly: cargo test -q"}));
    assert!(
        resolve(&messages).is_err(),
        "an old task does not supply a new task's proof"
    );
}

#[test]
fn references_do_not_guess_unknown_ids_or_reinterpret_literal_commands() {
    let command = "cargo test -q";
    let root = task(command);
    let mut messages = vec![json!({"role":"user","content":root})];
    result(&mut messages, "exec", json!({"command":command}), SUCCESS);
    let mut evidence = CurrentEvidence::from_native(&messages, None, Some(WORKSPACE));
    for id in [
        "@required-2",
        "@REQUIRED-1",
        "@required-1 --extra",
        "@",
        "@required-1\nother",
    ] {
        assert!(resolve_evidence(
            &json!([{"criterion":"acceptance 1","command":id}]),
            &evidence,
        )
        .is_err());
    }
    let literal = "@required-1";
    evidence.commands.push(literal.into());
    let resolved = resolve_evidence(
        &json!([{"criterion":"acceptance 1","command":literal}]),
        &evidence,
    )
    .unwrap();
    assert_eq!(resolved[0].command, literal);
    messages[0]["content"] = json!(root.replace("\"version\":1", "\"version\":2"));
    assert!(resolve_evidence(
        &json!([{"criterion":"acceptance 1","command":literal}]),
        &CurrentEvidence::from_native(&messages, None, Some(WORKSPACE)),
    )
    .is_err());
}

#[test]
fn optional_alias_selects_the_latest_root_execution() {
    let command = "cargo test -q";
    let root = format!(
        "Verify source.py.\n<task_checks>\n{}\n</task_checks>",
        json!({"version":1,"checks":[{"id":"unit","command":command}]}),
    );
    let mut messages = vec![json!({"role":"user","content":root})];
    let rows = json!([{"criterion":"acceptance 1","command":"@unit"}]);
    let resolve = |messages: &[Value]| {
        resolve_evidence(
            &rows,
            &CurrentEvidence::from_native(messages, None, Some(WORKSPACE)),
        )
    };
    result(
        &mut messages,
        "exec",
        json!({"command":command}),
        "OK (exit_code: 0)\ncwd: /workspace/sub",
    );
    result(&mut messages, "exec", json!({"command":command}), FAILED);
    assert!(resolve(&messages).is_err());
    assert_eq!(current_commands(&messages, None), vec![command]);

    let verbose = format!(
        "{SUCCESS}\nstdout:\n{}",
        "ordinary test output\n".repeat(20)
    );
    let compact = super::super::compact_success_tool_result_for_history("exec", &verbose);
    assert!(compact.contains("[history digest"));
    assert_eq!(compact.lines().nth(1), Some("cwd: /workspace"));
    result(&mut messages, "exec", json!({"command":command}), &compact);
    assert_eq!(resolve(&messages).unwrap()[0].command, command);
    result(
        &mut messages,
        "exec",
        json!({"command":command}),
        "FAILED (exit_code: 1)\ncwd: /workspace/sub/..",
    );
    assert!(resolve(&messages).is_err());
    result(&mut messages, "exec", json!({"command":command}), SUCCESS);
    assert_eq!(resolve(&messages).unwrap()[0].command, command);
    result(&mut messages, "exec", json!({"command":command}), FAILED);
    assert!(resolve(&messages).is_err());
}

#[test]
fn coder_and_observer_schemas_offer_the_same_short_citations() {
    let command = format!("python3 -S -m unittest test_{}", "x".repeat(250));
    let root = task(&command);
    let mut messages = vec![json!({"role":"user","content":root})];
    result(&mut messages, "exec", json!({"command":command}), SUCCESS);
    let mut obligations = task_obligations::project_advisory(&messages, None, Some(WORKSPACE));
    obligations.acceptance_criteria =
        vec![json!({"reference":"acceptance 1","criterion":"Verify"})];
    assert_eq!(obligations.citable_checks.len(), 1);
    let plan = PlanBlock {
        goal: "Repair source.py".into(),
        steps: vec!["Verify".into()],
        acceptance_criteria: vec!["Verify".into()],
        risks: "regressions".into(),
        assumptions: "source exists".into(),
    };
    for endpoint in [
        "https://api.openai.com/v1",
        "https://generativelanguage.googleapis.com/v1beta/openai",
    ] {
        let mut tools = json!([super::super::done_tool_def(endpoint)]);
        super::super::acceptance_schema::bind(
            &mut tools,
            Some(&plan),
            &obligations.current_commands,
            &obligations.citable_checks,
        );
        let mut schemas = crate::tui::suggestion::tool_schemas_for(endpoint);
        obligations.bind_observer_tools(&mut schemas);
        let done = schemas
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["name"] == "done")
            .unwrap();
        let coder =
            &tools[0]["function"]["parameters"]["properties"]["acceptance_evidence"]["items"];
        let observer = &done["parameters"]["properties"]["acceptance_evidence"]["items"];
        assert_eq!(coder, observer);
        let choices = if coder["type"] == "string" {
            &coder["enum"]
        } else {
            &coder["properties"]["command"]["enum"]
        };
        assert!(choices.to_string().contains("@required-1"));
        assert!(!choices.to_string().contains(&command));
    }
}
