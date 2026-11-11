use super::super::{current_evidence::Freshness, evidence_packet, *};
use super::Metrics;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

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

fn target_excerpt(arguments: &Value) -> String {
    let identity = arguments["path"]
        .as_str()
        .or_else(|| arguments["command"].as_str())
        .unwrap_or("");
    truncate_output(identity, 180)
}

fn entry(action: &evidence_packet::ActionEvidence) -> Value {
    // Full arguments (patches/source/commands) stay in the native transcript.
    let excerpt = action.result.as_deref().map(|result| {
        let header = result.lines().next().unwrap_or(result);
        let outdated_read = action.tool == "read_file"
            && action.executed == Some(true)
            && matches!(
                action.freshness,
                Freshness::Superseded | Freshness::BeforeMutation
            )
            && action.arguments["path"].as_str().is_some_and(|path| {
                crate::file_tools::read_range::header_counts(path, header).is_some()
            });
        // Keep the receipt identity and bounds. File text from an old read
        // competes with current observations and has no proof value here.
        failure_diagnostics::failure_excerpt(if outdated_read { header } else { result }, 360)
    });
    json!({"call_id":action.call_id, "tool":action.tool,
        "target_excerpt":target_excerpt(&action.arguments),
        "executed":action.executed,"succeeded":action.succeeded,
        "freshness":action.freshness,"result_step":action.result_step,
        "result_excerpt":excerpt})
}

fn approval_output(result: &str) -> bool {
    result.starts_with("REJECTED BY USER") || result.starts_with("Awaiting approval")
}

#[derive(Default)]
struct Digest {
    removed: BTreeSet<usize>,
    entries: VecDeque<Value>,
    totals: BTreeMap<String, usize>,
    exchanges: usize,
    omitted: usize,
}

impl Digest {
    fn record(&mut self, key: String, action: Value) {
        *self.totals.entry(key).or_default() += 1;
        if self.entries.len() == HISTORY_ENTRIES {
            self.entries.pop_front();
            self.omitted += 1;
        }
        self.entries.push_back(action);
    }

    fn render(&self, instruction: &str) -> Value {
        let digest = json!({"historical_only":true,
            "instruction":instruction, "compacted_exchanges":self.exchanges,
            "omitted_older_entries":self.omitted,"tool_execution_status_counts":self.totals,
            "last_historical_actions":self.entries});
        json!({"role":"assistant","content":format!("[Historical context projection]\n{}", super::render(&digest))})
    }
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
                    || a.result.as_deref().is_some_and(approval_output)
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
    let mut prior = Digest::default();
    let mut current = Digest::default();
    for start in 0..recent_start {
        let Some(end) = exchange_end(messages, start, &counts) else {
            continue;
        };
        let calls = messages[start]["tool_calls"].as_array().unwrap();
        if end > recent_start || anchors.range(start..end).next().is_some() {
            continue;
        }
        let digest = if end <= current_start {
            let results = &messages[start + 1..end];
            if results
                .iter()
                .any(|m| approval_output(m["content"].as_str().unwrap()))
            {
                continue;
            }
            // Earlier tasks have their own evidence scope. Quote their tool
            // results as history without assigning current proof or freshness.
            for result in results {
                let id = result["tool_call_id"].as_str().unwrap();
                let call = calls.iter().find(|c| c["id"] == id).unwrap();
                let tool = call["function"]["name"].as_str().unwrap();
                let args: Value = call["function"]["arguments"]
                    .as_str()
                    .and_then(|s| serde_json::from_str(s).ok())
                    .unwrap_or(Value::Null);
                prior.record(format!("{tool}:previous_task"), json!({
                    "call_id":id,"tool":tool,"task_scope":"previous_task",
                    "target_excerpt":target_excerpt(&args),
                    "result_excerpt":failure_diagnostics::failure_excerpt(result["content"].as_str().unwrap(), 360)
                }));
            }
            &mut prior
        } else if start >= current_start {
            if calls.iter().any(|c| {
                protected.contains(c["id"].as_str().unwrap())
                    || !actions.contains_key(c["id"].as_str().unwrap())
            }) {
                continue;
            }
            for call in calls {
                let action = actions[call["id"].as_str().unwrap()];
                current.record(
                    format!(
                        "{}:{:?}:{:?}",
                        action.tool, action.executed, action.succeeded
                    ),
                    entry(action),
                );
            }
            &mut current
        } else {
            continue;
        };
        digest.removed.extend(start..end);
        digest.exchanges += 1;
    }
    if prior.exchanges + current.exchanges == 0 {
        return messages.to_vec();
    }
    // Older model preambles are replaceable only when the native parser knows
    // their block format. Human/runtime/system/unknown prose stays intact.
    for (i, msg) in messages.iter().enumerate().take(recent_start) {
        let digest = if i < current_start {
            &mut prior
        } else {
            &mut current
        };
        if digest.exchanges > 0
            && msg["tool_calls"].as_array().is_none_or(Vec::is_empty)
            && !anchors.contains(&i)
            && assistant_message_is_compactable(msg)
        {
            digest.removed.insert(i);
        }
    }
    metrics.compacted_exchanges = prior.exchanges + current.exchanges;
    metrics.compacted_messages = prior.removed.len() + current.removed.len();
    metrics.omitted_history_entries = prior.omitted + current.omitted;
    let prior_first = prior.removed.first().copied();
    let current_first = current.removed.first().copied();
    let mut out = Vec::new();
    for (i, message) in messages.iter().enumerate() {
        if Some(i) == prior_first {
            out.push(prior.render("These compressed exchanges belong to earlier user tasks. Use the current task's native results and obligations for proof and the next action."));
        }
        if Some(i) == current_first {
            out.push(current.render("Compressed historical exchanges, not current file contents, executable arguments, approval or proof. Native history is retained. Use current task obligations, pending recovery, the admitted plan and recent unabridged exchanges for the next action. Reread a file when fresh content is needed. Success here describes the recorded tool status, not task completion."));
        }
        if !prior.removed.contains(&i) && !current.removed.contains(&i) {
            out.push(message.clone());
        }
    }
    out
}
