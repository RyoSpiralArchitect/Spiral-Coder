use super::*;

/// Control-flow outcome of a dispatched tool block. The caller maps this onto
/// the `for iter` loop's `continue`/`break` so extracted blocks keep the same
/// semantics they had inline.
pub(super) enum ToolFlow {
    Continue,
    Break,
}

/// Shared state bundle for the observation-tool dispatch blocks
/// (list_dir / glob / search_files). All fields are borrowed from the
/// run_agentic_json loop so the extracted blocks read/write the same locals.
pub(super) struct ObserveToolCtx<'a> {
    pub(super) tx: &'a mpsc::Sender<StreamToken>,
    pub(super) autosaver: &'a Option<Arc<crate::agent_session::SessionAutoSaver>>,
    pub(super) tool_root_abs: &'a Option<String>,
    pub(super) checkpoint: &'a Option<String>,
    pub(super) cur_cwd: &'a Option<String>,
    pub(super) messages: &'a mut Vec<serde_json::Value>,
    pub(super) observation_evidence: &'a mut ObservationEvidence,
    pub(super) recovery: &'a mut RecoveryGovernor,
    pub(super) pending_system_hint: &'a mut Option<String>,
    pub(super) state: &'a mut AgentState,
    pub(super) working_mem: &'a mut WorkingMemory,
    pub(super) assumption_ledger: &'a mut AssumptionLedger,
    pub(super) task_outcome: &'a mut outcome::TaskOutcome,
    pub(super) mem: &'a FailureMemory,
    pub(super) file_tool_consec_failures: usize,
    pub(super) last_mutation_step: Option<usize>,
    pub(super) last_verify_ok_step: Option<usize>,
    pub(super) last_reflection: &'a Option<ReflectionBlock>,
    pub(super) root_read_only: bool,
    pub(super) task_harness: TaskHarness,
    pub(super) active_plan: &'a Option<PlanBlock>,
    pub(super) iter: usize,
    pub(super) max_iters: usize,
    pub(super) root_user_text: &'a str,
    pub(super) required_verification: VerificationLevel,
    pub(super) test_cmd: Option<&'a str>,
    pub(super) step_label_for_turn: Option<&'a str>,
    pub(super) next_action_for_turn: Option<&'a str>,
}

impl ObserveToolCtx<'_> {
    async fn autosave(&self) {
        autosave_best_effort(
            self.autosaver,
            self.tx,
            self.tool_root_abs.as_deref(),
            self.checkpoint.as_deref(),
            self.cur_cwd.as_deref(),
            self.messages,
        )
        .await;
    }

    async fn emit_governor_state(&self) {
        let _ = self
            .tx
            .send(StreamToken::GovernorState(build_governor_state(
                *self.state,
                self.recovery,
                self.mem,
                self.file_tool_consec_failures,
                self.last_mutation_step,
                self.last_verify_ok_step,
                self.last_reflection.as_ref(),
            )))
            .await;
    }

    /// Post-result recovery/planning state transition and stage hint.
    fn apply_diagnostic_transition(&mut self, is_error: bool, repo_map_hint: Option<String>) {
        if is_error {
            *self.state = AgentState::Recovery;
        } else {
            *self.state = if self.recovery.in_recovery() {
                AgentState::Recovery
            } else {
                AgentState::Planning
            };
            *self.pending_system_hint = if self.recovery.stage == Some(RecoveryStage::Fix) {
                Some("Recovery stage=fix: apply a minimal fix now (edit files or run a corrected command).".to_string())
            } else if self.recovery.stage == Some(RecoveryStage::Verify) {
                Some(format!(
                    "Recovery stage=verify: {}",
                    verification_requirement_hint(self.required_verification, self.test_cmd)
                ))
            } else if repo_map_hint.is_some() {
                repo_map_hint
            } else {
                None
            };
        }
    }

    /// Read-only iteration-cap auto-finalization. Returns Break when the turn
    /// auto-finalized, Continue when the caller should emit the provided hint.
    async fn read_only_iter_cap_finalize(&mut self, plan: &PlanBlock) -> Result<ToolFlow> {
        if let Some(final_text) = build_read_only_iteration_cap_final_answer(
            self.root_user_text,
            plan,
            self.observation_evidence,
            self.messages,
            self.working_mem,
        ) {
            self.task_outcome.finalized(&final_text);
            *self.state = AgentState::Done;
            self.messages
                .push(json!({"role": "assistant", "content": final_text.clone()}));
            self.autosave().await;
            self.emit_governor_state().await;
            let _ = self
                .tx
                .send(StreamToken::Delta(format!(
                    "\n[agent] iteration cap reached; auto-finalized read-only inspection.\n\n{final_text}\n"
                )))
                .await;
            return Ok(ToolFlow::Break);
        }
        Ok(ToolFlow::Continue)
    }
}

