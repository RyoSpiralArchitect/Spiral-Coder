use super::*;

/// Control-flow outcome of a dispatched tool block. The caller maps this onto
/// the `for iter` loop's `continue`/`break` so extracted blocks keep the same
/// semantics they had inline.
pub(super) enum ToolFlow {
    Continue,
    Break,
}

/// Shared state bundle for the extracted tool dispatch blocks
/// (apply_diff / list_dir / glob / search_files). All fields are borrowed
/// from the run_agentic_json loop so the blocks read/write the same locals.
pub(super) struct ToolDispatchCtx<'a> {
    pub(super) tx: &'a mpsc::Sender<StreamToken>,
    pub(super) autosaver: &'a Option<Arc<crate::agent_session::SessionAutoSaver>>,
    pub(super) approver: &'a dyn Approver,
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
    pub(super) mem: &'a mut FailureMemory,
    pub(super) file_cache: &'a mut std::collections::HashMap<String, String>,
    pub(super) edit_failures: &'a mut EditFailureMemory,
    pub(super) file_tool_consec_failures: &'a mut usize,
    pub(super) last_mutation_step: &'a mut Option<usize>,
    pub(super) last_build_verify_ok_step: &'a mut Option<usize>,
    pub(super) last_behavioral_verify_ok_step: &'a mut Option<usize>,
    pub(super) last_verify_ok_step: &'a mut Option<usize>,
    pub(super) last_reflection: &'a Option<ReflectionBlock>,
    pub(super) reflection_required: &'a mut Option<String>,
    pub(super) reflection_trigger_sig: &'a mut Option<ActionIdentity>,
    pub(super) impact_required: &'a mut Option<String>,
    pub(super) intent_required_verification: &'a mut VerificationLevel,
    pub(super) path_required_verification: &'a mut VerificationLevel,
    pub(super) required_verification: &'a mut VerificationLevel,
    pub(super) exec_verification_context: &'a exec_verification::ExecVerificationContext<'a>,
    pub(super) root_read_only: bool,
    pub(super) task_harness: TaskHarness,
    pub(super) active_plan: &'a Option<PlanBlock>,
    pub(super) iter: usize,
    pub(super) max_iters: usize,
    pub(super) this_step: usize,
    pub(super) root_user_text: &'a str,
    pub(super) test_cmd: Option<&'a str>,
    pub(super) step_label_for_turn: Option<&'a str>,
    pub(super) next_action_for_turn: Option<&'a str>,
}

impl ToolDispatchCtx<'_> {
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
                *self.file_tool_consec_failures,
                *self.last_mutation_step,
                *self.last_verify_ok_step,
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
                    verification_requirement_hint(*self.required_verification, self.test_cmd)
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

