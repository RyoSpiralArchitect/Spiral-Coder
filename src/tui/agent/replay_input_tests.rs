//! Early replay failures must retain the artifact and cause through recovery.
use super::*;

const TARGET: &str = ".spiral-coder/tui_replay.json";
const CHECK: &str = "spiral-coder tui-replay --spec .spiral-coder/tui_replay.json";

fn missing_target_source() -> String {
    json!({"cases":[{
        "id":"new-case", "prompt":"Inspect the review panel",
        "coder_messages":[{"role":"assistant","content":"I have identified the review panel."}],
        "observer_response":{"summary":"Inspect the source"},
        "checks":[{"kind":"suggestion_parsed"}]
    }]})
    .to_string()
}

#[test]
fn early_replay_input_failure_survives_long_roots_compaction_and_resume() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp
        .path()
        .join("deep".repeat(40))
        .join("nested".repeat(20));
    std::fs::create_dir_all(&root).unwrap();
    let source = missing_target_source();
    let (written, failed) = crate::file_tools::tool_write_file(TARGET, &source, root.to_str());
    assert!(!failed);
    let error =
        crate::tui_replay::replay_spec_for_test(&root.join(TARGET), &root, &root.join("reports"))
            .unwrap_err();
    let output = format!(
        "{written}{}",
        failure_diagnostics::automatic_test_output(
            "",
            &format!("Error: {error}"),
            1,
            root.to_str().unwrap()
        )
    );
    let compact = compact_success_tool_result_for_history("write_file", &output);
    assert!(compact.contains(&format!("spec=\"{TARGET}\"")), "{compact}");
    assert!(
        compact.contains("could not infer a stuck target"),
        "{compact}"
    );
    assert!(compact.contains("failure-like assistant"), "{compact}");
    let call = ToolCallData {
        id: "invalid-input".into(),
        name: "write_file".into(),
        arguments: json!({"path":TARGET,"content":source}).to_string(),
        thought_signature: None,
    };
    let context =
        exec_verification::ExecVerificationContext::from_root(Some(CHECK), "Repair the replay");
    let mut governor = RecoveryGovernor {
        required_verification: VerificationLevel::Behavioral,
        ..Default::default()
    };
    governor
        .focus
        .record_result(&call, &compact, root.to_str(), Some(CHECK), &context);
    governor.on_successful_edit(crate::execution_evidence::AutoTestOutcome::Failed, None);
    assert!(governor.focus.requires_read());
    assert!(!governor.focus.repaired());
    let mut messages = vec![
        json!({"role":"assistant","tool_calls":[call.to_json()]}),
        governor.focus.tool_message(&call, compact),
    ];
    let unrelated = ToolCallData {
        id: "unrelated".into(),
        name: "exec".into(),
        arguments: json!({"command":"pwd"}).to_string(),
        thought_signature: None,
    };
    let output = inject_cwd(
        "OK (exit_code: 0)",
        &format!("cwd: {}", root.display()),
        None,
    );
    governor
        .focus
        .record_result(&unrelated, &output, root.to_str(), Some(CHECK), &context);
    governor.on_diagnostic_result(true);
    assert_eq!(governor.stage, Some(RecoveryStage::Diagnose));
    assert!(governor.focus.hint().unwrap().contains(TARGET));
    messages.extend([
        json!({"role":"assistant","tool_calls":[unrelated.to_json()]}),
        governor.focus.tool_message(&unrelated, output),
    ]);
    let mut resumed = RecoveryGovernor::restore_from_session(
        &FailureMemory::default(),
        &messages,
        VerificationLevel::Behavioral,
    );
    resumed.restore_focus(&messages, root.to_str(), Some(CHECK));
    assert_eq!(resumed.focus, governor.focus);
    assert_eq!(resumed.stage, Some(RecoveryStage::Diagnose));
    assert_eq!(std::fs::read_to_string(root.join(TARGET)).unwrap(), source);
}

#[tokio::test]
async fn empty_replay_selection_reports_the_input_and_existing_ids() {
    use clap::Parser;
    let root = tempfile::tempdir().unwrap();
    let source = missing_target_source();
    assert!(!crate::file_tools::tool_write_file(TARGET, &source, root.path().to_str()).1);
    let common = crate::CommonArgs::try_parse_from([
        "spiral-coder",
        "--provider",
        "openai",
        "--model",
        "local-mock",
        "--base-url",
        "http://127.0.0.1:9/v1",
    ])
    .unwrap();
    let args = crate::TuiReplayArgs {
        tool_root: Some(root.path().to_string_lossy().into()),
        spec: TARGET.into(),
        out_dir: Some(root.path().join("reports")),
        filter: Some("not-yet-present".into()),
        max_cases: None,
    };
    let error = crate::tui_replay::run(args, common)
        .await
        .unwrap_err()
        .to_string();
    for expected in [
        "selected 0 cases",
        "spec=\".spiral-coder/tui_replay.json\"",
        "not-yet-present",
        "available_case_ids",
        "new-case",
        "No case or check ran",
    ] {
        assert!(error.contains(expected), "{error}");
    }
    assert!(!root.path().join("reports/report.json").exists());
    assert_eq!(
        std::fs::read_to_string(root.path().join(TARGET)).unwrap(),
        source
    );
}
