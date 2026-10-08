use super::super::{classify_exec_kind, RecoveryGovernor, VerificationLevel};
use super::*;
use serde_json::json;

fn record(messages: &mut Vec<Value>, call: &ToolCallData, content: &str) {
    messages.push(json!({"role":"assistant","tool_calls":[{
        "id":call.id,"function":{"name":call.name,"arguments":call.arguments}
    }]}));
    messages.push(json!({"role":"tool","tool_call_id":call.id,"content":content}));
}

#[test]
fn failed_required_replay_preserves_fix_then_requires_fresh_verification() {
    let path = ".spiral-coder/tui_replay.json";
    let command = "spiral-coder --provider openai tui-replay --spec .spiral-coder/tui_replay.json --filter review-panel-replay-sensitive";
    let prompt = format!("<observer_benchmark_plan>\nlane: tui_replay\ncase_id_hint: review-panel-replay-sensitive\ntarget_files:\n- {path}\nrequired_checks:\n- {command}\nsuccess_criteria:\n- {path} includes src/tui/review_panel.rs\n</observer_benchmark_plan>");
    let harness = TaskHarness::infer(&prompt, false);
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().to_str().unwrap();
    std::fs::create_dir(temp.path().join(".spiral-coder")).unwrap();
    let invalid = r#"{"version":1,"cases":[{"id":"review-panel-replay-sensitive","note":"src/tui/review_panel.rs",}]}"#;
    std::fs::write(temp.path().join(path), invalid).unwrap();
    assert!(serde_json::from_str::<Value>(invalid).is_err());
    let read = ToolCallData {
        thought_signature: None,
        id: "diagnose".into(),
        name: "read_file".into(),
        arguments: json!({"path":path}).to_string(),
    };
    let verify = ToolCallData {
        thought_signature: None,
        id: "verify".into(),
        name: "exec".into(),
        arguments: json!({"command":command}).to_string(),
    };
    let repair = ToolCallData {
        thought_signature: None,
        id: "repair".into(),
        name: "patch_file".into(),
        arguments: json!({"path":path,"search":",}","replace":"}"}).to_string(),
    };
    let mut messages = vec![];
    record(
        &mut messages,
        &read,
        &format!("[{path}] (1 lines, 108 bytes)\n{invalid}"),
    );
    record(
        &mut messages,
        &verify,
        "FAILED (exit_code: 1)\nstderr:\ntrailing comma",
    );
    let mut recovery = RecoveryGovernor::default();
    recovery.on_exec_result(
        classify_exec_kind(command, Some(command)),
        classify_verify_level(command, Some(command)),
        false,
    );
    assert_eq!(recovery.stage, Some(RecoveryStage::Diagnose));
    assert!(coerce_benchmark_plan_tool_call(
        harness,
        &messages,
        &read,
        &prompt,
        Some(root),
        recovery.stage,
        Some(command)
    )
    .is_none());
    recovery.on_diagnostic_result(true);
    assert_eq!(recovery.stage, Some(RecoveryStage::Fix));
    assert!(
        coerce_benchmark_plan_tool_call(
            harness,
            &messages,
            &repair,
            &prompt,
            Some(root),
            recovery.stage,
            Some(command)
        )
        .is_none(),
        "the matching path literal must not turn a JSON repair into the failing check"
    );
    assert!(synthesize_benchmark_plan_no_tool_call(
        harness,
        &messages,
        &prompt,
        Some(root),
        recovery.stage,
        Some(command)
    )
    .is_none());
    assert!(recovery
        .maybe_block_tool(&repair, Some(command), harness, false)
        .is_none());
    let (output, failed) = crate::file_tools::tool_patch_file(path, ",}", "}", Some(root));
    assert!(!failed, "{output}");
    assert!(serde_json::from_str::<Value>(
        &std::fs::read_to_string(temp.path().join(path)).unwrap()
    )
    .is_ok());
    record(&mut messages, &repair, &output);
    recovery.on_fix_result(true, None);
    assert_eq!(recovery.stage, Some(RecoveryStage::Verify));
    assert!(benchmark_plan_missing_required_exec_proof(
        &prompt,
        &messages,
        Some(command)
    ));
    assert!(coerce_benchmark_plan_tool_call(
        harness,
        &messages,
        &verify,
        &prompt,
        Some(root),
        recovery.stage,
        Some(command)
    )
    .is_none());
    assert!(synthesize_benchmark_plan_no_tool_call(
        harness,
        &messages,
        &prompt,
        Some(root),
        recovery.stage,
        Some(command)
    )
    .is_none());
    record(&mut messages, &verify, "OK (exit_code: 0)");
    recovery.on_exec_result(
        classify_exec_kind(command, Some(command)),
        classify_verify_level(command, Some(command)),
        true,
    );
    assert_eq!(recovery.stage, None);
    assert!(!benchmark_plan_missing_required_exec_proof(
        &prompt,
        &messages,
        Some(command)
    ));
}

