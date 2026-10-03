use super::*;
use crate::runtime_eval::{evaluate_case, RuntimeEvalArtifacts, RuntimeEvalCase};

fn exchange(id: &str, name: &str, content: &str) -> [serde_json::Value; 2] {
    [
        json!({"role":"assistant","tool_calls":[{"id":id,"function":{"name":name,"arguments":"{\"path\":\"src/lib.rs\"}"}}]}),
        json!({"role":"tool","tool_call_id":id,"content":content}),
    ]
}

#[test]
fn old_patch_auto_test_evidence_survives_digest_pruning_and_evaluation() {
    let case: RuntimeEvalCase = serde_json::from_value(json!({
        "id":"pruned-auto-test", "prompt":"Fix the file and run its configured test",
        "checks":[{"kind":"auto_test_passed","command":"cargo test"}]
    }))
    .unwrap();
    for (status, expected) in [
        ("[auto-test] ✓ PASSED (exit 0)", true),
        ("[auto-test] ✗ FAILED (exit 1)", false),
        ("[auto-test] ✓ PASSED (exit 0) ", false),
    ] {
        let root = tempfile::tempdir().unwrap();
        let artifacts = RuntimeEvalArtifacts {
            case_dir: root.path().to_path_buf(),
            trace_path: root.path().join("trace.jsonl"),
            session_path: root.path().join("session.json"),
            json_path: root.path().join("final.json"),
            graph_path: root.path().join("graph.json"),
        };
        std::fs::write(&artifacts.trace_path, "{\"event\":\"verification_config\",\"data\":{\"test_command\":\"cargo test\"}}\n{\"event\":\"agent_outcome\",\"data\":{\"completed\":true}}\n{\"event\":\"agent_end\",\"data\":{\"ok\":true}}\n").unwrap();
        let result = format!(
            "OK: patched 'src/lib.rs'\n{}{status}\nstdout:\n[auto-test] ✓ PASSED (exit 0)\n{}",
            "[hash] diagnostic detail\n".repeat(30),
            "test output\n".repeat(100)
        );
        let compacted = compact_success_tool_result_for_history("patch_file", &result);
        assert_eq!(
            compacted
                .lines()
                .find(|line| line.starts_with("[auto-test]")),
            Some(status)
        );
        let mut messages =
            vec![json!({"role":"user","content":"Fix the file and run its configured test"})];
        messages.extend(exchange(
            "read",
            "read_file",
            "[src/lib.rs] (2 lines, 16 bytes)\nold code\n",
        ));
        messages.extend(exchange("patch", "patch_file", &compacted));
        for index in 0..KEEP_RECENT_TOOL_TURNS + 8 {
            messages.extend(exchange(
                &format!("read-{index}"),
                "read_file",
                "[src/lib.rs] (2 lines, 16 bytes)\nnew code\n",
            ));
        }
        prune_old_tool_results(&mut messages);
        prune_message_window(&mut messages);
        let pruned = messages
            .iter()
            .find(|msg| msg["tool_call_id"] == "patch")
            .unwrap()["content"]
            .as_str()
            .unwrap();
        assert!(pruned.contains("[pruned "));
        assert_eq!(
            pruned
                .lines()
                .filter(|line| line.starts_with("[auto-test]"))
                .collect::<Vec<_>>(),
            vec![status]
        );
        std::fs::write(
            &artifacts.json_path,
            json!({"messages":messages}).to_string(),
        )
        .unwrap();
        let report =
            evaluate_case(&case, root.path().to_str().unwrap(), artifacts, 0, None).unwrap();
        assert_eq!(report.ok, expected, "{status:?}");
        assert_eq!(
            report.metrics.fresh_auto_test_pass_count,
            usize::from(expected)
        );
        let (_, mutation, _, behavioral, _) =
            restore_done_gate_from_messages(&messages, Some("cargo test"));
        assert_eq!(
            behavioral.is_some(),
            expected,
            "resume must agree with the evaluator"
        );
        if expected {
            assert_eq!(behavioral, mutation);
        }
    }
}

