use super::*;

#[path = "diagnostic_followup_tests/source_repair.rs"]
mod source_repair;
use crate::tui::agent::{self, AgenticStartState};

const CHECK: &str = "python3 -S -m unittest -v test_failure";
const FILE: &str = "test_failure.py";
const TASK: &str = "Fix src/value.py. Preserve test_failure.py.\nRun exactly: python3 -S -m unittest -v test_failure";

struct Trace {
    messages: Vec<Value>,
    focus: RecoveryFocus,
    root: tempfile::TempDir,
    path: std::path::PathBuf,
}

impl Trace {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().canonicalize().unwrap();
        std::fs::write(path.join(FILE), "assert actual == expected\n").unwrap();
        let messages = vec![
            json!({"role":"user","content":TASK}),
            json!({"role":"assistant","accepted_plan":{
                "goal":"Fix src/value.py and preserve test_failure.py",
                "steps":["Diagnose test_failure.py","Repair src/value.py","Run the exact check"],
                "acceptance_criteria":["Exact check passes"],
                "risks":"preserve supplied tests","assumptions":"failure is reproducible"
            }}),
        ];
        Self {
            messages,
            focus: RecoveryFocus::default(),
            root,
            path,
        }
    }

    fn root(&self) -> Option<&str> {
        self.path.to_str()
    }

    fn append(&mut self, id: &str, name: &str, args: Value, output: String) {
        let call = ToolCallData {
            id: id.into(),
            name: name.into(),
            arguments: args.to_string(),
            thought_signature: None,
        };
        let ctx = ExecVerificationContext::from_messages(Some(CHECK), &self.messages);
        self.focus
            .record_result(&call, &output, self.path.to_str(), Some(CHECK), &ctx);
        self.messages
            .push(json!({"role":"assistant","tool_calls":[call.to_json()]}));
        self.messages.push(self.focus.tool_message(&call, output));
    }

    fn fail(&mut self, id: &str, actual: u32, elapsed: &str) {
        let body = format!(
            "FAIL: test_value (test_failure.Check.test_value)\n  File \"{}\", line 1, in test_value\nAssertionError: {actual} != 3\n\nRan 1 test in {elapsed}s\n\nFAILED (failures=1)\n",
            self.path.join(FILE).display()
        );
        let result = agent::inject_cwd(
            &agent::build_failed_tool_output("", &body, 1),
            &format!("cwd: {}", self.path.display()),
            None,
        );
        self.append(id, "exec", json!({"command":CHECK}), result);
        assert!(self.focus.requires_read());
    }

    fn fail_with_clipping(&mut self, id: &str, elapsed: &str) {
        self.fail(id, 2, elapsed);
        let result = &mut self.messages.last_mut().unwrap()["content"];
        *result = json!(format!(
            "{}\n[... output omitted ...]",
            result.as_str().unwrap()
        ));
    }

    fn read(&mut self, id: &str) {
        let (result, failed) = crate::file_tools::tool_read_file(FILE, self.root());
        assert!(!failed);
        self.append(id, "read_file", json!({"path":FILE}), result);
        assert!(self.focus.observed());
    }

    fn view(&self) -> Option<DiagnosticFollowup> {
        self.focus.diagnostic_followup(&self.messages, self.root())
    }
}

#[test]
fn repeated_diagnosis_projects_only_the_current_read_backed_failure() {
    let mut trace = Trace::new();
    trace.fail("f1", 2, "0.031");
    trace.read("r1");
    assert!(trace.view().is_none());
    trace.fail("f2", 2, "0.129");
    assert!(trace.view().is_none(), "the current read remains mandatory");
    trace.read("r2");
    let followup = trace.view().unwrap();
    assert!(followup.should_yield());
    assert_eq!(
        serde_json::to_value(&followup).unwrap(),
        json!({
            "failed_check":CHECK,"failure_path":FILE,"earlier_failure_call_id":"f1",
            "latest_failure_call_id":"f2","diagnostic_read_call_id":"r2","matching_failures":2,
            "diagnostic":trace.focus.pending.as_ref().unwrap().diagnostic
        })
    );
    assert!(trace
        .focus
        .diagnostic_followup(&trace.messages, Some("/other"))
        .is_none());
    trace.read("r2-again");
    assert!(!trace.view().unwrap().should_yield());
    trace.fail("f3", 2, "1.29");
    trace.read("r3");
    assert_eq!(trace.view().unwrap().matching_failures, 3);
    assert!(
        !trace.view().unwrap().should_yield(),
        "one early return per unchanged chain"
    );
    trace.fail("changed", 9, "0.04");
    trace.read("changed-read");
    assert!(
        trace.view().is_none(),
        "a different failure needs its own diagnosis"
    );
}