#[test]
fn failed_replay_edit_preserves_diagnostics_until_recovery_can_advance() {
    let prompt = "<observer_benchmark_plan>\nlane: tui_replay\ncase_id_hint: review-panel-replay-sensitive\ntarget_files:\n- .spiral-coder/tui_replay.json\nsuccess_criteria:\n- .spiral-coder/tui_replay.json includes src/tui/review_panel.rs\n</observer_benchmark_plan>";
    let harness = TaskHarness::infer(prompt, false);
    let messages = vec![
        json!({"role":"assistant","tool_calls":[{"id":"read","function":{"name":"read_file","arguments":"{\"path\":\".spiral-coder/tui_replay.json\"}"}}]}),
        json!({"role":"tool","tool_call_id":"read","content":"[.spiral-coder/tui_replay.json] (3 lines, 27 bytes)\n{\"version\":1,\"cases\":[]}"}),
        json!({"role":"assistant","tool_calls":[{"id":"edit","function":{"name":"apply_diff","arguments":"{\"path\":\".spiral-coder/tui_replay.json\",\"diff\":\"bad hunk\"}"}}]}),
        json!({"role":"tool","tool_call_id":"edit","content":"ERROR: no hunks applied. Tip: call read_file to inspect exact content."}),
    ];
    for requested in [
        ToolCallData {
            thought_signature: None,
            id: "read-again".into(),
            name: "read_file".into(),
            arguments: json!({"path":".spiral-coder/tui_replay.json"}).to_string(),
        },
        ToolCallData {
            thought_signature: None,
            id: "diagnose".into(),
            name: "exec".into(),
            arguments: json!({"command":"pwd"}).to_string(),
        },
    ] {
        let mut recovery = RecoveryGovernor::default();
        recovery.on_fix_result(false, None);
        assert_eq!(recovery.stage, Some(RecoveryStage::Diagnose));
        assert!(
            coerce_benchmark_plan_tool_call(
                harness, &messages, &requested, prompt, None, None, None
            )
            .is_some(),
            "without recovery, this request would become an edit"
        );
        assert!(coerce_benchmark_plan_tool_call(
            harness,
            &messages,
            &requested,
            prompt,
            None,
            recovery.stage,
            None
        )
        .is_none());
        assert!(synthesize_benchmark_plan_no_tool_call(
            harness,
            &messages,
            prompt,
            None,
            recovery.stage,
            None
        )
        .is_none());
        assert!(recovery
            .maybe_block_tool(&requested, None, harness, false)
            .is_none());
        recovery.on_diagnostic_result(true);
        assert_eq!(recovery.stage, Some(RecoveryStage::Fix));
        assert!(coerce_benchmark_plan_tool_call(
            harness,
            &messages,
            &requested,
            prompt,
            None,
            recovery.stage,
            None,
        )
        .is_none());
        recovery.on_fix_result(true, Some(VerificationLevel::Behavioral));
        assert_eq!(recovery.stage, None);
        let (next, _, _) = coerce_benchmark_plan_tool_call(
            harness,
            &messages,
            &requested,
            prompt,
            None,
            recovery.stage,
            None,
        )
        .expect("resume benchmark assistance after recovery completes");
        assert_eq!(next.name, "patch_file");
        assert!(recovery
            .maybe_block_tool(&next, None, harness, false)
            .is_none());
    }
}
