use super::*;

pub(crate) fn consecutive_missing_plan_blocks_for_diagnostic_tools(
    messages: &[serde_json::Value],
) -> usize {
    let mut count = 0usize;
    let mut idx = messages.len();
    while idx >= 2 {
        let tool_msg = &messages[idx - 1];
        let assistant_msg = &messages[idx - 2];
        if !is_missing_plan_governor_block(tool_msg) {
            break;
        }
        let Some(name) = assistant_blocked_tool_name(assistant_msg) else {
            break;
        };
        if !is_diagnostic_tool_name(name.as_str()) {
            break;
        }
        count = count.saturating_add(1);
        idx -= 2;
    }
    count
}

pub(crate) fn consecutive_missing_think_blocks_for_diagnostic_tools(
    messages: &[serde_json::Value],
) -> usize {
    let mut count = 0usize;
    let mut idx = messages.len();
    while idx >= 2 {
        let tool_msg = &messages[idx - 1];
        let assistant_msg = &messages[idx - 2];
        if !is_missing_think_governor_block(tool_msg) {
            break;
        }
        let Some(name) = assistant_blocked_tool_name(assistant_msg) else {
            break;
        };
        if !is_diagnostic_tool_name(name.as_str()) {
            break;
        }
        count = count.saturating_add(1);
        idx -= 2;
    }
    count
}

pub(crate) fn synthetic_action_plan_goal(root_user_text: &str) -> String {
    let goal = compact_one_line(root_user_text.trim(), 220);
    if goal.is_empty() {
        "Complete the requested task safely and verify the result.".to_string()
    } else {
        goal
    }
}

pub(crate) fn synthetic_action_plan_first_step(
    tc: &ToolCallData,
    task_harness: TaskHarness,
) -> String {
    match task_harness.artifact_mode {
        ArtifactMode::NewFiles => {
            if let Some(command) =
                canonicalize_tool_call_command(tc.name.as_str(), tc.arguments.as_str())
            {
                return format!("{command} to create the requested file now");
            }
            return "create the requested file now with write_file or a minimal exec".to_string();
        }
        ArtifactMode::NewRepo => {
            if let Some(command) =
                canonicalize_tool_call_command(tc.name.as_str(), tc.arguments.as_str())
            {
                return format!("{command} to create the requested repo/project artifact now");
            }
            return "create the requested repo/project artifact now with write_file or exec"
                .to_string();
        }
        ArtifactMode::ObserveOnly | ArtifactMode::ExistingFiles => {}
    }

    if let Some(command) = canonicalize_tool_call_command(tc.name.as_str(), tc.arguments.as_str()) {
        match tc.name.as_str() {
            "read_file" | "search_files" | "list_dir" | "glob" => {
                return format!(
                    "{command} to inspect the current target and confirm the relevant context"
                );
            }
            _ => {
                return format!("{command} to gather the smallest safe evidence first");
            }
        }
    }

    match tc.name.as_str() {
        "read_file" => "read the current target file and confirm the edit context".to_string(),
        "search_files" => {
            "search for the current target and confirm the strongest match".to_string()
        }
        "list_dir" => "list the relevant directory and confirm the likely target files".to_string(),
        "glob" => "glob for the likely target files before editing".to_string(),
        _ => format!("use {} to gather the smallest safe evidence first", tc.name),
    }
}

pub(crate) fn synthetic_action_plan_verify_step(
    required_verification: VerificationLevel,
    test_cmd: Option<&str>,
) -> String {
    match required_verification {
        VerificationLevel::Behavioral => {
            let command = test_cmd
                .map(|cmd| compact_one_line(cmd, 160))
                .filter(|cmd| !cmd.is_empty())
                .unwrap_or_else(|| "run the targeted behavioral verification command".to_string());
            format!("{command} and confirm the requested behavior now passes")
        }
        VerificationLevel::Build => {
            let command = verification_examples(VerificationLevel::Build);
            if command.trim().is_empty() {
                "run a real build/check/lint verification command and confirm it passes".to_string()
            } else {
                format!("{command} and confirm the requested scope still builds cleanly")
            }
        }
    }
}

