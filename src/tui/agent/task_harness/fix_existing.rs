use super::*;

pub(crate) fn coerce_fix_existing_tool_call(
    harness: TaskHarness,
    messages: &[Value],
    tc: &ToolCallData,
    test_cmd: Option<&str>,
    tool_root: Option<&str>,
) -> Option<(ToolCallData, String, String)> {
    if harness.lane != TaskLane::FixExisting || harness.artifact_mode != ArtifactMode::ExistingFiles
    {
        return None;
    }

    let wants_verify_exec = tc.name == "exec"
        && parse_exec_command_from_args(tc.arguments.as_str())
            .and_then(|command| classify_verify_level(command.as_str(), test_cmd))
            .is_some();
    if !wants_verify_exec && !is_observation_tool(tc.name.as_str()) {
        return None;
    }
    // A module guessed from a previous read is a discovery hint. Keep a
    // declared artifact read or a verification call intact. Its ordinary
    // governor and exact-proof checks still decide execution and completion.
    if let Some(contract) =
        super::super::requirement_proof::parse(crate::task_origin::root_user_text(messages)).ok()?
    {
        let reads_artifact = tc.name == "read_file"
            && tool_call_path(tc).is_some_and(|path| contract.names_artifact(&path));
        if wants_verify_exec || reads_artifact {
            return None;
        }
    }
    let mut path = match explicit_existing_rust_path_from_tool_call(tc, tool_root) {
        Some(path) => path,
        None => match direct_impl_from_attempted_subdir_listing(tc, tool_root) {
            Some(path) => path,
            None => match infer_fix_existing_focus(messages) {
                Some(FixExistingFocus::ReadImplementation(path)) => path,
                Some(FixExistingFocus::PatchImplementation(_)) => return None,
                None => concrete_impl_from_recent_subdir_listing(messages, tool_root)
                    .or_else(|| fallback_fix_existing_anchor(messages, tool_root))?,
            },
        },
    };
    path = resolve_existing_rust_path(path.as_str(), tool_root);
    if let Some(concrete_impl) = concrete_impl_from_recent_subdir_listing(messages, tool_root) {
        if path == "src/lib.rs" || path.ends_with("/mod.rs") {
            path = concrete_impl;
        }
    }
    if tc.name == "read_file"
        && serde_json::from_str::<Value>(tc.arguments.as_str())
            .ok()
            .and_then(|value| {
                value
                    .get("path")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
            })
            .as_deref()
            == Some(path.as_str())
    {
        return None;
    }

    let rewritten = ToolCallData {
        thought_signature: None,
        id: tc.id.clone(),
        name: "read_file".to_string(),
        arguments: serde_json::json!({ "path": path }).to_string(),
    };
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
    if original == coerced {
        None
    } else {
        Some((rewritten, original, coerced))
    }
}

pub(crate) fn coerce_fix_existing_blocked_mutation_tool_call(
    harness: TaskHarness,
    messages: &[Value],
    tc: &ToolCallData,
    root_user_text: &str,
    recovery_stage: Option<RecoveryStage>,
) -> Option<(ToolCallData, String, String)> {
    // Diagnosis and verification own the next action in those recovery stages.
    // Rewriting a requested diagnostic into a patch makes the recovery gate
    // reject the very operation it asked for, causing an unrecoverable loop.
    if matches!(
        recovery_stage,
        Some(RecoveryStage::Diagnose | RecoveryStage::Verify)
    ) {
        return None;
    }

    if harness.lane != TaskLane::FixExisting || harness.artifact_mode != ArtifactMode::ExistingFiles
    {
        return None;
    }
    if !is_observation_tool(tc.name.as_str()) {
        return None;
    }

    let rewritten = recent_blocked_fix_existing_mutation_action(messages, root_user_text)?;
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
    if original == coerced {
        None
    } else {
        Some((rewritten, original, coerced))
    }
}

pub(crate) fn repair_fix_existing_mutation_tool_call(
    harness: TaskHarness,
    messages: &[Value],
    tc: &ToolCallData,
    root_user_text: &str,
) -> Option<(ToolCallData, String, String)> {
    if harness.lane != TaskLane::FixExisting || harness.artifact_mode != ArtifactMode::ExistingFiles
    {
        return None;
    }
    if !matches!(tc.name.as_str(), "patch_file" | "apply_diff") {
        return None;
    }

    let reads = successful_read_contents(messages);
    let rewritten = validate_or_repair_blocked_fix_existing_mutation(tc, &reads, root_user_text)?;
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
    if original == coerced {
        None
    } else {
        Some((rewritten, original, coerced))
    }
}

pub(crate) fn coerce_fix_existing_literal_mutation_tool_call(
    harness: TaskHarness,
    messages: &[Value],
    tc: &ToolCallData,
    root_user_text: &str,
    recovery_stage: Option<RecoveryStage>,
) -> Option<(ToolCallData, String, String)> {
    // Diagnosis and verification own the next action in those recovery stages.
    // Rewriting a requested diagnostic into a patch makes the recovery gate
    // reject the very operation it asked for, causing an unrecoverable loop.
    if matches!(
        recovery_stage,
        Some(RecoveryStage::Diagnose | RecoveryStage::Verify)
    ) {
        return None;
    }

    if harness.lane != TaskLane::FixExisting || harness.artifact_mode != ArtifactMode::ExistingFiles
    {
        return None;
    }
    if !is_observation_tool(tc.name.as_str()) {
        return None;
    }

    let rewritten = synthesize_fix_existing_literal_member_patch(messages, root_user_text)?;
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
    if original == coerced {
        None
    } else {
        Some((rewritten, original, coerced))
    }
}