pub(super) async fn run_apply_diff(
    ctx: &mut ToolDispatchCtx<'_>,
    tc: &ToolCallData,
) -> Result<ToolFlow> {
    let args: serde_json::Value = serde_json::from_str(&tc.arguments)?;
    let path = args["path"].as_str().unwrap_or("").to_string();
    let diff = args["diff"].as_str().unwrap_or("").to_string();

    let _ = ctx
        .tx
        .send(StreamToken::Delta(format!("\n\n[APPLY_DIFF] {path}\n")))
        .await;
    let _ = ctx.tx.send(StreamToken::ToolCall(tc.clone())).await;

    let base = ctx.tool_root_abs.as_deref();
    let mut rejected_by_user = false;

    // B — capture old content for diff anchoring
    let old_for_cache = base.and_then(|b| {
        crate::file_tools::resolve_safe_path(&path, Some(b))
            .ok()
            .and_then(|abs| std::fs::read_to_string(&abs).ok())
    });

    let preview = truncate_preview(&diff, 2800, 140);
    let approval = ctx
        .approver
        .approve(ApprovalRequest::Edit {
            action: "apply_diff".to_string(),
            path: path.clone(),
            preview,
        })
        .await?;
    let mut cargo_inputs = None;
    let (mut result, is_error) = if approval == ApprovalOutcome::Approved {
        crate::file_tools::tool_apply_diff(&path, &diff, base)
    } else {
        rejected_by_user = true;
        (
            format!("REJECTED BY USER\naction: apply_diff\npath: {path}\n(no changes applied)"),
            true,
        )
    };

    // Invalidate file cache on success + auto-test
    let changed = crate::execution_evidence::file_edit_succeeded("apply_diff", &result);
    if changed {
        let cache_key = crate::file_tools::resolve_safe_path(&path, base)
            .ok()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.clone());
        ctx.file_cache.remove(&cache_key);
        let before_hash = old_for_cache.as_deref().map(hash_text);
        // Seed cache with new content + hash verification
        if let Ok(abs) = crate::file_tools::resolve_safe_path(&path, base) {
            if let Ok(new_content) = std::fs::read_to_string(&abs) {
                let after_hash = hash_text(&new_content);
                match before_hash {
                    Some(bh) => result.push_str(&format!(
                        "\n[hash] before={} after={}",
                        fmt_hash(bh),
                        fmt_hash(after_hash)
                    )),
                    None => result.push_str(&format!("\n[hash] after={}", fmt_hash(after_hash))),
                }
                ctx.file_cache.insert(cache_key, new_content);
            }
        }
        // A — auto-run tests
        if let Some(cmd) = ctx.test_cmd {
            if let Some(ref root) = ctx.tool_root_abs {
                let test_out =
                    automatic_test::observed(cmd, root, ctx.messages, &mut cargo_inputs).await;
                result.push_str(&test_out);
            }
        }
    }

    ctx.recovery.focus.record_result(
        tc,
        &result,
        ctx.tool_root_abs.as_deref(),
        ctx.test_cmd,
        ctx.exec_verification_context,
    );
    let automatic_test = crate::execution_evidence::auto_test_outcome("apply_diff", &result);
    let verified =
        !is_error && automatic_test == crate::execution_evidence::AutoTestOutcome::Passed;
    let verified_level = if verified {
        configured_test_cmd_verification_level(ctx.test_cmd)
    } else {
        None
    };
    if is_error {
        ctx.recovery.on_fix_result(false, None);
    } else if changed {
        exec_proof::record_file_result(
            exec_proof::FileProofResult {
                name: "apply_diff",
                content: &result,
                test_cmd: ctx.test_cmd,
            },
            ctx.this_step,
            ctx.last_mutation_step,
            ctx.last_build_verify_ok_step,
            ctx.last_behavioral_verify_ok_step,
        );
        *ctx.last_verify_ok_step = effective_verify_ok_step(
            *ctx.required_verification,
            *ctx.last_build_verify_ok_step,
            *ctx.last_behavioral_verify_ok_step,
        );
        ctx.mem.on_file_result(&tc.name, &result, ctx.test_cmd);
        ctx.recovery
            .on_successful_edit(automatic_test, verified_level);
    }

    ctx.edit_failures
        .on_result(tc, &result, ctx.exec_verification_context);
    *ctx.file_tool_consec_failures = ctx.edit_failures.count();
    let first_line = result.lines().next().unwrap_or("").to_string();
    if is_error {
        let _ = ctx
            .tx
            .send(StreamToken::Delta(format!(
                "[RESULT_FILE_ERR] {first_line}\n"
            )))
            .await;
        *ctx.state = AgentState::Recovery;
        if !rejected_by_user {
            *ctx.pending_system_hint = Some(format!("apply_diff error: {first_line}"));
            if let Some(reason) = ctx.edit_failures.reflection_reason() {
                *ctx.reflection_required = Some(reason);
            }
        } else {
            *ctx.pending_system_hint = Some(
                "The user rejected the edit. Choose a safer alternative or ask again with a smaller change."
                    .to_string(),
            );
        }
    } else {
        let _ = ctx
            .tx
            .send(StreamToken::Delta(format!("[RESULT_FILE] {first_line}\n")))
            .await;
        *ctx.state = if ctx.recovery.in_recovery() {
            AgentState::Recovery
        } else {
            AgentState::Planning
        };
        *ctx.pending_system_hint =
            if automatic_test == crate::execution_evidence::AutoTestOutcome::Failed {
                Some(recovery::AUTO_TEST_FAILURE_HINT.to_string())
            } else if ctx.recovery.stage == Some(RecoveryStage::Verify) {
                Some(format!(
                    "Recovery stage=verify: {}",
                    verification_requirement_hint(*ctx.required_verification, ctx.test_cmd)
                ))
            } else {
                None
            };
        if changed {
            update_working_memory_after_non_exec(
                ctx.working_mem,
                "apply_diff",
                verified,
                ctx.step_label_for_turn,
                ctx.next_action_for_turn,
                ctx.test_cmd,
            );
            ctx.assumption_ledger.refresh_confirmations(ctx.working_mem);
            let path_level = verification_level_for_mutation_path(path.as_str());
            if path_level > *ctx.path_required_verification {
                *ctx.path_required_verification = path_level;
                *ctx.required_verification =
                    (*ctx.intent_required_verification).max(*ctx.path_required_verification);
                ctx.recovery.required_verification = *ctx.required_verification;
                *ctx.last_verify_ok_step = effective_verify_ok_step(
                    *ctx.required_verification,
                    *ctx.last_build_verify_ok_step,
                    *ctx.last_behavioral_verify_ok_step,
                );
                if ctx.recovery.stage == Some(RecoveryStage::Verify) {
                    *ctx.pending_system_hint = Some(format!(
                        "Recovery stage=verify: {}",
                        verification_requirement_hint(*ctx.required_verification, ctx.test_cmd)
                    ));
                }
            }
            *ctx.impact_required = Some(format!(
                "successful mutation via apply_diff: {}",
                compact_one_line(path.as_str(), 80)
            ));
        }
    }

    if is_error {
        *ctx.reflection_required = Some(
            ctx.pending_system_hint
                .clone()
                .unwrap_or_else(|| "apply_diff failed".to_string()),
        );
        *ctx.reflection_trigger_sig = tool_call_action_sig(tc);
    }

    let history_result = if is_error {
        result.clone()
    } else {
        compact_success_tool_result_for_history("apply_diff", &result)
    };
    ctx.messages.push(CargoInputReceipt::attach(
        cargo_inputs,
        ctx.recovery.focus.tool_message(tc, history_result),
    ));
    if changed && !ctx.root_read_only {
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

pub(super) async fn run_list_dir(
    ctx: &mut ToolDispatchCtx<'_>,
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

pub(super) async fn run_glob(ctx: &mut ToolDispatchCtx<'_>, tc: &ToolCallData) -> Result<ToolFlow> {
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
    ctx: &mut ToolDispatchCtx<'_>,
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
