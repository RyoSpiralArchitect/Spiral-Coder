use super::task_harness::{fix_existing_target_hint, ArtifactMode, TaskHarness};
use super::*;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

use super::repo_scaffold::{
    default_repo_gitignore, repo_root_from_test_cmd, required_repo_files_from_test_cmd,
    resolve_repo_file_path, resolve_repo_scaffold_path, scaffold_repo_file_content,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FailurePattern {
    RepeatedObservationLoop,
    RepoScaffoldDrift,
}

impl FailurePattern {
    pub(super) fn label(self) -> &'static str {
        match self {
            FailurePattern::RepeatedObservationLoop => "repeated_observation_loop",
            FailurePattern::RepoScaffoldDrift => "repo_scaffold_drift",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PolicyAction {
    MutateExistingNow,
    CreateArtifactNow,
    AdvanceRepoScaffold,
}

impl PolicyAction {
    pub(super) fn label(self) -> &'static str {
        match self {
            PolicyAction::MutateExistingNow => "mutate_existing_now",
            PolicyAction::CreateArtifactNow => "create_artifact_now",
            PolicyAction::AdvanceRepoScaffold => "advance_repo_scaffold",
        }
    }

    pub(super) fn required_action_hint(self) -> &'static str {
        match self {
            PolicyAction::MutateExistingNow => {
                "Apply the smallest edit now with `patch_file` or `apply_diff`."
            }
            PolicyAction::CreateArtifactNow => {
                "Create the requested artifact now with `write_file` or a minimal `exec`."
            }
            PolicyAction::AdvanceRepoScaffold => {
                "Advance the repo scaffold to the next missing artifact instead of observing again."
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PolicyDelta {
    pub pattern: FailurePattern,
    pub action: PolicyAction,
    pub evidence_count: usize,
    pub attempted_command: Option<String>,
    pub next_target: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct MetaHarness {
    policy: Option<PolicyDelta>,
}

impl MetaHarness {
    pub(super) fn analyze(
        task_harness: TaskHarness,
        messages: &[Value],
        recovery_stage: Option<RecoveryStage>,
        test_cmd: Option<&str>,
        tool_root: Option<&str>,
    ) -> Self {
        if let Some(policy) =
            detect_repo_scaffold_drift(task_harness, messages, test_cmd, tool_root)
        {
            return Self {
                policy: Some(policy),
            };
        }
        if let Some(policy) =
            detect_repeated_observation_loop(task_harness, messages, recovery_stage, test_cmd)
        {
            return Self {
                policy: Some(policy),
            };
        }
        Self::default()
    }

    pub(super) fn telemetry_payload(&self) -> Option<Value> {
        let policy = self.policy.as_ref()?;
        Some(serde_json::json!({
            "pattern": policy.pattern.label(),
            "action": policy.action.label(),
            "evidence_count": policy.evidence_count,
            "attempted_command": policy.attempted_command,
            "next_target": policy.next_target,
        }))
    }

    pub(super) fn prompt(&self) -> Option<String> {
        let policy = self.policy.as_ref()?;
        let attempted = policy
            .attempted_command
            .as_deref()
            .map(|command| {
                format!(
                    "Attempted loop action: {}\n",
                    compact_one_line(command, 180)
                )
            })
            .unwrap_or_default();
        let target = policy
            .next_target
            .as_deref()
            .map(|target| format!("Next target artifact: {}\n", compact_one_line(target, 180)))
            .unwrap_or_default();
        Some(format!(
            "[Meta Harness]\n\
pattern: {}\n\
policy: {}\n\
evidence_count: {}\n\
{}{}\
Required now: {}\n\
Treat this as a runtime-updated rule derived from the current trace, not a suggestion.\n\
Do NOT spend the next turn on another same-target observation unless new tool output contradicts the current state.",
            policy.pattern.label(),
            policy.action.label(),
            policy.evidence_count,
            attempted,
            target,
            policy.action.required_action_hint(),
        ))
    }

    pub(super) fn compact_prompt(&self) -> Option<String> {
        let policy = self.policy.as_ref()?;
        Some(
            [
                "[Meta Harness cache]".to_string(),
                format!("- pattern: {}", policy.pattern.label()),
                format!("- policy: {}", policy.action.label()),
                format!("- evidence_count: {}", policy.evidence_count),
                format!(
                    "- attempted: {}",
                    policy
                        .attempted_command
                        .as_deref()
                        .map(|command| compact_one_line(command, 100))
                        .unwrap_or_else(|| "-".to_string())
                ),
                format!(
                    "- next_target: {}",
                    policy
                        .next_target
                        .as_deref()
                        .map(|target| compact_one_line(target, 100))
                        .unwrap_or_else(|| "-".to_string())
                ),
            ]
            .join("\n"),
        )
    }

    pub(super) fn synthesize_tool_call(&self, iter: usize) -> Option<ToolCallData> {
        let policy = self.policy.as_ref()?;
        if policy.action != PolicyAction::AdvanceRepoScaffold {
            return None;
        }
        let target = policy.next_target.as_deref()?.trim();
        let id = format!("meta_harness_call_{iter}");
        if let Some(repo_root) = target.strip_suffix("/.git") {
            return Some(ToolCallData {
                thought_signature: None,
                id,
                name: "exec".to_string(),
                arguments: serde_json::json!({
                    "command": format!("git init {repo_root}")
                })
                .to_string(),
            });
        }
        if target.ends_with("/.gitignore") {
            return Some(ToolCallData {
                thought_signature: None,
                id,
                name: "write_file".to_string(),
                arguments: serde_json::json!({
                    "path": target,
                    "content": default_repo_gitignore(),
                })
                .to_string(),
            });
        }
        Some(ToolCallData {
            thought_signature: None,
            id,
            name: "write_file".to_string(),
            arguments: serde_json::json!({
                "path": target,
                "content": scaffold_repo_file_content(target),
            })
            .to_string(),
        })
    }

    pub(super) fn policy(&self) -> Option<&PolicyDelta> {
        self.policy.as_ref()
    }

    #[cfg(test)]
    pub(super) fn for_test(policy: PolicyDelta) -> Self {
        Self {
            policy: Some(policy),
        }
    }
}

#[derive(Debug, Clone)]
struct PendingToolIntent {
    name: String,
    command: Option<String>,
    signature: Option<String>,
    path: Option<String>,
}

#[derive(Debug, Default)]
struct ObservationWindow {
    total_successes_since_mutation: usize,
    repeated_last_command: usize,
    last_command: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct RepoScaffoldProgress {
    repo_root: String,
    has_git: bool,
    required_files: Vec<String>,
    present_files: BTreeSet<String>,
}

fn detect_repeated_observation_loop(
    task_harness: TaskHarness,
    messages: &[Value],
    recovery_stage: Option<RecoveryStage>,
    test_cmd: Option<&str>,
) -> Option<PolicyDelta> {
    if task_harness.artifact_mode == ArtifactMode::ObserveOnly
        || recovery_stage == Some(RecoveryStage::Verify)
    {
        return None;
    }

    let window = observation_window_since_last_mutation(messages, test_cmd);
    let enough = match task_harness.artifact_mode {
        ArtifactMode::ExistingFiles => {
            window.repeated_last_command >= 2 || window.total_successes_since_mutation >= 3
        }
        ArtifactMode::NewFiles | ArtifactMode::NewRepo => {
            window.total_successes_since_mutation >= 2
        }
        ArtifactMode::ObserveOnly => false,
    };
    if !enough {
        return None;
    }

    let action = match task_harness.artifact_mode {
        ArtifactMode::ExistingFiles => PolicyAction::MutateExistingNow,
        ArtifactMode::NewFiles | ArtifactMode::NewRepo => PolicyAction::CreateArtifactNow,
        ArtifactMode::ObserveOnly => return None,
    };

    Some(PolicyDelta {
        pattern: FailurePattern::RepeatedObservationLoop,
        action,
        evidence_count: window.total_successes_since_mutation,
        attempted_command: window.last_command,
        next_target: match task_harness.artifact_mode {
            ArtifactMode::ExistingFiles => fix_existing_target_hint(messages),
            _ => None,
        },
    })
}

fn detect_repo_scaffold_drift(
    task_harness: TaskHarness,
    messages: &[Value],
    test_cmd: Option<&str>,
    tool_root: Option<&str>,
) -> Option<PolicyDelta> {
    if task_harness.artifact_mode != ArtifactMode::NewRepo {
        return None;
    }

    let progress = repo_scaffold_progress(messages, test_cmd?, tool_root)?;
    let missing_git_goal = latest_goal_check_missing(messages)
        .iter()
        .any(|entry| entry == ".git" || entry.ends_with("/.git"));

    let next_target = if !progress.has_git {
        if missing_git_goal || !progress.present_files.is_empty() {
            Some(format!("{}/.git", progress.repo_root))
        } else {
            None
        }
    } else {
        progress
            .required_files
            .iter()
            .find(|path| !progress.present_files.contains(*path))
            .cloned()
    }?;

    Some(PolicyDelta {
        pattern: FailurePattern::RepoScaffoldDrift,
        action: PolicyAction::AdvanceRepoScaffold,
        evidence_count: progress.present_files.len() + usize::from(progress.has_git),
        attempted_command: None,
        next_target: Some(next_target),
    })
}

fn observation_window_since_last_mutation(
    messages: &[Value],
    test_cmd: Option<&str>,
) -> ObservationWindow {
    let mut pending: BTreeMap<String, PendingToolIntent> = BTreeMap::new();
    let mut out = ObservationWindow::default();

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
                        .trim()
                        .to_string();
                    let arguments = function
                        .get("arguments")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .trim()
                        .to_string();
                    let command = if name == "exec" {
                        parse_exec_command_from_args(arguments.as_str())
                    } else {
                        None
                    };
                    let signature =
                        canonicalize_tool_call_command(name.as_str(), arguments.as_str());
                    let path = serde_json::from_str::<Value>(arguments.as_str())
                        .ok()
                        .and_then(|value| {
                            value
                                .get("path")
                                .or_else(|| value.get("dir"))
                                .and_then(|v| v.as_str())
                                .map(str::to_string)
                        });
                    pending.insert(
                        id.to_string(),
                        PendingToolIntent {
                            name,
                            command,
                            signature,
                            path,
                        },
                    );
                }
            }
            "tool" => {
                let id = msg
                    .get("tool_call_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim();
                let Some(intent) = pending.remove(id) else {
                    continue;
                };
                let content = msg.get("content").and_then(|v| v.as_str()).unwrap_or("");

                if successful_mutation(&intent, content, test_cmd) {
                    out = ObservationWindow::default();
                    continue;
                }

                if !successful_observation(&intent, content) {
                    continue;
                }

                let command = intent
                    .signature
                    .unwrap_or_else(|| intent.path.unwrap_or_else(|| intent.name.clone()));
                out.total_successes_since_mutation += 1;
                if out.last_command.as_deref() == Some(command.as_str()) {
                    out.repeated_last_command += 1;
                } else {
                    out.last_command = Some(command);
                    out.repeated_last_command = 1;
                }
            }
            _ => {}
        }
    }

    out
}

fn successful_mutation(intent: &PendingToolIntent, content: &str, test_cmd: Option<&str>) -> bool {
    match intent.name.as_str() {
        "write_file" => content.trim_start().starts_with("OK: wrote '"),
        "patch_file" => content.trim_start().starts_with("OK: patched '"),
        "apply_diff" => content.trim_start().starts_with("OK: applied "),
        "exec" => {
            let Some(command) = intent.command.as_deref() else {
                return false;
            };
            let (exit_code, stdout, stderr) = parse_exec_tool_output_sections(content);
            if exit_code != Some(0) {
                return false;
            }
            if suspicious_success_reason(stdout.as_str(), stderr.as_str()).is_some() {
                return false;
            }
            classify_exec_kind(command, test_cmd) == ExecKind::Action
        }
        _ => false,
    }
}

fn successful_observation(intent: &PendingToolIntent, content: &str) -> bool {
    if !matches!(
        intent.name.as_str(),
        "read_file" | "list_dir" | "glob" | "search_files"
    ) {
        return false;
    }
    let trimmed = content.trim_start();
    trimmed.starts_with('[') && !trimmed.starts_with("[RESULT")
}

fn repo_scaffold_progress(
    messages: &[Value],
    test_cmd: &str,
    tool_root: Option<&str>,
) -> Option<RepoScaffoldProgress> {
    let repo_root = repo_root_from_test_cmd(test_cmd)?;
    let required_files = required_repo_files_from_test_cmd(test_cmd, repo_root.as_str());
    let mut pending: BTreeMap<String, PendingToolIntent> = BTreeMap::new();
    let mut progress = RepoScaffoldProgress {
        repo_root: repo_root.clone(),
        required_files,
        ..RepoScaffoldProgress::default()
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
                        .trim()
                        .to_string();
                    let arguments = function
                        .get("arguments")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .trim()
                        .to_string();
                    let command = if name == "exec" {
                        parse_exec_command_from_args(arguments.as_str())
                    } else {
                        None
                    };
                    let path = serde_json::from_str::<Value>(arguments.as_str())
                        .ok()
                        .and_then(|value| {
                            value
                                .get("path")
                                .and_then(|v| v.as_str())
                                .map(str::to_string)
                        });
                    pending.insert(
                        id.to_string(),
                        PendingToolIntent {
                            name,
                            command,
                            signature: None,
                            path,
                        },
                    );
                }
            }
            "tool" => {
                let id = msg
                    .get("tool_call_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim();
                let Some(intent) = pending.remove(id) else {
                    continue;
                };
                let content = msg.get("content").and_then(|v| v.as_str()).unwrap_or("");
                match intent.name.as_str() {
                    "write_file" if content.trim_start().starts_with("OK: wrote '") => {
                        let Some(path) = intent.path else {
                            continue;
                        };
                        if path.starts_with(&format!("{repo_root}/")) {
                            progress.present_files.insert(path);
                        }
                    }
                    "exec" => {
                        let Some(command) = intent.command else {
                            continue;
                        };
                        let (exit_code, stdout, stderr) = parse_exec_tool_output_sections(content);
                        if exit_code != Some(0)
                            || suspicious_success_reason(stdout.as_str(), stderr.as_str()).is_some()
                        {
                            continue;
                        }
                        if command.trim() == format!("git init {repo_root}") {
                            progress.has_git = true;
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
        progress.has_git |= repo_path.join(".git").exists();
        for required_path in progress.required_files.clone() {
            if resolve_repo_file_path(root, required_path.as_str()).exists() {
                progress.present_files.insert(required_path);
            }
        }
    }

    Some(progress)
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::tempdir;

    #[test]
    fn meta_harness_detects_fix_observation_loop() {
        let messages = vec![
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_read_1",
                    "type": "function",
                    "function": {"name":"read_file","arguments":"{\"path\":\"src/lib.rs\"}"}
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_read_1",
                "content": "[src/lib.rs] (4 lines, 40 bytes)\npub fn add(a: i32, b: i32) -> i32 {"
            }),
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_read_2",
                    "type": "function",
                    "function": {"name":"read_file","arguments":"{\"path\":\"src/lib.rs\"}"}
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_read_2",
                "content": "[src/lib.rs] (4 lines, 40 bytes)\npub fn add(a: i32, b: i32) -> i32 {"
            }),
        ];

        let harness = MetaHarness::analyze(
            TaskHarness {
                lane: super::task_harness::TaskLane::FixExisting,
                artifact_mode: ArtifactMode::ExistingFiles,
            },
            &messages,
            Some(RecoveryStage::Fix),
            Some("cargo test 2>&1"),
            None,
        );

        let prompt = harness.prompt().expect("prompt");
        assert!(prompt.contains("repeated_observation_loop"));
        assert!(prompt.contains("patch_file"));
        assert!(harness.telemetry_payload().expect("telemetry")["action"]
            .as_str()
            .is_some_and(|s| s == "mutate_existing_now"));
    }

    #[test]
    fn meta_harness_detects_repo_scaffold_drift_after_git_init() {
        let messages = vec![
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_init",
                    "type": "function",
                    "function": {"name":"exec","arguments":"{\"command\":\"git init demo_repo\"}"}
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_init",
                "content": "OK (exit_code: 0)\nstdout:\nInitialized empty Git repository in demo_repo/.git/\n"
            }),
        ];

        let harness = MetaHarness::analyze(
            TaskHarness {
                lane: super::task_harness::TaskLane::ScaffoldRepo,
                artifact_mode: ArtifactMode::NewRepo,
            },
            &messages,
            Some(RecoveryStage::Diagnose),
            Some(
                "test -d demo_repo/.git && test -f demo_repo/README.md && test -f demo_repo/.gitignore",
            ),
            None,
        );

        let prompt = harness.prompt().expect("prompt");
        assert!(prompt.contains("repo_scaffold_drift"));
        assert!(prompt.contains("demo_repo/README.md") || prompt.contains("demo_repo/.gitignore"));
        assert!(harness.telemetry_payload().expect("telemetry")["action"]
            .as_str()
            .is_some_and(|s| s == "advance_repo_scaffold"));
    }

    #[test]
    fn meta_harness_synthesizes_repo_scaffold_tool_call() {
        let harness = MetaHarness {
            policy: Some(PolicyDelta {
                pattern: FailurePattern::RepoScaffoldDrift,
                action: PolicyAction::AdvanceRepoScaffold,
                evidence_count: 2,
                attempted_command: None,
                next_target: Some("demo_repo/README.md".to_string()),
            }),
        };

        let tool_call = harness.synthesize_tool_call(7).expect("tool call");

        assert_eq!(tool_call.name, "write_file");
        assert!(tool_call.arguments.contains("demo_repo/README.md"));
        assert!(tool_call.arguments.contains("# demo_repo\\n"));
    }

    #[test]
    fn meta_harness_synthesizes_rust_scaffold_files() {
        let cargo = scaffold_repo_file_content("maze_game/Cargo.toml");
        assert!(cargo.contains("name = \"maze_game\""));

        let lib = scaffold_repo_file_content("maze_game/src/lib.rs");
        assert!(lib.contains("pub struct MazeGame"));
        assert!(lib.contains("fn player_can_reach_exit"));

        let main = scaffold_repo_file_content("maze_game/src/main.rs");
        assert!(main.contains("use maze_game::{MazeGame, Move};"));
    }

    #[test]
    fn meta_harness_detects_missing_lib_after_binary_crate_generation_from_filesystem() {
        let dir = tempdir().unwrap();
        let repo = dir.path().join("maze_game");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        std::fs::create_dir_all(repo.join("src")).unwrap();
        std::fs::write(repo.join(".gitignore"), "target/\n").unwrap();
        std::fs::write(repo.join("Cargo.toml"), "[package]\nname = \"maze_game\"\n").unwrap();
        std::fs::write(repo.join("README.md"), "# maze_game\n").unwrap();
        std::fs::write(repo.join("src/main.rs"), "fn main() {}\n").unwrap();

        let harness = MetaHarness::analyze(
            TaskHarness {
                lane: super::task_harness::TaskLane::ScaffoldRepo,
                artifact_mode: ArtifactMode::NewRepo,
            },
            &[],
            Some(RecoveryStage::Diagnose),
            Some(
                "test -d maze_game/.git && test -f maze_game/README.md && test -f maze_game/Cargo.toml && test -f maze_game/src/lib.rs && test -f maze_game/src/main.rs && cd maze_game && cargo test 2>&1",
            ),
            Some(dir.path().to_str().unwrap()),
        );

        let telemetry = harness.telemetry_payload().expect("telemetry");
        assert_eq!(telemetry["pattern"].as_str(), Some("repo_scaffold_drift"));
        assert_eq!(
            telemetry["next_target"].as_str(),
            Some("maze_game/src/lib.rs")
        );
    }
}