pub(crate) fn synthesize_fix_existing_no_tool_mutation_tool_call(
    harness: TaskHarness,
    messages: &[Value],
    root_user_text: &str,
    recovery_stage: Option<RecoveryStage>,
) -> Option<ToolCallData> {
    // Diagnosis and verification own the next action in those recovery stages.
    // Rewriting a requested diagnostic into a patch makes the recovery gate
    // reject the very operation it asked for, causing an unrecoverable loop.
    if matches!(
        recovery_stage,
        Some(RecoveryStage::Diagnose | RecoveryStage::Verify)
    ) {
        return None;
    }

    if harness.lane != TaskLane::FixExisting || harness.artifact_mode != ArtifactMode::ExistingFiles
    {
        return None;
    }
    let target_path = match infer_fix_existing_focus(messages)? {
        FixExistingFocus::PatchImplementation(path) => path,
        FixExistingFocus::ReadImplementation(_) => return None,
    };
    let reads = successful_read_contents(messages);
    let body = reads.get(target_path.as_str())?;
    synthesize_failed_status_guard_patch(target_path.as_str(), body, root_user_text)
}

pub(crate) fn coerce_repo_goal_completion_tool_call(
    harness: TaskHarness,
    messages: &[Value],
    tc: &ToolCallData,
    test_cmd: Option<&str>,
    tool_root: Option<&str>,
) -> Option<(ToolCallData, String, String)> {
    if harness.artifact_mode != ArtifactMode::NewRepo || !is_observation_tool(tc.name.as_str()) {
        return None;
    }
    let goal_check_missing = latest_goal_check_missing(messages);
    let status = repo_scaffold_status(messages, test_cmd?, tool_root)?;
    if goal_check_missing.is_empty() && !has_repo_scaffold_progress(&status) {
        return None;
    }
    let rewritten = next_repo_scaffold_tool_call(tc.id.as_str(), &status)?;
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
            .unwrap_or_else(|| compact_one_line(rewritten.arguments.as_str(), 200));
    if original == coerced {
        None
    } else {
        Some((rewritten, original, coerced))
    }
}

pub(crate) fn repair_repo_scaffold_write_tool_call(
    harness: TaskHarness,
    messages: &[Value],
    tc: &ToolCallData,
    test_cmd: Option<&str>,
    tool_root: Option<&str>,
) -> Option<(ToolCallData, String, String)> {
    if harness.artifact_mode != ArtifactMode::NewRepo || tc.name != "write_file" {
        return None;
    }
    if !write_file_looks_malformed(tc) {
        return None;
    }
    let status = repo_scaffold_status(messages, test_cmd?, tool_root)?;
    let rewritten = next_repo_scaffold_tool_call(tc.id.as_str(), &status)?;
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
            .unwrap_or_else(|| compact_one_line(rewritten.arguments.as_str(), 200));
    if original == coerced {
        None
    } else {
        Some((rewritten, original, coerced))
    }
}

pub(crate) fn allows_artifact_creation_during_diagnose(
    harness: TaskHarness,
    tc: &ToolCallData,
) -> bool {
    matches!(
        harness.artifact_mode,
        ArtifactMode::NewFiles | ArtifactMode::NewRepo
    ) && matches!(tc.name.as_str(), "write_file" | "exec")
}

pub(crate) fn allows_artifact_creation_during_verify(
    harness: TaskHarness,
    tc: &ToolCallData,
) -> bool {
    harness.artifact_mode == ArtifactMode::NewRepo
        && matches!(tc.name.as_str(), "write_file" | "exec")
}