pub(crate) fn synthetic_action_plan_acceptance(
    required_verification: VerificationLevel,
    task_harness: TaskHarness,
) -> Vec<String> {
    if matches!(
        task_harness.artifact_mode,
        ArtifactMode::NewFiles | ArtifactMode::NewRepo
    ) {
        return match required_verification {
            VerificationLevel::Behavioral => vec![
                "the requested artifact is created and confirmed by a passing behavioral verification command"
                    .to_string(),
                "the final result cites the exact passing behavioral verification command"
                    .to_string(),
            ],
            VerificationLevel::Build => vec![
                "the requested artifact is created and confirmed by a passing build/check/lint verification command"
                    .to_string(),
                "the final result cites the exact passing build/check/lint verification command"
                    .to_string(),
            ],
        };
    }

    match required_verification {
        VerificationLevel::Behavioral => vec![
            "the requested change is implemented and confirmed by a passing behavioral verification command"
                .to_string(),
            "the final result cites the exact passing behavioral verification command".to_string(),
        ],
        VerificationLevel::Build => vec![
            "the requested change is implemented and confirmed by a passing build/check/lint verification command"
                .to_string(),
            "the final result cites the exact passing build/check/lint verification command"
                .to_string(),
        ],
    }
}

pub(crate) fn synthetic_action_plan_risks(
    required_verification: VerificationLevel,
    task_harness: TaskHarness,
) -> String {
    if matches!(
        task_harness.artifact_mode,
        ArtifactMode::NewFiles | ArtifactMode::NewRepo
    ) {
        return match required_verification {
            VerificationLevel::Behavioral => {
                "wrong path or incomplete artifact; the chosen test may miss the requested contents"
                    .to_string()
            }
            VerificationLevel::Build => {
                "wrong path or incomplete artifact; build-only verification may miss the requested contents"
                    .to_string()
            }
        };
    }

    match required_verification {
        VerificationLevel::Behavioral => {
            "wrong file or speculative fix; the chosen test may miss the requested behavior"
                .to_string()
        }
        VerificationLevel::Build => {
            "wrong file or speculative fix; build-only verification may miss the requested behavior"
                .to_string()
        }
    }
}

pub(crate) fn synthetic_action_plan(
    root_user_text: &str,
    tc: &ToolCallData,
    task_harness: TaskHarness,
    required_verification: VerificationLevel,
    test_cmd: Option<&str>,
) -> PlanBlock {
    let steps = match task_harness.artifact_mode {
        ArtifactMode::NewFiles => vec![
            synthetic_action_plan_first_step(tc, task_harness),
            synthetic_action_plan_verify_step(required_verification, test_cmd),
            "call done with the created file path and the verified outcome".to_string(),
        ],
        ArtifactMode::NewRepo => vec![
            synthetic_action_plan_first_step(tc, task_harness),
            "create any minimal companion files or directories required by the request".to_string(),
            synthetic_action_plan_verify_step(required_verification, test_cmd),
            "call done with the created repo/project path and the verified outcome".to_string(),
        ],
        ArtifactMode::ObserveOnly | ArtifactMode::ExistingFiles => vec![
            synthetic_action_plan_first_step(tc, task_harness),
            "read the strongest candidate file and confirm the exact edit context".to_string(),
            "apply the smallest evidence-backed code change in the confirmed target".to_string(),
            synthetic_action_plan_verify_step(required_verification, test_cmd),
            "call done with the verified outcome and any remaining gaps".to_string(),
        ],
    };

    PlanBlock {
        goal: synthetic_action_plan_goal(root_user_text),
        steps,
        acceptance_criteria: synthetic_action_plan_acceptance(required_verification, task_harness),
        risks: synthetic_action_plan_risks(required_verification, task_harness),
        assumptions:
            "the current repo state matches the task and the configured verification command is relevant"
                .to_string(),
    }
}

pub(crate) fn supports_action_task_rescues(provider: &ProviderKind) -> bool {
    matches!(
        provider,
        ProviderKind::OpenAiCompatible | ProviderKind::Mistral
    )
}

