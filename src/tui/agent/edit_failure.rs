//! Failed edits remain unresolved across successful diagnostic reads and resume.
use super::{exec_proof::ExecProofResult, exec_verification::ExecVerificationContext, ExecKind};
use crate::execution_evidence::file_edit_succeeded;
use crate::streaming::ToolCallData;
use serde_json::Value;
use std::collections::BTreeMap;

fn successful_reset(
    name: &str,
    arguments: &str,
    output: &str,
    context: &ExecVerificationContext<'_>,
) -> bool {
    file_edit_succeeded(name, output)
        || (name == "exec"
            && super::parse_exec_command_from_args(arguments)
                .is_some_and(|command| context.classify(&command).kind == ExecKind::Action)
            && ExecProofResult::from_history(output).succeeded)
}

/// Pruning older successful exchanges must not resurrect preceding failures.
pub(super) fn protected_reset_call_id(
    messages: &[Value],
    context: &ExecVerificationContext<'_>,
) -> Option<String> {
    let mut pending = BTreeMap::new();
    let mut latest = None;
    for message in crate::task_origin::current_task_messages(messages) {
        match message["role"].as_str() {
            Some("assistant") => {
                for call in message["tool_calls"].as_array().into_iter().flatten() {
                    if let (Some(id), Some(name), Some(arguments)) = (
                        call["id"].as_str().filter(|id| !id.is_empty()),
                        call["function"]["name"].as_str(),
                        call["function"]["arguments"].as_str(),
                    ) {
                        pending.insert(id, (name, arguments));
                    }
                }
            }
            Some("tool") => {
                let Some(id) = message["tool_call_id"].as_str() else {
                    continue;
                };
                if pending.remove(id).is_some_and(|(name, arguments)| {
                    successful_reset(
                        name,
                        arguments,
                        message["content"].as_str().unwrap_or(""),
                        context,
                    )
                }) {
                    latest = Some(id.to_string());
                }
            }
            _ => {}
        }
    }
    latest
}

#[derive(Debug, Default, PartialEq)]
pub(super) struct EditFailureMemory {
    failures: usize,
    last_attempt: Option<(String, Value)>,
    repeated_attempts: usize,
    read_since_failure: bool,
}

impl EditFailureMemory {
    pub(super) fn from_messages(messages: &[Value], context: &ExecVerificationContext<'_>) -> Self {
        let mut memory = Self::default();
        let mut pending = BTreeMap::new();
        for message in crate::task_origin::current_task_messages(messages) {
            match message["role"].as_str() {
                Some("assistant") => {
                    for call in message["tool_calls"].as_array().into_iter().flatten() {
                        let Some(id) = call["id"].as_str().filter(|id| !id.is_empty()) else {
                            continue;
                        };
                        let (Some(name), Some(arguments)) = (
                            call["function"]["name"].as_str(),
                            call["function"]["arguments"].as_str(),
                        ) else {
                            continue;
                        };
                        pending.insert(id, (name, arguments));
                    }
                }
                Some("tool") => {
                    let Some((name, arguments)) = message["tool_call_id"]
                        .as_str()
                        .and_then(|id| pending.remove(id))
                    else {
                        continue;
                    };
                    memory.record(
                        name,
                        arguments,
                        message["content"].as_str().unwrap_or(""),
                        context,
                    );
                }
                _ => {}
            }
        }
        memory
    }

    pub(super) fn on_result(
        &mut self,
        call: &ToolCallData,
        output: &str,
        context: &ExecVerificationContext<'_>,
    ) {
        self.record(&call.name, &call.arguments, output, context);
    }

