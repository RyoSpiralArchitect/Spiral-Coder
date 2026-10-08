use super::*;

#[tokio::test]
async fn failed_automatic_test_returns_successful_edit_to_diagnosis() {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().to_str().unwrap();
    let command = "exit 1";
    for name in ["write_file", "patch_file", "apply_diff"] {
        std::fs::write(root.path().join("fixture.txt"), "before\n").unwrap();
        let (mut output, error) = match name {
            "write_file" => crate::file_tools::tool_write_file("fixture.txt", "after\n", Some(dir)),
            "patch_file" => {
                crate::file_tools::tool_patch_file("fixture.txt", "before", "after", Some(dir))
            }
            _ => crate::file_tools::tool_apply_diff(
                "fixture.txt",
                "@@ -1 +1 @@\n-before\n+after\n",
                Some(dir),
            ),
        };
        assert!(!error, "{output}");
        output.push_str(&run_test_cmd(command, dir).await);
        output.push_str("\nError: could not infer a stuck target from coder_messages\n[auto-test] ✓ PASSED (exit 0)");
        let mut recovery = RecoveryGovernor {
            stage: Some(RecoveryStage::Fix),
            required_verification: VerificationLevel::Behavioral,
            ..Default::default()
        };
        let verified = crate::execution_evidence::auto_test_succeeded(name, &output);
        let level = verified
            .then(|| configured_test_cmd_verification_level(Some(command)))
            .flatten();
        recovery.on_successful_edit(
            crate::execution_evidence::auto_test_outcome(name, &output),
            level,
        );
        assert_eq!(
            recovery.stage,
            Some(RecoveryStage::Diagnose),
            "{name}: failed automatic verification must allow diagnostics without a duplicate exec"
        );
        let read = ToolCallData {
            id: "read".into(),
            name: "read_file".into(),
            arguments: json!({"path":"fixture.txt"}).to_string(),
        };
        let harness = TaskHarness::infer("Fix the existing replay fixture", false);
        assert!(recovery
            .maybe_block_tool(&read, Some(command), harness, false)
            .is_none());
        recovery.on_diagnostic_result(true);
        assert_eq!(recovery.stage, Some(RecoveryStage::Fix));
        assert_eq!(
            std::fs::read_to_string(root.path().join("fixture.txt")).unwrap(),
            "after\n"
        );
        let (mut mutation, mut build, mut behavioral) = (Some(1), Some(1), Some(1));
        exec_proof::record_file_result(
            exec_proof::FileProofResult {
                name,
                content: &output,
                test_cmd: Some(command),
            },
            2,
            &mut mutation,
            &mut build,
            &mut behavioral,
        );
        assert_eq!((mutation, build, behavioral), (Some(2), None, None));
        let messages = vec![
            json!({"role":"assistant","tool_calls":[{"id":"edit","function":{"name":name,"arguments":"{}"}}]}),
            json!({"role":"tool","tool_call_id":"edit","content":output}),
        ];
        let resumed = RecoveryGovernor::restore_from_session(
            &FailureMemory::default(),
            &messages,
            VerificationLevel::Behavioral,
        );
        assert_eq!(resumed.stage, Some(RecoveryStage::Diagnose));
        let (_, mutation, build, behavioral, _) =
            restore_done_gate_from_messages(&messages, Some(command));
        assert_eq!((mutation, build, behavioral), (Some(1), None, None));
    }
}

#[test]
fn automatic_test_states_keep_existing_verification_levels_and_missing_status_rules() {
    use crate::execution_evidence::{auto_test_outcome, AutoTestOutcome};
    for (status, outcome, level, expected) in [
        (
            "",
            AutoTestOutcome::NotRun,
            None,
            Some(RecoveryStage::Verify),
        ),
        (
            "[auto-test] unknown\n[auto-test] ✓ PASSED (exit 0)",
            AutoTestOutcome::NotRun,
            None,
            Some(RecoveryStage::Verify),
        ),
        (
            "[auto-test] ✓ PASSED (exit 0)\n[auto-test] ✗ FAILED (exit 1)",
            AutoTestOutcome::Passed,
            Some(VerificationLevel::Build),
            Some(RecoveryStage::Verify),
        ),
        (
            "[auto-test] ✓ PASSED (exit 0)",
            AutoTestOutcome::Passed,
            Some(VerificationLevel::Behavioral),
            None,
        ),
        (
            "[auto-test] ✗ FAILED (exit -1)\n[auto-test] ✓ PASSED (exit 0)",
            AutoTestOutcome::Failed,
            None,
            Some(RecoveryStage::Diagnose),
        ),
    ] {
        let output = format!("OK: patched 'fixture.txt'\n{status}");
        assert_eq!(auto_test_outcome("patch_file", &output), outcome);
        let mut recovery = RecoveryGovernor {
            stage: Some(RecoveryStage::Fix),
            required_verification: VerificationLevel::Behavioral,
            ..Default::default()
        };
        recovery.on_successful_edit(outcome, level);
        assert_eq!(recovery.stage, expected, "{status}");
    }
}