pub(crate) fn extract_loose_json_string_field(raw: &str, field: &str) -> Option<(String, bool)> {
    let needle = format!("\"{field}\"");
    let start = raw.find(needle.as_str())?;
    let after_key = &raw[start + needle.len()..];
    let colon = after_key.find(':')?;
    let mut tail = after_key[colon + 1..].trim_start();
    if !tail.starts_with('"') {
        return None;
    }
    tail = &tail[1..];

    let mut out = String::new();
    let mut chars = tail.chars();
    let mut escaped = false;
    while let Some(ch) = chars.next() {
        if escaped {
            match ch {
                '"' => out.push('"'),
                '\\' => out.push('\\'),
                '/' => out.push('/'),
                'n' => out.push('\n'),
                'r' => out.push('\r'),
                't' => out.push('\t'),
                'b' => out.push('\u{0008}'),
                'f' => out.push('\u{000C}'),
                'u' => {
                    let mut hex = String::new();
                    for _ in 0..4 {
                        let Some(h) = chars.next() else {
                            return Some((out, false));
                        };
                        hex.push(h);
                    }
                    if let Ok(value) = u16::from_str_radix(hex.as_str(), 16) {
                        if let Some(decoded) = char::from_u32(value as u32) {
                            out.push(decoded);
                        }
                    }
                }
                other => out.push(other),
            }
            escaped = false;
            continue;
        }
        match ch {
            '\\' => escaped = true,
            '"' => return Some((out, true)),
            other => out.push(other),
        }
    }

    Some((out, false))
}

pub(crate) fn extract_first_quoted_text(text: &str) -> Option<String> {
    let start = text.find('"')?;
    let mut out = String::new();
    let mut chars = text[start + 1..].chars();
    let mut escaped = false;
    while let Some(ch) = chars.next() {
        if escaped {
            out.push(match ch {
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                other => other,
            });
            escaped = false;
            continue;
        }
        match ch {
            '\\' => escaped = true,
            '"' => return Some(out),
            other => out.push(other),
        }
    }
    None
}

pub(crate) fn recent_assertion_string_mismatch(
    messages: &[serde_json::Value],
) -> Option<(String, String)> {
    for msg in messages.iter().rev() {
        let Some(content) = msg.get("content").and_then(|value| value.as_str()) else {
            continue;
        };
        let mut left: Option<String> = None;
        let mut right: Option<String> = None;
        for line in content.lines() {
            let trimmed = line.trim();
            if left.is_none() && trimmed.starts_with("left: ") {
                left = extract_first_quoted_text(trimmed);
            }
            if right.is_none() && trimmed.starts_with("right: ") {
                right = extract_first_quoted_text(trimmed);
            }
        }
        if let (Some(left), Some(right)) = (left, right) {
            return Some((left, right));
        }
    }
    None
}

pub(crate) fn mismatch_segment(left: &str, right: &str) -> Option<(String, String)> {
    if left == right {
        return None;
    }

    let left_chars: Vec<char> = left.chars().collect();
    let right_chars: Vec<char> = right.chars().collect();
    let mut prefix = 0usize;
    while prefix < left_chars.len()
        && prefix < right_chars.len()
        && left_chars[prefix] == right_chars[prefix]
    {
        prefix += 1;
    }

    let mut suffix = 0usize;
    while suffix + prefix < left_chars.len()
        && suffix + prefix < right_chars.len()
        && left_chars[left_chars.len() - 1 - suffix] == right_chars[right_chars.len() - 1 - suffix]
    {
        suffix += 1;
    }

    let left_mid = left_chars[prefix..left_chars.len().saturating_sub(suffix)]
        .iter()
        .collect::<String>();
    let right_mid = right_chars[prefix..right_chars.len().saturating_sub(suffix)]
        .iter()
        .collect::<String>();
    if left_mid.is_empty() && right_mid.is_empty() {
        None
    } else {
        Some((left_mid, right_mid))
    }
}

