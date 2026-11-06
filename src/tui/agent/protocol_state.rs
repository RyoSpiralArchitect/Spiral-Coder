use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AgentState {
    Planning,
    Executing,
    Verifying,
    Recovery,
    Done,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RecoveryStage {
    Diagnose,
    Fix,
    Verify,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GoalDelta {
    Closer,
    Same,
    Farther,
    Unknown,
}

impl GoalDelta {
    pub(crate) fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "closer" => Self::Closer,
            "same" => Self::Same,
            "farther" | "further" => Self::Farther,
            _ => Self::Unknown,
        }
    }

    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::Closer => "closer",
            Self::Same => "same",
            Self::Farther => "farther",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StrategyChange {
    Keep,
    Adjust,
    Abandon,
    Unknown,
}

impl StrategyChange {
    pub(crate) fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "keep" => Self::Keep,
            "adjust" => Self::Adjust,
            "abandon" => Self::Abandon,
            _ => Self::Unknown,
        }
    }

    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::Keep => "keep",
            Self::Adjust => "adjust",
            Self::Abandon => "abandon",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct PlanBlock {
    pub(crate) goal: String,
    pub(crate) steps: Vec<String>,
    pub(crate) acceptance_criteria: Vec<String>,
    pub(crate) risks: String,
    pub(crate) assumptions: String,
}

#[derive(Debug, Clone)]
pub(crate) struct ThinkBlock {
    pub(crate) goal: String,
    pub(crate) step: usize,
    pub(crate) tool: String,
    pub(crate) risk: String,
    pub(crate) doubt: String,
    pub(crate) next: String,
    pub(crate) verify: String,
}

#[derive(Debug, Clone)]
pub(crate) struct ReflectionBlock {
    pub(crate) last_outcome: String,
    pub(crate) goal_delta: GoalDelta,
    pub(crate) wrong_assumption: String,
    pub(crate) strategy_change: StrategyChange,
    pub(crate) next_minimal_action: String,
}

#[derive(Debug, Clone)]
pub(crate) struct ImpactBlock {
    pub(crate) changed: String,
    pub(crate) progress: String,
    pub(crate) remaining_gap: String,
}

#[derive(Debug, Clone)]
pub(crate) struct EvidenceBlock {
    pub(crate) target_files: Vec<String>,
    pub(crate) target_symbols: Vec<String>,
    pub(crate) evidence: String,
    pub(crate) open_questions: String,
    pub(crate) next_probe: String,
}

#[derive(Debug, Clone)]
pub(crate) struct TaskContract {
    pub(crate) task_summary: String,
    pub(crate) hard_constraints: Vec<String>,
    pub(crate) non_goals: Vec<String>,
    pub(crate) output_shape: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum InstructionAuthority {
    Root,
    System,
    Project,
    User,
    Execution,
}

impl InstructionAuthority {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::Root => "root",
            Self::System => "system",
            Self::Project => "project",
            Self::User => "user",
            Self::Execution => "execution",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InstructionSource {
    TaskContract,
    ProjectRules,
    UserRequest,
    Plan,
    Think,
}

impl InstructionSource {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::TaskContract => "task_contract",
            Self::ProjectRules => "project_rules",
            Self::UserRequest => "user_request",
            Self::Plan => "plan",
            Self::Think => "think",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct InstructionPriority {
    pub(crate) authority: InstructionAuthority,
    pub(crate) explicit: bool,
    pub(crate) locality: u8,
    pub(crate) sequence: usize,
}

impl InstructionPriority {
    pub(crate) fn outranks(&self, other: &Self) -> bool {
        (
            std::cmp::Reverse(governor_contract::instruction_authority_rank(
                self.authority.as_str(),
            )),
            self.explicit,
            self.locality,
            self.sequence,
        ) > (
            std::cmp::Reverse(governor_contract::instruction_authority_rank(
                other.authority.as_str(),
            )),
            other.explicit,
            other.locality,
            other.sequence,
        )
    }
}

#[derive(Debug, Clone)]
pub(crate) struct InstructionConflict {
    pub(crate) winner_authority: InstructionAuthority,
    pub(crate) winner_source: InstructionSource,
    pub(crate) loser_authority: InstructionAuthority,
    pub(crate) loser_source: InstructionSource,
    pub(crate) reason: String,
}

impl InstructionConflict {
    pub(crate) fn render(&self) -> String {
        governor_contract::instruction_resolver_conflict_message(
            self.winner_authority.as_str(),
            self.winner_source.as_str(),
            self.loser_authority.as_str(),
            self.loser_source.as_str(),
            self.reason.as_str(),
        )
    }
}

#[derive(Debug, Clone)]
pub(crate) struct InstructionResolver {
    pub(crate) task_summary: String,
    pub(crate) root_read_only: bool,
    pub(crate) project_rules_active: bool,
}

impl InstructionResolver {
    pub(crate) fn new(
        task_summary: &str,
        root_read_only: bool,
        project_rules_active: bool,
    ) -> Self {
        Self {
            task_summary: compact_one_line(task_summary.trim(), 220),
            root_read_only,
            project_rules_active,
        }
    }

