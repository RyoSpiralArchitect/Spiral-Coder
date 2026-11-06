use super::{canonicalize_tool_call_command, compact_one_line};
use crate::streaming::ToolCallData;
use serde_json::{json, Value};
use std::collections::BTreeMap;

#[path = "followup_edit_safety.rs"]
mod edit_safety;
mod evidence;
mod transforms;
pub(super) use self::evidence::*;
pub(super) use self::transforms::*;
use edit_safety::{successful_read_contents, ReadObservation};

#[path = "declared_followup.rs"]
mod declared_followup;

#[cfg(test)]
#[path = "followup_edit_safety_tests.rs"]
mod edit_safety_tests;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FollowupKind {
    Docs,
    TuiReplay,
    RuntimeEval,
}

impl FollowupKind {
    fn diagnostic(line: &str) -> Option<(FollowupKind, &str)> {
        let line = line.trim();
        for (prefix, kind) in [
            ("missing docs follow-up for ", FollowupKind::Docs),
            ("missing tui replay follow-up for ", FollowupKind::TuiReplay),
            (
                "missing runtime eval follow-up for ",
                FollowupKind::RuntimeEval,
            ),
        ] {
            if let Some(source) = line.strip_prefix(prefix) {
                return Some((kind, source.trim()));
            }
        }
        None
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FollowupRequirement {
    file_path: String,
    missing_literal: String,
    kind: FollowupKind,
}

pub(super) fn coerce_existing_followup_tool_call(
    messages: &[Value],
    tc: &ToolCallData,
    root_user_text: &str,
    recovery_stage: Option<super::RecoveryStage>,
) -> Option<(ToolCallData, String, String)> {
    if !is_followup_candidate_tool(tc)
        || edit_safety::preserve_diagnosis_or_closeout(tc, recovery_stage)
    {
        return None;
    }

    let messages = crate::task_origin::current_task_messages(messages);
    let reads = successful_read_contents(messages);
    let mutations = successful_literal_mutations(messages);
    let requirement = next_followup_requirement(messages, root_user_text, &reads, &mutations)?;
    if edit_safety::is_authored_followup_edit(tc, &requirement.file_path) {
        return None;
    }
    let rewritten = if let Some(read) = reads.get(requirement.file_path.as_str()) {
        // Partial observations can guide an authored localized edit, but cannot
        // become a whole-file SEARCH anchor or force another truncated read.
        synthesize_followup_patch(&requirement, read.snapshot.as_deref()?)?
    } else {
        ToolCallData {
            thought_signature: None,
            id: tc.id.clone(),
            name: "read_file".to_string(),
            arguments: json!({ "path": requirement.file_path }).to_string(),
        }
    };
    let original = canonicalize_tool_call_command(tc.name.as_str(), tc.arguments.as_str())
        .unwrap_or_else(|| {
            format!(
                "{}({})",
                tc.name,
                compact_one_line(tc.arguments.as_str(), 120)
            )
        });
    let coerced =
        canonicalize_tool_call_command(rewritten.name.as_str(), rewritten.arguments.as_str())
            .unwrap_or_else(|| {
                format!(
                    "{}({})",
                    rewritten.name,
                    compact_one_line(rewritten.arguments.as_str(), 120)
                )
            });
    if original == coerced {
        None
    } else {
        Some((rewritten, original, coerced))
    }
}

pub(super) fn matches_required_existing_followup(
    messages: &[Value],
    tc: &ToolCallData,
    root_user_text: &str,
) -> bool {
    let messages = crate::task_origin::current_task_messages(messages);
    let reads = successful_read_contents(messages);
    let mutations = successful_literal_mutations(messages);
    let Some(requirement) = next_followup_requirement(messages, root_user_text, &reads, &mutations)
    else {
        return false;
    };
    let Some(path) = tool_call_path(tc) else {
        return false;
    };
    if path != requirement.file_path {
        return false;
    }
    match tc.name.as_str() {
        "read_file" => true,
        "patch_file" | "apply_diff" | "write_file" => reads.contains_key(path.as_str()),
        _ => false,
    }
}

pub(super) fn required_existing_followup_no_tool_hint(
    messages: &[Value],
    root_user_text: &str,
) -> Option<String> {
    let messages = crate::task_origin::current_task_messages(messages);
    let reads = successful_read_contents(messages);
    let mutations = successful_literal_mutations(messages);
    let requirement = next_followup_requirement(messages, root_user_text, &reads, &mutations)?;
    let action = if reads.contains_key(requirement.file_path.as_str()) {
        format!("patch_file(path={})", requirement.file_path)
    } else {
        format!("read_file(path={})", requirement.file_path)
    };
    Some(format!(
        "Required follow-up edit is still pending for `{}`.\n\
Next assistant turn must call `{}` before verification or done.\n\
Do not run goal checks or finish until this follow-up path contains `{}`.",
        requirement.file_path, action, requirement.missing_literal
    ))
}

fn next_followup_requirement(
    messages: &[Value],
    root_user_text: &str,
    reads: &BTreeMap<String, ReadObservation>,
    mutations: &BTreeMap<String, String>,
) -> Option<FollowupRequirement> {
    if let Some(contract) = super::requirement_proof::parse(root_user_text).ok()? {
        return declared_followup::pending(messages, &contract);
    }
    let docs_path = docs_followup_path(root_user_text);
    let tui_replay_path = tui_replay_followup_path(root_user_text);
    let runtime_eval_path = runtime_eval_followup_path(root_user_text);
    let mut pending = Vec::new();
    let prompt_missing_literal = target_src_literal(root_user_text);

    if has_successful_primary_source_mutation(mutations) {
        if let (Some(file_path), Some(missing_literal)) =
            (docs_path.as_ref(), prompt_missing_literal.as_ref())
        {
            push_unique_requirement(
                &mut pending,
                FollowupRequirement {
                    file_path: file_path.clone(),
                    missing_literal: missing_literal.clone(),
                    kind: FollowupKind::Docs,
                },
            );
        }
        if let (Some(file_path), Some(missing_literal)) =
            (tui_replay_path.as_ref(), prompt_missing_literal.as_ref())
        {
            push_unique_requirement(
                &mut pending,
                FollowupRequirement {
                    file_path: file_path.clone(),
                    missing_literal: missing_literal.clone(),
                    kind: FollowupKind::TuiReplay,
                },
            );
        }
        if let (Some(file_path), Some(missing_literal)) =
            (runtime_eval_path.as_ref(), prompt_missing_literal.as_ref())
        {
            push_unique_requirement(
                &mut pending,
                FollowupRequirement {
                    file_path: file_path.clone(),
                    missing_literal: missing_literal.clone(),
                    kind: FollowupKind::RuntimeEval,
                },
            );
        }
    }

    for pending_requirement in missing_followup_requirements(
        messages,
        docs_path.clone(),
        tui_replay_path.clone(),
        runtime_eval_path.clone(),
    ) {
        push_unique_requirement(&mut pending, pending_requirement);
    }

    pending
        .into_iter()
        .find(|requirement| !followup_requirement_is_satisfied(requirement, reads, mutations))
}

fn push_unique_requirement(
    pending: &mut Vec<FollowupRequirement>,
    requirement: FollowupRequirement,
) {
    if !pending.contains(&requirement) {
        pending.push(requirement);
    }
}

fn tool_call_path(tc: &ToolCallData) -> Option<String> {
    let parsed = serde_json::from_str::<Value>(tc.arguments.as_str()).ok()?;
    parsed
        .get("path")
        .and_then(|v| v.as_str())
        .map(str::to_string)
}

fn is_followup_candidate_tool(tc: &ToolCallData) -> bool {
    matches!(
        tc.name.as_str(),
        "read_file"
            | "search_files"
            | "list_dir"
            | "glob"
            | "exec"
            | "patch_file"
            | "apply_diff"
            | "done"
    )
}

fn missing_followup_requirements(
    messages: &[Value],
    docs_path: Option<String>,
    tui_replay_path: Option<String>,
    runtime_eval_path: Option<String>,
) -> Vec<FollowupRequirement> {
    let mut pending = Vec::new();
    for msg in messages.iter().rev() {
        if msg.get("role").and_then(|v| v.as_str()) != Some("tool") {
            continue;
        }
        let content = msg.get("content").and_then(|v| v.as_str()).unwrap_or("");
        for (kind, literal) in content.lines().filter_map(FollowupKind::diagnostic) {
            let path = match kind {
                FollowupKind::Docs => &docs_path,
                FollowupKind::TuiReplay => &tui_replay_path,
                FollowupKind::RuntimeEval => &runtime_eval_path,
            };
            if let Some(path) = path {
                pending.push(FollowupRequirement {
                    file_path: path.clone(),
                    missing_literal: literal.to_owned(),
                    kind,
                });
            }
        }
    }
    pending
}

#[cfg(test)]
mod tests;
