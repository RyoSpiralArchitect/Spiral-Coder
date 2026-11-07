use super::*;
use serde_json::json;

#[test]
fn iteration_cap_final_answer_prefers_read_file_for_repo_map_fallback_task() {
    let plan = synthetic_read_only_observation_plan(
        "Find where coder-side repo-map read_file fallback is wired into the TUI agent flow. Do not edit anything.",
    );
    let messages = vec![
        json!({
            "role": "assistant",
            "tool_calls": [{
                "id": "call_search",
                "type": "function",
                "function": {
                    "name": "search_files",
                    "arguments": "{\"pattern\":\"lazy_read_fallback\",\"dir\":\"src/tui\"}"
                }
            }]
        }),
        json!({
            "role": "tool",
            "tool_call_id": "call_search",
            "content": "[search_files: 'lazy_read_fallback' — 1 match(es)]\nsrc/tui/agent.rs:12577: crate::repo_map::lazy_read_fallback(root, &path)"
        }),
        json!({
            "role": "assistant",
            "tool_calls": [{
                "id": "call_read",
                "type": "function",
                "function": {
                    "name": "read_file",
                    "arguments": "{\"path\":\"tui/agent.rs\"}"
                }
            }]
        }),
        json!({
            "role": "tool",
            "tool_call_id": "call_read",
            "content": "[src/tui/agent.rs] (16610 lines, 712345 bytes)\nif let Some(fallback) = crate::repo_map::lazy_read_fallback(root, &path) {"
        }),
    ];

    let mut evidence = collect_observation_evidence(&messages);
    evidence.remember_resolution("tui/agent.rs", "src/tui/agent.rs", "repo_map:read_file");
    let final_text = build_read_only_iteration_cap_final_answer(
        "Find where coder-side repo-map read_file fallback is wired into the TUI agent flow. Do not edit anything.",
        &plan,
        &evidence,
        &messages,
        &WorkingMemory::default(),
    )
    .expect("final answer");

    assert!(final_text.contains("src/tui/agent.rs") || final_text.contains("tui/agent.rs"));
    assert!(
        final_text.contains("via `read_file(path=src/tui/agent.rs)`")
            || final_text.contains("via `read_file(path=tui/agent.rs)`")
    );
    assert!(!final_text.contains("via `search_files("));
}

#[test]
fn validate_done_acceptance_prefers_read_file_evidence_when_present() {
    let plan = synthetic_read_only_observation_plan(
        "Find where coder-side repo-map read_file fallback is wired into the TUI agent flow. Do not edit anything.",
    );
    let completed_acceptance = vec![
        "1) File path identifying coder-side repo-map read_file fallback wiring".to_string(),
        "2) Confirmed handling context where read_file miss fallback is invoked".to_string(),
    ];
    let acceptance_evidence = vec![
        DoneAcceptanceEvidence {
            criterion: completed_acceptance[0].clone(),
            command: "search_files(dir=src/tui, pattern=lazy_read_fallback)".to_string(),
        },
        DoneAcceptanceEvidence {
            criterion: completed_acceptance[1].clone(),
            command: "read_file(path=src/tui/agent.rs)".to_string(),
        },
    ];
    let known_commands = vec![
        "search_files(dir=src/tui, pattern=lazy_read_fallback)".to_string(),
        "read_file(path=src/tui/agent.rs)".to_string(),
    ];
    let mut evidence = ObservationEvidence::default();
    evidence.remember_read("read_file(path=tui/agent.rs)", "src/tui/agent.rs");
    evidence.remember_resolution("tui/agent.rs", "src/tui/agent.rs", "repo_map:read_file");

    let rows = validate_done_acceptance(
        Some(&plan),
        &completed_acceptance,
        &[],
        &acceptance_evidence,
        &known_commands,
        &evidence,
    )
    .expect("done evidence rows");

    assert_eq!(
        rows,
        vec![
            (
                0,
                "search_files(dir=src/tui, pattern=lazy_read_fallback)".to_string(),
            ),
            (1, "read_file(path=src/tui/agent.rs)".to_string()),
        ]
    );
}