pub(crate) fn repair_truncated_patch_tool_call_from_recent_mismatch(
    messages: &[serde_json::Value],
    tc: &ToolCallData,
    root_read_only: bool,
    goal_wants_actions: bool,
    provider: ProviderKind,
    observations: &ObservationEvidence,
) -> Option<(ToolCallData, String, String)> {
    if root_read_only || !goal_wants_actions || !tc.can_rewrite() {
        return None;
    }
    if !matches!(provider, ProviderKind::OpenAiCompatible) || tc.name != "patch_file" {
        return None;
    }

    let parsed = serde_json::from_str::<serde_json::Value>(&tc.arguments).ok();
    if parsed
        .as_ref()
        .is_some_and(|value| value.get("json_pointer").is_some())
    {
        return None;
    }
    let parsed_path = parsed
        .as_ref()
        .and_then(|value| value.get("path").and_then(|v| v.as_str()))
        .map(str::to_string);
    let parsed_search = parsed
        .as_ref()
        .and_then(|value| value.get("search").and_then(|v| v.as_str()))
        .map(str::to_string);
    let parsed_replace = parsed
        .as_ref()
        .and_then(|value| value.get("replace").and_then(|v| v.as_str()))
        .map(str::to_string);
    if parsed_path.as_deref().unwrap_or("").trim().len() > 0
        && parsed_search.as_deref().unwrap_or("").trim().len() > 0
        && parsed_replace.as_deref().unwrap_or("").trim().len() > 0
    {
        return None;
    }

    let path = parsed_path.or_else(|| {
        extract_loose_json_string_field(tc.arguments.as_str(), "path").map(|(value, _)| value)
    })?;
    let search = parsed_search.or_else(|| {
        extract_loose_json_string_field(tc.arguments.as_str(), "search").map(|(value, _)| value)
    })?;
    if path.trim().is_empty()
        || search.trim().is_empty()
        || !observation_supports_target_path(path.as_str(), observations)
    {
        return None;
    }

    let replace_state = parsed_replace
        .map(|value| (value, true))
        .or_else(|| extract_loose_json_string_field(tc.arguments.as_str(), "replace"));
    if let Some((replace, closed)) = replace_state.as_ref() {
        if *closed && !replace.trim().is_empty() {
            return None;
        }
    }

    let (left, right) = recent_assertion_string_mismatch(messages)?;
    let repaired_replace = if search.contains(left.as_str()) {
        search.replacen(left.as_str(), right.as_str(), 1)
    } else if let Some((old, new)) = mismatch_segment(left.as_str(), right.as_str()) {
        if old.is_empty() || old.len() > 8 || search.matches(old.as_str()).count() != 1 {
            return None;
        }
        search.replacen(old.as_str(), new.as_str(), 1)
    } else {
        return None;
    };
    if repaired_replace == search {
        return None;
    }

    let rewritten = ToolCallData {
        thought_signature: None,
        id: tc.id.clone(),
        name: tc.name.clone(),
        arguments: json!({
            "path": path,
            "search": search,
            "replace": repaired_replace,
        })
        .to_string(),
    };
    let original = canonicalize_tool_call_command(tc.name.as_str(), tc.arguments.as_str())
        .unwrap_or_else(|| blocked_tool_call_signature(tc.name.as_str(), tc.arguments.as_str()));
    let coerced =
        canonicalize_tool_call_command(rewritten.name.as_str(), rewritten.arguments.as_str())
            .unwrap_or_else(|| {
                blocked_tool_call_signature(rewritten.name.as_str(), rewritten.arguments.as_str())
            });
    if original == coerced {
        None
    } else {
        Some((rewritten, original, coerced))
    }
}

pub(crate) fn rescue_missing_plan_for_tool_turn(
    messages: &[serde_json::Value],
    tc: &ToolCallData,
    root_user_text: &str,
    task_harness: TaskHarness,
    root_read_only: bool,
    goal_wants_actions: bool,
    provider: ProviderKind,
    required_verification: VerificationLevel,
    test_cmd: Option<&str>,
) -> Option<PlanBlock> {
    if root_read_only || !goal_wants_actions {
        return None;
    }
    if !supports_action_task_rescues(&provider) {
        return None;
    }
    let artifact_creation_turn = matches!(
        task_harness.artifact_mode,
        ArtifactMode::NewFiles | ArtifactMode::NewRepo
    ) && matches!(tc.name.as_str(), "write_file" | "exec");
    if !is_diagnostic_tool_name(tc.name.as_str()) && !artifact_creation_turn {
        return None;
    }

    let prior_blocks = consecutive_missing_plan_blocks_for_tool(messages, tc);
    let prior_diagnostic_blocks = consecutive_missing_plan_blocks_for_diagnostic_tools(messages);
    if artifact_creation_turn {
        if prior_blocks.saturating_add(1) < 2 {
            return None;
        }
    } else if matches!(provider, ProviderKind::Mistral) {
        if prior_blocks.saturating_add(1) < 2 && prior_diagnostic_blocks.saturating_add(1) < 2 {
            return None;
        }
    } else if prior_blocks.saturating_add(1) < 2 && prior_diagnostic_blocks.saturating_add(1) < 2 {
        return None;
    }

    Some(synthetic_action_plan(
        root_user_text,
        tc,
        task_harness,
        required_verification,
        test_cmd,
    ))
}

