//! Request-scoped Observer repair. Advice is never executable tool input.
use super::*;
use std::hash::{Hash, Hasher};

#[derive(Debug)]
pub(crate) struct ObserverAssistRequest {
    source_stamp: u64,
    original_prompt: String,
    repair_used: bool,
    pub(super) pending_repair: Option<String>,
}

impl ObserverAssistRequest {
    pub(super) fn new(app: &App, original_prompt: String) -> Self {
        Self {
            source_stamp: source_stamp(app),
            original_prompt,
            repair_used: false,
            pending_repair: None,
        }
    }

    fn schedule_repair(&mut self, raw: &str, issue: &str) -> bool {
        if self.repair_used {
            return false;
        }
        self.repair_used = true;
        self.pending_repair = Some(crate::observer_action::repair_prompt(
            &self.original_prompt,
            raw,
            issue,
        ));
        true
    }
}

fn source_stamp(app: &App) -> u64 {
    let mut stamp = std::collections::hash_map::DefaultHasher::new();
    app.tool_root.hash(&mut stamp);
    app.coder_intent_anchor
        .as_ref()
        .map(|a| a.revision)
        .hash(&mut stamp);
    for message in &app.coder.messages {
        std::mem::discriminant(&message.role).hash(&mut stamp);
        message.content.hash(&mut stamp);
        message.complete.hash(&mut stamp);
    }
    stamp.finish()
}

fn request_is_current(app: &App) -> bool {
    app.observer_assist_request
        .as_ref()
        .is_none_or(|request| request.source_stamp == source_stamp(app))
}

/// Coder advancement invalidates in-flight advice, even on a continuation. Only
/// already completed advice for the same intent may be consumed by that turn.
pub(super) fn prepare_coder_continuation(app: &mut App, anchor: &intent::IntentAnchor) {
    if !anchor.last_update_no_op || !request_is_current(app) {
        app.pending_observer_hint = None;
        app.pending_observer_contract = None;
    }
    if app.observer_next_action_mode {
        if let Some(task) = app.observer_task.take() {
            task.abort();
        }
        app.observer.finish_stream();
        app.observer_next_action_mode = false;
        app.observer_generation = app.observer_generation.wrapping_add(1);
        app.ignore_observer_tokens = true;
    }
    app.observer_assist_request = None;
}

pub(super) fn discard_pending_assist(app: &mut App) {
    app.observer_assist_request = None;
    app.pending_observer_hint = None;
    app.pending_observer_contract = None;
}

pub(super) async fn maybe_observer_action_repair(app: &mut App, tx: &mpsc::Sender<RequestEvent>) {
    if app.observer.streaming || app.coder.streaming {
        return;
    }
    if !request_is_current(app) {
        discard_pending_assist(app);
        return;
    }
    let Some(prompt) = app
        .observer_assist_request
        .as_mut()
        .and_then(|r| r.pending_repair.take())
    else {
        return;
    };
    if let Some(problem) = validate_pane_ready(app, PaneId::Observer) {
        app.observer.push_tool(problem);
        app.observer_assist_request = None;
        return;
    }
    start_request(
        app,
        tx,
        prompt,
        "[NEXT-ACTION REPAIR] attempt=1/1".to_string(),
    )
    .await;
}

pub(super) fn finalize_observer_next_action_suggestion(app: &mut App) {
    if !request_is_current(app) {
        discard_pending_assist(app);
        app.last_observer_suggestion = None;
        app.observer.push_tool(
            "(next-action) Coder context changed; stale advice was not forwarded.".to_string(),
        );
        return;
    }
    // The current streaming slot is authoritative, including an empty response.
    // Never fall back to an older successful Observer answer.
    let Some(idx) = app
        .observer
        .messages
        .iter()
        .rposition(|m| matches!(m.role, Role::Assistant) && m.complete)
    else {
        return;
    };
    let raw = app.observer.messages[idx].content.clone();
    let parsed = suggestion::parse_observer_suggestion_envelope(&raw);
    let issue = crate::observer_action::validation_issue(
        &raw,
        app.coder_session
            .as_ref()
            .map(|s| s.messages.as_slice())
            .unwrap_or(&[]),
        app.project_test_cmd.as_deref(),
    );
    app.pending_observer_hint = None;
    app.pending_observer_contract = None;
    if let Some(issue) = issue {
        // Keep the failed answer intact for inspection; neither free text nor
        // quickest_check can supply missing tool identity or arguments.
        app.observer.push_tool(format!("(next-action) invalid response prevented automatic forwarding: {issue}; raw Observer output was preserved."));
        let scheduled = app
            .observer_assist_request
            .as_mut()
            .is_some_and(|r| r.schedule_repair(&raw, &issue));
        app.observer.push_tool(
            if scheduled {
                "(next-action) requesting one explicit action-contract repair."
            } else {
                "(next-action) no further repair; retained for review."
            }
            .to_string(),
        );
    } else if let Some(parsed) = parsed.as_ref() {
        app.observer.messages[idx].raw_content = Some(raw);
        app.observer.messages[idx].content =
            suggestion::format_observer_suggestion_envelope(parsed);
        if let Some(hint) = build_observer_suggestion_soft_hint(parsed) {
            app.pending_observer_hint = Some(hint);
            app.pending_observer_contract = build_observer_response_contract(parsed);
            app.coder.push_tool("(observer suggestion) queued an advisory next step for the next coder continuation.".to_string());
        }
    }
    app.last_observer_suggestion = parsed;
    let stamp = source_stamp(app);
    if let Some(request) = app.observer_assist_request.as_mut() {
        request.source_stamp = stamp;
    }
}

