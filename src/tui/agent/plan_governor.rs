use super::*;

pub(crate) fn adopt_valid_plan(
    plan: &PlanBlock,
    working_mem: &mut WorkingMemory,
    assumption_ledger: &mut AssumptionLedger,
    active_plan: &mut Option<PlanBlock>,
    intent_required_verification: &mut VerificationLevel,
    path_required_verification: VerificationLevel,
    required_verification: &mut VerificationLevel,
    recovery: &mut RecoveryGovernor,
    last_verify_ok_step: &mut Option<usize>,
    last_build_verify_ok_step: Option<usize>,
    last_behavioral_verify_ok_step: Option<usize>,
) {
    working_mem.sync_to_plan(plan);
    assumption_ledger.sync_to_plan(plan);
    assumption_ledger.refresh_confirmations(working_mem);
    if let Some(level) = verification_level_from_plan(plan) {
        *intent_required_verification = level;
        *required_verification = (*intent_required_verification).max(path_required_verification);
        recovery.required_verification = *required_verification;
        *last_verify_ok_step = effective_verify_ok_step(
            *required_verification,
            last_build_verify_ok_step,
            last_behavioral_verify_ok_step,
        );
    }
    *active_plan = Some(plan.clone());
}

pub(crate) fn last_impact_step_from_messages(messages: &[serde_json::Value]) -> Option<usize> {
    let mut step_seq = 0usize;
    let mut last_impact_step = None;

    for msg in messages {
        let role = msg.get("role").and_then(|v| v.as_str()).unwrap_or("");
        if role == "assistant" {
            let content = msg.get("content").and_then(|c| c.as_str()).unwrap_or("");
            if parse_impact_block(content).is_some() {
                last_impact_step = Some(step_seq);
            }
            continue;
        }

        if role == "tool" {
            step_seq = step_seq.saturating_add(1);
        }
    }

    last_impact_step
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn validate_plan_for_task(plan: &PlanBlock, root_read_only: bool) -> Result<()> {
    validate_plan(plan)?;
    if root_read_only {
        if let Some(msg) = read_only_plan_violation(plan) {
            return Err(anyhow!(msg));
        }
    }
    Ok(())
}

pub(crate) fn validate_plan_against_instruction_resolver(
    plan: &PlanBlock,
    resolver: &InstructionResolver,
) -> Result<()> {
    if let Some(conflict) = resolver.plan_conflict(plan) {
        return Err(anyhow!(conflict.render()));
    }
    Ok(())
}

pub(crate) fn validate_plan_for_task_contract(
    plan: &PlanBlock,
    root_read_only: bool,
    contract: &TaskContract,
    resolver: &InstructionResolver,
    messages: &[serde_json::Value],
) -> Result<()> {
    validate_plan(plan)?;
    plan_revision::validate_alignment(plan, messages, contract)?;
    if root_read_only {
        validate_plan_against_instruction_resolver(plan, resolver)?;
    }
    Ok(())
}

pub(crate) fn think_next_matches_exec_command(next: &str, command: &str) -> bool {
    let next_sig = command_sig(next);
    let cmd_sig = command_sig(command);
    if next_sig.is_empty() || cmd_sig.is_empty() {
        return false;
    }
    if cmd_sig.contains(&next_sig) || next_sig.contains(&cmd_sig) {
        return true;
    }

    let next_prefix = next_sig
        .split_whitespace()
        .take(2)
        .collect::<Vec<_>>()
        .join(" ");
    let cmd_prefix = cmd_sig
        .split_whitespace()
        .take(2)
        .collect::<Vec<_>>()
        .join(" ");
    !next_prefix.is_empty() && next_prefix == cmd_prefix
}

pub(crate) fn build_reflection_prompt(
    reason: &str,
    mem: &FailureMemory,
    state: AgentState,
    file_tool_consec_failures: usize,
) -> String {
    format!(
        "[Self Reflection Required]\n\
Reason: {reason}\n\
State: {:?}\n\
Failure memory:\n\
- consecutive_failures: {}\n\
- same_command_repeats: {}\n\
- same_error_repeats: {}\n\
- same_output_repeats: {}\n\
- file_tool_consec_failures: {}\n\
\n\
Before your next tool call, emit exactly ONE <reflect> block:\n\
<reflect>\n\
last_outcome: success|failure|partial\n\
goal_delta: closer|same|farther\n\
wrong_assumption: <one short sentence>\n\
strategy_change: keep|adjust|abandon\n\
next_minimal_action: <one short sentence>\n\
</reflect>\n\
\n\
Rules:\n\
- One line per field.\n\
- Keep the whole block under 80 tokens.\n\
- If the same error/command/output repeated, strategy_change cannot be `keep`.\n\
- If file_tool_consec_failures >= 2, strategy_change cannot be `keep`.\n\
- If goal_delta is `same` or `farther`, choose a materially different next action.\n\
- After the <reflect> block: emit your normal <think> block (required), then call exactly one tool. Do not add prose.",
        state,
        mem.consecutive_failures,
        mem.same_command_repeats,
        mem.same_error_repeats,
        mem.same_output_repeats,
        file_tool_consec_failures
    )
}

/// Reflection system prompt plus the two most recent reflection summaries
/// from the transcript, so a re-triggered model sees its own history and
/// cannot repeat the same mistake verbatim.
pub(crate) fn build_reflection_prompt_with_recent(
    reason: &str,
    mem: &FailureMemory,
    state: AgentState,
    file_tool_consec_failures: usize,
    messages: &[serde_json::Value],
) -> String {
    let mut prompt = build_reflection_prompt(reason, mem, state, file_tool_consec_failures);
    let recent = crate::agent_session::recent_reflection_summaries_from_messages(messages, 2);
    if !recent.is_empty() {
        prompt.push_str("\n\n[Recent Reflections]\n");
        for r in recent.iter().rev() {
            let delta = r.goal_delta.as_deref().unwrap_or("-");
            let strat = r.strategy_change.as_deref().unwrap_or("-");
            let wrong = compact_one_line(r.wrong_assumption.as_deref().unwrap_or("-"), 90);
            let next = compact_one_line(r.next_minimal_action.as_deref().unwrap_or("-"), 90);
            prompt.push_str(&format!(
                "- delta={delta} strategy={strat} wrong={wrong} next={next}\n"
            ));
        }
        prompt.push_str("Do not repeat the same mistake verbatim.\n");
    }
    prompt
}

pub(crate) fn build_governor_state(
    state: AgentState,
    recovery: &RecoveryGovernor,
    mem: &FailureMemory,
    file_tool_consec_failures: usize,
    last_mutation_step: Option<usize>,
    last_verify_ok_step: Option<usize>,
    last_reflection: Option<&ReflectionBlock>,
) -> GovernorState {
    let done_verify_required = last_mutation_step.unwrap_or(0) > last_verify_ok_step.unwrap_or(0);
    let recovery_stage = if recovery.in_recovery() {
        Some(recovery.stage_label().to_string())
    } else {
        None
    };
    let last_reflection = last_reflection.map(|r| ReflectionSummary {
        last_outcome: if r.last_outcome.trim().is_empty() {
            None
        } else {
            Some(r.last_outcome.clone())
        },
        goal_delta: Some(r.goal_delta.as_str().to_string()),
        wrong_assumption: if r.wrong_assumption.trim().is_empty() {
            None
        } else {
            Some(r.wrong_assumption.clone())
        },
        strategy_change: Some(r.strategy_change.as_str().to_string()),
        next_minimal_action: if r.next_minimal_action.trim().is_empty() {
            None
        } else {
            Some(r.next_minimal_action.clone())
        },
    });

    GovernorState {
        state: format!("{state:?}").to_ascii_lowercase(),
        recovery_stage,

        consecutive_failures: mem.consecutive_failures,
        same_command_repeats: mem.same_command_repeats,
        same_error_repeats: mem.same_error_repeats,
        same_output_repeats: mem.same_output_repeats,
        file_tool_consec_failures,

        done_verify_required,
        last_mutation_step,
        last_verify_ok_step,

        last_reflection,
    }
}

pub(crate) fn has_project_rules_context(messages: &[serde_json::Value]) -> bool {
    messages.iter().any(|msg| {
        if msg.get("role").and_then(|v| v.as_str()) != Some("system") {
            return false;
        }
        let Some(content) = msg.get("content").and_then(|v| v.as_str()) else {
            return false;
        };
        governor_contract::instruction_resolver_project_rule_markers()
            .iter()
            .any(|marker| !marker.is_empty() && content.contains(marker))
    })
}

pub(crate) fn wants_local_actions(user_text: &str) -> bool {
    let s = user_text.to_ascii_lowercase();
    let kws = [
        "repo",
        "repository",
        "scaffold",
        "bootstrap",
        "init",
        "setup",
        "create",
        "generate",
        "implement",
        "build",
        "test",
        "run",
        "install",
        "folder",
        "directory",
        "file",
        "git",
        "commit",
        "push",
        // Japanese
        "リポ",
        "リポジトリ",
        "フォルダ",
        "ディレクトリ",
        "ファイル",
        "作成",
        "作る",
        "実装",
        "実行",
        "コミット",
        "プッシュ",
        // French
        "dépôt",
        "depot",
        "répertoire",
        "repertoire",
        "fichier",
        "commande",
        "créer",
        "creer",
        "générer",
        "generer",
        "exécuter",
        "executer",
        "installer",
    ];
    kws.iter().any(|k| s.contains(k))
}

#[derive(Debug, Clone)]
pub(crate) struct ImpliedExecScript {
    pub(crate) lang_hint: String,
    pub(crate) script: String,
}

#[derive(Debug, Clone)]
pub(crate) struct ImpliedWriteFile {
    pub(crate) path: String,
    pub(crate) lang_hint: String,
    pub(crate) content: String,
}
