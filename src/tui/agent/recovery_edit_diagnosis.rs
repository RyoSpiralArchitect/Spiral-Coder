//! Reconcile edit diagnosis with every correlated failure before restoring Fix.
//! This is derived state only: reads never resolve an edit or a failed check.
use super::*;
use crate::tui::agent::{EditFailureMemory, RecoveryStage};
use std::collections::BTreeSet;

#[derive(Debug)]
struct EditDiagnosis {
    call_id: String,
    observed: bool,
}

impl RecoveryFocus {
    /// An edit failure can target a different file from the failed check. Both
    /// diagnoses must survive a handoff, without borrowing either one's proof.
    pub(in crate::tui::agent) fn restored_edit_stage(
        &self,
        messages: &[Value],
        root: Option<&str>,
        check: Option<&str>,
    ) -> Option<RecoveryStage> {
        let context = ExecVerificationContext::from_messages(check, messages);
        let restored = EditFailureMemory::from_messages(messages, &context);
        if restored.count() == 0 {
            return None;
        }

        let resolved = super::resolved_failures(messages, root);
        let mut edits = EditFailureMemory::default();
        let mut targets = BTreeMap::<String, EditDiagnosis>::new();
        let mut failed_reads = BTreeSet::new();
        let mut failed_commands = BTreeSet::new();
        let mut last_edit = None;
        let mut unknown_edit = false;
        let mut unknown_failure = false;

        for (index, (tc, message)) in correlated_results(messages).enumerate() {
            let output = message["content"].as_str().unwrap_or("");
            if !crate::execution_evidence::exec_may_have_run(output)
                || output.starts_with("Awaiting approval")
            {
                continue;
            }
            let before = edits.count();
            edits.on_result(&tc, output, &context);
            if edits.count() == 0 {
                // Use the edit governor's actual-success boundary, including
                // shell mutations. Diagnostics and checks never reset edits.
                targets.clear();
                last_edit = None;
                unknown_edit = false;
            }
            let edit_failed = edits.count() > before;
            let path = argument_path(&tc).and_then(|path| relative_path(&path, root));
            if edit_failed {
                if let Some(path) = path.as_ref() {
                    targets.insert(
                        path.clone(),
                        EditDiagnosis {
                            call_id: tc.id.clone(),
                            observed: false,
                        },
                    );
                    last_edit = Some((path.clone(), tc.id.clone()));
                } else {
                    // Truncated legacy arguments cannot identify a failure.
                    unknown_edit = true;
                }
            }

            let file_failed =
                tc.name != "exec"
                    && output.trim_start().lines().next().is_some_and(|line| {
                        line.starts_with("ERROR:") || line.starts_with("ERROR ")
                    });
            if tc.name == "read_file" {
                if file_failed {
                    if let Some(path) = path.as_ref() {
                        failed_reads.insert(path.clone());
                        if let Some(target) = targets.get_mut(path) {
                            target.observed = false;
                        }
                    } else {
                        unknown_failure = true;
                    }
                } else if let Some(path) = path.as_ref() {
                    if successful_read(&tc, output, path, root) {
                        failed_reads.remove(path);
                        if let Some(target) = targets.get_mut(path) {
                            target.observed = true;
                        }
                    }
                }
            } else if file_failed && !edit_failed {
                unknown_failure = true;
            }

            if tc.name == "exec" {
                let command = crate::tui::agent::parse_exec_command_from_args(&tc.arguments);
                if ExecProofResult::from_history(output).succeeded {
                    if exec_ran_at_root(output, root) {
                        if let Some(command) = command {
                            failed_commands.remove(command.trim());
                        }
                    }
                } else if !resolved.contains(&index) {
                    if let Some(command) = command.filter(|_| exec_ran_at_root(output, root)) {
                        failed_commands.insert(command.trim().to_string());
                    } else {
                        unknown_failure = true;
                    }
                }
            } else if auto_test_outcome(&tc.name, output) == AutoTestOutcome::Failed
                && !resolved.contains(&index)
            {
                if let Some(command) = super::owned_check(&tc, message, root).as_deref().or(check) {
                    failed_commands.insert(command.trim().to_string());
                } else {
                    unknown_failure = true;
                }
            }
        }

        // Native check ownership retains deferred failures and independently
        // routes the newest one. An unrelated command cannot borrow that state.
        let unexplained_check = failed_commands
            .iter()
            .any(|command| !self.contains_check(command));
        let focused_diagnosis_pending = self.is_pending() && !self.observed() && !self.repaired();
        let current_edit_observed = last_edit.is_some_and(|(path, id)| {
            targets
                .get(&path)
                .is_some_and(|target| target.call_id == id && target.observed)
        });
        let complete = edits == restored
            && restored.target_was_read()
            && current_edit_observed
            && targets.values().all(|target| target.observed)
            && failed_reads.is_empty()
            && !unknown_edit
            && !unknown_failure
            && !unexplained_check
            && !focused_diagnosis_pending;
        Some(if complete {
            RecoveryStage::Fix
        } else {
            RecoveryStage::Diagnose
        })
    }
}

/// Require a native read header for the correlated argument's exact path. File
/// contents, another target's header, refusals and stale reads are not proof.
fn successful_read(tc: &ToolCallData, output: &str, path: &str, root: Option<&str>) -> bool {
    let Some((read_path, details)) = output
        .lines()
        .next()
        .and_then(|line| line.strip_prefix('['))
        .and_then(|line| line.split_once("] ("))
    else {
        return false;
    };
    relative_path(read_path, root).as_deref() == Some(path)
        && details.contains(" lines, ")
        && details.split_once(" bytes)").is_some_and(|(_, suffix)| {
            suffix.is_empty()
                || suffix == " [⚡ cached — unchanged since last read]"
                || suffix.starts_with(" [pruned ")
                || crate::file_tools::read_range::matches_range_header(
                    &tc.arguments,
                    output.lines().next().unwrap_or(""),
                )
        })
}