pub(crate) fn build_fix_stage_progress_hint(
    harness: TaskHarness,
    messages: &[Value],
    recovery_stage: Option<RecoveryStage>,
    test_cmd: Option<&str>,
) -> Option<String> {
    if recovery_stage != Some(RecoveryStage::Fix)
        || harness.artifact_mode == ArtifactMode::ObserveOnly
    {
        return None;
    }

    let history = observation_history(messages);
    let threshold = match harness.artifact_mode {
        ArtifactMode::ExistingFiles => {
            if history
                .by_command
                .keys()
                .any(|command| command.starts_with("read_file("))
            {
                1
            } else {
                2
            }
        }
        ArtifactMode::NewFiles | ArtifactMode::NewRepo => 1,
        ArtifactMode::ObserveOnly => unreachable!(),
    };
    if history.total_successes < threshold {
        return None;
    }

    let task = crate::task_origin::current_task_messages(messages);
    if harness.lane == TaskLane::FixExisting
        && super::super::explicit_edit_scope::Scope::parse(crate::task_origin::root_user_text(task))
            .is_some()
        && !super::super::meta_harness::observation_window_since_last_mutation(task, test_cmd)
            .searched
    {
        return Some("[Task Harness]\nIf the edit is clear, apply it to a listed file. If you still need the implementation or a caller, search for one literal identifier in a listed file or its immediate directory. Use the returned locations for the next action.".into());
    }

    let recent = history
        .last_successful_command
        .as_deref()
        .unwrap_or("recent observation");
    let fix_focus = infer_fix_existing_focus(messages);
    let action = match harness.artifact_mode {
        ArtifactMode::ExistingFiles => match harness.lane {
            TaskLane::BenchmarkPlan => {
                "patch the approved benchmark spec/fixture now with `patch_file` or `apply_diff`"
                    .to_string()
            }
            _ => match fix_focus.as_ref() {
                Some(FixExistingFocus::ReadImplementation(path)) => format!(
                    "read `{}` now to inspect the implementation before patching",
                    compact_one_line(path, 140)
                ),
                Some(FixExistingFocus::PatchImplementation(path)) => format!(
                    "apply the smallest edit now with `patch_file` or `apply_diff` on `{}`",
                    compact_one_line(path, 140)
                ),
                None => "apply the smallest edit now with `patch_file` or `apply_diff`".to_string(),
            },
        },
        ArtifactMode::NewFiles => {
            "create the requested file now with `write_file` or a minimal `exec`".to_string()
        }
        ArtifactMode::NewRepo => {
            "create the requested repo/project artifact now with `write_file` or `exec`".to_string()
        }
        ArtifactMode::ObserveOnly => unreachable!(),
    };
    let verify_hint = test_cmd
        .filter(|cmd| !cmd.trim().is_empty())
        .map(|cmd| format!("Then verify with `{}`.", compact_one_line(cmd, 140)))
        .unwrap_or_else(|| "Then run a real command that proves the artifact exists.".to_string());

    Some(format!(
        "[Task Harness]\n\
Task lane: {}\n\
Recent successful observation: {}\n\
You already have enough context for the current phase.\n\
Required now: {}.\n\
{}\n\
Do NOT continue with more read/search/list calls unless the target is still genuinely ambiguous.",
        harness.lane_label(),
        compact_one_line(recent, 160),
        action,
        verify_hint
    ))
}

