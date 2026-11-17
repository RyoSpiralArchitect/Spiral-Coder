//! Post-stream plan/think validation: when a tool call exists, the parsed
//! plan and think blocks are validated, rescued, or rejected through
//! governor blocks. Extracted verbatim from the agent loop.
use super::*;
use serde_json::json;

pub(super) enum PlanThinkFlow {
    Continue,
    Break,
    Validated,
}

pub(super) struct PlanThinkOut {
    pub flow: PlanThinkFlow,
    pub plan: Option<PlanBlock>,
    pub think: Option<ThinkBlock>,
}

/// Shared `GovernorEmit` view used by every rejection in this gate; only
/// `block`, `tc`, and the clean text vary per callsite.
#[allow(clippy::too_many_arguments)]
fn governor_emit<'a>(
    tx: &'a mpsc::Sender<StreamToken>,
    messages: &'a mut Vec<serde_json::Value>,
    autosaver: &'a Option<std::sync::Arc<crate::agent_session::SessionAutoSaver>>,
    tool_root_abs: &'a Option<String>,
    checkpoint: &'a Option<String>,
    cur_cwd: &'a Option<String>,
    pending_system_hint: &'a mut Option<String>,
    state: AgentState,
    recovery: &'a RecoveryGovernor,
    mem: &'a FailureMemory,
    file_tool_consec_failures: usize,
    last_mutation_step: Option<usize>,
    last_verify_ok_step: Option<usize>,
    last_reflection: Option<&'a ReflectionBlock>,
) -> GovernorEmit<'a> {
    GovernorEmit {
        tx,
        messages,
        autosaver,
        tool_root_abs,
        checkpoint,
        cur_cwd,
        pending_system_hint,
        state,
        recovery,
        mem,
        file_tool_consec_failures,
        last_mutation_step,
        last_verify_ok_step,
        last_reflection,
    }
}

pub(super) struct PlanThinkCtx<'a> {
    pub tx: &'a mpsc::Sender<StreamToken>,
    pub autosaver: &'a Option<std::sync::Arc<crate::agent_session::SessionAutoSaver>>,
    pub cfg: &'a RunConfig,
    pub task_harness: task_harness::TaskHarness,
    pub evaluator_loop: &'a EvaluatorLoop,
    pub tc: &'a ToolCallData,
    pub assistant_text: &'a str,
    pub assistant_text_clean: &'a String,
    pub parsed_plan: &'a Option<PlanBlock>,
    pub parsed_think: &'a Option<ThinkBlock>,
    pub observer_observation: Option<&'a observer_continuation::Observation>,
    pub compat_last_think: &'a Option<ThinkBlock>,
    pub root_user_text: &'a str,
    pub test_cmd: &'a Option<String>,
    pub tool_root_abs: &'a Option<String>,
    pub checkpoint: &'a Option<String>,
    pub cur_cwd: &'a Option<String>,
    pub task_contract: &'a TaskContract,
    pub instruction_resolver: &'a InstructionResolver,
    pub exec_verification_context: &'a exec_verification::ExecVerificationContext<'a>,
    pub observation_evidence: &'a ObservationEvidence,
    pub mem: &'a FailureMemory,
    pub last_reflection: &'a Option<ReflectionBlock>,
    pub file_tool_consec_failures: usize,
    pub last_mutation_step: Option<usize>,
    pub last_build_verify_ok_step: Option<usize>,
    pub last_behavioral_verify_ok_step: Option<usize>,
    pub path_required_verification: VerificationLevel,
    pub root_read_only: bool,
    pub goal_wants_actions: bool,
    pub messages: &'a mut Vec<serde_json::Value>,
    pub state: &'a mut AgentState,
    pub recovery: &'a mut RecoveryGovernor,
    pub pending_system_hint: &'a mut Option<String>,
    pub working_mem: &'a mut WorkingMemory,
    pub assumption_ledger: &'a mut AssumptionLedger,
    pub active_plan: &'a mut ActivePlan,
    pub intent_required_verification: &'a mut VerificationLevel,
    pub required_verification: &'a mut VerificationLevel,
    pub last_verify_ok_step: &'a mut Option<usize>,
    pub task_outcome: &'a mut outcome::TaskOutcome,
}