#[test]
fn replay_check_failures_survive_the_exec_stderr_budget() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("replay.json");
    let mut spec: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/runtime-benchmark-plan-tui-replay/.spiral-coder/tui_replay.json"
    ))
    .unwrap();
    // Parseable JSON and descriptive text are not a queued Observer action.
    spec["cases"][0]["observer_response"]["suggestions"] = json!([]);
    std::fs::write(&path, spec.to_string()).unwrap();
    let report =
        crate::tui_replay::replay_spec_for_test(&path, root.path(), &root.path().join("reports"))
            .unwrap();
    assert_eq!(report.summary.failed, 1);
    let stderr = format!(
        "{}\nError: {}",
        "[tui-replay] progress details\n".repeat(100),
        crate::tui_replay::diagnostics::failed_report(&report)
    );
    let rendered = build_failed_tool_output("", &stderr, 1);
    assert!(rendered.contains("[ERROR DIGEST"));
    assert!(rendered.contains("\"check\":\"hint_queued\""));
    assert!(rendered.contains("hint_queued=false"));
    assert!(rendered.contains("quickest_check alone does not queue a hint"));
    assert!(rendered.contains(&report.out_dir.join("report.json").display().to_string()));
    assert!(!rendered.contains("[auto-test] ✓ PASSED"));

    spec["cases"][0]["checks"] = json!([{"kind":"target_message_contains"}]);
    std::fs::write(&path, spec.to_string()).unwrap();
    let error = crate::tui_replay::replay_spec_for_test(
        &path,
        root.path(),
        &root.path().join("invalid-reports"),
    )
    .unwrap_err();
    let stderr = format!(
        "{}\nError: {error}",
        "[tui-replay] progress details\n".repeat(100)
    );
    let rendered = build_failed_tool_output("", &stderr, 1);
    assert!(rendered.contains("missing field `value`"));
    assert!(rendered.contains(r#"{"kind":"target_message_contains","value":"coder-0"}"#));
    assert!(!root.path().join("invalid-reports").exists());
}

#[test]
fn failed_auto_test_target_diagnostic_precedes_edit_context() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("replay.json");
    let spec = json!({"version":1,"cases":[{
        "id":"missing-assistant-target", "prompt":"Inspect the requested target",
        "coder_messages":[{"role":"user","content":"Inspect the requested target"}],
        "observer_response":{"summary":"Recorded advice", "suggestions":[]},
        "checks":[{"kind":"target_message_contains","value":"coder-0"}]
    }]});
    std::fs::write(&path, spec.to_string()).unwrap();
    let error =
        crate::tui_replay::replay_spec_for_test(&path, root.path(), &root.path().join("reports"))
            .unwrap_err();
    let result = format!(
        "OK: patched 'replay.json'\n[hash] before=old after=new\n{}\n[auto-test] ✗ FAILED (exit 1)\nError: {error}\n{}",
        "[diff] source context\nError: decoy in changed source\n".repeat(20),
        "[auto-test] ✓ PASSED (exit 0)\nstdout filler\n".repeat(30),
    );
    let compacted = compact_success_tool_result_for_history("patch_file", &result);
    assert!(compacted.contains("could not infer a stuck target from coder_messages"));
    assert!(
        compacted.contains("completed failure-like assistant message in top-level coder_messages")
    );
    assert!(compacted.contains("selector=msg:coder-<index>"));
    assert!(compacted
        .contains("Changing target_message_contains.value cannot create or select a target"));
    assert!(compacted.find("could not infer").unwrap() < compacted.find("[diff]").unwrap());
    assert!(!crate::execution_evidence::auto_test_succeeded(
        "patch_file",
        &compacted
    ));
    assert!(compacted.lines().count() <= SUCCESS_TOOL_HISTORY_MAX_LINES + 1);
    assert!(!root.path().join("reports").exists());
}

#[test]
fn failed_auto_test_without_error_prefix_keeps_assertion_before_diff() {
    let result = format!(
        "OK: wrote 'src/lib.rs'\n{}\n[auto-test] ✗ FAILED (exit 101)\nrunning 1 test\nthread 'behavior' panicked at src/lib.rs:12: assertion failed\n",
        "[diff] changed content\n".repeat(30),
    );
    let compacted = compact_success_tool_result_for_history("write_file", &result);
    assert!(
        compacted.find("thread 'behavior' panicked").unwrap() < compacted.find("[diff]").unwrap()
    );
    assert!(!crate::execution_evidence::auto_test_succeeded(
        "write_file",
        &compacted
    ));
}