pub(super) async fn run_list_dir(
    ctx: &mut ObserveToolCtx<'_>,
    tc: &ToolCallData,
) -> Result<ToolFlow> {
    let args: serde_json::Value = serde_json::from_str(&tc.arguments)?;
    let dir = args["dir"].as_str().unwrap_or("").to_string();
    let max_entries = args["max_entries"].as_u64().unwrap_or(200) as usize;
    let include_hidden = args["include_hidden"].as_bool().unwrap_or(false);

    let dir_label = if dir.trim().is_empty() {
        "."
    } else {
        dir.as_str()
    };
    let _ = ctx
        .tx
        .send(StreamToken::Delta(format!("\n\n[LIST_DIR] {dir_label}\n")))
        .await;
    let _ = ctx.tx.send(StreamToken::ToolCall(tc.clone())).await;

    let base = ctx.tool_root_abs.as_deref();
    let (mut result, is_error) =
        crate::file_tools::tool_list_dir(&dir, max_entries, include_hidden, base);
    let mut repo_map_hint: Option<String> = None;
    let mut repo_map_display: Option<String> = None;
    if is_error && !dir.trim().is_empty() {
        if let Some(root) = ctx.tool_root_abs.as_deref() {
            if let Some(fallback) = crate::repo_map::lazy_list_dir_fallback(root, &dir) {
                emit_repo_map_fallback_telemetry(ctx.tx, "list_dir", &dir, &fallback).await;
                remember_repo_map_resolution(
                    ctx.observation_evidence,
                    dir.as_str(),
                    &fallback,
                    "repo_map:list_dir",
                );
                sync_observation_cache_autosave(ctx.autosaver, ctx.observation_evidence);
                result.push_str("\n");
                result.push_str(&fallback.content);
                repo_map_display = Some(build_repo_map_display_banner("list_dir", &fallback));
                repo_map_hint = Some(build_repo_map_list_dir_hint(&dir, &fallback));
            }
        }
    }
    ctx.recovery.on_diagnostic_result(!is_error);

    let first_line = result.lines().next().unwrap_or("").to_string();
    if is_error {
        let _ = ctx
            .tx
            .send(StreamToken::Delta(format!(
                "[RESULT_FILE_ERR] {first_line}\n"
            )))
            .await;
        if let Some(display) = repo_map_display {
            let _ = ctx.tx.send(StreamToken::Delta(display)).await;
        }
        *ctx.pending_system_hint = repo_map_hint;
        *ctx.state = AgentState::Recovery;
    } else {
        let _ = ctx
            .tx
            .send(StreamToken::Delta(format!(
                "[RESULT_LIST_DIR] {first_line}\n"
            )))
            .await;
        ctx.apply_diagnostic_transition(false, repo_map_hint);
        update_working_memory_after_non_exec(
            ctx.working_mem,
            "list_dir",
            false,
            ctx.step_label_for_turn,
            ctx.next_action_for_turn,
            ctx.test_cmd,
        );
        ctx.assumption_ledger.refresh_confirmations(ctx.working_mem);
    }

    let history_result = if is_error {
        result.clone()
    } else {
        compact_success_tool_result_for_history("list_dir", &result)
    };
    ctx.messages.push(json!({
        "role": "tool",
        "tool_call_id": tc.id,
        "content": history_result,
    }));
    if !is_error && !ctx.root_read_only {
        if let Some(hint) = build_fix_stage_progress_hint(
            ctx.task_harness,
            ctx.messages,
            ctx.recovery.stage,
            ctx.test_cmd,
        ) {
            *ctx.pending_system_hint = Some(hint);
        }
    }
    if !is_error && ctx.root_read_only {
        if let Some(plan) = ctx.active_plan.as_ref() {
            if ctx.iter + 1 == ctx.max_iters {
                if let ToolFlow::Break = ctx.read_only_iter_cap_finalize(plan).await? {
                    return Ok(ToolFlow::Break);
                }
            } else if let Some(hint) = build_read_only_completion_hint(
                ctx.root_user_text,
                plan,
                ctx.observation_evidence,
                ctx.messages,
                ctx.working_mem,
            ) {
                *ctx.pending_system_hint = Some(hint);
            }
        }
    }
    ctx.autosave().await;
    ctx.emit_governor_state().await;
    Ok(ToolFlow::Continue)
}