#[test]
fn comparison_ignores_padding_only_before_the_clipped_assertion_operator() {
    let original = "AssertionError: {left} != {right}\n[... output omitted ...]\n!= {'used': None}\n\nRan 3 tests in 10.158s\n\nFAILED (failures=3)";
    let padded = original
        .replace("\n!= ", "\n != ")
        .replace("10.158s", "1.581s");
    assert_eq!(comparable_output(original), comparable_output(&padded));
    for changed in [
        padded.replace("'used': None", "'used': 3"),
        padded.replace("[... output omitted ...]", "Assertion context:"),
        padded.replace("\n != ", "\n   different operand "),
    ] {
        assert_ne!(comparable_output(original), comparable_output(&changed));
    }
}

#[test]
fn other_processes_and_edits_break_the_comparison_but_refusals_do_not_count() {
    for (tool, arguments, output) in [
        (
            "exec",
            json!({"command":"git status --short"}),
            "OK (exit_code: 0)",
        ),
        (
            "patch_file",
            json!({"path":"src/value.py"}),
            "OK: patched 'src/value.py'",
        ),
    ] {
        let mut trace = Trace::new();
        trace.fail("f1", 2, "0.031");
        trace.read("r1");
        trace.append("change", tool, arguments, output.into());
        trace.fail("f2", 2, "0.039");
        trace.append(
            "refused",
            "read_file",
            json!({"path":FILE}),
            "REJECTED BY USER".into(),
        );
        assert!(trace.view().is_none());
        trace.read("r2");
        assert!(trace.view().is_none());
    }
}

#[test]
fn changed_read_content_ends_the_comparison() {
    for already_observed in [false, true] {
        let mut trace = Trace::new();
        trace.fail("f1", 2, "0.031");
        trace.read("r1");
        trace.fail("f2", 2, "0.039");
        if already_observed {
            trace.read("r2");
            assert!(trace.view().is_some());
        }
        std::fs::write(trace.path.join(FILE), "assert actual == other_expected\n").unwrap();
        trace.read("changed-read");
        assert!(trace.view().is_none());
    }
}