pub(super) async fn validate_plan_think(ctx: &mut PlanThinkCtx<'_>) -> Result<PlanThinkOut> {
    let tx = ctx.tx;
    let autosaver = ctx.autosaver;
    let cfg = ctx.cfg;
    let task_harness = ctx.task_harness;
    let evaluator_loop = ctx.evaluator_loop;
    let tc = ctx.tc;
    let assistant_text = ctx.assistant_text;
    let assistant_text_clean = ctx.assistant_text_clean;
    let parsed_plan = ctx.parsed_plan;
    let parsed_think = ctx.parsed_think;
    let observer_observation = ctx
        .observer_observation
        .filter(|observation| observation.matches_accepted_call(tc, assistant_text));
    let compat_last_think = ctx.compat_last_think;
    let root_user_text = ctx.root_user_text;
    let test_cmd = ctx.test_cmd;
    let tool_root_abs = ctx.tool_root_abs;
    let checkpoint = ctx.checkpoint;
    let cur_cwd = ctx.cur_cwd;
    let task_contract = ctx.task_contract;
    let instruction_resolver = ctx.instruction_resolver;
    let exec_verification_context = ctx.exec_verification_context;
    let observation_evidence = ctx.observation_evidence;
    let mem = ctx.mem;
    let last_reflection = ctx.last_reflection;
    let file_tool_consec_failures = ctx.file_tool_consec_failures;
    let last_mutation_step = ctx.last_mutation_step;
    let last_build_verify_ok_step = ctx.last_build_verify_ok_step;
    let last_behavioral_verify_ok_step = ctx.last_behavioral_verify_ok_step;
    let path_required_verification = ctx.path_required_verification;
    let root_read_only = ctx.root_read_only;
    let goal_wants_actions = ctx.goal_wants_actions;
    let messages = &mut *ctx.messages;
    let state = &mut *ctx.state;
    let recovery = &mut *ctx.recovery;
    let pending_system_hint = &mut *ctx.pending_system_hint;
    let working_mem = &mut *ctx.working_mem;
    let assumption_ledger = &mut *ctx.assumption_ledger;
    let active_plan = &mut *ctx.active_plan;
    let intent_required_verification = &mut *ctx.intent_required_verification;
    let required_verification = &mut *ctx.required_verification;
    let last_verify_ok_step = &mut *ctx.last_verify_ok_step;
    let task_outcome = &mut *ctx.task_outcome;

    let validated_plan_for_turn: Option<PlanBlock>;
    let validated_think_for_turn: Option<ThinkBlock>;
    let mut used_general_plan_rescue = false;
    let inherited_closeout = parsed_plan.is_none() && active_plan.is_closeout_only();
    let candidate_plan = match parsed_plan.as_ref() {
        Some(plan) => {
            if let Err(e) = validate_plan_for_task_contract(
                plan,
                root_read_only,
                &task_contract,
                &instruction_resolver,
                &*messages,
            ) {
                let retained = observer_observation
                    .filter(|_| {
                        e.to_string() == governor_contract::task_contract_plan_drift_message()
                    })
                    .and_then(|_| active_plan.work())
                    .and_then(|active| {
                        plan_revision::preserve_goal_for_observation(
                            plan,
                            active,
                            messages,
                            task_contract,
                        )
                    })
                    .filter(|plan| {
                        validate_plan_for_task_contract(
                            plan,
                            root_read_only,
                            task_contract,
                            instruction_resolver,
                            messages,
                        )
                        .is_ok()
                    });
                if let Some(retained) = retained {
                    emit_telemetry_event(
                        tx,
                        "observer_plan_goal_retained",
                        json!({"call_id":tc.id,"tool":tc.name}),
                    )
                    .await;
                    retained
                } else if matches!(cfg.provider, ProviderKind::Mistral) {
                    if let Some(repaired) = repair_mistral_plan_for_tool_turn(
                        Some(plan),
                        active_plan.work(),
                        tc,
                        &root_user_text,
                        root_read_only,
                    ) {
                        if validate_plan_for_task_contract(
                            &repaired,
                            root_read_only,
                            &task_contract,
                            &instruction_resolver,
                            &*messages,
                        )
                        .is_ok()
                        {
                            let _ = tx
                            .send(StreamToken::Delta(
                                "\n[compat] repaired Mistral plan skeleton for observation tool turn\n".to_string(),
                            ))
                            .await;
                            repaired
                        } else {
                            *state = AgentState::Recovery;
                            let block = plan_revision::rejection(
                                &mut (*recovery).stage,
                                &e.to_string(),
                                active_plan.as_ref(),
                                &*messages,
                                &task_contract,
                            );

                            emit_governor_block(
                                governor_emit(
                                    &tx,
                                    &mut *messages,
                                    &autosaver,
                                    &tool_root_abs,
                                    &checkpoint,
                                    &cur_cwd,
                                    &mut *pending_system_hint,
                                    *state,
                                    &*recovery,
                                    &mem,
                                    file_tool_consec_failures,
                                    last_mutation_step,
                                    *last_verify_ok_step,
                                    last_reflection.as_ref(),
                                ),
                                block,
                                &*tc,
                                Some(&assistant_text_clean),
                            )
                            .await;
                            return Ok(PlanThinkOut {
                                flow: PlanThinkFlow::Continue,
                                plan: None,
                                think: None,
                            });
                        }
                    } else {
                        *state = AgentState::Recovery;
                        let block = plan_revision::rejection(
                            &mut (*recovery).stage,
                            &e.to_string(),
                            active_plan.as_ref(),
                            &*messages,
                            &task_contract,
                        );

                        emit_governor_block(
                            governor_emit(
                                &tx,
                                &mut *messages,
                                &autosaver,
                                &tool_root_abs,
                                &checkpoint,
                                &cur_cwd,
                                &mut *pending_system_hint,
                                *state,
                                &*recovery,
                                &mem,
                                file_tool_consec_failures,
                                last_mutation_step,
                                *last_verify_ok_step,
                                last_reflection.as_ref(),
                            ),
                            block,
                            &*tc,
                            Some(&assistant_text_clean),
                        )
                        .await;
                        return Ok(PlanThinkOut {
                            flow: PlanThinkFlow::Continue,
                            plan: None,
                            think: None,
                        });
                    }
                } else {
                    *state = AgentState::Recovery;
                    let block = plan_revision::rejection(
                        &mut (*recovery).stage,
                        &e.to_string(),
                        active_plan.as_ref(),
                        &*messages,
                        &task_contract,
                    );

                    emit_governor_block(
                        governor_emit(
                            &tx,
                            &mut *messages,
                            &autosaver,
                            &tool_root_abs,
                            &checkpoint,
                            &cur_cwd,
                            &mut *pending_system_hint,
                            *state,
                            &*recovery,
                            &mem,
                            file_tool_consec_failures,
                            last_mutation_step,
                            *last_verify_ok_step,
                            last_reflection.as_ref(),
                        ),
                        block,
                        &*tc,
                        Some(&assistant_text_clean),
                    )
                    .await;
                    return Ok(PlanThinkOut {
                        flow: PlanThinkFlow::Continue,
                        plan: None,
                        think: None,
                    });
                }
            } else {
                plan.clone()
            }
        }
        None => match active_plan.as_ref() {
            Some(plan) => plan.clone(),
            None => {
                if let Some(rescued) = rescue_read_only_missing_plan_for_tool_turn(
                    &*messages,
                    tc,
                    &root_user_text,
                    root_read_only,
                    cfg.provider.clone(),
                ) {
                    if validate_plan_for_task_contract(
                        &rescued,
                        root_read_only,
                        &task_contract,
                        &instruction_resolver,
                        &*messages,
                    )
                    .is_ok()
                    {
                        let _ = tx
                        .send(StreamToken::Delta(
                            "\n[compat] rescued repeated read-only plan-gate miss with synthetic observation plan\n"
                                .to_string(),
                        ))
                        .await;
                        rescued
                    } else {
                        *state = AgentState::Recovery;
                        recovery.stage = Some(RecoveryStage::Diagnose);
                        let block = governor_contract::missing_plan_message();

                        emit_governor_block(
                            governor_emit(
                                &tx,
                                &mut *messages,
                                &autosaver,
                                &tool_root_abs,
                                &checkpoint,
                                &cur_cwd,
                                &mut *pending_system_hint,
                                *state,
                                &*recovery,
                                &mem,
                                file_tool_consec_failures,
                                last_mutation_step,
                                *last_verify_ok_step,
                                last_reflection.as_ref(),
                            ),
                            block,
                            &*tc,
                            Some(&assistant_text_clean),
                        )
                        .await;
                        return Ok(PlanThinkOut {
                            flow: PlanThinkFlow::Continue,
                            plan: None,
                            think: None,
                        });
                    }
                } else if let Some(rescued) = rescue_missing_plan_for_tool_turn(
                    &*messages,
                    tc,
                    &root_user_text,
                    task_harness,
                    root_read_only,
                    goal_wants_actions,
                    cfg.provider.clone(),
                    *required_verification,
                    test_cmd.as_deref(),
                ) {
                    if validate_plan_for_task_contract(
                        &rescued,
                        root_read_only,
                        &task_contract,
                        &instruction_resolver,
                        &*messages,
                    )
                    .is_ok()
                    {
                        used_general_plan_rescue = true;
                        let _ = tx
                        .send(StreamToken::Delta(
                            "\n[compat] rescued repeated plan-gate miss with synthetic action plan\n"
                                .to_string(),
                        ))
                        .await;
                        rescued
                    } else {
                        *state = AgentState::Recovery;
                        recovery.stage = Some(RecoveryStage::Diagnose);
                        let block = governor_contract::missing_plan_message();

                        emit_governor_block(
                            governor_emit(
                                &tx,
                                &mut *messages,
                                &autosaver,
                                &tool_root_abs,
                                &checkpoint,
                                &cur_cwd,
                                &mut *pending_system_hint,
                                *state,
                                &*recovery,
                                &mem,
                                file_tool_consec_failures,
                                last_mutation_step,
                                *last_verify_ok_step,
                                last_reflection.as_ref(),
                            ),
                            block,
                            &*tc,
                            Some(&assistant_text_clean),
                        )
                        .await;
                        return Ok(PlanThinkOut {
                            flow: PlanThinkFlow::Continue,
                            plan: None,
                            think: None,
                        });
                    }
                } else if matches!(cfg.provider, ProviderKind::Mistral) {
                    if let Some(repaired) = repair_mistral_plan_for_tool_turn(
                        None,
                        None,
                        tc,
                        &root_user_text,
                        root_read_only,
                    ) {
                        if validate_plan_for_task_contract(
                            &repaired,
                            root_read_only,
                            &task_contract,
                            &instruction_resolver,
                            &*messages,
                        )
                        .is_ok()
                        {
                            let _ = tx
                                .send(StreamToken::Delta(
                                    "\n[compat] synthesized Mistral read-only observation plan\n"
                                        .to_string(),
                                ))
                                .await;
                            repaired
                        } else {
                            *state = AgentState::Recovery;
                            recovery.stage = Some(RecoveryStage::Diagnose);
                            let block = governor_contract::missing_plan_message();

                            emit_governor_block(
                                governor_emit(
                                    &tx,
                                    &mut *messages,
                                    &autosaver,
                                    &tool_root_abs,
                                    &checkpoint,
                                    &cur_cwd,
                                    &mut *pending_system_hint,
                                    *state,
                                    &*recovery,
                                    &mem,
                                    file_tool_consec_failures,
                                    last_mutation_step,
                                    *last_verify_ok_step,
                                    last_reflection.as_ref(),
                                ),
                                block,
                                &*tc,
                                Some(&assistant_text_clean),
                            )
                            .await;
                            return Ok(PlanThinkOut {
                                flow: PlanThinkFlow::Continue,
                                plan: None,
                                think: None,
                            });
                        }
                    } else {
                        *state = AgentState::Recovery;
                        recovery.stage = Some(RecoveryStage::Diagnose);
                        let block = governor_contract::missing_plan_message();

                        emit_governor_block(
                            governor_emit(
                                &tx,
                                &mut *messages,
                                &autosaver,
                                &tool_root_abs,
                                &checkpoint,
                                &cur_cwd,
                                &mut *pending_system_hint,
                                *state,
                                &*recovery,
                                &mem,
                                file_tool_consec_failures,
                                last_mutation_step,
                                *last_verify_ok_step,
                                last_reflection.as_ref(),
                            ),
                            block,
                            &*tc,
                            Some(&assistant_text_clean),
                        )
                        .await;
                        return Ok(PlanThinkOut {
                            flow: PlanThinkFlow::Continue,
                            plan: None,
                            think: None,
                        });
                    }
                } else {
                    *state = AgentState::Recovery;
                    recovery.stage = Some(RecoveryStage::Diagnose);
                    let block = governor_contract::missing_plan_message();

                    emit_governor_block(
                        governor_emit(
                            &tx,
                            &mut *messages,
                            &autosaver,
                            &tool_root_abs,
                            &checkpoint,
                            &cur_cwd,
                            &mut *pending_system_hint,
                            *state,
                            &*recovery,
                            &mem,
                            file_tool_consec_failures,
                            last_mutation_step,
                            *last_verify_ok_step,
                            last_reflection.as_ref(),
                        ),
                        block,
                        &*tc,
                        Some(&assistant_text_clean),
                    )
                    .await;
                    return Ok(PlanThinkOut {
                        flow: PlanThinkFlow::Continue,
                        plan: None,
                        think: None,
                    });
                }
            }
        },
    };

    if active_plan.is_none() && root_read_only && mistral_observation_tool(tc.name.as_str()) {
        adopt_valid_plan(
            &candidate_plan,
            &mut *working_mem,
            &mut *assumption_ledger,
            &mut *active_plan,
            &mut *intent_required_verification,
            path_required_verification,
            &mut *required_verification,
            &mut *recovery,
            &mut *last_verify_ok_step,
            last_build_verify_ok_step,
            last_behavioral_verify_ok_step,
        );
    }

    if root_read_only
        && tc.name.as_str() != "done"
        && matches!(
            tc.name.as_str(),
            "search_files" | "read_file" | "list_dir" | "glob"
        )
        && !observation_evidence.reads.is_empty()
    {
        if let Some(final_text) = build_read_only_strong_final_answer(
            &root_user_text,
            &candidate_plan,
            &observation_evidence,
            &*messages,
            &*working_mem,
        ) {
            task_outcome.finalized(&final_text);
            *state = AgentState::Done;
            let final_text = enrich_text_final_handoff(
                final_text.as_str(),
                &root_user_text,
                &*messages,
                test_cmd.as_deref(),
            )
            .unwrap_or(final_text);
            messages.push(json!({"role": "assistant", "content": final_text.clone()}));
            autosave_best_effort(
                &autosaver,
                &tx,
                tool_root_abs.as_deref(),
                checkpoint.as_deref(),
                cur_cwd.as_deref(),
                &*messages,
            )
            .await;
            let _ = tx
                .send(StreamToken::GovernorState(build_governor_state(
                    *state,
                    &*recovery,
                    &mem,
                    file_tool_consec_failures,
                    last_mutation_step,
                    *last_verify_ok_step,
                    last_reflection.as_ref(),
                )))
                .await;
            let _ = tx
            .send(StreamToken::Delta(format!(
                "\n[agent] strong read-only evidence already satisfied the task; auto-finalized instead of executing {}.\n\n{final_text}\n",
                tc.name
            )))
            .await;
            return Ok(PlanThinkOut {
                flow: PlanThinkFlow::Break,
                plan: None,
                think: None,
            });
        }
    }

    let known_acceptance_commands = canonicalize_known_acceptance_commands(
        &collect_known_acceptance_commands(&*messages, &*working_mem, test_cmd.as_deref()),
        &observation_evidence,
    );
    if benchmark_plan_pending_required_exec_command(
    &root_user_text,
    &*messages,
    test_cmd.as_deref(),
)
.is_none()
    && !recovery.focus.is_pending()
    // An intermediate passing test cannot finish an explicitly
    // authored task or prevent its next diagnostic/repair step.
    && !requires_authored_final_answer(&root_user_text)
    && should_prefer_done_after_verified_action(
        tc,
        &candidate_plan,
        &known_acceptance_commands,
        *required_verification,
        test_cmd.as_deref(),
        last_mutation_step,
        *last_verify_ok_step,
    ) {
        if let Some(final_text) = maybe_build_verified_action_auto_final_answer(
            Some(&candidate_plan),
            &*messages,
            &known_acceptance_commands,
            &observation_evidence,
            *required_verification,
            test_cmd.as_deref(),
            last_mutation_step,
            *last_verify_ok_step,
        )
        .filter(|_| {
            !requires_authored_final_answer(&root_user_text)
                && exact_content::validate_exact_content_task(
                    &root_user_text,
                    tool_root_abs.as_deref(),
                )
                .is_ok()
        }) {
            task_outcome.finalized(&final_text);
            *state = AgentState::Done;
            messages.push(json!({"role": "assistant", "content": final_text.clone()}));
            autosave_best_effort(
                &autosaver,
                &tx,
                tool_root_abs.as_deref(),
                checkpoint.as_deref(),
                cur_cwd.as_deref(),
                &*messages,
            )
            .await;
            let _ = tx
                .send(StreamToken::GovernorState(build_governor_state(
                    *state,
                    &*recovery,
                    &mem,
                    file_tool_consec_failures,
                    last_mutation_step,
                    *last_verify_ok_step,
                    last_reflection.as_ref(),
                )))
                .await;
            let _ = tx
            .send(StreamToken::Delta(format!(
                "\n[agent] auto-finalized verified action instead of executing redundant post-verify {}.\n\n{final_text}\n",
                tc.name
            )))
            .await;
            return Ok(PlanThinkOut {
                flow: PlanThinkFlow::Break,
                plan: None,
                think: None,
            });
        }

        *state = AgentState::Recovery;
        let mut block = build_post_verify_done_completion_hint(
            &candidate_plan,
            &known_acceptance_commands,
            tc,
            *required_verification,
            test_cmd.as_deref(),
        );
        if let Some(hint) = authored_final_answer_hint(&root_user_text) {
            block.push_str("\n\n");
            block.push_str(&hint);
        }

        emit_governor_block(
            governor_emit(
                &tx,
                &mut *messages,
                &autosaver,
                &tool_root_abs,
                &checkpoint,
                &cur_cwd,
                &mut *pending_system_hint,
                *state,
                &*recovery,
                &mem,
                file_tool_consec_failures,
                last_mutation_step,
                *last_verify_ok_step,
                last_reflection.as_ref(),
            ),
            block,
            &*tc,
            Some(&assistant_text_clean),
        )
        .await;
        return Ok(PlanThinkOut {
            flow: PlanThinkFlow::Continue,
            plan: None,
            think: None,
        });
    }

    let compat_synth_think = rescue_read_only_missing_think_for_tool_turn(
        &*messages,
        tc,
        &candidate_plan,
        &root_user_text,
        root_read_only,
        cfg.provider.clone(),
        &observation_evidence,
    )
    .or_else(|| {
        rescue_missing_think_for_tool_turn(
            &*messages,
            tc,
            &candidate_plan,
            root_read_only,
            goal_wants_actions,
            cfg.provider.clone(),
            used_general_plan_rescue,
        )
    });

    let (think, used_compat_synth) = select_think_for_tool_turn(
        parsed_think.as_ref(),
        compat_last_think.as_ref(),
        compat_synth_think.as_ref(),
        &candidate_plan,
        tc,
        cfg.provider.clone(),
    );

    let think = match think {
        Some(think) => think,
        None => {
            *state = AgentState::Recovery;
            recovery.stage = Some(RecoveryStage::Diagnose);
            let block = governor_contract::missing_think_message();

            emit_governor_block(
                governor_emit(
                    &tx,
                    &mut *messages,
                    &autosaver,
                    &tool_root_abs,
                    &checkpoint,
                    &cur_cwd,
                    &mut *pending_system_hint,
                    *state,
                    &*recovery,
                    &mem,
                    file_tool_consec_failures,
                    last_mutation_step,
                    *last_verify_ok_step,
                    last_reflection.as_ref(),
                ),
                block,
                &*tc,
                Some(&assistant_text_clean),
            )
            .await;
            return Ok(PlanThinkOut {
                flow: PlanThinkFlow::Continue,
                plan: None,
                think: None,
            });
        }
    };

    if used_compat_synth {
        let _ = tx
            .send(StreamToken::Delta(
                "\n[compat] synthesized think block for the rescued tool turn\n".to_string(),
            ))
            .await;
    }

    let resolved_think = observer_observation
        .and_then(observer_continuation::Observation::command)
        .filter(|_| think.next == "@observer")
        .map(|command| ThinkBlock {
            next: command.into(),
            ..think.clone()
        });
    let think = resolved_think.as_ref().unwrap_or(think);
    if let Err(e) = validate_think(think, &candidate_plan, tc) {
        *state = AgentState::Recovery;
        recovery.stage = Some(RecoveryStage::Diagnose);
        let block = governor_contract::invalid_think_message(&e.to_string());

        emit_governor_block(
            governor_emit(
                &tx,
                &mut *messages,
                &autosaver,
                &tool_root_abs,
                &checkpoint,
                &cur_cwd,
                &mut *pending_system_hint,
                *state,
                &*recovery,
                &mem,
                file_tool_consec_failures,
                last_mutation_step,
                *last_verify_ok_step,
                last_reflection.as_ref(),
            ),
            block,
            &*tc,
            Some(&assistant_text_clean),
        )
        .await;
        return Ok(PlanThinkOut {
            flow: PlanThinkFlow::Continue,
            plan: None,
            think: None,
        });
    }

    if let Some(conflict) =
        refuted_assumption_conflict(&*assumption_ledger, think, tc, &exec_verification_context)
    {
        *state = AgentState::Recovery;
        recovery.stage = Some(RecoveryStage::Diagnose);
        let block = format!(
        "[Assumption Ledger] {conflict}\nGather new evidence or choose a different next action before retrying."
    );

        emit_governor_block(
            governor_emit(
                &tx,
                &mut *messages,
                &autosaver,
                &tool_root_abs,
                &checkpoint,
                &cur_cwd,
                &mut *pending_system_hint,
                *state,
                &*recovery,
                &mem,
                file_tool_consec_failures,
                last_mutation_step,
                *last_verify_ok_step,
                last_reflection.as_ref(),
            ),
            block,
            &*tc,
            Some(&assistant_text_clean),
        )
        .await;
        return Ok(PlanThinkOut {
            flow: PlanThinkFlow::Continue,
            plan: None,
            think: None,
        });
    }

    if let Some(conflict) = instruction_resolver.tool_conflict(tc, test_cmd.as_deref()) {
        *state = AgentState::Recovery;
        recovery.stage = Some(RecoveryStage::Diagnose);
        let block = conflict.render();

        emit_governor_block(
            governor_emit(
                &tx,
                &mut *messages,
                &autosaver,
                &tool_root_abs,
                &checkpoint,
                &cur_cwd,
                &mut *pending_system_hint,
                *state,
                &*recovery,
                &mem,
                file_tool_consec_failures,
                last_mutation_step,
                *last_verify_ok_step,
                last_reflection.as_ref(),
            ),
            block,
            &*tc,
            Some(&assistant_text_clean),
        )
        .await;
        return Ok(PlanThinkOut {
            flow: PlanThinkFlow::Continue,
            plan: None,
            think: None,
        });
    }

    if let Some(block) = build_progress_gate_block(
        task_harness,
        tc,
        &*messages,
        recovery.stage,
        test_cmd.as_deref(),
        last_mutation_step,
        file_tool_consec_failures,
    ) {
        *state = AgentState::Recovery;
        emit_governor_block(
            governor_emit(
                &tx,
                &mut *messages,
                &autosaver,
                &tool_root_abs,
                &checkpoint,
                &cur_cwd,
                &mut *pending_system_hint,
                *state,
                &*recovery,
                &mem,
                file_tool_consec_failures,
                last_mutation_step,
                *last_verify_ok_step,
                last_reflection.as_ref(),
            ),
            block,
            &*tc,
            Some(&assistant_text_clean),
        )
        .await;
        return Ok(PlanThinkOut {
            flow: PlanThinkFlow::Continue,
            plan: None,
            think: None,
        });
    }

    if let Some(block) = evaluator_loop
        .build_violation_block(tc)
        .filter(|_| !requirement_proof::is_pending_baseline(tc, &*messages, test_cmd.as_deref()))
    {
        *state = AgentState::Recovery;
        emit_governor_block(
            governor_emit(
                &tx,
                &mut *messages,
                &autosaver,
                &tool_root_abs,
                &checkpoint,
                &cur_cwd,
                &mut *pending_system_hint,
                *state,
                &*recovery,
                &mem,
                file_tool_consec_failures,
                last_mutation_step,
                *last_verify_ok_step,
                last_reflection.as_ref(),
            ),
            block,
            &*tc,
            Some(&assistant_text_clean),
        )
        .await;
        return Ok(PlanThinkOut {
            flow: PlanThinkFlow::Continue,
            plan: None,
            think: None,
        });
    }

    if mutation_tool_requires_evidence(tc) {
        let observations = &observation_evidence;
        let evidence_block = match parse_evidence_block(&assistant_text) {
            Some(block) => block,
            None => {
                if let Some(block) = rescue_missing_evidence_for_tool_turn(
                    tc,
                    root_read_only,
                    goal_wants_actions,
                    cfg.provider.clone(),
                    observations,
                ) {
                    let _ = tx
                        .send(StreamToken::Delta(
                            "\n[compat] synthesized evidence block for the rescued mutation turn\n"
                                .to_string(),
                        ))
                        .await;
                    block
                } else {
                    *state = AgentState::Recovery;
                    recovery.stage = Some(RecoveryStage::Diagnose);
                    let block = build_evidence_gate_prompt(tc, &observations, &*assumption_ledger);
                    emit_governor_block(
                        governor_emit(
                            &tx,
                            &mut *messages,
                            &autosaver,
                            &tool_root_abs,
                            &checkpoint,
                            &cur_cwd,
                            &mut *pending_system_hint,
                            *state,
                            &*recovery,
                            &mem,
                            file_tool_consec_failures,
                            last_mutation_step,
                            *last_verify_ok_step,
                            last_reflection.as_ref(),
                        ),
                        block,
                        &*tc,
                        Some(&assistant_text_clean),
                    )
                    .await;
                    return Ok(PlanThinkOut {
                        flow: PlanThinkFlow::Continue,
                        plan: None,
                        think: None,
                    });
                }
            }
        };
        if let Err(e) = validate_evidence_block(&evidence_block, tc, &observations) {
            *state = AgentState::Recovery;
            recovery.stage = Some(RecoveryStage::Diagnose);
            let mut block = format!(
                "{}\n\n{}",
                governor_contract::evidence_invalid_message(&e.to_string()),
                build_evidence_gate_prompt(tc, &observations, &*assumption_ledger)
            );
            if !evidence_block.target_symbols.is_empty() {
                block.push_str("\n\nCurrent target_symbols:\n");
                for symbol in &evidence_block.target_symbols {
                    block.push_str("- ");
                    block.push_str(symbol);
                    block.push('\n');
                }
            }
            emit_governor_block(
                governor_emit(
                    &tx,
                    &mut *messages,
                    &autosaver,
                    &tool_root_abs,
                    &checkpoint,
                    &cur_cwd,
                    &mut *pending_system_hint,
                    *state,
                    &*recovery,
                    &mem,
                    file_tool_consec_failures,
                    last_mutation_step,
                    *last_verify_ok_step,
                    last_reflection.as_ref(),
                ),
                block,
                &*tc,
                Some(&assistant_text_clean),
            )
            .await;
            return Ok(PlanThinkOut {
                flow: PlanThinkFlow::Continue,
                plan: None,
                think: None,
            });
        }
    }

    // The old plan can validate the tool turn without becoming its work plan.
    // Adoption, step labels and accepted_plan snapshots need a work plan.
    validated_plan_for_turn = (!inherited_closeout).then_some(candidate_plan);
    validated_think_for_turn = Some(think.clone());
    Ok(PlanThinkOut {
        flow: PlanThinkFlow::Validated,
        plan: validated_plan_for_turn,
        think: validated_think_for_turn,
    })
}