pub(crate) fn fix_existing_target_hint(messages: &[Value]) -> Option<String> {
    let path = match infer_fix_existing_focus(messages)? {
        FixExistingFocus::ReadImplementation(path)
        | FixExistingFocus::PatchImplementation(path) => path,
    };
    let reads = successful_read_contents(messages);
    let function = infer_fix_existing_symbol(messages, &reads, path.as_str());
    Some(match function {
        Some(function) => format!("{path}::{function}"),
        None => path,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FixExistingFocus {
    ReadImplementation(String),
    PatchImplementation(String),
}

pub(crate) fn infer_fix_existing_focus(messages: &[Value]) -> Option<FixExistingFocus> {
    let scope = super::super::explicit_edit_scope::Scope::parse(
        crate::task_origin::root_user_text(messages),
    );
    let messages = if scope.is_some() {
        crate::task_origin::current_task_messages(messages)
    } else {
        messages
    };
    let eligible = |candidate: &String| scope.as_ref().is_none_or(|s| s.allows(candidate, None));
    let reads = successful_read_contents(messages);
    let target = search_derived_fix_existing_path(messages)
        .filter(eligible)
        .or_else(|| {
            ["src/lib.rs", "src/main.rs", "lib.rs", "main.rs"]
                .iter()
                .find_map(|entry| {
                    let body = reads.get(*entry)?;
                    let candidates: Vec<String> = rust_module_file_candidates(entry, body)
                        .into_iter()
                        .flat_map(|candidate| {
                            let mut forms = vec![candidate.clone()];
                            if let Some(stem) = candidate.strip_suffix(".rs") {
                                forms.push(format!("{stem}/mod.rs"));
                            }
                            forms
                        })
                        .filter(|candidate| !candidate.trim().is_empty() && eligible(candidate))
                        .collect();
                    // A module file that was already read is a better focus than
                    // a guessed sibling (e.g. `tui.rs`) that may not exist on
                    // disk when the repo uses `tui/mod.rs` layout.
                    candidates
                        .iter()
                        .find(|candidate| reads.contains_key(candidate.as_str()))
                        .or_else(|| candidates.first())
                        .cloned()
                })
        })
        .or_else(|| latest_successful_source_read_path(messages, &reads).filter(eligible))?;

    if reads.contains_key(target.as_str()) {
        Some(FixExistingFocus::PatchImplementation(target))
    } else {
        Some(FixExistingFocus::ReadImplementation(target))
    }
}

pub(crate) fn search_derived_fix_existing_path(messages: &[Value]) -> Option<String> {
    for hit in successful_search_hits(messages).into_iter().rev() {
        let Some(query) = normalize_rust_ident_fragment(hit.query.as_str()) else {
            continue;
        };
        for candidate in rust_module_file_candidates(hit.path.as_str(), hit.snippet.as_str()) {
            let stem = std::path::Path::new(candidate.as_str())
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("")
                .trim();
            if !stem.is_empty() && stem == query {
                return Some(candidate);
            }
        }
    }
    None
}

pub(crate) fn fallback_fix_existing_anchor(
    messages: &[Value],
    tool_root: Option<&str>,
) -> Option<String> {
    let reads = successful_read_contents(messages);
    if reads.contains_key("src/lib.rs") || reads.contains_key("src/main.rs") {
        return None;
    }

    let history = observation_history(messages);
    let saw_src_discovery = history.by_command.keys().any(|command| {
        command.starts_with("search_files(") && command.contains("dir=src")
            || command.starts_with("list_dir(") && command.contains("dir=src")
    });
    if !saw_src_discovery {
        return None;
    }

    let root = tool_root?.trim();
    if root.is_empty() {
        return None;
    }
    let lib = std::path::Path::new(root).join("src/lib.rs");
    if lib.exists() {
        return Some("src/lib.rs".to_string());
    }
    let main = std::path::Path::new(root).join("src/main.rs");
    if main.exists() {
        return Some("src/main.rs".to_string());
    }
    None
}

pub(crate) fn concrete_impl_from_recent_subdir_listing(
    messages: &[Value],
    tool_root: Option<&str>,
) -> Option<String> {
    let root = tool_root?.trim();
    if root.is_empty() {
        return None;
    }
    let listed_dir = latest_successful_list_dir_path(messages)?;
    if !listed_dir.starts_with("src/") || listed_dir == "src" {
        return None;
    }
    concrete_impl_under_dir(root, listed_dir.as_str())
}

pub(crate) fn direct_impl_from_attempted_subdir_listing(
    tc: &ToolCallData,
    tool_root: Option<&str>,
) -> Option<String> {
    if tc.name != "list_dir" {
        return None;
    }
    let root = tool_root?.trim();
    if root.is_empty() {
        return None;
    }
    let dir = serde_json::from_str::<Value>(tc.arguments.as_str())
        .ok()
        .and_then(|value| {
            value
                .get("dir")
                .and_then(|v| v.as_str())
                .map(str::to_string)
        })?;
    if !dir.starts_with("src/") || dir == "src" {
        return None;
    }
    concrete_impl_under_dir(root, dir.as_str())
}

pub(crate) fn explicit_existing_rust_path_from_tool_call(
    tc: &ToolCallData,
    tool_root: Option<&str>,
) -> Option<String> {
    if !matches!(tc.name.as_str(), "read_file" | "patch_file" | "apply_diff") {
        return None;
    }
    let path = serde_json::from_str::<Value>(tc.arguments.as_str())
        .ok()
        .and_then(|value| {
            value
                .get("path")
                .and_then(|v| v.as_str())
                .map(str::to_string)
        })?;
    if !path.starts_with("src/") || !path.ends_with(".rs") {
        return None;
    }
    let resolved = resolve_existing_rust_path(path.as_str(), tool_root);
    let root = tool_root?.trim();
    if root.is_empty() {
        return Some(resolved);
    }
    std::path::Path::new(root)
        .join(resolved.as_str())
        .exists()
        .then_some(resolved)
}

pub(crate) fn concrete_impl_under_dir(root: &str, dir: &str) -> Option<String> {
    let abs_dir = std::path::Path::new(root).join(dir);
    let entries = std::fs::read_dir(abs_dir).ok()?;
    let mut rust_files = entries
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            name.ends_with(".rs").then_some(name)
        })
        .collect::<Vec<_>>();
    rust_files.sort();
    let concrete = rust_files
        .into_iter()
        .filter(|name| !matches!(name.as_str(), "mod.rs" | "lib.rs" | "main.rs"))
        .collect::<Vec<_>>();
    if concrete.len() != 1 {
        return None;
    }
    Some(format!("{}/{}", dir, concrete[0]))
}

pub(crate) fn latest_successful_list_dir_path(messages: &[Value]) -> Option<String> {
    let mut pending: BTreeMap<String, String> = BTreeMap::new();
    let mut out = None;

    for msg in messages {
        let role = msg.get("role").and_then(|v| v.as_str()).unwrap_or("");
        match role {
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
                    if id.is_empty() || name != "list_dir" {
                        continue;
                    }
                    let dir = tc
                        .get("function")
                        .and_then(|v| v.get("arguments"))
                        .and_then(|v| v.as_str())
                        .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
                        .and_then(|value| {
                            value
                                .get("dir")
                                .and_then(|v| v.as_str())
                                .map(str::to_string)
                        })
                        .unwrap_or_default();
                    pending.insert(id.to_string(), dir);
                }
            }
            "tool" => {
                let tool_call_id = msg
                    .get("tool_call_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim();
                let Some(dir) = pending.remove(tool_call_id) else {
                    continue;
                };
                let content = msg.get("content").and_then(|v| v.as_str()).unwrap_or("");
                if non_exec_tool_succeeded(content) {
                    out = Some(dir);
                }
            }
            _ => {}
        }
    }

    out
}

