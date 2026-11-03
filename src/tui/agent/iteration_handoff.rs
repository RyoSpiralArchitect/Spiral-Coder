//! A validated terminal Observer clarification is advisory memory for this human task.
use crate::tui::suggestion::{ObserverSuggestionEnvelope, ObserverSuggestionKind};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Note {
    version: u8,
    source_task_id: String,
    summary: String,
    future_action: String,
}

impl Note {
    pub(crate) fn record(
        messages: &mut [Value],
        task_id: &str,
        reply: &ObserverSuggestionEnvelope,
    ) -> bool {
        let [entry] = reply.suggestions.as_slice() else {
            return false;
        };
        if !matches!(entry.kind, ObserverSuggestionKind::Clarify)
            || entry.suggested_tool.is_some()
            || !entry
                .suggested_args
                .as_object()
                .is_some_and(|args| args.is_empty())
            || reply.response_contract.required
            || entry.reason.trim().is_empty()
        {
            return false;
        }
        let Some(task) = messages
            .iter_mut()
            .rfind(|m| crate::task_origin::is_human_request(m))
        else {
            return false;
        };
        task["iteration_handoff"] = json!(Self {
            version: 1,
            source_task_id: task_id.chars().take(160).collect(),
            summary: reply.summary.chars().take(600).collect(),
            future_action: entry.reason.chars().take(800).collect(),
        });
        true
    }

    pub(super) fn from_task(messages: &[Value]) -> Option<Self> {
        let root = crate::task_origin::current_task_messages(messages).first()?;
        let note: Self = serde_json::from_value(root.get("iteration_handoff")?.clone()).ok()?;
        (crate::task_origin::is_human_request(root)
            && note.version == 1
            && note.source_task_id.chars().count() <= 160
            && note.summary.chars().count() <= 600
            && !note.future_action.is_empty()
            && note.future_action.chars().count() <= 800)
            .then_some(note)
    }
}