#[cfg(unix)]
#[test]
fn first_focused_comparison_retains_diagnosis_without_verification() {
    const OTHER_CHECK: &str = "cargo test --locked --bin spiral-coder -q";
    let mut trace = Trace::new();
    trace.messages[0]["content"] = json!(format!("{TASK}\nAlso run exactly: {OTHER_CHECK}"));
    trace.append(
        "passing-check",
        "exec",
        json!({"command":OTHER_CHECK}),
        format!("OK (exit_code: 0)\ncwd: {}", trace.path.display()),
    );
    std::fs::write(
        trace.path.join(FILE),
        "import unittest\n\nclass Check(unittest.TestCase):\n    def test_value(self):\n        self.assertEqual({'used': 2}, {'used': 3})\n",
    )
    .unwrap();
    trace.fail_with_clipping("f1", "0.01");
    assert!(trace.view().is_none());
    trace.read("r1");
    let followup = trace.view().unwrap();
    assert!(followup.should_yield());
    assert_eq!(followup.matching_failures, 1);
    let view = serde_json::to_value(agent::task_obligations::project_advisory(
        &trace.messages,
        Some(CHECK),
        trace.root(),
    ))
    .unwrap();
    let followup_view = &view["diagnostic_followup"];
    assert!(followup_view.get("earlier_failure_call_id").is_none());
    assert_eq!(
        followup_view["unittest_comparison"]["checks_to_refresh"],
        json!([OTHER_CHECK]),
        "required={}, pending={}",
        view["required_exact_checks"],
        view["pending_exact_checks"]
    );
    let args = &followup_view["unittest_comparison"]["arguments"];
    let command = args["command"].as_str().unwrap();
    let root = trace.root().unwrap();
    let call = ToolCallData {
        id: "comparison".into(),
        name: "exec".into(),
        arguments: args.to_string(),
        thought_signature: None,
    };
    assert!(trace
        .focus
        .matches_current_focused_exec(&call, &trace.messages, Some(root)));
    assert!(!trace.focus.matches_current_focused_exec(
        &call,
        &trace.messages[..trace.messages.len() - 2],
        Some(root),
    ));
    for changed in [
        json!({"command":command,"cwd":"/other"}),
        json!({"command":format!("{command} && cargo fmt"),"cwd":root}),
        json!({"command":command,"cwd":root,"unexpected":true}),
    ] {
        let changed_call = ToolCallData {
            arguments: changed.to_string(),
            ..call.clone()
        };
        assert!(!trace.focus.matches_current_focused_exec(
            &changed_call,
            &trace.messages,
            Some(root),
        ));
    }
    let id = followup_view["comparison_check_id"].as_str().unwrap();
    let selection = json!({"check_id":id});
    let catalog = agent::check_catalog::Catalog::from_root(TASK).with_comparison(Some(&followup));
    let mut selected = ToolCallData {
        arguments: selection.to_string(),
        ..call.clone()
    };
    let mut stale = selected.clone();
    assert!(agent::check_catalog::Catalog::from_root(TASK)
        .resolve(&mut stale, Some(root))
        .is_err());
    let mut tools = json!([agent::exec_tool_def()]);
    catalog
        .bind_for(&mut tools, "https://example.test/v1", None)
        .unwrap();
    assert!(
        tools[0]["function"]["parameters"]["properties"]["check_id"]["enum"]
            .as_array()
            .unwrap()
            .contains(&json!(id))
    );
    let resolved = catalog.resolve(&mut selected, Some(root)).unwrap().unwrap();
    assert_eq!(resolved["command"], command);
    assert!(trace
        .focus
        .matches_current_focused_exec(&selected, &trace.messages, Some(root)));
    let session = crate::agent_session::AgentSession::new(
        Some(root.into()),
        None,
        None,
        None,
        trace.messages.clone(),
    );
    let advice = serde_json::from_value(json!({"summary":"Compare the observed assertion",
        "suggestions":[{"kind":"verify","reason":"Read the differing public values","confidence":0.98,
        "suggested_tool":"exec","suggested_args":selection}]})).unwrap();
    let observation = agent::observer_continuation::Observation::from_advice(
        &advice,
        &session,
        Some(CHECK),
        root,
    )
    .unwrap();
    assert!(observation.guidance().contains(&selection.to_string()));
    assert!(observation.matches_accepted_call(
        &selected,
        "<observer_response>\ndecision: accept\n</observer_response>"
    ));
    let mut bad_id: Value = serde_json::from_str(&selected.arguments).unwrap();
    bad_id["check_id"] = json!("unittest:stale");
    assert!(!followup.matches_unittest_comparison(&bad_id));
    let process = std::process::Command::new("sh")
        .args(["-c", resolved["command"].as_str().unwrap()])
        .current_dir(root)
        .output()
        .unwrap();
    assert_eq!(process.status.code(), Some(1));
    let stdout = format!(
        "ordinary test output\nstderr:\n{}",
        String::from_utf8(process.stdout).unwrap()
    );
    assert!(stdout.contains("- {'used': 2}"));
    assert!(stdout.contains("+ {'used': 3}"));
    let receipt = followup
        .observed_comparison(command, root, &stdout)
        .unwrap();
    assert!(followup
        .observed_comparison(command, "/other", &stdout)
        .is_none());
    assert!(followup
        .observed_comparison(command, root, "unrelated failure")
        .is_none());

    let body = agent::inject_cwd(
        &agent::build_failed_tool_output(&stdout, "", 1),
        &format!("cwd: {root}"),
        None,
    );
    trace.append(
        "comparison",
        "exec",
        serde_json::from_str(&selected.arguments).unwrap(),
        body,
    );
    let focus = RecoveryFocus::from_messages(&trace.messages, trace.root(), Some(CHECK));
    assert!(!focus.progress_survives(&trace.messages, trace.root(), Some(CHECK)));
    let comparison_result = trace.messages.pop().unwrap();
    trace.messages.push(receipt.attach(comparison_result));
    let focus = RecoveryFocus::from_messages(&trace.messages, trace.root(), Some(CHECK));
    assert!(focus.observed() && focus.check_matches(CHECK));
    assert!(focus.progress_survives(&trace.messages, trace.root(), Some(CHECK)));
    assert!(crate::task_origin::provider_messages(&trace.messages)
        .last()
        .unwrap()
        .get(METADATA_KEY)
        .is_none());
    assert!(
        trace.view().is_none(),
        "an executed comparison consumes the offer"
    );
    let pending =
        agent::task_obligations::project_advisory(&trace.messages, Some(CHECK), trace.root())
            .pending_exact_checks;
    assert_eq!(pending, vec![CHECK, OTHER_CHECK]);

    // The comparison belongs to the observed failure. A subsequent repair
    // changes today's phase, but the same red comparison still has its owner.
    let repaired = {
        let mut history = trace.messages.clone();
        let repair = ToolCallData {
            id: "repair-after-comparison".into(),
            name: "patch_file".into(),
            arguments: json!({"path":"src/counter.rs"}).to_string(),
            thought_signature: None,
        };
        let result = "OK: patched 'src/counter.rs' (+1 line)".to_owned();
        let mut focus = RecoveryFocus::from_messages(&history, trace.root(), Some(CHECK));
        focus.record_result(
            &repair,
            &result,
            trace.root(),
            Some(CHECK),
            &ExecVerificationContext::from_messages(Some(CHECK), &history),
        );
        history.push(json!({"role":"assistant","tool_calls":[repair.to_json()]}));
        history.push(focus.tool_message(&repair, result));
        history
    };
    let mut governor = agent::RecoveryGovernor {
        stage: Some(agent::RecoveryStage::Diagnose),
        ..Default::default()
    };
    governor.restore_focus(&repaired, trace.root(), Some(CHECK));
    assert_eq!(governor.stage, Some(agent::RecoveryStage::Verify));
    assert!(governor.focus.check_matches(CHECK));
    let mut later_failure = repaired;
    let unrelated = ToolCallData {
        id: "unrelated-red".into(),
        name: "exec".into(),
        arguments: json!({"command":"check another invariant"}).to_string(),
        thought_signature: None,
    };
    later_failure.extend([json!({"role":"assistant","tool_calls":[unrelated.to_json()]}),
        json!({"role":"tool","tool_call_id":unrelated.id,"content":format!("FAILED (exit_code: 1)\ncwd: {}",trace.path.display())})]);
    governor.stage = Some(agent::RecoveryStage::Diagnose);
    governor.restore_focus(&later_failure, trace.root(), Some(CHECK));
    assert_eq!(governor.stage, Some(agent::RecoveryStage::Diagnose));

    // Both the native failing check and the read must still be correlated.
    let missing_read: Vec<_> = trace
        .messages
        .iter()
        .filter(|message| message["tool_call_id"] != "r1" && message["tool_calls"][0]["id"] != "r1")
        .cloned()
        .collect();
    let focus = RecoveryFocus::from_messages(&missing_read, trace.root(), Some(CHECK));
    assert!(!focus.progress_survives(&missing_read, trace.root(), Some(CHECK)));

    let comparison = trace.messages.split_off(trace.messages.len() - 2);
    trace.append(
        "intervening-process",
        "exec",
        json!({"command":"git status --short"}),
        format!("OK (exit_code: 0)\ncwd: {}", trace.path.display()),
    );
    trace.messages.extend(comparison);
    let focus = RecoveryFocus::from_messages(&trace.messages, trace.root(), Some(CHECK));
    assert!(!focus.progress_survives(&trace.messages, trace.root(), Some(CHECK)));
}

