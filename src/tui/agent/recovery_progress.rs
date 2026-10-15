//! A completed diagnosis can advance a restored generic Diagnose phase only
//! when the last actual failure belongs to that same localized obligation.
use super::*;

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
    for (tc, message) in correlated_results(messages) {
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
                check.is_some_and(|check| focus.check_matches(check))
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