pub(crate) fn latest_successful_source_read_path(
    messages: &[Value],
    reads: &BTreeMap<String, String>,
) -> Option<String> {
    for msg in messages.iter().rev() {
        if msg.get("role").and_then(|v| v.as_str()) != Some("assistant") {
            continue;
        }
        let Some(tool_calls) = msg.get("tool_calls").and_then(|v| v.as_array()) else {
            continue;
        };
        for tc in tool_calls.iter().rev() {
            let name = tc
                .get("function")
                .and_then(|v| v.get("name"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();
            if name != "read_file" {
                continue;
            }
            let path = tc
                .get("function")
                .and_then(|v| v.get("arguments"))
                .and_then(|v| v.as_str())
                .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
                .and_then(|value| {
                    value
                        .get("path")
                        .and_then(|v| v.as_str())
                        .map(str::to_string)
                });
            let Some(path) = path else {
                continue;
            };
            let path = normalize_observed_path(path.as_str());
            if reads.contains_key(path.as_str())
                && path.starts_with("src/")
                && path.ends_with(".rs")
            {
                return Some(path);
            }
        }
    }
    reads
        .keys()
        .rev()
        .find(|path| path.starts_with("src/") && path.ends_with(".rs"))
        .cloned()
}

pub(crate) fn resolve_existing_rust_path(path: &str, tool_root: Option<&str>) -> String {
    let Some(root) = tool_root.map(str::trim).filter(|root| !root.is_empty()) else {
        return path.to_string();
    };
    let abs = std::path::Path::new(root).join(path);
    if abs.exists() {
        return path.to_string();
    }
    if !path.ends_with(".rs") {
        return path.to_string();
    }
    let stem = path.trim_end_matches(".rs");
    let mod_path = format!("{stem}/mod.rs");
    if std::path::Path::new(root).join(mod_path.as_str()).exists() {
        mod_path
    } else {
        path.to_string()
    }
}

/// Lexical identity for an observed file path. Tool arguments keep their
/// original spelling for display, but observation maps key on this form so
/// `./src/x.rs` and `src/x.rs` resolve to the same cached read.
pub(crate) fn normalize_observed_path(path: &str) -> String {
    let mut trimmed = path.trim();
    while let Some(rest) = trimmed.strip_prefix("./") {
        trimmed = rest;
    }
    trimmed.trim_end_matches('/').to_string()
}

fn read_file_call_target(tc: &Value) -> Option<(String, bool)> {
    let arguments = tc
        .get("function")
        .and_then(|v| v.get("arguments"))
        .and_then(|v| v.as_str())
        .and_then(|raw| serde_json::from_str::<Value>(raw).ok())?;
    let path = arguments
        .get("path")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .unwrap_or_default();
    if path.trim().is_empty() {
        return None;
    }
    let ranged = arguments.get("start_line").is_some() || arguments.get("end_line").is_some();
    Some((normalize_observed_path(path.as_str()), ranged))
}

/// Paths that were fully read at least once (no start/end line range). A
/// ranged read still lands in `successful_read_contents`, but only an
/// unranged read proves the whole file was observed.
pub(crate) fn successful_full_read_paths(messages: &[Value]) -> std::collections::BTreeSet<String> {
    let mut pending: BTreeMap<String, String> = BTreeMap::new();
    let mut out = std::collections::BTreeSet::new();

    for msg in messages {
        let role = msg.get("role").and_then(|v| v.as_str()).unwrap_or("");
        match role {
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
                    if id.is_empty() || name != "read_file" {
                        continue;
                    }
                    if let Some((path, ranged)) = read_file_call_target(tc) {
                        if !ranged {
                            pending.insert(id.to_string(), path);
                        }
                    }
                }
            }
            "tool" => {
                let tool_call_id = msg
                    .get("tool_call_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim();
                let Some(path) = pending.remove(tool_call_id) else {
                    continue;
                };
                let content = msg.get("content").and_then(|v| v.as_str()).unwrap_or("");
                if non_exec_tool_succeeded(content) {
                    out.insert(path);
                }
            }
            _ => {}
        }
    }
    out
}

pub(crate) fn successful_read_contents(messages: &[Value]) -> BTreeMap<String, String> {
    let mut pending: BTreeMap<String, String> = BTreeMap::new();
    let mut out = BTreeMap::new();

    for msg in messages {
        let role = msg.get("role").and_then(|v| v.as_str()).unwrap_or("");
        match role {
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
                    if id.is_empty() || name != "read_file" {
                        continue;
                    }
                    if let Some((path, _)) = read_file_call_target(tc) {
                        pending.insert(id.to_string(), path);
                    }
                }
            }
            "tool" => {
                let tool_call_id = msg
                    .get("tool_call_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim();
                let Some(path) = pending.remove(tool_call_id) else {
                    continue;
                };
                let content = msg.get("content").and_then(|v| v.as_str()).unwrap_or("");
                if !non_exec_tool_succeeded(content) {
                    continue;
                }
                let body = content
                    .split_once('\n')
                    .map(|(_, rest)| rest.to_string())
                    .unwrap_or_default();
                out.insert(path, body);
            }
            _ => {}
        }
    }

    out
}

#[derive(Debug, Clone)]
pub(crate) struct SuccessfulSearchHit {
    query: String,
    path: String,
    snippet: String,
}

