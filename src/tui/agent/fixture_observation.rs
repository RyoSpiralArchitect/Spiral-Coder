//! Carry bounded fixture data printed by a human-declared check.
//! The observation is advisory and contributes no verification or edit proof.
use super::{
    exec_verification::ExecVerificationContext, parse_exec_command_from_args, prose_context,
    recovery_focus::correlated_results, requirement_proof, ExecKind,
};
use crate::execution_evidence::{exec_cwd, exec_may_have_run, exec_outcome, file_edit_succeeded};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, path::Path};

const PREFIX: &str = "Fixture observation v1: ";
const MAX_FRAME_BYTES: usize = 4096;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Declaration {
    version: u32,
    check: String,
    test_path: String,
    case: String,
}

fn declared(messages: &[Value]) -> Option<Declaration> {
    let task = crate::task_origin::root_user_text(messages);
    let mut body = None::<String>;
    let mut closed = false;
    let mut fence = None;
    for line in task.lines().map(str::trim) {
        if prose_context::is_fence_line(line) {
            let marker = prose_context::fence_marker(line);
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
        match line {
            "<fixture_observation>" if body.is_none() => body = Some(String::new()),
            "</fixture_observation>" if body.is_some() && !closed => closed = true,
            "<fixture_observation>" | "</fixture_observation>" => return None,
            _ if !closed => {
                if let Some(body) = body.as_mut() {
                    if body.len() + line.len() + 1 > MAX_FRAME_BYTES {
                        return None;
                    }
                    body.push_str(line);
                    body.push('\n');
                }
            }
            _ => {}
        }
    }
    if !closed {
        return None;
    }
    let declaration: Declaration = serde_json::from_str(&body?).ok()?;
    (declaration.version == 1
        && short_name(&declaration.case)
        && requirement_proof::is_declared_test_check(
            task,
            &declaration.check,
            &declaration.test_path,
        ))
    .then_some(declaration)
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Frame {
    version: u32,
    case: String,
    snapshots: Vec<Snapshot>,
    comparisons: Vec<Comparison>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Snapshot {
    stage: Stage,
    file: String,
    fields: BTreeMap<String, Value>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum Stage {
    Generated,
    ValidatorInput,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Comparison {
    field: String,
    observed: Value,
    expected: Value,
}

fn short_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= 160 && !name.chars().any(char::is_control)
}

fn scalar(value: &Value) -> bool {
    match value {
        Value::String(text) => text.len() <= 240 && !text.chars().any(char::is_control),
        Value::Number(_) | Value::Bool(_) | Value::Null => true,
        _ => false,
    }
}

impl Frame {
    fn valid(&self, case: &str) -> bool {
        self.version == 1
            && self.case == case
            && (1..=12).contains(&self.snapshots.len())
            && self
                .snapshots
                .iter()
                .any(|s| matches!(s.stage, Stage::ValidatorInput))
            && self.snapshots.iter().all(|s| {
                short_name(&s.file)
                    && crate::task_scope::relative_path(&s.file, None).as_deref()
                        == Some(s.file.as_str())
                    && (1..=16).contains(&s.fields.len())
                    && s.fields
                        .iter()
                        .all(|(key, value)| short_name(key) && scalar(value))
            })
            && self.comparisons.len() <= 12
            && self
                .comparisons
                .iter()
                .all(|c| short_name(&c.field) && scalar(&c.observed) && scalar(&c.expected))
    }

    pub(super) fn attach(&self, mut tool_result: Value) -> Value {
        tool_result["recovery_focus"]["fixture_observation"] =
            serde_json::to_value(self).expect("fixture observation serializes");
        tool_result
    }
}

/// Capture before truncation. A declaration and the exact failed exec at the
/// workspace root are checked by dispatch. Only one terminal frame is admitted.
pub(super) fn capture(messages: &[Value], check: &str, stdout: &str) -> Option<Frame> {
    let output = stdout.trim_end();
    let line = output.rsplit('\n').next()?;
    let body = line.strip_prefix(PREFIX)?;
    if body.len() > MAX_FRAME_BYTES
        || output[..output.len() - line.len()]
            .lines()
            .any(|l| l.starts_with(PREFIX))
    {
        return None;
    }
    let spec = declared(messages)?;
    if spec.check != check {
        return None;
    }
    let frame: Frame = serde_json::from_str(body).ok()?;
    frame.valid(&spec.case).then_some(frame)
}

#[derive(Debug, Serialize)]
pub(super) struct Observation {
    tool_call_id: String,
    check: String,
    #[serde(flatten)]
    frame: Frame,
    mismatched_fields: Vec<String>,
}

/// Bind retained local metadata to the current panic and check. A mutation or
/// a newer execution of that check retires the prior bytes.
pub(super) fn project(
    messages: &[Value],
    root: &str,
    check: &str,
    path: &str,
    case: &str,
    context: &ExecVerificationContext<'_>,
) -> Option<Observation> {
    let declaration = declared(messages)?;
    if declaration.check != check || declaration.test_path != path || declaration.case != case {
        return None;
    }
    let mut current = None;
    for (call, result) in correlated_results(messages) {
        let content = result["content"].as_str().unwrap_or_default();
        if file_edit_succeeded(&call.name, content) {
            current = None;
        }
        if call.name != "exec" || !exec_may_have_run(content) {
            continue;
        }
        let Some(command) = parse_exec_command_from_args(&call.arguments) else {
            continue;
        };
        if context.classify(&command).kind == ExecKind::Action {
            current = None;
        }
        if command != check {
            continue;
        }
        current = None;
        if exec_outcome(content) != Some(false)
            || exec_cwd(content).map(Path::new) != Some(Path::new(root))
            || result["recovery_focus"]["version"] != 2
        {
            continue;
        }
        let payload = &result["recovery_focus"]["fixture_observation"];
        let Some(frame) = serde_json::to_string(payload)
            .ok()
            .filter(|json| json.len() <= MAX_FRAME_BYTES)
            .and_then(|json| serde_json::from_str::<Frame>(&json).ok())
            .filter(|frame| frame.valid(case))
        else {
            continue;
        };
        current = Some(Observation {
            tool_call_id: call.id,
            check: check.into(),
            mismatched_fields: frame
                .comparisons
                .iter()
                .filter(|pair| pair.observed != pair.expected)
                .map(|pair| pair.field.clone())
                .collect(),
            frame,
        });
    }
    current
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::agent::{recovery_focus::RecoveryFocus, task_obligations, ToolCallData};
    use serde_json::json;

    const CHECK: &str = "python3 -S scripts/status_fixture_observe.py";
    const TEST_PATH: &str = "tests/import.rs";
    const CASE: &str = "first_import";

    fn task() -> String {
        format!(
            "Repair the test.\n<task_requirements>\n{}\n</task_requirements>\n<fixture_observation>\n{}\n</fixture_observation>",
            json!({"version":1,"requirements":[{"id":"import","description":"Verify input.",
                "artifacts":[{"path":TEST_PATH,"kind":"test"}],
                "checks":[{"command":CHECK,"baseline_failure":true}]}]}),
            json!({"version":1,"check":CHECK,"test_path":TEST_PATH,"case":CASE})
        )
    }

    fn frame() -> String {
        format!(
            "Test output\n{PREFIX}{}\n",
            json!({"version":1,"case":CASE,
                "snapshots":[{"stage":"generated","file":"index.json","fields":{"pointer":"record-02"}},
                            {"stage":"validator_input","file":"index.json","fields":{"pointer":"record-02","rows":0}}],
                "comparisons":[{"field":"pending pointer","observed":"record-02","expected":"record-01"}]})
        )
    }

    fn append(messages: &mut Vec<Value>, call: &ToolCallData, result: Value) {
        messages.push(
            json!({"role":"assistant","tool_calls":[{"id":call.id,"function":{
            "name":call.name,"arguments":call.arguments}}]}),
        );
        messages.push(result);
    }

    #[test]
    fn only_the_exact_declared_check_captures_a_single_terminal_frame() {
        let messages = vec![json!({"role":"user","content":task()})];
        let good = frame();
        assert!(capture(&messages, CHECK, &good).is_some());
        assert!(capture(&messages, "test-probe other", &good).is_none());
        assert!(capture(&messages, CHECK, &format!("{good}{good}")).is_none());
        assert!(capture(&messages, CHECK, &format!("{good}extra output")).is_none());
        assert!(capture(
            &messages,
            CHECK,
            &good.replace("index.json", "../index.json")
        )
        .is_none());
        assert!(capture(&messages, CHECK, &good.replace(CASE, "different_case")).is_none());
        assert!(capture(
            &messages,
            CHECK,
            &good.replace("record-02", &"a".repeat(500))
        )
        .is_none());
        assert!(capture(
            &[json!({"role":"user","content":format!("```\n{}\n```",task())})],
            CHECK,
            &good
        )
        .is_none());
        assert!(capture(
            &[
                json!({"role":"user","content":"Inspect the test"}),
                json!({"role":"assistant","content":task()})
            ],
            CHECK,
            &good
        )
        .is_none());
        assert!(capture(&[json!({"role":"user","content":task().replace("\"kind\":\"test\"", "\"kind\":\"source\"")})],
                        CHECK,&good).is_none());
    }

    #[test]
    fn same_leaf_in_another_module_cannot_claim_the_observation() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap();
        std::fs::create_dir(dir.path().join("tests")).unwrap();
        std::fs::write(
            dir.path().join(TEST_PATH),
            "mod a { #[test] fn first_import() {} }\nmod b { #[test] fn first_import() {} }\n",
        )
        .unwrap();
        let call = ToolCallData {
            id: "case".into(),
            name: "exec".into(),
            arguments: json!({"command":CHECK,"cwd":root}).to_string(),
            thought_signature: None,
        };
        for (declared, accepted) in [
            (CASE.to_owned(), false),
            (format!("a::{CASE}"), false),
            (format!("b::{CASE}"), true),
        ] {
            let mut messages =
                vec![json!({"role":"user","content":task().replace(CASE,&declared)})];
            let mut focus = RecoveryFocus::default();
            let failure = format!(
                "FAILED (exit_code: 101)\ncwd: {root}\nthread 'b::{CASE}' panicked at {TEST_PATH}:2:3:\nwrong input"
            );
            let context = ExecVerificationContext::from_messages(None, &messages);
            focus.record_result(&call, &failure, Some(root), None, &context);
            let frame = capture(&messages, CHECK, &frame().replace(CASE, &declared)).unwrap();
            append(
                &mut messages,
                &call,
                frame.attach(focus.tool_message(&call, failure)),
            );
            let packet = serde_json::to_value(task_obligations::project_advisory(
                &messages,
                None,
                Some(root),
            ))
            .unwrap();
            assert_eq!(packet["required_diagnostic_read"]["path"], TEST_PATH);
            assert_eq!(
                packet["authored_test_failure"]["fixture_observation"].is_object(),
                accepted,
                "{declared}"
            );
        }
    }

    #[test]
    fn current_sample_reaches_both_roles_and_retires_on_mutation_or_new_result() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().to_str().unwrap();
        let test_path = temp.path().join(TEST_PATH);
        std::fs::create_dir_all(test_path.parent().unwrap()).unwrap();
        std::fs::write(
            &test_path,
            format!("{}assert!(result.is_ok());\n", "\n".repeat(11)),
        )
        .unwrap();
        let mut messages = vec![json!({"role":"user","content":task()})];
        let call = ToolCallData {
            id: "failure".into(),
            name: "exec".into(),
            arguments: json!({"command":CHECK,"cwd":root}).to_string(),
            thought_signature: None,
        };
        let failure = format!(
            "FAILED (exit_code: 101)\ncwd: {root}\nthread '{CASE}' panicked at {TEST_PATH}:12:3:\nwrong input"
        );
        let mut focus = RecoveryFocus::default();
        let context = ExecVerificationContext::from_messages(None, &messages);
        assert_eq!(context.classify(CHECK).kind, ExecKind::Verify);
        focus.record_result(&call, &failure, Some(root), None, &context);
        // The normal bounded tool text has already lost the sample.
        let result = focus.tool_message(&call, failure.clone());
        append(&mut messages, &call, result);
        let view = |history: &[Value]| {
            serde_json::to_value(task_obligations::project_advisory(
                history,
                None,
                Some(root),
            ))
            .unwrap()
        };
        assert!(view(&messages)["authored_test_failure"]["fixture_observation"].is_null());

        let frame = capture(&messages, CHECK, &frame()).expect("declared fixture frame");
        let result = frame.attach(messages.pop().unwrap());
        messages.push(result);
        let restored: Vec<Value> =
            serde_json::from_slice(&serde_json::to_vec(&messages).unwrap()).unwrap();
        let packet = view(&restored);
        assert_eq!(
            packet["authored_test_failure"]["fixture_observation"]["mismatched_fields"],
            json!(["pending pointer"]),
            "{packet}"
        );
        assert_eq!(packet["required_diagnostic_read"]["path"], TEST_PATH);
        let coder = task_obligations::prompt(&restored, None, Some(root)).unwrap();
        let observer = crate::tui::events::observer_assist::build_tui_next_action_prompt(
            &json!({"task_obligations":packet}),
            "en",
            "verification_failure",
        );
        for prompt in [coder, observer] {
            let mut lines = prompt.lines();
            assert_eq!(
                lines.next(),
                Some("[Expected inputs for the authored fixture]")
            );
            let target: Value = serde_json::from_str(lines.next().unwrap()).unwrap();
            assert_eq!(
                target,
                json!({"test_path":TEST_PATH,"case":CASE,
                "expected_inputs":{"pending pointer":"record-01"}})
            );
            assert!(lines.any(|line| line.contains("record-02")));
            assert!(prompt.contains("test-input correction"));
            assert!(prompt.contains("mandatory read or exact recheck first"));
        }
        let mut after_action = restored.clone();
        let action = ToolCallData {
            id: "partial-action".into(),
            name: "exec".into(),
            arguments: json!({"command":"mv part.rs tests/import.rs && false","cwd":root})
                .to_string(),
            thought_signature: None,
        };
        let context = ExecVerificationContext::from_messages(None, &after_action);
        assert_eq!(
            context.classify("mv part.rs tests/import.rs && false").kind,
            ExecKind::Action
        );
        let partial_failure = format!("FAILED (exit_code: 1)\ncwd: {root}\nstopped after a change");
        std::fs::write(
            &test_path,
            format!("{}assert!(new_result.is_ok());\n", "\n".repeat(12)),
        )
        .unwrap();
        focus.record_result(&action, &partial_failure, Some(root), None, &context);
        append(
            &mut after_action,
            &action,
            focus.tool_message(&action, partial_failure),
        );
        assert!(view(&after_action)["authored_test_failure"].is_null());
        assert_eq!(
            view(&after_action)["required_diagnostic_read"]["path"],
            TEST_PATH
        );

        let refreshed = ToolCallData {
            id: "fresh-check".into(),
            ..call.clone()
        };
        let failure_at_new_line = failure.replace(":12:3:", ":13:3:");
        let context = ExecVerificationContext::from_messages(None, &after_action);
        focus.record_result(&refreshed, &failure_at_new_line, Some(root), None, &context);
        append(
            &mut after_action,
            &refreshed,
            focus.tool_message(&refreshed, failure_at_new_line),
        );
        assert_eq!(
            view(&after_action)["authored_test_failure"]["sites"][0]["line"],
            13
        );

        let context = ExecVerificationContext::from_messages(None, &restored);
        assert!(project(&restored, "/other", CHECK, TEST_PATH, CASE, &context).is_none());
        for (tool, content, arguments) in [
            ("exec", failure.as_str(), call.arguments.clone()),
            (
                "patch_file",
                "OK: patched 'tests/import.rs' (+0 lines, 12 total)",
                json!({"path":TEST_PATH}).to_string(),
            ),
        ] {
            let mut next = restored.clone();
            let new = ToolCallData {
                id: "new".into(),
                name: tool.into(),
                arguments,
                thought_signature: None,
            };
            append(
                &mut next,
                &new,
                json!({"role":"tool","tool_call_id":"new","content":content}),
            );
            assert!(project(&next, root, CHECK, TEST_PATH, CASE, &context).is_none());
        }
    }
}
