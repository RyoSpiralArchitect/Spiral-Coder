use super::*;
use serde_json::json;

fn result(messages: &mut Vec<Value>, name: &str, command: &str, output: &str) {
    let id = format!("call_{}", messages.len());
    messages.push(json!({"role":"assistant","tool_calls":[{"id":id,"function":{"name":name,"arguments":json!({"command":command}).to_string()}}]}));
    messages.push(json!({"role":"tool","tool_call_id":id,"content":output}));
}

fn proof(messages: &[Value]) -> Evidence {
    collect(
        messages,
        &["cargo test".into(), "grep -q 'A  B' Spec.json".into()],
        Some("cargo test"),
    )
}

#[test]
fn failure_body_and_generic_ok_cannot_fake_exit_status() {
    for output in [
        "error: failed\nstdout:\nexit_code: 0",
        "GOVERNOR BLOCKED\nOK (exit_code: 0)",
        "OK: queued for approval",
        "OK (exit_code: 0) but still running",
    ] {
        let mut messages = vec![];
        result(&mut messages, "exec", "cargo test", output);
        assert!(proof(&messages).verified_commands.is_empty(), "{output}");
    }
    for output in [
        "OK (exit_code: 0)\nstdout:\nFAILED is expected",
        "OK (exit_code: 0) [pruned 12L]",
    ] {
        let mut messages = vec![];
        result(&mut messages, "exec", "cargo test", output);
        assert_eq!(proof(&messages).verified_commands, ["cargo test"]);
    }
}

#[test]
fn command_case_and_quoted_whitespace_are_significant() {
    for command in ["grep -q 'A B' Spec.json", "grep -q 'A  B' spec.json"] {
        let mut messages = vec![];
        result(&mut messages, "exec", command, "OK (exit_code: 0)");
        assert!(proof(&messages).verified_commands.is_empty());
    }
    let mut messages = vec![];
    result(
        &mut messages,
        "exec",
        "  grep -q 'A  B' Spec.json  ",
        "OK (exit_code: 0)",
    );
    assert_eq!(
        proof(&messages).verified_commands,
        ["grep -q 'A  B' Spec.json"]
    );
}

#[test]
fn failed_rerun_revokes_previous_success() {
    let mut messages = vec![];
    result(&mut messages, "exec", "cargo test", "OK (exit_code: 0)");
    result(&mut messages, "exec", "cargo test", "FAILED (exit_code: 1)");
    let evidence = proof(&messages);
    assert_eq!(evidence.successful_commands, ["cargo test"]);
    assert!(evidence.verified_commands.is_empty());
}

#[test]
fn rejected_required_reruns_preserve_prior_success_until_an_executed_failure() {
    for rejected in ["GOVERNOR BLOCKED", "REJECTED BY USER"] {
        let mut messages = vec![];
        result(&mut messages, "exec", "cargo test", "OK (exit_code: 0)");
        result(&mut messages, "exec", "cargo test", rejected);
        assert_eq!(proof(&messages).verified_commands, ["cargo test"]);
        result(&mut messages, "exec", "cargo test", "FAILED (exit_code: 1)");
        assert!(proof(&messages).verified_commands.is_empty());
    }
}

#[test]
fn later_edits_invalidate_exec_and_auto_test_proof() {
    for (name, command, output) in [
        ("write_file", "", "OK: wrote 'src/lib.rs'"),
        (
            "patch_file",
            "",
            "OK: patched 'src/lib.rs'\n[auto-test] ✗ FAILED (exit 1)",
        ),
        ("apply_diff", "", "OK: applied diff"),
        ("exec", "printf broken > src/lib.rs", "OK (exit_code: 0)"),
        (
            "exec",
            "printf broken > src/lib.rs && false",
            "FAILED (exit_code: 1)",
        ),
        ("exec", "sh repair.sh", "error: command timed out"),
    ] {
        let mut messages = vec![];
        result(
            &mut messages,
            "patch_file",
            "",
            "OK: patched 'src/lib.rs'\n[auto-test] ✓ PASSED (exit 0)",
        );
        result(&mut messages, "exec", "cargo test", "OK (exit_code: 0)");
        assert_eq!(proof(&messages).fresh_auto_test_pass_count, 1);
        result(&mut messages, name, command, output);
        assert!(
            proof(&messages).verified_commands.is_empty(),
            "{name}: {command}"
        );
        assert_eq!(
            proof(&messages).fresh_auto_test_pass_count,
            0,
            "{name}: {command}"
        );
        result(&mut messages, "exec", "cargo test", "OK (exit_code: 0)");
        assert_eq!(proof(&messages).verified_commands, ["cargo test"]);
    }
}