pub(crate) fn rescue_read_only_missing_plan_for_tool_turn(
    messages: &[serde_json::Value],
    tc: &ToolCallData,
    root_user_text: &str,
    root_read_only: bool,
    provider: ProviderKind,
) -> Option<PlanBlock> {
    if !root_read_only || !mistral_observation_tool(tc.name.as_str()) {
        return None;
    }
    let prior_blocks = consecutive_missing_plan_blocks_for_tool(messages, tc);
    let prior_observation_blocks = consecutive_missing_plan_blocks_for_observation(messages);
    if prior_blocks.saturating_add(1) < 2 && prior_observation_blocks.saturating_add(1) < 3 {
        return None;
    }
    if matches!(provider, ProviderKind::Mistral) {
        if let Some(repaired) =
            repair_mistral_plan_for_tool_turn(None, None, tc, root_user_text, root_read_only)
        {
            return Some(repaired);
        }
    }
    Some(synthetic_read_only_observation_plan(root_user_text))
}

pub(crate) fn rescue_read_only_missing_think_for_tool_turn(
    messages: &[serde_json::Value],
    tc: &ToolCallData,
    plan: &PlanBlock,
    root_user_text: &str,
    root_read_only: bool,
    provider: ProviderKind,
    evidence: &ObservationEvidence,
) -> Option<ThinkBlock> {
    if !root_read_only || !mistral_observation_tool(tc.name.as_str()) {
        return None;
    }
    if tc.name == "read_file" {
        return Some(compat_synthetic_think(tc, plan));
    }
    if tc.name == "done"
        && build_read_only_completion_hint(
            root_user_text,
            plan,
            evidence,
            messages,
            &WorkingMemory::default(),
        )
        .is_some()
    {
        return Some(compat_synthetic_think(tc, plan));
    }
    if matches!(provider, ProviderKind::Mistral) {
        return Some(compat_synthetic_think(tc, plan));
    }
    let prior_blocks = consecutive_missing_think_blocks_for_tool(messages, tc);
    let prior_observation_blocks = consecutive_missing_think_blocks_for_observation(messages);
    if prior_blocks.saturating_add(1) < 2 && prior_observation_blocks.saturating_add(1) < 2 {
        return None;
    }
    Some(compat_synthetic_think(tc, plan))
}

pub(crate) fn rescue_missing_think_for_tool_turn(
    messages: &[serde_json::Value],
    tc: &ToolCallData,
    plan: &PlanBlock,
    root_read_only: bool,
    goal_wants_actions: bool,
    provider: ProviderKind,
    force: bool,
) -> Option<ThinkBlock> {
    if root_read_only || !goal_wants_actions {
        return None;
    }
    if !supports_action_task_rescues(&provider) {
        return None;
    }
    if matches!(
        tc.name.as_str(),
        "exec" | "write_file" | "patch_file" | "apply_diff" | "done"
    ) {
        return Some(compat_synthetic_think(tc, plan));
    }
    if tc.name == "read_file" {
        return Some(compat_synthetic_think(tc, plan));
    }
    if !is_diagnostic_tool_name(tc.name.as_str()) {
        return None;
    }
    if matches!(provider, ProviderKind::Mistral) {
        return Some(compat_synthetic_think(tc, plan));
    }
    if force {
        return Some(compat_synthetic_think(tc, plan));
    }

    let prior_blocks = consecutive_missing_think_blocks_for_tool(messages, tc);
    let prior_diagnostic_blocks = consecutive_missing_think_blocks_for_diagnostic_tools(messages);
    if prior_blocks.saturating_add(1) < 2 && prior_diagnostic_blocks.saturating_add(1) < 2 {
        return None;
    }

    Some(compat_synthetic_think(tc, plan))
}

