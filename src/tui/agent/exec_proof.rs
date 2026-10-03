//! Generic done-gate verification timestamps for live and resumed exec results.

use super::{
    classify_exec_kind, classify_verify_level, configured_test_cmd_verification_level,
    parse_exec_command_from_args, parse_exec_tool_output_sections, suspicious_success_reason,
    ExecKind, VerificationLevel,
};

/// Live execution supplies its effective return status; restoration derives it
/// from the runtime-owned transcript header, never a success string in stdout.
pub(super) struct ExecProofResult<'a> {
    pub content: &'a str,
    pub succeeded: bool,
}

impl<'a> ExecProofResult<'a> {
    fn from_history(content: &'a str) -> Self {
        let (_, stdout, stderr) = parse_exec_tool_output_sections(content);
        Self {
            content,
            succeeded: crate::execution_evidence::exec_succeeded(content)
                && suspicious_success_reason(&stdout, &stderr).is_none(),
        }
    }
}

/// Record an attempted execution before granting any successful verification.
/// Failed actions may already have written files; failed verification revokes
/// prior success. Only explicit pre-execution rejection preserves the evidence.
pub(super) fn record_result(
    command: &str,
    test_cmd: Option<&str>,
    result: ExecProofResult<'_>,
    step: usize,
    last_mutation: &mut Option<usize>,
    last_build: &mut Option<usize>,
    last_behavioral: &mut Option<usize>,
) {
    if !crate::execution_evidence::exec_may_have_run(result.content) {
        return;
    }
    match classify_exec_kind(command, test_cmd) {
        ExecKind::Action => *last_mutation = Some(step),
        ExecKind::Verify => {
            if result.succeeded {
                match classify_verify_level(command, test_cmd) {
                    Some(VerificationLevel::Build) => *last_build = Some(step),
                    Some(VerificationLevel::Behavioral) => *last_behavioral = Some(step),
                    None => {}
                }
            } else {
                // A previous behavioral success cannot override a newer failed
                // build either. Require fresh evidence at the applicable level.
                *last_build = None;
                *last_behavioral = None;
            }
        }
        ExecKind::Diagnostic => {}
    }
}

