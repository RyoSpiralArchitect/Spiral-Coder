use super::{pending_command, required_commands};
use serde_json::{json, Value};

fn fixture() -> (String, String, Vec<String>) {
    let spec: Value =
        serde_json::from_str(include_str!("../../../.spiral-coder/runtime_eval.json")).unwrap();
    let prompt = spec["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["id"] == "approved-benchmark-plan-tui-replay-smoke")
        .unwrap()["prompt"]
        .as_str()
        .unwrap()
        .to_string();
    let config =
        include_str!("../../../tests/fixtures/runtime-benchmark-plan-tui-replay/.spiral-coder.md")
            .lines()
            .find_map(|line| line.strip_prefix("test_cmd: "))
            .unwrap()
            .to_string();
    let required = required_commands(&prompt);
    assert_eq!(required.len(), 2);
    (prompt, config, required)
}

fn result(messages: &mut Vec<Value>, name: &str, command: &str, output: &str) {
    let id = format!("call_{}", messages.len());
    messages.push(json!({"role":"assistant","tool_calls":[{
        "id":id,"function":{"name":name,"arguments":json!({"command":command}).to_string()}
    }]}));
    messages.push(json!({"role":"tool","tool_call_id":id,"content":output}));
}

#[test]
fn configured_tui_composite_and_individual_proofs_can_both_finish() {
    let (prompt, configured, required) = fixture();
    let mut messages = vec![json!({"role":"user","content":prompt})];
    // The successful edit automatically ran the configured composite command.
    result(
        &mut messages,
        "patch_file",
        "",
        "OK: patched '.spiral-coder/tui_replay.json'\n[auto-test] ✓ PASSED (exit 0)",
    );
    assert_eq!(
        pending_command(&messages, &prompt, Some(&configured)).as_deref(),
        Some(required[0].as_str())
    );
    result(&mut messages, "exec", &required[0], "OK (exit_code: 0)");
    assert_eq!(
        pending_command(&messages, &prompt, Some(&configured)).as_deref(),
        Some(required[1].as_str())
    );
    result(&mut messages, "exec", &required[1], "OK (exit_code: 0)");
    assert_eq!(pending_command(&messages, &prompt, Some(&configured)), None);
    // Exact approved custom checks must also count for the generic done gate.
    // No extra composite rerun is needed to undo a spurious Action mutation.
    let (_, mutation, _, verified, _) =
        crate::tui::agent::restore_done_gate_from_messages(&messages, Some(&configured));
    assert_eq!((mutation, verified), (Some(1), Some(3)));
    let memory = crate::tui::agent::WorkingMemory::from_messages(&messages, Some(&configured));
    assert!(memory.successful_verifications.contains(&required[1]));
    // An optional additional composite rerun keeps the independent A/B proof.
    result(&mut messages, "exec", &configured, "OK (exit_code: 0)");
    let (_, mutation, _, verified, _) =
        crate::tui::agent::restore_done_gate_from_messages(&messages, Some(&configured));
    assert!(verified >= mutation);
    assert_eq!(pending_command(&messages, &prompt, Some(&configured)), None);
    // Configuration is explicit authority. A missing/different configuration
    // keeps this unknown custom composite conservative.
    for config in [None, Some("different check")] {
        assert_eq!(
            pending_command(&messages, &prompt, config).as_deref(),
            Some(required[0].as_str())
        );
    }
    result(&mut messages, "exec", &configured, "FAILED (exit_code: 1)");
    let (_, mutation, build, behavioral, _) =
        crate::tui::agent::restore_done_gate_from_messages(&messages, Some(&configured));
    assert_eq!((build, behavioral), (None, None));
    assert!(
        mutation.is_some(),
        "failed rerun must block generic completion"
    );
    // Configured aggregate checks do not replace the individual proof ledger.
    assert_eq!(pending_command(&messages, &prompt, Some(&configured)), None);
    result(
        &mut messages,
        "exec",
        &format!("{configured} && printf changed > receipt"),
        "OK (exit_code: 0)",
    );
    assert_eq!(
        pending_command(&messages, &prompt, Some(&configured)).as_deref(),
        Some(required[0].as_str())
    );
}

#[test]
fn required_rejection_preserves_prior_proof_but_executed_failure_revokes_it() {
    let (prompt, configured, required) = fixture();
    for rejected in ["GOVERNOR BLOCKED", "REJECTED BY USER"] {
        let mut messages = vec![];
        result(&mut messages, "exec", &required[0], "OK (exit_code: 0)");
        result(&mut messages, "exec", &required[0], rejected);
        assert_eq!(
            pending_command(&messages, &prompt, Some(&configured)).as_deref(),
            Some(required[1].as_str())
        );
        result(&mut messages, "exec", &required[1], "OK (exit_code: 0)");
        assert_eq!(pending_command(&messages, &prompt, Some(&configured)), None);
        result(&mut messages, "exec", &required[0], "FAILED (exit_code: 1)");
        assert_eq!(
            pending_command(&messages, &prompt, Some(&configured)).as_deref(),
            Some(required[0].as_str())
        );
    }
}
