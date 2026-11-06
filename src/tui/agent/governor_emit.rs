use super::*;

/// Borrowed loop state needed to emit one GOVERNOR BLOCK sequence: stream the
/// block to the UI, record the blocked exchange, autosave, queue the hint, and
/// publish the updated GovernorState. The caller keeps control flow (`continue`,
/// state mutation, and any extra tool messages stay at the callsite).
pub(crate) struct GovernorEmit<'a> {
    pub(crate) tx: &'a mpsc::Sender<StreamToken>,
    pub(crate) messages: &'a mut Vec<serde_json::Value>,
    pub(crate) autosaver: &'a Option<Arc<crate::agent_session::SessionAutoSaver>>,
    pub(crate) tool_root_abs: &'a Option<String>,
    pub(crate) checkpoint: &'a Option<String>,
    pub(crate) cur_cwd: &'a Option<String>,
    pub(crate) pending_system_hint: &'a mut Option<String>,
    pub(crate) state: AgentState,
    pub(crate) recovery: &'a RecoveryGovernor,
    pub(crate) mem: &'a FailureMemory,
    pub(crate) file_tool_consec_failures: usize,
    pub(crate) last_mutation_step: Option<usize>,
    pub(crate) last_verify_ok_step: Option<usize>,
    pub(crate) last_reflection: Option<&'a ReflectionBlock>,
}

pub(crate) async fn emit_governor_block(
    emit: GovernorEmit<'_>,
    block: String,
    tc: &ToolCallData,
    assistant_text_clean: Option<&str>,
) {
    let _ = emit
        .tx
        .send(StreamToken::Delta(format!(
            "[RESULT][Recovery] GOVERNOR BLOCK\n{block}\n"
        )))
        .await;
    if let Some(clean) = assistant_text_clean {
        push_blocked_tool_exchange(emit.messages, clean, tc, &block);
    } else {
        emit.messages.push(json!({
            "role": "tool",
            "tool_call_id": tc.id,
            "content": format!(
                "GOVERNOR BLOCKED\n\n{block}\n\ntool:\n{}\narguments:\n{}",
                tc.name, tc.arguments
            ),
        }));
    }
    autosave_best_effort(
        emit.autosaver,
        emit.tx,
        emit.tool_root_abs.as_deref(),
        emit.checkpoint.as_deref(),
        emit.cur_cwd.as_deref(),
        emit.messages,
    )
    .await;
    *emit.pending_system_hint = Some(block);
    let _ = emit
        .tx
        .send(StreamToken::GovernorState(build_governor_state(
            emit.state,
            emit.recovery,
            emit.mem,
            emit.file_tool_consec_failures,
            emit.last_mutation_step,
            emit.last_verify_ok_step,
            emit.last_reflection,
        )))
        .await;
}
