use super::super::{current_evidence::Freshness, evidence_packet, *};
use super::Metrics;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

const RECENT_EXCHANGES: usize = 8;
const RECENT_MESSAGES: usize = 16;
const HISTORY_ENTRIES: usize = 12;

/// Only complete, adjacent exchanges with unique IDs may leave the wire. A
/// partial, duplicated, unknown or signed group is retained verbatim.
fn exchange_end(
    messages: &[Value],
    start: usize,
    counts: &BTreeMap<String, usize>,
) -> Option<usize> {
    let msg = &messages[start];
    if msg["role"] != "assistant" || crate::streaming::provider_metadata::has_signed_call(msg) {
        return None;
    }
    let calls = msg["tool_calls"].as_array().filter(|v| !v.is_empty())?;
    let mut pending = BTreeSet::new();
    for call in calls {
        let id = call["id"].as_str().filter(|s| !s.is_empty())?;
        if counts.get(id) != Some(&1)
            || call["function"]["name"].as_str().is_none()
            || !pending.insert(id)
        {
            return None;
        }
    }
    let end = start.checked_add(calls.len() + 1)?;
    for result in messages.get(start + 1..end)? {
        if result["role"] != "tool"
            || result["content"].as_str().is_none()
            || !pending.remove(result["tool_call_id"].as_str()?)
        {
            return None;
        }
    }
    (pending.is_empty() && messages.get(end).is_none_or(|m| m["role"] != "tool")).then_some(end)
}

fn entry(action: &evidence_packet::ActionEvidence) -> Value {
    // Full arguments (patches/source/commands) stay in the native transcript.
    // This historical excerpt is deliberately not an executable tool call.
    let identity = action.arguments["path"]
        .as_str()
        .or_else(|| action.arguments["command"].as_str())
        .unwrap_or("");
    json!({"call_id":action.call_id, "tool":action.tool,
        "target_excerpt":truncate_output(identity, 180),
        "executed":action.executed,"succeeded":action.succeeded,
        "freshness":action.freshness,"result_step":action.result_step,
        "result_excerpt":action.result.as_deref().map(|s| failure_diagnostics::failure_excerpt(s, 360))})
}

