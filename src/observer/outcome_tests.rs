use super::{detector, engine, repo_rules};
use serde_json::{json, Value};

fn exchange(id: &str, name: &str, args: Value, result: &str) -> Vec<Value> {
    vec![
        json!({"role":"assistant","tool_calls":[{"id":id,"type":"function","function":{"name":name,"arguments":args.to_string()}}]}),
        json!({"role":"tool","tool_call_id":id,"content":result}),
    ]
}

fn edit(id: &str, path: &str) -> Vec<Value> {
    exchange(
        id,
        "patch_file",
        json!({"path":path,"search":"old","replace":"new"}),
        &format!("OK: patched '{path}'"),
    )
}

#[test]
fn observer_rejected_edits_do_not_create_anchors_or_discharge_docs() {
    for result in [
        "GOVERNOR BLOCKED\nOK: patched 'docs/state-schema.md'",
        "REJECTED BY USER",
        "",
        "unknown result",
    ] {
        let blocked = exchange(
            "blocked",
            "patch_file",
            json!({"path":"docs/state-schema.md"}),
            result,
        );
        assert!(
            !detector::detect_events(&blocked)
                .iter()
                .any(|e| matches!(e, detector::Event::FileWritten { .. })),
            "{result}"
        );
        let mut messages = edit("source", "src/tui/prefs.rs");
        messages.extend(blocked);
        let c = engine::run_observer(&messages, None);
        assert!(
            c.risks
                .iter()
                .any(|r| repo_rules::is_missing_state_schema_doc(&r.description)),
            "{result}"
        );
    }
}

#[test]
fn observer_failed_blocked_and_spoofed_checks_cannot_supply_replay_proof() {
    for (command, result) in [
        (
            "spiral-coder tui-replay --spec .spiral-coder/tui_replay.json",
            "FAILED (exit_code: 1)\nstdout:\nOK (exit_code: 0)",
        ),
        (
            "spiral-coder tui-replay --spec .spiral-coder/tui_replay.json",
            "GOVERNOR BLOCKED",
        ),
        ("echo 'spiral-coder tui-replay'", "OK (exit_code: 0)"),
        ("cat .spiral-coder/tui_replay.json", "OK (exit_code: 0)"),
        (
            "spiral-coder tui-replay --spec other.json",
            "OK (exit_code: 0)",
        ),
        ("spiral-coder tui-replay --help", "OK (exit_code: 0)"),
        ("spiral-coder tui-replay || true", "OK (exit_code: 0)"),
    ] {
        let mut messages = edit("source", "src/tui/events.rs");
        messages.extend(exchange(
            "check",
            "exec",
            json!({"command":command}),
            result,
        ));
        let c = engine::run_observer(&messages, None);
        assert!(
            c.risks
                .iter()
                .any(|r| repo_rules::is_missing_tui_replay_proof(&r.description)),
            "{command}: {result}"
        );
    }
}

#[test]
fn observer_only_current_successful_checks_clear_missing_verification() {
    for command in ["git status", "echo 'cargo test'", "cargo test || true"] {
        let mut messages = edit("source", "src/lib.rs");
        messages.extend(exchange(
            "check",
            "exec",
            json!({"command":command}),
            "OK (exit_code: 0)",
        ));
        let c = engine::run_observer(&messages, None);
        assert!(
            c.risks
                .iter()
                .any(|r| r.description == "No verification command after edits"),
            "{command}"
        );
    }
    let mut messages = edit("source", "src/tui/events.rs");
    messages.extend(exchange("check", "exec", json!({"command":"spiral-coder --provider openai tui-replay --spec .spiral-coder/tui_replay.json"}), "OK (exit_code: 0)"));
    assert!(!engine::run_observer(&messages, None)
        .risks
        .iter()
        .any(|r| repo_rules::is_missing_tui_replay_proof(&r.description)));
    messages.extend(edit("source2", "src/tui/events.rs"));
    assert!(engine::run_observer(&messages, None)
        .risks
        .iter()
        .any(|r| repo_rules::is_missing_tui_replay_proof(&r.description)));
}

