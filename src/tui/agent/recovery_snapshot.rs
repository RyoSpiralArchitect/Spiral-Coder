//! Restore native failure ownership; provider text is never snapshot authority.
use super::*;

impl RecoveryFocus {
    pub(in crate::tui::agent) fn tool_message(&self, tc: &ToolCallData, content: String) -> Value {
        json!({"role":"tool", "tool_call_id":tc.id, "content":content,
            "recovery_focus":{"version":2,"state":self}})
    }

    pub(in crate::tui::agent) fn from_messages(
        messages: &[Value],
        root: Option<&str>,
        check: Option<&str>,
    ) -> Self {
        let mut state = Self::default();
        let mut refreshed_legacy = false;
        let context = ExecVerificationContext::from_messages(check, messages);
        for (tc, message) in correlated_results(messages) {
            if let Some(snapshot) = validated_snapshot(message) {
                let legacy = message[METADATA_KEY]["version"] == 1
                    && message[METADATA_KEY]["state"]["pending"]
                        .get("location")
                        .is_none();
                let output = message["content"].as_str().unwrap_or("");
                if legacy
                    && refreshed_legacy
                    && snapshot
                        .pending
                        .as_ref()
                        .is_some_and(|pending| state.check_matches(&pending.check))
                {
                    // Subsequent legacy reads/actions carry the same old cause.
                    // Replay their actual effect instead of resurrecting it.
                    state.record_result(&tc, output, root, check, &context);
                    continue;
                }
                state = snapshot;
                refreshed_legacy = false;
                // Older snapshots may carry an obsolete cause beside a newer
                // failed result. Refresh only when original-check identity is
                // available; modern snapshots preserve creation-time evidence.
                if legacy
                    && check.is_some_and(|check| state.check_matches(check))
                    && (auto_test_outcome(&tc.name, output) == AutoTestOutcome::Failed
                        || (tc.name == "exec"
                            && !ExecProofResult::from_history(output).succeeded
                            && crate::execution_evidence::exec_may_have_run(output)
                            && exec_ran_at_root(output, root)
                            && super::super::parse_exec_command_from_args(&tc.arguments)
                                .is_some_and(|command| state.check_matches(&command))))
                {
                    state.record_result(&tc, output, root, check, &context);
                    refreshed_legacy = true;
                }
            } else {
                state.record_result(
                    &tc,
                    message["content"].as_str().unwrap_or(""),
                    root,
                    check,
                    &context,
                );
            }
        }
        state
    }
}

/// Retain the current obligations and native resolution transitions. A failure
/// is not drop-safe, so removing only its later success would resurrect it.
pub(in crate::tui::agent) fn protected_call_ids(messages: &[Value]) -> Vec<String> {
    let mut protected = std::collections::BTreeSet::new();
    let mut latest = None;
    let mut previous: Option<(String, RecoveryFocus)> = None;
    for (tc, message) in correlated_results(messages) {
        let Some(state) = validated_snapshot(message) else {
            continue;
        };
        latest = Some(tc.id.clone());
        if message[METADATA_KEY]["version"] != 2 {
            previous = None;
            continue;
        }
        let output = message["content"].as_str().unwrap_or("");
        let passed = (tc.name == "exec" && ExecProofResult::from_history(output).succeeded)
            || (file_edit_succeeded(&tc.name, output)
                && auto_test_outcome(&tc.name, output) == AutoTestOutcome::Passed);
        if passed {
            if let Some((id, before)) = &previous {
                if before
                    .pending
                    .iter()
                    .chain(&before.deferred)
                    .any(|failure| !state.contains_check(&failure.check))
                {
                    protected.insert(id.clone());
                    protected.insert(tc.id.clone());
                }
            }
        }
        previous = Some((tc.id, state));
    }
    protected.extend(latest);
    protected.into_iter().collect()
}

pub(super) fn validated_snapshot(message: &Value) -> Option<RecoveryFocus> {
    let snapshot = &message[METADATA_KEY];
    if snapshot["version"] != 1 && snapshot["version"] != 2 {
        return None;
    }
    let state: RecoveryFocus = serde_json::from_value(snapshot["state"].clone()).ok()?;
    if state.pending.is_none() && !state.deferred.is_empty() {
        return None;
    }
    let mut checks = std::collections::BTreeSet::new();
    for focus in state.pending.iter().chain(&state.deferred) {
        let valid_path = if focus.location == LocationEvidence::Unknown {
            focus.path.is_empty()
                && matches!(
                    focus.observation,
                    Observation::Unlocalized | Observation::RepairAttempted
                )
        } else {
            relative_path(&focus.path, None).as_deref() == Some(focus.path.as_str())
        };
        if !valid_path
            || focus.diagnostic.len() > 2048
            || focus.check.trim().is_empty()
            || focus.check.len() > 4096
            || !checks.insert(focus.check.trim())
        {
            return None;
        }
    }
    Some(state)
}

pub(super) fn correlated_results(
    messages: &[Value],
) -> impl Iterator<Item = (ToolCallData, &Value)> {
    let mut pending = BTreeMap::new();
    crate::task_origin::current_task_messages(messages)
        .iter()
        .filter_map(move |message| {
            if message["role"] == "assistant" {
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
            if message["role"] != "tool" {
                return None;
            }
            let id = message["tool_call_id"].as_str()?;
            let (name, arguments) = pending.remove(id)?;
            Some((
                ToolCallData {
                    thought_signature: None,
                    id: id.into(),
                    name: name.into(),
                    arguments: arguments.into(),
                },
                message,
            ))
        })
}
