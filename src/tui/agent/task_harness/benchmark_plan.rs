use super::*;

pub(crate) fn is_approved_observer_benchmark_plan(low: &str) -> bool {
    low.contains("[observer benchmark plan approved]") || low.contains("<observer_benchmark_plan")
}

pub(crate) fn approved_benchmark_required_commands(root_user_text: &str) -> Vec<String> {
    if is_approved_observer_benchmark_plan(&root_user_text.to_ascii_lowercase())
        || super::super::requirement_proof::declared(root_user_text)
    {
        benchmark_proof::required_commands(root_user_text)
    } else {
        Vec::new()
    }
}

pub(crate) fn coerce_benchmark_plan_tool_call(
    harness: TaskHarness,
    messages: &[Value],
    tc: &ToolCallData,
    root_user_text: &str,
    tool_root: Option<&str>,
    recovery_stage: Option<RecoveryStage>,
    test_cmd: Option<&str>,
) -> Option<(ToolCallData, String, String)> {
    // Recovery owns diagnosis, repair and re-verification. A target's matching
    // text does not prove it is valid or authorize replacing a repair with a check.
    if recovery_stage.is_some() {
        return None;
    }
    if harness.lane != TaskLane::BenchmarkPlan {
        return None;
    }
    if !is_benchmark_plan_candidate_tool(tc) {
        return None;
    }
    let reads = successful_read_contents(messages);
    for target_path in benchmark_plan_target_paths(root_user_text) {
        let target_was_read = reads.contains_key(target_path.as_str());
        let read_body = reads
            .get(target_path.as_str())
            .filter(|body| !body.trim().is_empty())
            .cloned();
        let body = target_was_read
            .then(|| read_tool_root_file(tool_root, target_path.as_str()).or(read_body))
            .flatten();
        if let Some(body) = body.as_deref() {
            if benchmark_plan_target_is_satisfied(target_path.as_str(), body, root_user_text) {
                continue;
            }
            if benchmark_plan_tool_call_is_complete_target_mutation(tc, target_path.as_str()) {
                return None;
            }
            let rewritten =
                synthesize_benchmark_plan_target_patch(target_path.as_str(), body, root_user_text)?;
            return compare_rewritten_tool_call(tc, rewritten);
        }

        if tc.name == "read_file" && tool_call_path(tc).as_deref() == Some(target_path.as_str()) {
            return None;
        }

        let rewritten = ToolCallData {
            thought_signature: None,
            id: tc.id.clone(),
            name: "read_file".to_string(),
            arguments: serde_json::json!({ "path": target_path }).to_string(),
        };
        return compare_rewritten_tool_call(tc, rewritten);
    }

    if let Some(command) =
        pending_benchmark_plan_required_check_exec(messages, root_user_text, test_cmd)
    {
        if benchmark_plan_tool_call_is_required_check_exec(tc, command.as_str()) {
            return None;
        }
        let rewritten = ToolCallData {
            thought_signature: None,
            id: tc.id.clone(),
            name: "exec".to_string(),
            arguments: serde_json::json!({ "command": command }).to_string(),
        };
        return compare_rewritten_tool_call(tc, rewritten);
    }

    None
}

pub(crate) fn synthesize_benchmark_plan_no_tool_call(
    harness: TaskHarness,
    messages: &[Value],
    root_user_text: &str,
    tool_root: Option<&str>,
    recovery_stage: Option<RecoveryStage>,
    test_cmd: Option<&str>,
) -> Option<ToolCallData> {
    // Keep all active recovery operations ahead of benchmark assistance.
    if recovery_stage.is_some() {
        return None;
    }
    let fallback = ToolCallData {
        thought_signature: None,
        id: "synthetic_benchmark_plan_no_tool".to_string(),
        name: "done".to_string(),
        arguments: serde_json::json!({
            "summary": "benchmark plan follow-up pending"
        })
        .to_string(),
    };
    coerce_benchmark_plan_tool_call(
        harness,
        messages,
        &fallback,
        root_user_text,
        tool_root,
        recovery_stage,
        test_cmd,
    )
    .map(|(rewritten, _original, _coerced)| rewritten)
}

