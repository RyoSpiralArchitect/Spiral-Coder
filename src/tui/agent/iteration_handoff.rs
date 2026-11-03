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
            && !note.source_task_id.is_empty()
            && note.source_task_id.chars().count() <= 160
            && note.summary.chars().count() <= 600
            && !note.future_action.is_empty()
            && note.future_action.chars().count() <= 800)
            .then_some(note)
    }
}

pub(crate) fn remember(
    session: &mut crate::agent_session::AgentSession,
    path: Option<&std::path::Path>,
    task_id: &str,
    reply: &ObserverSuggestionEnvelope,
) -> anyhow::Result<()> {
    use crate::agent_session::AgentSession;
    // Do not replace a newer autosave when the pane received an old answer.
    // The coupled report still retains the in-memory session and the raw reply.
    let durable = path
        .filter(|p| p.exists())
        .map(AgentSession::load)
        .transpose();
    let matches = durable
        .as_ref()
        .ok()
        .and_then(|s| s.as_ref())
        .is_some_and(|saved| {
            saved.tool_root == session.tool_root && saved.messages == session.messages
        });
    if !Note::record(&mut session.messages, task_id, reply) {
        return Ok(());
    }
    if let (Some(path), Some(mut saved)) = (path, durable?) {
        anyhow::ensure!(
            matches,
            "native session advanced before the Observer note could be saved"
        );
        Note::record(&mut saved.messages, task_id, reply);
        AgentSession::save_atomic(path, &saved)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_terminal_clarification_survives_same_task_reload_as_advice() {
        let mut m = vec![json!({"role":"user","content":"Fix it."})];
        let mut reply: ObserverSuggestionEnvelope = serde_json::from_value(json!({"summary":"Still blocked.","suggestions":[{"kind":"clarify","reason":"Read the recorded failure.","suggested_tool":null,"suggested_args":{}}]})).unwrap();
        assert!(Note::record(&mut m, "round-one", &reply));
        let mut restored: Vec<Value> =
            serde_json::from_str(&serde_json::to_string(&m).unwrap()).unwrap();
        restored.push(crate::task_origin::user_message(
            "Continue.",
            crate::task_origin::MessageOrigin::Runtime,
        ));
        assert_eq!(
            Note::from_task(&restored).unwrap().source_task_id,
            "round-one"
        );
        assert!(crate::task_origin::provider_messages(&restored)[0]
            .get("iteration_handoff")
            .is_none());
        restored.push(json!({"role":"user","content":"Fix it."}));
        assert!(Note::from_task(&restored).is_none());
        reply.response_contract.required = true;
        assert!(!Note::record(&mut restored, "other", &reply));
    }
}
