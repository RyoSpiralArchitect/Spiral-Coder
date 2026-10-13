use super::meta_harness::{FailurePattern, MetaHarness, PolicyAction, PolicyDelta};
use super::task_harness::TaskHarness;
use super::*;
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EvaluatorFindingKind {
    Keep,
    Problem,
    TryNow,
}

impl EvaluatorFindingKind {
    fn label(self) -> &'static str {
        match self {
            EvaluatorFindingKind::Keep => "keep",
            EvaluatorFindingKind::Problem => "problem",
            EvaluatorFindingKind::TryNow => "try_now",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct EvaluatorFinding {
    pub kind: EvaluatorFindingKind,
    pub summary: String,
    pub evidence: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EvaluatorBlockScope {
    ExactRepeatOnly,
    AnyBlockedTool,
}

impl EvaluatorBlockScope {
    fn label(self) -> &'static str {
        match self {
            EvaluatorBlockScope::ExactRepeatOnly => "exact_repeat_only",
            EvaluatorBlockScope::AnyBlockedTool => "any_blocked_tool",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PolicyPatch {
    pub id: &'static str,
    pub preferred_tools: Vec<&'static str>,
    pub blocked_tools: Vec<&'static str>,
    pub blocked_scope: EvaluatorBlockScope,
    pub blocked_command_display: Option<String>,
    pub blocked_command_signature: Option<String>,
    pub block_verify_exec_before_mutation: bool,
    pub behavioral_test_cmd: Option<String>,
    pub exit_hint: String,
    pub support_note: Option<String>,
    pub pattern: FailurePattern,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct EvaluatorLoop {
    findings: Vec<EvaluatorFinding>,
    patch: Option<PolicyPatch>,
}

impl EvaluatorLoop {
    pub(super) fn analyze(
        task_harness: TaskHarness,
        meta_harness: &MetaHarness,
        reflection_ledger: &crate::reflection_ledger::ReflectionLedger,
        last_reflection: Option<&ReflectionBlock>,
        test_cmd: Option<&str>,
        last_mutation_step: Option<usize>,
        file_tool_consec_failures: usize,
    ) -> Self {
        let Some(policy) = meta_harness.policy().cloned() else {
            return Self::default();
        };
        if policy.pattern == FailurePattern::RepeatedObservationLoop
            && last_mutation_step.is_none()
            && file_tool_consec_failures > 0
        {
            return Self::default();
        }

        let patch = build_policy_patch(&policy, test_cmd, last_mutation_step);
        let support_note = related_reflection_memory(
            reflection_ledger,
            last_reflection,
            patch.preferred_tools.as_slice(),
            patch.id,
        );
        let patch = PolicyPatch {
            support_note: support_note.clone(),
            ..patch
        };
        let findings = vec![
            keep_finding(task_harness, &policy, test_cmd),
            problem_finding(&policy),
            try_now_finding(&patch),
        ];

        Self {
            findings,
            patch: Some(patch),
        }
    }

    pub(super) fn telemetry_payload(&self, task_harness: TaskHarness) -> Option<Value> {
        let patch = self.patch.as_ref()?;
        Some(serde_json::json!({
            "policy_id": patch.id,
            "pattern": patch.pattern.label(),
            "lane": task_harness.lane_label(),
            "finding_count": self.findings.len(),
            "preferred_tools": patch.preferred_tools,
            "blocked_tools": patch.blocked_tools,
            "blocked_scope": patch.blocked_scope.label(),
            "blocked_command": patch.blocked_command_display,
            "block_verify_exec_before_mutation": patch.block_verify_exec_before_mutation,
            "support_note": patch.support_note,
        }))
    }

    pub(super) fn prompt(&self) -> Option<String> {
        let patch = self.patch.as_ref()?;
        let mut out = String::from("[Evaluator Loop]\n");
        for finding in &self.findings {
            out.push_str(&format!(
                "{}: {}\n",
                finding.kind.label(),
                compact_one_line(finding.summary.as_str(), 220)
            ));
            out.push_str(&format!(
                "{}_evidence: {}\n",
                finding.kind.label(),
                compact_one_line(finding.evidence.as_str(), 220)
            ));
        }
        out.push_str("policy_patch:\n");
        out.push_str(&format!("- id: {}\n", patch.id));
        out.push_str(&format!(
            "- preferred_tools: {}\n",
            patch.preferred_tools.join(", ")
        ));
        out.push_str(&format!(
            "- blocked_tools: {}\n",
            patch.blocked_tools.join(", ")
        ));
        out.push_str(&format!(
            "- blocked_scope: {}\n",
            patch.blocked_scope.label()
        ));
        if patch.block_verify_exec_before_mutation {
            out.push_str("- verify_exec_before_mutation: blocked\n");
        }
        if let Some(command) = patch.blocked_command_display.as_deref() {
            out.push_str(&format!(
                "- blocked_repeat: {}\n",
                compact_one_line(command, 180)
            ));
        }
        out.push_str(&format!(
            "- exit_hint: {}\n",
            compact_one_line(patch.exit_hint.as_str(), 220)
        ));
        if let Some(support) = patch.support_note.as_deref() {
            out.push_str(&format!("- support: {}\n", compact_one_line(support, 220)));
        }
        out.push_str(
            "Treat this as deterministic evaluator output derived from the current trace.\n\
If your next tool call matches the blocked scope without contradictory new evidence, it will be rejected.\n\
Prefer the listed tools now instead of widening observation.",
        );
        Some(out)
    }

    pub(super) fn compact_prompt(&self) -> Option<String> {
        let patch = self.patch.as_ref()?;
        let mut lines = vec![
            "[Evaluator Loop cache]".to_string(),
            format!("- policy_id: {}", patch.id),
            format!("- pattern: {}", patch.pattern.label()),
            format!("- preferred: {}", patch.preferred_tools.join(", ")),
            format!("- blocked: {}", patch.blocked_tools.join(", ")),
            format!("- scope: {}", patch.blocked_scope.label()),
        ];
        if patch.block_verify_exec_before_mutation {
            lines.push("- verify_exec_before_mutation: blocked".to_string());
        }
        if let Some(command) = patch.blocked_command_display.as_deref() {
            lines.push(format!(
                "- blocked_repeat: {}",
                compact_one_line(command, 90)
            ));
        }
        if let Some(finding) = self
            .findings
            .iter()
            .find(|finding| matches!(finding.kind, EvaluatorFindingKind::TryNow))
        {
            lines.push(format!(
                "- try_now: {}",
                compact_one_line(finding.summary.as_str(), 100)
            ));
        }
        Some(lines.join("\n"))
    }

    pub(super) fn build_violation_block(&self, tc: &ToolCallData) -> Option<String> {
        let patch = self.patch.as_ref()?;
        let blocks_verify_exec = patch.block_verify_exec_before_mutation
            && tc.name == "exec"
            && parse_exec_command_from_args(tc.arguments.as_str())
                .and_then(|command| {
                    classify_verify_level(command.as_str(), patch.behavioral_test_cmd.as_deref())
                })
                .is_some();
        if !blocks_verify_exec
            && !patch
                .blocked_tools
                .iter()
                .any(|tool| *tool == tc.name.as_str())
        {
            return None;
        }

        let tool_signature =
            canonicalize_tool_call_command(tc.name.as_str(), tc.arguments.as_str()).unwrap_or_else(
                || blocked_tool_call_signature(tc.name.as_str(), tc.arguments.as_str()),
            );
        if !blocks_verify_exec {
            match patch.blocked_scope {
                EvaluatorBlockScope::ExactRepeatOnly => {
                    let blocked_signature = patch.blocked_command_signature.as_deref()?;
                    if normalize_for_signature(tool_signature.as_str()) != blocked_signature {
                        return None;
                    }
                }
                EvaluatorBlockScope::AnyBlockedTool => {}
            }
        }

        let repeated = patch
            .blocked_command_display
            .as_deref()
            .map(|command| format!("Blocked repeat: {}\n", compact_one_line(command, 180)))
            .unwrap_or_default();
        let support = patch
            .support_note
            .as_deref()
            .map(|note| format!("Support: {}\n", compact_one_line(note, 200)))
            .unwrap_or_default();
        Some(format!(
            "[Evaluator Loop]\n\
policy_id: {}\n\
pattern: {}\n\
Blocked tool call: {}\n\
{}Required now: use one of {}.\n\
Exit hint: {}\n\
{}This rejection was generated by a trace-derived runtime policy patch.",
            patch.id,
            patch.pattern.label(),
            compact_one_line(tool_signature.as_str(), 180),
            repeated,
            patch.preferred_tools.join(", "),
            compact_one_line(patch.exit_hint.as_str(), 220),
            support,
        ))
    }

    pub(super) fn policy_id(&self) -> Option<&'static str> {
        self.patch.as_ref().map(|patch| patch.id)
    }

    pub(super) fn patch(&self) -> Option<&PolicyPatch> {
        self.patch.as_ref()
    }
}

fn keep_finding(
    task_harness: TaskHarness,
    policy: &PolicyDelta,
    test_cmd: Option<&str>,
) -> EvaluatorFinding {
    let summary = match policy.pattern {
        FailurePattern::RepeatedObservationLoop => format!(
            "Keep the confirmed working set and stay in the `{}` lane.",
            task_harness.lane_label()
        ),
        FailurePattern::RepoScaffoldDrift => {
            let target = policy
                .next_target
                .as_deref()
                .map(|target| compact_one_line(target, 120))
                .unwrap_or_else(|| "the current repo root".to_string());
            format!("Keep the current repo scaffold destination and finish `{target}`.")
        }
    };
    let evidence = match (policy.pattern, test_cmd) {
        (FailurePattern::RepeatedObservationLoop, Some(command)) => format!(
            "The request is still action-oriented and verification remains `{}`.",
            compact_one_line(command, 120)
        ),
        (FailurePattern::RepeatedObservationLoop, None) => {
            "The trace already contains enough successful observation to stop widening search."
                .to_string()
        }
        (FailurePattern::RepoScaffoldDrift, _) => {
            "The scaffold target is already known, so additional workspace probing is lower value than creating the next missing artifact.".to_string()
        }
    };
    EvaluatorFinding {
        kind: EvaluatorFindingKind::Keep,
        summary,
        evidence,
    }
}

fn problem_finding(policy: &PolicyDelta) -> EvaluatorFinding {
    let summary = match policy.pattern {
        FailurePattern::RepeatedObservationLoop => {
            "Observation repeated without increasing confidence enough to justify another probe."
                .to_string()
        }
        FailurePattern::RepoScaffoldDrift => {
            "Repo scaffold progress drifted into partial completion instead of advancing the next missing artifact.".to_string()
        }
    };
    let mut evidence_parts = vec![format!("evidence_count={}", policy.evidence_count)];
    if let Some(command) = policy.attempted_command.as_deref() {
        evidence_parts.push(format!("attempted={}", compact_one_line(command, 140)));
    }
    if let Some(target) = policy.next_target.as_deref() {
        evidence_parts.push(format!("next_target={}", compact_one_line(target, 140)));
    }
    EvaluatorFinding {
        kind: EvaluatorFindingKind::Problem,
        summary,
        evidence: evidence_parts.join(" | "),
    }
}

fn try_now_finding(patch: &PolicyPatch) -> EvaluatorFinding {
    let summary = match patch.id {
        "force_mutation_after_observation_loop" => {
            "Make the smallest edit now with `patch_file` or `apply_diff`, then verify.".to_string()
        }
        "force_artifact_creation_after_observation_loop" => {
            "Create the requested artifact now with `write_file` or the smallest `exec`."
                .to_string()
        }
        "advance_repo_scaffold_artifact" => {
            "Advance the scaffold to the next missing artifact instead of observing again."
                .to_string()
        }
        _ => "Act now on the preferred tool path.".to_string(),
    };
    let mut evidence = format!(
        "preferred_tools={} | blocked_scope={} | exit_hint={}",
        patch.preferred_tools.join(", "),
        patch.blocked_scope.label(),
        compact_one_line(patch.exit_hint.as_str(), 140)
    );
    if let Some(note) = patch.support_note.as_deref() {
        evidence.push_str(&format!(" | support={}", compact_one_line(note, 140)));
    }
    EvaluatorFinding {
        kind: EvaluatorFindingKind::TryNow,
        summary,
        evidence,
    }
}

fn build_policy_patch(
    policy: &PolicyDelta,
    test_cmd: Option<&str>,
    last_mutation_step: Option<usize>,
) -> PolicyPatch {
    match policy.action {
        PolicyAction::MutateExistingNow => PolicyPatch {
            id: "force_mutation_after_observation_loop",
            preferred_tools: vec!["patch_file", "apply_diff"],
            blocked_tools: observation_tool_names(),
            blocked_scope: EvaluatorBlockScope::ExactRepeatOnly,
            blocked_command_display: policy.attempted_command.clone(),
            blocked_command_signature: policy
                .attempted_command
                .as_deref()
                .map(normalize_for_signature),
            block_verify_exec_before_mutation: last_mutation_step.is_none(),
            behavioral_test_cmd: test_cmd.map(str::to_string),
            exit_hint: match (policy.next_target.as_deref(), test_cmd) {
                (Some(target), Some(command)) => format!(
                    "Patch `{}` with the minimal change, then verify with `{}` before `done`.",
                    compact_one_line(target, 140),
                    compact_one_line(command, 120)
                ),
                (Some(target), None) => format!(
                    "Patch `{}` with the minimal change, then run the narrowest verification before `done`.",
                    compact_one_line(target, 140)
                ),
                (None, Some(command)) => format!(
                    "Apply the minimal change, then verify with `{}` before `done`.",
                    compact_one_line(command, 120)
                ),
                (None, None) => {
                    "Apply the minimal change, then run the narrowest verification before `done`."
                        .to_string()
                }
            },
            support_note: None,
            pattern: policy.pattern,
        },
        PolicyAction::CreateArtifactNow => PolicyPatch {
            id: "force_artifact_creation_after_observation_loop",
            preferred_tools: vec!["write_file", "exec"],
            blocked_tools: observation_tool_names(),
            blocked_scope: EvaluatorBlockScope::ExactRepeatOnly,
            blocked_command_display: policy.attempted_command.clone(),
            blocked_command_signature: policy
                .attempted_command
                .as_deref()
                .map(normalize_for_signature),
            block_verify_exec_before_mutation: false,
            behavioral_test_cmd: None,
            exit_hint: "Create the requested artifact on disk now, then verify it before `done`."
                .to_string(),
            support_note: None,
            pattern: policy.pattern,
        },
        PolicyAction::AdvanceRepoScaffold => PolicyPatch {
            id: "advance_repo_scaffold_artifact",
            preferred_tools: vec!["exec", "write_file"],
            blocked_tools: observation_tool_names(),
            blocked_scope: EvaluatorBlockScope::AnyBlockedTool,
            blocked_command_display: None,
            blocked_command_signature: None,
            block_verify_exec_before_mutation: false,
            behavioral_test_cmd: None,
            exit_hint: policy
                .next_target
                .as_deref()
                .map(|target| {
                    format!(
                        "Create `{}` on disk, then verify the repo artifacts before `done`.",
                        compact_one_line(target, 140)
                    )
                })
                .unwrap_or_else(|| {
                    "Advance the repo scaffold with the next missing artifact before `done`."
                        .to_string()
                }),
            support_note: None,
            pattern: policy.pattern,
        },
    }
}

fn related_reflection_memory(
    reflection_ledger: &crate::reflection_ledger::ReflectionLedger,
    last_reflection: Option<&ReflectionBlock>,
    preferred_tools: &[&'static str],
    policy_id: &str,
) -> Option<String> {
    if let Some(reflect) = last_reflection {
        let next = reflect.next_minimal_action.trim();
        if !next.is_empty()
            && reflect.strategy_change != StrategyChange::Keep
            && action_matches_preferences(next, preferred_tools)
        {
            return Some(format!(
                "Recent reflection already shifted strategy: {} -> {}",
                compact_one_line(reflect.wrong_assumption.as_str(), 100),
                compact_one_line(next, 100)
            ));
        }
    }

    let mut best: Option<(&crate::reflection_ledger::ReflectionLedgerEntry, f32)> = None;
    for entry in &reflection_ledger.entries {
        if !action_matches_preferences(entry.next_minimal_action.as_str(), preferred_tools) {
            continue;
        }
        let mut score = token_overlap_score(
            &keyword_tokens(entry.next_minimal_action.as_str()),
            &keyword_tokens(policy_id),
        );
        if action_matches_preferences(entry.next_minimal_action.as_str(), preferred_tools) {
            score += 0.35;
        }
        if score < 0.35 {
            continue;
        }
        match best {
            Some((_, best_score)) if best_score >= score => {}
            _ => best = Some((entry, score)),
        }
    }
    best.map(|(entry, _)| {
        format!(
            "Reflection ledger agrees: {} -> {}",
            compact_one_line(entry.wrong_assumption.as_str(), 100),
            compact_one_line(entry.next_minimal_action.as_str(), 100)
        )
    })
}

fn action_matches_preferences(action: &str, preferred_tools: &[&'static str]) -> bool {
    let low = action.to_ascii_lowercase();
    preferred_tools.iter().any(|tool| low.contains(tool))
}

fn observation_tool_names() -> Vec<&'static str> {
    vec!["read_file", "search_files", "list_dir", "glob"]
}

#[cfg(test)]
mod tests {
    use super::super::meta_harness::PolicyDelta;
    use super::*;

    fn base_fix_meta_harness() -> MetaHarness {
        MetaHarness::for_test(PolicyDelta {
            pattern: FailurePattern::RepeatedObservationLoop,
            action: PolicyAction::MutateExistingNow,
            evidence_count: 3,
            attempted_command: Some("read_file(path=src/lib.rs)".to_string()),
            next_target: None,
        })
    }

    #[test]
    fn evaluator_loop_builds_kpt_prompt_for_fix_loop() {
        let reflection_ledger = crate::reflection_ledger::ReflectionLedger::default();
        let evaluator = EvaluatorLoop::analyze(
            TaskHarness {
                lane: super::task_harness::TaskLane::FixExisting,
                artifact_mode: super::task_harness::ArtifactMode::ExistingFiles,
            },
            &base_fix_meta_harness(),
            &reflection_ledger,
            None,
            Some("cargo test 2>&1"),
            None,
            0,
        );

        let prompt = evaluator.prompt().expect("prompt");
        assert!(prompt.contains("[Evaluator Loop]"));
        assert!(prompt.contains("keep:"));
        assert!(prompt.contains("problem:"));
        assert!(prompt.contains("try_now:"));
        let telemetry = evaluator
            .telemetry_payload(TaskHarness {
                lane: super::task_harness::TaskLane::FixExisting,
                artifact_mode: super::task_harness::ArtifactMode::ExistingFiles,
            })
            .expect("telemetry");
        assert_eq!(
            telemetry["policy_id"].as_str(),
            Some("force_mutation_after_observation_loop")
        );
    }

    #[test]
    fn evaluator_loop_blocks_exact_repeated_observation_command() {
        let reflection_ledger = crate::reflection_ledger::ReflectionLedger::default();
        let evaluator = EvaluatorLoop::analyze(
            TaskHarness {
                lane: super::task_harness::TaskLane::FixExisting,
                artifact_mode: super::task_harness::ArtifactMode::ExistingFiles,
            },
            &base_fix_meta_harness(),
            &reflection_ledger,
            None,
            Some("cargo test 2>&1"),
            None,
            0,
        );
        let repeated = ToolCallData {
            thought_signature: None,
            id: "call_repeat".to_string(),
            name: "read_file".to_string(),
            arguments: serde_json::json!({"path":"src/lib.rs"}).to_string(),
        };
        let different = ToolCallData {
            thought_signature: None,
            id: "call_other".to_string(),
            name: "read_file".to_string(),
            arguments: serde_json::json!({"path":"src/main.rs"}).to_string(),
        };

        let block = evaluator
            .build_violation_block(&repeated)
            .expect("repeated block");
        assert!(block.contains("force_mutation_after_observation_loop"));
        assert!(evaluator.build_violation_block(&different).is_none());
    }

    #[test]
    fn evaluator_loop_blocks_any_observation_during_repo_scaffold_drift() {
        let reflection_ledger = crate::reflection_ledger::ReflectionLedger::default();
        let evaluator = EvaluatorLoop::analyze(
            TaskHarness {
                lane: super::task_harness::TaskLane::ScaffoldRepo,
                artifact_mode: super::task_harness::ArtifactMode::NewRepo,
            },
            &MetaHarness::for_test(PolicyDelta {
                pattern: FailurePattern::RepoScaffoldDrift,
                action: PolicyAction::AdvanceRepoScaffold,
                evidence_count: 2,
                attempted_command: None,
                next_target: Some("demo_repo/README.md".to_string()),
            }),
            &reflection_ledger,
            None,
            Some(
                "test -d demo_repo/.git && test -f demo_repo/README.md && test -f demo_repo/.gitignore",
            ),
            None,
            0,
        );
        let tc = ToolCallData {
            thought_signature: None,
            id: "call_list".to_string(),
            name: "list_dir".to_string(),
            arguments: serde_json::json!({"dir":"demo_repo","include_hidden":true}).to_string(),
        };

        let block = evaluator.build_violation_block(&tc).expect("block");
        assert!(block.contains("advance_repo_scaffold_artifact"));
        assert!(block.contains("demo_repo/README.md"));
    }

    #[test]
    fn evaluator_loop_uses_recent_reflection_as_support() {
        let reflection_ledger = crate::reflection_ledger::ReflectionLedger::default();
        let evaluator = EvaluatorLoop::analyze(
            TaskHarness {
                lane: super::task_harness::TaskLane::FixExisting,
                artifact_mode: super::task_harness::ArtifactMode::ExistingFiles,
            },
            &base_fix_meta_harness(),
            &reflection_ledger,
            Some(&ReflectionBlock {
                last_outcome: "same".to_string(),
                goal_delta: GoalDelta::Same,
                wrong_assumption: "reading again would help".to_string(),
                strategy_change: StrategyChange::Adjust,
                next_minimal_action: "use patch_file on src/lib.rs".to_string(),
            }),
            Some("cargo test 2>&1"),
            None,
            0,
        );

        let prompt = evaluator.prompt().expect("prompt");
        assert!(prompt.contains("Recent reflection already shifted strategy"));
    }

    #[test]
    fn evaluator_loop_exit_hint_mentions_next_target() {
        let reflection_ledger = crate::reflection_ledger::ReflectionLedger::default();
        let evaluator = EvaluatorLoop::analyze(
            TaskHarness {
                lane: super::task_harness::TaskLane::FixExisting,
                artifact_mode: super::task_harness::ArtifactMode::ExistingFiles,
            },
            &MetaHarness::for_test(PolicyDelta {
                pattern: FailurePattern::RepeatedObservationLoop,
                action: PolicyAction::MutateExistingNow,
                evidence_count: 4,
                attempted_command: Some("read_file(path=src/robot.rs)".to_string()),
                next_target: Some("src/robot.rs::turn_left".to_string()),
            }),
            &reflection_ledger,
            None,
            Some("cargo test 2>&1"),
            None,
            0,
        );

        let prompt = evaluator.prompt().expect("prompt");
        assert!(prompt.contains("src/robot.rs::turn_left"));
    }

    #[test]
    fn evaluator_loop_blocks_verify_exec_before_first_mutation() {
        let reflection_ledger = crate::reflection_ledger::ReflectionLedger::default();
        let evaluator = EvaluatorLoop::analyze(
            TaskHarness {
                lane: super::task_harness::TaskLane::FixExisting,
                artifact_mode: super::task_harness::ArtifactMode::ExistingFiles,
            },
            &base_fix_meta_harness(),
            &reflection_ledger,
            None,
            Some("cargo test 2>&1"),
            None,
            0,
        );
        let tc = ToolCallData {
            thought_signature: None,
            id: "call_verify".to_string(),
            name: "exec".to_string(),
            arguments: serde_json::json!({"command":"cargo test 2>&1"}).to_string(),
        };

        let block = evaluator
            .build_violation_block(&tc)
            .expect("verify exec block");
        assert!(block.contains("patch_file, apply_diff"));
        assert!(block.contains("cargo test 2>&1"));
    }

    #[test]
    fn evaluator_loop_allows_verify_exec_after_mutation_exists() {
        let reflection_ledger = crate::reflection_ledger::ReflectionLedger::default();
        let evaluator = EvaluatorLoop::analyze(
            TaskHarness {
                lane: super::task_harness::TaskLane::FixExisting,
                artifact_mode: super::task_harness::ArtifactMode::ExistingFiles,
            },
            &base_fix_meta_harness(),
            &reflection_ledger,
            None,
            Some("cargo test 2>&1"),
            Some(5),
            0,
        );
        let tc = ToolCallData {
            thought_signature: None,
            id: "call_verify".to_string(),
            name: "exec".to_string(),
            arguments: serde_json::json!({"command":"cargo test 2>&1"}).to_string(),
        };

        assert!(evaluator.build_violation_block(&tc).is_none());
    }

    #[test]
    fn evaluator_loop_pauses_observation_policy_after_file_tool_failure() {
        let reflection_ledger = crate::reflection_ledger::ReflectionLedger::default();
        let evaluator = EvaluatorLoop::analyze(
            TaskHarness {
                lane: super::task_harness::TaskLane::FixExisting,
                artifact_mode: super::task_harness::ArtifactMode::ExistingFiles,
            },
            &base_fix_meta_harness(),
            &reflection_ledger,
            None,
            Some("cargo test 2>&1"),
            None,
            1,
        );

        assert!(evaluator.prompt().is_none());
    }
}
