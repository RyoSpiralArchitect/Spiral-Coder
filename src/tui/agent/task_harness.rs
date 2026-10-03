use super::failure_localization::infer_fix_existing_symbol;
use super::repo_scaffold::{
    default_repo_gitignore, repo_root_from_test_cmd, required_repo_files_from_test_cmd,
    resolve_repo_file_path, resolve_repo_scaffold_path, scaffold_repo_file_content,
};
use super::{
    canonicalize_tool_call_command, classify_verify_level, compact_one_line,
    non_exec_tool_succeeded, parse_exec_command_from_args, RecoveryStage,
};
use crate::streaming::ToolCallData;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[path = "benchmark_proof.rs"]
mod benchmark_proof;
#[cfg(test)]
#[path = "benchmark_recovery_tests.rs"]
mod benchmark_recovery_tests;
#[path = "benchmark_replay.rs"]
mod benchmark_replay;
#[path = "progress_gate.rs"]
mod progress_gate;
pub(super) use progress_gate::build_progress_gate_block;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TaskLane {
    ReadOnlyObserve,
    BenchmarkPlan,
    FixExisting,
    EditExisting,
    CreateFile,
    InitRepo,
    ScaffoldRepo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ArtifactMode {
    ObserveOnly,
    ExistingFiles,
    NewFiles,
    NewRepo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct TaskHarness {
    pub lane: TaskLane,
    pub artifact_mode: ArtifactMode,
}

impl TaskHarness {
    pub(super) fn infer(root_user_text: &str, root_read_only: bool) -> Self {
        if root_read_only {
            return Self {
                lane: TaskLane::ReadOnlyObserve,
                artifact_mode: ArtifactMode::ObserveOnly,
            };
        }

        let low = root_user_text.to_ascii_lowercase();
        if is_approved_observer_benchmark_plan(low.as_str()) {
            return Self {
                lane: TaskLane::BenchmarkPlan,
                artifact_mode: ArtifactMode::ExistingFiles,
            };
        }

        let mentions_repo = text_contains_any(
            low.as_str(),
            &[
                "repo",
                "repository",
                "git repo",
                "git repository",
                "project",
                "workspace",
                "service",
                "app",
                "リポ",
                "リポジトリ",
                "新規プロジェクト",
                "プロジェクト",
            ],
        );

        if text_contains_any(
            low.as_str(),
            &[
                "failing test",
                "fix the failing",
                "smallest code change",
                "bug",
                "regression",
                "broken",
                "repair",
                "debug",
                "panic",
                "fix ",
                "修正",
                "直す",
                "バグ",
                "壊れ",
                "失敗テスト",
                "デバッグ",
            ],
        ) {
            return Self {
                lane: TaskLane::FixExisting,
                artifact_mode: ArtifactMode::ExistingFiles,
            };
        }

        if mentions_repo
            && text_contains_any(
                low.as_str(),
                &[
                    "git init",
                    "initialize",
                    "initialise",
                    "initialize a new repo",
                    "initialize git",
                    "init a new repo",
                    "init repo",
                    "初期化",
                    "git リポジトリ",
                ],
            )
        {
            return Self {
                lane: TaskLane::InitRepo,
                artifact_mode: ArtifactMode::NewRepo,
            };
        }

        if mentions_repo
            && text_contains_any(
                low.as_str(),
                &["create", "new", "make", "add", "作成", "作る", "新規"],
            )
        {
            return Self {
                lane: TaskLane::ScaffoldRepo,
                artifact_mode: ArtifactMode::NewRepo,
            };
        }

        if mentions_repo
            && text_contains_any(
                low.as_str(),
                &[
                    "scaffold",
                    "bootstrap",
                    "starter",
                    "boilerplate",
                    "template",
                    "skeleton",
                    "雛形",
                    "テンプレート",
                    "ひな形",
                    "スキャフォールド",
                ],
            )
        {
            return Self {
                lane: TaskLane::ScaffoldRepo,
                artifact_mode: ArtifactMode::NewRepo,
            };
        }

        if text_contains_any(
            low.as_str(),
            &[
                "create file",
                "new file",
                "write file",
                "add file",
                "create a file",
                "write a file",
                "create ",
                "write ",
                "ファイルを作成",
                "新規ファイル",
                "ファイルを追加",
                "作成",
                "作る",
            ],
        ) && (text_contains_any(low.as_str(), &["file", "ファイル"])
            || prompt_mentions_path_literal(root_user_text))
        {
            return Self {
                lane: TaskLane::CreateFile,
                artifact_mode: ArtifactMode::NewFiles,
            };
        }

        Self {
            lane: TaskLane::EditExisting,
            artifact_mode: ArtifactMode::ExistingFiles,
        }
    }

    pub(super) fn lane_label(self) -> &'static str {
        match self.lane {
            TaskLane::ReadOnlyObserve => "read_only_observe",
            TaskLane::BenchmarkPlan => "benchmark_plan",
            TaskLane::FixExisting => "fix_existing_files",
            TaskLane::EditExisting => "edit_existing_files",
            TaskLane::CreateFile => "create_file",
            TaskLane::InitRepo => "init_repo",
            TaskLane::ScaffoldRepo => "scaffold_repo",
        }
    }

    pub(super) fn artifact_mode_label(self) -> &'static str {
        match self.artifact_mode {
            ArtifactMode::ObserveOnly => "observe_only",
            ArtifactMode::ExistingFiles => "existing_files",
            ArtifactMode::NewFiles => "new_files",
            ArtifactMode::NewRepo => "new_repo",
        }
    }

    pub(super) fn allows_repo_goal_check(self) -> bool {
        self.artifact_mode == ArtifactMode::NewRepo
    }

    fn progress_shape_hint(self) -> &'static str {
        match self.lane {
            TaskLane::ReadOnlyObserve => {
                "Observe -> confirm -> done. Do not mutate or create files."
            }
            TaskLane::BenchmarkPlan => {
                "Read the approved Observer benchmark plan, update the smallest eval/replay spec or fixture, then verify it."
            }
            TaskLane::FixExisting => {
                "Inspect just enough to confirm the bug, then mutate, then verify."
            }
            TaskLane::EditExisting => {
                "Inspect just enough to confirm the target, then edit, then verify."
            }
            TaskLane::CreateFile => {
                "If the destination is clear, create the file early instead of over-inspecting."
            }
            TaskLane::InitRepo | TaskLane::ScaffoldRepo => {
                "Create the repo/project artifact early, then verify the created structure."
            }
        }
    }

    fn deliverable_hint(self) -> &'static str {
        if self.lane == TaskLane::BenchmarkPlan {
            return "Completion must leave a deterministic regression artifact: update .spiral-coder/runtime_eval.json, .spiral-coder/tui_replay.json, or a targeted test/fixture.";
        }
        match self.artifact_mode {
            ArtifactMode::ObserveOnly => "Leave no file mutations behind.",
            ArtifactMode::ExistingFiles => {
                "Completion must leave a concrete change in an existing project file."
            }
            ArtifactMode::NewFiles => "Completion must leave the requested new file(s) on disk.",
            ArtifactMode::NewRepo => {
                "Completion must leave the requested repo/project directory on disk."
            }
        }
    }

    pub(super) fn prompt(self, test_cmd: Option<&str>) -> String {
        let mut out = format!(
            "[Task Harness]\n\
lane: {}\n\
artifact_mode: {}\n\
progress_shape: {}\n\
deliverable: {}\n",
            self.lane_label(),
            self.artifact_mode_label(),
            self.progress_shape_hint(),
            self.deliverable_hint()
        );
        match self.artifact_mode {
            ArtifactMode::ObserveOnly => {
                out.push_str(
                    "When the target path/context is confirmed, stop probing and call `done`.\n",
                );
            }
            ArtifactMode::ExistingFiles => {
                out.push_str(
                    "Do not stay in repeated read/search loops once the current target file is confirmed.\n\
Move to `patch_file`/`apply_diff`, or run verification if you believe the fix is already present.\n",
                );
                if self.lane == TaskLane::BenchmarkPlan {
                    out.push_str(
                        "Treat `<observer_benchmark_plan>` as approved external Observer input.\n\
If lane is `runtime_eval`, update `.spiral-coder/runtime_eval.json` and any smallest required fixture.\n\
If lane is `tui_replay`, update `.spiral-coder/tui_replay.json`.\n\
If lane is `observer_unit` or `unit`, add the smallest targeted regression test instead.\n\
Prefer the `required_checks` listed in the approved plan.\n\
Do not only explain the benchmark plan; create or edit the regression artifact.\n",
                    );
                }
            }
            ArtifactMode::NewFiles => {
                out.push_str(
                    "Do not treat this as read-only exploration. Create the requested file when the destination is clear.\n",
                );
            }
            ArtifactMode::NewRepo => {
                out.push_str(
                    "Do not treat this as read-only exploration. Create the repo/project directory and starter artifacts when the destination is clear.\n",
                );
            }
        }
        if let Some(cmd) = test_cmd.filter(|cmd| !cmd.trim().is_empty()) {
            out.push_str("Configured verification command:\n- ");
            out.push_str(&compact_one_line(cmd, 180));
            out.push('\n');
        }
        out.push_str("If your next tool call would only repeat already-successful observation on the same target, change phase instead.");
        out
    }

    pub(super) fn compact_prompt(self, test_cmd: Option<&str>) -> String {
        let verify = test_cmd
            .filter(|cmd| !cmd.trim().is_empty())
            .map(|cmd| compact_one_line(cmd, 80))
            .unwrap_or_else(|| "none".to_string());
        [
            "[Task Harness cache]".to_string(),
            format!("- lane: {}", self.lane_label()),
            format!("- artifact_mode: {}", self.artifact_mode_label()),
            format!(
                "- progress_shape: {}",
                compact_one_line(self.progress_shape_hint(), 100)
            ),
            format!("- verify: {verify}"),
        ]
        .join("\n")
    }
}

fn is_approved_observer_benchmark_plan(low: &str) -> bool {
    low.contains("[observer benchmark plan approved]") || low.contains("<observer_benchmark_plan")
}

pub(super) fn approved_benchmark_required_commands(root_user_text: &str) -> Vec<String> {
    if is_approved_observer_benchmark_plan(&root_user_text.to_ascii_lowercase()) {
        benchmark_proof::required_commands(root_user_text)
    } else {
        Vec::new()
    }
}

fn is_src_read_file_tool_call(tc: &ToolCallData) -> bool {
    if tc.name != "read_file" {
        return false;
    }
    serde_json::from_str::<Value>(tc.arguments.as_str())
        .ok()
        .and_then(|value| {
            value
                .get("path")
                .and_then(|v| v.as_str())
                .map(str::to_string)
        })
        .is_some_and(|path| path.starts_with("src/") && path.ends_with(".rs"))
}