#[test]
fn done_acceptance_recovery_hint_lists_valid_criterion_references() {
    let plan = PlanBlock {
        goal: "Add a replay case".into(),
        steps: vec!["Edit spec".into(), "Verify".into()],
        acceptance_criteria: vec![
            "the requested change is implemented and confirmed by a passing behavioral verification command".into(),
            "the final result cites the exact passing behavioral verification command".into(),
        ],
        risks: "stale proof".into(),
        assumptions: "native evidence".into(),
    };
    let hint = build_done_acceptance_recovery_hint(
        "done.completed_acceptance must reference a real current acceptance criterion",
        Some(&plan),
        &[],
        &["grep -q \"src/tui/review_panel.rs\" .spiral-coder/tui_replay.json".into()],
        &[],
        false,
    );
    assert!(hint.contains("acceptance 1: the requested change is implemented"));
    assert!(hint.contains("acceptance 2: the final result cites"));
    assert!(hint.contains("\"criterion\":\"acceptance 1\""));
    assert!(hint.contains("only criteria with proof"));
    assert!(hint.contains("unproven \"acceptance N\""));
    assert!(!hint.contains("\"criterion\":\"acceptance 2\""));
    assert!(hint.contains("grep -q \"src/tui/review_panel.rs\""));
}

#[test]
fn done_acceptance_recovery_hint_uses_flat_rows_for_flat_schema() {
    let plan = PlanBlock {
        goal: "Add a replay case".into(),
        steps: vec!["Edit spec".into(), "Verify".into()],
        acceptance_criteria: vec!["verified".into()],
        risks: "stale proof".into(),
        assumptions: "native evidence".into(),
    };
    let hint = build_done_acceptance_recovery_hint(
        "done.acceptance_evidence must cover every completed acceptance criterion exactly once",
        Some(&plan),
        &["acceptance 1".to_string()],
        &["cargo test".into()],
        &[],
        true,
    );
    assert!(hint.contains("acceptance 1 | <paste a command"), "{hint}");
    assert!(!hint.contains("\"criterion\":\"acceptance 1\""), "{hint}");
    assert!(hint.contains("\"acceptance 1 | cargo test\""), "{hint}");
}

#[test]
fn done_acceptance_recovery_hint_omits_criteria_list_without_plan() {
    let hint = build_done_acceptance_recovery_hint(
        "done requires a valid current plan",
        None,
        &[],
        &[],
        &[],
        false,
    );
    assert!(!hint.contains("Valid acceptance criteria"));
}

#[test]
fn completion_hint_accepts_single_observed_generic_target_after_read() {
    let plan = synthetic_read_only_observation_plan(
        "Locate where project-local profile aliases are loaded for the greet command. Do not edit anything.",
    );
    let messages = vec![
        json!({
            "role": "assistant",
            "tool_calls": [{
                "id": "call_search",
                "type": "function",
                "function": {
                    "name": "search_files",
                    "arguments": "{\"pattern\":\"aliases\",\"dir\":\"src\"}"
                }
            }]
        }),
        json!({
            "role": "tool",
            "tool_call_id": "call_search",
            "content": "[search_files: 'aliases' — 7 match(es)]\nsrc/config.rs:7:     pub aliases: BTreeMap<String, String>,"
        }),
        json!({
            "role": "assistant",
            "tool_calls": [{
                "id": "call_read",
                "type": "function",
                "function": {
                    "name": "read_file",
                    "arguments": "{\"path\":\"src/config.rs\"}"
                }
            }]
        }),
        json!({
            "role": "tool",
            "tool_call_id": "call_read",
            "content": "[src/config.rs] (39 lines, 1136 bytes)\npub fn resolve_profile_alias(root: &Path, requested: Option<&str>, config: &AppConfig) -> String {"
        }),
    ];
    let evidence = collect_observation_evidence(&messages);

    let hint = build_read_only_completion_hint(
        "Locate where project-local profile aliases are loaded for the greet command. Do not edit anything.",
        &plan,
        &evidence,
        &messages,
        &WorkingMemory::default(),
    )
    .expect("completion hint");

    assert!(hint.contains("call done directly now"));
    assert!(hint.contains("src/config.rs"));
}

#[test]
fn iteration_cap_final_answer_supports_single_observed_generic_target() {
    let plan = synthetic_read_only_observation_plan(
        "Locate where project-local profile aliases are loaded for the greet command. Do not edit anything.",
    );
    let messages = vec![
        json!({
            "role": "assistant",
            "tool_calls": [{
                "id": "call_search",
                "type": "function",
                "function": {
                    "name": "search_files",
                    "arguments": "{\"pattern\":\"aliases\",\"dir\":\"src\"}"
                }
            }]
        }),
        json!({
            "role": "tool",
            "tool_call_id": "call_search",
            "content": "[search_files: 'aliases' — 7 match(es)]\nsrc/config.rs:7:     pub aliases: BTreeMap<String, String>,"
        }),
        json!({
            "role": "assistant",
            "tool_calls": [{
                "id": "call_read",
                "type": "function",
                "function": {
                    "name": "read_file",
                    "arguments": "{\"path\":\"src/config.rs\"}"
                }
            }]
        }),
        json!({
            "role": "tool",
            "tool_call_id": "call_read",
            "content": "[src/config.rs] (39 lines, 1136 bytes)\npub fn resolve_profile_alias(root: &Path, requested: Option<&str>, config: &AppConfig) -> String {"
        }),
    ];
    let evidence = collect_observation_evidence(&messages);

    let final_text = build_read_only_iteration_cap_final_answer(
        "Locate where project-local profile aliases are loaded for the greet command. Do not edit anything.",
        &plan,
        &evidence,
        &messages,
        &WorkingMemory::default(),
    )
    .expect("final answer");

    assert!(final_text.contains("src/config.rs"));
    assert!(final_text.contains("read_file(path=src/config.rs)"));
}