#[test]
fn another_window_of_the_same_clipped_failure_does_not_yield_again() {
    let mut trace = Trace::new();
    std::fs::write(
        trace.path.join(FILE),
        "assert actual == expected\n# context\n# end\n",
    )
    .unwrap();
    let read_window = |trace: &mut Trace, id: &str, end: usize| {
        let args = json!({"path": FILE, "start_line": 1, "end_line": end});
        let (output, failed) = crate::file_tools::read_range::execute_args(&args, trace.root());
        assert!(!failed);
        trace.append(id, "read_file", args, output);
    };
    trace.fail_with_clipping("first-failure", "0.01");
    read_window(&mut trace, "first-window", 1);
    assert!(trace.view().unwrap().should_yield());
    read_window(&mut trace, "wider-window", 2);
    let followup = trace.view().unwrap();
    assert!(followup.has_unittest_comparison());
    assert!(!followup.should_yield());
    trace.fail_with_clipping("same-failure-again", "0.02");
    read_window(&mut trace, "narrow-window", 1);
    assert!(!trace.view().unwrap().should_yield());
}

#[test]
fn rerunning_the_same_clipped_failure_does_not_yield_again_after_its_read() {
    let mut trace = Trace::new();
    trace.fail_with_clipping("f1", "0.01");
    trace.read("r1");
    assert!(trace.view().unwrap().should_yield());
    trace.fail_with_clipping("f2", "0.02");
    trace.read("r2");
    let followup = trace.view().unwrap();
    assert!(followup.has_unittest_comparison());
    assert!(!followup.should_yield());
}

