use super::*;

pub(crate) async fn autosave_best_effort(
    autosaver: &Option<Arc<crate::agent_session::SessionAutoSaver>>,
    tx: &mpsc::Sender<StreamToken>,
    tool_root_abs: Option<&str>,
    checkpoint: Option<&str>,
    cur_cwd: Option<&str>,
    messages: &[serde_json::Value],
) {
    let Some(ref saver) = autosaver else {
        return;
    };
    let Some(warn) = saver.save_best_effort(tool_root_abs, checkpoint, cur_cwd, messages) else {
        return;
    };
    let _ = tx
        .send(StreamToken::Delta(format!("\n[autosave] WARN: {warn}\n")))
        .await;
}

pub(crate) fn sync_observation_cache_autosave(
    autosaver: &Option<Arc<crate::agent_session::SessionAutoSaver>>,
    evidence: &ObservationEvidence,
) {
    if let Some(saver) = autosaver {
        saver.set_observation_cache(Some(evidence.to_session_cache()));
    }
}

pub(crate) fn sync_progress_context_autosave(
    autosaver: &Option<Arc<crate::agent_session::SessionAutoSaver>>,
    context: crate::progress_state::ProgressSaveContext,
) {
    if let Some(saver) = autosaver {
        saver.set_progress_context(Some(context));
    }
}

pub(crate) async fn emit_telemetry_event(
    tx: &mpsc::Sender<StreamToken>,
    event: &str,
    data: serde_json::Value,
) {
    let _ = tx
        .send(StreamToken::Telemetry(TelemetryEvent {
            event: event.to_string(),
            data,
        }))
        .await;
}

pub(crate) async fn emit_repo_map_fallback_telemetry(
    tx: &mpsc::Sender<StreamToken>,
    tool_name: &str,
    query: &str,
    fallback: &crate::repo_map::RepoMapFallback,
) {
    emit_telemetry_event(
        tx,
        "repo_map_fallback",
        json!({
            "tool": tool_name,
            "query": query,
            "top_path": fallback.top_path,
            "top_dir": fallback.top_dir,
            "top_confidence": fallback.top_confidence,
            "typo_likely": fallback.typo_likely,
            "reasons": fallback.top_path_reasons,
        }),
    )
    .await;
}

pub(crate) async fn emit_resolution_memory_hit_telemetry(
    tx: &mpsc::Sender<StreamToken>,
    tool_name: &str,
    query: &str,
    canonical_path: &str,
) {
    emit_telemetry_event(
        tx,
        "resolution_memory_hit",
        json!({
            "tool": tool_name,
            "query": query,
            "canonical_path": canonical_path,
        }),
    )
    .await;
}

pub(crate) fn reflection_ledger_action_match<'a>(
    ledger: &'a crate::reflection_ledger::ReflectionLedger,
    tc: &ToolCallData,
) -> Option<(
    &'a crate::reflection_ledger::ReflectionLedgerEntry,
    f32,
    String,
)> {
    let action = canonicalize_tool_call_command(tc.name.as_str(), tc.arguments.as_str())
        .unwrap_or_else(|| blocked_tool_call_signature(tc.name.as_str(), tc.arguments.as_str()));
    let action_sig = normalize_for_signature(action.as_str());
    let action_tokens = keyword_tokens(action.as_str());
    let tool_name = tc.name.to_ascii_lowercase();
    let args: serde_json::Value = serde_json::from_str(&tc.arguments).unwrap_or_else(|_| json!({}));
    let primary_target = args
        .get("path")
        .or_else(|| args.get("dir"))
        .or_else(|| args.get("command"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let target_sig = normalize_for_signature(primary_target.as_str());

    let mut best: Option<(&crate::reflection_ledger::ReflectionLedgerEntry, f32)> = None;
    for entry in &ledger.entries {
        let next = entry.next_minimal_action.as_str();
        let next_sig = normalize_for_signature(next);
        let next_tokens = keyword_tokens(next);
        let mut score = token_overlap_score(&next_tokens, &action_tokens);
        if !tool_name.is_empty() && next_sig.contains(tool_name.as_str()) {
            score += 0.10;
        }
        if !target_sig.is_empty() && next_sig.contains(target_sig.as_str()) {
            score += 0.35;
        }
        if next_sig.contains(action_sig.as_str()) || action_sig.contains(next_sig.as_str()) {
            score += 0.25;
        }
        score = score.clamp(0.0, 1.0);
        if score >= 0.45 {
            match best {
                Some((_, best_score)) if best_score >= score => {}
                _ => best = Some((entry, score)),
            }
        }
    }
    best.map(|(entry, score)| (entry, score, action))
}

pub(crate) fn push_blocked_tool_exchange(
    messages: &mut Vec<serde_json::Value>,
    assistant_text: &str,
    tc: &ToolCallData,
    block: &str,
) {
    messages.push(json!({
        "role": "assistant",
        "content": assistant_text,
        "tool_calls": [tc.to_json()]
    }));
    messages.push(json!({
        "role": "tool",
        "tool_call_id": tc.id,
        "content": format!(
            "GOVERNOR BLOCKED\n\n{block}\n\ntool:\n{}\narguments:\n{}",
            tc.name, tc.arguments
        ),
    }));
}