#[test]
fn should_prefer_done_after_verified_action_blocks_post_verify_reads() {
    let plan = PlanBlock {
        goal: "Fix the failing test with the smallest code change.".to_string(),
        steps: vec![
            "inspect the failing code path".to_string(),
            "patch the smallest confirmed bug".to_string(),
            "run cargo test 2>&1".to_string(),
            "call done with verified results".to_string(),
        ],
        acceptance_criteria: vec![
            "the requested change is implemented".to_string(),
            "cargo test 2>&1 passes".to_string(),
        ],
        risks: "wrong file".to_string(),
        assumptions: "cargo test is relevant".to_string(),
    };
    let tc = ToolCallData {
        thought_signature: None,
        id: "call_read".to_string(),
        name: "read_file".to_string(),
        arguments: serde_json::json!({"path":"Cargo.toml"}).to_string(),
    };
    let known_commands = vec!["cargo test 2>&1".to_string()];

    assert!(should_prefer_done_after_verified_action(
        &tc,
        &plan,
        &known_commands,
        VerificationLevel::Behavioral,
        Some("cargo test 2>&1"),
        Some(4),
        Some(5),
    ));
}

#[test]
fn should_prefer_done_after_verified_action_accepts_verified_patch_auto_test() {
    let plan = PlanBlock {
        goal: "Fix the failing test with the smallest code change.".to_string(),
        steps: vec![
            "inspect the failing code path".to_string(),
            "patch the smallest confirmed bug".to_string(),
            "run cargo test 2>&1".to_string(),
            "call done with verified results".to_string(),
        ],
        acceptance_criteria: vec![
            "the requested change is implemented".to_string(),
            "cargo test 2>&1 passes".to_string(),
        ],
        risks: "wrong file".to_string(),
        assumptions: "cargo test is relevant".to_string(),
    };
    let tc = ToolCallData {
        thought_signature: None,
        id: "call_exec".to_string(),
        name: "exec".to_string(),
        arguments: serde_json::json!({"command":"cargo test 2>&1"}).to_string(),
    };
    let known_commands = vec!["cargo test 2>&1".to_string()];

    assert!(should_prefer_done_after_verified_action(
        &tc,
        &plan,
        &known_commands,
        VerificationLevel::Behavioral,
        Some("cargo test 2>&1"),
        Some(4),
        Some(4),
    ));
}

#[test]
fn should_prefer_done_after_verified_action_requires_known_verification_command() {
    let plan = PlanBlock {
        goal: "Fix the failing test with the smallest code change.".to_string(),
        steps: vec![
            "inspect the failing code path".to_string(),
            "patch the smallest confirmed bug".to_string(),
            "run cargo test 2>&1".to_string(),
            "call done with verified results".to_string(),
        ],
        acceptance_criteria: vec![
            "the requested change is implemented".to_string(),
            "cargo test 2>&1 passes".to_string(),
        ],
        risks: "wrong file".to_string(),
        assumptions: "cargo test is relevant".to_string(),
    };
    let tc = ToolCallData {
        thought_signature: None,
        id: "call_read".to_string(),
        name: "read_file".to_string(),
        arguments: serde_json::json!({"path":"Cargo.toml"}).to_string(),
    };

    assert!(!should_prefer_done_after_verified_action(
        &tc,
        &plan,
        &[],
        VerificationLevel::Behavioral,
        Some("cargo test 2>&1"),
        Some(4),
        Some(4),
    ));
}

#[test]
fn post_verify_done_completion_hint_mentions_done_and_verification() {
    let plan = PlanBlock {
        goal: "Fix the failing test with the smallest code change.".to_string(),
        steps: vec![
            "inspect the failing code path".to_string(),
            "patch the smallest confirmed bug".to_string(),
            "run cargo test 2>&1".to_string(),
            "call done with verified results".to_string(),
        ],
        acceptance_criteria: vec![
            "the requested change is implemented".to_string(),
            "cargo test 2>&1 passes".to_string(),
        ],
        risks: "wrong file".to_string(),
        assumptions: "cargo test is relevant".to_string(),
    };
    let tc = ToolCallData {
        thought_signature: None,
        id: "call_read".to_string(),
        name: "read_file".to_string(),
        arguments: serde_json::json!({"path":"Cargo.toml"}).to_string(),
    };
    let hint = build_post_verify_done_completion_hint(
        &plan,
        &["cargo test 2>&1".to_string()],
        &tc,
        VerificationLevel::Behavioral,
        Some("cargo test 2>&1"),
    );

    assert!(hint.contains("[Completion Gate]"));
    assert!(hint.contains("tool: done"));
    assert!(hint.contains("cargo test 2>&1"));
}