#[tokio::test]
async fn repeated_diagnosis_returns_before_another_coder_call_and_forwards_a_different_read() {
    use agent::RealizePreset;
    use clap::Parser;
    use httpmock::prelude::*;
    let mut trace = Trace::new();
    trace.fail("f1", 2, "0.031");
    trace.read("r1");
    trace.fail("f2", 2, "0.045");
    let server = MockServer::start();
    let diagnostic = server.mock(|when, then| {
        when.method(POST).path("/chat/completions")
            .body_contains("Native verification failed before continuation")
            .matches(|req| !String::from_utf8_lossy(req.body.as_deref().unwrap_or_default()).contains("fresh-read"));
        let answer = "<reflect>\nlast_outcome: failure\ngoal_delta: same\nwrong_assumption: the check would pass\nstrategy_change: adjust\nnext_minimal_action: read test_failure.py\n</reflect>\n<think>\ngoal: repair the failed check\nstep: 1\ntool: read_file\nrisk: none\ndoubt: exact cause unknown\nnext: read test_failure.py\nverify: run the exact check after the fix\n</think>";
        let chunk = json!({"choices":[{"delta":{"content":answer,"tool_calls":[{"index":0,"id":"fresh-read","function":{
            "name":"read_file","arguments":json!({"path":FILE}).to_string()
        }}]},"finish_reason":"tool_calls"}]});
        then.status(200).header("content-type","text/event-stream")
            .body(format!("data: {chunk}\n\ndata: [DONE]\n\n"));
    });
    let duplicate = server.mock(|when, then| {
        when.method(POST)
            .path("/chat/completions")
            .body_contains("fresh-read");
        then.status(500)
            .body("Another Coder request preceded the diagnostic handoff");
    });
    let cfg = crate::Cli::try_parse_from([
        "spiral-coder",
        "--provider",
        "openai",
        "--base-url",
        &server.base_url(),
        "--api-key",
        "offline",
        "--timeout-seconds",
        "3",
    ])
    .unwrap()
    .common
    .to_partial_config()
    .resolve()
    .unwrap();
    let budget = crate::work_budget::WorkBudget::new(TASK, 8, None);
    let (tx, mut rx) = tokio::sync::mpsc::channel(512);
    let root = trace.path.to_str().unwrap();
    let start = AgenticStartState {
        messages: trace.messages,
        checkpoint: None,
        cur_cwd: None,
        observation_cache: None,
        session_bridge: None,
        create_checkpoint: false,
    };
    let result = crate::work_budget::scope(
        Some(crate::work_budget::Context {
            budget: budget.with_observer_return(1),
            role: "coder",
        }),
        Box::pin(agent::run_agentic_json(
            start,
            &cfg,
            Some(root),
            1,
            tx,
            None,
            None,
            Some(CHECK.into()),
            false,
            Some(RealizePreset::Off),
            None,
            &crate::approvals::AutoApprover,
        )),
    )
    .await
    .unwrap();
    diagnostic.assert_hits(1);
    duplicate.assert_hits(0);
    assert!(result
        .messages
        .iter()
        .any(|m| m["tool_call_id"] == "fresh-read"
            && m["content"]
                .as_str()
                .is_some_and(|body| body.contains("assert actual == expected"))));
    let mut yielded = false;
    while let Some(token) = rx.recv().await {
        if let crate::streaming::StreamToken::Telemetry(event) = token {
            if event.event == "agent_outcome" {
                assert_eq!(event.data["completed"], false);
                yielded = event.data["yield_reason"] == "diagnostic_followup";
            }
        }
    }
    assert!(yielded);
    let reply = json!({"summary":"Inspect the calculation against the known assertion.",
        "suggestions":[{"kind":"read","reason":"Compare the calculation with the expected value of 3.",
        "confidence":0.95,"suggested_tool":"read_file","suggested_args":{"path":"src/value.py"},
        "based_on":["task_obligations"]}]}).to_string();
    let check = agent::task_obligations::prompt(&result.messages, Some(CHECK), Some(root)).unwrap();
    assert!(check.contains("one different bounded observation"));
    let observed = crate::observer_action::evaluate(crate::observer_action::Request {
        packet: json!({"tool_root":root,"work_budget":{"limit":8,"used":1}}),
        messages: result.messages,
        test_command: Some(CHECK.into()),
        lang: "en".into(),
        reason: "diagnostic_followup".into(),
        response: Some(reply),
        final_summary: None,
        repair_native: false,
    });
    assert!(observed.issue.is_none());
    assert!(observed
        .hint
        .unwrap()
        .contains("read_file(path=src/value.py)"));
    assert!(observed
        .prompt
        .contains("one different bounded observation"));
    assert_eq!(
        observed.packet["task_obligations"]["pending_exact_checks"],
        json!([CHECK])
    );
    drop(trace.root);
}

