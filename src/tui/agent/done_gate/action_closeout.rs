use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RepoScaffoldArtifacts {
    repo_root: String,
    logic_path: Option<String>,
    entrypoint_path: Option<String>,
}

pub(crate) fn known_commands_have_required_verification(
    known_commands: &[String],
    required_verification: VerificationLevel,
    test_cmd: Option<&str>,
) -> bool {
    known_commands.iter().any(|command| {
        classify_verify_level(command.as_str(), test_cmd)
            .map(|level| level.satisfies(required_verification))
            .unwrap_or(false)
    })
}

pub(crate) fn should_prefer_done_after_verified_action(
    tc: &ToolCallData,
    plan: &PlanBlock,
    known_commands: &[String],
    required_verification: VerificationLevel,
    test_cmd: Option<&str>,
    last_mutation_step: Option<usize>,
    last_verify_ok_step: Option<usize>,
) -> bool {
    if tc.name.as_str() == "done" {
        return false;
    }

    let last_mutation = last_mutation_step.unwrap_or(0);
    let last_verify_ok = last_verify_ok_step.unwrap_or(0);
    if last_mutation == 0 || last_verify_ok < last_mutation {
        return false;
    }

    if !plan.steps.iter().any(|step| finalization_step_hint(step)) {
        return false;
    }

    if !known_commands_have_required_verification(known_commands, required_verification, test_cmd) {
        return false;
    }

    match tc.name.as_str() {
        "read_file" | "search_files" | "list_dir" | "glob" => true,
        "exec" => {
            let command = parse_exec_command_from_args(tc.arguments.as_str()).unwrap_or_default();
            classify_verify_level(command.as_str(), test_cmd)
                .map(|level| level.satisfies(required_verification))
                .unwrap_or_else(|| is_diagnostic_command(command.as_str()))
        }
        _ => false,
    }
}

pub(crate) fn build_post_verify_done_completion_hint(
    plan: &PlanBlock,
    known_commands: &[String],
    tc: &ToolCallData,
    required_verification: VerificationLevel,
    test_cmd: Option<&str>,
) -> String {
    let attempted = canonicalize_tool_call_command(tc.name.as_str(), tc.arguments.as_str())
        .unwrap_or_else(|| blocked_tool_call_signature(tc.name.as_str(), tc.arguments.as_str()));
    let verification_commands = known_commands
        .iter()
        .filter(|command| {
            classify_verify_level(command.as_str(), test_cmd)
                .map(|level| level.satisfies(required_verification))
                .unwrap_or(false)
        })
        .take(3)
        .cloned()
        .collect::<Vec<_>>();

    let mut out = format!(
        "[Completion Gate]\n\
Required verification already passed after the latest mutation.\n\
Attempted next action: {attempted}\n\
Do NOT reopen inspection or rerun verification now.\n\
Next assistant turn: emit a <think> block with `tool: done`, then call `done` immediately.\n\
If any acceptance criterion is not fully proven, keep it in `remaining_acceptance` instead of gathering unrelated evidence.\n"
    );
    if !plan.acceptance_criteria.is_empty() {
        out.push_str("Current acceptance criteria:\n");
        for (idx, criterion) in plan.acceptance_criteria.iter().enumerate().take(4) {
            out.push_str(&format!(
                "- acceptance {}: {}\n",
                idx + 1,
                compact_one_line(criterion.as_str(), 160)
            ));
        }
    }
    if !verification_commands.is_empty() {
        out.push_str("Known successful verification commands you can cite now:\n");
        for command in verification_commands {
            out.push_str("- ");
            out.push_str(&compact_one_line(command.as_str(), 200));
            out.push('\n');
        }
    }
    out.push_str(
        "If you still need to explain remaining work, use `next_steps` in `done`; do not read unrelated files now.",
    );
    out
}

pub(crate) fn preferred_action_verification_command(
    known_commands: &[String],
    required_verification: VerificationLevel,
    test_cmd: Option<&str>,
) -> Option<String> {
    if let Some(configured) = test_cmd
        .map(str::trim)
        .filter(|command| !command.is_empty())
        .filter(|command| {
            configured_test_cmd_verification_level(Some(command))
                .map(|level| level.satisfies(required_verification))
                .unwrap_or(false)
        })
    {
        if let Some(candidate) = known_commands
            .iter()
            .find(|candidate| candidate.trim() == configured)
        {
            return Some(candidate.clone());
        }
    }

    known_commands.iter().rev().find_map(|command| {
        classify_verify_level(command.as_str(), test_cmd)
            .filter(|level| level.satisfies(required_verification))
            .map(|_| command.clone())
    })
}

