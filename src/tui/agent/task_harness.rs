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
pub(super) mod benchmark_proof;
#[cfg(test)]
#[path = "benchmark_recovery_tests.rs"]
mod benchmark_recovery_tests;
#[path = "benchmark_replay.rs"]
pub(crate) mod benchmark_replay;
#[cfg(test)]
#[path = "fix_recovery_tests.rs"]
mod fix_recovery_tests;
#[path = "progress_gate.rs"]
mod progress_gate;
pub(super) use progress_gate::build_progress_gate_block;
mod benchmark_plan;
mod fix_existing;
mod scaffold_status;
pub(super) use self::benchmark_plan::*;
pub(super) use self::fix_existing::*;
pub(super) use self::scaffold_status::*;

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
        if super::requirement_proof::declared(root_user_text) {
            return Self {
                lane: TaskLane::FixExisting,
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
For JSON spec files use `patch_file(path, json_pointer=\"/cases/-\", json_value=<JSON-encoded new case>)` to append only the new case. Omit search/replace. The runtime preserves the full document, even beyond read_file truncation. For replacing a member supply its pointer and json_expected. Never rewrite a large document from a truncated read.\n\
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

fn supports_artifact_action(harness: TaskHarness, name: &str) -> bool {
    match harness.artifact_mode {
        ArtifactMode::NewFiles => matches!(name, "write_file" | "exec"),
        ArtifactMode::NewRepo => matches!(name, "write_file" | "exec"),
        ArtifactMode::ObserveOnly | ArtifactMode::ExistingFiles => false,
    }
}

#[cfg(test)]
mod literal_list_recovery_tests;
#[cfg(test)]
mod tests;