pub(crate) fn successful_search_hits(messages: &[Value]) -> Vec<SuccessfulSearchHit> {
    let mut pending: BTreeMap<String, String> = BTreeMap::new();
    let mut out = Vec::new();

    for msg in messages {
        let role = msg.get("role").and_then(|v| v.as_str()).unwrap_or("");
        match role {
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
                    if id.is_empty() || name != "search_files" {
                        continue;
                    }
                    let pattern = tc
                        .get("function")
                        .and_then(|v| v.get("arguments"))
                        .and_then(|v| v.as_str())
                        .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
                        .and_then(|value| {
                            value
                                .get("pattern")
                                .and_then(|v| v.as_str())
                                .map(str::to_string)
                        })
                        .unwrap_or_default();
                    if !pattern.trim().is_empty() {
                        pending.insert(id.to_string(), pattern);
                    }
                }
            }
            "tool" => {
                let tool_call_id = msg
                    .get("tool_call_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim();
                let Some(query) = pending.remove(tool_call_id) else {
                    continue;
                };
                let content = msg.get("content").and_then(|v| v.as_str()).unwrap_or("");
                if !content.trim_start().starts_with("[search_files:") {
                    continue;
                }
                for line in content.lines().skip(1) {
                    let trimmed = line.trim();
                    if trimmed.is_empty() || trimmed.starts_with('[') {
                        continue;
                    }
                    let mut parts = trimmed.splitn(3, ':');
                    let Some(path) = parts.next() else {
                        continue;
                    };
                    let Some(_line_no) = parts.next() else {
                        continue;
                    };
                    let Some(snippet) = parts.next() else {
                        continue;
                    };
                    out.push(SuccessfulSearchHit {
                        query: query.clone(),
                        path: path.trim().to_string(),
                        snippet: snippet.trim().to_string(),
                    });
                }
            }
            _ => {}
        }
    }

    out
}

pub(crate) fn normalize_rust_ident_fragment(raw: &str) -> Option<String> {
    let ident: String = raw
        .trim()
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
        .collect();
    if ident.is_empty() {
        None
    } else {
        Some(ident)
    }
}

pub(crate) fn rust_module_file_candidates(seed_path: &str, body: &str) -> Vec<String> {
    let dir = seed_path.rsplit_once('/').map(|(dir, _)| dir).unwrap_or("");
    let mut out = Vec::new();
    for line in body.lines() {
        let trimmed = line.trim();
        let rest = trimmed
            .strip_prefix("mod ")
            .or_else(|| trimmed.strip_prefix("pub mod "));
        let Some(rest) = rest else {
            continue;
        };
        let Some(module) = rest.strip_suffix(';') else {
            continue;
        };
        let module = module.trim();
        if module.is_empty()
            || module.contains("::")
            || !module
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        {
            continue;
        }
        let candidate = if dir.is_empty() {
            format!("{module}.rs")
        } else {
            format!("{dir}/{module}.rs")
        };
        if !out.contains(&candidate) {
            out.push(candidate);
        }
    }
    out
}

#[derive(Debug, Default)]
pub(crate) struct ObservationHistory {
    total_successes: usize,
    last_successful_command: Option<String>,
    by_command: BTreeMap<String, usize>,
}

