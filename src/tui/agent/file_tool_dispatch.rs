use super::tool_dispatch::{ToolDispatchCtx, ToolFlow};
use super::*;

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
