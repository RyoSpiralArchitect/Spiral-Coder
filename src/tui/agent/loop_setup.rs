use super::*;

/// Persisted state loaded during loop setup plus the checkpoint handle and the
/// already-mutated message history (context notes are injected in place).
pub(crate) struct SetupPersisted {
    pub(crate) reflection_ledger: crate::reflection_ledger::ReflectionLedger,
    pub(crate) progress_state: Option<crate::progress_state::RepoProgressState>,
    pub(crate) harness_promoted_overlay: harness_evolution::GovernorContractOverlay,
    pub(crate) harness_evolution_queue: harness_evolution::HarnessEvolutionQueue,
    pub(crate) harness_evolution_path: Option<std::path::PathBuf>,
    pub(crate) reflection_ledger_path: Option<std::path::PathBuf>,
}

pub(crate) fn has_system_prefix(messages: &[serde_json::Value], prefix: &str) -> bool {
    messages.iter().any(|m| {
        m.get("role").and_then(|r| r.as_str()) == Some("system")
            && m.get("content")
                .and_then(|c| c.as_str())
                .map(|s| s.trim_start().starts_with(prefix))
                .unwrap_or(false)
    })
}

pub(crate) async fn load_persisted_state(
    tx: &mpsc::Sender<StreamToken>,
    tool_root_abs: &Option<String>,
) -> SetupPersisted {
    let reflection_ledger_path = tool_root_abs
        .as_deref()
        .map(crate::reflection_ledger::path_for_root);
    let reflection_ledger = if let Some(path) = reflection_ledger_path.as_ref() {
        match crate::reflection_ledger::ReflectionLedger::load(path) {
            Ok(ledger) => ledger,
            Err(e) => {
                let _ = tx
                    .send(StreamToken::Delta(format!(
                        "\n[reflection_ledger] WARN: {e:#}\n"
                    )))
                    .await;
                crate::reflection_ledger::ReflectionLedger::default()
            }
        }
    } else {
        crate::reflection_ledger::ReflectionLedger::default()
    };
    emit_telemetry_event(
        tx,
        "reflection_ledger_loaded",
        json!({
            "entries": reflection_ledger.entries.len(),
            "path": reflection_ledger_path.as_ref().map(|p| p.to_string_lossy().to_string()),
        }),
    )
    .await;
    let progress_state_path = tool_root_abs
        .as_deref()
        .map(crate::progress_state::path_for_root);
    let progress_state = if let Some(path) = progress_state_path.as_ref() {
        match crate::progress_state::RepoProgressState::load(path) {
            Ok(progress) => Some(progress),
            Err(_) if !path.exists() => None,
            Err(e) => {
                let _ = tx
                    .send(StreamToken::Delta(format!(
                        "\n[progress_state] WARN: {e:#}\n"
                    )))
                    .await;
                None
            }
        }
    } else {
        None
    };
    emit_telemetry_event(
        tx,
        "progress_state_loaded",
        json!({
            "path": progress_state_path.as_ref().map(|p| p.to_string_lossy().to_string()),
            "has_state": progress_state.is_some(),
            "completed_artifacts": progress_state.as_ref().map(|state| state.completed_artifacts.len()).unwrap_or(0),
            "verified_commands": progress_state.as_ref().map(|state| state.verified_commands.len()).unwrap_or(0),
        }),
    )
    .await;
    let harness_promoted_overlay_path = tool_root_abs
        .as_deref()
        .map(super::harness_evolution::overlay_path_for_root);
    let harness_promoted_overlay = if let Some(path) = harness_promoted_overlay_path.as_ref() {
        match harness_evolution::GovernorContractOverlay::load(path) {
            Ok(overlay) => overlay,
            Err(e) => {
                let _ = tx
                    .send(StreamToken::Delta(format!(
                        "\n[harness_evolution_promoted] WARN: {e:#}\n"
                    )))
                    .await;
                harness_evolution::GovernorContractOverlay::default()
            }
        }
    } else {
        harness_evolution::GovernorContractOverlay::default()
    };
    emit_telemetry_event(
        tx,
        "harness_evolution_promoted_loaded",
        json!({
            "policies": harness_promoted_overlay.promoted_policies.len(),
            "path": harness_promoted_overlay_path.as_ref().map(|p| p.to_string_lossy().to_string()),
        }),
    )
    .await;
    let harness_evolution_path = tool_root_abs
        .as_deref()
        .map(super::harness_evolution::path_for_root);
    let harness_evolution_queue = if let Some(path) = harness_evolution_path.as_ref() {
        match harness_evolution::HarnessEvolutionQueue::load(path) {
            Ok(queue) => queue,
            Err(e) => {
                let _ = tx
                    .send(StreamToken::Delta(format!(
                        "\n[harness_evolution] WARN: {e:#}\n"
                    )))
                    .await;
                harness_evolution::HarnessEvolutionQueue::default()
            }
        }
    } else {
        harness_evolution::HarnessEvolutionQueue::default()
    };
    emit_telemetry_event(
        tx,
        "harness_evolution_loaded",
        json!({
            "proposals": harness_evolution_queue.proposal_count(),
            "path": harness_evolution_path.as_ref().map(|p| p.to_string_lossy().to_string()),
        }),
    )
    .await;
    SetupPersisted {
        reflection_ledger,
        progress_state,
        harness_promoted_overlay,
        harness_evolution_queue,
        harness_evolution_path,
        reflection_ledger_path,
    }
}