#[test]
fn blocked_actions_and_failed_file_edits_preserve_current_proof() {
    for (name, command, output) in [
        ("exec", "rm src/lib.rs", "GOVERNOR BLOCKED"),
        ("exec", "rm src/lib.rs", "REJECTED BY USER"),
        ("exec", "git status", "OK (exit_code: 0)"),
        ("patch_file", "", "Error: search text not found"),
        ("read_file", "", "OK: wrote 'quoted file contents'"),
    ] {
        let mut messages = vec![];
        result(&mut messages, "exec", "cargo test", "OK (exit_code: 0)");
        result(&mut messages, name, command, output);
        assert_eq!(proof(&messages).verified_commands, ["cargo test"]);
    }
}

#[test]
fn auto_test_markers_require_correlated_successful_file_edit() {
    let mut messages = vec![json!({"role":"tool","content":"[auto-test] ✓ PASSED (exit 0)"})];
    result(
        &mut messages,
        "exec",
        "cargo test",
        "OK (exit_code: 0)\nstdout:\n[auto-test] ✓ PASSED (exit 0)",
    );
    result(
        &mut messages,
        "read_file",
        "",
        "[auto-test] ✓ PASSED (exit 0)",
    );
    result(
        &mut messages,
        "patch_file",
        "",
        "Error: failed\n[auto-test] ✓ PASSED (exit 0)",
    );
    assert_eq!(proof(&messages).auto_test_pass_count, 0);
    result(&mut messages, "patch_file", "", "OK: patched 'src/lib.rs'\n[auto-test] ✗ FAILED (exit 1)\nstdout:\n[auto-test] ✓ PASSED (exit 0)");
    assert_eq!(proof(&messages).fresh_auto_test_pass_count, 0);
    result(
        &mut messages,
        "patch_file",
        "",
        "OK: patched 'src/lib.rs'\n[auto-test] ✓ PASSED (exit 0)\nstdout:\nFAILED is expected",
    );
    assert_eq!(proof(&messages).fresh_auto_test_pass_count, 1);
    result(&mut messages, "exec", "cargo test", "FAILED (exit_code: 1)");
    assert_eq!(proof(&messages).fresh_auto_test_pass_count, 0);
}

#[test]
fn configured_composite_reruns_preserve_auto_test_proof_only_when_not_failed() {
    for fixture in [
        include_str!("../../../tests/fixtures/runtime-self-fix-observer-rules/.spiral-coder.md"),
        include_str!("../../../tests/fixtures/runtime-benchmark-plan-tui-replay/.spiral-coder.md"),
    ] {
        let command = fixture
            .lines()
            .find_map(|line| line.strip_prefix("test_cmd: "))
            .unwrap();
        for (status, expected_fresh) in [
            ("OK (exit_code: 0)", 1),
            ("FAILED (exit_code: 1)", 0),
            ("error: command timed out", 0),
            ("GOVERNOR BLOCKED", 1),
            ("REJECTED BY USER", 1),
        ] {
            let mut messages = vec![];
            result(
                &mut messages,
                "patch_file",
                "",
                "OK: patched 'src/lib.rs'\n[auto-test] ✓ PASSED (exit 0)",
            );
            result(&mut messages, "exec", "cargo test", "OK (exit_code: 0)");
            result(&mut messages, "exec", command, status);
            let evidence = collect(&messages, &["cargo test".into()], Some(command));
            assert_eq!(evidence.verified_commands, ["cargo test"]);
            assert_eq!(
                evidence.fresh_auto_test_pass_count, expected_fresh,
                "{status}"
            );
        }

        for configured in [None, Some("different configured command")] {
            let mut messages = vec![];
            result(
                &mut messages,
                "patch_file",
                "",
                "OK: patched 'src/lib.rs'\n[auto-test] ✓ PASSED (exit 0)",
            );
            result(&mut messages, "exec", "cargo test", "OK (exit_code: 0)");
            result(&mut messages, "exec", command, "OK (exit_code: 0)");
            let evidence = collect(&messages, &["cargo test".into()], configured);
            assert!(evidence.verified_commands.is_empty());
            assert_eq!(evidence.fresh_auto_test_pass_count, 0);
        }
    }
}