#[test]
fn named_scalar_subtests_reach_both_roles_after_the_native_test_read() {
    let mut trace = Trace::new();
    let frame = format!(
        "  File \"{}\", line 1, in test_value\n",
        trace.path.join(FILE).display()
    );
    let body = format!(
        "FAIL: test_value (test_failure.Check.test_value) (mode='active')\n{frame}AssertionError: 0 == 0 : accepted active\n\n\
         FAIL: test_value (test_failure.Check.test_value) (mode='saved')\n{frame}AssertionError: 1 != 2 : wrong saved count\n\n\
         Ran 1 test in 0.1s\n\nFAILED (failures=2)\n"
    );
    let result = agent::inject_cwd(
        &agent::build_failed_tool_output("", &body, 1),
        &format!("cwd: {}", trace.path.display()),
        None,
    );
    trace.append("failed", "exec", json!({"command":CHECK}), result);
    assert!(trace.view().is_none());
    trace.read("test");
    let followup = trace.view().unwrap();
    assert!(followup.should_yield());
    let view = serde_json::to_value(followup).unwrap();
    assert_eq!(
        view["reported_subtests"],
        json!({
            "method":"test_failure.Check.test_value",
            "cases":[{"parameters":"(mode='active')","assertion":"0 == 0 : accepted active"},
                     {"parameters":"(mode='saved')","assertion":"1 != 2 : wrong saved count"}]
        })
    );
    assert!(view.get("unittest_comparison").is_none());
    assert_eq!(view["latest_failure_call_id"], "failed");
    for prompt in [
        agent::task_obligations::prompt(&trace.messages, Some(CHECK), trace.root()).unwrap(),
        crate::observer_action::intervention::prompt(
            &json!({"task_obligations":{"diagnostic_followup":view}}),
        ),
    ] {
        assert!(prompt.contains("wrong saved count") && prompt.contains("which rows"));
    }
    trace.read("same-test");
    assert!(!trace.view().unwrap().should_yield());
    // A later check owns its own cases and removes the earlier advisory.
    trace.fail("new-failure", 9, "0.02");
    trace.read("new-test");
    assert!(!trace.view().is_some_and(|f| f.has_reported_subtests()));
}