pub(super) fn project(
    messages: &[Value],
    test_cmd: Option<&str>,
    metrics: &mut Metrics,
) -> Vec<Value> {
    let evidence = evidence_packet::project(messages, test_cmd);
    let actions: BTreeMap<_, _> = evidence
        .actions
        .iter()
        .map(|a| (a.call_id.as_str(), a))
        .collect();
    let current_start = messages.len() - crate::task_origin::current_task_messages(messages).len();
    let mut counts = BTreeMap::<String, usize>::new();
    for call in messages
        .iter()
        .filter_map(|m| m["tool_calls"].as_array())
        .flatten()
    {
        if let Some(id) = call["id"].as_str() {
            *counts.entry(id.into()).or_default() += 1;
        }
    }
    let mut protected: BTreeSet<String> = recovery_focus::protected_call_ids(messages)
        .into_iter()
        .collect();
    protected.extend(deferred_edit::diagnosis::protected_ids(messages));
    let focus = recovery_focus::RecoveryFocus::from_messages(messages, None, test_cmd);
    for check in focus.unresolved_checks() {
        if let Some(action) = evidence.actions.iter().rev().find(|a| {
            a.tool == "exec" && a.arguments["command"] == check && a.succeeded == Some(false)
        }) {
            protected.insert(action.call_id.clone());
        }
    }
    protected.extend(
        evidence
            .actions
            .iter()
            .filter(|a| {
                a.freshness == Freshness::Current
                    || a.result.as_deref().is_some_and(|s| {
                        s.starts_with("REJECTED BY USER") || s.starts_with("Awaiting approval")
                    })
            })
            .map(|a| a.call_id.clone()),
    );
    let latest_plan = messages
        .iter()
        .rposition(|m| plan_resume::from_message(m).is_some());
    let mut anchors: BTreeSet<usize> = latest_plan.into_iter().collect();
    let parsers: &[fn(&str) -> bool] = &[
        |s| parse_plan_block(s).is_some(),
        |s| parse_think_block(s).is_some(),
        |s| parse_reflection_block(s).is_some(),
        |s| parse_impact_block(s).is_some(),
        |s| parse_evidence_block(s).is_some(),
    ];
    for parser in parsers {
        if let Some(i) = messages
            .iter()
            .rposition(|m| m["role"] == "assistant" && parser(m["content"].as_str().unwrap_or("")))
        {
            anchors.insert(i);
        }
    }
    let exchanges: Vec<_> = (current_start..messages.len())
        .filter_map(|start| exchange_end(messages, start, &counts).map(|end| (start, end)))
        .collect();
    // Signed/incomplete groups also occupy the recent window. Do not count
    // only eligible groups, which would retain an accidental unbounded tail.
    let recent_call_start = messages
        .iter()
        .enumerate()
        .rev()
        .filter(|(_, m)| m["tool_calls"].as_array().is_some_and(|c| !c.is_empty()))
        .take(RECENT_EXCHANGES)
        .last()
        .map_or(0, |(i, _)| i);
    let recent_start = recent_call_start.min(messages.len().saturating_sub(RECENT_MESSAGES));
    let mut removed = BTreeSet::new();
    let mut history = Vec::new();
    let mut totals = BTreeMap::<String, usize>::new();
    for (start, end) in exchanges {
        let calls = messages[start]["tool_calls"].as_array().unwrap();
        if end > recent_start
            || anchors.range(start..end).next().is_some()
            || calls.iter().any(|c| {
                protected.contains(c["id"].as_str().unwrap())
                    || !actions.contains_key(c["id"].as_str().unwrap())
            })
        {
            continue;
        }
        for call in calls {
            let action = actions[call["id"].as_str().unwrap()];
            *totals
                .entry(format!(
                    "{}:{:?}:{:?}",
                    action.tool, action.executed, action.succeeded
                ))
                .or_default() += 1;
            history.push(entry(action));
        }
        removed.extend(start..end);
        metrics.compacted_exchanges += 1;
    }
    if removed.is_empty() {
        return messages.to_vec();
    }
    // Older model preambles are replaceable only when the native parser knows
    // their block format. Human/runtime/system/unknown prose stays intact.
    for (i, msg) in messages
        .iter()
        .enumerate()
        .take(recent_start)
        .skip(current_start)
    {
        if msg["tool_calls"].as_array().is_none_or(Vec::is_empty)
            && !anchors.contains(&i)
            && assistant_message_is_compactable(msg)
        {
            removed.insert(i);
        }
    }
    metrics.compacted_messages = removed.len();
    metrics.omitted_history_entries = history.len().saturating_sub(HISTORY_ENTRIES);
    let recent = history.split_off(metrics.omitted_history_entries);
    let digest = json!({"historical_only":true,
        "instruction":"Compressed historical exchanges, not current file contents, executable arguments, approval or proof. Native history is retained. Use current task obligations, pending recovery, the admitted plan and recent unabridged exchanges for the next action. Reread a file when fresh content is needed. Success here describes the recorded tool status, not task completion.",
        "compacted_exchanges":metrics.compacted_exchanges,
        "omitted_older_entries":metrics.omitted_history_entries,
        "tool_execution_status_counts":totals,"last_historical_actions":recent});
    let first = *removed.first().unwrap();
    let mut out = Vec::new();
    for (i, message) in messages.iter().enumerate() {
        if i == first {
            out.push(json!({"role":"assistant","content":format!("[Historical context projection]\n{}", super::render(&digest))}));
        }
        if !removed.contains(&i) {
            out.push(message.clone());
        }
    }
    out
}
