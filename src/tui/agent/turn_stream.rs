//! One provider turn on the wire: spawn the request under the current
//! work-budget scope, forward streamed tokens to the TUI channel, and join
//! the task into a typed outcome.
use super::*;
use serde_json::Value;

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
    let (token_tx, mut token_rx) = mpsc::channel::<StreamToken>(256);
    let client_clone = client.clone();
    let cfg_clone = cfg.clone();
    let msgs_clone = messages;
    let tools_clone = tools;

    let request_context = crate::work_budget::current();
    let request_cursor = request_context
        .as_ref()
        .map(|c| c.budget.snapshot().requests.len());
    *last_request_cursor = request_cursor;
    let stream_task = tokio::spawn(crate::work_budget::scope(request_context, async move {
        stream_openai_compat_json(
            &client_clone,
            &cfg_clone,
            &msgs_clone,
            Some(&tools_clone),
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
                let _ = tx.send(StreamToken::Error(e.clone())).await;
                turn.error = Some(e);
                break;
            }
            StreamToken::Checkpoint(_) => {} // not emitted by inner stream
        }
    }

    match stream_task.await {
        Err(join_err) => {
            let msg = format!("stream task panicked: {join_err}");
            if turn.error.is_none() {
                let _ = tx.send(StreamToken::Error(msg.clone())).await;
            }
            turn.error = Some(msg);
        }
        Ok(Err(e)) => {
            // Stream failed (network error, bad status, etc.) — surface it.
            let msg = format!("{e:#}");
            if turn.error.is_none() {
                let _ = tx.send(StreamToken::Error(msg.clone())).await;
            }
            turn.error = Some(msg);
        }
        Ok(Ok(())) => {}
    }

    turn
}