#[test]
fn rescue_invalid_done_payload_for_verified_action_repairs_smoke_acceptance() {
    let plan = PlanBlock {
        goal: "Fix the failing test with the smallest code change.".to_string(),
        steps: vec![
            "inspect the failing code path".to_string(),
            "patch the smallest confirmed bug".to_string(),
            "run cargo test 2>&1".to_string(),
            "call done with verified results".to_string(),
        ],
        acceptance_criteria: vec![
            "the requested change is implemented and confirmed by a passing behavioral verification command".to_string(),
            "the final result cites the exact passing behavioral verification command".to_string(),
        ],
        risks: "wrong file".to_string(),
        assumptions: "cargo test is relevant".to_string(),
    };
    let observation_evidence = ObservationEvidence {
        reads: vec![ObservationReadEvidence {
            command: "read_file(path=src/lib.rs)".to_string(),
            path: "src/lib.rs".to_string(),
        }],
        searches: Vec::new(),
        resolutions: Vec::new(),
    };

    let (completed, remaining, evidence) = rescue_invalid_done_payload_for_verified_action(
        Some(&plan),
        &[
            "read_file(path=src/lib.rs)".to_string(),
            "cargo test 2>&1".to_string(),
        ],
        &observation_evidence,
        VerificationLevel::Behavioral,
        Some("cargo test 2>&1"),
        Some(3),
        Some(3),
    )
    .expect("rescued done payload");

    assert_eq!(
        completed,
        vec![
            acceptance_reference_label(&plan, 0),
            acceptance_reference_label(&plan, 1)
        ]
    );
    assert!(remaining.is_empty());
    assert_eq!(evidence.len(), 2);
    assert!(evidence.iter().all(|row| row.command == "cargo test 2>&1"));
}

#[test]
fn synthesize_action_done_summary_prefers_created_file_path() {
    let messages = vec![
        json!({
            "role": "assistant",
            "tool_calls": [{
                "id": "call_write",
                "type": "function",
                "function": {
                    "name":"write_file",
                    "arguments": "{\"path\":\"notes/todo.txt\",\"content\":\"ship it\\n\"}"
                }
            }]
        }),
        json!({
            "role": "tool",
            "tool_call_id": "call_write",
            "content": "OK: wrote 'notes/todo.txt' (1 lines, 8 bytes)\n[auto-test] ✓ PASSED (exit 0)"
        }),
    ];

    let summary = synthesize_action_done_summary(None, &messages).expect("summary");
    assert!(summary.contains("notes/todo.txt"));
    assert!(summary.contains("Created"));
}

#[test]
fn synthesize_action_done_summary_mentions_code_docs_and_replay_followups() {
    let messages = vec![
        json!({
            "role": "assistant",
            "tool_calls": [{
                "id": "call_rules",
                "type": "function",
                "function": {
                    "name":"patch_file",
                    "arguments":"{\"path\":\"src/observer/repo_rules.rs\",\"search\":\"old\",\"replace\":\"new\"}"
                }
            }]
        }),
        json!({
            "role": "tool",
            "tool_call_id": "call_rules",
            "content": "OK: patched 'src/observer/repo_rules.rs' (+1 lines, 10 total)"
        }),
        json!({
            "role": "assistant",
            "tool_calls": [{
                "id": "call_docs",
                "type": "function",
                "function": {
                    "name":"patch_file",
                    "arguments":"{\"path\":\"docs/runtime-architecture.md\",\"search\":\"old\",\"replace\":\"new\"}"
                }
            }]
        }),
        json!({
            "role": "tool",
            "tool_call_id": "call_docs",
            "content": "OK: patched 'docs/runtime-architecture.md' (+1 lines, 12 total)"
        }),
        json!({
            "role": "assistant",
            "tool_calls": [{
                "id": "call_replay",
                "type": "function",
                "function": {
                    "name":"patch_file",
                    "arguments":"{\"path\":\".spiral-coder/tui_replay.json\",\"search\":\"old\",\"replace\":\"new\"}"
                }
            }]
        }),
        json!({
            "role": "tool",
            "tool_call_id": "call_replay",
            "content": "OK: patched '.spiral-coder/tui_replay.json' (+1 lines, 10 total)"
        }),
    ];

    let summary = synthesize_action_done_summary(None, &messages).expect("summary");
    assert!(summary.contains("src/observer/repo_rules.rs"));
    assert!(summary.contains("docs/runtime-architecture.md"));
    assert!(summary.contains(".spiral-coder/tui_replay.json"));
}

