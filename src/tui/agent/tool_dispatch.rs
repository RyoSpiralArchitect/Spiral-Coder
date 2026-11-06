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
    pub(super) cur_cwd: &'a mut Option<String>,
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
    pub(super) last_exec_step: &'a mut Option<usize>,
    pub(super) last_reflection: &'a Option<ReflectionBlock>,
    pub(super) reflection_guard: &'a Option<ReflectionBlock>,
    pub(super) reflection_required: &'a mut Option<String>,
    pub(super) reflection_trigger_sig: &'a mut Option<ActionIdentity>,
    pub(super) impact_required: &'a mut Option<String>,
    pub(super) intent_required_verification: &'a mut VerificationLevel,
    pub(super) path_required_verification: &'a mut VerificationLevel,
    pub(super) required_verification: &'a mut VerificationLevel,
    pub(super) exec_verification_context: &'a exec_verification::ExecVerificationContext<'a>,
    pub(super) cfg: &'a RunConfig,
    pub(super) realize_cfg: &'a RealizeOnDemandConfig,
    pub(super) latent_plan: &'a Option<LatentPlanBuffer>,
    pub(super) validated_plan_for_turn: &'a Option<PlanBlock>,
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

    /// Reborrowed view used by the shared GOVERNOR BLOCK emitter.
    fn governor_emit(&mut self) -> GovernorEmit<'_> {
        GovernorEmit {
            tx: self.tx,
            messages: self.messages,
            autosaver: self.autosaver,
            tool_root_abs: self.tool_root_abs,
            checkpoint: self.checkpoint,
            cur_cwd: self.cur_cwd,
            pending_system_hint: self.pending_system_hint,
            state: *self.state,
            recovery: self.recovery,
            mem: self.mem,
            file_tool_consec_failures: *self.file_tool_consec_failures,
            last_mutation_step: *self.last_mutation_step,
            last_verify_ok_step: *self.last_verify_ok_step,
            last_reflection: self.last_reflection.as_ref(),
        }
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

    /// Shared tail for read-only auto-finalization: mark done, append the
    /// answer, autosave, emit governor state, and surface the notice.
    async fn complete_done(&mut self, final_text: &str, notice: &str) {
        self.task_outcome.finalized(final_text);
        *self.state = AgentState::Done;
        self.messages
            .push(json!({"role": "assistant", "content": final_text.to_string()}));
        self.autosave().await;
        self.emit_governor_state().await;
        let _ = self
            .tx
            .send(StreamToken::Delta(format!("{notice}{final_text}\n")))
            .await;
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
            self.complete_done(
                &final_text,
                "\n[agent] iteration cap reached; auto-finalized read-only inspection.\n\n",
            )
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

pub(super) async fn run_file_tools(
    ctx: &mut ToolDispatchCtx<'_>,
    tc: &ToolCallData,
) -> Result<ToolFlow> {
    let args: serde_json::Value = serde_json::from_str(&tc.arguments)?;
    let path = args["path"].as_str().unwrap_or("").to_string();

    // Emit annotation (visible in TUI).
    let tool_upper = tc.name.to_ascii_uppercase();
    let _ = ctx
        .tx
        .send(StreamToken::Delta(format!("\n\n[{tool_upper}] {path}\n")))
        .await;
    let _ = ctx.tx.send(StreamToken::ToolCall(tc.clone())).await;

    let base = ctx.tool_root_abs.as_deref();
    let mut repo_map_hint: Option<String> = None;
    let mut repo_map_display: Option<String> = None;

    // Cache key: canonical absolute path string.
    let cache_key = crate::file_tools::resolve_safe_path(&path, base)
        .ok()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| format!("{}|{}", path, base.unwrap_or("")));
    let mut rejected_by_user = false;

    let mut cargo_inputs = None;
    let (result, is_error) = match tc.name.as_str() {
        "read_file" => {
            let ranged = args.get("start_line").is_some() || args.get("end_line").is_some();
            if assumption_gate::needs_fresh_file_read(ctx.assumption_ledger, &path)
                || (!ranged
                    && ctx.file_cache.contains_key(&cache_key)
                    && deferred_edit::confirming_read(ctx.messages, &tc.id, &path))
            {
                ctx.file_cache.remove(&cache_key);
            }
            // ── Gap 6: serve from cache if file hasn't changed ──────
            if ranged {
                let (content, err) = crate::file_tools::read_range::execute_args(&args, base);
                if !err && crate::file_tools::read_range::full_snapshot(&content) {
                    ctx.file_cache.insert(cache_key.clone(), content.clone());
                }
                (content, err)
            } else if let Some(result) =
                ctx.recovery
                    .focus
                    .read_for_diagnosis(tc, base, ctx.file_cache)
            {
                result
            } else if let Some(cached) = ctx
                .file_cache
                .get(&cache_key)
                .and_then(|cached| crate::file_tools::read_cache::for_request(cached, &path, base))
            {
                let header = cached.lines().next().unwrap_or(&path).to_string();
                let _ = ctx
                    .tx
                    .send(StreamToken::Delta(format!("[CACHE_HIT] {header}\n")))
                    .await;
                (cached, false)
            } else {
                let (mut content, err) = crate::file_tools::tool_read_file(&path, base);
                if err {
                    if let Some(root) = ctx.tool_root_abs.as_deref() {
                        if let Some(fallback) = crate::repo_map::lazy_read_fallback(root, &path) {
                            emit_repo_map_fallback_telemetry(ctx.tx, "read_file", &path, &fallback)
                                .await;
                            remember_repo_map_resolution(
                                ctx.observation_evidence,
                                path.as_str(),
                                &fallback,
                                "repo_map:read_file",
                            );
                            sync_observation_cache_autosave(
                                ctx.autosaver,
                                ctx.observation_evidence,
                            );
                            content.push_str("\n");
                            content.push_str(&fallback.content);
                            repo_map_display =
                                Some(build_repo_map_display_banner("read_file", &fallback));
                            repo_map_hint = Some(build_repo_map_read_hint(&path, &fallback));
                        }
                    }
                }
                if !err && crate::file_tools::read_range::full_snapshot(&content) {
                    ctx.file_cache.insert(cache_key.clone(), content.clone());
                }
                (content, err)
            }
        }
        "write_file" => {
            let content = args["content"].as_str().unwrap_or("").to_string();

            // Safety: do not overwrite an existing file unless we already read it in this session.
            // This prevents accidental destructive writes when the agent guessed the path/content.
            let file_exists = crate::file_tools::resolve_safe_path(&path, base)
                .ok()
                .and_then(|abs| abs.metadata().ok())
                .map(|m| m.is_file())
                .unwrap_or(false);
            if file_exists && !ctx.file_cache.contains_key(&cache_key) {
                if let Some(snapshot) = read_context::matching_snapshot(ctx.messages, &path, base) {
                    ctx.file_cache.insert(cache_key.clone(), snapshot);
                }
            }
            if file_exists && !ctx.file_cache.contains_key(&cache_key) {
                (
                    format!(
                        "GOVERNOR BLOCKED: write_file refused because '{path}' already exists.\n\
Action required: obtain a complete read of current contents before write_file. For a range or truncated read, use patch_file/apply_diff to preserve unseen content."
                    ),
                    true,
                )
            } else {
                // Approval: show a compact before/after preview.
                let old = ctx
                    .file_cache
                    .get(&cache_key)
                    .cloned()
                    .or_else(|| {
                        crate::file_tools::resolve_safe_path(&path, base)
                            .ok()
                            .and_then(|abs| std::fs::read_to_string(&abs).ok())
                    })
                    .unwrap_or_default();
                let preview = simple_before_after(&old, &content);
                let before_hash = hash_text(&old);
                let after_hash = hash_text(&content);
                let approval = ctx
                    .approver
                    .approve(ApprovalRequest::Edit {
                        action: "write_file".to_string(),
                        path: path.clone(),
                        preview,
                    })
                    .await?;
                if approval == ApprovalOutcome::Rejected {
                    rejected_by_user = true;
                    (
                        format!(
                            "REJECTED BY USER\naction: write_file\npath: {path}\n(no changes applied)"
                        ),
                        true,
                    )
                } else {
                    let (mut r_text, r_err) =
                        crate::cargo_inputs::write_file(&path, &content, base, ctx.messages);
                    if !r_err
                        && crate::execution_evidence::file_edit_succeeded("write_file", &r_text)
                    {
                        ctx.file_cache.insert(cache_key.clone(), content.clone());
                        r_text.push_str(&format!(
                            "\n[hash] before={} after={}",
                            fmt_hash(before_hash),
                            fmt_hash(after_hash)
                        ));
                        // A — auto-test after write
                        if let Some(cmd) = ctx.test_cmd {
                            if let Some(ref root) = ctx.tool_root_abs {
                                r_text.push_str(
                                    &automatic_test::observed(
                                        cmd,
                                        root,
                                        ctx.messages,
                                        &mut cargo_inputs,
                                    )
                                    .await,
                                );
                            }
                        }
                    }
                    (r_text, r_err)
                }
            }
        }
        _ => {
            // ── patch_file ─────────────────────────────────────────
            match crate::file_tools::json_patch::text_edit(&path, &args, base) {
                Err(error) => (format!("ERROR: {error}"), true),
                Ok((search, replace)) => {
                    if args.get("json_pointer").is_some() {
                        ctx.file_cache.remove(&cache_key);
                    }
                    // B — capture old content for diff preview before patching.
                    let old_content_for_diff =
                        ctx.file_cache.get(&cache_key).cloned().or_else(|| {
                            crate::file_tools::resolve_safe_path(&path, base)
                                .ok()
                                .and_then(|abs| std::fs::read_to_string(&abs).ok())
                        });
                    let before_hash = old_content_for_diff
                        .as_deref()
                        .map(hash_text)
                        .unwrap_or_else(|| hash_text(""));

                    // Approval: show the computed patch diff (or search/replace when diff can't be made).
                    let mut preview = old_content_for_diff
                        .as_deref()
                        .map(|old| crate::file_tools::make_patch_diff(old, &search, &replace))
                        .unwrap_or_default();
                    if preview.trim().is_empty() {
                        let s = truncate_preview(&search, 700, 28);
                        let r = truncate_preview(&replace, 700, 28);
                        preview = format!(
                            "(no context diff available)\n--- search ---\n{s}\n\n--- replace ---\n{r}\n"
                        );
                    }
                    let approval = ctx
                        .approver
                        .approve(ApprovalRequest::Edit {
                            action: "patch_file".to_string(),
                            path: path.clone(),
                            preview,
                        })
                        .await?;
                    if approval == ApprovalOutcome::Rejected {
                        rejected_by_user = true;
                        (
                            format!(
                                "REJECTED BY USER\naction: patch_file\npath: {path}\n(no changes applied)"
                            ),
                            true,
                        )
                    } else {
                        let (mut patch_result, patch_err) =
                            crate::file_tools::tool_patch_file(&path, &search, &replace, base);

                        if !patch_err
                            && crate::execution_evidence::file_edit_succeeded(
                                "patch_file",
                                &patch_result,
                            )
                        {
                            ctx.file_cache.remove(&cache_key); // invalidate stale cache

                            // B — append diff preview (shows exactly what changed).
                            if let Some(ref old) = old_content_for_diff {
                                let diff =
                                    crate::file_tools::make_patch_diff(old, &search, &replace);
                                if !diff.is_empty() {
                                    patch_result.push_str(&format!("\n{diff}"));
                                }
                            }

                            // Gap 7: auto-verify patch was applied correctly.
                            if let Ok(abs) = crate::file_tools::resolve_safe_path(&path, base) {
                                if let Ok(new_content) = std::fs::read_to_string(&abs) {
                                    let after_hash = hash_text(&new_content);
                                    patch_result.push_str(&format!(
                                        "\n[hash] before={} after={}",
                                        fmt_hash(before_hash),
                                        fmt_hash(after_hash)
                                    ));
                                    if replace.is_empty() || new_content.contains(&replace) {
                                        patch_result
                                            .push_str("\n✓ auto-verify: patch confirmed in file");
                                    } else {
                                        patch_result.push_str(
                                            "\n✗ auto-verify FAILED: replacement text not found — \
                                             file may be in unexpected state; call read_file to inspect",
                                        );
                                    }
                                    // Seed cache with the freshly written content.
                                    ctx.file_cache.insert(cache_key.clone(), new_content);
                                }
                            }
                            // A — auto-test after successful patch
                            if let Some(cmd) = ctx.test_cmd {
                                if let Some(ref root) = ctx.tool_root_abs {
                                    patch_result.push_str(
                                        &automatic_test::observed(
                                            cmd,
                                            root,
                                            ctx.messages,
                                            &mut cargo_inputs,
                                        )
                                        .await,
                                    );
                                }
                            }
                        }
                        (patch_result, patch_err)
                    }
                }
            }
        }
    };

    let changed = crate::execution_evidence::file_edit_succeeded(&tc.name, &result);
    ctx.edit_failures
        .on_result(tc, &result, ctx.exec_verification_context);
    *ctx.file_tool_consec_failures = ctx.edit_failures.count();
    if is_error && !rejected_by_user && tc.name != "read_file" {
        if let Some(reason) = ctx.edit_failures.reflection_reason() {
            *ctx.reflection_required = Some(reason);
        }
    }

    ctx.recovery.focus.record_result(
        tc,
        &result,
        ctx.tool_root_abs.as_deref(),
        ctx.test_cmd,
        ctx.exec_verification_context,
    );
    let automatic_test = crate::execution_evidence::auto_test_outcome(tc.name.as_str(), &result);
    let verified =
        !is_error && automatic_test == crate::execution_evidence::AutoTestOutcome::Passed;
    let verified_level = if verified {
        configured_test_cmd_verification_level(ctx.test_cmd)
    } else {
        None
    };
    match tc.name.as_str() {
        "read_file" => ctx.recovery.on_diagnostic_result(!is_error),
        "write_file" | "patch_file" => {
            if is_error {
                ctx.recovery.on_fix_result(false, None);
            } else if changed {
                exec_proof::record_file_result(
                    exec_proof::FileProofResult {
                        name: tc.name.as_str(),
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
        }
        _ => {}
    }

    // Emit result label.
    let first_line = result.lines().next().unwrap_or("").to_string();
    if is_error {
        let _ = ctx
            .tx
            .send(StreamToken::Delta(format!(
                "[RESULT_FILE_ERR] {first_line}\n"
            )))
            .await;
        if tc.name.as_str() == "read_file" {
            if let Some(display) = repo_map_display {
                let _ = ctx.tx.send(StreamToken::Delta(display)).await;
            }
        }
        *ctx.state = AgentState::Recovery;
        let hint = if rejected_by_user {
            "The user rejected the edit. Choose a safer alternative or ask again with a smaller change."
                .to_string()
        } else if tc.name.as_str() == "read_file" && repo_map_hint.is_some() {
            repo_map_hint.unwrap_or_default()
        } else {
            format!(
                "File tool error: {first_line}\n\
                 Read the error message carefully and fix the issue before proceeding."
            )
        };
        *ctx.pending_system_hint = Some(hint);
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
        *ctx.pending_system_hint = if automatic_test
            == crate::execution_evidence::AutoTestOutcome::Failed
        {
            Some(recovery::AUTO_TEST_FAILURE_HINT.to_string())
        } else if ctx.recovery.stage == Some(RecoveryStage::Fix) {
            Some("Recovery stage=fix: apply a minimal fix now (edit files or run a corrected command).".to_string())
        } else if ctx.recovery.stage == Some(RecoveryStage::Verify) {
            Some(format!(
                "Recovery stage=verify: {}",
                verification_requirement_hint(*ctx.required_verification, ctx.test_cmd)
            ))
        } else {
            None
        };
        if tc.name == "read_file" || changed {
            update_working_memory_after_non_exec(
                ctx.working_mem,
                tc.name.as_str(),
                verified,
                ctx.step_label_for_turn,
                ctx.next_action_for_turn,
                ctx.test_cmd,
            );
        }
        if tc.name.as_str() == "read_file" {
            let command = canonicalize_tool_call_command("read_file", tc.arguments.as_str())
                .unwrap_or_else(|| {
                    format!("read_file(path={})", compact_one_line(path.as_str(), 160))
                });
            let observed_path = parse_read_file_result_path(result.as_str())
                .unwrap_or_else(|| compact_one_line(path.as_str(), 160));
            ctx.observation_evidence
                .remember_read(command.as_str(), observed_path.as_str());
            ctx.observation_evidence.remember_resolution(
                path.as_str(),
                observed_path.as_str(),
                "read_file",
            );
            sync_observation_cache_autosave(ctx.autosaver, ctx.observation_evidence);
        }
        assumption_gate::confirm_file_existence_after_result(
            ctx.assumption_ledger,
            &tc.name,
            &path,
            &result,
        );
        ctx.assumption_ledger.refresh_confirmations(ctx.working_mem);
        if changed {
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
                "successful mutation via {}: {}",
                tc.name.as_str(),
                compact_one_line(path.as_str(), 80)
            ));
        }
    }

    if is_error || *ctx.file_tool_consec_failures >= 2 {
        *ctx.reflection_required = Some(
            ctx.pending_system_hint
                .clone()
                .unwrap_or_else(|| "file tool failure".to_string()),
        );
        *ctx.reflection_trigger_sig = tool_call_action_sig(tc);
    }

    // Append tool result to conversation.
    let history_result = if is_error {
        result.clone()
    } else {
        compact_success_tool_result_for_history(tc.name.as_str(), &result)
    };
    ctx.messages.push(CargoInputReceipt::attach(
        cargo_inputs,
        ctx.recovery.focus.tool_message(tc, history_result),
    ));
    if !is_error && (tc.name == "read_file" || changed) && !ctx.root_read_only {
        if let Some(hint) = build_fix_stage_progress_hint(
            ctx.task_harness,
            ctx.messages,
            ctx.recovery.stage,
            ctx.test_cmd,
        ) {
            *ctx.pending_system_hint = Some(hint);
        }
    }
    if !is_error && ctx.root_read_only && tc.name.as_str() == "read_file" {
        if let Some(plan) = ctx.active_plan.as_ref() {
            if let Some(final_text) = maybe_build_read_only_auto_final_answer(
                true,
                ctx.root_user_text,
                Some(plan),
                ctx.observation_evidence,
                ctx.messages,
                ctx.working_mem,
            ) {
                ctx.complete_done(
                    &final_text,
                    "\n[agent] auto-finalized read-only inspection after sufficient observation.\n\n",
                )
                .await;
                return Ok(ToolFlow::Break);
            } else if ctx.iter + 1 == ctx.max_iters {
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

    if ctx.iter + 1 == ctx.max_iters {
        let _ = ctx
            .tx
            .send(StreamToken::Delta(format!(
                "\n[agent] iteration cap ({}) reached.\n",
                ctx.max_iters
            )))
            .await;
    }
    Ok(ToolFlow::Continue) // skip exec block below
}

pub(super) async fn run_done(ctx: &mut ToolDispatchCtx<'_>, tc: &ToolCallData) -> Result<ToolFlow> {
    // A generic passing check cannot discharge a localized failed check.
    if ctx.recovery.focus.is_pending() {
        let block = format!(
            "[Done Gate] {}",
            ctx.recovery.focus.hint().unwrap_or_default()
        );
        ctx.messages.push(json!({"role":"tool", "tool_call_id":tc.id, "content":format!("GOVERNOR BLOCKED\n{block}")}));
        *ctx.pending_system_hint = Some(block);
        *ctx.state = AgentState::Recovery;
        ctx.autosave().await;
        return Ok(ToolFlow::Continue);
    }

    if ctx.realize_cfg.enabled {
        if let Some(latent) = ctx.latent_plan.as_ref() {
            *ctx.state = AgentState::Recovery;
            ctx.recovery.stage = Some(RecoveryStage::Diagnose);
            let block = build_realize_done_gate_message(latent);
            emit_governor_block(ctx.governor_emit(), block, tc, None).await;
            return Ok(ToolFlow::Continue);
        }
    }

    let last_mutation = ctx.last_mutation_step.unwrap_or(0);
    let last_verify_ok = ctx.last_verify_ok_step.unwrap_or(0);
    if last_mutation > 0 && last_verify_ok < last_mutation {
        *ctx.state = AgentState::Recovery;
        ctx.recovery.stage = Some(RecoveryStage::Verify);
        let block = format!(
            "[Done Gate] Verification required before `done`.\n\
Last mutation step: {last_mutation}\n\
Last verification step: {last_verify_ok}\n\
Required now: {}",
            verification_requirement_hint(*ctx.required_verification, ctx.test_cmd)
        );
        emit_governor_block(ctx.governor_emit(), block, tc, None).await;
        return Ok(ToolFlow::Continue);
    }

    if let Some(command) =
        benchmark_plan_pending_required_exec_command(ctx.root_user_text, ctx.messages, ctx.test_cmd)
    {
        *ctx.state = AgentState::Recovery;
        ctx.recovery.stage = Some(RecoveryStage::Verify);
        let verification_status = task_harness::benchmark_plan_verification_hint(
            ctx.root_user_text,
            ctx.messages,
            ctx.test_cmd,
        )
        .unwrap_or_default();
        let block = format!(
            "[Done Gate] Approved benchmark plan requires explicit verification proof before `done`.\n\
Required now: run `{command}`.\n{verification_status}"
        );
        emit_governor_block(ctx.governor_emit(), block, tc, None).await;
        return Ok(ToolFlow::Continue);
    }

    let args: serde_json::Value = serde_json::from_str(&tc.arguments)?;
    let report = final_handoff::report_context(ctx.root_user_text, ctx.messages, ctx.test_cmd);
    let summary = report_surface::authored(args["summary"].as_str().unwrap_or(""), &report);
    let completed_acceptance = parse_string_list_arg(&args["completed_acceptance"]);
    let remaining_acceptance = parse_string_list_arg(&args["remaining_acceptance"]);
    let acceptance_evidence =
        acceptance_report::parse_done_acceptance_evidence(&args["acceptance_evidence"]);
    let next_steps = args["next_steps"].as_str().unwrap_or("").trim();
    let done_plan = ctx
        .validated_plan_for_turn
        .as_ref()
        .or(ctx.active_plan.as_ref());
    let known_acceptance_commands = acceptance_report::current_commands(ctx.messages, ctx.test_cmd);
    let read_only_scores = if ctx.root_read_only {
        done_plan
            .map(|plan| {
                build_read_only_evidence_scores(ctx.root_user_text, plan, ctx.observation_evidence)
            })
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    let evidence_rows = match validate_done_acceptance(
        done_plan,
        &completed_acceptance,
        &remaining_acceptance,
        &acceptance_evidence,
        &known_acceptance_commands,
        ctx.observation_evidence,
    ) {
        Ok(rows) => rows,
        Err(e) => {
            *ctx.state = AgentState::Recovery;
            ctx.recovery.stage = Some(if ctx.root_read_only {
                RecoveryStage::Diagnose
            } else {
                RecoveryStage::Verify
            });
            let hint = build_done_acceptance_recovery_hint(
                &e.to_string(),
                done_plan,
                &known_acceptance_commands,
                &read_only_scores,
                ctx.cfg
                    .base_url
                    .contains("generativelanguage.googleapis.com"),
            );
            let block = format!(
                "{}{}{}",
                governor_contract::done_invalid_acceptance_message(&e.to_string()),
                hint,
                acceptance_report::hint(done_plan, &known_acceptance_commands)
            );
            emit_governor_block(ctx.governor_emit(), block, tc, None).await;
            return Ok(ToolFlow::Continue);
        }
    };

    let summary_text = if summary.is_empty() {
        synthesize_action_done_summary(done_plan, ctx.messages).unwrap_or_default()
    } else {
        summary.to_string()
    };

    let mut final_text = String::new();
    final_text.push_str("[DONE]\n");
    if !summary_text.is_empty() {
        final_text.push_str(summary_text.as_str());
    }
    final_text.push_str("\n\nAcceptance:\n");
    let evidence_by_idx: std::collections::HashMap<usize, String> =
        evidence_rows.into_iter().collect();
    let done_plan = done_plan.expect("validated done plan");
    for item in &completed_acceptance {
        let idx =
            resolve_acceptance_reference(item, done_plan).expect("completed acceptance validated");
        final_text.push_str("- done: ");
        final_text.push_str(acceptance_reference_label(done_plan, idx).as_str());
        if let Some(command) = evidence_by_idx.get(&idx) {
            final_text.push_str(" via `");
            final_text.push_str(command.as_str());
            final_text.push('`');
        }
        final_text.push('\n');
    }
    for item in &remaining_acceptance {
        let idx =
            resolve_acceptance_reference(item, done_plan).expect("remaining acceptance validated");
        final_text.push_str("- remaining: ");
        final_text.push_str(acceptance_reference_label(done_plan, idx).as_str());
        final_text.push('\n');
    }
    if !next_steps.is_empty() {
        final_text.push_str("\n\nNext:\n");
        final_text.push_str(next_steps);
    }

    let exact_content_error = exact_content::validate_exact_content_task(
        ctx.root_user_text,
        ctx.tool_root_abs.as_deref(),
    )
    .err();
    let needs_artifact_repair = exact_content_error.is_some();
    let requirement_error = remaining_acceptance
        .is_empty()
        .then(|| requirement_proof::issue(ctx.messages, ctx.test_cmd))
        .flatten();
    let handoff_error = exact_content_error.or(requirement_error).or_else(|| {
        if ctx.root_read_only {
            None
        } else {
            validate_authored_done_summary(
                ctx.root_user_text,
                summary,
                &final_text,
                ctx.messages,
                ctx.test_cmd,
            )
            .err()
        }
    });
    if let Some(block) = handoff_error {
        *ctx.state = AgentState::Recovery;
        if needs_artifact_repair {
            // A line-oriented test cannot override a failed exact-byte contract.
            *ctx.last_build_verify_ok_step = None;
            *ctx.last_behavioral_verify_ok_step = None;
            *ctx.last_verify_ok_step = None;
            ctx.recovery.stage = Some(RecoveryStage::Fix);
        }
        ctx.messages.push(json!({
            "role": "tool",
            "tool_call_id": tc.id,
            "content": format!("GOVERNOR BLOCKED\n\n{block}"),
        }));
        ctx.autosave().await;
        let _ = ctx
            .tx
            .send(StreamToken::Delta(format!(
                "[RESULT][Recovery] GOVERNOR BLOCK\n{block}\n"
            )))
            .await;
        *ctx.pending_system_hint = Some(block);
        return Ok(ToolFlow::Continue);
    }

    ctx.task_outcome.accept_done(&remaining_acceptance);

    // Close out the tool call so session JSON remains valid on resume.
    ctx.messages.push(json!({
        "role": "tool",
        "tool_call_id": tc.id,
        "content": "OK: done"
    }));
    ctx.messages
        .push(json!({"role": "assistant", "content": final_text.clone()}));

    ctx.autosave().await;

    let _ = ctx
        .tx
        .send(StreamToken::Delta(format!("\n\n{final_text}\n")))
        .await;
    Ok(ToolFlow::Break)
}

pub(super) async fn run_exec(ctx: &mut ToolDispatchCtx<'_>, tc: &ToolCallData) -> Result<ToolFlow> {
    let args: serde_json::Value = serde_json::from_str(&tc.arguments)?;
    let command = args["command"].as_str().unwrap_or("").to_string();

    // Resolve an optional cwd (absolute or relative). Relative cwd is resolved against
    // the current tracked directory (or tool_root).
    let mut cwd_note: Option<String> = None;
    let cwd_used: Option<String> = if let Some(c0) = args["cwd"]
        .as_str()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        let pb = std::path::PathBuf::from(c0);
        let candidate = if pb.is_absolute() {
            pb
        } else if let Some(base) = ctx.cur_cwd.as_deref().or(ctx.tool_root_abs.as_deref()) {
            std::path::PathBuf::from(base).join(pb)
        } else {
            pb
        };
        let cand_str = candidate.to_string_lossy().into_owned();
        if let Some(ref root) = ctx.tool_root_abs {
            if is_within_root(&cand_str, root) {
                Some(cand_str)
            } else {
                cwd_note = Some(format!(
                    "NOTE: requested cwd is outside tool_root; ignoring: {c0}"
                ));
                ctx.cur_cwd.clone().or_else(|| ctx.tool_root_abs.clone())
            }
        } else {
            Some(cand_str)
        }
    } else {
        ctx.cur_cwd.clone().or_else(|| ctx.tool_root_abs.clone())
    };
    let cwd_used_label = cwd_used
        .as_deref()
        .unwrap_or("(workspace root)")
        .to_string();

    // Emit tool-call annotation to TUI (dim line the UI will colour differently).
    let _ = ctx
        .tx
        .send(StreamToken::Delta(format!(
            "\n\n[TOOL][{:?}] {command}\n[cwd] {cwd_used_label}\n",
            *ctx.state
        )))
        .await;
    let _ = ctx.tx.send(StreamToken::ToolCall(tc.clone())).await;

    if let Some(block) = should_block_git_landmines(&command, ctx.tool_root_abs.as_deref()) {
        *ctx.state = AgentState::Recovery;
        ctx.recovery.on_exec_result(ExecKind::Action, None, false);
        let _ = ctx
            .tx
            .send(StreamToken::Delta(format!(
                "[RESULT][Recovery] GOVERNOR BLOCK\n{block}\n"
            )))
            .await;

        let cwd_line = format!("cwd: {cwd_used_label}");
        let tool_output = inject_cwd(
            &format!("GOVERNOR BLOCKED\n\n{block}\n\ncommand:\n{command}"),
            &cwd_line,
            cwd_note.as_deref(),
        );

        ctx.messages.push(json!({
            "role": "tool",
            "tool_call_id": tc.id,
            "content": tool_output,
        }));
        ctx.autosave().await;

        // Update memory so repeating the same blocked command triggers stronger hints.
        let _ = ctx.mem.on_tool_result(&command, "", &block, 1);
        *ctx.last_exec_step = Some(ctx.this_step);
        *ctx.pending_system_hint = Some(block);
        ctx.emit_governor_state().await;

        if ctx.iter + 1 == ctx.max_iters {
            let _ = ctx
                .tx
                .send(StreamToken::Delta(format!(
                    "\n[agent] iteration cap ({}) reached.\n",
                    ctx.max_iters
                )))
                .await;
        }
        return Ok(ToolFlow::Continue);
    }

    // Runtime block: prevent "same command spam" once it's already repeated.
    // Allow repeating after a successful mutation (e.g., fix file then rerun tests).
    let cmd_sig = command_sig(&command);
    let repeated_cmd = ctx.mem.same_command_repeats >= 3
        && ctx.mem.last_command_sig.as_deref() == Some(cmd_sig.as_str());
    let mutated_since_last_exec =
        ctx.last_mutation_step.unwrap_or(0) > ctx.last_exec_step.unwrap_or(0);
    if repeated_cmd && !mutated_since_last_exec {
        *ctx.state = AgentState::Recovery;
        ctx.recovery.stage = Some(RecoveryStage::Diagnose);
        let block = format!(
            "Repeated identical command blocked.\n\
same_command_repeats: {}\n\
command_sig: {}\n\
\n\
Required now: change strategy (diagnose or apply a fix) before retrying.\n\
Do NOT run the exact same command again without a material change.",
            ctx.mem.same_command_repeats, cmd_sig
        );

        let _ = ctx
            .tx
            .send(StreamToken::Delta(format!(
                "[RESULT][Recovery] GOVERNOR BLOCK\n{block}\n"
            )))
            .await;

        let cwd_line = format!("cwd: {cwd_used_label}");
        let tool_output = inject_cwd(
            &format!("GOVERNOR BLOCKED\n\n{block}\n\ncommand:\n{command}"),
            &cwd_line,
            cwd_note.as_deref(),
        );

        ctx.messages.push(json!({
            "role": "tool",
            "tool_call_id": tc.id,
            "content": tool_output,
        }));
        ctx.autosave().await;

        let _ = ctx.mem.on_tool_result(&command, "", &block, 1);
        *ctx.last_exec_step = Some(ctx.this_step);
        *ctx.pending_system_hint = Some(block.clone());
        *ctx.reflection_required = Some(block);
        *ctx.reflection_trigger_sig = tool_call_action_sig(tc);
        ctx.emit_governor_state().await;

        if ctx.iter + 1 == ctx.max_iters {
            let _ = ctx
                .tx
                .send(StreamToken::Delta(format!(
                    "\n[agent] iteration cap ({}) reached.\n",
                    ctx.max_iters
                )))
                .await;
        }
        return Ok(ToolFlow::Continue);
    }

    let approval = ctx
        .approver
        .approve(ApprovalRequest::Command {
            command: command.clone(),
            cwd: cwd_used.clone(),
        })
        .await?;
    if approval == ApprovalOutcome::Rejected {
        *ctx.state = AgentState::Recovery;
        ctx.recovery.on_exec_result(ExecKind::Action, None, false);
        let _ = ctx
            .tx
            .send(StreamToken::Delta(
                "[RESULT][Recovery] REJECTED by user\n".to_string(),
            ))
            .await;
        let cwd_line = format!("cwd: {cwd_used_label}");
        let tool_output = format!("REJECTED BY USER\n{cwd_line}\ncommand:\n{command}");

        ctx.messages.push(json!({
            "role": "tool",
            "tool_call_id": tc.id,
            "content": tool_output,
        }));
        ctx.autosave().await;
        *ctx.last_exec_step = Some(ctx.this_step);
        *ctx.pending_system_hint = Some(
            "The user rejected the command. Choose a safer alternative or explain why it is necessary before retrying."
                .to_string(),
        );
        ctx.emit_governor_state().await;

        if ctx.iter + 1 == ctx.max_iters {
            let _ = ctx
                .tx
                .send(StreamToken::Delta(format!(
                    "\n[agent] iteration cap ({}) reached.\n",
                    ctx.max_iters
                )))
                .await;
        }
        return Ok(ToolFlow::Continue);
    }

    let cargo_input_probe = cwd_used
        .as_deref()
        .zip(ctx.tool_root_abs.as_deref())
        .and_then(|(cwd, root)| CargoInputProbe::start(ctx.messages, &command, cwd, root));
    let exec_cmd = wrap_exec_with_pwd(&command);
    let exec_result = exec::run_command(&exec_cmd, cwd_used.as_deref()).await;

    let (stdout, stderr, exit_code) = match exec_result {
        Ok(r) => (r.stdout, r.stderr, r.exit_code),
        Err(e) => (String::new(), e.to_string(), -1),
    };

    // Update cwd from marker output.
    let (stdout, pwd_after) = strip_pwd_marker(&stdout);
    let mut cwd_after_note: Option<String> = None;
    if let Some(p) = pwd_after {
        if let Some(ref root) = ctx.tool_root_abs {
            if is_within_root(&p, root) {
                *ctx.cur_cwd = Some(p);
            } else {
                cwd_after_note = Some(format!(
                    "NOTE: cwd_after was outside tool_root; ignored: {p}"
                ));
            }
        } else {
            *ctx.cur_cwd = Some(p);
        }
    }
    let escaped_tool_root = cwd_after_note.is_some();
    let cwd_after_label = ctx
        .cur_cwd
        .as_deref()
        .unwrap_or(cwd_used_label.as_str())
        .to_string();
    let cwd_line = if cwd_used_label == cwd_after_label {
        format!("cwd: {cwd_used_label}")
    } else {
        format!("cwd: {cwd_used_label}\ncwd_after: {cwd_after_label}")
    };

    let mut note_lines: Vec<String> = Vec::new();
    if let Some(n) = cwd_note.take() {
        note_lines.push(n);
    }
    if let Some(n) = cwd_after_note.take() {
        note_lines.push(n);
    }
    let note = if note_lines.is_empty() {
        None
    } else {
        Some(note_lines.join("\n"))
    };

    *ctx.state = AgentState::Verifying;
    let suspicious_reason = if exit_code == 0 {
        suspicious_success_reason(&stdout, &stderr)
    } else {
        None
    };
    let effective_exit_code =
        if exit_code == 0 && (suspicious_reason.is_some() || escaped_tool_root) {
            1
        } else {
            exit_code
        };

    let tool_output = if effective_exit_code == 0 {
        let base = build_ok_tool_output(&stdout);
        inject_cwd(&base, &cwd_line, note.as_deref())
    } else {
        let mut out = build_failed_tool_output(&stdout, &stderr, effective_exit_code);
        if let Some(reason) = suspicious_reason {
            out = format!(
                "NOTE: command returned exit_code=0 but was treated as failure.\nreason: {reason}\n\n{out}"
            );
        }
        if escaped_tool_root && exit_code == 0 {
            out = format!(
                "NOTE: command escaped tool_root and was treated as failure.\n\
This is blocked to prevent nested-repo / accidental repo-root modifications.\n\n{out}"
            );
        }
        inject_cwd(&out, &cwd_line, note.as_deref())
    };

    ctx.edit_failures
        .on_result(tc, &tool_output, ctx.exec_verification_context);
    *ctx.file_tool_consec_failures = ctx.edit_failures.count();
    ctx.recovery.focus.record_result(
        tc,
        &tool_output,
        ctx.tool_root_abs.as_deref(),
        ctx.test_cmd,
        ctx.exec_verification_context,
    );

    let result_label = if effective_exit_code == 0 {
        format!("[RESULT][{:?}] exit=0\n", *ctx.state)
    } else {
        format!("[RESULT][{:?}] exit={effective_exit_code} !\n", *ctx.state)
    };
    let _ = ctx.tx.send(StreamToken::Delta(result_label)).await;

    // ── Append tool result (with tool_call_id preserved) ───────────────
    let history_tool_output = if effective_exit_code == 0 {
        compact_success_tool_result_for_history("exec", &tool_output)
    } else {
        tool_output.clone()
    };
    let cargo_inputs =
        cargo_input_probe.map(|p| p.finish(effective_exit_code == 0, &stdout, &stderr));
    ctx.messages.push(report_counts::capture(
        CargoInputReceipt::attach(
            cargo_inputs,
            ctx.recovery.focus.tool_message(tc, history_tool_output),
        ),
        &stdout,
        &stderr,
    ));
    *ctx.last_exec_step = Some(ctx.this_step);
    ctx.autosave().await;

    // Update failure memory + recovery governor + possibly inject a system hint.
    let classification = ctx.exec_verification_context.classify(&command);
    let verify_level = classification.verification;
    let exec_kind = classification.kind;
    exec_proof::record_result(
        &command,
        ctx.exec_verification_context,
        exec_proof::ExecProofResult {
            content: &tool_output,
            succeeded: effective_exit_code == 0 && !escaped_tool_root,
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
    if effective_exit_code == 0 && !escaped_tool_root {
        update_working_memory_after_exec(
            ctx.working_mem,
            command.as_str(),
            stdout.as_str(),
            tool_output.as_str(),
            exec_kind,
            ctx.step_label_for_turn,
            ctx.next_action_for_turn,
        );
        ctx.assumption_ledger.refresh_confirmations(ctx.working_mem);
        if exec_kind == ExecKind::Action {
            *ctx.impact_required = Some(format!(
                "successful mutation command: {}",
                compact_one_line(command.as_str(), 100)
            ));
        }
    }
    let mut hint = if escaped_tool_root {
        Some(
            "SANDBOX BREACH: Your command ended outside tool_root.\n\
Action: re-run from tool_root, avoid `cd ..` / absolute paths, and verify `pwd` stays under tool_root."
                .to_string(),
        )
    } else {
        ctx.mem
            .on_tool_result(&command, &stdout, &stderr, effective_exit_code)
    };

    ctx.recovery.on_exec_result(
        exec_kind,
        verify_level,
        effective_exit_code == 0 && !escaped_tool_root,
    );

    if effective_exit_code == 0 && !escaped_tool_root {
        if ctx.recovery.stage == Some(RecoveryStage::Fix) {
            hint = Some(
                "Recovery stage=fix: apply a minimal fix now (edit files or run a corrected command)."
                    .to_string(),
            );
        } else if ctx.recovery.stage == Some(RecoveryStage::Verify) {
            hint = Some(format!(
                "Recovery stage=verify: {}",
                verification_requirement_hint(*ctx.required_verification, ctx.test_cmd)
            ));
        }
    }

    let hint_clone_for_reflect = hint.clone();
    *ctx.pending_system_hint = hint;
    *ctx.state = if effective_exit_code != 0 || ctx.recovery.in_recovery() {
        AgentState::Recovery
    } else {
        AgentState::Planning
    };

    if effective_exit_code != 0 || ctx.mem.repeated_failure_or_stall() {
        let default_reason = if effective_exit_code != 0 {
            let class_ctx = error_class_hint(&ctx.mem.last_error_class);
            if class_ctx.is_empty() {
                "failure detected".to_string()
            } else {
                class_ctx.to_string()
            }
        } else {
            "stall detected".to_string()
        };
        *ctx.reflection_required = Some(hint_clone_for_reflect.unwrap_or_else(|| default_reason));
        *ctx.reflection_trigger_sig = tool_call_action_sig(tc);
    } else if ctx.reflection_guard.is_none() {
        *ctx.reflection_trigger_sig = None;
    }

    ctx.emit_governor_state().await;

    // Safety: stop if we've hit the iteration cap.
    if ctx.iter + 1 == ctx.max_iters {
        let _ = ctx
            .tx
            .send(StreamToken::Delta(format!(
                "\n[agent] iteration cap ({}) reached.\n",
                ctx.max_iters
            )))
            .await;
    }
    Ok(ToolFlow::Continue)
}
