//! One provider turn on the wire: spawn the request under the current
//! work-budget scope, forward streamed tokens to the TUI channel, and join
//! the task into a typed outcome.
use super::*;
use serde_json::Value;
use std::sync::Arc;

/// Result of a single streamed provider response.
pub(super) struct StreamedTurn {
    /// Cursor of the wire request receipt, captured immediately before the
    /// request was spawned. Governor-block accounting attributes the response
    /// to this receipt rather than a positional guess at the log tail.
    pub request_cursor: Option<usize>,
    pub assistant_text: String,
    pub tool_calls: Vec<ToolCallData>,
    pub error: Option<String>,
}

/// Spawn the wire request and drain its token stream into `tx` until the
/// provider closes it. `last_request_cursor` records the receipt cursor for
/// transport-rejection and governor-block attribution.
pub(super) async fn request_turn_stream(
    client: &reqwest::Client,
    cfg: &RunConfig,
    messages: Vec<Value>,
    tools: Value,
    tx: &mpsc::Sender<StreamToken>,
    last_request_cursor: &mut Option<usize>,
) -> StreamedTurn {
    let context = crate::work_budget::current();
    let mut payload = Arc::new((messages, tools));
    let mut turn = delivery(client, cfg, payload.clone(), tx, context.clone()).await;
    let retry = match (&context, turn.request_cursor, &turn.error) {
        (Some(ctx), Some(cursor), Some(_)) if ctx.role == "coder" && !tx.is_closed() => ctx
            .budget
            .coder_reconnect(cursor)
            .map(|budget| (cursor, budget)),
        _ => None,
    };
    if let Some((parent, budget)) = retry {
        let instruction = "Coder stream recovery: the previous response was incomplete. Choose one short, complete next action from the current evidence. Any partial text or tool arguments from that delivery were discarded.";
        if tx
            .send(StreamToken::Delta(format!("\n[agent] {instruction}\n")))
            .await
            .is_ok()
        {
            emit_telemetry_event(
                tx,
                "coder_stream_reconnect",
                json!({"source_request":parent}),
            )
            .await;
            let messages = &mut Arc::make_mut(&mut payload).0;
            let at = messages
                .iter()
                .position(|m| m["role"] != "system")
                .unwrap_or(messages.len());
            messages.insert(at, json!({"role":"system","content":instruction}));
            let retry = crate::work_budget::Context {
                budget,
                role: "coder",
            };
            turn = delivery(client, cfg, payload, tx, Some(retry)).await;
        }
    }
    *last_request_cursor = turn.request_cursor;
    if let Some(error) = &turn.error {
        let _ = tx.send(StreamToken::Error(error.clone())).await;
    }
    turn
}

async fn delivery(
    client: &reqwest::Client,
    cfg: &RunConfig,
    payload: Arc<(Vec<Value>, Value)>,
    tx: &mpsc::Sender<StreamToken>,
    request_context: Option<crate::work_budget::Context>,
) -> StreamedTurn {
    let (token_tx, mut token_rx) = mpsc::channel::<StreamToken>(256);
    let client_clone = client.clone();
    let cfg_clone = cfg.clone();
    let request_cursor = request_context
        .as_ref()
        .map(|c| c.budget.snapshot().requests.len());
    let stream_task = tokio::spawn(crate::work_budget::scope(request_context, async move {
        stream_openai_compat_json(
            &client_clone,
            &cfg_clone,
            &payload.0,
            Some(&payload.1),
            token_tx,
        )
        .await
    }));
    let _cancel_stream = crate::streaming::request_guard::AbortOnDrop::new(&stream_task);

    let mistral = matches!(cfg.provider, ProviderKind::Mistral);
    let mut turn = StreamedTurn {
        request_cursor,
        assistant_text: String::new(),
        tool_calls: Vec::new(),
        error: None,
    };

    while let Some(token) = token_rx.recv().await {
        match token {
            StreamToken::Delta(s) => {
                turn.assistant_text.push_str(&s);
                let _ = tx.send(StreamToken::Delta(s)).await;
            }
            StreamToken::ToolCall(tc) => {
                if mistral {
                    let (blocks, normalized) = normalize_mistral_tool_call(&tc);
                    for block_text in blocks {
                        if !turn.assistant_text.trim().is_empty() {
                            turn.assistant_text.push_str("\n\n");
                        }
                        turn.assistant_text.push_str(&block_text);
                        let _ = tx.send(StreamToken::Delta(block_text)).await;
                    }
                    if let Some(tc) = normalized {
                        turn.tool_calls.push(tc);
                    }
                    continue;
                }
                turn.tool_calls.push(tc);
            }
            StreamToken::GovernorState(_) => {} // not emitted by inner stream
            StreamToken::RealizeState(_) => {}  // not emitted by inner stream
            StreamToken::Telemetry(ev) => {
                let _ = tx.send(StreamToken::Telemetry(ev)).await;
            }
            StreamToken::Done => break,
            StreamToken::Error(e) => {
                turn.error = Some(e);
                break;
            }
            StreamToken::Checkpoint(_) => {} // not emitted by inner stream
        }
    }

    match stream_task.await {
        Err(error) => turn.error = Some(format!("stream task panicked: {error}")),
        Ok(Err(error)) if turn.error.is_none() => turn.error = Some(format!("{error:#}")),
        _ => {}
    }
    turn
}