#[test]
fn synthesize_action_done_summary_prefers_repo_logic_over_late_gitignore() {
    let messages = vec![
        json!({
            "role": "assistant",
            "tool_calls": [{
                "id": "call_lib",
                "type": "function",
                "function": {
                    "name":"write_file",
                    "arguments":"{\"path\":\"maze_game/src/lib.rs\",\"content\":\"pub fn demo() {}\\n\"}"
                }
            }]
        }),
        json!({
            "role": "tool",
            "tool_call_id": "call_lib",
            "content": "OK: wrote 'maze_game/src/lib.rs' (1 lines, 17 bytes)"
        }),
        json!({
            "role": "assistant",
            "tool_calls": [{
                "id": "call_main",
                "type": "function",
                "function": {
                    "name":"write_file",
                    "arguments":"{\"path\":\"maze_game/src/main.rs\",\"content\":\"fn main() {}\\n\"}"
                }
            }]
        }),
        json!({
            "role": "tool",
            "tool_call_id": "call_main",
            "content": "OK: wrote 'maze_game/src/main.rs' (1 lines, 13 bytes)"
        }),
        json!({
            "role": "assistant",
            "tool_calls": [{
                "id": "call_gitignore",
                "type": "function",
                "function": {
                    "name":"write_file",
                    "arguments":"{\"path\":\"maze_game/.gitignore\",\"content\":\"target/\\n\"}"
                }
            }]
        }),
        json!({
            "role": "tool",
            "tool_call_id": "call_gitignore",
            "content": "OK: wrote 'maze_game/.gitignore' (1 lines, 8 bytes)"
        }),
    ];

    let summary = synthesize_action_done_summary(None, &messages).expect("summary");
    assert!(summary.contains("maze_game/"));
    assert!(summary.contains("maze_game/src/lib.rs"));
    assert!(summary.contains("maze_game/src/main.rs"));
    assert!(!summary.contains(".gitignore"));
}

#[test]
fn synthesize_action_done_summary_mentions_repo_root_and_python_logic() {
    let messages = vec![
        json!({
            "role": "assistant",
            "tool_calls": [{
                "id": "call_readme",
                "type": "function",
                "function": {
                    "name":"write_file",
                    "arguments":"{\"path\":\"maze_game_pygame/README.md\",\"content\":\"# maze\\n\"}"
                }
            }]
        }),
        json!({
            "role": "tool",
            "tool_call_id": "call_readme",
            "content": "OK: wrote 'maze_game_pygame/README.md' (1 lines, 7 bytes)"
        }),
        json!({
            "role": "assistant",
            "tool_calls": [{
                "id": "call_game",
                "type": "function",
                "function": {
                    "name":"write_file",
                    "arguments":"{\"path\":\"maze_game_pygame/game.py\",\"content\":\"class MazeGame: ...\\n\"}"
                }
            }]
        }),
        json!({
            "role": "tool",
            "tool_call_id": "call_game",
            "content": "OK: wrote 'maze_game_pygame/game.py' (1 lines, 20 bytes)"
        }),
        json!({
            "role": "assistant",
            "tool_calls": [{
                "id": "call_main",
                "type": "function",
                "function": {
                    "name":"write_file",
                    "arguments":"{\"path\":\"maze_game_pygame/main.py\",\"content\":\"def main(): ...\\n\"}"
                }
            }]
        }),
        json!({
            "role": "tool",
            "tool_call_id": "call_main",
            "content": "OK: wrote 'maze_game_pygame/main.py' (1 lines, 16 bytes)"
        }),
    ];

    let summary = synthesize_action_done_summary(None, &messages).expect("summary");
    assert!(summary.contains("maze_game_pygame/"));
    assert!(summary.contains("maze_game_pygame/game.py"));
    assert!(summary.contains("maze_game_pygame/main.py"));
}