#[test]
fn observer_result_correlation_cannot_look_forward_or_reuse_a_result() {
    let pair = edit("reused", "src/lib.rs");
    let messages = vec![pair[1].clone(), pair[0].clone()];
    assert!(detector::detect_events(&messages).is_empty());
    let messages = vec![pair[0].clone(), pair[1].clone(), pair[1].clone()];
    assert_eq!(
        detector::detect_events(&messages)
            .iter()
            .filter(|e| matches!(e, detector::Event::FileWritten { .. }))
            .count(),
        1
    );
}

#[test]
fn observer_automatic_test_failure_preserves_mutation_and_failed_diagnosis() {
    let messages = exchange("edit", "patch_file", json!({"path":"src/lib.rs"}), "OK: patched 'src/lib.rs'\n[auto-test] ✗ FAILED (exit 1)\nError: missing docs follow-up\n[auto-test] ✓ PASSED (exit 0)");
    let c = engine::run_observer(&messages, None);
    let d = c.coder_diagnostic.expect("diagnostic");
    assert_eq!(
        d.mutation_anchor.as_ref().map(|a| a.path.as_str()),
        Some("src/lib.rs")
    );
    assert_eq!(d.failure_mode, "failing_verification");
    assert!(d
        .evidence
        .iter()
        .any(|s| s.contains("missing docs follow-up")));
}

#[test]
fn observer_visible_edit_markers_without_receipts_are_unconfirmed() {
    for marker in [
        "✎ patch_file: src/tui/prefs.rs",
        "✎ write_file: src/tui/prefs.rs",
        "⟁ apply_diff: src/tui/prefs.rs (13000 chars, 12 hunks)",
    ] {
        let c =
            engine::run_observer_from_transcript(&format!("{marker}\nERROR patching file"), None);
        let d = c.coder_diagnostic.expect("unconfirmed diagnosis");
        assert!(d.mutation_anchor.is_none());
        assert_eq!(d.failure_mode, "unconfirmed_edit");
        assert!(d.required_followups.is_empty());
        assert_eq!(
            d.next_coder_action.unwrap().args,
            json!({"path":"src/tui/prefs.rs"})
        );
    }
}

#[test]
fn observer_transcript_verification_requires_result_outside_stdout() {
    let prefix = "✎ patch_file: src/tui/events.rs\nOK: patched 'src/tui/events.rs'\n```bash\n$ spiral-coder tui-replay --spec .spiral-coder/tui_replay.json\n";
    for (tail, missing) in [
        ("exit: 0\n```", true),
        ("exit: 0\n```\nexit: 1", true),
        ("```\nexit: 0", false),
    ] {
        let c = engine::run_observer_from_transcript(&format!("{prefix}{tail}"), None);
        assert_eq!(
            c.risks
                .iter()
                .any(|r| repo_rules::is_missing_tui_replay_proof(&r.description)),
            missing,
            "{tail}"
        );
    }
}

#[test]
fn observer_success_header_outweighs_failure_text_in_stdout() {
    let mut messages = edit("source", "src/lib.rs");
    messages.extend(exchange("check", "exec", json!({"command":"cargo test -q"}), "OK (exit_code: 0)\nstdout:\nFAILED (exit_code: 1) is an expected test fixture\nError: expected error string"));
    let c = engine::run_observer(&messages, None);
    assert!(c.risks.is_empty(), "{:?}", c.risks);
}

#[test]
fn observer_unicode_failure_evidence_and_long_commands_do_not_panic() {
    let command = format!("cargo test '{}'", "猫と渦の検証".repeat(30));
    let messages = exchange(
        "check",
        "exec",
        json!({"command":command}),
        "FAILED (exit_code: 1)\n猫と渦の検証に失敗\nstderr: 検証対象がありません",
    );
    let c = engine::run_observer(&messages, None);
    assert!(c
        .coder_diagnostic
        .unwrap()
        .evidence
        .iter()
        .any(|e| e.contains("検証対象")));
    let transcript = format!(
        "```bash\n$ {command}\n猫と渦の検証に失敗\nstderr: 検証対象がありません\n```\nexit: 1"
    );
    assert!(!engine::run_observer_from_transcript(&transcript, None)
        .risks
        .is_empty());
}
