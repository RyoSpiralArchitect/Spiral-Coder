//! Verification authority supplied by the root task and project configuration.

use super::{classify_exec_kind, classify_verify_level, ExecKind, VerificationLevel};

pub(super) struct ExecClassification {
    pub kind: ExecKind,
    pub verification: Option<VerificationLevel>,
}

pub(super) struct ExecVerificationContext<'a> {
    configured: Option<&'a str>,
    approved_required: Vec<String>,
}

impl<'a> ExecVerificationContext<'a> {
    /// Only the root task grants this authority, never an assistant scratchpad
    /// or a command's output. Individual required checks still need their own
    /// successful result in the separate benchmark proof ledger.
    pub(super) fn from_root(configured: Option<&'a str>, root_user_text: &str) -> Self {
        Self {
            configured,
            approved_required: super::task_harness::approved_benchmark_required_commands(
                root_user_text,
            ),
        }
    }

    pub(super) fn from_messages(
        configured: Option<&'a str>,
        messages: &[serde_json::Value],
    ) -> Self {
        let root_user_text = messages
            .iter()
            .rev()
            .find(|message| message["role"].as_str() == Some("user"))
            .and_then(|message| message["content"].as_str())
            .unwrap_or("");
        Self::from_root(configured, root_user_text)
    }

    pub(super) fn classify(&self, command: &str) -> ExecClassification {
        let verification = classify_verify_level(command, self.configured);
        if self
            .approved_required
            .iter()
            .any(|required| !command.trim().is_empty() && command.trim() == required.trim())
        {
            // An explicitly approved custom check can run an unfamiliar runner
            // or write its own receipt. Its exact contract grants verification
            // authority; unrelated or extended shell commands remain Actions.
            return ExecClassification {
                kind: ExecKind::Verify,
                verification: Some(verification.unwrap_or(VerificationLevel::Behavioral)),
            };
        }
        ExecClassification {
            kind: classify_exec_kind(command, self.configured),
            verification,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn live_and_resumed_contract_checks_share_failure_and_mutation_boundaries() {
        let check = "verify-custom 'A  B' && printf verified > receipt";
        let root = format!(
            "<observer_benchmark_plan>\nrequired_checks:\n- {check}\n</observer_benchmark_plan>"
        );
        let context = ExecVerificationContext::from_root(None, &root);
        let mut messages = vec![json!({"role":"user","content":root})];
        let (mut mutation, mut build, mut behavioral) = (None, None, None);
        let cases = [
            (
                "printf changed > src.rs",
                "OK (exit_code: 0)",
                true,
                Some(1),
                None,
            ),
            (check, "OK (exit_code: 0)", true, Some(1), Some(2)),
            (check, "GOVERNOR BLOCKED", false, Some(1), Some(2)),
            (check, "FAILED (exit_code: 1)", false, Some(1), None),
            (check, "OK (exit_code: 0)", true, Some(1), Some(5)),
            (
                "verify-custom 'A B' && printf verified > receipt",
                "OK (exit_code: 0)",
                true,
                Some(6),
                Some(5),
            ),
            (
                "printf changed > src.rs && false",
                "FAILED (exit_code: 1)",
                false,
                Some(7),
                Some(5),
            ),
        ];
        for (index, (command, content, succeeded, expected_mutation, expected_verification)) in
            cases.into_iter().enumerate()
        {
            let id = format!("call_{index}");
            messages.push(json!({"role":"assistant","tool_calls":[{
                "id":id,"function":{"name":"exec","arguments":json!({"command":command}).to_string()}
            }]}));
            messages.push(json!({"role":"tool","tool_call_id":id,"content":content}));
            super::super::exec_proof::record_result(
                command,
                &context,
                super::super::exec_proof::ExecProofResult { content, succeeded },
                index + 1,
                &mut mutation,
                &mut build,
                &mut behavioral,
            );
            assert_eq!(
                (mutation, behavioral),
                (expected_mutation, expected_verification)
            );
            let (_, resumed_mutation, resumed_build, resumed_behavioral, _) =
                super::super::restore_done_gate_from_messages(&messages, None);
            assert_eq!(
                (mutation, build, behavioral),
                (resumed_mutation, resumed_build, resumed_behavioral)
            );
        }
    }

    #[test]
    fn only_exact_root_contract_checks_gain_custom_verification_authority() {
        let command = "verify-custom 'A  B' && printf verified > receipt";
        let root = format!("<observer_benchmark_plan>\nrequired_checks:\n- {command}\nsuccess_criteria:\n- check the artifact\n</observer_benchmark_plan>");
        let context = ExecVerificationContext::from_root(None, &root);
        assert_eq!(context.classify(command).kind, ExecKind::Verify);
        assert_eq!(
            context.classify(command).verification,
            Some(VerificationLevel::Behavioral)
        );
        for changed in [
            command.replace("A  B", "A B"),
            command.replace("verify-custom", "Verify-custom"),
            format!("{command} && touch changed"),
        ] {
            assert_eq!(context.classify(&changed).kind, ExecKind::Action);
        }
        let messages = vec![
            json!({"role":"user","content":"Inspect the repository"}),
            json!({"role":"assistant","content":root}),
        ];
        let unapproved = ExecVerificationContext::from_messages(None, &messages);
        assert_eq!(unapproved.classify(command).kind, ExecKind::Action);
        assert_eq!(
            ExecVerificationContext::from_root(None, command)
                .classify(command)
                .kind,
            ExecKind::Action
        );
    }
}