/// Emits the git checkpoint token/notice and resolves the checkpoint to use.
pub(crate) async fn create_or_reemit_checkpoint(
    tx: &mpsc::Sender<StreamToken>,
    tool_root_abs: &Option<String>,
    checkpoint: &Option<String>,
    create_checkpoint: bool,
) -> Option<String> {
    let mut checkpoint = checkpoint.clone();
    if let Some(ref root) = tool_root_abs {
        if checkpoint.is_none() && create_checkpoint {
            if let Some(hash) = git_create_checkpoint(root).await {
                checkpoint = Some(hash.clone());
                let short = hash[..hash.len().min(8)].to_string();
                let _ = tx.send(StreamToken::Checkpoint(hash)).await;
                let _ = tx
                    .send(StreamToken::Delta(format!(
                        "[git checkpoint] {short} saved — use /rollback to restore\n\n"
                    )))
                    .await;
            }
        } else if let Some(ref hash) = checkpoint {
            let _ = tx.send(StreamToken::Checkpoint(hash.clone())).await;
        }
    }
    checkpoint
}

/// Injects the working-directory note, project context, and AGENTS.md/
/// .spiral-coder.md rules into the message history (once each).
pub(crate) fn inject_setup_context(
    messages: &mut Vec<serde_json::Value>,
    tool_root_abs: &Option<String>,
    project_context: Option<String>,
    agents_md: Option<String>,
) {
    if let Some(ref root) = tool_root_abs {
        let _ = std::fs::create_dir_all(root);
        let note = format!(
            "[Working directory]\n\
Working directory (tool_root): {root}\n\
IMPORTANT: Each exec runs in a fresh process; `cd` does NOT persist unless the tool reports cwd.\n\
 Always operate under tool_root. Create new repos under tool_root (fresh directory).\n\
 NEVER create a git repo inside another git repo. If you see 'embedded git repository', STOP and relocate."
        );
        if !has_system_prefix(messages, "[Working directory]") {
            if messages.first().and_then(|m| m["role"].as_str()) == Some("system") {
                messages.insert(1, json!({"role":"system","content": note}));
            } else {
                messages.insert(0, json!({"role":"system","content": note}));
            }
        }
    }
    if let Some(ctx_text) = project_context {
        if !ctx_text.is_empty() {
            if !has_system_prefix(messages, "[Project Context") {
                let pos = messages.len().min(2);
                messages.insert(pos, json!({"role":"system","content": ctx_text}));
            }
        }
    }
    if let Some(agents_text) = agents_md {
        if !agents_text.is_empty() {
            if !has_system_prefix(
                messages,
                "[Project Instructions — .spiral-coder.md / AGENTS.md]",
            ) {
                let pos = messages.len().min(3);
                messages.insert(pos, json!({
                    "role": "system",
                    "content": format!("[Project Instructions — .spiral-coder.md / AGENTS.md]\n{agents_text}")
                }));
            }
        }
    }
}