pub(crate) fn allows_benchmark_plan_followup_during_verify(
    harness: TaskHarness,
    messages: &[Value],
    tc: &ToolCallData,
    root_user_text: &str,
    tool_root: Option<&str>,
    test_cmd: Option<&str>,
) -> bool {
    if harness.lane != TaskLane::BenchmarkPlan {
        return false;
    }
    if benchmark_plan_targets_satisfied(messages, root_user_text, tool_root)
        && pending_benchmark_plan_required_check_exec(messages, root_user_text, test_cmd)
            .is_some_and(|command| {
                benchmark_plan_tool_call_is_required_check_exec(tc, command.as_str())
            })
    {
        return true;
    }
    let Some(tc_path) = tool_call_path(tc) else {
        return false;
    };
    let reads = successful_read_contents(messages);
    for target_path in benchmark_plan_target_paths(root_user_text) {
        let target_was_read = reads.contains_key(target_path.as_str());
        let read_body = reads
            .get(target_path.as_str())
            .filter(|body| !body.trim().is_empty())
            .cloned();
        let body = target_was_read
            .then(|| read_tool_root_file(tool_root, target_path.as_str()).or(read_body))
            .flatten();
        if let Some(body) = body.as_deref() {
            if benchmark_plan_target_is_satisfied(target_path.as_str(), body, root_user_text) {
                continue;
            }
            return tc_path == target_path
                && benchmark_plan_tool_call_is_complete_target_mutation(tc, target_path.as_str());
        }

        return tc.name == "read_file" && tc_path == target_path;
    }

    false
}

pub(crate) fn benchmark_plan_pending_required_exec_command(
    root_user_text: &str,
    messages: &[Value],
    test_cmd: Option<&str>,
) -> Option<String> {
    root_user_text
        .contains("<observer_benchmark_plan")
        .then(|| pending_benchmark_plan_required_check_exec(messages, root_user_text, test_cmd))
        .flatten()
}

pub(crate) fn benchmark_plan_missing_required_exec_proof(
    root_user_text: &str,
    messages: &[Value],
    test_cmd: Option<&str>,
) -> bool {
    benchmark_plan_pending_required_exec_command(root_user_text, messages, test_cmd).is_some()
}

pub(crate) fn benchmark_plan_verification_hint(
    root_user_text: &str,
    messages: &[Value],
    test_cmd: Option<&str>,
) -> Option<String> {
    benchmark_proof::verification_hint(messages, root_user_text, test_cmd)
}

pub(crate) fn benchmark_plan_protected_call_ids(messages: &[Value]) -> BTreeSet<String> {
    benchmark_proof::protected_call_ids(messages)
}

pub(crate) fn compare_rewritten_tool_call(
    tc: &ToolCallData,
    rewritten: ToolCallData,
) -> Option<(ToolCallData, String, String)> {
    let original = canonicalize_tool_call_command(tc.name.as_str(), tc.arguments.as_str())
        .unwrap_or_else(|| {
            format!(
                "{}({})",
                tc.name,
                compact_one_line(tc.arguments.as_str(), 120)
            )
        });
    let coerced =
        canonicalize_tool_call_command(rewritten.name.as_str(), rewritten.arguments.as_str())
            .unwrap_or_else(|| {
                format!(
                    "{}({})",
                    rewritten.name,
                    compact_one_line(rewritten.arguments.as_str(), 120)
                )
            });
    // Display strings may collapse quoted whitespace or truncate long commands.
    // Compare the actual arguments before deciding whether a rewrite is redundant.
    let same_arguments = match (
        serde_json::from_str::<Value>(&tc.arguments),
        serde_json::from_str::<Value>(&rewritten.arguments),
    ) {
        (Ok(original), Ok(rewritten)) => original == rewritten,
        _ => tc.arguments == rewritten.arguments,
    };
    if tc.name == rewritten.name && same_arguments {
        None
    } else {
        Some((rewritten, original, coerced))
    }
}

pub(crate) fn is_benchmark_plan_candidate_tool(tc: &ToolCallData) -> bool {
    matches!(
        tc.name.as_str(),
        "read_file"
            | "search_files"
            | "list_dir"
            | "glob"
            | "exec"
            | "done"
            | "patch_file"
            | "apply_diff"
    )
}