pub(crate) fn rescue_missing_reflection_for_tool_turn(
    reason: &str,
    tc: &ToolCallData,
    root_read_only: bool,
    goal_wants_actions: bool,
    provider: ProviderKind,
    mem: &FailureMemory,
    file_tool_consec_failures: usize,
) -> Option<ReflectionBlock> {
    if root_read_only || !goal_wants_actions {
        return None;
    }
    if !supports_action_task_rescues(&provider) {
        return None;
    }

    let reason_low = reason.to_ascii_lowercase();
    let last_outcome = if mem.consecutive_failures > 0
        || reason_low.contains("failure")
        || reason_low.contains("error")
        || reason_low.contains("wrong results")
    {
        "failure"
    } else {
        "partial"
    };
    let repeated_failure = mem.repeated_failure_or_stall() || file_tool_consec_failures >= 2;
    let strategy_change = if repeated_failure {
        StrategyChange::Abandon
    } else {
        StrategyChange::Adjust
    };
    let wrong_assumption = match tc.name.as_str() {
        "exec" => "verification happened before the fix was confirmed",
        "patch_file" | "apply_diff" | "write_file" => {
            "the intended edit was not yet precise enough"
        }
        "read_file" => "the current evidence was not specific enough",
        "search_files" | "list_dir" | "glob" => "discovery was still too broad",
        _ => "the previous action did not match the shortest safe next step",
    }
    .to_string();
    let next_minimal_action = match tc.name.as_str() {
        "exec" => "inspect the failing source before rerunning tests".to_string(),
        "patch_file" | "apply_diff" => serde_json::from_str::<serde_json::Value>(&tc.arguments)
            .ok()
            .and_then(|v| {
                v.get("path")
                    .and_then(|x| x.as_str())
                    .map(|path| format!("patch {path} with the smallest confirmed fix"))
            })
            .unwrap_or_else(|| "apply the smallest confirmed code fix".to_string()),
        "write_file" => serde_json::from_str::<serde_json::Value>(&tc.arguments)
            .ok()
            .and_then(|v| {
                v.get("path")
                    .and_then(|x| x.as_str())
                    .map(|path| format!("rewrite {path} only if replacement is necessary"))
            })
            .unwrap_or_else(|| "rewrite only the confirmed target file".to_string()),
        "read_file" => serde_json::from_str::<serde_json::Value>(&tc.arguments)
            .ok()
            .and_then(|v| {
                v.get("path")
                    .and_then(|x| x.as_str())
                    .map(|path| format!("read {path} and inspect the failing logic"))
            })
            .unwrap_or_else(|| "read the strongest failing code path".to_string()),
        "search_files" => "search for the exact failing symbol or assertion".to_string(),
        "list_dir" => "read the most likely source file instead of listing again".to_string(),
        "glob" => "inspect the strongest matching file instead of globbing again".to_string(),
        _ => format!("change approach before rerunning {}", tc.name),
    };

    Some(ReflectionBlock {
        last_outcome: last_outcome.to_string(),
        goal_delta: GoalDelta::Same,
        wrong_assumption,
        strategy_change,
        next_minimal_action,
    })
}

pub(crate) fn synthetic_impact_progress(plan: &PlanBlock) -> String {
    let chosen_idx = plan
        .steps
        .iter()
        .position(|step| {
            let low = step.to_ascii_lowercase();
            [
                "patch",
                "edit",
                "write",
                "apply",
                "fix",
                "change",
                "update",
                "implement",
            ]
            .iter()
            .any(|term| low.contains(term))
        })
        .unwrap_or(0);
    let label = plan
        .steps
        .get(chosen_idx)
        .cloned()
        .unwrap_or_else(|| "the current plan".to_string());
    format!("step {} moved because {}", chosen_idx + 1, label)
}

pub(crate) fn synthetic_impact_changed(reason: &str) -> String {
    let detail = reason
        .split_once(':')
        .map(|(_, rest)| compact_one_line(rest.trim(), 120))
        .unwrap_or_default();
    if detail.is_empty() {
        "applied the confirmed smallest fix".to_string()
    } else if reason.to_ascii_lowercase().contains("command") {
        format!("applied the confirmed mutation command: {detail}")
    } else {
        format!("applied the confirmed smallest fix in {detail}")
    }
}