#[test]
fn verified_action_auto_final_answer_uses_repo_artifact_evidence() {
    let plan = PlanBlock {
        goal: "Create a new git repo in demo_repo with starter files.".to_string(),
        steps: vec![
            "create demo_repo".to_string(),
            "verify repo artifacts".to_string(),
            "call done".to_string(),
        ],
        acceptance_criteria: vec![
            "the requested artifact is created and confirmed by a passing build/check/lint verification command".to_string(),
            "the final result cites the exact passing build/check/lint verification command".to_string(),
        ],
        risks: "wrong path".to_string(),
        assumptions: "the configured verification command is relevant".to_string(),
    };
    let messages = vec![
        json!({
            "role": "assistant",
            "tool_calls": [{
                "id": "call_write",
                "type": "function",
                "function": {
                    "name":"write_file",
                    "arguments":"{\"path\":\"demo_repo/.gitignore\",\"content\":\"target/\\n\"}"
                }
            }]
        }),
        json!({
            "role": "tool",
            "tool_call_id": "call_write",
            "content": "OK: wrote 'demo_repo/.gitignore' (1 lines, 8 bytes)\n[auto-test] ✓ PASSED (exit 0)"
        }),
    ];

    let final_text = maybe_build_verified_action_auto_final_answer(
        Some(&plan),
        &messages,
        &[
            "test -d demo_repo/.git && test -f demo_repo/README.md && test -f demo_repo/.gitignore"
                .to_string(),
        ],
        &ObservationEvidence::default(),
        VerificationLevel::Build,
        Some(
            "test -d demo_repo/.git && test -f demo_repo/README.md && test -f demo_repo/.gitignore",
        ),
        Some(6),
        Some(6),
    )
    .expect("auto final");

    assert!(final_text.contains("demo_repo/.gitignore"));
    assert!(final_text.contains("Acceptance:"));
    assert!(final_text.contains("test -d demo_repo/.git"));
    assert!(final_text.contains("README.md"));
    assert!(final_text.contains("Artifacts:"));
    assert!(final_text.contains("repo: `demo_repo/`"));
}

#[test]
fn verified_action_auto_final_answer_accepts_successful_write_as_artifact_proof() {
    let plan = PlanBlock {
        goal: "Create a pygame maze repo and verify it.".to_string(),
        steps: vec![
            "initialize the repo".to_string(),
            "write the pygame files".to_string(),
            "run the unittest smoke".to_string(),
            "call done".to_string(),
        ],
        acceptance_criteria: vec![
            "maze_game_pygame/README.md explains the controls".to_string(),
            "maze_game_pygame/game.py contains the maze gameplay logic".to_string(),
            "maze_game_pygame/main.py launches the game loop".to_string(),
            "maze_game_pygame/test_game.py passes under SDL_VIDEODRIVER=dummy python3 -m unittest -q 2>&1".to_string(),
        ],
        risks: "wrong repo layout".to_string(),
        assumptions: "pygame is available".to_string(),
    };
    let verify_command = "SDL_VIDEODRIVER=dummy python3 -m unittest -q 2>&1";
    let final_text = maybe_build_verified_action_auto_final_answer(
        Some(&plan),
        &[json!({
            "role": "assistant",
            "tool_calls": [{
                "id": "call_main",
                "type": "function",
                "function": {
                    "name":"write_file",
                    "arguments":"{\"path\":\"maze_game_pygame/main.py\",\"content\":\"print('maze')\\n\"}"
                }
            }]
        }), json!({
            "role": "tool",
            "tool_call_id": "call_main",
            "content": "OK: wrote 'maze_game_pygame/main.py' (1 lines, 13 bytes)\n[auto-test] ✓ PASSED (exit 0)"
        })],
        &[
            verify_command.to_string(),
            "write_file(path=maze_game_pygame/main.py)".to_string(),
        ],
        &ObservationEvidence::default(),
        VerificationLevel::Behavioral,
        Some(verify_command),
        Some(7),
        Some(7),
    )
    .expect("auto final");

    assert!(final_text.contains("maze_game_pygame/main.py"));
    assert!(final_text.contains("SDL_VIDEODRIVER=dummy python3 -m unittest -q 2>&1"));
}

#[test]
fn rescue_invalid_done_payload_for_verified_action_uses_file_specific_artifact_proof() {
    let plan = PlanBlock {
        goal: "Create a pygame maze repo and verify it.".to_string(),
        steps: vec![
            "initialize the repo".to_string(),
            "write the pygame files".to_string(),
            "run the unittest smoke".to_string(),
            "call done".to_string(),
        ],
        acceptance_criteria: vec![
            "maze_game_pygame/game.py contains the maze gameplay logic".to_string(),
            "maze_game_pygame/main.py remains runnable".to_string(),
            "maze_game_pygame/test_game.py passes under SDL_VIDEODRIVER=dummy python3 -m unittest -q 2>&1".to_string(),
        ],
        risks: "wrong repo layout".to_string(),
        assumptions: "pygame is available".to_string(),
    };
    let verify_command = "SDL_VIDEODRIVER=dummy python3 -m unittest -q 2>&1";

    let (_, _, evidence) = rescue_invalid_done_payload_for_verified_action(
        Some(&plan),
        &[
            "write_file(path=maze_game_pygame/game.py)".to_string(),
            "write_file(path=maze_game_pygame/main.py)".to_string(),
            "write_file(path=maze_game_pygame/test_game.py)".to_string(),
            verify_command.to_string(),
        ],
        &ObservationEvidence::default(),
        VerificationLevel::Behavioral,
        Some(verify_command),
        Some(7),
        Some(7),
    )
    .expect("rescued done payload");

    assert_eq!(evidence.len(), 3);
    assert_eq!(
        evidence[0].command,
        "write_file(path=maze_game_pygame/game.py)"
    );
    assert_eq!(
        evidence[1].command,
        "write_file(path=maze_game_pygame/main.py)"
    );
    assert_eq!(evidence[2].command, verify_command);
}

