use super::*;
use serde_json::json;
use tempfile::TempDir;

fn case(checks: Vec<RuntimeEvalCheck>) -> RuntimeEvalCase {
    serde_json::from_value(
        json!({"id":"truthfulness", "prompt":"Complete the requested work", "checks":checks}),
    )
    .unwrap()
}

fn fixture(dir: &TempDir, trace: &str, messages: Value) -> RuntimeEvalArtifacts {
    let artifacts = RuntimeEvalArtifacts {
        case_dir: dir.path().to_path_buf(),
        trace_path: dir.path().join("trace.jsonl"),
        session_path: dir.path().join("session.json"),
        json_path: dir.path().join("final.json"),
        graph_path: dir.path().join("graph.json"),
    };
    std::fs::write(&artifacts.trace_path, trace).unwrap();
    std::fs::write(
        &artifacts.json_path,
        json!({"messages":messages}).to_string(),
    )
    .unwrap();
    artifacts
}

fn successful_trace() -> &'static str {
    "{\"event\":\"agent_outcome\",\"data\":{\"completed\":true}}\n{\"event\":\"agent_end\",\"data\":{\"ok\":true}}\n"
}

#[test]
fn custom_checks_cannot_skip_completion_or_error_gates() {
    let dir = tempfile::tempdir().unwrap();
    for trace in [
        "{\"event\":\"done\"}\n{\"event\":\"agent_end\",\"data\":{\"ok\":true}}\n",
        "{\"event\":\"agent_outcome\",\"data\":{\"completed\":false}}\n{\"event\":\"done\"}\n{\"event\":\"agent_end\",\"data\":{\"ok\":true}}\n",
        "{\"event\":\"agent_outcome\",\"data\":{\"completed\":true}}\n{\"event\":\"error\"}\n{\"event\":\"agent_end\",\"data\":{\"ok\":true}}\n",
    ] {
        let artifacts = fixture(&dir, trace, json!([{"role":"assistant","content":"Found src/main.rs, still investigating."}]));
        let report = evaluate_case(&case(vec![RuntimeEvalCheck::AssistantContains { value: "src/main.rs".into() }]), dir.path().to_str().unwrap(), artifacts, 0, None).unwrap();
        assert!(!report.ok);
        assert_eq!(report.checks.iter().filter(|check| check.label=="completed").count(), 1);
        assert_eq!(report.checks.iter().filter(|check| check.label=="error_free").count(), 1);
    }
}

#[test]
fn outcome_must_belong_to_the_latest_round_and_clean_run_end() {
    let dir = tempfile::tempdir().unwrap();
    for suffix in [
        "{\"event\":\"round_start\"}\n{\"event\":\"agent_end\",\"data\":{\"ok\":true}}\n",
        "{\"event\":\"agent_end\",\"data\":{\"ok\":false}}\n",
    ] {
        let trace = format!("{}{suffix}", successful_trace());
        let artifacts = fixture(&dir, &trace, json!([]));
        let report = evaluate_case(&case(vec![]), ".", artifacts, 0, None).unwrap();
        assert!(!report.ok);
    }
    let artifacts = fixture(
        &dir,
        successful_trace(),
        json!([{"role":"assistant","content":"[DONE]"}]),
    );
    assert!(
        evaluate_case(&case(vec![]), ".", artifacts, 0, None)
            .unwrap()
            .ok
    );
}

#[test]
fn create_file_requires_exact_artifact_bytes_even_when_claim_and_tool_call_match() {
    let dir = tempfile::tempdir().unwrap();
    let requested = case(vec![RuntimeEvalCheck::ToolRootFileEquals {
        path: "todo.txt".into(),
        value: "ship it".into(),
    }]);
    for content in [
        None,
        Some("ship it\n"),
        Some("do not ship it"),
        Some("ship it later"),
        Some("ship it"),
    ] {
        let file = dir.path().join("todo.txt");
        if let Some(content) = content {
            std::fs::write(&file, content).unwrap();
        }
        let artifacts = fixture(
            &dir,
            successful_trace(),
            json!([{"role":"assistant","content":"[DONE] Created todo.txt containing ship it"}]),
        );
        let report =
            evaluate_case(&requested, dir.path().to_str().unwrap(), artifacts, 0, None).unwrap();
        assert_eq!(report.ok, content == Some("ship it"), "{content:?}");
    }
}

