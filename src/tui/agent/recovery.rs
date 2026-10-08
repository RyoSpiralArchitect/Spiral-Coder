//! Recovery phases preserve the distinction between edits and their automatic tests.
use super::*;
use crate::execution_evidence::{auto_test_outcome, AutoTestOutcome};

pub(super) const AUTO_TEST_FAILURE_HINT: &str = "Recovery stage=diagnose: the edit succeeded but its automatic test failed. Read the reported failing file or diagnostic evidence, then repair it. Do not rerun the known failing check merely to enter recovery.";

#[derive(Debug, Default)]
pub(super) struct RecoveryGovernor {
    pub(super) stage: Option<RecoveryStage>,
    pub(super) required_verification: VerificationLevel,
    pub(super) focus: super::recovery_focus::RecoveryFocus,
}

impl RecoveryGovernor {
    pub(super) fn restore_focus(
        &mut self,
        messages: &[serde_json::Value],
        root: Option<&str>,
        check: Option<&str>,
    ) {
        self.focus = super::recovery_focus::RecoveryFocus::from_messages(messages, root, check);
        if self.focus.is_pending() && self.stage != Some(RecoveryStage::Diagnose) {
            self.stage = Some(if self.focus.repaired() {
                RecoveryStage::Verify
            } else if self.focus.observed() {
                RecoveryStage::Fix
            } else {
                RecoveryStage::Diagnose
            });
        }
    }

    /// A failed edit survives intervening reads and resume. Re-enter diagnosis
    /// conservatively without weakening the benchmark repair ownership guard.
    pub(super) fn restore_pending_edits(&mut self, edits: &EditFailureMemory) {
        if self.stage.is_none() && edits.count() > 0 {
            self.stage = Some(RecoveryStage::Diagnose);
        }
    }

    pub(super) fn on_successful_edit(
        &mut self,
        automatic_test: AutoTestOutcome,
        configured_level: Option<VerificationLevel>,
    ) {
        match automatic_test {
            AutoTestOutcome::Failed => self.stage = Some(RecoveryStage::Diagnose),
            AutoTestOutcome::Passed => self.on_fix_result(true, configured_level),
            AutoTestOutcome::NotRun => self.on_fix_result(true, None),
        }
    }