pub(crate) fn synthetic_impact_remaining_gap(tc: &ToolCallData) -> String {
    match tc.name.as_str() {
        "exec" => parse_exec_command_from_args(&tc.arguments)
            .map(|command| {
                format!("still need to run {command} and confirm the acceptance criteria")
            })
            .unwrap_or_else(|| {
                "still need the next verification run and final confirmation".to_string()
            }),
        _ => format!(
            "still need the next planned action ({}) and final verification",
            tc.name
        ),
    }
}

pub(crate) fn rescue_missing_impact_for_tool_turn(
    reason: &str,
    tc: &ToolCallData,
    root_read_only: bool,
    goal_wants_actions: bool,
    provider: ProviderKind,
    plan: Option<&PlanBlock>,
    pending: Option<impact_recovery::UnreviewedMutation>,
) -> Option<ImpactBlock> {
    if root_read_only || !goal_wants_actions {
        return None;
    }
    if !supports_action_task_rescues(&provider) {
        return None;
    }
    let pending = pending?;
    let plan = plan?;

    Some(ImpactBlock {
        changed: if reason.contains(':') {
            synthetic_impact_changed(reason)
        } else {
            format!(
                "recorded workspace mutation at step {} still requires acceptance verification",
                pending.step
            )
        },
        progress: synthetic_impact_progress(plan),
        remaining_gap: synthetic_impact_remaining_gap(tc),
    })
}

pub(crate) fn rescue_missing_evidence_for_tool_turn(
    tc: &ToolCallData,
    root_read_only: bool,
    goal_wants_actions: bool,
    provider: ProviderKind,
    observations: &ObservationEvidence,
) -> Option<EvidenceBlock> {
    if root_read_only || !goal_wants_actions {
        return None;
    }
    if !supports_action_task_rescues(&provider) {
        return None;
    }
    if !mutation_tool_requires_evidence(tc) {
        return None;
    }
    let target_path = mutation_target_path(tc)?;
    if !observation_supports_target_path(target_path.as_str(), observations) {
        return None;
    }

    Some(EvidenceBlock {
        target_files: vec![target_path.clone()],
        target_symbols: vec!["confirmed edit region".to_string()],
        evidence: format!(
            "read_file(path={target_path}) already confirmed the current code at the mutation target"
        ),
        open_questions: "none".to_string(),
        next_probe: format!("patch {target_path} with the smallest confirmed fix"),
    })
}

pub(crate) fn repair_mistral_plan_for_tool_turn(
    parsed_plan: Option<&PlanBlock>,
    active_plan: Option<&PlanBlock>,
    tc: &ToolCallData,
    root_user_text: &str,
    root_read_only: bool,
) -> Option<PlanBlock> {
    if !root_read_only || !mistral_observation_tool(tc.name.as_str()) {
        return None;
    }
    let fallback = synthetic_read_only_observation_plan(root_user_text);
    let mut repaired = active_plan.cloned().unwrap_or_else(|| fallback.clone());
    if let Some(parsed) = parsed_plan {
        if !parsed.goal.trim().is_empty() {
            repaired.goal = parsed.goal.clone();
        }
        if !parsed.steps.is_empty() {
            repaired.steps = parsed.steps.clone();
        }
        if !parsed.acceptance_criteria.is_empty() {
            repaired.acceptance_criteria = parsed.acceptance_criteria.clone();
        }
        if !parsed.risks.trim().is_empty() {
            repaired.risks = parsed.risks.clone();
        }
        if !parsed.assumptions.trim().is_empty() {
            repaired.assumptions = parsed.assumptions.clone();
        }
    }
    if repaired.steps.len() < plan_field_min_items("steps").unwrap_or(2) {
        repaired.steps = fallback.steps;
    }
    if repaired.acceptance_criteria.len() < plan_field_min_items("acceptance").unwrap_or(1) {
        repaired.acceptance_criteria = fallback.acceptance_criteria;
    }
    if repaired.risks.trim().is_empty() {
        repaired.risks = fallback.risks;
    }
    if repaired.assumptions.trim().is_empty() {
        repaired.assumptions = fallback.assumptions;
    }
    Some(repaired)
}