#[test]
fn auto_test_command_requires_recorded_runtime_configuration() {
    let dir = tempfile::tempdir().unwrap();
    let requested = case(vec![RuntimeEvalCheck::AutoTestPassed {
        command: Some("cargo test".into()),
    }]);
    let messages = json!([
        {"role":"assistant","tool_calls":[{"id":"edit","function":{"name":"patch_file","arguments":"{}"}}]},
        {"role":"tool","tool_call_id":"edit","content":"OK: patched 'src/lib.rs'\n[auto-test] ✓ PASSED (exit 0)"},
        {"role":"assistant","content":"[DONE] cargo test passed"}
    ]);
    for command in [None, Some("cargo check"), Some("cargo test")] {
        let metadata = command
            .map(|command| {
                json!({"event":"verification_config","data":{"test_command":command}}).to_string()
                    + "\n"
            })
            .unwrap_or_default();
        let trace = format!("{metadata}{}", successful_trace());
        let artifacts = fixture(&dir, &trace, messages.clone());
        let report = evaluate_case(&requested, ".", artifacts, 0, None).unwrap();
        assert_eq!(report.ok, command == Some("cargo test"));
    }
}

#[test]
fn pre_promotion_defers_only_reserved_overlay_existence() {
    let checks = vec![
        RuntimeEvalCheck::Completed,
        RuntimeEvalCheck::ToolRootFileExists {
            path: "notes/todo.txt".into(),
        },
        RuntimeEvalCheck::ToolRootFileExists {
            path: ".spiral-coder/governor_contract.overlay.json".into(),
        },
        RuntimeEvalCheck::ToolRootFileContains {
            path: ".spiral-coder/governor_contract.overlay.json".into(),
            value: "required".into(),
        },
        RuntimeEvalCheck::ToolRootFileEquals {
            path: "notes/todo.txt".into(),
            value: "ship it".into(),
        },
        RuntimeEvalCheck::ToolRootFileExists {
            path: "other/.spiral-coder/governor_contract.overlay.json".into(),
        },
    ];
    let original = case(checks);
    let precheck = pre_promotion_case(&original);
    assert_eq!(original.checks.len(), 6);
    assert_eq!(precheck.checks.len(), 5);
    assert_eq!(
        serde_json::to_value(&precheck.checks).unwrap(),
        serde_json::to_value([
            &original.checks[0],
            &original.checks[1],
            &original.checks[3],
            &original.checks[4],
            &original.checks[5]
        ])
        .unwrap()
    );
    let dir = tempfile::tempdir().unwrap();
    let artifacts = fixture(&dir, successful_trace(), json!([]));
    assert!(
        !evaluate_case(&precheck, dir.path().to_str().unwrap(), artifacts, 0, None)
            .unwrap()
            .ok
    );
}

#[test]
fn old_done_text_cannot_satisfy_a_later_user_request() {
    let messages = vec![
        json!({"role":"assistant","content":"[DONE] Created old.txt"}),
        json!({"role":"user","content":"Create new.txt"}),
        json!({"role":"assistant","content":"Investigating"}),
    ];
    assert_eq!(
        select_terminal_assistant_message(&messages).as_deref(),
        Some("Investigating")
    );
}

#[test]
fn old_report_metrics_keep_unknown_evaluator_and_historical_result() {
    let historical = RuntimeEvalMetrics {
        completed: true,
        successful_exec_commands: vec!["cargo test".into()],
        ..RuntimeEvalMetrics::default()
    };
    let mut value = serde_json::to_value(&historical).unwrap();
    for new_field in [
        "evaluator_revision",
        "task_completed",
        "verified_exec_commands",
        "fresh_auto_test_pass_count",
        "auto_test_command",
    ] {
        value.as_object_mut().unwrap().remove(new_field);
    }
    let loaded: RuntimeEvalMetrics = serde_json::from_value(value).unwrap();
    assert_eq!(loaded.evaluator_revision, None);
    assert_eq!(loaded.task_completed, None);
    assert!(loaded.verified_exec_commands.is_empty());
    assert!(
        loaded.completed,
        "loading a historical report does not rewrite its verdict"
    );

    let dir = tempfile::tempdir().unwrap();
    let artifacts = fixture(&dir, successful_trace(), json!([]));
    let current = evaluate_case(&case(vec![]), ".", artifacts, 0, None).unwrap();
    assert_eq!(
        current.metrics.evaluator_revision.as_deref(),
        Some("outcome-proof-v2")
    );
}