    pub(super) fn stage_label(&self) -> &'static str {
        match self.stage {
            None => "none",
            Some(RecoveryStage::Diagnose) => "diagnose",
            Some(RecoveryStage::Fix) => "fix",
            Some(RecoveryStage::Verify) => "verify",
        }
    }

    pub(super) fn in_recovery(&self) -> bool {
        self.stage.is_some()
    }

    pub(super) fn restore_from_session(
        mem: &FailureMemory,
        messages: &[serde_json::Value],
        required_verification: VerificationLevel,
    ) -> Self {
        let mut g = RecoveryGovernor {
            stage: None,
            required_verification,
            ..Default::default()
        };
        if mem.consecutive_failures > 0 || last_tool_looks_failed(messages) {
            g.stage = Some(RecoveryStage::Diagnose);
        }
        g
    }

    pub(super) fn maybe_block_tool(
        &self,
        tc: &ToolCallData,
        test_cmd: Option<&str>,
        task_harness: TaskHarness,
        allow_existing_followup_verify: bool,
    ) -> Option<String> {
        let Some(stage) = self.stage else {
            return None;
        };
        let name = tc.name.as_str();

        // Note: `done` is handled earlier in the main loop.
        match stage {
            RecoveryStage::Diagnose => {
                if is_diagnostic_tool_name(name) {
                    return None;
                }
                if name == "exec" {
                    let cmd =
                        parse_exec_command_from_args(tc.arguments.as_str()).unwrap_or_default();
                    if is_diagnostic_command(cmd.as_str()) {
                        return None;
                    }
                }
                if self.focus.requires_read() {
                    return self
                        .focus
                        .hint()
                        .map(|hint| format!("[Recovery Gate] stage=diagnose\n{hint}"));
                }
                if allows_artifact_creation_during_diagnose(task_harness, tc) {
                    return None;
                }
                Some(format!(
                    "[Recovery Gate] stage=diagnose\n\
You are in recovery mode. Do NOT start new work yet.\n\
Required now: run diagnostics first (e.g. `pwd`, `ls`/`dir`, `git status`, `git rev-parse --show-toplevel`)."
                ))
            }
            RecoveryStage::Fix => None, // allow edits/commands to fix
            RecoveryStage::Verify => {
                if self.focus.is_pending() {
                    if name == "read_file"
                        || (name == "exec"
                            && parse_exec_command_from_args(&tc.arguments)
                                .is_some_and(|command| self.focus.check_matches(&command)))
                    {
                        return None;
                    }
                    return self
                        .focus
                        .hint()
                        .map(|hint| format!("[Recovery Gate] stage=verify\n{hint}"));
                }
                if allows_artifact_creation_during_verify(task_harness, tc)
                    || allow_existing_followup_verify
                {
                    return None;
                }
                if name == "exec" {
                    let cmd =
                        parse_exec_command_from_args(tc.arguments.as_str()).unwrap_or_default();
                    let verify_level = classify_verify_level(cmd.as_str(), test_cmd);
                    if verify_level
                        .map(|level| level.satisfies(self.required_verification))
                        .unwrap_or(false)
                    {
                        return None;
                    }
                }
                Some(format!(
                    "[Recovery Gate] stage=verify\n\
You already applied a fix. Verify before continuing.\n\
Required now: {}",
                    verification_requirement_hint(self.required_verification, test_cmd)
                ))
            }
        }
    }

    pub(super) fn on_diagnostic_result(&mut self, ok: bool) {
        if !self.in_recovery() && !ok {
            self.stage = Some(RecoveryStage::Diagnose);
            return;
        }
        if !ok {
            self.stage = Some(RecoveryStage::Diagnose);
            return;
        }
        if self.stage == Some(RecoveryStage::Diagnose) && !self.focus.requires_read() {
            self.stage = Some(RecoveryStage::Fix);
        }
    }

    pub(super) fn on_fix_result(&mut self, ok: bool, verified_level: Option<VerificationLevel>) {
        if !self.in_recovery() && !ok {
            self.stage = Some(RecoveryStage::Diagnose);
            return;
        }
        if !ok {
            self.stage = Some(RecoveryStage::Diagnose);
            return;
        }
        if !self.focus.is_pending()
            && verified_level
                .map(|level| level.satisfies(self.required_verification))
                .unwrap_or(false)
        {
            self.stage = None;
            return;
        }
        match self.stage {
            Some(RecoveryStage::Diagnose) | Some(RecoveryStage::Fix) => {
                self.stage = Some(RecoveryStage::Verify);
            }
            _ => {}
        }
    }

    pub(super) fn on_exec_result(
        &mut self,
        kind: ExecKind,
        verify_level: Option<VerificationLevel>,
        ok: bool,
    ) {
        if !ok {
            self.stage = Some(RecoveryStage::Diagnose);
            return;
        }
        if kind == ExecKind::Verify
            && !self.focus.is_pending()
            && verify_level
                .map(|level| level.satisfies(self.required_verification))
                .unwrap_or(false)
        {
            // A successful verification ends recovery regardless of the current stage.
            self.stage = None;
            return;
        }
        match (self.stage, kind) {
            (Some(RecoveryStage::Diagnose), ExecKind::Diagnostic)
                if !self.focus.requires_read() =>
            {
                self.stage = Some(RecoveryStage::Fix);
            }
            (Some(RecoveryStage::Fix), ExecKind::Action) => {
                self.stage = Some(RecoveryStage::Verify);
            }
            _ => {}
        }
    }
}

fn last_tool_looks_failed(messages: &[serde_json::Value]) -> bool {
    let Some(last_tool) = messages
        .iter()
        .rev()
        .find(|m| m.get("role").and_then(|v| v.as_str()) == Some("tool"))
    else {
        return false;
    };
    let content = last_tool
        .get("content")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if let Some(id) = last_tool["tool_call_id"].as_str() {
        let name = messages
            .iter()
            .rev()
            .filter(|msg| msg["role"] == "assistant")
            .flat_map(|msg| msg["tool_calls"].as_array().into_iter().flatten())
            .find(|call| call["id"].as_str() == Some(id))
            .and_then(|call| call["function"]["name"].as_str());
        if let Some(name) =
            name.filter(|name| crate::execution_evidence::file_edit_succeeded(name, content))
        {
            return auto_test_outcome(name, content) == AutoTestOutcome::Failed;
        }
    }
    let low = content.to_ascii_lowercase();
    low.contains("failed (exit_code:")
        || low.contains("governor blocked")
        || low.contains("rejected by user")
        || low.contains("[result_file_err]")
}