pub(crate) fn benchmark_plan_spec_path(root_user_text: &str) -> Option<String> {
    let low = root_user_text.to_ascii_lowercase();
    if low.contains("lane: tui_replay") || low.contains("\"lane\": \"tui_replay\"") {
        return Some(".spiral-coder/tui_replay.json".to_string());
    }
    if low.contains("lane: runtime_eval") || low.contains("\"lane\": \"runtime_eval\"") {
        return Some(".spiral-coder/runtime_eval.json".to_string());
    }
    if low.contains(".spiral-coder/tui_replay.json") {
        return Some(".spiral-coder/tui_replay.json".to_string());
    }
    if low.contains(".spiral-coder/runtime_eval.json") {
        return Some(".spiral-coder/runtime_eval.json".to_string());
    }
    None
}

pub(crate) fn benchmark_plan_target_paths(root_user_text: &str) -> Vec<String> {
    if let Some(scope) = crate::task_scope::artifact_scope(root_user_text) {
        return scope.target_files;
    }
    let mut out = Vec::new();
    if let Some(path) = benchmark_plan_spec_path(root_user_text) {
        push_unique_path(&mut out, path);
    }
    for path in path_literals_in_text(root_user_text) {
        if is_benchmark_plan_target_path(path.as_str()) {
            push_unique_path(&mut out, path);
        }
    }
    out
}

pub(crate) fn is_benchmark_plan_target_path(path: &str) -> bool {
    path.starts_with("docs/") && path.ends_with(".md")
        || path == ".spiral-coder/runtime_eval.json"
        || path == ".spiral-coder/tui_replay.json"
        || path.ends_with("/.spiral-coder/runtime_eval.json")
        || path.ends_with("/.spiral-coder/tui_replay.json")
}

pub(crate) fn push_unique_path(out: &mut Vec<String>, path: String) {
    if !out.contains(&path) {
        out.push(path);
    }
}

pub(crate) fn pending_benchmark_plan_required_check_exec(
    messages: &[Value],
    root_user_text: &str,
    test_cmd: Option<&str>,
) -> Option<String> {
    benchmark_proof::pending_command(messages, root_user_text, test_cmd)
}

pub(crate) fn benchmark_plan_targets_satisfied(
    messages: &[Value],
    root_user_text: &str,
    tool_root: Option<&str>,
) -> bool {
    let targets = benchmark_plan_target_paths(root_user_text);
    if targets.is_empty() {
        return false;
    }
    let reads = successful_read_contents(messages);
    targets.into_iter().all(|target_path| {
        let target_was_read = reads.contains_key(target_path.as_str());
        let read_body = reads
            .get(target_path.as_str())
            .filter(|body| !body.trim().is_empty())
            .cloned();
        let body = target_was_read
            .then(|| read_tool_root_file(tool_root, target_path.as_str()).or(read_body))
            .flatten();
        body.as_deref().is_some_and(|body| {
            benchmark_plan_target_is_satisfied(target_path.as_str(), body, root_user_text)
        })
    })
}

pub(crate) fn benchmark_plan_tool_call_is_required_check_exec(
    tc: &ToolCallData,
    command: &str,
) -> bool {
    if tc.name != "exec" {
        return false;
    }
    serde_json::from_str::<Value>(tc.arguments.as_str())
        .ok()
        .and_then(|value| {
            value
                .get("command")
                .and_then(|v| v.as_str())
                .map(str::to_string)
        })
        .is_some_and(|actual| benchmark_proof::commands_match(actual.as_str(), command))
}

pub(crate) fn tool_call_path(tc: &ToolCallData) -> Option<String> {
    serde_json::from_str::<Value>(tc.arguments.as_str())
        .ok()
        .and_then(|value| {
            value
                .get("path")
                .and_then(|v| v.as_str())
                .map(str::to_string)
        })
}

pub(crate) fn read_tool_root_file(tool_root: Option<&str>, path: &str) -> Option<String> {
    let root = tool_root?.trim();
    if root.is_empty() {
        return None;
    }
    std::fs::read_to_string(std::path::Path::new(root).join(path)).ok()
}