pub(super) async fn send_next_action_assist(
    app: &mut App,
    tx: &mpsc::Sender<RequestEvent>,
    selector: &str,
    reason_hint: &str,
) {
    if let Some(problem) = validate_pane_ready(app, PaneId::Observer) {
        app.observer.push_tool(problem);
        return;
    }

    app.right_tab = RightTab::Observer;
    app.observer.scroll = 0;
    let packet = match build_tui_meta_failure_packet_for_selector(app, selector) {
        Ok(packet) => packet,
        Err(msg) => {
            app.observer.push_tool(msg);
            return;
        }
    };
    let target_id = packet
        .get("target_message_id")
        .and_then(|v| v.as_str())
        .unwrap_or("coder");
    let failure_kind = packet
        .get("failure_kind")
        .and_then(|v| v.as_str())
        .unwrap_or("unclear");
    let prompt = build_tui_next_action_prompt(&packet, app.lang.as_str(), reason_hint);
    let label = format!("[NEXT-ACTION] target={target_id} kind={failure_kind}");

    if let Some(handle) = app.observer_task.take() {
        handle.abort();
    }

    app.observer_assist_request = Some(ObserverAssistRequest::new(app, prompt.clone()));
    start_request(app, tx, prompt, label).await;
}

async fn start_request(
    app: &mut App,
    tx: &mpsc::Sender<RequestEvent>,
    prompt: String,
    label: String,
) {
    app.observer_meta_mode = false;
    app.observer_next_action_mode = true;
    app.last_observer_suggestion = None;
    app.pending_observer_hint = None;
    app.pending_observer_contract = None;
    app.ignore_observer_tokens = false;
    app.observer_loop_retry_budget = 0;
    app.observer_loop_pending = None;
    app.observer_lang_retry_budget = 1;
    app.observer_lang_pending = None;
    app.observer.push_user(label);
    app.observer.streaming = true;
    app.observer
        .messages
        .push(Message::new_streaming(Role::Assistant));

    let cfg = next_action_config(&app.observer_cfg);
    let lang = app.lang.clone();
    let handle = request_stream::spawn(
        tx,
        &mut app.observer_generation,
        Some(crate::work_budget::Context {
            budget: app.work_budget.clone(),
            role: "observer",
        }),
        move |tx| async move {
            use crate::config::ProviderKind;
            use crate::streaming::{stream_anthropic, stream_openai_compat};
            let client = reqwest::Client::new();
            let system = format!(
                "You are Observer in next-action assist mode.\n\
Do not perform a broad critique.\n\
Focus only on the next concrete step.\n\n\
[Language]\n{}",
                language_instruction(Some(&lang), &cfg.mode)
            );
            let messages = vec![
                ChatMessage {
                    role: "system".to_string(),
                    content: system,
                },
                ChatMessage {
                    role: "user".to_string(),
                    content: prompt,
                },
            ];
            let result = match cfg.provider {
                ProviderKind::Anthropic => {
                    stream_anthropic(&client, &cfg, &messages, tx.clone()).await
                }
                ProviderKind::Hf => {
                    let provider = providers::build_provider(client.clone(), &cfg);
                    let req = ChatRequest {
                        messages: messages.clone(),
                        temperature: Some(0.2),
                        max_tokens: Some(cfg.max_tokens),
                        metadata: None,
                    };
                    match provider.chat(&req).await {
                        Ok(resp) => {
                            fake_stream_text(&tx, &resp.content).await;
                            Ok(())
                        }
                        Err(e) => Err(e),
                    }
                }
                _ => stream_openai_compat(&client, &cfg, &messages, None, tx.clone()).await,
            };
            if let Err(e) = result {
                let prefix = match lang.as_str() {
                    "fr" => "Observer next-action failed",
                    "en" => "Observer next-action failed",
                    _ => "Observer next-action failed",
                };
                let _ = tx.send(StreamToken::Error(format!("{prefix}: {e}"))).await;
            }
            None
        },
    );
    app.observer_task = Some(handle);
}

/// The effective next-action settings are also used by coupled run receipts.
pub(super) fn next_action_config(cfg: &crate::config::RunConfig) -> crate::config::RunConfig {
    let mut cfg = cfg.clone();
    cfg.temperature = 0.2;
    cfg.max_tokens = cfg.max_tokens.min(1200);
    cfg
}
