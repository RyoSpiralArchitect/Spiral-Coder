//! Native Coder request construction and same-task continuation.
use super::*;

pub(super) async fn send_coder_with_text(
    app: &mut App,
    tx: &mpsc::Sender<RequestEvent>,
    text: String,
) {
    send_coder_input(app, tx, text, CoderInput::HumanSteering).await;
}

pub(super) async fn send_coder_input(
    app: &mut App,
    tx: &mpsc::Sender<RequestEvent>,
    text: String,
    input: CoderInput,
) {
    let text = text.trim().to_string();
    if text.is_empty() || app.coder.streaming {
        return;
    }

    if let Some(selector) = parse_meta_diagnose_command(&text) {
        send_meta_diagnose(app, tx, &selector).await;
        return;
    }

    // Handle slash commands before sending to AI.
    if handle_slash_command(&text, app, PaneId::Coder) {
        return;
    }
    if let Some(problem) = validate_pane_ready(app, PaneId::Coder) {
        app.coder.push_tool(problem);
        return;
    }

    // Abort any previous task before starting a new one.
    if let Some(handle) = app.coder_task.take() {
        handle.abort();
    }

    // Reset state for the new send.
    app.coder_iter = 0;
    app.coder.scroll = 0; // pin to bottom for new output
    app.ignore_coder_tokens = false;
    app.coder_realize_state = None;

    // Resolve tool_root early (needed for @ref file reads below).
    let tool_root = app.tool_root.clone().or_else(|| {
        std::env::current_dir()
            .ok()
            .map(|p| p.to_string_lossy().into_owned())
    });

    coder_session::restore(app);
    if app.coder_intent_anchor.is_none() {
        if let Some(session) = &app.coder_session {
            let original = crate::task_origin::root_user_text(&session.messages);
            app.coder_intent_anchor = Some(intent::apply_intent_update(
                None,
                intent::normalize_intent_update(original, None),
                original,
            ));
        }
    }
    let intent_text = if input == CoderInput::RuntimeContinuation {
        "Continue."
    } else {
        &text
    };
    let intent_update =
        intent::normalize_intent_update(intent_text, app.coder_intent_anchor.as_ref());
    let intent_anchor =
        intent::apply_intent_update(app.coder_intent_anchor.as_ref(), intent_update, intent_text);
    let intent_anchor_message = intent::render_intent_anchor(&intent_anchor);
    observer_request::prepare_coder_continuation(app, &intent_anchor);
    let observer_soft_hint = app.pending_observer_hint.take();
    let observer_contract = app.pending_observer_contract.take();
    if intent_anchor.requires_human_confirmation {
        app.coder.push_tool(
            "[intent] ambiguous update detected; keeping current scope until clarified."
                .to_string(),
        );
    } else if intent_anchor.last_update_no_op {
        app.coder
            .push_tool("[intent] continue update preserved current scope.".to_string());
    }
    app.coder_intent_anchor = Some(intent_anchor.clone());
    if observer_soft_hint.is_some() {
        app.coder.push_tool(
            "(observer suggestion) advisory applied to this coder continuation turn.".to_string(),
        );
    }
    if observer_contract.is_some() {
        app.coder.push_tool(
            "(observer contract) next coder turn should explicitly accept, override, or defer the latest high-confidence observer suggestion."
                .to_string(),
        );
    }
    app.active_observer_contract = observer_contract.clone();
    app.active_observer_first_tool = None;

    // Expand @file references: read files and collect system messages to inject.
    let at_refs = parse_at_refs(&text);
    let mut at_ref_messages: Vec<ChatMessage> = Vec::new();
    app.coder.push_user(text.clone());
    for ref_path in &at_refs {
        let (content, is_err) = crate::file_tools::tool_read_file(ref_path, tool_root.as_deref());
        if is_err {
            app.coder.push_tool(format!("📎 @{ref_path}: not found"));
        } else {
            // Header line is "[path] (N lines, B bytes)" — use it as the notification.
            let header = content
                .lines()
                .next()
                .unwrap_or(ref_path.as_str())
                .to_string();
            app.coder.push_tool(format!("📎 injected: {header}"));
            at_ref_messages.push(ChatMessage {
                role: "system".to_string(),
                content: format!("[@{ref_path}]\n{content}"),
            });
        }
    }
    app.coder.streaming = true;
    app.coder
        .messages
        .push(Message::new_streaming(Role::Assistant));

    let history = Vec::new(); // Execution uses the native session, never display history.
    let cfg = app.coder_cfg.clone();
    let max_iters = app.coder_max_iters.unwrap_or(agent::DEFAULT_MAX_ITERS);
    let realize_preset = app.coder_realize_preset;

    let persona_prompt = resolve_persona(&cfg.persona)
        .map(|p| p.prompt)
        .unwrap_or("");
    let lang = language_instruction(Some(&app.lang), &cfg.mode);
    let messages = build_coder_request_messages(
        lang,
        realize_preset,
        &history,
        intent_anchor_message,
        observer_soft_hint,
        observer_contract
            .as_ref()
            .map(build_observer_response_contract_message),
        &at_ref_messages,
        &text,
        persona_prompt,
    );

    // Scan project context once per session (guarded by stack_label being None).
    let (project_context, agents_md): (Option<String>, Option<String>) =
        if app.project_stack_label.is_none() {
            if let Some(ref root) = tool_root {
                if let Some(ctx) = crate::project::ProjectContext::scan(root).await {
                    app.project_stack_label = Some(ctx.stack_label());
                    if app.project_test_cmd.is_none() {
                        app.project_test_cmd = ctx.test_cmd.clone();
                    }
                    let agents = ctx.agents_md.clone();
                    (Some(ctx.to_context_text()), agents)
                } else {
                    (None, None)
                }
            } else {
                (None, None)
            }
        } else {
            (None, None)
        };
    let test_cmd = app.project_test_cmd.clone();

    let continuing = input == CoderInput::RuntimeContinuation || intent_anchor.last_update_no_op;
    if continuing && app.coder_session.is_none() {
        app.coder.push_tool(
            "No native task is available to continue; send the full request.".to_string(),
        );
        app.coder.finish_stream();
        return;
    }
    let budget_path = tool_root
        .as_deref()
        .map(|r| crate::runtime_storage::state_path(r, "tui-work.json"));
    let start = coder_session::start_state(app.coder_session.as_ref(), messages, continuing);
    // Clear the UI rollback handle before startup can fail or be interrupted.
    // A new task may only publish its newly captured checkpoint.
    app.last_git_checkpoint = start.checkpoint.clone();
    let root_request = crate::task_origin::root_user_text(&start.messages);
    app.coder_session = Some(crate::agent_session::AgentSession::new(
        tool_root.clone(),
        start.checkpoint.clone(),
        start.cur_cwd.clone(),
        start.observation_cache.clone(),
        start.messages.clone(),
    ));
    if !continuing {
        app.work_budget = crate::work_budget::WorkBudget::new(root_request, max_iters, budget_path);
    } else if app.work_budget.snapshot().root != root_request {
        app.work_budget = budget_path
            .and_then(|path| crate::work_budget::WorkBudget::load(root_request, path))
            .unwrap_or_else(|| crate::work_budget::WorkBudget::new(root_request, 0, None));
    }
    if let Some(root) = tool_root.as_deref() {
        if let Err(error) = app.work_budget.protect_replay(std::path::Path::new(root)) {
            app.coder
                .push_tool(format!("Could not preserve the replay baseline: {error}"));
            app.coder.finish_stream();
            return;
        }
    }
    let remaining = app.work_budget.remaining();
    if remaining == 0 {
        app.coder.push_tool("Shared task budget exhausted; completion is unconfirmed. Remaining checks and the final report stay pending.".into());
        app.coder.finish_stream();
        app.pending_observer_hint = None;
        app.active_observer_contract = None;
        return;
    }
    let max_iters = if observer_contract.is_none() && remaining >= 8 && app.coder_chunk_reserve > 0
    {
        remaining
            .saturating_sub(
                app.coder_chunk_reserve.max(
                    agent::evidence_packet::project(&start.messages, test_cmd.as_deref())
                        .pending_exact_checks
                        .len()
                        + 3,
                ),
            )
            .max(1)
    } else {
        remaining
    };
    let chunk_limit = app.coder_chunk_requests.filter(|_| {
        !app.work_budget
            .snapshot()
            .requests
            .iter()
            .any(|r| r.role == "observer")
    });
    let max_iters = chunk_limit.map_or(max_iters, |limit| max_iters.min(limit));
    let request_budget = chunk_limit.map_or_else(
        || app.work_budget.clone(),
        |_| app.work_budget.with_request_allowance(max_iters),
    );
    app.coder_completed = None;
    app.active_observer_start = app
        .coder_session
        .as_ref()
        .map(|session| {
            agent::evidence_packet::project(&session.messages, test_cmd.as_deref())
                .actions
                .into_iter()
                .map(|a| a.call_id)
                .collect()
        })
        .unwrap_or_default();
    app.active_assist_id = app
        .active_observer_contract
        .as_ref()
        .map(|_| app.observer_generation);

    let autosaver = coder_session::autosaver(app);
    let error_session_path = coder_session::session_path(app);
    let error_task_root = root_request.to_owned();
    let handle = request_stream::spawn(
        tx,
        &mut app.coder_generation,
        Some(crate::work_budget::Context {
            budget: request_budget,
            role: "coder",
        }),
        move |tx| async move {
            let approver = crate::approvals::AutoApprover;
            match agent::run_agentic_json(
                start,
                &cfg,
                tool_root.as_deref(),
                max_iters,
                tx.clone(),
                project_context,
                agents_md,
                test_cmd,
                false,
                Some(realize_preset),
                autosaver,
                &approver,
            )
            .await
            {
                Ok(end) => Some(crate::agent_session::AgentSession::new(
                    tool_root,
                    end.checkpoint,
                    end.cur_cwd,
                    end.observation_cache,
                    end.messages,
                )),
                Err(e) => {
                    if e.downcast_ref::<crate::work_budget::ChunkExhausted>()
                        .is_some()
                    {
                        let _ = tx.send(StreamToken::Delta("[agent] Coder request chunk cap reached; task remains incomplete.\n".into())).await;
                    }
                    let _ = tx.send(StreamToken::Error(format!("{e:#}"))).await;
                    error_session_path
                        .and_then(|path| crate::agent_session::AgentSession::load(&path).ok())
                        .filter(|session| {
                            crate::task_origin::root_user_text(&session.messages) == error_task_root
                        })
                }
            }
        },
    );
    app.coder_task = Some(handle);
}

