//! Native execution history is separate from the rendered conversation.
use super::*;
use crate::agent_session::{AgentSession, SessionAutoSaver};
use crate::task_origin::{user_message, MessageOrigin};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CoderInput {
    HumanSteering,
    RuntimeContinuation,
}

pub(super) fn session_path(app: &App) -> Option<PathBuf> {
    app.tool_root
        .as_deref()
        .map(|root| PathBuf::from(root).join(".spiral-coder/tui-session.json"))
}

pub(super) fn restore(app: &mut App) {
    if app.coder_session.is_some() {
        return;
    }
    let Some(path) = session_path(app) else {
        return;
    };
    if let Ok(mut session) = AgentSession::load(&path) {
        if session.tool_root.as_deref() != app.tool_root.as_deref() {
            return;
        }
        if let Some(warning) = session.repair_for_resume() {
            app.coder.push_tool(warning);
        }
        let root = crate::task_origin::root_user_text(&session.messages);
        let budget_path = path.with_file_name("tui-work.json");
        app.work_budget = crate::work_budget::WorkBudget::load(root, budget_path)
            .unwrap_or_else(|| crate::work_budget::WorkBudget::new(root, 0, None));
        app.coder_session = Some(session);
    }
}

pub(super) fn start_state(
    previous: Option<&AgentSession>,
    request: Vec<ChatMessage>,
    continuing: bool,
) -> agent::AgenticStartState {
    let mut messages = previous.map(|s| s.messages.clone()).unwrap_or_default();
    messages.retain(|message| message["ui_context"] != true);
    // The request builder's display transcript is never used for execution.
    // Fresh system context replaces prior copies; native call/result pairs stay intact.
    for (index, message) in request.into_iter().enumerate() {
        if message.role == "system" {
            if !messages
                .iter()
                .any(|m| m["role"] == "system" && m["content"] == message.content)
            {
                let managed = index == 0
                    || message.content.starts_with("[Intent Anchor]")
                    || message.content.starts_with("<observer_advisory")
                    || message.content.starts_with("<observer_response_contract");
                messages.push(
                    json!({"role":"system", "content":message.content, "ui_context": managed}),
                );
            }
        } else if message.role == "user" {
            messages.push(user_message(
                &message.content,
                if continuing {
                    MessageOrigin::Runtime
                } else {
                    MessageOrigin::User
                },
            ));
        }
    }
    agent::AgenticStartState {
        messages,
        checkpoint: previous.and_then(|s| s.checkpoint.clone()),
        cur_cwd: previous.and_then(|s| s.cur_cwd.clone()),
        observation_cache: continuing
            .then(|| previous.and_then(|s| s.observation_cache.clone()))
            .flatten(),
        session_bridge: continuing
            .then(|| previous.and_then(|s| s.session_bridge.clone()))
            .flatten(),
        create_checkpoint: previous.is_none(),
    }
}

pub(super) fn autosaver(app: &App) -> Option<std::sync::Arc<SessionAutoSaver>> {
    session_path(app)
        .map(|path| std::sync::Arc::new(SessionAutoSaver::new(path, app.coder_session.as_ref())))
}

/// Changing workspaces invalidates all native state and asynchronous advice.
/// Persisted sessions stay on disk and are restored from the new root only.
pub(super) fn change_root(app: &mut App) {
    for task in [&mut app.coder_task, &mut app.observer_task] {
        if let Some(task) = task.take() {
            task.abort();
        }
    }
    app.coder_generation = app.coder_generation.wrapping_add(1);
    app.observer_generation = app.observer_generation.wrapping_add(1);
    app.coder.finish_stream();
    app.observer.finish_stream();
    observer_request::discard_pending_assist(app);
    app.coder_session = None;
    app.coder_intent_anchor = None;
    app.coder_completed = None;
    app.active_observer_contract = None;
    app.active_assist_id = None;
    app.project_stack_label = None;
    app.project_test_cmd = None;
    app.work_budget = crate::work_budget::WorkBudget::new("", 0, None);
}