pub(super) async fn run_glob(ctx: &mut ObserveToolCtx<'_>, tc: &ToolCallData) -> Result<ToolFlow> {
    let args: serde_json::Value = serde_json::from_str(&tc.arguments)?;
    let pattern = args["pattern"].as_str().unwrap_or("").to_string();
    let dir = args["dir"].as_str().unwrap_or("").to_string();

    let _ = ctx
        .tx
        .send(StreamToken::Delta(format!("\n\n[GLOB] {pattern}\n")))
        .await;
    let _ = ctx.tx.send(StreamToken::ToolCall(tc.clone())).await;

    let base = ctx.tool_root_abs.as_deref();
    let (mut result, is_error) = crate::file_tools::tool_glob_files(&pattern, &dir, base);
    let mut repo_map_hint: Option<String> = None;
    let mut repo_map_display: Option<String> = None;
    if !is_error && result.starts_with("[glob] No files matching") {
        if let Some(root) = ctx.tool_root_abs.as_deref() {
            if let Some(fallback) = crate::repo_map::lazy_glob_fallback(root, &pattern) {
                emit_repo_map_fallback_telemetry(ctx.tx, "glob", &pattern, &fallback).await;
                remember_repo_map_resolution(
                    ctx.observation_evidence,
                    pattern.as_str(),
                    &fallback,
                    "repo_map:glob",
                );
                sync_observation_cache_autosave(ctx.autosaver, ctx.observation_evidence);
                result.push_str("\n");
                result.push_str(&fallback.content);
                repo_map_display = Some(build_repo_map_display_banner("glob", &fallback));
                repo_map_hint = Some(build_repo_map_glob_hint(&pattern, &fallback));
            }
        }
    }
    ctx.recovery.on_diagnostic_result(!is_error);

    let first_line = result.lines().next().unwrap_or("").to_string();
    if is_error {
        let _ = ctx
            .tx
            .send(StreamToken::Delta(format!(
                "[RESULT_FILE_ERR] {first_line}\n"
            )))
            .await;
        *ctx.state = AgentState::Recovery;
    } else {
        let _ = ctx
            .tx
            .send(StreamToken::Delta(format!("[RESULT_GLOB] {first_line}\n")))
            .await;
        if let Some(display) = repo_map_display {
            let _ = ctx.tx.send(StreamToken::Delta(display)).await;
        }
        ctx.apply_diagnostic_transition(false, repo_map_hint);
        update_working_memory_after_non_exec(
            ctx.working_mem,
            "glob",
            false,
            ctx.step_label_for_turn,
            ctx.next_action_for_turn,
            ctx.test_cmd,
        );
        ctx.assumption_ledger.refresh_confirmations(ctx.working_mem);
    }

    let history_result = if is_error {
        result.clone()
    } else {
        compact_success_tool_result_for_history("glob", &result)
    };
    ctx.messages.push(json!({
        "role": "tool",
        "tool_call_id": tc.id,
        "content": history_result,
    }));
    if !is_error && !ctx.root_read_only {
        if let Some(hint) = build_fix_stage_progress_hint(
            ctx.task_harness,
            ctx.messages,
            ctx.recovery.stage,
            ctx.test_cmd,
        ) {
            *ctx.pending_system_hint = Some(hint);
        }
    }
    if !is_error && ctx.root_read_only {
        if let Some(plan) = ctx.active_plan.as_ref() {
            if ctx.iter + 1 == ctx.max_iters {
                if let ToolFlow::Break = ctx.read_only_iter_cap_finalize(plan).await? {
                    return Ok(ToolFlow::Break);
                }
            } else if let Some(hint) = build_read_only_completion_hint(
                ctx.root_user_text,
                plan,
                ctx.observation_evidence,
                ctx.messages,
                ctx.working_mem,
            ) {
                *ctx.pending_system_hint = Some(hint);
            }
        }
    }
    ctx.autosave().await;
    ctx.emit_governor_state().await;
    Ok(ToolFlow::Continue)
}