    pub(crate) fn user_task_summary(&self) -> &str {
        self.task_summary.as_str()
    }

    pub(crate) fn prompt(&self, required_verification: VerificationLevel) -> String {
        let mut out = format!(
            "[{}]\n{}\n",
            governor_contract::instruction_resolver_title(),
            governor_contract::instruction_resolver_priority_title()
        );
        for label in governor_contract::instruction_priority_labels() {
            out.push_str("- ");
            out.push_str(label.as_str());
            out.push('\n');
        }
        out.push_str(governor_contract::instruction_resolver_rules_title());
        out.push('\n');
        for rule in governor_contract::instruction_resolver_rules() {
            out.push_str("- ");
            out.push_str(rule.as_str());
            out.push('\n');
        }
        out.push_str(governor_contract::instruction_resolver_current_title());
        out.push('\n');
        out.push_str("- ");
        out.push_str(
            governor_contract::instruction_resolver_root_runtime_line_message(
                InstructionAuthority::Root.as_str(),
            )
            .as_str(),
        );
        out.push('\n');
        out.push_str("- ");
        out.push_str(
            governor_contract::instruction_resolver_user_task_line_message(
                InstructionAuthority::User.as_str(),
                InstructionSource::UserRequest.as_str(),
                self.user_task_summary(),
            )
            .as_str(),
        );
        out.push('\n');
        if self.root_read_only {
            out.push_str("- ");
            out.push_str(
                governor_contract::instruction_resolver_read_only_line_message(
                    InstructionAuthority::System.as_str(),
                    InstructionSource::TaskContract.as_str(),
                )
                .as_str(),
            );
            out.push('\n');
        }
        out.push_str("- ");
        out.push_str(
            governor_contract::instruction_resolver_done_requires_line_message(
                InstructionAuthority::System.as_str(),
                InstructionSource::TaskContract.as_str(),
                match required_verification {
                    VerificationLevel::Build => "real build/check/lint",
                    VerificationLevel::Behavioral => "real behavioral",
                },
            )
            .as_str(),
        );
        out.push('\n');
        if self.project_rules_active {
            out.push_str("- ");
            out.push_str(
                governor_contract::instruction_resolver_project_rules_line_message(
                    InstructionAuthority::Project.as_str(),
                    InstructionSource::ProjectRules.as_str(),
                )
                .as_str(),
            );
            out.push('\n');
        }
        out
    }

    pub(crate) fn plan_conflict(&self, plan: &PlanBlock) -> Option<InstructionConflict> {
        if !self.root_read_only {
            return None;
        }
        let reason = read_only_plan_violation(plan)?;
        let winner = InstructionPriority {
            authority: InstructionAuthority::System,
            explicit: true,
            locality: 2,
            sequence: 1,
        };
        let loser = InstructionPriority {
            authority: InstructionAuthority::Execution,
            explicit: true,
            locality: 3,
            sequence: 1,
        };
        if !winner.outranks(&loser) {
            return None;
        }
        Some(InstructionConflict {
            winner_authority: InstructionAuthority::System,
            winner_source: InstructionSource::TaskContract,
            loser_authority: InstructionAuthority::Execution,
            loser_source: InstructionSource::Plan,
            reason,
        })
    }