#[test]
fn compare_the_complete_public_failures_across_one_landed_source_patch() {
    for scenario in [
        "same",
        "changed_assertion",
        "different_case",
        "unchanged_file",
        "unrelated_exec",
        "other_path",
        "wrong_cwd",
    ] {
        let mut trace = Trace::new();
        let contract = json!({"version":1,"requirements":[{"id":"catalog","description":"Validate the catalog",
            "artifacts":[{"path":"src/value.py","kind":"source"},{"path":FILE,"kind":"test"}],
            "checks":[{"command":CHECK}]}]});
        trace.messages[0]["content"] = json!(format!(
            "{TASK}\n<task_requirements>\n{contract}\n</task_requirements>"
        ));
        let failed = |trace: &mut Trace, id: &str, name: &str, actual: usize, correct_cwd: bool| {
            let frame = format!(
                "  File \"{}\", line 1, in test_value",
                trace.path.join(FILE).display()
            );
            let body=format!("FAIL: test_value (test_failure.Check.test_value) (item='first')\n{frame}\nAssertionError: 0 != 2\nFAIL: test_value (test_failure.Check.test_value) (item='{name}')\n{frame}\nAssertionError: {actual} != 2\n\nRan 1 test in 0.1s\n\nFAILED (failures=2)\n");
            let cwd = if correct_cwd {
                trace.root().unwrap()
            } else {
                "/another-workspace"
            };
            let output = agent::inject_cwd(
                &agent::build_failed_tool_output("", &body, 1),
                &format!("cwd: {cwd}"),
                None,
            );
            trace.append(id, "exec", json!({"command":CHECK}), output);
        };
        failed(&mut trace, "before", "second", 1, scenario != "wrong_cwd");
        if scenario != "wrong_cwd" {
            trace.read("earlier-read");
        }
        let path = if scenario == "other_path" {
            "other.py"
        } else {
            "src/value.py"
        };
        let receipt = if scenario == "unchanged_file" {
            format!("OK: unchanged '{path}'")
        } else {
            format!("OK: patched '{path}'")
        };
        trace.append(
            "patch",
            "patch_file",
            json!({"path":path,"search":"old","replace":"new"}),
            receipt,
        );
        if scenario == "unrelated_exec" {
            trace.append(
                "barrier",
                "exec",
                json!({"command":"python3 other.py"}),
                "OK (exit_code: 0)".into(),
            );
        }
        failed(
            &mut trace,
            "after",
            if scenario == "different_case" {
                "third"
            } else {
                "second"
            },
            if scenario == "changed_assertion" {
                3
            } else {
                1
            },
            true,
        );
        trace.read("after-read");
        let followup = serde_json::to_value(trace.view().expect("two labeled assertions")).unwrap();
        if scenario == "same" {
            assert_eq!(
                followup["unchanged_after_patch"],
                json!({"before_check_call_id":"before","patch_call_id":"patch","path":"src/value.py","repeated_assertions":2})
            );
            let coder = agent::task_obligations::prompt(&trace.messages, Some(CHECK), trace.root())
                .unwrap();
            let observer = crate::observer_action::intervention::prompt(
                &json!({"task_obligations":{"diagnostic_followup":followup}}),
            );
            assert!(
                coder.contains("Inspect the caller") && observer.contains("Inspect the caller")
            );
        } else {
            assert!(
                followup.get("unchanged_after_patch").is_none(),
                "{scenario}: {followup}"
            );
        }
    }
}
