use super::*;

pub(crate) fn resolve_step_label(
    plan: Option<&PlanBlock>,
    think: Option<&ThinkBlock>,
) -> Option<String> {
    let plan = plan?;
    let think = think?;
    think
        .step
        .checked_sub(1)
        .and_then(|idx| plan.steps.get(idx))
        .cloned()
}

pub(crate) fn extract_cwd_from_tool_output(content: &str) -> Option<String> {
    for line in content.lines() {
        if let Some(rest) = line.strip_prefix("cwd_after:") {
            let value = rest.trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    for line in content.lines() {
        if let Some(rest) = line.strip_prefix("cwd:") {
            let value = rest.trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

pub(crate) fn first_non_empty_line(s: &str) -> Option<&str> {
    s.lines().map(str::trim).find(|line| !line.is_empty())
}

pub(crate) fn working_memory_facts_for_exec(
    command: &str,
    stdout: &str,
    tool_output: &str,
) -> Vec<String> {
    let mut facts: Vec<String> = Vec::new();
    if let Some(cwd) = extract_cwd_from_tool_output(tool_output) {
        facts.push(format!("cwd: {cwd}"));
    }

    let sig = command_sig(command);
    if sig.contains("git rev-parse") {
        if let Some(line) = first_non_empty_line(stdout) {
            facts.push(format!("git top-level: {}", compact_one_line(line, 120)));
        }
    }
    if sig.contains("git status") {
        let low = stdout.to_ascii_lowercase();
        if low.contains("nothing to commit, working tree clean") {
            facts.push("git working tree: clean".to_string());
        } else if low.contains("changes not staged")
            || low.contains("changes to be committed")
            || low.contains("untracked files")
        {
            facts.push("git working tree: dirty".to_string());
        }
    }

    facts
}

pub(crate) fn non_exec_tool_succeeded(content: &str) -> bool {
    let low = content.to_ascii_lowercase();
    !low.starts_with("error:")
        && !low.starts_with("ok: unchanged ")
        && !low.contains("rejected by user")
        && !low.contains("governor blocked")
        && !low.contains("failed (exit_code:")
}

pub(crate) fn update_working_memory_after_exec(
    mem: &mut WorkingMemory,
    command: &str,
    stdout: &str,
    tool_output: &str,
    exec_kind: ExecKind,
    step_label: Option<&str>,
    next_action: Option<&str>,
) {
    if let Some(next) = next_action {
        mem.set_strategy(next);
    }
    for fact in working_memory_facts_for_exec(command, stdout, tool_output) {
        mem.remember_fact(fact.as_str());
    }
    if exec_kind == ExecKind::Verify {
        mem.remember_successful_verification(command);
    }
    if matches!(exec_kind, ExecKind::Diagnostic | ExecKind::Verify) {
        if let Some(step) = step_label {
            mem.remember_completed_step(step);
        }
    }
}

pub(crate) fn update_working_memory_after_non_exec(
    mem: &mut WorkingMemory,
    tool_name: &str,
    verified: bool,
    step_label: Option<&str>,
    next_action: Option<&str>,
    test_cmd: Option<&str>,
) {
    if let Some(next) = next_action {
        mem.set_strategy(next);
    }
    if matches!(
        tool_name,
        "read_file"
            | "list_dir"
            | "glob"
            | "search_files"
            | "write_file"
            | "patch_file"
            | "apply_diff"
    ) {
        if let Some(step) = step_label {
            mem.remember_completed_step(step);
        }
    }
    if verified {
        mem.remember_successful_verification(test_cmd.unwrap_or("post-edit auto-test"));
    }
}

pub(crate) fn build_working_memory_prompt(mem: &WorkingMemory) -> Option<String> {
    if mem.is_empty() {
        return None;
    }

    let mut out = String::from("[Working Memory]\n");

    if let Some(strategy) = mem
        .chosen_strategy
        .as_deref()
        .filter(|s| !s.trim().is_empty())
    {
        out.push_str(&format!("Current strategy:\n- {strategy}\n"));
    }
    if !mem.facts.is_empty() {
        out.push_str("Confirmed facts:\n");
        for fact in &mem.facts {
            out.push_str(&format!("- {fact}\n"));
        }
    }
    if !mem.completed_steps.is_empty() {
        out.push_str("Completed steps:\n");
        for step in &mem.completed_steps {
            out.push_str(&format!("- {step}\n"));
        }
    }
    if !mem.successful_verifications.is_empty() {
        out.push_str("Known-good verification commands:\n");
        for verify in &mem.successful_verifications {
            out.push_str(&format!("- {verify}\n"));
        }
    }
    out.push_str(
        "Use this memory to avoid redoing solved work. Re-check only when new tool output contradicts it.",
    );
    Some(out)
}

pub(crate) fn render_cached_prompt(
    slot: &mut Option<u64>,
    full: String,
    compact: String,
) -> String {
    let digest = hash_text(full.as_str());
    let unchanged = slot.replace(digest) == Some(digest);
    if unchanged {
        compact
    } else {
        full
    }
}

pub(crate) fn build_instruction_resolver_compact_prompt(
    resolver: &InstructionResolver,
    required_verification: VerificationLevel,
) -> String {
    let full = resolver.prompt(required_verification);
    [
        "[Instruction Resolver cache]".to_string(),
        format!("hash: {}", fmt_hash(hash_text(full.as_str()))),
        "- order: root > system > project > user > execution".to_string(),
        format!(
            "- task: {}",
            compact_one_line(resolver.user_task_summary(), 120)
        ),
        format!(
            "- read_only: {}",
            if resolver.root_read_only { "yes" } else { "no" }
        ),
        format!(
            "- project_rules: {}",
            if resolver.project_rules_active {
                "yes"
            } else {
                "no"
            }
        ),
        format!("- verification_floor: {}", required_verification.as_str()),
    ]
    .join("\n")
}

pub(crate) fn build_task_contract_compact_prompt(
    contract: &TaskContract,
    active_plan: Option<&PlanBlock>,
    required_verification: VerificationLevel,
) -> String {
    let full = build_task_contract_prompt(contract, active_plan, required_verification);
    let mut lines = vec![
        "[Task Contract cache]".to_string(),
        format!("hash: {}", fmt_hash(hash_text(full.as_str()))),
        format!(
            "- task: {}",
            compact_one_line(contract.task_summary.as_str(), 120)
        ),
        format!(
            "- hard_constraints: {} non_goals: {} output_shape: {}",
            contract.hard_constraints.len(),
            contract.non_goals.len(),
            contract.output_shape.len()
        ),
        format!("- verification_floor: {}", required_verification.as_str()),
    ];
    if let Some(first) = contract.hard_constraints.first() {
        lines.push(format!(
            "- key_constraint: {}",
            compact_one_line(first, 120)
        ));
    }
    if let Some(plan) = active_plan {
        lines.push(format!(
            "- acceptance_items: {}",
            plan.acceptance_criteria.len()
        ));
    }
    lines.join("\n")
}

pub(crate) fn build_working_memory_compact_prompt(mem: &WorkingMemory) -> Option<String> {
    let full = build_working_memory_prompt(mem)?;
    let mut lines = vec![
        "[Working Memory cache]".to_string(),
        format!("hash: {}", fmt_hash(hash_text(full.as_str()))),
        format!(
            "- facts: {} completed_steps: {} verifications: {}",
            mem.facts.len(),
            mem.completed_steps.len(),
            mem.successful_verifications.len()
        ),
    ];
    if let Some(strategy) = mem.chosen_strategy.as_deref() {
        lines.push(format!("- strategy: {}", compact_one_line(strategy, 120)));
    }
    if !mem.completed_steps.is_empty() {
        let recent = mem
            .completed_steps
            .iter()
            .rev()
            .take(2)
            .cloned()
            .collect::<Vec<_>>();
        lines.push(format!(
            "- recent_steps: {}",
            recent.into_iter().rev().collect::<Vec<_>>().join(" | ")
        ));
    }
    Some(lines.join("\n"))
}

pub(crate) fn build_assumption_ledger_compact_prompt(ledger: &AssumptionLedger) -> Option<String> {
    let full = build_assumption_ledger_prompt(ledger)?;
    let open = ledger
        .entries
        .iter()
        .filter(|entry| entry.status == AssumptionStatus::Unknown)
        .count();
    let confirmed = ledger
        .entries
        .iter()
        .filter(|entry| entry.status == AssumptionStatus::Confirmed)
        .count();
    let refuted = ledger
        .entries
        .iter()
        .filter(|entry| entry.status == AssumptionStatus::Refuted)
        .collect::<Vec<_>>();
    let mut lines = vec![
        "[Assumption Ledger cache]".to_string(),
        format!("hash: {}", fmt_hash(hash_text(full.as_str()))),
        format!(
            "- open: {open} confirmed: {confirmed} refuted: {}",
            refuted.len()
        ),
    ];
    for entry in refuted.iter().rev().take(2).rev() {
        lines.push(format!(
            "- refuted: {}",
            compact_one_line(entry.text.as_str(), 120)
        ));
    }
    Some(lines.join("\n"))
}

pub(crate) fn build_verification_requirement_compact_prompt(
    level: VerificationLevel,
    test_cmd: Option<&str>,
    plan: Option<&PlanBlock>,
) -> String {
    let full = verification_requirement_note(level, test_cmd, plan);
    [
        "[Verification Requirement cache]".to_string(),
        format!("hash: {}", fmt_hash(hash_text(full.as_str()))),
        format!("- required: {}", level.as_str()),
        format!(
            "- hint: {}",
            compact_one_line(verification_requirement_hint(level, test_cmd).as_str(), 140)
        ),
        format!(
            "- acceptance_items: {}",
            plan.map(|p| p.acceptance_criteria.len()).unwrap_or(0)
        ),
    ]
    .join(
        "
",
    )
}

pub(crate) fn derive_task_contract(
    root_user_text: &str,
    root_read_only: bool,
    goal_wants_actions: bool,
    required_verification: VerificationLevel,
) -> TaskContract {
    let task_summary = compact_one_line(root_user_text.trim(), 220);
    let mut hard_constraints = vec![
        "Solve the user’s requested task before any cleanup or refactor.".to_string(),
        "Keep edits evidence-backed and scope-bounded.".to_string(),
    ];
    if root_read_only {
        hard_constraints.push("This task is inspection-only: do not edit files.".to_string());
        hard_constraints.push(
            "Do not run build/test/behavior checks just to finish a read-only task.".to_string(),
        );
    } else if !goal_wants_actions {
        hard_constraints.push(
            "If the request is underspecified, inspect first and avoid speculative edits."
                .to_string(),
        );
    }
    match required_verification {
        VerificationLevel::Behavioral => hard_constraints
            .push("Completion requires a real behavioral verification command.".to_string()),
        VerificationLevel::Build => hard_constraints
            .push("Completion requires a real build/check/lint verification.".to_string()),
    }

    let non_goals = vec![
        "Do not broaden scope into unrelated files or prompt/governor rewrites.".to_string(),
        "Do not replace working code without evidence that it is the target.".to_string(),
    ];

    let output_shape = if root_read_only {
        vec![
            "Name the confirmed file path or symbol you located.".to_string(),
            "Summarize the confirmed handling context from observation evidence.".to_string(),
        ]
    } else {
        vec![
            "Keep the final answer tied to changed files, verification, and remaining gaps."
                .to_string(),
            "If unfinished, leave the next exact command/file to continue from.".to_string(),
        ]
    };

    TaskContract {
        task_summary: if task_summary == "-" {
            "complete the requested task".to_string()
        } else {
            task_summary
        },
        hard_constraints,
        non_goals,
        output_shape,
        alignment_terms: Vec::new(),
    }
}

/// Continuation-style roots resume an earlier task instead of stating one, so
/// the plan-drift gate must read task vocabulary from the requests they resume.
pub(crate) fn is_continuation_request(text: &str) -> bool {
    let s = text.to_ascii_lowercase();
    const MARKERS: &[&str] = &[
        "continue from",
        "continue the",
        "continue working",
        "previous session",
        "previous state",
        "resume the",
        "resume from",
        "unfinished task",
        "interrupted session",
        "pick up where",
        "reprends",
        "reprendre",
        "inachev",
        "前回の",
        "未完了",
        "再開",
    ];
    MARKERS.iter().any(|marker| s.contains(marker))
}

pub(crate) fn build_task_contract_prompt(
    contract: &TaskContract,
    active_plan: Option<&PlanBlock>,
    required_verification: VerificationLevel,
) -> String {
    let mut out = String::from("[Task Contract]\n");
    out.push_str("Task summary:\n");
    out.push_str(&format!("- {}\n", contract.task_summary));
    out.push_str("Hard constraints:\n");
    for item in &contract.hard_constraints {
        out.push_str(&format!("- {item}\n"));
    }
    out.push_str("Non-goals:\n");
    for item in &contract.non_goals {
        out.push_str(&format!("- {item}\n"));
    }
    out.push_str("Expected output shape:\n");
    for item in &contract.output_shape {
        out.push_str(&format!("- {item}\n"));
    }
    out.push_str("Verification floor:\n");
    out.push_str(&format!(
        "- {}\n",
        match required_verification {
            VerificationLevel::Build => "real build/check/lint before done",
            VerificationLevel::Behavioral => "real behavioral test before done",
        }
    ));
    if let Some(plan) = active_plan {
        out.push_str("Current plan acceptance:\n");
        for (idx, criterion) in plan.acceptance_criteria.iter().enumerate() {
            out.push_str(&format!("- acceptance {}: {}\n", idx + 1, criterion));
        }
    }
    out.push_str("If the next action would violate this contract, inspect/replan first.");
    out
}

pub(crate) fn validate_plan_against_task_contract(
    plan: &PlanBlock,
    contract: &TaskContract,
) -> Result<()> {
    let mut task_tokens = keyword_tokens(contract.task_summary.as_str());
    for term in &contract.alignment_terms {
        task_tokens.extend(keyword_tokens(term));
    }
    if task_tokens.len() < 3 {
        return Ok(());
    }

    let mut plan_tokens = keyword_tokens(plan.goal.as_str());
    for step in &plan.steps {
        plan_tokens.extend(keyword_tokens(step.as_str()));
    }
    for criterion in &plan.acceptance_criteria {
        plan_tokens.extend(keyword_tokens(criterion.as_str()));
    }

    if token_overlap_score(&task_tokens, &plan_tokens) < 0.20 {
        return Err(anyhow!(
            governor_contract::task_contract_plan_drift_message()
        ));
    }

    Ok(())
}

pub(crate) fn build_assumption_ledger_prompt(ledger: &AssumptionLedger) -> Option<String> {
    if ledger.entries.is_empty() {
        return None;
    }
    let mut out = String::from("[Assumption Ledger]\n");
    let mut wrote = false;
    let sections = [
        ("Open assumptions", AssumptionStatus::Unknown),
        ("Confirmed assumptions", AssumptionStatus::Confirmed),
        ("Refuted assumptions", AssumptionStatus::Refuted),
    ];
    for (title, status) in sections {
        let items = ledger
            .entries
            .iter()
            .filter(|entry| entry.status == status)
            .collect::<Vec<_>>();
        if items.is_empty() {
            continue;
        }
        wrote = true;
        out.push_str(title);
        out.push_str(":\n");
        for entry in items {
            out.push_str(&format!("- [{}] {}", entry.status.as_str(), entry.text));
            if let Some(evidence) = entry.evidence.as_deref() {
                out.push_str(&format!(" — {evidence}"));
            }
            out.push('\n');
        }
    }
    if !wrote {
        return None;
    }
    out.push_str(
        "Do not rely on refuted assumptions. Prefer probes that convert open assumptions into confirmed facts.",
    );
    Some(out)
}

pub(crate) fn parse_evidence_block(text: &str) -> Option<EvidenceBlock> {
    let fields = parse_block_fields(text, "evidence")?;

    Some(EvidenceBlock {
        target_files: block_list_value(&fields, "target_files"),
        target_symbols: block_list_value(&fields, "target_symbols"),
        evidence: compact_one_line(block_text_value(&fields, "evidence").as_str(), 220),
        open_questions: compact_one_line(block_text_value(&fields, "open_questions").as_str(), 180),
        next_probe: compact_one_line(block_text_value(&fields, "next_probe").as_str(), 180),
    })
}

pub(crate) fn parse_impact_block(text: &str) -> Option<ImpactBlock> {
    let fields = parse_block_fields(text, "impact")?;

    Some(ImpactBlock {
        changed: block_text_value(&fields, "changed"),
        progress: block_text_value(&fields, "progress"),
        remaining_gap: block_text_value(&fields, "remaining_gap"),
    })
}

pub(crate) fn impact_progress_matches_entry(progress: &str, entry: &str) -> bool {
    let progress_sig = normalize_memory_entry(progress);
    let entry_sig = normalize_memory_entry(entry);
    if progress_sig.is_empty() || entry_sig.is_empty() {
        return false;
    }
    progress_sig.contains(&entry_sig) || entry_sig.contains(&progress_sig)
}

pub(crate) fn impact_progress_matches_plan(progress: &str, plan: &PlanBlock) -> bool {
    let progress_sig = normalize_memory_entry(progress);
    if progress_sig.is_empty() {
        return false;
    }

    if progress_sig.contains("step") {
        if let Some(n) = parse_first_usize(progress) {
            if n <= plan.steps.len() {
                return true;
            }
        }
    }

    if progress_sig.contains("acceptance")
        || progress_sig.contains("criterion")
        || progress_sig.contains("criteria")
    {
        if let Some(n) = parse_first_usize(progress) {
            if n <= plan.acceptance_criteria.len() {
                return true;
            }
        }
    }

    plan.steps
        .iter()
        .any(|step| impact_progress_matches_entry(progress, step))
        || plan
            .acceptance_criteria
            .iter()
            .any(|criterion| impact_progress_matches_entry(progress, criterion))
}

pub(crate) fn parse_string_list_arg(value: &serde_json::Value) -> Vec<String> {
    match value {
        serde_json::Value::Array(items) => items
            .iter()
            .filter_map(|item| item.as_str())
            .map(|item| compact_one_line(item.trim(), 160))
            .filter(|item| !item.is_empty())
            .collect(),
        serde_json::Value::String(s) => parse_plan_items(s),
        _ => Vec::new(),
    }
}