    pub(crate) fn tool_conflict(
        &self,
        tc: &ToolCallData,
        test_cmd: Option<&str>,
    ) -> Option<InstructionConflict> {
        if !self.root_read_only {
            return None;
        }
        let reason = match tc.name.as_str() {
            "write_file" | "patch_file" | "apply_diff" => {
                Some(governor_contract::instruction_resolver_read_only_mutation_message())
            }
            "exec" => {
                let command =
                    parse_exec_command_from_args(tc.arguments.as_str()).unwrap_or_default();
                match classify_exec_kind(command.as_str(), test_cmd) {
                    ExecKind::Diagnostic => None,
                    ExecKind::Verify => Some(
                        governor_contract::instruction_resolver_read_only_verify_exec_message(
                            command.as_str(),
                        ),
                    ),
                    ExecKind::Action => Some(
                        governor_contract::instruction_resolver_read_only_action_exec_message(
                            command.as_str(),
                        ),
                    ),
                }
            }
            _ => None,
        }?;
        let winner = InstructionPriority {
            authority: InstructionAuthority::System,
            explicit: true,
            locality: 2,
            sequence: 1,
        };
        let loser = InstructionPriority {
            authority: InstructionAuthority::Execution,
            explicit: true,
            locality: 3,
            sequence: 2,
        };
        if !winner.outranks(&loser) {
            return None;
        }
        Some(InstructionConflict {
            winner_authority: InstructionAuthority::System,
            winner_source: InstructionSource::TaskContract,
            loser_authority: InstructionAuthority::Execution,
            loser_source: InstructionSource::Think,
            reason,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AssumptionStatus {
    Unknown,
    Confirmed,
    Refuted,
}

impl AssumptionStatus {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Confirmed => "confirmed",
            Self::Refuted => "refuted",
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct AssumptionEntry {
    pub(crate) text: String,
    pub(crate) status: AssumptionStatus,
    pub(crate) evidence: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct AssumptionLedger {
    pub(crate) entries: Vec<AssumptionEntry>,
}

impl AssumptionLedger {
    pub(crate) fn remember_unknown(&mut self, assumption: &str) {
        let text = compact_one_line(assumption.trim(), 140);
        if text == "-" {
            return;
        }
        let sig = normalize_memory_entry(text.as_str());
        if sig.is_empty() {
            return;
        }
        if let Some(existing) = self
            .entries
            .iter_mut()
            .find(|entry| normalize_memory_entry(entry.text.as_str()) == sig)
        {
            if existing.status == AssumptionStatus::Unknown {
                existing.text = text;
            }
            return;
        }
        self.entries.push(AssumptionEntry {
            text,
            status: AssumptionStatus::Unknown,
            evidence: None,
        });
        if self.entries.len() > 8 {
            self.entries.remove(0);
        }
    }

    pub(crate) fn sync_to_plan(&mut self, plan: &PlanBlock) {
        let plan_assumptions = parse_assumption_items(plan.assumptions.as_str());
        for assumption in &plan_assumptions {
            self.remember_unknown(assumption);
        }
        self.entries.retain(|entry| {
            if entry.status == AssumptionStatus::Refuted {
                return true;
            }
            let sig = normalize_memory_entry(entry.text.as_str());
            plan_assumptions
                .iter()
                .any(|assumption| normalize_memory_entry(assumption.as_str()) == sig)
        });
    }

    pub(crate) fn mark_refuted(&mut self, assumption: &str, evidence: Option<&str>) {
        let text = compact_one_line(assumption.trim(), 140);
        if text == "-" {
            return;
        }
        let sig = normalize_memory_entry(text.as_str());
        let evidence = evidence
            .map(|item| compact_one_line(item.trim(), 160))
            .filter(|item| item != "-");
        if let Some(existing) = self
            .entries
            .iter_mut()
            .find(|entry| normalize_memory_entry(entry.text.as_str()) == sig)
        {
            existing.status = AssumptionStatus::Refuted;
            existing.evidence = evidence;
            existing.text = text;
            return;
        }
        self.entries.push(AssumptionEntry {
            text,
            status: AssumptionStatus::Refuted,
            evidence,
        });
        if self.entries.len() > 8 {
            self.entries.remove(0);
        }
    }

    pub(crate) fn refresh_confirmations(&mut self, working_mem: &WorkingMemory) {
        let support = working_mem
            .facts
            .iter()
            .chain(working_mem.successful_verifications.iter())
            .cloned()
            .collect::<Vec<_>>();
        for entry in &mut self.entries {
            if entry.status != AssumptionStatus::Unknown {
                continue;
            }
            let assumption_sig = normalize_memory_entry(entry.text.as_str());
            if assumption_sig.is_empty() {
                continue;
            }
            let assumption_tokens = keyword_tokens(entry.text.as_str());
            let best = support
                .iter()
                .map(|candidate| {
                    let candidate_sig = normalize_memory_entry(candidate.as_str());
                    let mut score = token_overlap_score(
                        &assumption_tokens,
                        &keyword_tokens(candidate.as_str()),
                    );
                    if !candidate_sig.is_empty()
                        && (candidate_sig.contains(assumption_sig.as_str())
                            || assumption_sig.contains(candidate_sig.as_str()))
                    {
                        score = score.max(0.9);
                    }
                    (score, candidate)
                })
                .max_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
            if let Some((score, candidate)) = best {
                if score >= 0.72 {
                    entry.status = AssumptionStatus::Confirmed;
                    entry.evidence = Some(compact_one_line(candidate.as_str(), 160));
                }
            }
        }
    }

    pub(crate) fn from_messages(
        messages: &[serde_json::Value],
        working_mem: &WorkingMemory,
    ) -> Self {
        let mut ledger = Self::default();
        let mut file_evidence = assumption_gate::FileExistenceReplay::default();
        for msg in messages {
            if msg.get("role").and_then(|v| v.as_str()) == Some("assistant") {
                let content = msg.get("content").and_then(|v| v.as_str()).unwrap_or("");
                if let Some(plan) =
                    plan_resume::from_message(msg).filter(|p| validate_plan(p).is_ok())
                {
                    ledger.sync_to_plan(&plan);
                }
                if let Some(reflect) = parse_reflection_block(content) {
                    ledger.mark_refuted(
                        reflect.wrong_assumption.as_str(),
                        Some(reflect.next_minimal_action.as_str()),
                    );
                }
            }
            file_evidence.observe(&mut ledger, msg);
        }
        ledger.refresh_confirmations(working_mem);
        ledger
    }

    pub(crate) fn has_refuted(&self) -> bool {
        self.entries
            .iter()
            .any(|entry| entry.status == AssumptionStatus::Refuted)
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct WorkingMemory {
    pub(crate) facts: Vec<String>,
    pub(crate) completed_steps: Vec<String>,
    pub(crate) successful_verifications: Vec<String>,
    pub(crate) chosen_strategy: Option<String>,
}

impl WorkingMemory {
    pub(crate) fn is_empty(&self) -> bool {
        self.facts.is_empty()
            && self.completed_steps.is_empty()
            && self.successful_verifications.is_empty()
            && self
                .chosen_strategy
                .as_deref()
                .map(|s| s.trim().is_empty())
                .unwrap_or(true)
    }

    pub(crate) fn remember_fact(&mut self, fact: &str) {
        remember_recent_unique(&mut self.facts, fact, 6, 120);
    }

    pub(crate) fn remember_completed_step(&mut self, step: &str) {
        remember_recent_unique(&mut self.completed_steps, step, 7, 120);
    }

    pub(crate) fn remember_successful_verification(&mut self, command: &str) {
        remember_recent_unique(&mut self.successful_verifications, command, 4, 1024);
    }

    pub(crate) fn set_strategy(&mut self, strategy: &str) {
        let value = compact_one_line(strategy.trim(), 140);
        if value == "-" {
            return;
        }
        self.chosen_strategy = Some(value);
    }

    pub(crate) fn sync_to_plan(&mut self, plan: &PlanBlock) {
        self.completed_steps.retain(|step| {
            let want = normalize_memory_entry(step);
            plan.steps
                .iter()
                .any(|candidate| normalize_memory_entry(candidate) == want)
        });
    }

    pub(crate) fn from_messages(messages: &[serde_json::Value], test_cmd: Option<&str>) -> Self {
        #[derive(Debug, Clone)]
        struct PendingToolIntent {
            name: String,
            command: Option<String>,
            next_action: Option<String>,
            step_label: Option<String>,
        }

        let mut mem = Self::default();
        let exec_context =
            exec_verification::ExecVerificationContext::from_messages(test_cmd, messages);
        let mut active_plan: Option<PlanBlock> = None;
        let mut pending: std::collections::HashMap<String, PendingToolIntent> =
            std::collections::HashMap::new();

        for msg in messages {
            let role = msg.get("role").and_then(|v| v.as_str()).unwrap_or("");

            if role == "assistant" {
                let content = msg.get("content").and_then(|v| v.as_str()).unwrap_or("");

                if let Some(plan) = parse_plan_block(content).filter(|p| validate_plan(p).is_ok()) {
                    mem.sync_to_plan(&plan);
                    active_plan = Some(plan);
                }

                if let Some(reflect) = parse_reflection_block(content) {
                    if matches!(
                        reflect.strategy_change,
                        StrategyChange::Adjust | StrategyChange::Abandon
                    ) {
                        mem.set_strategy(reflect.next_minimal_action.as_str());
                    }
                }

                let parsed_think = parse_think_block(content);
                if let Some(tcs) = msg.get("tool_calls").and_then(|v| v.as_array()) {
                    for tc in tcs {
                        let id = tc.get("id").and_then(|v| v.as_str()).unwrap_or("").trim();
                        if id.is_empty() {
                            continue;
                        }
                        let name = tc
                            .get("function")
                            .and_then(|f| f.get("name"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .trim()
                            .to_string();
                        let args = tc
                            .get("function")
                            .and_then(|f| f.get("arguments"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .trim()
                            .to_string();
                        let command = if name == "exec" {
                            parse_exec_command_from_args(args.as_str())
                        } else {
                            None
                        };
                        let step_label =
                            resolve_step_label(active_plan.as_ref(), parsed_think.as_ref());
                        let next_action = parsed_think
                            .as_ref()
                            .map(|think| think.next.clone())
                            .filter(|s| !s.trim().is_empty());
                        pending.insert(
                            id.to_string(),
                            PendingToolIntent {
                                name,
                                command,
                                next_action,
                                step_label,
                            },
                        );
                    }
                }
                continue;
            }

            if role != "tool" {
                continue;
            }

            let id = msg
                .get("tool_call_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();
            if id.is_empty() {
                continue;
            }

            let Some(intent) = pending.remove(id) else {
                continue;
            };

            let content = msg.get("content").and_then(|v| v.as_str()).unwrap_or("");
            match intent.name.as_str() {
                "exec" => {
                    let command = intent.command.unwrap_or_default();
                    let (exit_code, stdout, stderr) = parse_exec_tool_output_sections(content);
                    let Some(exit_code) = exit_code else {
                        continue;
                    };
                    let effective_exit_code = if exit_code == 0
                        && suspicious_success_reason(stdout.as_str(), stderr.as_str()).is_some()
                    {
                        1
                    } else {
                        exit_code
                    };
                    if effective_exit_code != 0 {
                        continue;
                    }
                    let exec_kind = exec_context.classify(command.as_str()).kind;
                    update_working_memory_after_exec(
                        &mut mem,
                        command.as_str(),
                        stdout.as_str(),
                        content,
                        exec_kind,
                        intent.step_label.as_deref(),
                        intent.next_action.as_deref(),
                    );
                }
                "read_file" | "list_dir" | "glob" | "search_files" | "write_file"
                | "patch_file" | "apply_diff" => {
                    if !non_exec_tool_succeeded(content) {
                        continue;
                    }
                    let verified = crate::execution_evidence::auto_test_succeeded(
                        intent.name.as_str(),
                        content,
                    );
                    update_working_memory_after_non_exec(
                        &mut mem,
                        intent.name.as_str(),
                        verified,
                        intent.step_label.as_deref(),
                        intent.next_action.as_deref(),
                        test_cmd,
                    );
                }
                _ => {}
            }
        }

        mem
    }
}