pub(super) async fn run_search_files(
    ctx: &mut ObserveToolCtx<'_>,
    tc: &ToolCallData,
) -> Result<ToolFlow> {
    let args: serde_json::Value = serde_json::from_str(&tc.arguments)?;
    let pattern = args["pattern"].as_str().unwrap_or("").to_string();
    let dir = args["dir"].as_str().unwrap_or("").to_string();
    let ci = args["case_insensitive"].as_bool().unwrap_or(false);

    let _ = ctx
        .tx
        .send(StreamToken::Delta(format!(
            "\n\n[SEARCH_FILES] {pattern}\n"
        )))
        .await;
    let _ = ctx.tx.send(StreamToken::ToolCall(tc.clone())).await;

    let base = ctx.tool_root_abs.as_deref();
    let (mut result, is_error) = crate::file_tools::tool_search_files(&pattern, &dir, ci, base);
    let parsed_search_paths = if is_error {
        Vec::new()
    } else {
        parse_search_result_paths(result.as_str())
    };
    let mut repo_map_hint: Option<String> = None;
    let mut repo_map_display: Option<String> = None;
    if !is_error && result.starts_with("[search_files] No matches for") && dir.trim().is_empty() {
        if let Some(root) = ctx.tool_root_abs.as_deref() {
            if let Some(fallback) = crate::repo_map::lazy_search_fallback(root, &pattern) {
                emit_repo_map_fallback_telemetry(ctx.tx, "search_files", &pattern, &fallback).await;
                result.push_str("\n");
                result.push_str(&fallback.content);
                repo_map_display = Some(build_repo_map_display_banner("search_files", &fallback));
                repo_map_hint = Some(build_repo_map_search_hint(&pattern, &fallback));
            }
        }
    }
    ctx.recovery.on_diagnostic_result(!is_error);

    let first_line = result.lines().next().unwrap_or("").to_string();
    if is_error {
        let _ = ctx
            .tx
            .send(StreamToken::Delta(format!(
                "[RESULT_FILE_ERR] {first_line}\n"
            )))
            .await;
        *ctx.state = AgentState::Recovery;
    } else {
        let _ = ctx
            .tx
            .send(StreamToken::Delta(format!(
                "[RESULT_SEARCH] {first_line}\n"
            )))
            .await;
        if let Some(display) = repo_map_display {
            let _ = ctx.tx.send(StreamToken::Delta(display)).await;
        }
        ctx.apply_diagnostic_transition(false, repo_map_hint);
        update_working_memory_after_non_exec(
            ctx.working_mem,
            "search_files",
            false,
            ctx.step_label_for_turn,
            ctx.next_action_for_turn,
            ctx.test_cmd,
        );
        let command = canonicalize_tool_call_command("search_files", tc.arguments.as_str())
            .unwrap_or_else(|| format!("search_files(pattern={pattern}, dir={dir})"));
        ctx.observation_evidence.remember_search(
            command.as_str(),
            pattern.as_str(),
            parse_search_hit_count(result.as_str()),
            parsed_search_paths.as_slice(),
        );
        sync_observation_cache_autosave(ctx.autosaver, ctx.observation_evidence);
        ctx.assumption_ledger.refresh_confirmations(ctx.working_mem);
    }

    let history_result = if is_error {
        result.clone()
    } else {
        compact_success_tool_result_for_history("search_files", &result)
    };
    ctx.messages.push(json!({
        "role": "tool",
        "tool_call_id": tc.id,
        "content": history_result,
    }));
    if !is_error && ctx.root_read_only {
        if let Some(plan) = ctx.active_plan.as_ref() {
            if ctx.iter + 1 == ctx.max_iters {
                if let ToolFlow::Break = ctx.read_only_iter_cap_finalize(plan).await? {
                    return Ok(ToolFlow::Break);
                }
            } else if let Some(hint) = build_read_only_search_to_read_hint(
                ctx.root_user_text,
                plan,
                parsed_search_paths.as_slice(),
                ctx.observation_evidence,
            ) {
                *ctx.pending_system_hint = Some(hint);
            } else if let Some(hint) = build_read_only_completion_hint(
                ctx.root_user_text,
                plan,
                ctx.observation_evidence,
                ctx.messages,
                ctx.working_mem,
            ) {
                *ctx.pending_system_hint = Some(hint);
            }
        }
    }
    ctx.autosave().await;
    Ok(ToolFlow::Continue)
}
