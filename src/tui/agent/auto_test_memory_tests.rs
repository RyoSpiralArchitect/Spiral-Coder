use super::super::{
    recovery::RecoveryGovernor, requirement_proof, RecoveryStage, TaskHarness, ToolCallData,
    VerificationLevel,
};
use super::FailureMemory;
use serde_json::{json, Value};

const CHECK: &str = "python3 -S -m unittest discover -s tests -p test_runner.py -v";

fn result(messages: &mut Vec<Value>, name: &str, args: Value, output: &str) {
    let id = format!("call-{}", messages.len());
    messages.push(json!({"role":"assistant","tool_calls":[{"id":id,"function":{"name":name,"arguments":args.to_string()}}]}));
    messages.push(json!({"role":"tool","tool_call_id":id,"content":output}));
}

fn baseline() -> Vec<Value> {
    let mut messages = vec![json!({"role":"user", "content": format!(
        "Repair runner.py\n<task_requirements>\n{}\n</task_requirements>",
        json!({"version":1,"requirements":[{"id":"repair","description":"Repair the runner","artifacts":[{"path":"runner.py","kind":"source"}],"checks":[{"command":CHECK,"baseline_failure":true}]}]})
    )})];
    result(
        &mut messages,
        "exec",
        json!({"command":CHECK}),
        "FAILED (exit_code: 1)\nstderr:\nAssertionError: default broken",
    );
    messages
}

#[test]
fn automatic_success_survives_handoff_without_fabricating_final_exec_proof() {
    for name in ["write_file", "patch_file", "apply_diff"] {
        let mut messages = baseline();
        let output = match name {
            "write_file" => "OK: wrote 'runner.py'",
            "patch_file" => "OK: patched 'runner.py'",
            _ => "OK: applied 1/1 hunk(s) to 'runner.py'",
        };
        let output = format!("{output}\n[auto-test] ✓ PASSED (exit 0)");
        let mut live = FailureMemory::from_recent_messages(&messages, Some(CHECK));
        live.on_file_result(name, &output, Some(CHECK));
        result(&mut messages, name, json!({"path":"runner.py"}), &output);
        messages.push(crate::task_origin::user_message(
            "Continue",
            crate::task_origin::MessageOrigin::Runtime,
        ));
        let resumed = FailureMemory::from_recent_messages(&messages, Some(CHECK));
        for memory in [live, resumed] {
            assert_eq!(memory.consecutive_failures, 0);
            assert!(!memory.repeated_failure_or_stall());
            let recovery = RecoveryGovernor::restore_from_session(
                &memory,
                &messages,
                VerificationLevel::Behavioral,
            );
            assert_eq!(recovery.stage, None);
            let call = ToolCallData {
                thought_signature: None,
                id: "verify".into(),
                name: "exec".into(),
                arguments: json!({"command":CHECK}).to_string(),
            };
            assert!(recovery
                .maybe_block_tool(
                    &call,
                    Some(CHECK),
                    TaskHarness::infer("Repair runner.py", false),
                    false
                )
                .is_none());
        }
        let status = requirement_proof::project(&messages, Some(CHECK));
        assert!(status.pending_baselines.is_empty());
        assert!(
            !status.pending.is_empty(),
            "auto success must not supply the explicit final check"
        );
        result(
            &mut messages,
            "exec",
            json!({"command":CHECK}),
            "OK (exit_code: 0)\nstdout:\nall pass",
        );
        assert!(requirement_proof::project(&messages, Some(CHECK))
            .pending
            .is_empty());
    }
}

#[test]
fn failed_unrelated_refused_or_unpaired_tests_cannot_clear_failure_memory() {
    for (output, configured) in [
        (
            "OK: patched 'runner.py'\n[auto-test] ✗ FAILED (exit 1)",
            Some(CHECK),
        ),
        ("OK: patched 'runner.py'", Some(CHECK)),
        (
            "GOVERNOR BLOCKED\nOK: patched 'runner.py'\n[auto-test] ✓ PASSED (exit 0)",
            Some(CHECK),
        ),
        (
            "OK: patched 'runner.py'\n[auto-test] ✓ PASSED (exit 0)",
            Some("other check"),
        ),
        (
            "OK: patched 'runner.py'\n[auto-test] ✓ PASSED (exit 0)",
            None,
        ),
    ] {
        let mut messages = baseline();
        result(
            &mut messages,
            "patch_file",
            json!({"path":"runner.py"}),
            output,
        );
        let mem = FailureMemory::from_recent_messages(&messages, configured);
        assert_eq!(mem.consecutive_failures, 1, "{output} / {configured:?}");
        assert_eq!(
            RecoveryGovernor::restore_from_session(&mem, &messages, VerificationLevel::Behavioral)
                .stage,
            Some(RecoveryStage::Diagnose)
        );
    }
    let mut messages = baseline();
    messages.push(json!({"role":"tool","tool_call_id":"orphan","content":"OK: patched 'runner.py'\n[auto-test] ✓ PASSED (exit 0)"}));
    assert_eq!(
        FailureMemory::from_recent_messages(&messages, Some(CHECK)).consecutive_failures,
        1
    );
    // A later failure is not covered by an earlier passing auto test.
    result(
        &mut messages,
        "patch_file",
        json!({"path":"runner.py"}),
        "OK: patched 'runner.py'\n[auto-test] ✓ PASSED (exit 0)",
    );
    result(
        &mut messages,
        "exec",
        json!({"command":CHECK}),
        "FAILED (exit_code: 1)",
    );
    assert_eq!(
        FailureMemory::from_recent_messages(&messages, Some(CHECK)).consecutive_failures,
        1
    );
    messages.push(json!({"role":"user","content":"Start a different task"}));
    assert_eq!(
        FailureMemory::from_recent_messages(&messages, Some(CHECK)).consecutive_failures,
        0
    );
}