pub(super) fn coerce_artifact_creation_tool_call(
    harness: TaskHarness,
    messages: &[Value],
    tc: &ToolCallData,
) -> Option<(ToolCallData, String, String)> {
    if !matches!(
        harness.artifact_mode,
        ArtifactMode::NewFiles | ArtifactMode::NewRepo
    ) {
        return None;
    }
    if !is_observation_tool(tc.name.as_str()) {
        return None;
    }

    let rewritten = recent_blocked_artifact_action(messages, harness)?;
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

pub(super) fn coerce_fix_existing_tool_call(
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
    let wants_verify_exec = tc.name == "exec"
        && parse_exec_command_from_args(tc.arguments.as_str())
            .and_then(|command| classify_verify_level(command.as_str(), test_cmd))
            .is_some();
    if !wants_verify_exec && !is_observation_tool(tc.name.as_str()) {
        return None;
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

pub(super) fn coerce_benchmark_plan_tool_call(
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
            id: tc.id.clone(),
            name: "exec".to_string(),
            arguments: serde_json::json!({ "command": command }).to_string(),
        };
        return compare_rewritten_tool_call(tc, rewritten);
    }

    None
}

pub(super) fn synthesize_benchmark_plan_no_tool_call(
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

pub(super) fn allows_benchmark_plan_followup_during_verify(
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

pub(super) fn benchmark_plan_pending_required_exec_command(
    root_user_text: &str,
    messages: &[Value],
    test_cmd: Option<&str>,
) -> Option<String> {
    root_user_text
        .contains("<observer_benchmark_plan")
        .then(|| pending_benchmark_plan_required_check_exec(messages, root_user_text, test_cmd))
        .flatten()
}

pub(super) fn benchmark_plan_missing_required_exec_proof(
    root_user_text: &str,
    messages: &[Value],
    test_cmd: Option<&str>,
) -> bool {
    benchmark_plan_pending_required_exec_command(root_user_text, messages, test_cmd).is_some()
}

pub(super) fn benchmark_plan_protected_call_ids(messages: &[Value]) -> BTreeSet<String> {
    benchmark_proof::protected_call_ids(messages)
}

fn compare_rewritten_tool_call(
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

fn is_benchmark_plan_candidate_tool(tc: &ToolCallData) -> bool {
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

fn benchmark_plan_spec_path(root_user_text: &str) -> Option<String> {
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

fn benchmark_plan_target_paths(root_user_text: &str) -> Vec<String> {
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

fn is_benchmark_plan_target_path(path: &str) -> bool {
    path.starts_with("docs/") && path.ends_with(".md")
        || path == ".spiral-coder/runtime_eval.json"
        || path == ".spiral-coder/tui_replay.json"
        || path.ends_with("/.spiral-coder/runtime_eval.json")
        || path.ends_with("/.spiral-coder/tui_replay.json")
}

fn push_unique_path(out: &mut Vec<String>, path: String) {
    if !out.contains(&path) {
        out.push(path);
    }
}

fn pending_benchmark_plan_required_check_exec(
    messages: &[Value],
    root_user_text: &str,
    test_cmd: Option<&str>,
) -> Option<String> {
    benchmark_proof::pending_command(messages, root_user_text, test_cmd)
}

fn benchmark_plan_targets_satisfied(
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

fn benchmark_plan_tool_call_is_required_check_exec(tc: &ToolCallData, command: &str) -> bool {
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

fn tool_call_path(tc: &ToolCallData) -> Option<String> {
    serde_json::from_str::<Value>(tc.arguments.as_str())
        .ok()
        .and_then(|value| {
            value
                .get("path")
                .and_then(|v| v.as_str())
                .map(str::to_string)
        })
}

fn read_tool_root_file(tool_root: Option<&str>, path: &str) -> Option<String> {
    let root = tool_root?.trim();
    if root.is_empty() {
        return None;
    }
    std::fs::read_to_string(std::path::Path::new(root).join(path)).ok()
}

fn benchmark_plan_tool_call_is_complete_target_mutation(tc: &ToolCallData, target: &str) -> bool {
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

fn benchmark_plan_target_is_satisfied(_target: &str, body: &str, root_user_text: &str) -> bool {
    benchmark_plan_required_path_literal(root_user_text)
        .is_some_and(|required_path| body.contains(required_path.as_str()))
}

fn synthesize_benchmark_plan_target_patch(
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

fn synthesize_benchmark_plan_json_body(
    target: &str,
    body: &str,
    required_path: &str,
    root_user_text: &str,
) -> Option<String> {
    let mut value = serde_json::from_str::<Value>(body).ok()?;
    if target.ends_with("/tui_replay.json") {
        // Use the replay contract, not runtime-eval check names or a literal-only update.
        let mut spec = serde_json::from_value::<crate::tui_replay::TuiReplaySpec>(value).ok()?;
        spec.cases.push(benchmark_replay::case(
            benchmark_plan_case_id_hint(root_user_text)
                .unwrap_or_else(|| "observer-benchmark-plan-regression".to_string()),
            required_path,
        ));
        value = serde_json::to_value(spec).ok()?;
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

fn synthesize_benchmark_plan_markdown_body(body: &str, required_path: &str) -> String {
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

fn benchmark_plan_required_path_literal(root_user_text: &str) -> Option<String> {
    path_literals_in_text(root_user_text)
        .into_iter()
        .find(|path| path.starts_with("src/") && path.ends_with(".rs"))
}

fn benchmark_plan_case_id_hint(root_user_text: &str) -> Option<String> {
    root_user_text.lines().find_map(|line| {
        line.trim()
            .strip_prefix("case_id_hint:")
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    })
}

pub(super) fn coerce_fix_existing_blocked_mutation_tool_call(
    harness: TaskHarness,
    messages: &[Value],
    tc: &ToolCallData,
    root_user_text: &str,
) -> Option<(ToolCallData, String, String)> {
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

pub(super) fn repair_fix_existing_mutation_tool_call(
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

pub(super) fn coerce_fix_existing_literal_mutation_tool_call(
    harness: TaskHarness,
    messages: &[Value],
    tc: &ToolCallData,
    root_user_text: &str,
) -> Option<(ToolCallData, String, String)> {
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

pub(super) fn synthesize_fix_existing_no_tool_mutation_tool_call(
    harness: TaskHarness,
    messages: &[Value],
    root_user_text: &str,
) -> Option<ToolCallData> {
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

pub(super) fn coerce_repo_goal_completion_tool_call(
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

pub(super) fn repair_repo_scaffold_write_tool_call(
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

pub(super) fn allows_artifact_creation_during_diagnose(
    harness: TaskHarness,
    tc: &ToolCallData,
) -> bool {
    matches!(
        harness.artifact_mode,
        ArtifactMode::NewFiles | ArtifactMode::NewRepo
    ) && matches!(tc.name.as_str(), "write_file" | "exec")
}

pub(super) fn allows_artifact_creation_during_verify(
    harness: TaskHarness,
    tc: &ToolCallData,
) -> bool {
    harness.artifact_mode == ArtifactMode::NewRepo
        && matches!(tc.name.as_str(), "write_file" | "exec")
}

pub(super) fn build_fix_stage_progress_hint(
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

pub(super) fn fix_existing_target_hint(messages: &[Value]) -> Option<String> {
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
enum FixExistingFocus {
    ReadImplementation(String),
    PatchImplementation(String),
}

fn infer_fix_existing_focus(messages: &[Value]) -> Option<FixExistingFocus> {
    let reads = successful_read_contents(messages);
    let target = search_derived_fix_existing_path(messages)
        .or_else(|| {
            ["src/lib.rs", "src/main.rs", "lib.rs", "main.rs"]
                .iter()
                .find_map(|entry| {
                    let body = reads.get(*entry)?;
                    rust_module_file_candidates(entry, body)
                        .into_iter()
                        .find(|candidate| !candidate.trim().is_empty())
                })
        })
        .or_else(|| latest_successful_source_read_path(messages, &reads))?;

    if reads.contains_key(target.as_str()) {
        Some(FixExistingFocus::PatchImplementation(target))
    } else {
        Some(FixExistingFocus::ReadImplementation(target))
    }
}

fn search_derived_fix_existing_path(messages: &[Value]) -> Option<String> {
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

fn fallback_fix_existing_anchor(messages: &[Value], tool_root: Option<&str>) -> Option<String> {
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

fn concrete_impl_from_recent_subdir_listing(
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

fn direct_impl_from_attempted_subdir_listing(
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

fn explicit_existing_rust_path_from_tool_call(
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

fn concrete_impl_under_dir(root: &str, dir: &str) -> Option<String> {
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

fn latest_successful_list_dir_path(messages: &[Value]) -> Option<String> {
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

fn latest_successful_source_read_path(
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

fn resolve_existing_rust_path(path: &str, tool_root: Option<&str>) -> String {
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

fn successful_read_contents(messages: &[Value]) -> BTreeMap<String, String> {
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
                        })
                        .unwrap_or_default();
                    if !path.trim().is_empty() {
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
struct SuccessfulSearchHit {
    query: String,
    path: String,
    snippet: String,
}

fn successful_search_hits(messages: &[Value]) -> Vec<SuccessfulSearchHit> {
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

fn normalize_rust_ident_fragment(raw: &str) -> Option<String> {
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

fn rust_module_file_candidates(seed_path: &str, body: &str) -> Vec<String> {
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
struct ObservationHistory {
    total_successes: usize,
    last_successful_command: Option<String>,
    by_command: BTreeMap<String, usize>,
}

fn observation_history(messages: &[Value]) -> ObservationHistory {
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

fn looks_like_successful_observation_result(content: &str) -> bool {
    let trimmed = content.trim_start();
    trimmed.starts_with('[') && !trimmed.starts_with("[RESULT")
}

fn is_observation_tool(name: &str) -> bool {
    matches!(name, "read_file" | "search_files" | "list_dir" | "glob")
}

fn recent_blocked_artifact_action(
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

fn recent_blocked_fix_existing_mutation_action(
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

fn synthesize_fix_existing_literal_member_patch(
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
    let missing_literal = path_literals_in_text(root_user_text)
        .into_iter()
        .find(|literal| literal.starts_with("src/") && literal != &target_path)?;
    let (search, replace) = synthesize_list_member_insertion(body, missing_literal.as_str())?;
    Some(ToolCallData {
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

fn synthesize_failed_status_guard_patch(
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

fn validate_or_repair_blocked_fix_existing_mutation(
    tc: &ToolCallData,
    reads: &BTreeMap<String, String>,
    root_user_text: &str,
) -> Option<ToolCallData> {
    if tc.name == "apply_diff" {
        let parsed = serde_json::from_str::<Value>(tc.arguments.as_str()).ok()?;
        let path = parsed.get("path").and_then(|v| v.as_str())?.trim();
        let diff = parsed.get("diff").and_then(|v| v.as_str())?.trim();
        if path.is_empty() || diff.is_empty() || !reads.contains_key(path) {
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
    if !reads.contains_key(path.as_str()) {
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
        reads.get(path.as_str())?,
        root_user_text,
    )
}

fn repair_blocked_patch_with_prompt_literal(
    tc: &ToolCallData,
    path: &str,
    search_prefix: &str,
    body: &str,
    root_user_text: &str,
) -> Option<ToolCallData> {
    let missing_literal = path_literals_in_text(root_user_text)
        .into_iter()
        .find(|literal| literal.starts_with("src/") && literal != path)?;
    if search_prefix.trim().is_empty() {
        return None;
    }
    let start = body.find(search_prefix)?;
    let tail = &body[start..];
    let closing_rel = tail.find("\n];")?;
    let segment = &tail[..closing_rel + 3];
    let mut last_item_line = None;
    let mut closing_line = None;
    for line in segment.lines() {
        let trimmed = line.trim();
        if trimmed == "];" {
            closing_line = Some(line.to_string());
            break;
        }
        if trimmed.starts_with('"') && trimmed.ends_with(',') {
            last_item_line = Some(line.to_string());
        }
    }
    let last_item_line = last_item_line?;
    let closing_line = closing_line?;
    let indent: String = last_item_line
        .chars()
        .take_while(|ch| ch.is_whitespace())
        .collect();
    let search = format!("{last_item_line}\n{closing_line}");
    let replace = format!("{last_item_line}\n{indent}\"{missing_literal}\",\n{closing_line}");
    Some(ToolCallData {
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

fn synthesize_list_member_insertion(body: &str, missing_literal: &str) -> Option<(String, String)> {
    let quoted_literal = format!("\"{missing_literal}\",");
    if body.lines().any(|line| line.trim() == quoted_literal) {
        return None;
    }
    let mut last_item_line = None;
    let mut closing_line = None;
    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed == "];" {
            closing_line = Some(line.to_string());
            break;
        }
        if trimmed.starts_with('"') && trimmed.ends_with(',') {
            last_item_line = Some(line.to_string());
        }
    }
    let last_item_line = last_item_line?;
    let closing_line = closing_line?;
    let indent: String = last_item_line
        .chars()
        .take_while(|ch| ch.is_whitespace())
        .collect();
    let search = format!("{last_item_line}\n{closing_line}");
    let replace = format!("{last_item_line}\n{indent}\"{missing_literal}\",\n{closing_line}");
    Some((search, replace))
}

fn first_tool_call_from_message(msg: &Value) -> Option<ToolCallData> {
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
        id: id.to_string(),
        name: name.to_string(),
        arguments: arguments.to_string(),
    })
}

fn looks_like_missing_gate_block(content: &str) -> bool {
    content.contains("GOVERNOR BLOCKED")
        && (content.contains("Missing valid <plan>") || content.contains("Missing <think>"))
}

fn extract_loose_json_string_field(raw: &str, field: &str) -> Option<String> {
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

fn path_literals_in_text(text: &str) -> Vec<String> {
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

fn supports_artifact_action(harness: TaskHarness, name: &str) -> bool {
    match harness.artifact_mode {
        ArtifactMode::NewFiles => matches!(name, "write_file" | "exec"),
        ArtifactMode::NewRepo => matches!(name, "write_file" | "exec"),
        ArtifactMode::ObserveOnly | ArtifactMode::ExistingFiles => false,
    }
}

fn latest_goal_check_missing(messages: &[Value]) -> Vec<String> {
    for msg in messages.iter().rev() {
        if msg.get("role").and_then(|v| v.as_str()) != Some("user") {
            continue;
        }
        let content = msg.get("content").and_then(|v| v.as_str()).unwrap_or("");
        if !content.starts_with("[goal_check]") {
            continue;
        }
        for line in content.lines() {
            let trimmed = line.trim();
            let Some(rest) = trimmed.strip_prefix("Missing:") else {
                continue;
            };
            return rest
                .split(',')
                .map(str::trim)
                .filter(|entry| !entry.is_empty())
                .map(ToString::to_string)
                .collect();
        }
    }
    Vec::new()
}

#[derive(Debug, Clone, Default)]
struct RepoScaffoldStatus {
    repo_root: String,
    has_git: bool,
    has_gitignore: bool,
    has_readme: bool,
    required_files: Vec<String>,
    present_files: BTreeSet<String>,
}

fn repo_scaffold_status(
    messages: &[Value],
    test_cmd: &str,
    tool_root: Option<&str>,
) -> Option<RepoScaffoldStatus> {
    let repo_root = repo_root_from_test_cmd(test_cmd)?;
    let required_files = required_repo_files_from_test_cmd(test_cmd, repo_root.as_str());
    let mut pending_by_id: BTreeMap<String, (String, String)> = BTreeMap::new();
    let mut status = RepoScaffoldStatus {
        repo_root: repo_root.clone(),
        required_files,
        ..RepoScaffoldStatus::default()
    };

    for msg in messages {
        match msg.get("role").and_then(|v| v.as_str()).unwrap_or("") {
            "assistant" => {
                let Some(tool_calls) = msg.get("tool_calls").and_then(|v| v.as_array()) else {
                    continue;
                };
                for tc in tool_calls {
                    let id = tc.get("id").and_then(|v| v.as_str()).unwrap_or("").trim();
                    if id.is_empty() {
                        continue;
                    }
                    let Some(function) = tc.get("function") else {
                        continue;
                    };
                    let name = function
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .trim();
                    let arguments = function
                        .get("arguments")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .trim();
                    match name {
                        "write_file" => {
                            let path =
                                serde_json::from_str::<Value>(arguments)
                                    .ok()
                                    .and_then(|value| {
                                        value
                                            .get("path")
                                            .and_then(|v| v.as_str())
                                            .map(str::to_string)
                                    });
                            if let Some(path) = path {
                                pending_by_id.insert(id.to_string(), (name.to_string(), path));
                            }
                        }
                        "exec" => {
                            let command =
                                serde_json::from_str::<Value>(arguments)
                                    .ok()
                                    .and_then(|value| {
                                        value
                                            .get("command")
                                            .and_then(|v| v.as_str())
                                            .map(str::to_string)
                                    });
                            if let Some(command) = command {
                                pending_by_id.insert(id.to_string(), (name.to_string(), command));
                            }
                        }
                        _ => {}
                    }
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
                let Some((kind, payload)) = pending_by_id.get(id) else {
                    continue;
                };
                match kind.as_str() {
                    "write_file" if content.trim_start().starts_with("OK: wrote '") => {
                        if payload.starts_with(&format!("{repo_root}/")) {
                            status.present_files.insert(payload.clone());
                        }
                        if payload.ends_with("/README.md") {
                            status.has_readme = true;
                        }
                        if payload.ends_with("/.gitignore") {
                            status.has_gitignore = true;
                        }
                    }
                    "exec" if content.trim_start().starts_with("OK (exit_code: 0)") => {
                        if payload.trim() == format!("git init {}", repo_root) {
                            status.has_git = true;
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    if let Some(root) = tool_root {
        let repo_path = resolve_repo_scaffold_path(root, repo_root.as_str());
        status.has_git |= repo_path.join(".git").exists();
        status.has_gitignore |= repo_path.join(".gitignore").exists();
        status.has_readme |= repo_path.join("README.md").exists();
        for required_path in status.required_files.clone() {
            if resolve_repo_file_path(root, required_path.as_str()).exists() {
                if required_path.ends_with("/README.md") {
                    status.has_readme = true;
                }
                if required_path.ends_with("/.gitignore") {
                    status.has_gitignore = true;
                }
                status.present_files.insert(required_path);
            }
        }
    }

    Some(status)
}

fn write_file_looks_malformed(tc: &ToolCallData) -> bool {
    let parsed = serde_json::from_str::<Value>(tc.arguments.as_str()).ok();
    let path = parsed
        .as_ref()
        .and_then(|value| value.get("path").and_then(|v| v.as_str()))
        .map(str::trim)
        .unwrap_or("");
    let content = parsed
        .as_ref()
        .and_then(|value| value.get("content").and_then(|v| v.as_str()))
        .map(str::trim)
        .unwrap_or("");
    parsed.is_none()
        || path.is_empty()
        || content.is_empty()
        || path == ".git"
        || path.ends_with("/.git")
}

fn has_repo_scaffold_progress(status: &RepoScaffoldStatus) -> bool {
    status.has_git || status.has_gitignore || status.has_readme || !status.present_files.is_empty()
}

fn next_repo_scaffold_tool_call(
    tool_call_id: &str,
    status: &RepoScaffoldStatus,
) -> Option<ToolCallData> {
    if !status.has_git {
        return Some(ToolCallData {
            id: tool_call_id.to_string(),
            name: "exec".to_string(),
            arguments: serde_json::json!({
                "command": format!("git init {}", status.repo_root)
            })
            .to_string(),
        });
    }
    if let Some(target) = preferred_missing_repo_required_file(status) {
        let content = if target.ends_with("/.gitignore") {
            default_repo_gitignore().to_string()
        } else {
            scaffold_repo_file_content(target)
        };
        return Some(ToolCallData {
            id: tool_call_id.to_string(),
            name: "write_file".to_string(),
            arguments: serde_json::json!({
                "path": target,
                "content": content,
            })
            .to_string(),
        });
    }
    if !status.has_gitignore {
        return Some(ToolCallData {
            id: tool_call_id.to_string(),
            name: "write_file".to_string(),
            arguments: serde_json::json!({
                "path": format!("{}/.gitignore", status.repo_root),
                "content": default_repo_gitignore(),
            })
            .to_string(),
        });
    }
    if !status.has_readme {
        let readme_path = format!("{}/README.md", status.repo_root);
        let readme_content = scaffold_repo_file_content(readme_path.as_str());
        return Some(ToolCallData {
            id: tool_call_id.to_string(),
            name: "write_file".to_string(),
            arguments: serde_json::json!({
                "path": readme_path,
                "content": readme_content,
            })
            .to_string(),
        });
    }
    None
}

fn preferred_missing_repo_required_file(status: &RepoScaffoldStatus) -> Option<&str> {
    if let Some(gitignore) = status
        .required_files
        .iter()
        .find(|path| path.ends_with("/.gitignore") && !status.present_files.contains(*path))
    {
        return Some(gitignore.as_str());
    }

    status
        .required_files
        .iter()
        .find(|path| !status.present_files.contains(*path))
        .map(String::as_str)
}

fn text_contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

fn prompt_mentions_path_literal(text: &str) -> bool {
    text.split_whitespace().any(|token| {
        let trimmed = token.trim_matches(|c: char| {
            c.is_ascii_punctuation()
                || matches!(
                    c,
                    '「' | '」' | '『' | '』' | '（' | '）' | '`' | '"' | '\''
                )
        });
        let has_path_sep = trimmed.contains('/') || trimmed.contains('\\');
        let has_extension = trimmed
            .split('/')
            .next_back()
            .is_some_and(|segment| segment.contains('.'));
        has_path_sep || has_extension
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::tempdir;

    #[test]
    fn infer_task_harness_detects_fix_lane() {
        let harness =
            TaskHarness::infer("Fix the failing test with the smallest code change.", false);
        assert_eq!(harness.lane, TaskLane::FixExisting);
        assert_eq!(harness.artifact_mode, ArtifactMode::ExistingFiles);
    }

    #[test]
    fn infer_task_harness_detects_new_file_lane() {
        let harness = TaskHarness::infer(
            "Create `notes/todo.txt` containing exactly `ship it`.",
            false,
        );
        assert_eq!(harness.lane, TaskLane::CreateFile);
        assert_eq!(harness.artifact_mode, ArtifactMode::NewFiles);
    }

    #[test]
    fn infer_task_harness_detects_init_repo_lane() {
        let harness = TaskHarness::infer(
            "Create a new git repo in `demo_repo/` and initialize it with README.md.",
            false,
        );
        assert_eq!(harness.lane, TaskLane::InitRepo);
        assert_eq!(harness.artifact_mode, ArtifactMode::NewRepo);
    }

    #[test]
    fn infer_task_harness_detects_approved_benchmark_plan_lane() {
        let harness = TaskHarness::infer(
            "[Observer benchmark plan approved]\n\
<observer_benchmark_plan source=\"deterministic_observer\" required=\"true\">\n\
lane: runtime_eval\n\
case_id_hint: runtime-eval-task-harness\n\
required_checks:\n\
- cargo run --quiet -- eval --spec .spiral-coder/runtime_eval.json --max-cases 1\n\
</observer_benchmark_plan>",
            false,
        );
        assert_eq!(harness.lane, TaskLane::BenchmarkPlan);
        assert_eq!(harness.artifact_mode, ArtifactMode::ExistingFiles);

        let prompt = harness.prompt(None);
        assert!(prompt.contains("Treat `<observer_benchmark_plan>`"));
        assert!(prompt.contains(".spiral-coder/runtime_eval.json"));
        assert!(prompt.contains("Do not only explain"));
    }

    #[test]
    fn coerce_benchmark_plan_tool_call_reads_runtime_eval_spec_first() {
        let harness = TaskHarness::infer(
            "[Observer benchmark plan approved]\n\
<observer_benchmark_plan source=\"deterministic_observer\" required=\"true\">\n\
lane: runtime_eval\n\
objective: Add a deterministic regression.\n\
required_checks:\n\
- cargo run --quiet -- eval --spec .spiral-coder/runtime_eval.json --max-cases 1\n\
</observer_benchmark_plan>",
            false,
        );
        let tc = ToolCallData {
            id: "call_search".to_string(),
            name: "search_files".to_string(),
            arguments: json!({"pattern":"runtime eval","path":"src"}).to_string(),
        };

        let (rewritten, original, coerced) = coerce_benchmark_plan_tool_call(
            harness,
            &[],
            &tc,
            "lane: runtime_eval",
            None,
            None,
            None,
        )
        .expect("rewritten");

        assert_eq!(original, "search_files(path=src, pattern=runtime eval)");
        assert_eq!(rewritten.name, "read_file");
        assert!(rewritten
            .arguments
            .contains(".spiral-coder/runtime_eval.json"));
        assert_eq!(coerced, "read_file(path=.spiral-coder/runtime_eval.json)");
    }

    #[test]
    fn coerce_benchmark_plan_tool_call_does_not_patch_before_spec_read() {
        let dir = tempdir().unwrap();
        let spiral_coder = dir.path().join(".spiral-coder");
        std::fs::create_dir_all(&spiral_coder).unwrap();
        std::fs::write(
            spiral_coder.join("runtime_eval.json"),
            "{\n  \"version\": 1,\n  \"paths\": []\n}\n",
        )
        .unwrap();
        let tc = ToolCallData {
            id: "call_list".to_string(),
            name: "list_dir".to_string(),
            arguments: json!({"dir":".spiral-coder"}).to_string(),
        };

        let (rewritten, _original, coerced) = coerce_benchmark_plan_tool_call(
            TaskHarness {
                lane: TaskLane::BenchmarkPlan,
                artifact_mode: ArtifactMode::ExistingFiles,
            },
            &[],
            &tc,
            "<observer_benchmark_plan>\n\
lane: runtime_eval\n\
success_criteria:\n\
- .spiral-coder/runtime_eval.json includes src/tui/agent/session_bridge.rs\n\
</observer_benchmark_plan>",
            Some(dir.path().to_str().unwrap()),
            None,
            None,
        )
        .expect("read first");

        assert_eq!(rewritten.name, "read_file");
        assert_eq!(coerced, "read_file(path=.spiral-coder/runtime_eval.json)");
    }

    #[test]
    fn coerce_benchmark_plan_tool_call_stops_after_spec_read() {
        let harness = TaskHarness {
            lane: TaskLane::BenchmarkPlan,
            artifact_mode: ArtifactMode::ExistingFiles,
        };
        let messages = vec![
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_read",
                    "type": "function",
                    "function": {"name":"read_file","arguments":"{\"path\":\".spiral-coder/tui_replay.json\"}"}
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_read",
                "content": "[.spiral-coder/tui_replay.json] (3 lines, 42 bytes)\n{\"cases\":[]}"
            }),
        ];
        let tc = ToolCallData {
            id: "call_list".to_string(),
            name: "list_dir".to_string(),
            arguments: json!({"dir":".spiral-coder"}).to_string(),
        };

        let rewritten = coerce_benchmark_plan_tool_call(
            harness,
            &messages,
            &tc,
            "<observer_benchmark_plan>\nlane: tui_replay\n</observer_benchmark_plan>",
            None,
            None,
            None,
        );

        assert!(rewritten.is_none());
    }

    #[test]
    fn coerce_benchmark_plan_tool_call_synthesizes_runtime_eval_patch_after_spec_read() {
        let body = "{\n  \"version\": 1,\n  \"paths\": [\n    \"src/tui/agent/task_harness.rs\"\n  ],\n  \"cases\": []\n}\n";
        let messages = vec![
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_read",
                    "type": "function",
                    "function": {"name":"read_file","arguments":"{\"path\":\".spiral-coder/runtime_eval.json\"}"}
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_read",
                "content": format!("[.spiral-coder/runtime_eval.json] (8 lines, 100 bytes)\n{body}")
            }),
        ];
        let tc = ToolCallData {
            id: "call_bad_patch".to_string(),
            name: "patch_file".to_string(),
            arguments: "{\"path\":\".spiral-coder/runtime_eval.json\",\"search\":\"{".to_string(),
        };

        let (rewritten, _original, coerced) = coerce_benchmark_plan_tool_call(
            TaskHarness {
                lane: TaskLane::BenchmarkPlan,
                artifact_mode: ArtifactMode::ExistingFiles,
            },
            &messages,
            &tc,
            "<observer_benchmark_plan>\n\
lane: runtime_eval\n\
case_id_hint: self-fix-session-bridge-runtime-followup\n\
success_criteria:\n\
- .spiral-coder/runtime_eval.json includes src/tui/agent/session_bridge.rs\n\
</observer_benchmark_plan>",
            None,
            None,
            None,
        )
        .expect("synthetic benchmark patch");

        assert_eq!(rewritten.name, "patch_file");
        assert!(rewritten
            .arguments
            .contains(".spiral-coder/runtime_eval.json"));
        assert!(rewritten
            .arguments
            .contains("src/tui/agent/session_bridge.rs"));
        assert!(coerced.starts_with("patch_file("));
    }

    #[test]
    fn coerce_benchmark_plan_tool_call_allows_verify_after_filesystem_patch() {
        let dir = tempdir().unwrap();
        let spiral_coder = dir.path().join(".spiral-coder");
        std::fs::create_dir_all(&spiral_coder).unwrap();
        std::fs::write(
            spiral_coder.join("runtime_eval.json"),
            "{\n  \"version\": 1,\n  \"paths\": [\n    \"src/tui/agent/session_bridge.rs\"\n  ],\n  \"cases\": []\n}\n",
        )
        .unwrap();
        let stale_body = "{\n  \"version\": 1,\n  \"paths\": [],\n  \"cases\": []\n}\n";
        let messages = vec![
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_read",
                    "type": "function",
                    "function": {"name":"read_file","arguments":"{\"path\":\".spiral-coder/runtime_eval.json\"}"}
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_read",
                "content": format!("[.spiral-coder/runtime_eval.json] (5 lines, 70 bytes)\n{stale_body}")
            }),
        ];
        let tc = ToolCallData {
            id: "call_verify".to_string(),
            name: "exec".to_string(),
            arguments: json!({"command":"cargo test 2>&1"}).to_string(),
        };

        let rewritten = coerce_benchmark_plan_tool_call(
            TaskHarness {
                lane: TaskLane::BenchmarkPlan,
                artifact_mode: ArtifactMode::ExistingFiles,
            },
            &messages,
            &tc,
            "<observer_benchmark_plan>\n\
lane: runtime_eval\n\
success_criteria:\n\
- .spiral-coder/runtime_eval.json includes src/tui/agent/session_bridge.rs\n\
</observer_benchmark_plan>",
            Some(dir.path().to_str().unwrap()),
            None,
            None,
        );

        assert!(rewritten.is_none());
    }

    #[test]
    fn coerce_benchmark_plan_tool_call_reads_docs_after_spec_is_satisfied() {
        let dir = tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".spiral-coder")).unwrap();
        std::fs::create_dir_all(dir.path().join("docs")).unwrap();
        std::fs::write(
            dir.path().join(".spiral-coder/runtime_eval.json"),
            "{\n  \"version\": 1,\n  \"paths\": [\n    \"src/tui/agent/session_bridge.rs\"\n  ],\n  \"cases\": []\n}\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("docs/runtime-architecture.md"),
            "# Runtime\n",
        )
        .unwrap();
        let messages = vec![
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_read_spec",
                    "type": "function",
                    "function": {"name":"read_file","arguments":"{\"path\":\".spiral-coder/runtime_eval.json\"}"}
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_read_spec",
                "content": "[.spiral-coder/runtime_eval.json] (5 lines, 70 bytes)\n{\"paths\":[]}"
            }),
        ];
        let tc = ToolCallData {
            id: "call_verify".to_string(),
            name: "exec".to_string(),
            arguments: json!({"command":"grep -q session_bridge .spiral-coder/runtime_eval.json"})
                .to_string(),
        };

        let (rewritten, _original, coerced) = coerce_benchmark_plan_tool_call(
            TaskHarness {
                lane: TaskLane::BenchmarkPlan,
                artifact_mode: ArtifactMode::ExistingFiles,
            },
            &messages,
            &tc,
            "<observer_benchmark_plan>\n\
lane: runtime_eval\n\
target_files:\n\
- .spiral-coder/runtime_eval.json\n\
- docs/runtime-architecture.md\n\
success_criteria:\n\
- both files include src/tui/agent/session_bridge.rs\n\
</observer_benchmark_plan>",
            Some(dir.path().to_str().unwrap()),
            None,
            None,
        )
        .expect("docs read");

        assert_eq!(rewritten.name, "read_file");
        assert!(rewritten.arguments.contains("docs/runtime-architecture.md"));
        assert_eq!(coerced, "read_file(path=docs/runtime-architecture.md)");
    }

    #[test]
    fn coerce_benchmark_plan_tool_call_synthesizes_docs_patch_after_docs_read() {
        let messages = vec![
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_read_spec",
                    "type": "function",
                    "function": {"name":"read_file","arguments":"{\"path\":\".spiral-coder/runtime_eval.json\"}"}
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_read_spec",
                "content": "[.spiral-coder/runtime_eval.json] (5 lines, 70 bytes)\n{\"paths\":[\"src/tui/agent/session_bridge.rs\"]}"
            }),
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_read_docs",
                    "type": "function",
                    "function": {"name":"read_file","arguments":"{\"path\":\"docs/runtime-architecture.md\"}"}
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_read_docs",
                "content": "[docs/runtime-architecture.md] (1 lines, 10 bytes)\n# Runtime\n"
            }),
        ];
        let tc = ToolCallData {
            id: "call_done".to_string(),
            name: "done".to_string(),
            arguments: json!({"summary":"done"}).to_string(),
        };

        let (rewritten, _original, coerced) = coerce_benchmark_plan_tool_call(
            TaskHarness {
                lane: TaskLane::BenchmarkPlan,
                artifact_mode: ArtifactMode::ExistingFiles,
            },
            &messages,
            &tc,
            "<observer_benchmark_plan>\n\
lane: runtime_eval\n\
target_files:\n\
- .spiral-coder/runtime_eval.json\n\
- docs/runtime-architecture.md\n\
success_criteria:\n\
- both files include src/tui/agent/session_bridge.rs\n\
</observer_benchmark_plan>",
            None,
            None,
            None,
        )
        .expect("docs patch");

        assert_eq!(rewritten.name, "patch_file");
        assert!(rewritten.arguments.contains("docs/runtime-architecture.md"));
        assert!(rewritten
            .arguments
            .contains("src/tui/agent/session_bridge.rs"));
        assert!(coerced.starts_with("patch_file("));
    }

    #[test]
    fn coerce_benchmark_plan_tool_call_allows_verify_after_spec_and_docs_patch() {
        let dir = tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".spiral-coder")).unwrap();
        std::fs::create_dir_all(dir.path().join("docs")).unwrap();
        std::fs::write(
            dir.path().join(".spiral-coder/runtime_eval.json"),
            "{\n  \"paths\": [\"src/tui/agent/session_bridge.rs\"]\n}\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("docs/runtime-architecture.md"),
            "- `src/tui/agent/session_bridge.rs`\n",
        )
        .unwrap();
        let messages = vec![
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_read_spec",
                    "type": "function",
                    "function": {"name":"read_file","arguments":"{\"path\":\".spiral-coder/runtime_eval.json\"}"}
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_read_spec",
                "content": "[.spiral-coder/runtime_eval.json] (2 lines, 20 bytes)\n{\"paths\":[]}"
            }),
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_read_docs",
                    "type": "function",
                    "function": {"name":"read_file","arguments":"{\"path\":\"docs/runtime-architecture.md\"}"}
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_read_docs",
                "content": "[docs/runtime-architecture.md] (1 lines, 10 bytes)\n# Runtime\n"
            }),
        ];
        let tc = ToolCallData {
            id: "call_verify".to_string(),
            name: "exec".to_string(),
            arguments: json!({"command":"grep -q session_bridge docs/runtime-architecture.md"})
                .to_string(),
        };

        let rewritten = coerce_benchmark_plan_tool_call(
            TaskHarness {
                lane: TaskLane::BenchmarkPlan,
                artifact_mode: ArtifactMode::ExistingFiles,
            },
            &messages,
            &tc,
            "<observer_benchmark_plan>\n\
lane: runtime_eval\n\
target_files:\n\
- .spiral-coder/runtime_eval.json\n\
- docs/runtime-architecture.md\n\
success_criteria:\n\
- both files include src/tui/agent/session_bridge.rs\n\
</observer_benchmark_plan>",
            Some(dir.path().to_str().unwrap()),
            None,
            None,
        );

        assert!(rewritten.is_none());
    }

    #[test]
    fn allows_benchmark_plan_followup_during_verify_reads_next_unsatisfied_target() {
        let dir = tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".spiral-coder")).unwrap();
        std::fs::create_dir_all(dir.path().join("docs")).unwrap();
        std::fs::write(
            dir.path().join(".spiral-coder/runtime_eval.json"),
            "{\n  \"paths\": [\"src/tui/agent/session_bridge.rs\"]\n}\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("docs/runtime-architecture.md"),
            "# Runtime\n",
        )
        .unwrap();
        let messages = vec![
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_read_spec",
                    "type": "function",
                    "function": {"name":"read_file","arguments":"{\"path\":\".spiral-coder/runtime_eval.json\"}"}
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_read_spec",
                "content": "[.spiral-coder/runtime_eval.json] (2 lines, 20 bytes)\n{\"paths\":[]}"
            }),
        ];
        let tc = ToolCallData {
            id: "call_read_docs".to_string(),
            name: "read_file".to_string(),
            arguments: json!({"path":"docs/runtime-architecture.md"}).to_string(),
        };

        assert!(allows_benchmark_plan_followup_during_verify(
            TaskHarness {
                lane: TaskLane::BenchmarkPlan,
                artifact_mode: ArtifactMode::ExistingFiles,
            },
            &messages,
            &tc,
            "<observer_benchmark_plan>\n\
lane: runtime_eval\n\
target_files:\n\
- .spiral-coder/runtime_eval.json\n\
- docs/runtime-architecture.md\n\
success_criteria:\n\
- both files include src/tui/agent/session_bridge.rs\n\
</observer_benchmark_plan>",
            Some(dir.path().to_str().unwrap()),
            None
        ));
    }

    #[test]
    fn allows_benchmark_plan_followup_during_verify_allows_target_patch_after_read() {
        let messages = vec![
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_read_spec",
                    "type": "function",
                    "function": {"name":"read_file","arguments":"{\"path\":\".spiral-coder/runtime_eval.json\"}"}
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_read_spec",
                "content": "[.spiral-coder/runtime_eval.json] (2 lines, 20 bytes)\n{\"paths\":[\"src/tui/agent/session_bridge.rs\"]}"
            }),
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_read_docs",
                    "type": "function",
                    "function": {"name":"read_file","arguments":"{\"path\":\"docs/runtime-architecture.md\"}"}
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_read_docs",
                "content": "[docs/runtime-architecture.md] (1 lines, 10 bytes)\n# Runtime\n"
            }),
        ];
        let tc = ToolCallData {
            id: "call_patch_docs".to_string(),
            name: "patch_file".to_string(),
            arguments: json!({
                "path": "docs/runtime-architecture.md",
                "search": "# Runtime\n",
                "replace": "# Runtime\n\n- `src/tui/agent/session_bridge.rs` is covered.\n"
            })
            .to_string(),
        };

        assert!(allows_benchmark_plan_followup_during_verify(
            TaskHarness {
                lane: TaskLane::BenchmarkPlan,
                artifact_mode: ArtifactMode::ExistingFiles,
            },
            &messages,
            &tc,
            "<observer_benchmark_plan>\n\
lane: runtime_eval\n\
target_files:\n\
- .spiral-coder/runtime_eval.json\n\
- docs/runtime-architecture.md\n\
success_criteria:\n\
- both files include src/tui/agent/session_bridge.rs\n\
</observer_benchmark_plan>",
            None,
            None
        ));
    }

    #[test]
    fn allows_benchmark_plan_followup_during_verify_rejects_wrong_target() {
        let messages = vec![
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_read_spec",
                    "type": "function",
                    "function": {"name":"read_file","arguments":"{\"path\":\".spiral-coder/runtime_eval.json\"}"}
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_read_spec",
                "content": "[.spiral-coder/runtime_eval.json] (2 lines, 20 bytes)\n{\"paths\":[\"src/tui/agent/session_bridge.rs\"]}"
            }),
        ];
        let tc = ToolCallData {
            id: "call_read_readme".to_string(),
            name: "read_file".to_string(),
            arguments: json!({"path":"README.md"}).to_string(),
        };

        assert!(!allows_benchmark_plan_followup_during_verify(
            TaskHarness {
                lane: TaskLane::BenchmarkPlan,
                artifact_mode: ArtifactMode::ExistingFiles,
            },
            &messages,
            &tc,
            "<observer_benchmark_plan>\n\
lane: runtime_eval\n\
target_files:\n\
- .spiral-coder/runtime_eval.json\n\
- docs/runtime-architecture.md\n\
success_criteria:\n\
- both files include src/tui/agent/session_bridge.rs\n\
</observer_benchmark_plan>",
            None,
            None
        ));
    }

    #[test]
    fn coerce_benchmark_plan_tool_call_runs_required_check_after_targets_satisfied() {
        let dir = tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".spiral-coder")).unwrap();
        std::fs::create_dir_all(dir.path().join("docs")).unwrap();
        std::fs::write(
            dir.path().join(".spiral-coder/runtime_eval.json"),
            "{\n  \"paths\": [\"src/tui/agent/session_bridge.rs\"]\n}\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("docs/runtime-architecture.md"),
            "- `src/tui/agent/session_bridge.rs`\n",
        )
        .unwrap();
        let messages = vec![
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_read_spec",
                    "type": "function",
                    "function": {"name":"read_file","arguments":"{\"path\":\".spiral-coder/runtime_eval.json\"}"}
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_read_spec",
                "content": "[.spiral-coder/runtime_eval.json] (2 lines, 20 bytes)\n{\"paths\":[]}"
            }),
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_read_docs",
                    "type": "function",
                    "function": {"name":"read_file","arguments":"{\"path\":\"docs/runtime-architecture.md\"}"}
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_read_docs",
                "content": "[docs/runtime-architecture.md] (1 lines, 10 bytes)\n# Runtime\n"
            }),
        ];
        let tc = ToolCallData {
            id: "call_done".to_string(),
            name: "done".to_string(),
            arguments: json!({"summary":"done"}).to_string(),
        };

        let (rewritten, _original, coerced) = coerce_benchmark_plan_tool_call(
            TaskHarness {
                lane: TaskLane::BenchmarkPlan,
                artifact_mode: ArtifactMode::ExistingFiles,
            },
            &messages,
            &tc,
            "<observer_benchmark_plan>\n\
lane: runtime_eval\n\
target_files:\n\
- .spiral-coder/runtime_eval.json\n\
- docs/runtime-architecture.md\n\
required_checks:\n\
- grep -q \"src/tui/agent/session_bridge.rs\" .spiral-coder/runtime_eval.json && grep -q \"src/tui/agent/session_bridge.rs\" docs/runtime-architecture.md\n\
success_criteria:\n\
- both files include src/tui/agent/session_bridge.rs\n\
</observer_benchmark_plan>",
            Some(dir.path().to_str().unwrap()),
        None, None
        )
        .expect("required check exec");

        assert_eq!(rewritten.name, "exec");
        assert!(rewritten
            .arguments
            .contains("grep -q \\\"src/tui/agent/session_bridge.rs\\\""));
        assert!(coerced.contains("grep -q"));
    }

    #[test]
    fn synthesize_benchmark_plan_no_tool_call_patches_pending_docs_target() {
        let messages = vec![
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_read_spec",
                    "type": "function",
                    "function": {"name":"read_file","arguments":"{\"path\":\".spiral-coder/runtime_eval.json\"}"}
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_read_spec",
                "content": "[.spiral-coder/runtime_eval.json] (2 lines, 20 bytes)\n{\"paths\":[\"src/tui/agent/session_bridge.rs\"]}"
            }),
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_read_docs",
                    "type": "function",
                    "function": {"name":"read_file","arguments":"{\"path\":\"docs/runtime-architecture.md\"}"}
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_read_docs",
                "content": "[docs/runtime-architecture.md] (1 lines, 10 bytes)\n# Runtime\n"
            }),
        ];

        let synthesized = synthesize_benchmark_plan_no_tool_call(
            TaskHarness {
                lane: TaskLane::BenchmarkPlan,
                artifact_mode: ArtifactMode::ExistingFiles,
            },
            &messages,
            "<observer_benchmark_plan>\n\
lane: runtime_eval\n\
target_files:\n\
- .spiral-coder/runtime_eval.json\n\
- docs/runtime-architecture.md\n\
required_checks:\n\
- grep -q \"src/tui/agent/session_bridge.rs\" .spiral-coder/runtime_eval.json && grep -q \"src/tui/agent/session_bridge.rs\" docs/runtime-architecture.md\n\
success_criteria:\n\
- both files include src/tui/agent/session_bridge.rs\n\
</observer_benchmark_plan>",
            None,
        None, None
        )
        .expect("synthetic docs patch");

        assert_eq!(synthesized.name, "patch_file");
        assert!(synthesized
            .arguments
            .contains("docs/runtime-architecture.md"));
        assert!(synthesized
            .arguments
            .contains("src/tui/agent/session_bridge.rs"));
    }

    #[test]
    fn benchmark_plan_missing_required_exec_proof_clears_after_successful_exec() {
        let root_user_text = "<observer_benchmark_plan>\n\
lane: runtime_eval\n\
target_files:\n\
- .spiral-coder/runtime_eval.json\n\
required_checks:\n\
- grep -q \"src/tui/agent/session_bridge.rs\" .spiral-coder/runtime_eval.json\n\
</observer_benchmark_plan>";
        let messages = vec![
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_exec",
                    "type": "function",
                    "function": {
                        "name": "exec",
                        "arguments": "{\"command\":\"grep -q \\\"src/tui/agent/session_bridge.rs\\\" .spiral-coder/runtime_eval.json\"}"
                    }
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_exec",
                "content": "OK (exit_code: 0)\nstdout:\n"
            }),
        ];

        assert!(benchmark_plan_missing_required_exec_proof(
            root_user_text,
            &[],
            None
        ));
        assert!(!benchmark_plan_missing_required_exec_proof(
            root_user_text,
            &messages,
            None
        ));
    }

    #[test]
    fn benchmark_plan_advances_to_second_check_in_verify_and_done_coercion() {
        let prompt = "<observer_benchmark_plan>\nlane: runtime_eval\nrequired_checks:\n- check Spec.json\n- grep -q 'A  B' Spec.json\nsuccess_criteria:\n- cover src/example.rs\n</observer_benchmark_plan>";
        let messages = vec![
            json!({"role": "assistant", "tool_calls": [{
                "id": "read", "function": {"name": "read_file", "arguments": json!({"path": ".spiral-coder/runtime_eval.json"}).to_string()}
            }]}),
            json!({"role": "tool", "tool_call_id": "read", "content": "[.spiral-coder/runtime_eval.json] (1 lines, 30 bytes)\n{\"paths\":[\"src/example.rs\"]}"}),
            json!({"role": "assistant", "tool_calls": [{
                "id": "check", "function": {"name": "exec", "arguments": json!({"command": "check Spec.json"}).to_string()}
            }]}),
            json!({"role": "tool", "tool_call_id": "check", "content": "OK (exit_code: 0)"}),
        ];
        let harness = TaskHarness {
            lane: TaskLane::BenchmarkPlan,
            artifact_mode: ArtifactMode::ExistingFiles,
        };
        let second = "grep -q 'A  B' Spec.json";
        assert!(benchmark_plan_missing_required_exec_proof(
            prompt, &messages, None
        ));
        assert_eq!(
            benchmark_plan_pending_required_exec_command(prompt, &messages, None).as_deref(),
            Some(second)
        );

        for (name, args) in [
            ("done", json!({"summary": "done"})),
            ("exec", json!({"command": "grep -q 'A B' Spec.json"})),
            ("exec", json!({"command": "grep -q 'a  b' Spec.json"})),
        ] {
            let candidate = ToolCallData {
                id: "next".to_string(),
                name: name.to_string(),
                arguments: args.to_string(),
            };
            let (rewritten, _, _) = coerce_benchmark_plan_tool_call(
                harness, &messages, &candidate, prompt, None, None, None,
            )
            .expect("second check remains required");
            assert_eq!(rewritten.name, "exec");
            assert_eq!(
                serde_json::from_str::<Value>(&rewritten.arguments).unwrap(),
                json!({"command": second})
            );
            assert!(allows_benchmark_plan_followup_during_verify(
                harness, &messages, &rewritten, prompt, None, None
            ));
        }
    }

    #[test]
    fn benchmark_plan_spec_path_prefers_explicit_lane_over_required_check_paths() {
        let path = benchmark_plan_spec_path(
            "<observer_benchmark_plan>\n\
lane: runtime_eval\n\
required_checks:\n\
- cargo run --quiet -- tui-replay --spec .spiral-coder/tui_replay.json\n\
</observer_benchmark_plan>",
        );

        assert_eq!(path.as_deref(), Some(".spiral-coder/runtime_eval.json"));
    }

    #[test]
    fn progress_gate_blocks_repeated_fix_read() {
        let messages = vec![
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_read",
                    "type": "function",
                    "function": {"name":"read_file","arguments":"{\"path\":\"src/lib.rs\"}"}
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_read",
                "content": "[src/lib.rs] (13 lines, 228 bytes)\npub fn greet(name: &str) -> String {"
            }),
        ];
        let tc = ToolCallData {
            id: "call_repeat".to_string(),
            name: "read_file".to_string(),
            arguments: json!({"path":"src/lib.rs"}).to_string(),
        };

        let block = build_progress_gate_block(
            TaskHarness {
                lane: TaskLane::FixExisting,
                artifact_mode: ArtifactMode::ExistingFiles,
            },
            &tc,
            &messages,
            Some(RecoveryStage::Fix),
            Some("cargo test 2>&1"),
            None,
            0,
        )
        .expect("progress gate block");

        assert!(block.contains("[Progress Gate]"));
        assert!(block.contains("patch_file"));
        assert!(block.contains("cargo test 2>&1"));
    }

    #[test]
    fn progress_gate_blocks_third_observation_for_create_file_lane() {
        let messages = vec![
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_list",
                    "type": "function",
                    "function": {"name":"list_dir","arguments":"{\"dir\":\".\",\"include_hidden\":true}"}
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_list",
                "content": "[list_dir: '.' ・ 2 item(s)]\nREADME.md\n.spiral-coder.md"
            }),
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_read",
                    "type": "function",
                    "function": {"name":"read_file","arguments":"{\"path\":\"README.md\"}"}
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_read",
                "content": "[README.md] (2 lines, 20 bytes)\nfixture"
            }),
        ];
        let tc = ToolCallData {
            id: "call_search".to_string(),
            name: "search_files".to_string(),
            arguments: json!({"pattern":"todo","dir":"."}).to_string(),
        };

        let block = build_progress_gate_block(
            TaskHarness {
                lane: TaskLane::CreateFile,
                artifact_mode: ArtifactMode::NewFiles,
            },
            &tc,
            &messages,
            Some(RecoveryStage::Fix),
            Some("test -f notes/todo.txt && grep -Fx \"ship it\" notes/todo.txt"),
            None,
            0,
        )
        .expect("progress gate block");

        assert!(block.contains("create the requested file now"));
        assert!(block.contains("notes/todo.txt") || block.contains("Configured"));
    }

    #[test]
    fn build_fix_stage_progress_hint_prefers_creation_after_first_success() {
        let messages = vec![
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_list",
                    "type": "function",
                    "function": {"name":"list_dir","arguments":"{\"dir\":\".\",\"include_hidden\":true}"}
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_list",
                "content": "[list_dir: '.' ・ 2 item(s)]\nREADME.md\n.spiral-coder.md"
            }),
        ];

        let hint = build_fix_stage_progress_hint(
            TaskHarness {
                lane: TaskLane::CreateFile,
                artifact_mode: ArtifactMode::NewFiles,
            },
            &messages,
            Some(RecoveryStage::Fix),
            Some("test -f notes/todo.txt && grep -Fx \"ship it\" notes/todo.txt"),
        )
        .expect("fix-stage hint");

        assert!(hint.contains("create the requested file now"));
        assert!(hint.contains("notes/todo.txt"));
    }

    #[test]
    fn progress_gate_blocks_verify_exec_before_first_mutation_in_fix_lane() {
        let mut messages = vec![
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_read_cargo",
                    "type": "function",
                    "function": {"name":"read_file","arguments":"{\"path\":\"Cargo.toml\"}"}
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_read_cargo",
                "content": "[Cargo.toml] (8 lines, 120 bytes)\n[package]"
            }),
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_read_lib",
                    "type": "function",
                    "function": {"name":"read_file","arguments":"{\"path\":\"src/lib.rs\"}"}
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_read_lib",
                "content": "[src/lib.rs] (22 lines, 561 bytes)\nmod maze;"
            }),
        ];
        messages.push(json!({
            "role":"assistant", "tool_calls":[{
                "id":"baseline", "type":"function", "function":{
                    "name":"exec", "arguments":json!({"command":"cargo test 2>&1"}).to_string()
                }
            }]
        }));
        messages.push(json!({"role":"tool", "tool_call_id":"baseline",
            "content":"FAILED (exit_code: 1)\nstdout:\ntest failed"}));
        let tc = ToolCallData {
            id: "call_verify".to_string(),
            name: "exec".to_string(),
            arguments: json!({"command":"cargo test 2>&1"}).to_string(),
        };

        let block = build_progress_gate_block(
            TaskHarness {
                lane: TaskLane::FixExisting,
                artifact_mode: ArtifactMode::ExistingFiles,
            },
            &tc,
            &messages,
            Some(RecoveryStage::Fix),
            Some("cargo test 2>&1"),
            None,
            0,
        )
        .expect("progress gate verify block");

        assert!(
            block.contains("read `src/maze.rs` now to inspect the implementation before patching.")
        );
        assert!(block
            .contains("Do NOT rerun verification or widen search until `src/maze.rs` is read."));
    }

    #[test]
    fn progress_gate_allows_first_target_read_in_fix_lane() {
        let messages = vec![
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_read_cargo",
                    "type": "function",
                    "function": {"name":"read_file","arguments":"{\"path\":\"Cargo.toml\"}"}
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_read_cargo",
                "content": "[Cargo.toml] (8 lines, 120 bytes)\n[package]"
            }),
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_read_lib",
                    "type": "function",
                    "function": {"name":"read_file","arguments":"{\"path\":\"src/lib.rs\"}"}
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_read_lib",
                "content": "[src/lib.rs] (22 lines, 561 bytes)\nmod maze;"
            }),
        ];
        let tc = ToolCallData {
            id: "call_read_maze".to_string(),
            name: "read_file".to_string(),
            arguments: json!({"path":"src/maze.rs"}).to_string(),
        };

        let block = build_progress_gate_block(
            TaskHarness {
                lane: TaskLane::FixExisting,
                artifact_mode: ArtifactMode::ExistingFiles,
            },
            &tc,
            &messages,
            Some(RecoveryStage::Fix),
            Some("cargo test 2>&1"),
            None,
            0,
        );

        assert!(block.is_none());
    }

    #[test]
    fn progress_gate_allows_src_reread_after_file_tool_failure() {
        let messages = vec![
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_read",
                    "type": "function",
                    "function": {"name":"read_file","arguments":"{\"path\":\"src/merge_gate.rs\"}"}
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_read",
                "content": "[src/merge_gate.rs] (20 lines, 500 bytes)\nfn status_for_case() {}"
            }),
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_patch",
                    "type": "function",
                    "function": {"name":"patch_file","arguments":"{\"path\":\"src/merge_gate.rs\",\"search\":\"fn status\""}
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_patch",
                "content": "ERROR: search string cannot be empty"
            }),
        ];
        let tc = ToolCallData {
            id: "call_reread".to_string(),
            name: "read_file".to_string(),
            arguments: json!({"path":"src/merge_gate.rs"}).to_string(),
        };

        let block = build_progress_gate_block(
            TaskHarness {
                lane: TaskLane::FixExisting,
                artifact_mode: ArtifactMode::ExistingFiles,
            },
            &tc,
            &messages,
            Some(RecoveryStage::Fix),
            Some("cargo test 2>&1"),
            None,
            1,
        );

        assert!(block.is_none());
    }

    #[test]
    fn synthesize_no_tool_mutation_adds_failed_status_guard() {
        let body = "fn status_for_case(case: &MergeGateCase<'_>) -> MergeGateBoardStatus {\n    if let Some(review) = case.review {\n        return match review.decision {\n            MergeGateReviewDecision::Approved => MergeGateBoardStatus::Approved,\n            MergeGateReviewDecision::Held => MergeGateBoardStatus::Held,\n        };\n    }\n    if case.status == \"passed\" {\n        MergeGateBoardStatus::NeedsReview\n    } else if case.rollback_command.is_some() {\n        MergeGateBoardStatus::RollbackAvailable\n    } else {\n        MergeGateBoardStatus::Blocked\n    }\n}\n";
        let messages = vec![
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_read",
                    "type": "function",
                    "function": {"name":"read_file","arguments":"{\"path\":\"src/merge_gate.rs\"}"}
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_read",
                "content": format!("[src/merge_gate.rs] (20 lines, 500 bytes)\n{body}")
            }),
            json!({
                "role": "user",
                "content": "[goal_check]\nTests are failing.\n---- merge_gate::tests::stale_approval_cannot_approve_failed_case_with_rollback stdout ----"
            }),
        ];

        let tc = synthesize_fix_existing_no_tool_mutation_tool_call(
            TaskHarness {
                lane: TaskLane::FixExisting,
                artifact_mode: ArtifactMode::ExistingFiles,
            },
            &messages,
            "Fix failed cases with rollback so stale approved reviews stay rollback available and failed cases without rollback stay blocked.",
        )
        .expect("synthetic guard patch");

        assert_eq!(tc.name, "patch_file");
        assert!(tc.arguments.contains("src/merge_gate.rs"));
        assert!(tc.arguments.contains("case.status != \\\"passed\\\""));
        assert!(tc.arguments.contains("RollbackAvailable"));
        assert!(tc.arguments.contains("Blocked"));
    }

    #[test]
    fn fix_existing_tasks_do_not_request_repo_goal_check() {
        assert!(!TaskHarness {
            lane: TaskLane::FixExisting,
            artifact_mode: ArtifactMode::ExistingFiles,
        }
        .allows_repo_goal_check());
        assert!(TaskHarness {
            lane: TaskLane::ScaffoldRepo,
            artifact_mode: ArtifactMode::NewRepo,
        }
        .allows_repo_goal_check());
    }

    #[test]
    fn coerce_fix_existing_tool_call_rewrites_verify_to_impl_read() {
        let messages = vec![
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_read_cargo",
                    "type": "function",
                    "function": {"name":"read_file","arguments":"{\"path\":\"Cargo.toml\"}"}
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_read_cargo",
                "content": "[Cargo.toml] (8 lines, 120 bytes)\n[package]"
            }),
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_read_lib",
                    "type": "function",
                    "function": {"name":"read_file","arguments":"{\"path\":\"src/lib.rs\"}"}
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_read_lib",
                "content": "[src/lib.rs] (22 lines, 561 bytes)\nmod maze;\n\npub use maze::{Direction, Maze, Point};"
            }),
        ];
        let tc = ToolCallData {
            id: "call_verify".to_string(),
            name: "exec".to_string(),
            arguments: json!({"command":"cargo test 2>&1"}).to_string(),
        };

        let (rewritten, original, coerced) = coerce_fix_existing_tool_call(
            TaskHarness {
                lane: TaskLane::FixExisting,
                artifact_mode: ArtifactMode::ExistingFiles,
            },
            &messages,
            &tc,
            Some("cargo test 2>&1"),
            None,
        )
        .expect("fix-existing coercion");

        assert_eq!(original, "cargo test 2>&1");
        assert_eq!(rewritten.name, "read_file");
        assert!(rewritten.arguments.contains("src/maze.rs"));
        assert_eq!(coerced, "read_file(path=src/maze.rs)");
    }

    #[test]
    fn coerce_fix_existing_tool_call_uses_src_anchor_after_search() {
        let tmp = tempdir().expect("tempdir");
        std::fs::create_dir_all(tmp.path().join("src")).expect("create src");
        std::fs::write(tmp.path().join("src/lib.rs"), "mod robot;\n").expect("write lib");

        let messages = vec![
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_list_src",
                    "type": "function",
                    "function": {"name":"list_dir","arguments":"{\"dir\":\"src\"}"}
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_list_src",
                "content": "[list_dir: 'src' ・ 2 item(s)]\nlib.rs\nrobot.rs"
            }),
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_search_tests",
                    "type": "function",
                    "function": {"name":"search_files","arguments":"{\"dir\":\"src\",\"pattern\":\"#[test]\"}"}
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_search_tests",
                "content": "[search_files: '#[test]' — 2 match(es)] [pruned 3L]"
            }),
        ];
        let tc = ToolCallData {
            id: "call_repeat_search".to_string(),
            name: "search_files".to_string(),
            arguments: json!({"dir":"src","pattern":"#[test]"}).to_string(),
        };

        let (rewritten, original, coerced) = coerce_fix_existing_tool_call(
            TaskHarness {
                lane: TaskLane::FixExisting,
                artifact_mode: ArtifactMode::ExistingFiles,
            },
            &messages,
            &tc,
            Some("cargo test 2>&1"),
            tmp.path().to_str(),
        )
        .expect("fix-existing anchor coercion");

        assert_eq!(original, "search_files(dir=src, pattern=#[test])");
        assert_eq!(rewritten.name, "read_file");
        assert_eq!(coerced, "read_file(path=src/lib.rs)");
    }

    #[test]
    fn coerce_fix_existing_tool_call_prefers_mod_rs_when_module_directory_exists() {
        let tmp = tempdir().expect("tempdir");
        std::fs::create_dir_all(tmp.path().join("src/observer")).expect("create observer dir");
        std::fs::write(tmp.path().join("src/lib.rs"), "pub mod observer;\n").expect("write lib");
        std::fs::write(
            tmp.path().join("src/observer/mod.rs"),
            "pub mod repo_rules;\n",
        )
        .expect("write observer mod");

        let messages = vec![
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_read_lib",
                    "type": "function",
                    "function": {"name":"read_file","arguments":"{\"path\":\"src/lib.rs\"}"}
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_read_lib",
                "content": "[src/lib.rs] (1 line, 18 bytes)\npub mod observer;"
            }),
        ];
        let tc = ToolCallData {
            id: "call_list_observer".to_string(),
            name: "list_dir".to_string(),
            arguments: json!({"dir":"src/observer"}).to_string(),
        };

        let (rewritten, original, coerced) = coerce_fix_existing_tool_call(
            TaskHarness {
                lane: TaskLane::FixExisting,
                artifact_mode: ArtifactMode::ExistingFiles,
            },
            &messages,
            &tc,
            Some("cargo test 2>&1"),
            tmp.path().to_str(),
        )
        .expect("fix-existing mod.rs coercion");

        assert_eq!(original, "list_dir(dir=src/observer)");
        assert_eq!(rewritten.name, "read_file");
        assert_eq!(coerced, "read_file(path=src/observer/mod.rs)");
    }

    #[test]
    fn coerce_fix_existing_tool_call_prefers_single_impl_file_after_subdir_listing() {
        let tmp = tempdir().expect("tempdir");
        std::fs::create_dir_all(tmp.path().join("src/observer")).expect("create observer dir");
        std::fs::write(tmp.path().join("src/lib.rs"), "pub mod observer;\n").expect("write lib");
        std::fs::write(
            tmp.path().join("src/observer/mod.rs"),
            "pub mod repo_rules;\n",
        )
        .expect("write observer mod");
        std::fs::write(
            tmp.path().join("src/observer/repo_rules.rs"),
            "pub const REPLAY_SENSITIVE: &[&str] = &[];\n",
        )
        .expect("write repo_rules");

        let messages = vec![
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_read_lib",
                    "type": "function",
                    "function": {"name":"read_file","arguments":"{\"path\":\"src/lib.rs\"}"}
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_read_lib",
                "content": "[src/lib.rs] (1 line, 18 bytes)\npub mod observer;"
            }),
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_list_observer",
                    "type": "function",
                    "function": {"name":"list_dir","arguments":"{\"dir\":\"src/observer\"}"}
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_list_observer",
                "content": "[list_dir: 'src/observer' ・ 2 item(s)]\nmod.rs\nrepo_rules.rs"
            }),
        ];
        let tc = ToolCallData {
            id: "call_list_observer_again".to_string(),
            name: "list_dir".to_string(),
            arguments: json!({"dir":"src/observer","include_hidden":true}).to_string(),
        };

        let (rewritten, original, coerced) = coerce_fix_existing_tool_call(
            TaskHarness {
                lane: TaskLane::FixExisting,
                artifact_mode: ArtifactMode::ExistingFiles,
            },
            &messages,
            &tc,
            Some("cargo test 2>&1"),
            tmp.path().to_str(),
        )
        .expect("fix-existing concrete impl coercion");

        assert_eq!(original, "list_dir(dir=src/observer, include_hidden=true)");
        assert_eq!(rewritten.name, "read_file");
        assert_eq!(coerced, "read_file(path=src/observer/repo_rules.rs)");
    }

    #[test]
    fn coerce_fix_existing_tool_call_keeps_explicit_impl_read_path() {
        let tmp = tempdir().expect("tempdir");
        std::fs::create_dir_all(tmp.path().join("src/observer")).expect("create observer dir");
        std::fs::write(tmp.path().join("src/lib.rs"), "pub mod observer;\n").expect("write lib");
        std::fs::write(
            tmp.path().join("src/observer/mod.rs"),
            "pub mod repo_rules;\n",
        )
        .expect("write observer mod");
        std::fs::write(
            tmp.path().join("src/observer/repo_rules.rs"),
            "pub const REPLAY_SENSITIVE: &[&str] = &[];\n",
        )
        .expect("write repo_rules");

        let messages = vec![json!({
            "role": "assistant",
            "tool_calls": [{
                "id": "call_list_src",
                "type": "function",
                "function": {"name":"list_dir","arguments":"{\"dir\":\"src\"}"}
            }]
        })];
        let tc = ToolCallData {
            id: "call_read_repo_rules".to_string(),
            name: "read_file".to_string(),
            arguments: json!({"path":"src/observer/repo_rules.rs"}).to_string(),
        };

        let rewritten = coerce_fix_existing_tool_call(
            TaskHarness {
                lane: TaskLane::FixExisting,
                artifact_mode: ArtifactMode::ExistingFiles,
            },
            &messages,
            &tc,
            Some("cargo test 2>&1"),
            tmp.path().to_str(),
        );

        assert!(
            rewritten.is_none(),
            "explicit impl read should not be rerouted"
        );
    }

    #[test]
    fn coerce_fix_existing_tool_call_prefers_single_impl_file_from_attempted_subdir_listing() {
        let tmp = tempdir().expect("tempdir");
        std::fs::create_dir_all(tmp.path().join("src/observer")).expect("create observer dir");
        std::fs::write(tmp.path().join("src/lib.rs"), "pub mod observer;\n").expect("write lib");
        std::fs::write(
            tmp.path().join("src/observer/mod.rs"),
            "pub mod repo_rules;\n",
        )
        .expect("write observer mod");
        std::fs::write(
            tmp.path().join("src/observer/repo_rules.rs"),
            "pub const REPLAY_SENSITIVE: &[&str] = &[];\n",
        )
        .expect("write repo_rules");

        let messages = vec![
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_list_src",
                    "type": "function",
                    "function": {"name":"list_dir","arguments":"{\"dir\":\"src\"}"}
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_list_src",
                "content": "[list_dir: 'src' ・ 2 item(s)]\nlib.rs\nobserver/"
            }),
        ];
        let tc = ToolCallData {
            id: "call_list_observer".to_string(),
            name: "list_dir".to_string(),
            arguments: json!({"dir":"src/observer","include_hidden":false}).to_string(),
        };

        let (rewritten, original, coerced) = coerce_fix_existing_tool_call(
            TaskHarness {
                lane: TaskLane::FixExisting,
                artifact_mode: ArtifactMode::ExistingFiles,
            },
            &messages,
            &tc,
            Some("cargo test 2>&1"),
            tmp.path().to_str(),
        )
        .expect("fix-existing direct impl coercion");

        assert_eq!(original, "list_dir(dir=src/observer, include_hidden=false)");
        assert_eq!(rewritten.name, "read_file");
        assert_eq!(coerced, "read_file(path=src/observer/repo_rules.rs)");
    }

    #[test]
    fn coerce_fix_existing_tool_call_prefers_module_candidate_from_search_hit() {
        let messages = vec![
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_read_lib",
                    "type": "function",
                    "function": {"name":"read_file","arguments":"{\"path\":\"src/lib.rs\"}"}
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_read_lib",
                "content": "[src/lib.rs] (1 line, 18 bytes)\npub mod observer;"
            }),
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_search_repo_rules",
                    "type": "function",
                    "function": {"name":"search_files","arguments":"{\"pattern\":\"repo_rules\",\"dir\":\"\"}"}
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_search_repo_rules",
                "content": "[search_files: 'repo_rules' — 1 match(es)]\nsrc/observer/mod.rs:1: pub mod repo_rules;"
            }),
        ];
        let tc = ToolCallData {
            id: "call_repeat_search".to_string(),
            name: "search_files".to_string(),
            arguments: json!({"pattern":"repo_rules","dir":""}).to_string(),
        };

        let (rewritten, original, coerced) = coerce_fix_existing_tool_call(
            TaskHarness {
                lane: TaskLane::FixExisting,
                artifact_mode: ArtifactMode::ExistingFiles,
            },
            &messages,
            &tc,
            Some("cargo test 2>&1"),
            None,
        )
        .expect("fix-existing search-derived coercion");

        assert_eq!(original, "search_files(pattern=repo_rules)");
        assert_eq!(rewritten.name, "read_file");
        assert_eq!(coerced, "read_file(path=src/observer/repo_rules.rs)");
    }

    #[test]
    fn coerce_fix_existing_blocked_mutation_tool_call_restores_prompt_literal_patch() {
        let messages = vec![
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_read_rules",
                    "type": "function",
                    "function": {"name":"read_file","arguments":"{\"path\":\"src/observer/repo_rules.rs\"}"}
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_read_rules",
                "content": "[src/observer/repo_rules.rs] (10 lines, 200 bytes)\nconst TUI_REPLAY_PATHS: &[&str] = &[\n    \"src/tui/events.rs\",\n    \"src/tui/app.rs\",\n    \"src/tui/prefs.rs\",\n    \"src/tui/ui.rs\",\n    \"src/tui/suggestion.rs\",\n];\n"
            }),
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_patch_rules",
                    "type": "function",
                    "function": {
                        "name":"patch_file",
                        "arguments":"{\"path\":\"src/observer/repo_rules.rs\",\"search\":\"const TUI_REPLAY_PATHS: &["
                    }
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_patch_rules",
                "content": "GOVERNOR BLOCKED\n\n[Evidence Gate]\nTarget path: <path>\n\ntool:\npatch_file\narguments:\n{\"path\":\"src/observer/repo_rules.rs\",\"search\":\"const TUI_REPLAY_PATHS: &["
            }),
        ];
        let tc = ToolCallData {
            id: "call_search_again".to_string(),
            name: "search_files".to_string(),
            arguments: json!({"pattern":"review_panel.rs"}).to_string(),
        };

        let (rewritten, original, coerced) = coerce_fix_existing_blocked_mutation_tool_call(
            TaskHarness {
                lane: TaskLane::FixExisting,
                artifact_mode: ArtifactMode::ExistingFiles,
            },
            &messages,
            &tc,
            "Fix the existing observer rule module so `src/tui/review_panel.rs` is treated as replay-sensitive.",
        )
        .expect("blocked mutation coercion");

        assert_eq!(original, "search_files(pattern=review_panel.rs)");
        assert_eq!(rewritten.name, "patch_file");
        assert!(rewritten
            .arguments
            .contains("\"path\":\"src/observer/repo_rules.rs\""));
        assert!(rewritten.arguments.contains("src/tui/review_panel.rs"));
        assert!(coerced.starts_with("patch_file("));
    }

    #[test]
    fn repair_fix_existing_mutation_tool_call_repairs_truncated_patch_before_execution() {
        let messages = vec![
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_read_rules",
                    "type": "function",
                    "function": {"name":"read_file","arguments":"{\"path\":\"src/observer/repo_rules.rs\"}"}
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_read_rules",
                "content": "[src/observer/repo_rules.rs] (10 lines, 200 bytes)\nconst TUI_REPLAY_PATHS: &[&str] = &[\n    \"src/tui/events.rs\",\n    \"src/tui/app.rs\",\n    \"src/tui/prefs.rs\",\n    \"src/tui/ui.rs\",\n    \"src/tui/suggestion.rs\",\n];\n"
            }),
        ];
        let tc = ToolCallData {
            id: "call_patch".to_string(),
            name: "patch_file".to_string(),
            arguments: "{\"path\":\"src/observer/repo_rules.rs\",\"search\":\"const TUI_REPLAY_PATHS: &[&str] = &[\\n    \\\"src/tui/events.rs\\\",\\n    \\\"src/tui/app.rs\\\",\\n    \\\"src/tui/prefs".to_string(),
        };

        let (rewritten, _original, coerced) = repair_fix_existing_mutation_tool_call(
            TaskHarness {
                lane: TaskLane::FixExisting,
                artifact_mode: ArtifactMode::ExistingFiles,
            },
            &messages,
            &tc,
            "Fix the existing observer rule module so `src/tui/review_panel.rs` is treated as replay-sensitive.",
        )
        .expect("repair malformed patch");

        assert_eq!(rewritten.name, "patch_file");
        assert!(rewritten
            .arguments
            .contains("\"path\":\"src/observer/repo_rules.rs\""));
        assert!(rewritten.arguments.contains("src/tui/review_panel.rs"));
        assert!(coerced.starts_with("patch_file("));
    }

    #[test]
    fn coerce_fix_existing_literal_mutation_tool_call_synthesizes_patch_after_impl_read() {
        let messages = vec![
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_read_lib",
                    "type": "function",
                    "function": {"name":"read_file","arguments":"{\"path\":\"src/lib.rs\"}"}
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_read_lib",
                "content": "[src/lib.rs] (1 line, 18 bytes)\npub mod observer;"
            }),
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_search_repo_rules",
                    "type": "function",
                    "function": {"name":"search_files","arguments":"{\"pattern\":\"repo_rules\",\"dir\":\"\"}"}
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_search_repo_rules",
                "content": "[search_files: 'repo_rules' — 1 match(es)]\nsrc/observer/mod.rs:1: pub mod repo_rules;"
            }),
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_read_rules",
                    "type": "function",
                    "function": {"name":"read_file","arguments":"{\"path\":\"src/observer/repo_rules.rs\"}"}
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_read_rules",
                "content": "[src/observer/repo_rules.rs] (10 lines, 200 bytes)\nconst TUI_REPLAY_PATHS: &[&str] = &[\n    \"src/tui/events.rs\",\n    \"src/tui/app.rs\",\n    \"src/tui/prefs.rs\",\n    \"src/tui/ui.rs\",\n    \"src/tui/suggestion.rs\",\n];\n"
            }),
        ];
        let tc = ToolCallData {
            id: "call_search_again".to_string(),
            name: "search_files".to_string(),
            arguments: json!({"pattern":"review_panel.rs"}).to_string(),
        };

        let (rewritten, original, coerced) = coerce_fix_existing_literal_mutation_tool_call(
            TaskHarness {
                lane: TaskLane::FixExisting,
                artifact_mode: ArtifactMode::ExistingFiles,
            },
            &messages,
            &tc,
            "Fix the existing observer rule module so `src/tui/review_panel.rs` is treated as replay-sensitive, using the smallest safe code change.",
        )
        .expect("literal mutation coercion");

        assert_eq!(original, "search_files(pattern=review_panel.rs)");
        assert_eq!(rewritten.name, "patch_file");
        assert!(rewritten
            .arguments
            .contains("\"path\":\"src/observer/repo_rules.rs\""));
        assert!(rewritten.arguments.contains("src/tui/review_panel.rs"));
        assert!(coerced.starts_with("patch_file("));
    }

    #[test]
    fn coerce_fix_existing_literal_mutation_tool_call_ignores_test_only_literal_match() {
        let messages = vec![
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_read_rules",
                    "type": "function",
                    "function": {"name":"read_file","arguments":"{\"path\":\"src/observer/repo_rules.rs\"}"}
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_read_rules",
                "content": "[src/observer/repo_rules.rs] (27 lines, 629 bytes)\nconst TUI_REPLAY_PATHS: &[&str] = &[\n    \"src/tui/events.rs\",\n    \"src/tui/app.rs\",\n    \"src/tui/prefs.rs\",\n    \"src/tui/ui.rs\",\n    \"src/tui/suggestion.rs\",\n];\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn review_panel_is_replay_sensitive() {\n        assert!(requires_tui_replay(\"src/tui/review_panel.rs\"));\n    }\n}\n"
            }),
        ];
        let tc = ToolCallData {
            id: "call_search_again".to_string(),
            name: "search_files".to_string(),
            arguments: json!({"pattern":"review_panel.rs"}).to_string(),
        };

        let (rewritten, _original, coerced) = coerce_fix_existing_literal_mutation_tool_call(
            TaskHarness {
                lane: TaskLane::FixExisting,
                artifact_mode: ArtifactMode::ExistingFiles,
            },
            &messages,
            &tc,
            "Fix the existing observer rule module so `src/tui/review_panel.rs` is treated as replay-sensitive.",
        )
        .expect("literal mutation despite test reference");

        assert_eq!(rewritten.name, "patch_file");
        assert!(rewritten
            .arguments
            .contains("\"path\":\"src/observer/repo_rules.rs\""));
        assert!(rewritten.arguments.contains("src/tui/review_panel.rs"));
        assert!(coerced.starts_with("patch_file("));
    }

    #[test]
    fn fix_existing_target_hint_includes_impl_function() {
        let messages = vec![
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_read_main",
                    "type": "function",
                    "function": {"name":"read_file","arguments":"{\"path\":\"src/main.rs\"}"}
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_read_main",
                "content": "[src/main.rs] (22 lines, 400 bytes)\nmod robot;\n\nfn main() {\n    println!(\"{}\", Robot::demo().status());\n}\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn turning_left_from_north_points_west() {\n        let mut robot = Robot::demo();\n        robot.turn_left();\n    }\n}"
            }),
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_read_robot",
                    "type": "function",
                    "function": {"name":"read_file","arguments":"{\"path\":\"src/robot.rs\"}"}
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_read_robot",
                "content": "[src/robot.rs] (40 lines, 800 bytes)\npub fn turn_left(&mut self) {\n    self.heading = match self.heading {\n        Heading::North => Heading::East,\n        Heading::East => Heading::North,\n        Heading::South => Heading::East,\n        Heading::West => Heading::South,\n    };\n}"
            }),
        ];

        assert_eq!(
            fix_existing_target_hint(&messages).as_deref(),
            Some("src/robot.rs::turn_left")
        );
    }

    #[test]
    fn coerce_artifact_creation_tool_call_restores_blocked_write_file() {
        let messages = vec![
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_write",
                    "type": "function",
                    "function": {
                        "name":"write_file",
                        "arguments":"{\"path\":\"notes/todo.txt\",\"content\":\"ship it\\n\"}"
                    }
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_write",
                "content": "GOVERNOR BLOCKED\n\n[Plan Gate] Missing valid <plan>.\n\ntool:\nwrite_file\narguments:\n{\"path\":\"notes/todo.txt\",\"content\":\"ship it\\n\"}"
            }),
        ];
        let tc = ToolCallData {
            id: "call_list".to_string(),
            name: "list_dir".to_string(),
            arguments: json!({"dir":".","include_hidden":true}).to_string(),
        };

        let (rewritten, original, coerced) = coerce_artifact_creation_tool_call(
            TaskHarness {
                lane: TaskLane::CreateFile,
                artifact_mode: ArtifactMode::NewFiles,
            },
            &messages,
            &tc,
        )
        .expect("rewritten");

        assert_eq!(original, "list_dir(dir=., include_hidden=true)");
        assert_eq!(rewritten.name, "write_file");
        assert!(rewritten.arguments.contains("notes/todo.txt"));
        assert!(coerced.contains("write_file"));
    }

    #[test]
    fn allows_artifact_creation_during_diagnose_for_new_file_write() {
        let tc = ToolCallData {
            id: "call_write".to_string(),
            name: "write_file".to_string(),
            arguments: json!({"path":"notes/todo.txt","content":"ship it\n"}).to_string(),
        };
        assert!(allows_artifact_creation_during_diagnose(
            TaskHarness {
                lane: TaskLane::CreateFile,
                artifact_mode: ArtifactMode::NewFiles,
            },
            &tc,
        ));
    }

    #[test]
    fn allows_artifact_creation_during_verify_for_new_repo_write() {
        let tc = ToolCallData {
            id: "call_write".to_string(),
            name: "write_file".to_string(),
            arguments: json!({"path":"demo_repo/.gitignore","content":"target/\n"}).to_string(),
        };
        assert!(allows_artifact_creation_during_verify(
            TaskHarness {
                lane: TaskLane::ScaffoldRepo,
                artifact_mode: ArtifactMode::NewRepo,
            },
            &tc,
        ));
    }

    #[test]
    fn coerce_repo_goal_completion_tool_call_rewrites_to_git_init() {
        let messages = vec![json!({
            "role": "user",
            "content": "[goal_check]\nThe task is NOT complete yet.\nMissing: .git\nFix it by using exec/write_file. Do NOT stop until the goals are satisfied."
        })];
        let tc = ToolCallData {
            id: "call_list".to_string(),
            name: "list_dir".to_string(),
            arguments: json!({"dir":"demo_repo","include_hidden":true}).to_string(),
        };

        let (rewritten, original, coerced) = coerce_repo_goal_completion_tool_call(
            TaskHarness {
                lane: TaskLane::ScaffoldRepo,
                artifact_mode: ArtifactMode::NewRepo,
            },
            &messages,
            &tc,
            Some("test -d demo_repo/.git && test -f demo_repo/README.md && test -f demo_repo/.gitignore"),
            None,
        )
        .expect("rewritten");

        assert_eq!(rewritten.name, "exec");
        assert!(rewritten.arguments.contains("git init demo_repo"));
        assert!(original.contains("list_dir"));
        assert_eq!(coerced, "git init demo_repo");
    }

    #[test]
    fn coerce_repo_goal_completion_tool_call_advances_to_gitignore_after_git_init() {
        let messages = vec![
            json!({
                "role": "user",
                "content": "[goal_check]\nThe task is NOT complete yet.\nMissing: .git\nFix it by using exec/write_file. Do NOT stop until the goals are satisfied."
            }),
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_init",
                    "type": "function",
                    "function": {
                        "name":"exec",
                        "arguments":"{\"command\":\"git init demo_repo\"}"
                    }
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_init",
                "content": "OK (exit_code: 0)\nInitialized empty Git repository in demo_repo/.git/"
            }),
        ];
        let tc = ToolCallData {
            id: "call_list".to_string(),
            name: "list_dir".to_string(),
            arguments: json!({"dir":"demo_repo","include_hidden":true}).to_string(),
        };

        let (rewritten, _original, coerced) = coerce_repo_goal_completion_tool_call(
            TaskHarness {
                lane: TaskLane::ScaffoldRepo,
                artifact_mode: ArtifactMode::NewRepo,
            },
            &messages,
            &tc,
            Some("test -d demo_repo/.git && test -f demo_repo/README.md && test -f demo_repo/.gitignore"),
            None,
        )
        .expect("rewritten");

        assert_eq!(rewritten.name, "write_file");
        assert!(rewritten.arguments.contains("demo_repo/.gitignore"));
        assert!(coerced.contains(".gitignore"));
    }

    #[test]
    fn coerce_repo_goal_completion_tool_call_prefers_filesystem_progress_over_stale_goal_check() {
        let dir = tempdir().unwrap();
        let repo = dir.path().join("maze_game");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        std::fs::write(repo.join(".gitignore"), "target/\n").unwrap();
        std::fs::write(repo.join("README.md"), "# Maze Game\n").unwrap();

        let messages = vec![json!({
            "role": "user",
            "content": "[goal_check]\nThe task is NOT complete yet.\nMissing: .git\nFix it by using exec/write_file. Do NOT stop until the goals are satisfied."
        })];
        let tc = ToolCallData {
            id: "call_list".to_string(),
            name: "list_dir".to_string(),
            arguments: json!({"dir":"maze_game","include_hidden":true}).to_string(),
        };

        let rewritten = coerce_repo_goal_completion_tool_call(
            TaskHarness {
                lane: TaskLane::ScaffoldRepo,
                artifact_mode: ArtifactMode::NewRepo,
            },
            &messages,
            &tc,
            Some(
                "test -d maze_game/.git && test -f maze_game/README.md && test -f maze_game/.gitignore",
            ),
            Some(dir.path().to_str().unwrap()),
        );

        assert!(rewritten.is_none());
    }

    #[test]
    fn coerce_repo_goal_completion_tool_call_advances_to_missing_required_file_after_binary_crate()
    {
        let dir = tempdir().unwrap();
        let repo = dir.path().join("maze_game");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        std::fs::create_dir_all(repo.join("src")).unwrap();
        std::fs::write(repo.join(".gitignore"), "target/\n").unwrap();
        std::fs::write(repo.join("Cargo.toml"), "[package]\nname = \"maze_game\"\n").unwrap();
        std::fs::write(repo.join("README.md"), "# Maze Game\n").unwrap();
        std::fs::write(repo.join("src/main.rs"), "fn main() {}\n").unwrap();

        let messages = vec![json!({
            "role": "user",
            "content": "[goal_check]\nThe task is NOT complete yet.\nMissing: .git\nFix it by using exec/write_file. Do NOT stop until the goals are satisfied."
        })];
        let tc = ToolCallData {
            id: "call_list".to_string(),
            name: "list_dir".to_string(),
            arguments: json!({"dir":"maze_game","include_hidden":true}).to_string(),
        };

        let (rewritten, _original, coerced) = coerce_repo_goal_completion_tool_call(
            TaskHarness {
                lane: TaskLane::ScaffoldRepo,
                artifact_mode: ArtifactMode::NewRepo,
            },
            &messages,
            &tc,
            Some(
                "test -d maze_game/.git && test -f maze_game/README.md && test -f maze_game/Cargo.toml && test -f maze_game/src/lib.rs && test -f maze_game/src/main.rs && cd maze_game && cargo test 2>&1",
            ),
            Some(dir.path().to_str().unwrap()),
        )
        .expect("rewritten");

        assert_eq!(rewritten.name, "write_file");
        assert!(rewritten.arguments.contains("maze_game/src/lib.rs"));
        assert!(coerced.contains("maze_game/src/lib.rs"));
    }

    #[test]
    fn repair_repo_scaffold_write_tool_call_rewrites_truncated_git_path_to_init() {
        let messages = vec![
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_write_readme",
                    "type": "function",
                    "function": {
                        "name":"write_file",
                        "arguments":"{\"path\":\"demo_repo/README.md\",\"content\":\"# demo_repo\\n\"}"
                    }
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_write_readme",
                "content": "OK: wrote 'demo_repo/README.md' (1 lines, 12 bytes)"
            }),
        ];
        let tc = ToolCallData {
            id: "call_bad".to_string(),
            name: "write_file".to_string(),
            arguments: "{\"path\":\"demo_repo/.git".to_string(),
        };

        let (rewritten, _original, coerced) = repair_repo_scaffold_write_tool_call(
            TaskHarness {
                lane: TaskLane::ScaffoldRepo,
                artifact_mode: ArtifactMode::NewRepo,
            },
            &messages,
            &tc,
            Some("test -d demo_repo/.git && test -f demo_repo/README.md && test -f demo_repo/.gitignore"),
            None,
        )
        .expect("rewritten");

        assert_eq!(rewritten.name, "exec");
        assert!(rewritten.arguments.contains("git init demo_repo"));
        assert_eq!(coerced, "git init demo_repo");
    }

    #[test]
    fn repair_repo_scaffold_write_tool_call_rewrites_to_gitignore_after_git_init() {
        let messages = vec![
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_init",
                    "type": "function",
                    "function": {
                        "name":"exec",
                        "arguments":"{\"command\":\"git init demo_repo\"}"
                    }
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_init",
                "content": "OK (exit_code: 0)\nInitialized empty Git repository in demo_repo/.git/"
            }),
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_write_readme",
                    "type": "function",
                    "function": {
                        "name":"write_file",
                        "arguments":"{\"path\":\"demo_repo/README.md\",\"content\":\"# demo_repo\\n\"}"
                    }
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_write_readme",
                "content": "OK: wrote 'demo_repo/README.md' (1 lines, 12 bytes)"
            }),
        ];
        let tc = ToolCallData {
            id: "call_bad".to_string(),
            name: "write_file".to_string(),
            arguments: json!({"path":"demo_repo/.git","content":"oops"}).to_string(),
        };

        let (rewritten, _original, coerced) = repair_repo_scaffold_write_tool_call(
            TaskHarness {
                lane: TaskLane::ScaffoldRepo,
                artifact_mode: ArtifactMode::NewRepo,
            },
            &messages,
            &tc,
            Some("test -d demo_repo/.git && test -f demo_repo/README.md && test -f demo_repo/.gitignore"),
            None,
        )
        .expect("rewritten");

        assert_eq!(rewritten.name, "write_file");
        assert!(rewritten.arguments.contains("demo_repo/.gitignore"));
        assert!(coerced.contains("write_file"));
        assert!(coerced.contains(".gitignore"));
    }

    #[test]
    fn repair_repo_scaffold_write_tool_call_rewrites_to_readme_after_gitignore() {
        let messages = vec![
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_init",
                    "type": "function",
                    "function": {
                        "name":"exec",
                        "arguments":"{\"command\":\"git init demo_repo\"}"
                    }
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_init",
                "content": "OK (exit_code: 0)\nInitialized empty Git repository in demo_repo/.git/"
            }),
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_write_gitignore",
                    "type": "function",
                    "function": {
                        "name":"write_file",
                        "arguments":"{\"path\":\"demo_repo/.gitignore\",\"content\":\"node_modules/\\n\"}"
                    }
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_write_gitignore",
                "content": "OK: wrote 'demo_repo/.gitignore' (1 lines, 14 bytes)"
            }),
        ];
        let tc = ToolCallData {
            id: "call_bad".to_string(),
            name: "write_file".to_string(),
            arguments: "{\"path\":\"demo_repo/.git".to_string(),
        };

        let (rewritten, _original, coerced) = repair_repo_scaffold_write_tool_call(
            TaskHarness {
                lane: TaskLane::ScaffoldRepo,
                artifact_mode: ArtifactMode::NewRepo,
            },
            &messages,
            &tc,
            Some("test -d demo_repo/.git && test -f demo_repo/README.md && test -f demo_repo/.gitignore"),
            None,
        )
        .expect("rewritten");

        assert_eq!(rewritten.name, "write_file");
        assert!(rewritten.arguments.contains("demo_repo/README.md"));
        assert!(rewritten.arguments.contains("# demo_repo\\n"));
        assert!(coerced.contains("README.md"));
    }
}