pub(crate) fn preferred_action_observation_command(
    known_commands: &[String],
    observation_evidence: &ObservationEvidence,
) -> Option<String> {
    for prefix in ["read_file(", "search_files(", "glob(", "list_dir("] {
        if let Some(command) = known_commands.iter().rev().find(|command| {
            canonicalize_evidence_command_with_resolution(command.as_str(), observation_evidence)
                .starts_with(prefix)
        }) {
            return Some(command.clone());
        }
    }
    None
}

pub(crate) fn preferred_action_artifact_command(
    known_commands: &[String],
    observation_evidence: &ObservationEvidence,
) -> Option<String> {
    for prefix in ["write_file(", "patch_file(", "apply_diff("] {
        if let Some(command) = known_commands.iter().rev().find(|command| {
            canonicalize_evidence_command_with_resolution(command.as_str(), observation_evidence)
                .starts_with(prefix)
        }) {
            return Some(command.clone());
        }
    }
    None
}

pub(crate) fn artifact_command_path(
    command: &str,
    observation_evidence: &ObservationEvidence,
) -> Option<String> {
    let canonical = canonicalize_evidence_command_with_resolution(command, observation_evidence);
    let (name, args) = parse_named_command_signature(canonical.as_str())?;
    if !matches!(name.as_str(), "write_file" | "patch_file" | "apply_diff") {
        return None;
    }
    args.get("path").cloned()
}

pub(crate) fn criterion_prefers_artifact_proof(criterion: &str) -> bool {
    let low = criterion.to_ascii_lowercase();
    low.contains('/')
        || [
            ".rs",
            ".py",
            ".md",
            ".toml",
            ".json",
            ".yaml",
            ".yml",
            ".gitignore",
            "entrypoint",
            "readme",
            "gameplay",
            "logic",
        ]
        .iter()
        .any(|term| low.contains(term))
}

pub(crate) fn score_artifact_command_for_criterion(criterion: &str, path: &str) -> usize {
    let criterion_low = criterion.to_ascii_lowercase();
    let path_low = path.to_ascii_lowercase();
    let basename = path_low.rsplit('/').next().unwrap_or(path_low.as_str());

    let mut score = 0usize;
    if criterion_low.contains(path_low.as_str()) {
        score += 8;
    }
    if criterion_low.contains(basename) {
        score += 6;
    }
    for token in basename
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|token| token.len() >= 3)
    {
        if criterion_low.contains(token) {
            score += 1;
        }
    }
    score
}

pub(crate) fn preferred_action_artifact_command_for_criterion(
    criterion: &str,
    known_commands: &[String],
    observation_evidence: &ObservationEvidence,
) -> Option<String> {
    known_commands
        .iter()
        .rev()
        .filter_map(|command| {
            let path = artifact_command_path(command.as_str(), observation_evidence)?;
            Some((
                score_artifact_command_for_criterion(criterion, path.as_str()),
                command.clone(),
            ))
        })
        .filter(|(score, _)| *score > 0)
        .max_by_key(|(score, _)| *score)
        .map(|(_, command)| command)
        .or_else(|| preferred_action_artifact_command(known_commands, observation_evidence))
}

pub(crate) fn criterion_prefers_verification_proof(criterion: &str) -> bool {
    let low = criterion.to_ascii_lowercase();
    [
        "implement",
        "implemented",
        "change",
        "fix",
        "verify",
        "verification",
        "behavioral",
        "build",
        "lint",
        "test",
        "pass",
        "passes",
        "final result",
        "final answer",
        "cite",
        "command",
        "summary",
        "report",
    ]
    .iter()
    .any(|term| low.contains(term))
}

pub(crate) fn criterion_prefers_observation_proof(criterion: &str) -> bool {
    let low = criterion.to_ascii_lowercase();
    [
        "file", "path", "read", "context", "handler", "symbol", "location", "line",
    ]
    .iter()
    .any(|term| low.contains(term))
}