pub(crate) fn benchmark_plan_tool_call_is_complete_target_mutation(
    tc: &ToolCallData,
    target: &str,
) -> bool {
    if !matches!(tc.name.as_str(), "patch_file" | "apply_diff") {
        return false;
    }
    let Some(parsed) = serde_json::from_str::<Value>(tc.arguments.as_str()).ok() else {
        return false;
    };
    if parsed.get("path").and_then(|v| v.as_str()) != Some(target) {
        return false;
    }
    match tc.name.as_str() {
        "patch_file" => {
            if parsed.get("json_pointer").and_then(Value::as_str).is_some()
                && parsed
                    .get("json_value")
                    .and_then(Value::as_str)
                    .is_some_and(|s| serde_json::from_str::<Value>(s).is_ok())
            {
                return true;
            }
            parsed
                .get("search")
                .and_then(|v| v.as_str())
                .is_some_and(|value| !value.trim().is_empty())
                && parsed
                    .get("replace")
                    .and_then(|v| v.as_str())
                    .is_some_and(|value| !value.trim().is_empty())
        }
        "apply_diff" => parsed
            .get("diff")
            .and_then(|v| v.as_str())
            .is_some_and(|value| !value.trim().is_empty()),
        _ => false,
    }
}

pub(crate) fn benchmark_plan_target_is_satisfied(
    _target: &str,
    body: &str,
    root_user_text: &str,
) -> bool {
    benchmark_plan_required_path_literal(root_user_text)
        .is_some_and(|required_path| body.contains(required_path.as_str()))
}

pub(crate) fn synthesize_benchmark_plan_target_patch(
    target: &str,
    body: &str,
    root_user_text: &str,
) -> Option<ToolCallData> {
    let required_path = benchmark_plan_required_path_literal(root_user_text)?;
    if body.contains(required_path.as_str()) {
        return None;
    }
    let replace = if target.ends_with(".json") {
        synthesize_benchmark_plan_json_body(target, body, required_path.as_str(), root_user_text)?
    } else if target.starts_with("docs/") && target.ends_with(".md") {
        synthesize_benchmark_plan_markdown_body(body, required_path.as_str())
    } else {
        return None;
    };
    Some(ToolCallData {
        thought_signature: None,
        id: "synthetic_benchmark_plan_patch".to_string(),
        name: "patch_file".to_string(),
        arguments: serde_json::json!({
            "path": target,
            "search": body,
            "replace": replace,
        })
        .to_string(),
    })
}

pub(crate) fn synthesize_benchmark_plan_json_body(
    target: &str,
    body: &str,
    required_path: &str,
    root_user_text: &str,
) -> Option<String> {
    let mut value = serde_json::from_str::<Value>(body).ok()?;
    if target.ends_with("/tui_replay.json") {
        // Use the replay contract, not runtime-eval check names or a literal-only update.
        // Validate without round-tripping old cases through typed structs:
        // extension fields and nested evidence must survive byte-for-value.
        serde_json::from_value::<crate::tui_replay::TuiReplaySpec>(value.clone()).ok()?;
        let case = benchmark_replay::case(
            benchmark_plan_case_id_hint(root_user_text)
                .unwrap_or_else(|| "observer-benchmark-plan-regression".to_string()),
            required_path,
        );
        let cases = value.get_mut("cases")?.as_array_mut()?;
        if cases.iter().any(|old| old["id"] == case.id) {
            return None;
        }
        cases.push(serde_json::to_value(case).ok()?);
    } else if let Some(paths) = value.get_mut("paths").and_then(|v| v.as_array_mut()) {
        paths.push(Value::String(required_path.to_string()));
    } else if let Some(cases) = value.get_mut("cases").and_then(|v| v.as_array_mut()) {
        cases.push(serde_json::json!({
            "id": benchmark_plan_case_id_hint(root_user_text)
                .unwrap_or_else(|| "observer-benchmark-plan-regression".to_string()),
            "tags": ["benchmark-plan"],
            "prompt": format!(
                "Keep `{required_path}` covered by the approved Observer benchmark plan."
            ),
            "checks": [
                { "kind": "completed" },
                { "kind": "assistant_contains", "value": required_path }
            ]
        }));
    } else {
        return None;
    }
    let mut replace = serde_json::to_string_pretty(&value).ok()?;
    if body.ends_with('\n') {
        replace.push('\n');
    }
    Some(replace)
}

pub(crate) fn synthesize_benchmark_plan_markdown_body(body: &str, required_path: &str) -> String {
    let mut out = body.trim_end().to_string();
    if !out.is_empty() {
        out.push_str("\n\n");
    }
    out.push_str("## Observer Benchmark Plan Coverage\n\n");
    out.push_str("- `");
    out.push_str(required_path);
    out.push_str("` must stay covered by approved Observer benchmark plans.\n");
    if body.ends_with('\n') {
        out.push('\n');
    }
    out
}