#[test]
fn canonicalize_known_acceptance_commands_preserves_long_exec_verification_command() {
    let verify_command = "test -d maze_game_pygame/.git && test -f maze_game_pygame/README.md && test -f maze_game_pygame/game.py && test -f maze_game_pygame/main.py && test -f maze_game_pygame/test_game.py && cd maze_game_pygame && SDL_VIDEODRIVER=dummy python3 -m unittest -q 2>&1";

    let commands = canonicalize_known_acceptance_commands(
        &[verify_command.to_string()],
        &ObservationEvidence::default(),
    );

    assert_eq!(commands, vec![verify_command.to_string()]);
}

#[test]
fn preferred_action_verification_command_preserves_the_recorded_command_case() {
    let configured = "test -d maze_game_pygame/.git && test -f maze_game_pygame/README.md && test -f maze_game_pygame/game.py && test -f maze_game_pygame/main.py && test -f maze_game_pygame/test_game.py && cd maze_game_pygame && SDL_VIDEODRIVER=dummy python3 -m unittest -q 2>&1";
    let lowercased = "test -d maze_game_pygame/.git && test -f maze_game_pygame/readme.md && test -f maze_game_pygame/game.py && test -f maze_game_pygame/main.py && test -f maze_game_pygame/test_game.py && cd maze_game_pygame && sdl_videodriver=dummy python3 -m unittest -q 2>&1";

    let chosen = preferred_action_verification_command(
        &[lowercased.to_string()],
        VerificationLevel::Behavioral,
        Some(configured),
    )
    .expect("verification command");

    assert_eq!(chosen, lowercased);
    assert_eq!(
        preferred_action_verification_command(
            &[lowercased.into(), configured.into()],
            VerificationLevel::Behavioral,
            Some(configured)
        ),
        Some(configured.into())
    );
}

