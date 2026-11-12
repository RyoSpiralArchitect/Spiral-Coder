//! Native evidence for changing course after a repeated localized failure.
use super::*;
use crate::execution_evidence::{exec_cwd, exec_may_have_run, exec_outcome};

const MAX_COMPARABLE_BYTES: usize = 16 * 1024;

#[derive(Debug, Serialize)]
pub(crate) struct DiagnosticFollowup {
    failed_check: String,
    failure_path: String,
    earlier_failure_call_id: String,
    latest_failure_call_id: String,
    diagnostic_read_call_id: String,
    matching_failures: usize,
    diagnostic: String,
    #[serde(skip)]
    first_read_just_returned: bool,
}

impl DiagnosticFollowup {
    pub(in crate::tui::agent) fn should_yield(&self) -> bool {
        self.matching_failures == 2 && self.first_read_just_returned
    }
}

struct Read<'a> {
    call_id: String,
    result: &'a str,
}

struct Cycle<'a> {
    output: String,
    latest_failure: String,
    earlier_failure: Option<String>,
    matching_failures: usize,
    earlier_read_result: Option<&'a str>,
    first_read: Option<Read<'a>>,
}

impl RecoveryFocus {
    pub(in crate::tui::agent) fn diagnostic_followup(
        &self,
        messages: &[Value],
        root: Option<&str>,
    ) -> Option<DiagnosticFollowup> {
        let root = root.filter(|r| !r.is_empty()).map(Path::new)?;
        let focus = self.pending.as_ref()?;
        if focus.observation != Observation::Observed || focus.location != LocationEvidence::Current
        {
            return None;
        }
        let mut cycle: Option<Cycle<'_>> = None;
        let mut first_read_just_returned = false;
        for (call, message) in correlated_results(messages) {
            first_read_just_returned = false;
            let output = message["content"].as_str().unwrap_or_default();
            if !exec_may_have_run(output) || output.starts_with("Awaiting approval") {
                continue;
            }
            if matches!(
                call.name.as_str(),
                "exec" | "write_file" | "patch_file" | "apply_diff"
            ) {
                let previous = cycle.take();
                if call.name == "exec"
                    && exec_outcome(output) == Some(false)
                    && exec_cwd(output).map(Path::new) == Some(root)
                    && super::super::parse_exec_command_from_args(&call.arguments)
                        .is_some_and(|command| command == focus.check)
                    && matches_snapshot(message, focus, Observation::AwaitingRead)
                {
                    if let Some(output) = comparable_output(output) {
                        let same = previous.filter(|previous| {
                            previous.first_read.is_some() && previous.output == output
                        });
                        cycle = Some(Cycle {
                            output,
                            latest_failure: call.id,
                            matching_failures: same.as_ref().map_or(1, |p| p.matching_failures + 1),
                            earlier_read_result: same
                                .as_ref()
                                .and_then(|p| p.first_read.as_ref().map(|r| r.result)),
                            earlier_failure: same.map(|p| p.latest_failure),
                            first_read: None,
                        });
                    }
                }
            } else if call.name == "read_file" {
                if let Some(cycle) = cycle.as_mut() {
                    let root = root.to_str();
                    if argument_path(&call)
                        .and_then(|path| relative_path(&path, root))
                        .as_deref()
                        == Some(focus.path.as_str())
                        && recovery_progress::successful_read(&call, output, &focus.path, root)
                        && matches_snapshot(message, focus, Observation::Observed)
                    {
                        let prior = cycle
                            .first_read
                            .as_ref()
                            .map(|read| read.result)
                            .or(cycle.earlier_read_result);
                        if prior.is_some_and(|prior| prior != output) {
                            cycle.earlier_failure = None;
                            cycle.matching_failures = 1;
                            cycle.earlier_read_result = None;
                            cycle.first_read = None;
                        }
                        if cycle.first_read.is_none() {
                            cycle.first_read = Some(Read {
                                call_id: call.id,
                                result: output,
                            });
                            first_read_just_returned = true;
                        }
                    }
                }
            }
        }
        let cycle = cycle?;
        Some(DiagnosticFollowup {
            failed_check: focus.check.clone(),
            failure_path: focus.path.clone(),
            earlier_failure_call_id: cycle.earlier_failure?,
            latest_failure_call_id: cycle.latest_failure,
            diagnostic_read_call_id: cycle.first_read?.call_id,
            matching_failures: cycle.matching_failures,
            diagnostic: focus.diagnostic.clone(),
            first_read_just_returned,
        })
    }
}

fn matches_snapshot(message: &Value, focus: &FailureFocus, observation: Observation) -> bool {
    message[METADATA_KEY]["version"] == 2
        && native_snapshot(message)
            .and_then(|snapshot| snapshot.pending)
            .is_some_and(|pending| {
                pending.check == focus.check
                    && pending.path == focus.path
                    && pending.diagnostic == focus.diagnostic
                    && pending.location == LocationEvidence::Current
                    && pending.observation == observation
            })
}

fn comparable_output(output: &str) -> Option<String> {
    if output.len() > MAX_COMPARABLE_BYTES {
        return None;
    }
    let mut lines = output.lines().peekable();
    let mut body = String::with_capacity(output.len());
    while let Some(line) = lines.next() {
        // Unittest prints elapsed time just before the final status. Preserve
        // its test count and every other line of the retained result.
        let stable = unittest_timing_prefix(line).filter(|_| {
            let mut rest = lines.clone();
            rest.next() == Some("") && rest.next().is_some_and(|line| line.starts_with("FAILED ("))
        });
        body.push_str(stable.unwrap_or(line));
        body.push('\n');
    }
    Some(body)
}

fn unittest_timing_prefix(line: &str) -> Option<&str> {
    let (prefix, elapsed) = line.strip_prefix("Ran ")?.split_once(" in ")?;
    let count = prefix
        .strip_suffix(" tests")
        .or_else(|| prefix.strip_suffix(" test"))?;
    count.parse::<usize>().ok()?;
    let elapsed = elapsed.strip_suffix('s')?;
    if elapsed.is_empty() || !elapsed.bytes().all(|b| b.is_ascii_digit() || b == b'.') {
        return None;
    }
    elapsed.parse::<f64>().ok()?;
    line.split_once(" in ").map(|(prefix, _)| prefix)
}

#[cfg(test)]
#[path = "diagnostic_followup_tests.rs"]
mod tests;