pub(crate) fn successful_mutation_paths(messages: &[serde_json::Value]) -> Vec<(String, String)> {
    let mut pending_by_id: std::collections::BTreeMap<String, (String, String)> =
        std::collections::BTreeMap::new();
    let mut successes = Vec::new();

    for msg in messages {
        match msg.get("role").and_then(|v| v.as_str()).unwrap_or("") {
            "assistant" => {
                let Some(tool_calls) = msg.get("tool_calls").and_then(|v| v.as_array()) else {
                    continue;
                };
                for tc in tool_calls {
                    let id = tc.get("id").and_then(|v| v.as_str()).unwrap_or("").trim();
                    let name = tc
                        .get("function")
                        .and_then(|v| v.get("name"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .trim();
                    if id.is_empty() || !matches!(name, "write_file" | "patch_file" | "apply_diff")
                    {
                        continue;
                    }
                    let Some(arguments) = tc
                        .get("function")
                        .and_then(|v| v.get("arguments"))
                        .and_then(|v| v.as_str())
                    else {
                        continue;
                    };
                    let path = serde_json::from_str::<serde_json::Value>(arguments)
                        .ok()
                        .and_then(|value| {
                            value
                                .get("path")
                                .and_then(|v| v.as_str())
                                .map(str::trim)
                                .filter(|path| !path.is_empty())
                                .map(ToString::to_string)
                        });
                    let Some(path) = path else {
                        continue;
                    };
                    pending_by_id.insert(id.to_string(), (name.to_string(), path));
                }
            }
            "tool" => {
                let id = msg
                    .get("tool_call_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim();
                if id.is_empty() {
                    continue;
                }
                let content = msg.get("content").and_then(|v| v.as_str()).unwrap_or("");
                if !looks_like_successful_mutation_result(content) {
                    continue;
                }
                if let Some(entry) = pending_by_id.get(id) {
                    successes.push(entry.clone());
                }
            }
            _ => {}
        }
    }

    successes
}

pub(crate) fn latest_successful_mutation_path(
    messages: &[serde_json::Value],
) -> Option<(String, String)> {
    successful_mutation_paths(messages).into_iter().last()
}

pub(crate) fn unique_successful_mutation_paths(messages: &[serde_json::Value]) -> Vec<String> {
    let mut out = Vec::new();
    for (_, path) in successful_mutation_paths(messages) {
        if !out.contains(&path) {
            out.push(path);
        }
    }
    out
}

pub(crate) fn inferred_followup_summary_paths(messages: &[serde_json::Value]) -> Vec<String> {
    let paths = unique_successful_mutation_paths(messages);
    let mut chosen = Vec::new();

    if let Some(code_path) = paths
        .iter()
        .rev()
        .find(|path| path.starts_with("src/") && path.ends_with(".rs"))
        .cloned()
    {
        chosen.push(code_path);
    }
    if let Some(docs_path) = paths
        .iter()
        .rev()
        .find(|path| path.starts_with("docs/") && path.ends_with(".md"))
        .cloned()
    {
        if !chosen.contains(&docs_path) {
            chosen.push(docs_path);
        }
    }
    if let Some(replay_path) = paths
        .iter()
        .rev()
        .find(|path| path.ends_with("tui_replay.json"))
        .cloned()
    {
        if !chosen.contains(&replay_path) {
            chosen.push(replay_path);
        }
    }

    chosen
}

pub(crate) fn format_path_list(paths: &[String]) -> String {
    let quoted = paths
        .iter()
        .map(|path| format!("`{}`", compact_one_line(path.as_str(), 160)))
        .collect::<Vec<_>>();
    match quoted.as_slice() {
        [] => String::new(),
        [only] => only.clone(),
        [first, second] => format!("{first} and {second}"),
        _ => {
            let mut out = quoted[..quoted.len() - 1].join(", ");
            out.push_str(", and ");
            out.push_str(&quoted[quoted.len() - 1]);
            out
        }
    }
}

pub(crate) fn repo_root_from_success_path(path: &str) -> Option<String> {
    let trimmed = path.trim().trim_end_matches('/');
    for suffix in [
        "/src/lib.rs",
        "/src/main.rs",
        "/README.md",
        "/.gitignore",
        "/Cargo.toml",
        "/game.py",
        "/main.py",
        "/test_game.py",
    ] {
        if let Some(root) = trimmed.strip_suffix(suffix) {
            let root = root.trim_end_matches('/');
            if !root.is_empty() {
                return Some(root.to_string());
            }
        }
    }
    None
}

pub(crate) fn preferred_repo_logic_path(repo_root: &str, paths: &[String]) -> Option<String> {
    let exact_candidates = [
        format!("{repo_root}/src/lib.rs"),
        format!("{repo_root}/game.py"),
        format!("{repo_root}/lib.py"),
        format!("{repo_root}/src/main.rs"),
        format!("{repo_root}/main.py"),
    ];
    for candidate in &exact_candidates {
        if paths.iter().any(|path| path == candidate) {
            return Some(candidate.clone());
        }
    }

    paths.iter().find_map(|path| {
        let low = path.to_ascii_lowercase();
        if !path.starts_with(&format!("{repo_root}/")) {
            return None;
        }
        if low.ends_with("/readme.md")
            || low.ends_with("/.gitignore")
            || low.ends_with("/cargo.toml")
            || low.ends_with("/test_game.py")
            || low.contains("/tests/")
            || low.contains("/test_")
        {
            return None;
        }
        if [".rs", ".py", ".js", ".ts"]
            .iter()
            .any(|ext| low.ends_with(ext))
        {
            return Some(path.clone());
        }
        None
    })
}

pub(crate) fn preferred_repo_entrypoint_path(repo_root: &str, paths: &[String]) -> Option<String> {
    let exact_candidates = [
        format!("{repo_root}/src/main.rs"),
        format!("{repo_root}/main.py"),
        format!("{repo_root}/main.js"),
        format!("{repo_root}/main.ts"),
    ];
    for candidate in &exact_candidates {
        if paths.iter().any(|path| path == candidate) {
            return Some(candidate.clone());
        }
    }
    None
}

pub(crate) fn inferred_repo_scaffold_artifacts(
    messages: &[serde_json::Value],
) -> Option<RepoScaffoldArtifacts> {
    let successes = successful_mutation_paths(messages);
    let mut groups: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();
    for (_, path) in &successes {
        let Some(repo_root) = repo_root_from_success_path(path.as_str()) else {
            continue;
        };
        groups.entry(repo_root).or_default().push(path.clone());
    }

    let (repo_root, paths) = groups.into_iter().max_by_key(|(repo_root, paths)| {
        let mut score = paths.len();
        if paths.iter().any(|path| path.ends_with("/README.md")) {
            score += 3;
        }
        if paths
            .iter()
            .any(|path| path.ends_with("/src/lib.rs") || path.ends_with("/game.py"))
        {
            score += 4;
        }
        if paths
            .iter()
            .any(|path| path.ends_with("/src/main.rs") || path.ends_with("/main.py"))
        {
            score += 2;
        }
        score + repo_root.len() / 1000
    })?;

    let logic_path = preferred_repo_logic_path(repo_root.as_str(), paths.as_slice());
    let entrypoint_path = preferred_repo_entrypoint_path(repo_root.as_str(), paths.as_slice());

    Some(RepoScaffoldArtifacts {
        repo_root,
        logic_path,
        entrypoint_path,
    })
}

pub(crate) fn looks_like_successful_mutation_result(content: &str) -> bool {
    let trimmed = content.trim_start();
    trimmed.starts_with("OK: wrote '")
        || trimmed.starts_with("OK: patched '")
        || trimmed.starts_with("OK: applied ")
}

pub(crate) fn latest_assistant_message_is_done(messages: &[serde_json::Value]) -> bool {
    messages
        .iter()
        .rev()
        .find(|msg| msg.get("role").and_then(|v| v.as_str()) == Some("assistant"))
        .and_then(|msg| msg.get("content").and_then(|v| v.as_str()))
        .map(|content| content.trim_start().starts_with("[DONE]"))
        .unwrap_or(false)
}

pub(crate) fn synthesize_action_done_summary(
    plan: Option<&PlanBlock>,
    messages: &[serde_json::Value],
) -> Option<String> {
    if let Some(artifacts) = inferred_repo_scaffold_artifacts(messages) {
        let repo_root = compact_one_line(artifacts.repo_root.as_str(), 160);
        let logic_path = artifacts
            .logic_path
            .as_deref()
            .map(|path| compact_one_line(path, 160));
        let entrypoint_path = artifacts
            .entrypoint_path
            .as_deref()
            .map(|path| compact_one_line(path, 160));
        return Some(match (logic_path, entrypoint_path) {
            (Some(logic), Some(entrypoint)) if logic != entrypoint => format!(
                "Created repository at `{repo_root}/` with main logic in `{logic}` and runnable entrypoint in `{entrypoint}`."
            ),
            (Some(logic), _) => {
                format!("Created repository at `{repo_root}/` with main logic in `{logic}`.")
            }
            (None, Some(entrypoint)) => format!(
                "Created repository at `{repo_root}/` with runnable entrypoint in `{entrypoint}`."
            ),
            (None, None) => format!("Created repository at `{repo_root}/` and verified its starter artifacts."),
        });
    }

    let followup_paths = inferred_followup_summary_paths(messages);
    if followup_paths.len() >= 2 {
        return Some(format!(
            "Updated {} and verified the requested change.",
            format_path_list(&followup_paths)
        ));
    }

    if let Some((tool, path)) = latest_successful_mutation_path(messages) {
        let path = compact_one_line(path.as_str(), 160);
        return Some(match tool.as_str() {
            "write_file" => format!("Created `{path}` and verified it."),
            "patch_file" | "apply_diff" => {
                format!("Updated `{path}` and verified the requested change.")
            }
            _ => format!("Completed the requested change in `{path}` and verified it."),
        });
    }

    plan.map(|plan| {
        let goal = compact_one_line(plan.goal.as_str(), 180);
        if goal == "-" {
            "Completed the requested task and verified it.".to_string()
        } else {
            format!("Completed and verified: {goal}")
        }
    })
}

pub(crate) fn maybe_build_verified_action_auto_final_answer(
    plan: Option<&PlanBlock>,
    messages: &[serde_json::Value],
    known_commands: &[String],
    observation_evidence: &ObservationEvidence,
    required_verification: VerificationLevel,
    test_cmd: Option<&str>,
    last_mutation_step: Option<usize>,
    last_verify_ok_step: Option<usize>,
) -> Option<String> {
    let plan = plan?;
    let (completed_acceptance, remaining_acceptance, acceptance_evidence) =
        rescue_invalid_done_payload_for_verified_action(
            Some(plan),
            known_commands,
            observation_evidence,
            required_verification,
            test_cmd,
            last_mutation_step,
            last_verify_ok_step,
        )?;
    let evidence_rows = validate_done_acceptance(
        Some(plan),
        &completed_acceptance,
        &remaining_acceptance,
        &acceptance_evidence,
        known_commands,
        observation_evidence,
    )
    .ok()?;
    let evidence_by_idx: std::collections::HashMap<usize, String> =
        evidence_rows.into_iter().collect();
    let summary = synthesize_action_done_summary(Some(plan), messages).unwrap_or_default();

    let mut final_text = String::from("[DONE]\n");
    if !summary.is_empty() {
        final_text.push_str(summary.as_str());
    }
    if let Some(artifacts) = inferred_repo_scaffold_artifacts(messages) {
        final_text.push_str("\n\nArtifacts:\n");
        final_text.push_str("- repo: `");
        final_text.push_str(compact_one_line(artifacts.repo_root.as_str(), 160).as_str());
        final_text.push_str("/`\n");
        if let Some(logic_path) = artifacts.logic_path.as_deref() {
            final_text.push_str("- main logic: `");
            final_text.push_str(compact_one_line(logic_path, 160).as_str());
            final_text.push_str("`\n");
        }
        if let Some(entrypoint_path) = artifacts.entrypoint_path.as_deref() {
            final_text.push_str("- entrypoint: `");
            final_text.push_str(compact_one_line(entrypoint_path, 160).as_str());
            final_text.push_str("`\n");
        }
    }
    final_text.push_str("\n\nAcceptance:\n");
    for item in &completed_acceptance {
        let idx = resolve_acceptance_reference(item, plan)?;
        final_text.push_str("- done: ");
        final_text.push_str(acceptance_reference_label(plan, idx).as_str());
        if let Some(command) = evidence_by_idx.get(&idx) {
            final_text.push_str(" via `");
            final_text.push_str(command.as_str());
            final_text.push('`');
        }
        final_text.push('\n');
    }
    for item in &remaining_acceptance {
        let idx = resolve_acceptance_reference(item, plan)?;
        final_text.push_str("- remaining: ");
        final_text.push_str(acceptance_reference_label(plan, idx).as_str());
        final_text.push('\n');
    }
    Some(final_text)
}

pub(crate) fn rescue_invalid_done_payload_for_verified_action(
    plan: Option<&PlanBlock>,
    known_commands: &[String],
    observation_evidence: &ObservationEvidence,
    required_verification: VerificationLevel,
    test_cmd: Option<&str>,
    last_mutation_step: Option<usize>,
    last_verify_ok_step: Option<usize>,
) -> Option<(Vec<String>, Vec<String>, Vec<DoneAcceptanceEvidence>)> {
    let plan = plan?;
    let last_mutation = last_mutation_step.unwrap_or(0);
    let last_verify_ok = last_verify_ok_step.unwrap_or(0);
    if last_mutation == 0 || last_verify_ok < last_mutation {
        return None;
    }

    let verify_command =
        preferred_action_verification_command(known_commands, required_verification, test_cmd);
    let observation_command =
        preferred_action_observation_command(known_commands, observation_evidence);

    let mut completed_acceptance = Vec::new();
    let mut remaining_acceptance = Vec::new();
    let mut acceptance_evidence = Vec::new();

    for (idx, criterion) in plan.acceptance_criteria.iter().enumerate() {
        let label = acceptance_reference_label(plan, idx);
        let artifact_command = preferred_action_artifact_command_for_criterion(
            criterion,
            known_commands,
            observation_evidence,
        );
        let command = if criterion_prefers_verification_proof(criterion) {
            verify_command
                .clone()
                .or_else(|| artifact_command.clone())
                .or_else(|| observation_command.clone())
        } else if criterion_prefers_observation_proof(criterion) {
            observation_command
                .clone()
                .or_else(|| artifact_command.clone())
                .or_else(|| verify_command.clone())
        } else if criterion_prefers_artifact_proof(criterion) {
            artifact_command
                .clone()
                .or_else(|| observation_command.clone())
                .or_else(|| verify_command.clone())
        } else {
            None
        };

        if let Some(command) = command {
            completed_acceptance.push(label.clone());
            acceptance_evidence.push(DoneAcceptanceEvidence {
                criterion: label,
                command,
            });
        } else {
            remaining_acceptance.push(label);
        }
    }

    if completed_acceptance.is_empty() {
        return None;
    }

    Some((
        completed_acceptance,
        remaining_acceptance,
        acceptance_evidence,
    ))
}

pub(crate) fn maybe_build_verified_action_closeout_text(
    root_user_text: &str,
    tool_root: Option<&str>,
    plan: Option<&PlanBlock>,
    messages: &[serde_json::Value],
    working_mem: &WorkingMemory,
    observation_evidence: &ObservationEvidence,
    required_verification: VerificationLevel,
    test_cmd: Option<&str>,
    last_mutation_step: Option<usize>,
    last_verify_ok_step: Option<usize>,
) -> Option<String> {
    if super::super::final_handoff::requires_authored_final_answer(root_user_text)
        || super::super::exact_content::validate_exact_content_task(root_user_text, tool_root)
            .is_err()
        || super::super::recovery_focus::RecoveryFocus::from_messages(messages, tool_root, test_cmd)
            .is_pending()
    {
        return None;
    }
    if latest_assistant_message_is_done(messages) {
        return None;
    }
    if super::super::followup_requirements::required_existing_followup_no_tool_hint(
        messages,
        root_user_text,
    )
    .is_some()
    {
        return None;
    }
    if super::super::task_harness::benchmark_plan_missing_required_exec_proof(
        root_user_text,
        messages,
        test_cmd,
    ) {
        return None;
    }

    let known_acceptance_commands = canonicalize_known_acceptance_commands(
        &super::super::collect_known_acceptance_commands(messages, working_mem, test_cmd),
        observation_evidence,
    );

    maybe_build_verified_action_auto_final_answer(
        plan,
        messages,
        &known_acceptance_commands,
        observation_evidence,
        required_verification,
        test_cmd,
        last_mutation_step,
        last_verify_ok_step,
    )
}