#[test]
fn latest_assistant_message_is_done_ignores_prior_done_when_newer_message_exists() {
    let messages = vec![
        json!({
            "role": "assistant",
            "content": "[DONE]\nCreated `demo_repo/.gitignore` and verified it."
        }),
        json!({
            "role": "assistant",
            "content": "Still working through one more verification step."
        }),
    ];

    assert!(!latest_assistant_message_is_done(&messages));

    let mut completed = messages;
    completed.push(json!({
        "role": "assistant",
        "content": "[DONE]\nCreated `demo_repo/README.md` and verified it."
    }));

    assert!(latest_assistant_message_is_done(&completed));
}
#[test]
fn maybe_build_verified_action_closeout_text_finalizes_pygame_repo_after_verify() {
    let plan = PlanBlock {
        goal: "Create a small pygame maze game repo and verify it.".to_string(),
        steps: vec![
            "initialize the repo".to_string(),
            "write the pygame gameplay files".to_string(),
            "run the unittest smoke".to_string(),
            "call done with verification evidence".to_string(),
        ],
        acceptance_criteria: vec![
            "maze_game_pygame/README.md explains the controls".to_string(),
            "maze_game_pygame/game.py contains the maze gameplay logic".to_string(),
            "maze_game_pygame/main.py launches the game loop".to_string(),
            "maze_game_pygame/test_game.py passes under SDL_VIDEODRIVER=dummy python3 -m unittest -q 2>&1".to_string(),
        ],
        risks: "wrong repo layout".to_string(),
        assumptions: "pygame is available".to_string(),
    };
    let verify_command = "test -d maze_game_pygame/.git && test -f maze_game_pygame/README.md && test -f maze_game_pygame/game.py && test -f maze_game_pygame/main.py && test -f maze_game_pygame/test_game.py && cd maze_game_pygame && SDL_VIDEODRIVER=dummy python3 -m unittest -q 2>&1";
    let messages = vec![
        json!({
            "role": "assistant",
            "tool_calls": [{
                "id": "call_game",
                "type": "function",
                "function": {
                    "name": "write_file",
                    "arguments": "{\"path\":\"maze_game_pygame/game.py\",\"content\":\"class MazeGame:\\n    pass\\n\"}"
                }
            }]
        }),
        json!({
            "role": "tool",
            "tool_call_id": "call_game",
            "content": "OK: wrote 'maze_game_pygame/game.py' (2 lines, 25 bytes)"
        }),
        json!({
            "role": "assistant",
            "tool_calls": [{
                "id": "call_main",
                "type": "function",
                "function": {
                    "name": "write_file",
                    "arguments": "{\"path\":\"maze_game_pygame/main.py\",\"content\":\"from game import MazeGame\\n\\nif __name__ == \\\"__main__\\\":\\n    MazeGame().run()\\n\"}"
                }
            }]
        }),
        json!({
            "role": "tool",
            "tool_call_id": "call_main",
            "content": "OK: wrote 'maze_game_pygame/main.py' (4 lines, 67 bytes)"
        }),
        json!({
            "role": "assistant",
            "tool_calls": [{
                "id": "call_verify",
                "type": "function",
                "function": {
                    "name": "exec",
                    "arguments": format!("{{\"command\":{}}}", serde_json::to_string(verify_command).expect("json"))
                }
            }]
        }),
        json!({
            "role": "tool",
            "tool_call_id": "call_verify",
            "content": "OK (exit_code: 0)\nstdout:\nRan 3 tests in 0.002s\n\nOK"
        }),
    ];
    let mut working_mem = WorkingMemory::default();
    working_mem.remember_successful_verification(verify_command);

    let final_text = maybe_build_verified_action_closeout_text(
        "Create a pygame maze game repo.",
        None,
        Some(&plan),
        &messages,
        &working_mem,
        &ObservationEvidence::default(),
        VerificationLevel::Behavioral,
        Some("SDL_VIDEODRIVER=dummy python3 -m unittest -q 2>&1"),
        Some(7),
        Some(7),
    )
    .expect("auto final answer");

    assert!(final_text.starts_with("[DONE]"));
    assert!(final_text.contains("maze_game_pygame/game.py"));
    assert!(final_text.contains("maze_game_pygame/main.py"));
    assert!(final_text.contains("SDL_VIDEODRIVER=dummy python3 -m unittest -q 2>&1"));

    let explicit_handoff = maybe_build_verified_action_closeout_text(
        "Create a pygame maze game repo. Final answer must include verification_receipt.txt and fresh exec proof.",
        None,
        Some(&plan),
        &messages,
        &working_mem,
        &ObservationEvidence::default(),
        VerificationLevel::Behavioral,
        Some("SDL_VIDEODRIVER=dummy python3 -m unittest -q 2>&1"),
        Some(7),
        Some(7),
    );
    assert!(
        explicit_handoff.is_none(),
        "an explicit handoff must reach the model's done summary"
    );
}

#[test]
fn maybe_build_verified_action_closeout_text_waits_for_required_followups() {
    let plan = PlanBlock {
        goal: "Fix the PR-ready merge approval handoff.".to_string(),
        steps: vec![
            "inspect merge approval code".to_string(),
            "patch the smallest bug".to_string(),
            "verify the configured smoke".to_string(),
            "call done with handoff paths".to_string(),
        ],
        acceptance_criteria: vec![
            "the requested change is implemented".to_string(),
            "the verification command passes".to_string(),
        ],
        risks: "missing follow-up paths".to_string(),
        assumptions: "configured smoke is relevant".to_string(),
    };
    let messages = vec![
        json!({
            "role":"assistant",
            "tool_calls":[{
                "id":"call_patch_merge",
                "type":"function",
                "function":{"name":"patch_file","arguments":"{\"path\":\"src/tui/agent/merge_approval.rs\",\"search\":\"old\",\"replace\":\"new\"}"}
            }]
        }),
        json!({
            "role":"tool",
            "tool_call_id":"call_patch_merge",
            "content":"OK: patched 'src/tui/agent/merge_approval.rs' (+1 lines, 99 total)\n[auto-test] ✓ PASSED (exit 0)"
        }),
    ];
    let test_cmd =
        "cargo test -q tui::agent::merge_approval::tests:: 2>&1 && bash scripts/pr-ready-smoke.sh";

    let final_text = maybe_build_verified_action_closeout_text(
        "Update `docs/state-schema.md` and `.spiral-coder/runtime_eval.json` to include `src/tui/agent/merge_approval.rs`.",
        None,
        Some(&plan),
        &messages,
        &WorkingMemory::default(),
        &ObservationEvidence::default(),
        VerificationLevel::Behavioral,
        Some(test_cmd),
        Some(1),
        Some(1),
    );

    assert!(final_text.is_none());
}