pub(crate) fn compat_synthetic_think(tc: &ToolCallData, plan: &PlanBlock) -> ThinkBlock {
    let next = match tc.name.as_str() {
        "exec" => parse_exec_command_from_args(&tc.arguments)
            .unwrap_or_else(|| "run the command".to_string()),
        "read_file" => serde_json::from_str::<serde_json::Value>(&tc.arguments)
            .ok()
            .and_then(|v| {
                v.get("path")
                    .and_then(|x| x.as_str())
                    .map(|path| format!("read {path}"))
            })
            .unwrap_or_else(|| "read the target file".to_string()),
        "search_files" => serde_json::from_str::<serde_json::Value>(&tc.arguments)
            .ok()
            .and_then(|v| {
                let pattern = v
                    .get("pattern")
                    .and_then(|x| x.as_str())
                    .unwrap_or("pattern");
                let dir = v.get("dir").and_then(|x| x.as_str()).unwrap_or(".");
                Some(format!("search {pattern} in {dir}"))
            })
            .unwrap_or_else(|| "search the codebase".to_string()),
        "list_dir" => serde_json::from_str::<serde_json::Value>(&tc.arguments)
            .ok()
            .and_then(|v| {
                v.get("dir")
                    .and_then(|x| x.as_str())
                    .map(|dir| format!("list {dir}"))
            })
            .unwrap_or_else(|| "list the directory".to_string()),
        "glob" => serde_json::from_str::<serde_json::Value>(&tc.arguments)
            .ok()
            .and_then(|v| {
                let pattern = v.get("pattern").and_then(|x| x.as_str()).unwrap_or("*");
                let dir = v.get("dir").and_then(|x| x.as_str()).unwrap_or(".");
                Some(format!("glob {pattern} in {dir}"))
            })
            .unwrap_or_else(|| "glob for candidate files".to_string()),
        "write_file" => serde_json::from_str::<serde_json::Value>(&tc.arguments)
            .ok()
            .and_then(|v| {
                v.get("path")
                    .and_then(|x| x.as_str())
                    .map(|path| format!("write {path}"))
            })
            .unwrap_or_else(|| "write the requested file".to_string()),
        "patch_file" => serde_json::from_str::<serde_json::Value>(&tc.arguments)
            .ok()
            .and_then(|v| {
                v.get("path")
                    .and_then(|x| x.as_str())
                    .map(|path| format!("patch {path}"))
            })
            .unwrap_or_else(|| "patch the target file".to_string()),
        "done" => "call done immediately".to_string(),
        _ => format!("run {}", tc.name),
    };

    ThinkBlock {
        goal: compact_one_line(plan.goal.as_str(), 120),
        step: 1,
        tool: tc.name.clone(),
        risk: "wrong target or overly broad action".to_string(),
        doubt: "may need one narrower follow-up".to_string(),
        next,
        verify: "confirm the tool output matches the request".to_string(),
    }
}

pub(crate) fn select_think_for_tool_turn<'a>(
    parsed: Option<&'a ThinkBlock>,
    compat_last: Option<&'a ThinkBlock>,
    compat_synth: Option<&'a ThinkBlock>,
    plan: &PlanBlock,
    tc: &ToolCallData,
    provider: ProviderKind,
) -> (Option<&'a ThinkBlock>, bool) {
    if matches!(provider, ProviderKind::Mistral) {
        if let Some(think) = parsed.filter(|think| validate_think(think, plan, tc).is_ok()) {
            return (Some(think), false);
        }
        if let Some(think) = compat_synth.filter(|think| validate_think(think, plan, tc).is_ok()) {
            return (Some(think), true);
        }
        if let Some(think) = compat_last.filter(|think| validate_think(think, plan, tc).is_ok()) {
            return (Some(think), false);
        }
        return (
            parsed.or(compat_synth).or(compat_last),
            compat_synth.is_some(),
        );
    }

    if let Some(think) = parsed.filter(|think| validate_think(think, plan, tc).is_ok()) {
        return (Some(think), false);
    }
    if let Some(think) = compat_last.filter(|think| validate_think(think, plan, tc).is_ok()) {
        return (Some(think), false);
    }
    if let Some(think) = compat_synth.filter(|think| validate_think(think, plan, tc).is_ok()) {
        return (Some(think), true);
    }

    let fallback = parsed.or(compat_last).or(compat_synth);
    let used_synth = fallback
        .zip(compat_synth)
        .map(|(selected, synth)| std::ptr::eq(selected, synth))
        .unwrap_or(false);
    (fallback, used_synth)
}
