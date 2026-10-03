use super::super::RecoveryGovernor;
use super::*;
use serde_json::json;

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
            id: "read-again".into(),
            name: "read_file".into(),
            arguments: json!({"path":".spiral-coder/tui_replay.json"}).to_string(),
        },
        ToolCallData {
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
        let (next, _, _) = coerce_benchmark_plan_tool_call(
            harness,
            &messages,
            &requested,
            prompt,
            None,
            recovery.stage,
            None,
        )
        .expect("resume the approved patch after diagnosis");
        assert_eq!(next.name, "patch_file");
        assert!(recovery
            .maybe_block_tool(&next, None, harness, false)
            .is_none());
    }
}