pub(crate) fn observation_history(messages: &[Value]) -> ObservationHistory {
    let mut pending_by_id: BTreeMap<String, String> = BTreeMap::new();
    let mut out = ObservationHistory::default();

    for msg in messages {
        let role = msg.get("role").and_then(|v| v.as_str()).unwrap_or("");
        match role {
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
                    let arguments = tc
                        .get("function")
                        .and_then(|v| v.get("arguments"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .trim();
                    if id.is_empty() || !is_observation_tool(name) {
                        continue;
                    }
                    if let Some(command) = canonicalize_tool_call_command(name, arguments) {
                        pending_by_id.insert(id.to_string(), command);
                    }
                }
            }
            "tool" => {
                let tcid = msg
                    .get("tool_call_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim();
                let content = msg
                    .get("content")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim();
                let Some(command) = pending_by_id.get(tcid) else {
                    continue;
                };
                if !looks_like_successful_observation_result(content) {
                    continue;
                }
                out.total_successes += 1;
                out.last_successful_command = Some(command.clone());
                *out.by_command.entry(command.clone()).or_insert(0) += 1;
            }
            _ => {}
        }
    }

    out
}

pub(crate) fn looks_like_successful_observation_result(content: &str) -> bool {
    let trimmed = content.trim_start();
    trimmed.starts_with('[') && !trimmed.starts_with("[RESULT")
}

pub(crate) fn is_observation_tool(name: &str) -> bool {
    matches!(name, "read_file" | "search_files" | "list_dir" | "glob")
}

pub(crate) fn recent_blocked_artifact_action(
    messages: &[Value],
    harness: TaskHarness,
) -> Option<ToolCallData> {
    let mut idx = messages.len();
    while idx >= 2 {
        let tool_msg = &messages[idx - 1];
        let assistant_msg = &messages[idx - 2];
        idx -= 2;

        if tool_msg.get("role").and_then(|v| v.as_str()) != Some("tool") {
            continue;
        }
        let content = tool_msg
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        if !looks_like_missing_gate_block(content) {
            break;
        }
        let Some(candidate) = first_tool_call_from_message(assistant_msg) else {
            continue;
        };
        if !supports_artifact_action(harness, candidate.name.as_str()) {
            continue;
        }
        if candidate.arguments.trim().is_empty() {
            continue;
        }
        return Some(candidate);
    }
    None
}

pub(crate) fn recent_blocked_fix_existing_mutation_action(
    messages: &[Value],
    root_user_text: &str,
) -> Option<ToolCallData> {
    let reads = successful_read_contents(messages);
    let mut idx = messages.len();
    while idx >= 2 {
        let tool_msg = &messages[idx - 1];
        let assistant_msg = &messages[idx - 2];
        idx -= 2;

        if tool_msg.get("role").and_then(|v| v.as_str()) != Some("tool") {
            continue;
        }
        let content = tool_msg
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        if !looks_like_missing_gate_block(content) && !content.contains("[Evidence Gate]") {
            break;
        }
        let Some(candidate) = first_tool_call_from_message(assistant_msg) else {
            continue;
        };
        if !matches!(candidate.name.as_str(), "patch_file" | "apply_diff") {
            continue;
        }
        if let Some(repaired) =
            validate_or_repair_blocked_fix_existing_mutation(&candidate, &reads, root_user_text)
        {
            return Some(repaired);
        }
    }
    None
}

pub(crate) fn unique_requested_source_member(root: &str, target: &str) -> Option<String> {
    let mut members = path_literals_in_text(root)
        .into_iter()
        .filter(|path| path.starts_with("src/") && path != target);
    let first = members.next()?;
    members.all(|member| member == first).then_some(first)
}

pub(crate) fn synthesize_fix_existing_literal_member_patch(
    messages: &[Value],
    root_user_text: &str,
) -> Option<ToolCallData> {
    let reads = successful_read_contents(messages);
    let target_path = match infer_fix_existing_focus(messages)? {
        FixExistingFocus::PatchImplementation(path) => path,
        FixExistingFocus::ReadImplementation(_) => return None,
    };
    if !target_path.starts_with("src/") {
        return None;
    }
    let body = reads.get(target_path.as_str())?;
    let missing_literal = unique_requested_source_member(root_user_text, &target_path)?;
    let (search, replace) = synthesize_list_member_insertion(body, missing_literal.as_str())?;
    Some(ToolCallData {
        thought_signature: None,
        id: "synthetic_fix_existing_patch".to_string(),
        name: "patch_file".to_string(),
        arguments: serde_json::json!({
            "path": target_path,
            "search": search,
            "replace": replace,
        })
        .to_string(),
    })
}

pub(crate) fn synthesize_failed_status_guard_patch(
    path: &str,
    body: &str,
    root_user_text: &str,
) -> Option<ToolCallData> {
    let prompt = root_user_text.to_ascii_lowercase();
    if !["failed", "rollback", "blocked", "approved"]
        .iter()
        .all(|needle| prompt.contains(needle))
    {
        return None;
    }
    if body.contains("case.status != \"passed\"") {
        return None;
    }
    if !body.contains("MergeGateBoardStatus::RollbackAvailable")
        || !body.contains("MergeGateBoardStatus::Blocked")
        || !body.contains("MergeGateBoardStatus::Approved")
    {
        return None;
    }
    let function_start = "fn status_for_case(case: &MergeGateCase<'_>) -> MergeGateBoardStatus {\n";
    let review_start = "    if let Some(review) = case.review {\n";
    let search = format!("{function_start}{review_start}");
    if !body.contains(search.as_str()) {
        return None;
    }
    let guard = concat!(
        "    if case.status != \"passed\" {\n",
        "        return if case.rollback_command.is_some() {\n",
        "            MergeGateBoardStatus::RollbackAvailable\n",
        "        } else {\n",
        "            MergeGateBoardStatus::Blocked\n",
        "        };\n",
        "    }\n\n",
    );
    let replace = format!("{function_start}{guard}{review_start}");
    Some(ToolCallData {
        thought_signature: None,
        id: "synthetic_fix_existing_guard_patch".to_string(),
        name: "patch_file".to_string(),
        arguments: serde_json::json!({
            "path": path,
            "search": search,
            "replace": replace,
        })
        .to_string(),
    })
}

pub(crate) fn validate_or_repair_blocked_fix_existing_mutation(
    tc: &ToolCallData,
    reads: &BTreeMap<String, String>,
    root_user_text: &str,
) -> Option<ToolCallData> {
    if tc.name == "apply_diff" {
        let parsed = serde_json::from_str::<Value>(tc.arguments.as_str()).ok()?;
        let path = parsed.get("path").and_then(|v| v.as_str())?.trim();
        let diff = parsed.get("diff").and_then(|v| v.as_str())?.trim();
        if path.is_empty()
            || diff.is_empty()
            || !reads.contains_key(normalize_observed_path(path).as_str())
        {
            return None;
        }
        return Some(tc.clone());
    }

    let parsed = serde_json::from_str::<Value>(tc.arguments.as_str()).ok();
    let path = parsed
        .as_ref()
        .and_then(|value| {
            value
                .get("path")
                .and_then(|v| v.as_str())
                .map(str::to_string)
        })
        .or_else(|| extract_loose_json_string_field(tc.arguments.as_str(), "path"))?;
    if !reads.contains_key(normalize_observed_path(path.as_str()).as_str()) {
        return None;
    }
    let search = parsed
        .as_ref()
        .and_then(|value| {
            value
                .get("search")
                .and_then(|v| v.as_str())
                .map(str::to_string)
        })
        .or_else(|| extract_loose_json_string_field(tc.arguments.as_str(), "search"));
    let replace = parsed
        .as_ref()
        .and_then(|value| {
            value
                .get("replace")
                .and_then(|v| v.as_str())
                .map(str::to_string)
        })
        .or_else(|| extract_loose_json_string_field(tc.arguments.as_str(), "replace"));

    if let (Some(search), Some(replace)) = (search.as_ref(), replace.as_ref()) {
        if !search.trim().is_empty() && !replace.trim().is_empty() {
            return Some(tc.clone());
        }
    }

    repair_blocked_patch_with_prompt_literal(
        tc,
        path.as_str(),
        search.as_deref().unwrap_or(""),
        reads.get(normalize_observed_path(path.as_str()).as_str())?,
        root_user_text,
    )
}

pub(crate) fn repair_blocked_patch_with_prompt_literal(
    tc: &ToolCallData,
    path: &str,
    search_prefix: &str,
    body: &str,
    root_user_text: &str,
) -> Option<ToolCallData> {
    let missing_literal = unique_requested_source_member(root_user_text, path)?;
    if search_prefix.trim().is_empty() {
        return None;
    }
    let start = body.find(search_prefix)?;
    let tail = &body[start..];
    let closing_rel = tail.find("\n];")?;
    let segment = &tail[..closing_rel + 3];
    let (search, replace) = synthesize_list_member_insertion(segment, &missing_literal)?;
    Some(ToolCallData {
        thought_signature: None,
        id: tc.id.clone(),
        name: "patch_file".to_string(),
        arguments: serde_json::json!({
            "path": path,
            "search": search,
            "replace": replace,
        })
        .to_string(),
    })
}

pub(crate) fn synthesize_list_member_insertion(
    body: &str,
    missing_literal: &str,
) -> Option<(String, String)> {
    let quoted_literal = format!("\"{missing_literal}\",");
    if body.lines().any(|line| line.trim() == quoted_literal) {
        return None;
    }
    // A different quoted string elsewhere in a file is not a path-list anchor.
    // Require the observed adjacent member and closing line to be unique.
    let mut pairs = body
        .lines()
        .zip(body.lines().skip(1))
        .filter(|(member, closing)| {
            closing.trim() == "];"
                && member
                    .trim()
                    .strip_prefix('"')
                    .and_then(|s| s.strip_suffix("\","))
                    .is_some_and(|s| s.starts_with("src/") && !s.contains('"'))
        });
    let (last_item_line, closing_line) = pairs.next()?;
    if pairs.next().is_some() {
        return None;
    }
    let indent: String = last_item_line
        .chars()
        .take_while(|ch| ch.is_whitespace())
        .collect();
    let search = format!("{last_item_line}\n{closing_line}");
    let replace = format!("{last_item_line}\n{indent}\"{missing_literal}\",\n{closing_line}");
    Some((search, replace))
}

pub(crate) fn first_tool_call_from_message(msg: &Value) -> Option<ToolCallData> {
    let tool_calls = msg.get("tool_calls")?.as_array()?;
    let tc = tool_calls.first()?;
    let id = tc.get("id").and_then(|v| v.as_str())?.trim();
    let function = tc.get("function")?;
    let name = function.get("name").and_then(|v| v.as_str())?.trim();
    let arguments = function.get("arguments").and_then(|v| v.as_str())?.trim();
    if id.is_empty() || name.is_empty() {
        return None;
    }
    Some(ToolCallData {
        thought_signature: None,
        id: id.to_string(),
        name: name.to_string(),
        arguments: arguments.to_string(),
    })
}

pub(crate) fn looks_like_missing_gate_block(content: &str) -> bool {
    content.contains("GOVERNOR BLOCKED")
        && (content.contains("Missing valid <plan>") || content.contains("Missing <think>"))
}

pub(crate) fn extract_loose_json_string_field(raw: &str, field: &str) -> Option<String> {
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

    Some(out)
}

pub(crate) fn path_literals_in_text(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for token in text.split_whitespace() {
        let trimmed = token
            .trim_matches(|c: char| {
                (c.is_ascii_punctuation() && !matches!(c, '.' | '/' | '\\' | '_' | '-'))
                    || matches!(
                        c,
                        '「' | '」'
                            | '『'
                            | '』'
                            | '（'
                            | '）'
                            | '('
                            | ')'
                            | '['
                            | ']'
                            | '{'
                            | '}'
                            | '`'
                            | '"'
                            | '\''
                    )
            })
            .trim_end_matches(|c: char| matches!(c, '.' | ',' | ';' | ':' | '!' | '?'))
            .trim_matches(|c: char| {
                matches!(
                    c,
                    '「' | '」'
                        | '『'
                        | '』'
                        | '（'
                        | '）'
                        | '('
                        | ')'
                        | '['
                        | ']'
                        | '{'
                        | '}'
                        | '`'
                        | '"'
                        | '\''
                )
            });
        let has_path_sep = trimmed.contains('/') || trimmed.contains('\\');
        let has_extension = trimmed
            .split('/')
            .next_back()
            .is_some_and(|segment| segment.contains('.'));
        if (has_path_sep || has_extension) && !trimmed.is_empty() {
            let literal = trimmed.replace('\\', "/");
            if !out.contains(&literal) {
                out.push(literal);
            }
        }
    }
    out
}