    fn record(
        &mut self,
        name: &str,
        arguments: &str,
        output: &str,
        context: &ExecVerificationContext<'_>,
    ) {
        if successful_reset(name, arguments, output, context) {
            // A failed automatic test still follows an actual edit. Verification
            // recovery is owned by RecoveryGovernor, separately from edit retry.
            *self = Self::default();
            return;
        }
        if name == "read_file"
            && self.needs_target_read(name, arguments)
            && output.lines().next().is_some_and(|line| {
                line.starts_with('[') && line.contains(" lines, ") && line.contains(" bytes)")
            })
        {
            self.read_since_failure = true;
        }
        if !matches!(name, "patch_file" | "apply_diff" | "write_file") {
            return;
        }
        let header = output.trim_start().lines().next().unwrap_or("");
        if !header.starts_with("ERROR:") && !header.starts_with("ERROR ") {
            // User refusals, governor blocks and uncorrelated/malformed output
            // are not evidence that an edit was attempted and failed.
            return;
        }
        self.failures = self.failures.saturating_add(1);
        self.read_since_failure = false;
        let args = serde_json::from_str(arguments)
            .unwrap_or_else(|_| Value::String(arguments.to_string()));
        let attempt = (name.to_string(), args);
        if self.last_attempt.as_ref() == Some(&attempt) {
            self.repeated_attempts = self.repeated_attempts.saturating_add(1);
        } else {
            self.last_attempt = Some(attempt);
            self.repeated_attempts = 1;
        }
    }

    pub(super) fn count(&self) -> usize {
        self.failures
    }

    /// One successful target read after each failed edit is diagnosis, including
    /// JSON/config/docs outside src/. Unrelated diagnostics do not consume it.
    pub(super) fn needs_target_read(&self, name: &str, arguments: &str) -> bool {
        if name != "read_file" || self.read_since_failure {
            return false;
        }
        let Some((_, failed)) = self.last_attempt.as_ref() else {
            return false;
        };
        let attempted: Value = serde_json::from_str(arguments).unwrap_or_default();
        match (failed["path"].as_str(), attempted["path"].as_str()) {
            (Some(failed), Some(attempted)) if !failed.is_empty() => {
                let components = |path: &str| {
                    std::path::Path::new(path)
                        .components()
                        .filter(|c| *c != std::path::Component::CurDir)
                        .map(|c| c.as_os_str().to_os_string())
                        .collect::<Vec<_>>()
                };
                components(failed) == components(attempted)
            }
            // Legacy sessions can retain truncated tool JSON. Preserve their
            // existing source-read fallback, bounded by one actual read.
            (None, Some(attempted)) if failed.is_string() => {
                attempted.starts_with("src/") && attempted.ends_with(".rs")
            }
            _ => false,
        }
    }

    pub(super) fn diagnostic_hint(&self) -> Option<String> {
        let (_, args) = self.last_attempt.as_ref()?;
        let path = args.get("path")?.as_str()?;
        let next = if self.read_since_failure {
            "The target has been read since the failed edit. Repair using that exact content."
        } else {
            "Read this exact target before repairing it; pwd/listing/search cannot inspect the failed edit."
        };
        Some(format!(
            "[Pending edit failure] Target path (quoted data): {}. {next} Preserve unrelated content, then run the required checks. A read alone does not resolve the failure.",
            serde_json::to_string(path).expect("path serializes")
        ))
    }

    pub(super) fn reflection_reason(&self) -> Option<String> {
        (self.failures >= 2).then(|| {
            format!(
                "{} unresolved edit failures despite diagnostics",
                self.failures
            )
        })
    }

    pub(super) fn strategy_hint(&self) -> Option<String> {
        self.reflection_reason()?;
        let (name, arguments) = self.last_attempt.as_ref()?;
        let path: String = arguments["path"]
            .as_str()
            .unwrap_or("the target")
            .chars()
            .take(160)
            .map(|ch| if ch.is_control() { ' ' } else { ch })
            .collect();
        let path = serde_json::to_string(&path).expect("string serialization");
        let repeated = if self.repeated_attempts >= 2 {
            format!(
                " The identical {name} arguments failed {} times.",
                self.repeated_attempts
            )
        } else {
            String::new()
        };
        Some(format!(
            "[Edit recovery] {} edit attempts remain unresolved; a successful read is diagnosis, not a successful edit.{repeated}\n\
             Target path (quoted data): {path}. Change the edit strategy now. Do not resend the same failed edit or rewrite a large guessed block. \
             Use the latest exact file contents to choose the smallest unique anchor and preserve surrounding fields. \
             If the latest read is unavailable or stale, read the target first. For a structural change, use a context-backed diff \
             or a format-aware transformation that preserves unrelated content. Do not guess which duplicate match to replace, \
             reinterpret escapes, or overwrite the entire file merely to bypass an anchor mismatch. \
             After an actual edit succeeds, run the required verification; do not report completion from reads alone.",
            self.failures
        ))
    }
}

#[cfg(test)]
#[path = "edit_failure_tests.rs"]
mod tests;