pub(crate) fn benchmark_plan_required_path_literal(root_user_text: &str) -> Option<String> {
    path_literals_in_text(root_user_text)
        .into_iter()
        .find(|path| path.starts_with("src/") && path.ends_with(".rs"))
}

pub(crate) fn benchmark_plan_case_id_hint(root_user_text: &str) -> Option<String> {
    root_user_text.lines().find_map(|line| {
        line.trim()
            .strip_prefix("case_id_hint:")
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    })
}

/// Return a concrete example snippet to guide weak models when adding a new
/// Observer benchmark plan case to a JSON spec file.
pub(crate) fn benchmark_plan_example_hint(root_user_text: &str) -> Option<String> {
    let low = root_user_text.to_ascii_lowercase();
    let case_id = benchmark_plan_case_id_hint(root_user_text)?;
    let required_path = benchmark_plan_required_path_literal(root_user_text)?;

    if low.contains("lane: tui_replay") || low.contains("\"lane\": \"tui_replay\"") {
        let prompt = format!(
            "Find the role of `{}`. Read the target before widening the search.",
            required_path
        );
        let json_example = serde_json::json!({
            "id": "{CASE_ID}",
            "prompt": "{PROMPT}",
            "tool_root": null,
            "lang": "en",
            "selector": null,
            "reason_hint": null,
            "tags": ["benchmark-plan", "observer-handoff"],
            "coder_messages": [
                {"role": "user", "content": "{PROMPT}"},
                {"role": "assistant", "content": "read_file(path={REQUIRED_PATH})"}
            ],
            "observer_response": {
                "summary": "Read the requested target before broadening the search.",
                "primary_blocker": "missing_concrete_next_step",
                "quickest_check": "read_file(path={REQUIRED_PATH})",
                "why_this_first": "Keep the continuation attached to the approved target.",
                "suggestions": [
                    {
                        "kind": "read",
                        "reason": "The approved benchmark names this target as the next focused read.",
                        "confidence": 0.9,
                        "suggested_tool": "read_file",
                        "suggested_args": {"path": "{REQUIRED_PATH}"},
                        "based_on": ["intent_anchor", "failure_kind"]
                    }
                ]
            },
            "checks": [
                {"kind": "suggestion_parsed"},
                {"kind": "hint_queued"},
                {"kind": "observer_contains", "value": "{REQUIRED_PATH}"},
                {"kind": "coder_system_contains", "value": "read_file(path={REQUIRED_PATH})"}
            ]
        })
        .to_string()
        .replace("{CASE_ID}", &case_id)
        .replace("{PROMPT}", &prompt)
        .replace("{REQUIRED_PATH}", &required_path);

        let action_example = format!(
            "\n[Benchmark plan action example]\n\
For the first tool call, emit a valid `<plan>` block, then a literal tab/newline/think block, then call `read_file(path={})`:\n\
<plan>\n\
goal: Add the smallest TUI replay regression for `{}` to `.spiral-coder/tui_replay.json`.\n\
steps:\n\
1) Read `.spiral-coder/tui_replay.json` to understand the existing case shape.\n\
2) Append a new case with id `{}` that targets `{}`.\n\
3) Confirm `{}` appears in `.spiral-coder/tui_replay.json`.\n\
4) Run the required tui-replay check to verify the new case passes.\n\
acceptance:\n\
1) `.spiral-coder/tui_replay.json` contains `{}`.\n\
2) The required replay check passes.\n\
risks: corrupting existing cases; use the structured JSON append mode.\n\
assumptions: the existing spec version and defaults remain valid.\n\
</plan>\n\
\t\n\
goal: Add the smallest TUI replay regression for `{}`.\n\
step: 1\n\
tool: read_file\n\
risk: misreading the JSON structure\n\
doubt: none\n\
next: read the current `.spiral-coder/tui_replay.json` content\n\
verify: see the existing cases list\n\
\t\n",
            required_path, required_path, case_id, required_path, required_path, required_path,
            required_path
        );
        Some(format!(
            "\n[Benchmark plan example]\n\
When appending to `.spiral-coder/tui_replay.json`, add one new case object like this (replace `<case-id>` with the actual `case_id_hint` value and `<target-path>` with the required path):\n\
{}\n\
Preserve all existing cases unchanged; only append this new case.\n\
{}",
            json_example, action_example
        ))
    } else {
        None
    }
}
