//! A completed diagnosis can advance a restored generic Diagnose phase only
//! when the last actual failure belongs to that same localized obligation.
use super::*;

#[path = "recovery_edit_diagnosis.rs"]
mod edit_diagnosis;

pub(super) fn survives(
    focus: &RecoveryFocus,
    messages: &[Value],
    root: Option<&str>,
    check: Option<&str>,
) -> bool {
    let Some(pending) = focus.pending.as_ref() else {
        return false;
    };
    if !focus.observed() && !focus.repaired() {
        return false;
    }
    let mut related_failure = false;
    let mut progress_after_failure = false;
    let resolved = resolved_failures(messages, root);
    for (index, (tc, message)) in correlated_results(messages).enumerate() {
        if resolved.contains(&index) {
            continue;
        }
        let output = message["content"].as_str().unwrap_or("");
        if !crate::execution_evidence::exec_may_have_run(output)
            || output.starts_with("Awaiting approval")
        {
            continue;
        }
        let auto_failed = auto_test_outcome(&tc.name, output) == AutoTestOutcome::Failed;
        let exec_failed = tc.name == "exec" && !ExecProofResult::from_history(output).succeeded;
        let file_failed = tc.name != "exec"
            && output
                .lines()
                .next()
                .is_some_and(|line| line.starts_with("ERROR"));
        if exec_failed || auto_failed || file_failed {
            related_failure = if exec_failed {
                super::super::parse_exec_command_from_args(&tc.arguments)
                    .is_some_and(|command| focus.check_matches(&command))
            } else if auto_failed {
                owned_check(&tc, message, root)
                    .as_deref()
                    .or(check)
                    .is_some_and(|check| focus.check_matches(check))
            } else {
                argument_path(&tc)
                    .and_then(|path| relative_path(&path, root))
                    .as_deref()
                    == Some(pending.path.as_str())
            };
            progress_after_failure = false;
        } else if (tc.name == "read_file"
            && argument_path(&tc)
                .and_then(|path| relative_path(&path, root))
                .as_deref()
                == Some(pending.path.as_str())
            && output.lines().next().is_some_and(|line| {
                line.starts_with('[') && line.contains(" lines, ") && line.contains(" bytes)")
            }))
            || (focus.repaired() && file_edit_succeeded(&tc.name, output))
        {
            progress_after_failure = true;
        }
    }
    related_failure && progress_after_failure
}

/// Version 2 snapshots bind the actual failed check, including automatic tests
/// whose configuration may have changed since this result was recorded.
fn owned_check(tc: &ToolCallData, message: &Value, root: Option<&str>) -> Option<String> {
    if message[METADATA_KEY]["version"] != 2 {
        return None;
    }
    let state = recovery_snapshot::validated_snapshot(message)?;
    let pending = state.pending?;
    if !matches!(
        pending.observation,
        Observation::AwaitingRead | Observation::Unlocalized
    ) {
        return None;
    }
    let output = message["content"].as_str()?;
    let matches = if tc.name == "exec" {
        !ExecProofResult::from_history(output).succeeded
            && crate::execution_evidence::exec_may_have_run(output)
            && exec_ran_at_root(output, root)
            && super::super::parse_exec_command_from_args(&tc.arguments)
                .is_some_and(|command| command.trim() == pending.check.trim())
    } else {
        file_edit_succeeded(&tc.name, output)
            && auto_test_outcome(&tc.name, output) == AutoTestOutcome::Failed
    };
    matches.then_some(pending.check)
}

/// Ignore only a managed failure followed by its own actual success. Merely
/// disappearing from a snapshot (or passing a different check) is insufficient.
fn resolved_failures(messages: &[Value], root: Option<&str>) -> std::collections::BTreeSet<usize> {
    let mut open: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    let mut resolved = std::collections::BTreeSet::new();
    let mut previous: Option<RecoveryFocus> = None;
    for (index, (tc, message)) in correlated_results(messages).enumerate() {
        let output = message["content"].as_str().unwrap_or("");
        let current = (message[METADATA_KEY]["version"] == 2)
            .then(|| recovery_snapshot::validated_snapshot(message))
            .flatten();
        let mut passed = Vec::new();
        if tc.name == "exec"
            && ExecProofResult::from_history(output).succeeded
            && exec_ran_at_root(output, root)
        {
            if let Some(command) = super::super::parse_exec_command_from_args(&tc.arguments) {
                passed.push(command);
            }
        } else if file_edit_succeeded(&tc.name, output)
            && auto_test_outcome(&tc.name, output) == AutoTestOutcome::Passed
        {
            if let (Some(before), Some(after)) = (&previous, &current) {
                // The native transition removes exactly the configured check.
                // Do not infer past configuration from the current CLI setting.
                passed.extend(
                    before
                        .pending
                        .iter()
                        .chain(&before.deferred)
                        .filter(|failure| !after.contains_check(&failure.check))
                        .map(|failure| failure.check.clone()),
                );
            }
        }
        for check in passed {
            resolved.extend(open.remove(check.trim()).unwrap_or_default());
        }
        if let Some(check) = owned_check(&tc, message, root) {
            open.entry(check.trim().into()).or_default().push(index);
        }
        if current.is_some() {
            previous = current;
        } else if recovery_snapshot::validated_snapshot(message).is_some() {
            // A legacy snapshot cannot bind a version 2 automatic-check clear.
            previous = None;
        }
    }
    resolved
}