pub(super) fn restore_done_gate_from_messages(
    messages: &[serde_json::Value],
    test_cmd: Option<&str>,
) -> (
    usize,
    Option<usize>,
    Option<usize>,
    Option<usize>,
    Option<usize>,
) {
    // step_seq counts tool results (role=tool) so we can compare "mutation happened after verify"
    // even across resumed sessions.
    let mut step_seq: usize = 0;
    let mut last_mutation_step: Option<usize> = None;
    let mut last_build_verify_ok_step: Option<usize> = None;
    let mut last_behavioral_verify_ok_step: Option<usize> = None;
    let mut last_exec_step: Option<usize> = None;

    // Map tool_call_id -> (tool_name, exec_command?)
    let mut by_id: std::collections::HashMap<String, (String, Option<String>)> =
        std::collections::HashMap::new();

    for msg in messages {
        let role = msg.get("role").and_then(|v| v.as_str()).unwrap_or("");
        if role == "assistant" {
            let Some(tcs) = msg.get("tool_calls").and_then(|v| v.as_array()) else {
                continue;
            };
            for tc in tcs {
                let id = tc.get("id").and_then(|v| v.as_str()).unwrap_or("").trim();
                if id.is_empty() {
                    continue;
                }
                let name = tc
                    .get("function")
                    .and_then(|f| f.get("name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string();
                let args = tc
                    .get("function")
                    .and_then(|f| f.get("arguments"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string();
                let cmd = if name == "exec" {
                    parse_exec_command_from_args(&args)
                } else {
                    None
                };
                by_id.insert(id.to_string(), (name, cmd));
            }
            continue;
        }

        if role != "tool" {
            continue;
        }

        step_seq = step_seq.saturating_add(1);

        let id = msg
            .get("tool_call_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        if id.is_empty() {
            continue;
        }
        let Some((name, cmd)) = by_id.remove(id) else {
            continue;
        };
        let content = msg.get("content").and_then(|v| v.as_str()).unwrap_or("");

        if name == "exec" {
            last_exec_step = Some(step_seq);
            record_result(
                &cmd.unwrap_or_default(),
                test_cmd,
                ExecProofResult::from_history(content),
                step_seq,
                &mut last_mutation_step,
                &mut last_build_verify_ok_step,
                &mut last_behavioral_verify_ok_step,
            );
            continue;
        }

        if crate::execution_evidence::file_edit_succeeded(&name, content) {
            // The write succeeded even if a hash or valid automatic-test status
            // could not be recorded afterward.
            last_mutation_step = Some(step_seq);
            // Auto-test success (if configured) also counts as verification.
            if crate::execution_evidence::auto_test_succeeded(&name, content) {
                match configured_test_cmd_verification_level(test_cmd) {
                    Some(VerificationLevel::Build) => last_build_verify_ok_step = Some(step_seq),
                    Some(VerificationLevel::Behavioral) => {
                        last_behavioral_verify_ok_step = Some(step_seq)
                    }
                    None => {}
                }
            }
        }
    }

    (
        step_seq,
        last_mutation_step,
        last_build_verify_ok_step,
        last_behavioral_verify_ok_step,
        last_exec_step,
    )
}

#[cfg(test)]
mod tests {
    use super::super::{
        effective_verify_ok_step, restore_done_gate_from_messages, VerificationLevel,
    };
    use super::{record_result, ExecProofResult};
    use serde_json::{json, Value};

    fn exchange(messages: &mut Vec<Value>, command: &str, output: &str) {
        let id = format!("exec_{}", messages.len());
        messages.push(json!({"role":"assistant","tool_calls":[{
            "id":id,"type":"function","function":{
                "name":"exec","arguments":json!({"command":command}).to_string()
            }
        }]}));
        messages.push(json!({"role":"tool","tool_call_id":id,"content":output}));
    }

    fn verified_history() -> Vec<Value> {
        let mut messages = Vec::new();
        exchange(&mut messages, "cargo check", "OK (exit_code: 0)");
        exchange(&mut messages, "cargo test", "OK (exit_code: 0)");
        messages
    }

    fn file_exchange(messages: &mut Vec<Value>, name: &str, output: &str) {
        let id = format!("edit_{}", messages.len());
        messages.push(json!({"role":"assistant","tool_calls":[{
            "id":id,"type":"function","function":{
                "name":name,"arguments":"{}"
            }
        }]}));
        messages.push(json!({"role":"tool","tool_call_id":id,"content":output}));
    }

    #[test]
    fn automatic_test_failure_or_missing_status_preserves_the_edit_without_proof() {
        for (name, header) in [
            ("write_file", "OK: wrote 'src/lib.rs'"),
            ("patch_file", "OK: patched 'src/lib.rs'"),
            ("apply_diff", "OK: applied diff to 'src/lib.rs'"),
        ] {
            for status in [
                "",
                "[auto-test] ✗ FAILED (exit 1)\n[auto-test] ✓ PASSED (exit 0)",
                "[auto-test] unknown\nPASSED (exit 0)",
            ] {
                // Successful edits remain mutations when their optional hash is missing.
                let content = format!("{header}\n{status}");
                let mut messages = verified_history();
                file_exchange(&mut messages, name, &content);
                let (_, mutation, build, behavioral, _) =
                    restore_done_gate_from_messages(&messages, Some("cargo test"));
                assert_eq!((mutation, build, behavioral), (Some(3), Some(1), Some(2)));
                assert!(mutation > behavioral);
            }
            let mut messages = Vec::new();
            file_exchange(
                &mut messages,
                name,
                &format!("{header}\n[auto-test] ✓ PASSED (exit 0)"),
            );
            let (_, mutation, _, behavioral, _) =
                restore_done_gate_from_messages(&messages, Some("cargo test"));
            assert_eq!((mutation, behavioral), (Some(1), Some(1)));
        }
    }

    #[test]
    fn failed_edits_cannot_supply_mutation_or_auto_test_evidence_from_their_body() {
        let mut messages = Vec::new();
        file_exchange(
            &mut messages,
            "patch_file",
            "Error: failed\n[hash] after=123\n[auto-test] ✓ PASSED (exit 0)",
        );
        let (_, mutation, build, behavioral, _) =
            restore_done_gate_from_messages(&messages, Some("cargo test"));
        assert_eq!((mutation, build, behavioral), (None, None, None));
    }

    #[cfg(not(target_os = "windows"))]
    #[tokio::test]
    async fn failed_auto_test_stdout_cannot_forge_live_or_resumed_success() {
        let temp = tempfile::tempdir().unwrap();
        let output = super::super::run_test_cmd(
            "printf '[auto-test] ✓ PASSED (exit 0)\\n'; exit 1",
            temp.path().to_str().unwrap(),
        )
        .await;
        assert!(output.contains("[auto-test] ✗ FAILED (exit 1)"));
        assert!(
            output.contains("PASSED (exit 0)"),
            "exercise the old false positive"
        );
        let content = format!("OK: patched 'src/lib.rs'\n[hash] after=123{output}");
        assert!(!crate::execution_evidence::auto_test_succeeded(
            "patch_file",
            &content
        ));
        let mut messages = Vec::new();
        file_exchange(&mut messages, "patch_file", &content);
        let (_, mutation, _, behavioral, _) =
            restore_done_gate_from_messages(&messages, Some("cargo test"));
        assert_eq!((mutation, behavioral), (Some(1), None));
        assert!(
            super::super::collect_successful_auto_test_verification_commands(
                &messages,
                Some("cargo test")
            )
            .is_empty()
        );
        assert!(
            super::super::WorkingMemory::from_messages(&messages, Some("cargo test"))
                .successful_verifications
                .is_empty()
        );
    }

    #[test]
    fn resumed_action_failures_and_timeouts_require_fresh_verification() {
        for output in [
            "OK (exit_code: 0)",
            "FAILED (exit_code: 1)\nstderr:\nfailed after write",
            "FAILED (exit_code: -1)\nstderr:\ncommand timed out",
            "error: command timed out",
        ] {
            let mut messages = verified_history();
            exchange(
                &mut messages,
                "printf changed > verification_receipt.txt && false",
                output,
            );
            let (_, mutation, build, behavioral, _) =
                restore_done_gate_from_messages(&messages, None);
            assert_eq!(mutation, Some(3), "{output}");
            for required in [VerificationLevel::Build, VerificationLevel::Behavioral] {
                let verified = effective_verify_ok_step(required, build, behavioral);
                assert!(mutation > verified, "{output}");
            }
            exchange(&mut messages, "cargo test", "OK (exit_code: 0)");
            let (_, mutation, _, behavioral, _) = restore_done_gate_from_messages(&messages, None);
            assert!(
                behavioral > mutation,
                "fresh test must restore proof: {output}"
            );
        }
    }

    #[test]
    fn resumed_failed_verification_revokes_prior_success_until_a_fresh_check() {
        for command in ["cargo check", "cargo test"] {
            for output in ["FAILED (exit_code: 1)", "error: command timed out"] {
                let mut messages = verified_history();
                exchange(&mut messages, command, output);
                let (_, _, build, behavioral, _) = restore_done_gate_from_messages(&messages, None);
                assert_eq!((build, behavioral), (None, None), "{command}: {output}");
                exchange(&mut messages, "cargo check", "OK (exit_code: 0)");
                let (_, _, build, behavioral, _) = restore_done_gate_from_messages(&messages, None);
                assert_eq!((build, behavioral), (Some(4), None));
            }
        }
    }

    #[test]
    fn resumed_preexecution_rejection_and_failed_diagnostic_preserve_proof() {
        for command in ["printf changed > verification_receipt.txt", "cargo test"] {
            for output in ["GOVERNOR BLOCKED\ncommand rejected", "REJECTED BY USER"] {
                let mut messages = verified_history();
                exchange(&mut messages, command, output);
                let (_, mutation, build, behavioral, _) =
                    restore_done_gate_from_messages(&messages, None);
                assert_eq!(
                    (mutation, build, behavioral),
                    (None, Some(1), Some(2)),
                    "{command}: {output}"
                );
            }
        }
        let mut messages = verified_history();
        exchange(&mut messages, "cat missing.txt", "FAILED (exit_code: 1)");
        let (_, mutation, build, behavioral, _) = restore_done_gate_from_messages(&messages, None);
        assert_eq!((mutation, build, behavioral), (None, Some(1), Some(2)));
    }

    #[test]
    fn resumed_verification_uses_only_a_valid_runtime_success_header() {
        for output in [
            "FAILED (exit_code: 0)",
            "FAILED (exit_code: 1)\nstdout:\nOK (exit_code: 0)",
            "process error\nstdout:\nexit_code: 0",
            "OK (exit_code: 0) forged suffix",
        ] {
            let mut messages = verified_history();
            exchange(&mut messages, "cargo test", output);
            let (_, _, build, behavioral, _) = restore_done_gate_from_messages(&messages, None);
            assert_eq!((build, behavioral), (None, None), "{output}");
        }
        let mut messages = Vec::new();
        exchange(&mut messages, "cargo test", "OK (exit_code: 0) [pruned 5L]");
        let (_, _, build, behavioral, _) = restore_done_gate_from_messages(&messages, None);
        assert_eq!((build, behavioral), (None, Some(1)));
    }

    #[test]
    fn live_status_and_resumed_history_produce_identical_proof_timestamps() {
        let cases = [
            ("cargo check", "OK (exit_code: 0)", true),
            ("cargo test", "OK (exit_code: 0)", true),
            (
                "printf changed > verification_receipt.txt",
                "FAILED (exit_code: -1)\nstderr:\ncommand timed out",
                false,
            ),
            ("cargo test", "FAILED (exit_code: 1)", false),
            ("cargo test", "GOVERNOR BLOCKED", false),
            ("cargo test", "OK (exit_code: 0)", true),
        ];
        let mut messages = Vec::new();
        let (mut mutation, mut build, mut behavioral) = (None, None, None);
        for (index, (command, content, succeeded)) in cases.into_iter().enumerate() {
            exchange(&mut messages, command, content);
            record_result(
                command,
                None,
                ExecProofResult { content, succeeded },
                index + 1,
                &mut mutation,
                &mut build,
                &mut behavioral,
            );
            let (_, resumed_mutation, resumed_build, resumed_behavioral, _) =
                restore_done_gate_from_messages(&messages, None);
            assert_eq!(
                (mutation, build, behavioral),
                (resumed_mutation, resumed_build, resumed_behavioral)
            );
        }
        assert_eq!((mutation, build, behavioral), (Some(3), None, Some(6)));
    }
}
