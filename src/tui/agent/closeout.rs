use super::*;

/// Borrowed loop state consumed by the post-iteration epilogue: verified-action
/// auto-finalize, session-end latent-plan realization, outcome telemetry, and
/// the final `AgenticEndState` assembly.
pub(crate) struct Closeout<'a> {
    pub(crate) tx: &'a mpsc::Sender<StreamToken>,
    pub(crate) messages: &'a mut Vec<serde_json::Value>,
    pub(crate) task_outcome: &'a mut outcome::TaskOutcome,
    pub(crate) observer_handoff: &'a Option<observer_handoff::Failure>,
    pub(crate) blocked_terminal: bool,
    pub(crate) state: AgentState,
    pub(crate) root_user_text: &'a str,
    pub(crate) tool_root_abs: &'a Option<String>,
    pub(crate) checkpoint: &'a Option<String>,
    pub(crate) cur_cwd: &'a Option<String>,
    pub(crate) autosaver: &'a Option<Arc<crate::agent_session::SessionAutoSaver>>,
    pub(crate) test_cmd: Option<&'a str>,
    pub(crate) active_plan: &'a mut Option<PlanBlock>,
    pub(crate) working_mem: &'a mut WorkingMemory,
    pub(crate) assumption_ledger: &'a mut AssumptionLedger,
    pub(crate) observation_evidence: &'a ObservationEvidence,
    pub(crate) intent_required_verification: &'a mut VerificationLevel,
    pub(crate) path_required_verification: VerificationLevel,
    pub(crate) required_verification: &'a mut VerificationLevel,
    pub(crate) recovery: &'a mut RecoveryGovernor,
    pub(crate) last_mutation_step: Option<usize>,
    pub(crate) last_verify_ok_step: &'a mut Option<usize>,
    pub(crate) last_build_verify_ok_step: Option<usize>,
    pub(crate) last_behavioral_verify_ok_step: Option<usize>,
    pub(crate) realize_cfg: &'a RealizeOnDemandConfig,
    pub(crate) latent_plan: &'a mut Option<LatentPlanBuffer>,
    pub(crate) realize_metrics: &'a mut RealizeMetrics,
    pub(crate) max_iters: usize,
}

pub(crate) async fn run_closeout(c: Closeout<'_>) -> AgenticEndState {
    if let Some(final_text) = done_gate::maybe_build_verified_action_closeout_text(
        c.root_user_text,
        c.tool_root_abs.as_deref(),
        c.active_plan.as_ref(),
        c.messages,
        c.working_mem,
        c.observation_evidence,
        *c.required_verification,
        c.test_cmd,
        c.last_mutation_step,
        *c.last_verify_ok_step,
    )
    .filter(|_| c.observer_handoff.is_none() && !c.blocked_terminal)
    {
        c.task_outcome.finalized(&final_text);
        let final_text = enrich_text_final_handoff(
            final_text.as_str(),
            c.root_user_text,
            c.messages,
            c.test_cmd,
        )
        .unwrap_or(final_text);
        c.messages
            .push(json!({"role": "assistant", "content": final_text.clone()}));
        autosave_best_effort(
            c.autosaver,
            c.tx,
            c.tool_root_abs.as_deref(),
            c.checkpoint.as_deref(),
            c.cur_cwd.as_deref(),
            c.messages,
        )
        .await;
        let _ =
            c.tx.send(StreamToken::Delta(format!(
                "\n[agent] auto-finalized verified action task at session end.\n\n{final_text}\n"
            )))
            .await;
    }

    if c.realize_cfg.enabled {
        if let Some(latent) = c.latent_plan.take() {
            let latency = c.max_iters.saturating_sub(latent.created_iter);
            let drift = drift_distance(
                c.realize_cfg.drift_metric,
                &latent.summary,
                &latent.anchor_baseline,
            );
            c.realize_metrics.missing += 1;
            c.realize_metrics.realize_count += 1;
            c.realize_metrics.total_realize_latency += latency;
            adopt_valid_plan(
                &latent.plan,
                c.working_mem,
                c.assumption_ledger,
                c.active_plan,
                c.intent_required_verification,
                c.path_required_verification,
                c.required_verification,
                c.recovery,
                c.last_verify_ok_step,
                c.last_build_verify_ok_step,
                c.last_behavioral_verify_ok_step,
            );
            c.messages
                .push(json!({"role": "assistant", "content": latent.raw_text}));
            let _ =
                c.tx.send(StreamToken::Delta(build_realize_banner(
                    "session_end",
                    latency,
                    drift,
                    c.realize_metrics,
                )))
                .await;
            autosave_best_effort(
                c.autosaver,
                c.tx,
                c.tool_root_abs.as_deref(),
                c.checkpoint.as_deref(),
                c.cur_cwd.as_deref(),
                c.messages,
            )
            .await;
            emit_realize_state(
                c.tx,
                c.realize_cfg,
                None,
                c.max_iters,
                None,
                c.realize_metrics,
            )
            .await;
        }
    }

    emit_telemetry_event(
        c.tx,
        "agent_outcome",
        json!({
            "completed": !c.blocked_terminal && c.observer_handoff.is_none() && c.task_outcome.completed(c.messages),
            "state": format!("{:?}", c.state).to_ascii_lowercase(),
            "yield_reason": if c.blocked_terminal { Some(super::super::coder_yield::CoderYieldReason::Blocked) } else { c.observer_handoff.as_ref().map(|_| super::super::coder_yield::CoderYieldReason::VerificationFailure) },
        }),
    )
    .await;
    let _ = c.tx.send(StreamToken::Done).await;
    AgenticEndState {
        messages: std::mem::take(c.messages),
        checkpoint: c.checkpoint.clone(),
        cur_cwd: c.cur_cwd.clone(),
        observation_cache: Some(c.observation_evidence.to_session_cache()),
    }
}