pub(super) fn build_coder_request_messages(
    lang: &str,
    realize_preset: agent::RealizePreset,
    history: &[ChatMessage],
    intent_anchor_message: String,
    observer_soft_hint: Option<String>,
    observer_response_contract: Option<String>,
    at_ref_messages: &[ChatMessage],
    user_text: &str,
    persona_prompt: &str,
) -> Vec<ChatMessage> {
    let system = agent::coder_system(persona_prompt, lang, Some(realize_preset));
    let mut messages = vec![ChatMessage {
        role: "system".to_string(),
        content: system,
    }];
    messages.push(ChatMessage {
        role: "system".to_string(),
        content: intent_anchor_message,
    });
    if let Some(observer_hint) = observer_soft_hint {
        messages.push(ChatMessage {
            role: "system".to_string(),
            content: observer_hint,
        });
    }
    if let Some(observer_contract) = observer_response_contract {
        messages.push(ChatMessage {
            role: "system".to_string(),
            content: observer_contract,
        });
    }
    let hist_len = history.len();
    for m in history.iter().take(hist_len.saturating_sub(1)) {
        messages.push(m.clone());
    }
    messages.extend(at_ref_messages.iter().cloned());
    messages.push(ChatMessage {
        role: "user".to_string(),
        content: user_text.to_string(),
    });
    messages
}

pub(super) async fn send_coder_message(
    app: &mut App,
    tx: &mpsc::Sender<RequestEvent>,
    observer_tx: &mpsc::Sender<RequestEvent>,
) {
    let text = app.coder.textarea.lines().join("\n").trim().to_string();
    if text.is_empty() || app.coder.streaming {
        return;
    }

    if let Some(selector) = parse_meta_diagnose_command(&text) {
        if let Some(problem) = validate_pane_ready(app, PaneId::Observer) {
            app.coder.push_tool(problem);
            return;
        }
        app.coder.textarea = tui_textarea::TextArea::default();
        send_meta_diagnose(app, observer_tx, &selector).await;
        return;
    }

    // Handle slash commands before sending to AI.
    if handle_slash_command(&text, app, PaneId::Coder) {
        app.coder.textarea = tui_textarea::TextArea::default();
        return;
    }

    if let Some(problem) = validate_pane_ready(app, PaneId::Coder) {
        app.coder.push_tool(problem);
        return;
    }

    app.coder.textarea = tui_textarea::TextArea::default();
    send_coder_with_text(app, tx, text).await;
}
