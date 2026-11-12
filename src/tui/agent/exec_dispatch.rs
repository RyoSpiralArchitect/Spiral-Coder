use super::tool_dispatch::{ToolDispatchCtx, ToolFlow};
use super::*;

pub(super) async fn run_exec(ctx: &mut ToolDispatchCtx<'_>, tc: &ToolCallData) -> Result<ToolFlow> {
    let args: serde_json::Value = serde_json::from_str(&tc.arguments)?;
    let command = args["command"].as_str().unwrap_or("").to_string();

    // Resolve an optional cwd (absolute or relative). Relative cwd is resolved against
    // the current tracked directory (or tool_root).
    let mut cwd_note: Option<String> = None;
    let default_cwd = ctx.cur_cwd.as_deref().or(ctx.tool_root_abs.as_deref());
    let default_cwd = crate::exec::execution_cwd(default_cwd)
        .ok()
        .flatten()
        .or_else(|| default_cwd.map(str::to_owned))
        .filter(|cwd| {
            ctx.tool_root_abs
                .as_deref()
                .is_none_or(|root| is_within_root(cwd, root))
        })
        .or_else(|| ctx.tool_root_abs.clone());
    let requested_cwd = args["cwd"]
        .as_str()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty());
    let cwd_used = match requested_cwd {
        // Preserve invalid input for the runner's native validation failure.
        // In particular, normalization must not erase a forbidden component.
        Some(cwd) if crate::exec::validate_cwd(cwd).is_err() => Some(cwd.to_owned()),
        Some(c0) => {
            let pb = std::path::PathBuf::from(c0);
            let candidate = if pb.is_absolute() {
                pb
            } else if let Some(base) = ctx.cur_cwd.as_deref().or(ctx.tool_root_abs.as_deref()) {
                std::path::PathBuf::from(base).join(pb)
            } else {
                pb
            };
            let raw = candidate.to_string_lossy().into_owned();
            let cand_str = crate::exec::execution_cwd(Some(&raw))
                .ok()
                .flatten()
                .unwrap_or(raw);
            if let Some(ref root) = ctx.tool_root_abs {
                if is_within_root(&cand_str, root) {
                    Some(cand_str)
                } else {
                    cwd_note = Some(format!(
                        "NOTE: requested cwd is outside tool_root; ignoring: {c0}"
                    ));
                    default_cwd
                }
            } else {
                Some(cand_str)
            }
        }
        None => default_cwd,
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

    let (stdout, stderr, exit_code, actual_cwd) = match exec_result {
        Ok(r) => (r.stdout, r.stderr, r.exit_code, r.cwd),
        Err(e) => (String::new(), e.to_string(), -1, None),
    };
    let cwd_used_label = actual_cwd.unwrap_or(cwd_used_label);

    // Update cwd from marker output.
    let (stdout, pwd_after) = strip_pwd_marker(&stdout);
    let mut cwd_after_note: Option<String> = None;
    if let Some(p) = pwd_after {
        let p = crate::exec::canonical_cwd(Some(&p)).unwrap_or(p);
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

    // A focused comparison deliberately keeps the test's failing exit code.
    // The observed diff advances diagnosis for the original check only.
    let observed_comparison = effective_exit_code == 1
        && !escaped_tool_root
        && command.contains("Focused unittest:")
        && ctx
            .recovery
            .focus
            .diagnostic_followup(ctx.messages, ctx.tool_root_abs.as_deref())
            .is_some_and(|followup| {
                followup.observed_comparison(&command, &cwd_used_label, &stdout)
            });

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

    if observed_comparison {
        hint = Some("The focused comparison captured the differing assertion operands. Use the changed values and the test source to choose the next source read or minimal fix. The original exact check is still pending.".into());
    } else {
        ctx.recovery.on_exec_result(
            exec_kind,
            verify_level,
            effective_exit_code == 0 && !escaped_tool_root,
        );
    }

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

    if !observed_comparison && (effective_exit_code != 0 || ctx.mem.repeated_failure_or_stall()) {
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
