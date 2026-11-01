//! Path mentions do not add literal requirements to a typed task.
use super::super::{evidence_packet, requirement_proof::Contract};
use super::{FollowupKind, FollowupRequirement};
use serde_json::Value;
use std::collections::BTreeSet;

pub(super) fn pending(messages: &[Value], contract: &Contract) -> Option<FollowupRequirement> {
    // Project full native evidence only when there is a relevant diagnostic to bind.
    if !messages.iter().any(|m| {
        m["role"] == "tool"
            && m["content"]
                .as_str()
                .is_some_and(|s| s.contains(" follow-up for "))
    }) {
        return None;
    }
    let packet = evidence_packet::project(messages, None);
    for action in packet.current_actions().into_iter().rev() {
        if action.tool != "exec" || action.executed != Some(true) || action.succeeded != Some(false)
        {
            continue;
        }
        let command = action.arguments["command"].as_str();
        for (kind, source) in action
            .result
            .as_deref()
            .unwrap_or_default()
            .lines()
            .filter_map(FollowupKind::diagnostic)
        {
            // A typed failure may request a source reference only within requirements
            // owning that exact check, and only when it identifies one writable output.
            let targets: BTreeSet<_> = contract
                .requirements
                .iter()
                .filter(|r| {
                    r.checks.iter().any(|c| Some(c.command.as_str()) == command)
                        && r.has_source(source)
                })
                .flat_map(|r| &r.artifacts)
                .filter(|a| match kind {
                    FollowupKind::Docs => a.is_markdown_document(),
                    FollowupKind::TuiReplay => a.is_spec_named("tui_replay.json"),
                    FollowupKind::RuntimeEval => a.is_spec_named("runtime_eval.json"),
                })
                .map(|a| a.path.as_str())
                .collect();
            if targets.len() == 1 {
                // An earlier read containing this text cannot clear a current
                // check failure. A later change or check supersedes the receipt.
                return Some(FollowupRequirement {
                    file_path: targets.first()?.to_string(),
                    missing_literal: source.to_owned(),
                    kind,
                });
            }
        }
    }
    None
}
