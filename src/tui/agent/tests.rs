use super::*;

#[test]
fn extracts_implied_write_files_from_markdown() {
    let text = r#"
File: README.md
```md
# Hello
```

### src/main.rs
```rust
fn main() {}
```

```diff
--- a/x
+++ b/x
@@ -1 +1 @@
-a
+b
```
"#;
    let (_, files) = implied_actions::extract(text);
    assert_eq!(files.len(), 2);
    assert_eq!(files[0].path, "README.md");
    assert!(files[0].content.contains("# Hello"));
    assert_eq!(files[1].path, "src/main.rs");
    assert!(files[1].content.contains("fn main"));
}

#[test]
fn blocks_git_init_inside_existing_repo() {
    let td = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir_all(td.path().join(".git")).expect("mkdir .git");
    let root = td.path().to_string_lossy();
    let msg =
        should_block_git_landmines("git init Foo", Some(root.as_ref())).expect("expected block");
    assert!(msg.to_ascii_lowercase().contains("refusing"));
}

#[test]
fn git_checkpoint_requires_tool_root_to_be_git_top_level() {
    let td = tempfile::tempdir().expect("tempdir");
    let root = td.path().join("repo");
    let child = root.join(".tmp").join("case").join("tool_root");
    std::fs::create_dir_all(&child).expect("mkdir child");

    assert!(git_checkpoint_root_matches_tool_root(
        root.to_string_lossy().as_ref(),
        root.to_string_lossy().as_ref()
    ));
    assert!(!git_checkpoint_root_matches_tool_root(
        child.to_string_lossy().as_ref(),
        root.to_string_lossy().as_ref()
    ));
}

#[test]
fn blocks_git_add_when_nested_repo_detected() {
    let td = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir_all(td.path().join(".git")).expect("mkdir .git");
    std::fs::create_dir_all(td.path().join("MazeGame").join(".git")).expect("mkdir nested");
    let root = td.path().to_string_lossy();
    let msg =
        should_block_git_landmines("git add -A", Some(root.as_ref())).expect("expected block");
    assert!(msg.contains("MazeGame"));
}

#[test]
fn allows_git_add_when_nested_is_submodule() {
    let td = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir_all(td.path().join(".git")).expect("mkdir .git");
    std::fs::create_dir_all(td.path().join("MazeGame").join(".git")).expect("mkdir nested");
    std::fs::write(
        td.path().join(".gitmodules"),
        "[submodule \"MazeGame\"]\n\tpath = MazeGame\n\turl = https://example.invalid/MazeGame.git\n",
    )
    .expect("write .gitmodules");
    let root = td.path().to_string_lossy();
    assert!(
        should_block_git_landmines("git add -A", Some(root.as_ref())).is_none(),
        "should not block when nested repo is declared as submodule"
    );
}

#[test]
fn injects_git_proxy_hint_for_github_via_localhost() {
    let stderr = "fatal: unable to access 'https://github.com/x/y.git/': Failed to connect to github.com port 443 via 127.0.0.1 after 2041 ms: Could not connect to server";
    let out = build_failed_tool_output("", stderr, 1);
    assert!(
        out.contains("ssh.github.com"),
        "should suggest ssh-over-443"
    );
    assert!(
        out.contains("HTTP_PROXY") || out.contains("HTTPS_PROXY"),
        "should mention proxy env vars"
    );
}

#[test]
fn injects_cargo_exe_lock_hint_on_windows_style_error() {
    let stderr = "error: failed to remove file `C:\\\\Users\\\\user\\\\spiral-coder\\\\target\\\\debug\\\\spiral-coder.exe`\nCaused by: Access is denied. (os error 5)";
    let out = build_failed_tool_output("", stderr, 1);
    assert!(
        out.to_ascii_lowercase().contains("cargo_target_dir"),
        "should suggest isolated target dir"
    );
}

#[test]
fn parses_reflection_block_fields() {
    let text = "\
<reflect>\n\
last_outcome: failure\n\
goal_delta: Further\n\
wrong_assumption: I thought the file existed.\n\
strategy_change: abandon\n\
next_minimal_action: read_file src/tui/agent.rs\n\
</reflect>\n";
    let r = parse_reflection_block(text).expect("reflection parsed");
    assert_eq!(r.goal_delta, GoalDelta::Farther);
    assert_eq!(r.strategy_change, StrategyChange::Abandon);
    assert_eq!(r.wrong_assumption, "I thought the file existed.");
    assert_eq!(r.next_minimal_action, "read_file src/tui/agent.rs");
}

#[test]
fn parses_impact_block_fields() {
    let text = "\
<impact>\n\
changed: patched src/tui/agent.rs validation\n\
progress: step 2 moved because runtime gate exists\n\
remaining_gap: still need to run cargo test\n\
</impact>\n";
    let impact = parse_impact_block(text).expect("impact parsed");
    assert_eq!(impact.changed, "patched src/tui/agent.rs validation");
    assert_eq!(impact.progress, "step 2 moved because runtime gate exists");
    assert_eq!(impact.remaining_gap, "still need to run cargo test");
}

#[test]
fn validate_impact_requires_all_fields() {
    let impact = ImpactBlock {
        changed: "patched file".to_string(),
        progress: "".to_string(),
        remaining_gap: "".to_string(),
    };
    let plan = PlanBlock {
        goal: "ship fix".to_string(),
        steps: vec!["inspect".to_string(), "verify".to_string()],
        acceptance_criteria: vec!["cargo check passes".to_string()],
        risks: "wrong file".to_string(),
        assumptions: "tests exist".to_string(),
    };
    assert!(validate_impact(&impact, Some(&plan)).is_err());
}

#[test]
fn validate_impact_accepts_matching_acceptance_criterion() {
    let impact = ImpactBlock {
        changed: "runtime gate now blocks invalid tool calls".to_string(),
        progress: "acceptance 2 moved because runtime gate blocks bad tool calls".to_string(),
        remaining_gap: "still need cargo check".to_string(),
    };
    let plan = PlanBlock {
        goal: "ship fix".to_string(),
        steps: vec!["inspect".to_string(), "verify".to_string()],
        acceptance_criteria: vec![
            "cargo check passes".to_string(),
            "runtime gate blocks bad tool calls".to_string(),
        ],
        risks: "wrong file".to_string(),
        assumptions: "tests exist".to_string(),
    };
    assert!(validate_impact(&impact, Some(&plan)).is_ok());
}

#[test]
fn validate_impact_rejects_unknown_progress_reference() {
    let impact = ImpactBlock {
        changed: "updated docs".to_string(),
        progress: "made criterion 9 move".to_string(),
        remaining_gap: "still need review".to_string(),
    };
    let plan = PlanBlock {
        goal: "ship fix".to_string(),
        steps: vec!["inspect".to_string(), "verify".to_string()],
        acceptance_criteria: vec!["cargo check passes".to_string()],
        risks: "wrong file".to_string(),
        assumptions: "tests exist".to_string(),
    };
    assert!(validate_impact(&impact, Some(&plan)).is_err());
}

#[test]
fn validate_done_acceptance_accepts_completed_and_remaining_coverage() {
    let plan = PlanBlock {
        goal: "ship fix".to_string(),
        steps: vec!["inspect".to_string(), "verify".to_string()],
        acceptance_criteria: vec![
            "cargo check passes".to_string(),
            "runtime gate blocks bad tool calls".to_string(),
        ],
        risks: "wrong file".to_string(),
        assumptions: "tests exist".to_string(),
    };

    let completed_acceptance = vec!["acceptance 1".to_string()];
    let remaining_acceptance = vec!["runtime gate blocks bad tool calls".to_string()];
    let acceptance_evidence = vec![DoneAcceptanceEvidence {
        criterion: "acceptance 1".to_string(),
        command: "cargo check".to_string(),
    }];
    let known_commands = vec!["cargo check".to_string()];

    assert!(validate_done_acceptance(
        Some(&plan),
        &completed_acceptance,
        &remaining_acceptance,
        &acceptance_evidence,
        &known_commands,
        &ObservationEvidence::default(),
    )
    .is_ok());
}

#[test]
fn validate_done_acceptance_rejects_missing_criterion_coverage() {
    let plan = PlanBlock {
        goal: "ship fix".to_string(),
        steps: vec!["inspect".to_string(), "verify".to_string()],
        acceptance_criteria: vec![
            "cargo check passes".to_string(),
            "runtime gate blocks bad tool calls".to_string(),
        ],
        risks: "wrong file".to_string(),
        assumptions: "tests exist".to_string(),
    };

    let completed_acceptance = vec!["acceptance 1".to_string()];
    let remaining_acceptance = Vec::new();
    let acceptance_evidence = vec![DoneAcceptanceEvidence {
        criterion: "acceptance 1".to_string(),
        command: "cargo check".to_string(),
    }];
    let known_commands = vec!["cargo check".to_string()];

    assert!(validate_done_acceptance(
        Some(&plan),
        &completed_acceptance,
        &remaining_acceptance,
        &acceptance_evidence,
        &known_commands,
        &ObservationEvidence::default(),
    )
    .is_err());
}

#[test]
fn validate_done_acceptance_rejects_unknown_reference() {
    let plan = PlanBlock {
        goal: "ship fix".to_string(),
        steps: vec!["inspect".to_string(), "verify".to_string()],
        acceptance_criteria: vec!["cargo check passes".to_string()],
        risks: "wrong file".to_string(),
        assumptions: "tests exist".to_string(),
    };

    let completed_acceptance = vec!["acceptance 9".to_string()];
    let remaining_acceptance = Vec::new();
    let acceptance_evidence = vec![DoneAcceptanceEvidence {
        criterion: "acceptance 9".to_string(),
        command: "cargo check".to_string(),
    }];
    let known_commands = vec!["cargo check".to_string()];

    assert!(validate_done_acceptance(
        Some(&plan),
        &completed_acceptance,
        &remaining_acceptance,
        &acceptance_evidence,
        &known_commands,
        &ObservationEvidence::default(),
    )
    .is_err());
}

#[test]
fn validate_done_acceptance_rejects_missing_evidence_for_completed_criterion() {
    let plan = PlanBlock {
        goal: "ship fix".to_string(),
        steps: vec!["inspect".to_string(), "verify".to_string()],
        acceptance_criteria: vec!["cargo check passes".to_string()],
        risks: "wrong file".to_string(),
        assumptions: "tests exist".to_string(),
    };

    let completed_acceptance = vec!["acceptance 1".to_string()];
    let remaining_acceptance = Vec::new();
    let acceptance_evidence = Vec::new();
    let known_commands = vec!["cargo check".to_string()];

    assert!(validate_done_acceptance(
        Some(&plan),
        &completed_acceptance,
        &remaining_acceptance,
        &acceptance_evidence,
        &known_commands,
        &ObservationEvidence::default(),
    )
    .is_err());
}

#[test]
fn validate_done_acceptance_rejects_unknown_verification_command() {
    let plan = PlanBlock {
        goal: "ship fix".to_string(),
        steps: vec!["inspect".to_string(), "verify".to_string()],
        acceptance_criteria: vec!["cargo check passes".to_string()],
        risks: "wrong file".to_string(),
        assumptions: "tests exist".to_string(),
    };

    let completed_acceptance = vec!["acceptance 1".to_string()];
    let remaining_acceptance = Vec::new();
    let acceptance_evidence = vec![DoneAcceptanceEvidence {
        criterion: "acceptance 1".to_string(),
        command: "cargo test".to_string(),
    }];
    let known_commands = vec!["cargo check".to_string()];

    assert!(validate_done_acceptance(
        Some(&plan),
        &completed_acceptance,
        &remaining_acceptance,
        &acceptance_evidence,
        &known_commands,
        &ObservationEvidence::default(),
    )
    .is_err());
}

#[test]
fn validate_done_acceptance_accepts_canonicalized_tool_evidence() {
    let plan = PlanBlock {
        goal: "locate slash command".to_string(),
        steps: vec!["inspect".to_string(), "confirm".to_string()],
        acceptance_criteria: vec![
            "The exact file path is identified".to_string(),
            "The handler context is confirmed".to_string(),
        ],
        risks: "wrong file".to_string(),
        assumptions: "repo indexed".to_string(),
    };

    let completed_acceptance = vec!["acceptance 1".to_string(), "acceptance 2".to_string()];
    let remaining_acceptance = Vec::new();
    let acceptance_evidence = vec![
        DoneAcceptanceEvidence {
            criterion: "acceptance 1".to_string(),
            command: "search_files(pattern=\"/realize\", dir=\"src/tui/events.rs\")".to_string(),
        },
        DoneAcceptanceEvidence {
            criterion: "acceptance 2".to_string(),
            command: "read_file(path=\"src/tui/events.rs\")".to_string(),
        },
    ];
    let known_commands = vec![
        "search_files(dir=/Users/ignored, pattern=nope)".to_string(),
        "search_files(dir=src/tui/events.rs, pattern=/realize)".to_string(),
        "read_file(path=src/tui/events.rs)".to_string(),
    ];

    assert!(validate_done_acceptance(
        Some(&plan),
        &completed_acceptance,
        &remaining_acceptance,
        &acceptance_evidence,
        &known_commands,
        &ObservationEvidence::default(),
    )
    .is_ok());
}

#[test]
fn resolve_acceptance_reference_accepts_leading_numbered_text() {
    let plan = PlanBlock {
        goal: "locate handler".to_string(),
        steps: vec!["inspect".to_string()],
        acceptance_criteria: vec![
            "The exact file path containing the `/realize` slash command handler is identified."
                .to_string(),
            "The handler logic is confirmed to be part of the TUI component.".to_string(),
        ],
        risks: "wrong file".to_string(),
        assumptions: "repo indexed".to_string(),
    };

    assert_eq!(
        resolve_acceptance_reference(
            "1) The exact file path containing the `/realize` slash command handler is identified (`src/tui/events.rs`).",
            &plan
        ),
        Some(0)
    );
    assert_eq!(
        resolve_acceptance_reference(
            "2) The handler logic is confirmed to be part of the TUI component (within `tui/events.rs`).",
            &plan
        ),
        Some(1)
    );
}

#[test]
fn build_read_only_evidence_scores_prefers_search_plus_read_confirmation() {
    let plan = PlanBlock {
        goal: "Locate the /realize slash command handler in the TUI".to_string(),
        steps: vec![
            "inspect src".to_string(),
            "search for /realize".to_string(),
            "read the matching file".to_string(),
        ],
        acceptance_criteria: vec![
            "The exact file path containing the `/realize` slash command handler is identified."
                .to_string(),
            "The handler logic is confirmed to be part of the TUI component.".to_string(),
            "The location is verified by reading the file and confirming the context.".to_string(),
        ],
        risks: "wrong file".to_string(),
        assumptions: "repo indexed".to_string(),
    };
    let evidence = ObservationEvidence {
        searches: vec![ObservationSearchEvidence {
            command: "search_files(dir=src, pattern=/realize)".to_string(),
            pattern: "/realize".to_string(),
            hit_count: 1,
            paths: vec!["src/tui/events.rs".to_string()],
        }],
        reads: vec![ObservationReadEvidence {
            command: "read_file(path=src/tui/events.rs)".to_string(),
            path: "src/tui/events.rs".to_string(),
        }],
        resolutions: Vec::new(),
    };

    let scores = build_read_only_evidence_scores(
        "Locate where the /realize slash command is handled in the TUI. Do not edit anything.",
        &plan,
        &evidence,
    );

    assert_eq!(scores.len(), 3);
    assert!(scores[0].total >= 0.80);
    assert!(scores[1].read_confirm >= 0.80);
    assert!(scores[1]
        .suggested_commands
        .first()
        .is_some_and(|cmd| cmd.contains("read_file")));
    assert!(scores[2]
        .suggested_commands
        .iter()
        .any(|cmd| cmd.contains("read_file")));
}

#[test]
fn build_read_only_completion_hint_promotes_done_after_strong_observation() {
    let plan = PlanBlock {
        goal: "Locate the /realize slash command handler in the TUI".to_string(),
        steps: vec![
            "search src".to_string(),
            "read the matching file".to_string(),
            "confirm the context".to_string(),
        ],
        acceptance_criteria: vec![
            "The exact file path containing the `/realize` slash command handler is identified."
                .to_string(),
            "The handler logic is confirmed to be part of the TUI component.".to_string(),
        ],
        risks: "wrong file".to_string(),
        assumptions: "repo indexed".to_string(),
    };
    let messages = vec![
        json!({
            "role": "assistant",
            "tool_calls": [{
                "id": "call_search",
                "type": "function",
                "function": {
                    "name": "search_files",
                    "arguments": "{\"pattern\":\"/realize\",\"dir\":\"src\"}"
                }
            }]
        }),
        json!({
            "role": "tool",
            "tool_call_id": "call_search",
            "content": "[search_files: '/realize' — 1 match(es)]\nsrc/tui/events.rs:465:         \"/realize\" => {"
        }),
        json!({
            "role": "assistant",
            "tool_calls": [{
                "id": "call_read",
                "type": "function",
                "function": {
                    "name": "read_file",
                    "arguments": "{\"path\":\"src/tui/events.rs\"}"
                }
            }]
        }),
        json!({
            "role": "tool",
            "tool_call_id": "call_read",
            "content": "[src/tui/events.rs] (2918 lines, 108025 bytes)\nfn handle_slash_command(text: &str, app: &mut App, pane: PaneId) -> bool {\n    match cmd_lc.as_str() {\n        \"/realize\" => {"
        }),
    ];

    let evidence = collect_observation_evidence(&messages);
    let hint = build_read_only_completion_hint(
        "Locate where the /realize slash command is handled in the TUI. Do not edit anything.",
        &plan,
        &evidence,
        &messages,
        &WorkingMemory::default(),
    )
    .expect("completion hint");

    assert!(hint.contains("call done directly now"));
    assert!(hint.contains("tool: done"));
    assert!(hint.contains("src/tui/events.rs") || hint.contains("read_file"));
}

#[test]
fn build_read_only_search_to_read_hint_prefers_best_src_tui_candidate() {
    let plan = PlanBlock {
        goal: "Locate the /realize slash command handler in the TUI".to_string(),
        steps: vec![
            "search src".to_string(),
            "read the matching file".to_string(),
            "confirm the context".to_string(),
        ],
        acceptance_criteria: vec![
            "The exact file path containing the `/realize` slash command handler is identified."
                .to_string(),
            "The handler logic is confirmed to be part of the TUI component.".to_string(),
        ],
        risks: "wrong file".to_string(),
        assumptions: "repo indexed".to_string(),
    };
    let evidence = ObservationEvidence {
        searches: vec![ObservationSearchEvidence {
            command: "search_files(pattern=/realize, dir=src)".to_string(),
            pattern: "/realize".to_string(),
            hit_count: 4,
            paths: vec![
                "README.md".to_string(),
                "src/runtime_eval.rs".to_string(),
                "src/tui/events.rs".to_string(),
                "src/tui/agent.rs".to_string(),
            ],
        }],
        reads: vec![],
        resolutions: Vec::new(),
    };

    let hint = build_read_only_search_to_read_hint(
        "Locate where the /realize slash command is handled in the TUI. Do not edit anything. Final answer must include the file path.",
        &plan,
        &evidence.searches[0].paths,
        &evidence,
    )
    .expect("search-to-read hint");

    assert!(hint.contains("read_file(path=\"src/tui/events.rs\")"));
    assert!(hint.contains("plausible code candidate"));
}

#[test]
fn best_read_only_followup_read_path_prefers_events_over_ui_for_slash_handler() {
    let plan = PlanBlock {
        goal: "Locate where `/realize` is handled in the TUI".to_string(),
        steps: vec![
            "search src".to_string(),
            "read the matching file".to_string(),
            "confirm the handler context".to_string(),
        ],
        acceptance_criteria: vec![
            "The exact file path containing the `/realize` slash command handler is identified."
                .to_string(),
            "The handler branch is confirmed by read_file.".to_string(),
        ],
        risks: "wrong file".to_string(),
        assumptions: "repo indexed".to_string(),
    };
    let best = best_read_only_followup_read_path(
        "Locate where the /realize slash command is handled in the TUI. Do not edit anything.",
        &plan,
        &[
            "src/tui/ui.rs".to_string(),
            "src/tui/events.rs".to_string(),
            "src/runtime_eval.rs".to_string(),
        ],
        &ObservationEvidence::default(),
    )
    .expect("best follow-up path");

    assert_eq!(best, "src/tui/events.rs");
}

#[test]
fn best_read_only_followup_read_path_prefers_prefs_rs_for_prefs_task() {
    let plan = synthetic_read_only_observation_plan(
        "Find where pane-scoped TUI preferences are serialized and restored. Do not edit anything.",
    );
    let best = best_read_only_followup_read_path(
        "Find where pane-scoped TUI preferences are serialized and restored. Do not edit anything.",
        &plan,
        &[
            "src/tui/events.rs".to_string(),
            "src/tui/ui.rs".to_string(),
            "src/tui/app.rs".to_string(),
        ],
        &ObservationEvidence::default(),
    )
    .expect("best follow-up path");

    assert_eq!(best, "src/tui/prefs.rs");
}

#[test]
fn best_read_only_followup_read_path_prefers_agent_rs_for_repo_map_flow_task() {
    let plan = synthetic_read_only_observation_plan(
        "Find where the coder-side repo-map fallback for read_file misses is wired into the TUI agent flow. Do not edit anything.",
    );
    let best = best_read_only_followup_read_path(
        "Find where the coder-side repo-map fallback for read_file misses is wired into the TUI agent flow. Do not edit anything.",
        &plan,
        &[
            "src/runtime_eval.rs".to_string(),
            "src/agent_session.rs".to_string(),
            "src/repo_map.rs".to_string(),
        ],
        &ObservationEvidence::default(),
    )
    .expect("best follow-up path");

    assert_eq!(best, "src/tui/agent.rs");
}

#[test]
fn build_read_only_diagnose_coercion_hint_starts_with_search_when_no_observation_exists() {
    let hint = build_read_only_diagnose_coercion_hint(
        "Locate where the /realize slash command is handled in the TUI. Do not edit anything.",
        None,
        &ObservationEvidence::default(),
        &[],
        &WorkingMemory::default(),
    )
    .expect("diagnose coercion hint");

    assert!(hint.contains("search_files(pattern=\"/realize\", dir=\"src/tui\")"));
}

#[test]
fn first_action_constraint_hint_prefers_observation_tool_for_read_only() {
    assert_eq!(first_action_deadline_iters(true, true), 2);
    let hint = build_first_action_constraint_hint(
        "Locate where the /realize slash command is handled in the TUI. Do not edit anything.",
        true,
        true,
    )
    .expect("hint");
    assert!(hint.contains("[First Action Constraint]"));
    assert!(hint.contains("search_files(pattern=\"/realize\", dir=\"src/tui\")"));
}

#[test]
fn first_action_constraint_hint_requires_tool_for_action_tasks() {
    assert_eq!(first_action_deadline_iters(false, true), 2);
    let hint = build_first_action_constraint_hint(
        "Fix the /realize command handling and keep tests passing.",
        false,
        true,
    )
    .expect("hint");
    assert!(hint.contains("Within the first 2 turns"));
    assert!(hint.contains("call ONE real tool"));
}

#[test]
fn build_read_only_diagnose_coercion_hint_switches_to_read_after_search() {
    let plan = PlanBlock {
        goal: "Locate the /realize slash command handler in the TUI".to_string(),
        steps: vec![
            "search src".to_string(),
            "read the matching file".to_string(),
            "confirm the context".to_string(),
        ],
        acceptance_criteria: vec![
            "The exact file path containing the `/realize` slash command handler is identified."
                .to_string(),
            "The handler logic is confirmed to be part of the TUI component.".to_string(),
        ],
        risks: "wrong file".to_string(),
        assumptions: "repo indexed".to_string(),
    };
    let evidence = ObservationEvidence {
        searches: vec![ObservationSearchEvidence {
            command: "search_files(pattern=/realize, dir=src)".to_string(),
            pattern: "/realize".to_string(),
            hit_count: 1,
            paths: vec!["src/tui/events.rs".to_string()],
        }],
        reads: vec![],
        resolutions: Vec::new(),
    };

    let hint = build_read_only_diagnose_coercion_hint(
        "Locate where the /realize slash command is handled in the TUI. Do not edit anything.",
        Some(&plan),
        &evidence,
        &[],
        &WorkingMemory::default(),
    )
    .expect("diagnose coercion hint");

    assert!(hint.contains("read_file(path=\"src/tui/events.rs\")"));
}

#[test]
fn choose_read_only_diagnose_rescue_action_starts_with_search() {
    let action = choose_read_only_diagnose_rescue_action(
        "Locate where the /realize slash command is handled in the TUI. Do not edit anything.",
        None,
        &ObservationEvidence::default(),
    );

    assert_eq!(
        action,
        Some(ReadOnlyDiagnoseRescueAction::Search {
            pattern: "/realize".to_string(),
            dir: "src/tui".to_string(),
        })
    );
}

#[test]
fn choose_read_only_diagnose_rescue_action_advances_to_read() {
    let plan = PlanBlock {
        goal: "Locate the /realize slash command handler in the TUI".to_string(),
        steps: vec![
            "search src".to_string(),
            "read the matching file".to_string(),
            "confirm the context".to_string(),
        ],
        acceptance_criteria: vec![
            "The exact file path containing the `/realize` slash command handler is identified."
                .to_string(),
            "The handler logic is confirmed to be part of the TUI component.".to_string(),
        ],
        risks: "wrong file".to_string(),
        assumptions: "repo indexed".to_string(),
    };
    let evidence = ObservationEvidence {
        searches: vec![ObservationSearchEvidence {
            command: "search_files(pattern=/realize, dir=src)".to_string(),
            pattern: "/realize".to_string(),
            hit_count: 1,
            paths: vec!["src/tui/events.rs".to_string()],
        }],
        reads: vec![],
        resolutions: Vec::new(),
    };

    let action = choose_read_only_diagnose_rescue_action(
        "Locate where the /realize slash command is handled in the TUI. Do not edit anything.",
        Some(&plan),
        &evidence,
    );

    assert_eq!(
        action,
        Some(ReadOnlyDiagnoseRescueAction::Read {
            path: "src/tui/events.rs".to_string(),
        })
    );
}

#[test]
fn build_read_only_iteration_cap_final_answer_includes_path() {
    let plan = PlanBlock {
        goal: "Locate the /realize slash command handler in the TUI".to_string(),
        steps: vec![
            "search src".to_string(),
            "read the matching file".to_string(),
            "confirm the context".to_string(),
        ],
        acceptance_criteria: vec![
            "The exact file path containing the `/realize` slash command handler is identified."
                .to_string(),
            "The handler logic is confirmed to be part of the TUI component.".to_string(),
        ],
        risks: "wrong file".to_string(),
        assumptions: "repo indexed".to_string(),
    };
    let messages = vec![
        json!({
            "role": "assistant",
            "tool_calls": [{
                "id": "call_search",
                "type": "function",
                "function": {
                    "name": "search_files",
                    "arguments": "{\"pattern\":\"/realize\",\"dir\":\"src\"}"
                }
            }]
        }),
        json!({
            "role": "tool",
            "tool_call_id": "call_search",
            "content": "[search_files: '/realize' — 1 match(es)]\nsrc/tui/events.rs:465:         \"/realize\" => {"
        }),
        json!({
            "role": "assistant",
            "tool_calls": [{
                "id": "call_read",
                "type": "function",
                "function": {
                    "name": "read_file",
                    "arguments": "{\"path\":\"src/tui/events.rs\"}"
                }
            }]
        }),
        json!({
            "role": "tool",
            "tool_call_id": "call_read",
            "content": "[src/tui/events.rs] (2918 lines, 108025 bytes)\nfn handle_slash_command(text: &str, app: &mut App, pane: PaneId) -> bool {\n    match cmd_lc.as_str() {\n        \"/realize\" => {"
        }),
    ];

    let evidence = collect_observation_evidence(&messages);
    let final_text = build_read_only_iteration_cap_final_answer(
        "Locate where the /realize slash command is handled in the TUI. Do not edit anything. Final answer must include the file path.",
        &plan,
        &evidence,
        &messages,
        &WorkingMemory::default(),
    )
    .expect("final answer");

    assert!(final_text.starts_with("[DONE]"));
    assert!(final_text.contains("src/tui/events.rs"));
    assert!(final_text.contains("acceptance 1"));
}

#[test]
fn maybe_build_read_only_auto_final_answer_reuses_read_only_evidence() {
    let plan = PlanBlock {
        goal: "Locate the /realize slash command handler in the TUI".to_string(),
        steps: vec![
            "search src".to_string(),
            "read the matching file".to_string(),
            "confirm the context".to_string(),
        ],
        acceptance_criteria: vec![
            "The exact file path containing the `/realize` slash command handler is identified."
                .to_string(),
            "The handler logic is confirmed to be part of the TUI component.".to_string(),
        ],
        risks: "wrong file".to_string(),
        assumptions: "repo indexed".to_string(),
    };
    let messages = vec![
        json!({
            "role": "assistant",
            "tool_calls": [{
                "id": "call_search",
                "type": "function",
                "function": {
                    "name": "search_files",
                    "arguments": "{\"pattern\":\"/realize\",\"dir\":\"src\"}"
                }
            }]
        }),
        json!({
            "role": "tool",
            "tool_call_id": "call_search",
            "content": "[search_files: '/realize' — 1 match(es)]\nsrc/tui/events.rs:465:         \"/realize\" => {"
        }),
        json!({
            "role": "assistant",
            "tool_calls": [{
                "id": "call_read",
                "type": "function",
                "function": {
                    "name": "read_file",
                    "arguments": "{\"path\":\"src/tui/events.rs\"}"
                }
            }]
        }),
        json!({
            "role": "tool",
            "tool_call_id": "call_read",
            "content": "[src/tui/events.rs] (2918 lines, 108025 bytes)\nfn handle_slash_command(text: &str, app: &mut App, pane: PaneId) -> bool {\n    match cmd_lc.as_str() {\n        \"/realize\" => {"
        }),
    ];
    let evidence = collect_observation_evidence(&messages);

    let final_text = maybe_build_read_only_auto_final_answer(
        true,
        "Locate where the /realize slash command is handled in the TUI. Do not edit anything. Final answer must include the file path.",
        Some(&plan),
        &evidence,
        &messages,
        &WorkingMemory::default(),
    )
    .expect("auto final answer");

    assert!(final_text.starts_with("[DONE]"));
    assert!(final_text.contains("src/tui/events.rs"));
    assert!(final_text.contains("via `read_file(path=src/tui/events.rs)`"));
}

#[test]
fn build_read_only_strong_final_answer_requires_strong_read_backed_evidence() {
    let plan = PlanBlock {
        goal: "Locate the /realize slash command handler in the TUI".to_string(),
        steps: vec![
            "search src".to_string(),
            "read the matching file".to_string(),
            "confirm the context".to_string(),
        ],
        acceptance_criteria: vec![
            "The exact file path containing the `/realize` slash command handler is identified."
                .to_string(),
            "The handler logic is confirmed to be part of the TUI component.".to_string(),
        ],
        risks: "wrong file".to_string(),
        assumptions: "repo indexed".to_string(),
    };
    let strong_messages = vec![
        json!({
            "role": "assistant",
            "tool_calls": [{
                "id": "call_search",
                "type": "function",
                "function": {
                    "name": "search_files",
                    "arguments": "{\"pattern\":\"/realize\",\"dir\":\"src/tui\"}"
                }
            }]
        }),
        json!({
            "role": "tool",
            "tool_call_id": "call_search",
            "content": "[search_files: '/realize' — 2 match(es)]\nsrc/tui/events.rs:763: \"/realize\" => {\nsrc/tui/ui.rs:401: \"/realize\""
        }),
        json!({
            "role": "assistant",
            "tool_calls": [{
                "id": "call_read",
                "type": "function",
                "function": {
                    "name": "read_file",
                    "arguments": "{\"path\":\"src/tui/events.rs\"}"
                }
            }]
        }),
        json!({
            "role": "tool",
            "tool_call_id": "call_read",
            "content": "[src/tui/events.rs] (3639 lines, 133416 bytes)\nfn handle_slash_command(text: &str, app: &mut App, pane: PaneId) -> bool {\n    match cmd_lc.as_str() {\n        \"/realize\" => {"
        }),
    ];
    let strong_evidence = collect_observation_evidence(&strong_messages);
    let final_text = build_read_only_strong_final_answer(
        "Locate where the /realize slash command is handled in the TUI. Do not edit anything.",
        &plan,
        &strong_evidence,
        &strong_messages,
        &WorkingMemory::default(),
    )
    .expect("strong final answer");
    assert!(final_text.contains("src/tui/events.rs"));

    let weak_messages = vec![
        json!({
            "role": "assistant",
            "tool_calls": [{
                "id": "call_search",
                "type": "function",
                "function": {
                    "name": "search_files",
                    "arguments": "{\"pattern\":\"realize\"}"
                }
            }]
        }),
        json!({
            "role": "tool",
            "tool_call_id": "call_search",
            "content": "[search_files: 'realize' — 50 match(es)]\nREADME.md:511: realize\nsrc/tui/events.rs:763: \"/realize\" => {\nsrc/tui/ui.rs:401: \"/realize\""
        }),
        json!({
            "role": "assistant",
            "tool_calls": [{
                "id": "call_read",
                "type": "function",
                "function": {
                    "name": "read_file",
                    "arguments": "{\"path\":\"src/tui/ui.rs\"}"
                }
            }]
        }),
        json!({
            "role": "tool",
            "tool_call_id": "call_read",
            "content": "[src/tui/ui.rs] (1861 lines, 67793 bytes)\npub fn render(frame: &mut Frame, app: &App) {"
        }),
    ];
    let weak_evidence = collect_observation_evidence(&weak_messages);
    assert!(build_read_only_strong_final_answer(
        "Locate where the /realize slash command is handled in the TUI. Do not edit anything.",
        &plan,
        &weak_evidence,
        &weak_messages,
        &WorkingMemory::default(),
    )
    .is_none());
}

#[test]
fn build_read_only_strong_final_answer_supports_pane_prefs_task() {
    let plan = synthetic_read_only_observation_plan(
        "Find where pane-scoped TUI preferences are serialized and restored. Do not edit anything.",
    );
    let messages = vec![
        json!({
            "role": "assistant",
            "tool_calls": [{
                "id": "call_search",
                "type": "function",
                "function": {
                    "name": "search_files",
                    "arguments": "{\"pattern\":\"prefs\",\"dir\":\"src/tui\"}"
                }
            }]
        }),
        json!({
            "role": "tool",
            "tool_call_id": "call_search",
            "content": "[search_files: 'prefs' — 2 match(es)]\nsrc/tui/events.rs:124: let saved = prefs::snapshot_app_prefs(app);\nsrc/tui/prefs.rs:183: pub fn snapshot_app_prefs(app: &App) -> TuiPrefs {"
        }),
        json!({
            "role": "assistant",
            "tool_calls": [{
                "id": "call_read",
                "type": "function",
                "function": {
                    "name": "read_file",
                    "arguments": "{\"path\":\"src/tui/prefs.rs\"}"
                }
            }]
        }),
        json!({
            "role": "tool",
            "tool_call_id": "call_read",
            "content": "[src/tui/prefs.rs] (327 lines, 10301 bytes)\npub fn snapshot_app_prefs(app: &App) -> TuiPrefs {\npub fn apply_prefs_to_app(app: &mut App, prefs: &TuiPrefs) {"
        }),
    ];
    let evidence = collect_observation_evidence(&messages);
    let final_text = build_read_only_strong_final_answer(
        "Find where pane-scoped TUI preferences are serialized and restored. Do not edit anything.",
        &plan,
        &evidence,
        &messages,
        &WorkingMemory::default(),
    )
    .expect("strong final answer");

    assert!(final_text.contains("src/tui/prefs.rs"));
    assert!(final_text.contains("read_file(path=src/tui/prefs.rs)"));
}

#[test]
fn maybe_build_read_only_auto_final_answer_falls_back_to_synthetic_plan() {
    let messages = vec![
        json!({
            "role": "assistant",
            "tool_calls": [{
                "id": "call_search",
                "type": "function",
                "function": {
                    "name": "search_files",
                    "arguments": "{\"pattern\":\"/realize\",\"dir\":\"src/tui\"}"
                }
            }]
        }),
        json!({
            "role": "tool",
            "tool_call_id": "call_search",
            "content": "[search_files: '/realize' — 3 match(es)]\nevents.rs:763: \"/realize\" => {\nintent.rs:519: raw_user_prompt: \"Find /realize handler\"\nagent.rs:100: something"
        }),
        json!({
            "role": "assistant",
            "tool_calls": [{
                "id": "call_read",
                "type": "function",
                "function": {
                    "name": "read_file",
                    "arguments": "{\"path\":\"src/tui/events.rs\"}"
                }
            }]
        }),
        json!({
            "role": "tool",
            "tool_call_id": "call_read",
            "content": "[src/tui/events.rs] (3639 lines, 133416 bytes)\nfn handle_slash_command(text: &str, app: &mut App, pane: PaneId) -> bool {\n    match cmd_lc.as_str() {\n        \"/realize\" => {"
        }),
    ];
    let evidence = collect_observation_evidence(&messages);

    let final_text = maybe_build_read_only_auto_final_answer(
        true,
        "Locate where the /realize slash command is handled in the TUI. Do not edit anything. Final answer must include the file path.",
        None,
        &evidence,
        &messages,
        &WorkingMemory::default(),
    )
    .expect("auto final answer with synthetic plan");

    assert!(final_text.starts_with("[DONE]"));
    assert!(final_text.contains("src/tui/events.rs"));
}

#[test]
fn root_read_only_detection_ignores_plan_drift_verbs() {
    assert!(is_root_read_only_observation_task(
        "Locate where the /realize slash command is handled in the TUI. Do not edit anything."
    ));
    assert!(!is_root_read_only_observation_task(
        "Fix the /realize command and update the handler implementation."
    ));
}

#[test]
fn first_slash_literal_extracts_command_token() {
    assert_eq!(
        first_slash_literal("Locate where the `/realize` slash command is handled in the TUI."),
        Some("/realize".to_string())
    );
}

#[test]
fn build_mistral_think_only_hint_suggests_real_tool() {
    let think = ThinkBlock {
        goal: "find direct references".to_string(),
        step: 1,
        tool: "search_files".to_string(),
        risk: "dynamic dispatch".to_string(),
        doubt: "might be indirect".to_string(),
        next: "search for /realize".to_string(),
        verify: "see a TUI match".to_string(),
    };
    let hint = build_mistral_think_only_hint(
        "Locate where the /realize slash command is handled in the TUI.",
        &think,
    );
    assert!(hint.contains("think` is not a real tool call"));
    assert!(hint.contains("search_files(pattern=\"/realize\", dir=\"src/tui\")"));
}

#[test]
fn parse_think_block_accepts_bracketed_mistral_fields() {
    let text = "<think>\ntruegoal[\"confirm /realize handler context\"]step[\"2: inspect matching file\"]tool[\"read_file\"]risk[\"false positive match\"]doubt[\"handler may be elsewhere\"]next[\"read src/tui/events.rs\"]verify[\"see the match arm\"]</think>";
    let think = parse_think_block(text).expect("think block");
    assert_eq!(think.goal, "confirm /realize handler context");
    assert_eq!(think.step, 2);
    assert_eq!(think.tool, "read_file");
    assert_eq!(think.next, "read src/tui/events.rs");
}

#[test]
fn build_read_only_plan_rewrite_hint_is_inspect_only() {
    let hint = build_read_only_plan_rewrite_hint(
        "Locate where the /realize slash command is handled in the TUI. Do not edit anything.",
    );
    assert!(hint.contains("search_files(pattern=\"/realize\", dir=\"src/tui\")"));
    assert!(hint.contains("call done once the file path and code context are confirmed"));
    assert!(hint.contains("Do NOT mention cargo test"));
    assert!(hint.contains("strictly inspect-only plan"));
}

#[test]
fn build_read_only_plan_rewrite_hint_uses_prefs_specific_search() {
    let hint = build_read_only_plan_rewrite_hint(
        "Find where pane-scoped TUI preferences are serialized and restored. Do not edit anything.",
    );
    assert!(hint.contains(
        "goal: Locate the main file where pane-scoped TUI preferences are serialized and restored."
    ));
    assert!(hint.contains("search_files(pattern=\"prefs\", dir=\"src/tui\")"));
    assert!(hint.contains("search_files(pattern=\"save_tui_prefs\", dir=\"src/tui\")"));
    assert!(hint.contains("serialize and restore context is confirmed by read_file"));
}

#[test]
fn synthetic_read_only_observation_plan_prefers_agent_flow_search_terms() {
    let plan = synthetic_read_only_observation_plan(
        "Find where the coder-side repo-map fallback for read_file misses is wired into the TUI agent flow. Do not edit anything.",
    );
    assert!(plan.goal.contains("repo-map fallback"));
    assert!(
        plan.steps
            .iter()
            .any(|step| step
                .contains("search_files(pattern=\"lazy_read_fallback\", dir=\"src/tui\")"))
    );
    assert!(plan
        .steps
        .iter()
        .any(|step| step.contains("search_files(pattern=\"repo_map\", dir=\"src/tui\")")));
    assert!(plan
        .acceptance_criteria
        .iter()
        .any(|item| item.contains("read_file miss handling context")));
}

#[test]
fn preferred_read_only_search_pattern_ignores_instruction_stopwords() {
    let pattern = preferred_read_only_search_pattern(
        "Find the failing test and explain why it fails. Do not edit anything. Final answer must include the file path and the exact mismatch.",
    );
    assert_eq!(pattern, "test");
}

#[test]
fn synthetic_read_only_observation_plan_captures_failure_mismatch_tasks() {
    let plan = synthetic_read_only_observation_plan(
        "Find the failing test and explain why it fails. Do not edit anything. Final answer must include the file path and the exact mismatch.",
    );
    assert!(plan.goal.contains("failing test"));
    assert!(plan.goal.contains("assertion mismatch"));
    assert!(plan
        .acceptance_criteria
        .iter()
        .any(|item| item.contains("failing test file path")));
    assert!(plan
        .acceptance_criteria
        .iter()
        .any(|item| item.contains("exact assertion mismatch")));
}

#[test]
fn compact_success_tool_result_for_history_keeps_exec_signal() {
    let content = "OK (exit_code: 0)\nduration_ms: 150\ncwd: /tmp/demo\nstdout:\nline 1\nline 2\nline 3\nline 4\nline 5\nline 6\nline 7\nline 8\nline 9\n[auto-test] ✓ PASSED (exit 0)\nfinished";
    let compact = compact_success_tool_result_for_history("exec", content);
    assert!(compact.contains("OK (exit_code: 0)"));
    assert!(compact.contains("[history digest"));
    assert!(compact.contains("[auto-test]"));
}

#[test]
fn validate_plan_for_task_rejects_behavioral_acceptance_for_read_only_tasks() {
    let plan = PlanBlock {
        goal: "locate slash command handler".to_string(),
        steps: vec![
            "search src".to_string(),
            "read src/tui/events.rs".to_string(),
        ],
        acceptance_criteria: vec![
            "The exact file path is identified".to_string(),
            "A behavioral test must confirm the /realize command works in the TUI.".to_string(),
        ],
        risks: "wrong file".to_string(),
        assumptions: "repo is local".to_string(),
    };

    assert!(validate_plan_for_task(&plan, true).is_err());
    assert!(validate_plan_for_task(&plan, false).is_ok());
}

#[test]
fn validate_done_acceptance_accepts_read_only_shorthand_evidence() {
    let plan = PlanBlock {
        goal: "locate slash command".to_string(),
        steps: vec![
            "inspect src".to_string(),
            "read src/tui/events.rs".to_string(),
        ],
        acceptance_criteria: vec![
            "The exact file path is identified".to_string(),
            "The handler context is confirmed".to_string(),
        ],
        risks: "wrong file".to_string(),
        assumptions: "repo indexed".to_string(),
    };

    let completed_acceptance = vec!["acceptance 1".to_string(), "acceptance 2".to_string()];
    let remaining_acceptance = Vec::new();
    let acceptance_evidence = vec![
        DoneAcceptanceEvidence {
            criterion: "acceptance 1".to_string(),
            command: "grep -n \"/realize\" src/tui/events.rs".to_string(),
        },
        DoneAcceptanceEvidence {
            criterion: "acceptance 2".to_string(),
            command: "read_file src/tui/events.rs".to_string(),
        },
    ];
    let known_commands = vec![
        "search_files(dir=src, pattern=/realize)".to_string(),
        "read_file(path=src/tui/events.rs)".to_string(),
    ];

    assert!(validate_done_acceptance(
        Some(&plan),
        &completed_acceptance,
        &remaining_acceptance,
        &acceptance_evidence,
        &known_commands,
        &ObservationEvidence::default(),
    )
    .is_ok());
}

#[test]
fn validate_done_acceptance_uses_resolution_memory_for_shorthand_paths() {
    let plan = PlanBlock {
        goal: "locate slash command".to_string(),
        steps: vec![
            "inspect src".to_string(),
            "read src/tui/events.rs".to_string(),
        ],
        acceptance_criteria: vec![
            "The exact file path is identified".to_string(),
            "The handler context is confirmed".to_string(),
        ],
        risks: "wrong file".to_string(),
        assumptions: "repo indexed".to_string(),
    };

    let completed_acceptance = vec!["acceptance 1".to_string(), "acceptance 2".to_string()];
    let remaining_acceptance = Vec::new();
    let acceptance_evidence = vec![
        DoneAcceptanceEvidence {
            criterion: "acceptance 1".to_string(),
            command: "grep -n \"/realize\" tui/events.rs".to_string(),
        },
        DoneAcceptanceEvidence {
            criterion: "acceptance 2".to_string(),
            command: "read_file(path=tui/events.rs)".to_string(),
        },
    ];
    let known_commands = vec![
        "search_files(dir=src/tui/events.rs, pattern=/realize)".to_string(),
        "read_file(path=src/tui/events.rs)".to_string(),
    ];
    let mut observation_evidence = ObservationEvidence::default();
    observation_evidence.remember_resolution(
        "tui/events.rs",
        "src/tui/events.rs",
        "repo_map:read_file",
    );

    assert!(validate_done_acceptance(
        Some(&plan),
        &completed_acceptance,
        &remaining_acceptance,
        &acceptance_evidence,
        &known_commands,
        &observation_evidence,
    )
    .is_ok());
}

#[test]
fn build_read_only_completion_hint_canonicalizes_resolution_memory_paths() {
    let plan = PlanBlock {
        goal: "Locate the /realize slash command handler in the TUI".to_string(),
        steps: vec![
            "search src".to_string(),
            "read the matching file".to_string(),
            "confirm the context".to_string(),
        ],
        acceptance_criteria: vec![
            "The exact file path containing the `/realize` slash command handler is identified."
                .to_string(),
            "The handler logic is confirmed to be part of the TUI component.".to_string(),
        ],
        risks: "wrong file".to_string(),
        assumptions: "repo indexed".to_string(),
    };
    let messages = vec![
        json!({
            "role": "assistant",
            "tool_calls": [{
                "id": "call_search",
                "type": "function",
                "function": {
                    "name": "search_files",
                    "arguments": "{\"pattern\":\"/realize\",\"dir\":\"src\"}"
                }
            }]
        }),
        json!({
            "role": "tool",
            "tool_call_id": "call_search",
            "content": "[search_files: '/realize' — 1 match(es)]\nsrc/tui/events.rs:465:         \"/realize\" => {"
        }),
        json!({
            "role": "assistant",
            "tool_calls": [{
                "id": "call_read",
                "type": "function",
                "function": {
                    "name": "read_file",
                    "arguments": "{\"path\":\"tui/events.rs\"}"
                }
            }]
        }),
        json!({
            "role": "tool",
            "tool_call_id": "call_read",
            "content": "[src/tui/events.rs] (2918 lines, 108025 bytes)\nfn handle_slash_command(text: &str, app: &mut App, pane: PaneId) -> bool {\n    match cmd_lc.as_str() {\n        \"/realize\" => {"
        }),
    ];

    let mut evidence = collect_observation_evidence(&messages);
    evidence.remember_resolution("tui/events.rs", "src/tui/events.rs", "repo_map:read_file");
    let hint = build_read_only_completion_hint(
        "Locate where the /realize slash command is handled in the TUI. Do not edit anything.",
        &plan,
        &evidence,
        &messages,
        &WorkingMemory::default(),
    )
    .expect("completion hint");

    assert!(hint.contains("read_file(path=src/tui/events.rs)"));
    assert!(!hint.contains("read_file(path=tui/events.rs)"));
}

#[test]
fn parses_plan_block_steps() {
    let text = "\
<plan>\n\
goal: make tests pass\n\
steps: 1) inspect failing test 2) patch the bug 3) run cargo test\n\
acceptance: 1) cargo test passes 2) bug reproduction no longer fails\n\
risks: flaky test and wrong file\n\
assumptions: repo already builds\n\
</plan>\n";
    let plan = parse_plan_block(text).expect("plan parsed");
    assert_eq!(plan.goal, "make tests pass");
    assert_eq!(plan.steps.len(), 3);
    assert_eq!(plan.acceptance_criteria.len(), 2);
    assert_eq!(plan.steps[0], "inspect failing test");
    assert_eq!(plan.steps[2], "run cargo test");
}

#[test]
fn parses_plan_block_with_colon_and_dash_numbering() {
    let text = "\
<plan>\n\
goal: harden governor parsing\n\
steps: 1: inspect parser drift 2- unify block parsing 3. run tests\n\
acceptance_criteria: 1: parser accepts contract aliases 2- tests pass\n\
risks: parser mismatch\n\
assumptions: shared contract is loaded\n\
</plan>\n";
    let plan = parse_plan_block(text).expect("plan parsed");
    assert_eq!(plan.steps.len(), 3);
    assert_eq!(plan.steps[1], "unify block parsing");
    assert_eq!(plan.acceptance_criteria.len(), 2);
    assert_eq!(
        plan.acceptance_criteria[0],
        "parser accepts contract aliases"
    );
}

#[test]
fn parses_xml_style_plan_block_fields() {
    let text = "\
<plan>\n\
<goal>locate the slash command handler</goal>\n\
<steps>1) search src 2) read the matching file 3) verify the handler</steps>\n\
<acceptance>1) exact file path identified 2) handler block confirmed</acceptance>\n\
<risks>wrong file or command aliasing</risks>\n\
<assumptions>the TUI code is local</assumptions>\n\
</plan>\n";
    let plan = parse_plan_block(text).expect("plan parsed");
    assert_eq!(plan.goal, "locate the slash command handler");
    assert_eq!(plan.steps.len(), 3);
    assert_eq!(plan.acceptance_criteria.len(), 2);
    assert_eq!(plan.steps[0], "search src");
    assert_eq!(plan.acceptance_criteria[1], "handler block confirmed");
}

#[test]
fn parses_plan_block_with_truncated_closing_tag() {
    let text = "\
<plan>\n\
goal: locate slash command\n\
steps: 1) search src 2) read the matching file\n\
acceptance: 1) path identified 2) handler confirmed\n\
risks: wrong file\n\
assumptions: repo indexed\n\
</plan";
    let plan = parse_plan_block(text).expect("plan parsed");
    assert_eq!(plan.goal, "locate slash command");
    assert_eq!(plan.steps.len(), 2);
    assert_eq!(plan.acceptance_criteria.len(), 2);
}

#[test]
fn validates_plan_requires_numbered_steps() {
    let plan = PlanBlock {
        goal: "ship fix".to_string(),
        steps: vec!["only one step".to_string()],
        acceptance_criteria: vec!["tests pass".to_string()],
        risks: "wrong file".to_string(),
        assumptions: "tests exist".to_string(),
    };
    assert!(validate_plan(&plan).is_err());
}

#[test]
fn validates_plan_requires_acceptance_criteria() {
    let plan = PlanBlock {
        goal: "ship fix".to_string(),
        steps: vec!["inspect".to_string(), "verify".to_string()],
        acceptance_criteria: vec![],
        risks: "wrong file".to_string(),
        assumptions: "tests exist".to_string(),
    };
    assert!(validate_plan(&plan).is_err());
}

#[test]
fn verification_level_from_plan_uses_acceptance_criteria() {
    let docs_plan = PlanBlock {
        goal: "update docs".to_string(),
        steps: vec!["edit readme".to_string(), "verify formatting".to_string()],
        acceptance_criteria: vec![
            "README wording is updated".to_string(),
            "markdown renders cleanly".to_string(),
        ],
        risks: "wrong file".to_string(),
        assumptions: "docs build exists".to_string(),
    };
    let code_plan = PlanBlock {
        goal: "fix runtime bug".to_string(),
        steps: vec![
            "inspect".to_string(),
            "patch".to_string(),
            "verify".to_string(),
        ],
        acceptance_criteria: vec![
            "cargo test passes".to_string(),
            "runtime gate blocks repeated failure".to_string(),
        ],
        risks: "wrong file".to_string(),
        assumptions: "tests exist".to_string(),
    };

    assert_eq!(
        verification_level_from_plan(&docs_plan),
        Some(VerificationLevel::Build)
    );
    assert_eq!(
        verification_level_from_plan(&code_plan),
        Some(VerificationLevel::Behavioral)
    );
}

#[test]
fn parses_think_block_fields() {
    let text = "\
<think>\n\
goal: verify the fix\n\
step: 3\n\
tool: exec\n\
risk: wrong test target\n\
doubt: maybe cargo check is enough\n\
next: cargo test\n\
verify: exit code is zero\n\
</think>\n";
    let think = parse_think_block(text).expect("think parsed");
    assert_eq!(think.step, 3);
    assert_eq!(think.tool, "exec");
    assert_eq!(think.next, "cargo test");
}

#[test]
fn parses_xml_style_think_block_fields() {
    let text = "\
<think>\n\
<goal>find the realize handler</goal>\n\
<step>1 (search for the slash command)</step>\n\
<tool>search_files</tool>\n\
<risk>wrong path scope</risk>\n\
<doubt>could be registered indirectly</doubt>\n\
<next>search /realize inside src</next>\n\
<verify>match appears in a TUI file</verify>\n\
</think>\n";
    let think = parse_think_block(text).expect("think parsed");
    assert_eq!(think.step, 1);
    assert_eq!(think.tool, "search_files");
    assert_eq!(think.next, "search /realize inside src");
}

#[test]
fn pseudo_plan_tool_call_converts_to_plan_block_text() {
    let tc = ToolCallData {
        thought_signature: None,
        id: "call_1".to_string(),
        name: "plan".to_string(),
        arguments: serde_json::json!({
            "goal": "locate handler",
            "steps": ["search code", "read file", "verify"],
            "acceptance": ["path identified", "handler confirmed"],
            "risks": "wrong file",
            "assumptions": "code is local"
        })
        .to_string(),
    };

    let text = pseudo_tool_call_to_block_text(&tc).expect("block text");
    let plan = parse_plan_block(&text).expect("plan parsed");
    assert_eq!(plan.goal, "locate handler");
    assert_eq!(plan.steps.len(), 3);
    assert_eq!(plan.acceptance_criteria.len(), 2);
}

#[test]
fn pseudo_think_tool_call_converts_to_think_block_text() {
    let tc = ToolCallData {
        thought_signature: None,
        id: "call_1".to_string(),
        name: "think".to_string(),
        arguments: serde_json::json!({
            "goal": "inspect the source",
            "step": "2",
            "tool": "read_file",
            "risk": "wrong path",
            "doubt": "maybe router is elsewhere",
            "next": "read src/tui/events.rs",
            "verify": "command returns file contents"
        })
        .to_string(),
    };

    let text = pseudo_tool_call_to_block_text(&tc).expect("block text");
    let think = parse_think_block(&text).expect("think parsed");
    assert_eq!(think.step, 2);
    assert_eq!(think.tool, "read_file");
}

#[test]
fn normalize_mistral_tool_call_extracts_sidecar_plan_from_arguments() {
    let plan = serde_json::json!({
        "goal": "locate handler",
        "steps": ["search code", "read file", "verify"],
        "acceptance": ["path identified"],
        "risks": "wrong file",
        "assumptions": "repo is local"
    })
    .to_string();
    let args = serde_json::json!({"pattern":"/realize","dir":"src"}).to_string();
    let tc = ToolCallData {
        thought_signature: None,
        id: "call_1".to_string(),
        name: "search_files".to_string(),
        arguments: format!("{plan}{args}"),
    };

    let (blocks, normalized) = normalize_mistral_tool_call(&tc);
    assert_eq!(blocks.len(), 1);
    let plan_block = parse_plan_block(&blocks[0]).expect("plan parsed");
    assert_eq!(plan_block.goal, "locate handler");
    let normalized = normalized.expect("real tool preserved");
    assert_eq!(normalized.name, "search_files");
    assert_eq!(
        normalized.arguments,
        serde_json::json!({"pattern":"/realize","dir":"src"}).to_string()
    );
}

#[test]
fn normalize_mistral_tool_call_extracts_inline_plan_name_and_nested_tool() {
    let tc = ToolCallData {
        thought_signature: None,
        id: "call_1".to_string(),
        name: "plan>goal: locate handler steps: 1) search src 2) read file acceptance: 1) path identified risks: wrong file assumptions: repo is local </plan>".to_string(),
        arguments: serde_json::json!({
            "tool": "search_files",
            "arguments": {"pattern": "/realize", "dir": "src"}
        })
        .to_string(),
    };

    let (blocks, normalized) = normalize_mistral_tool_call(&tc);
    assert_eq!(blocks.len(), 1);
    let plan = parse_plan_block(&blocks[0]).expect("plan parsed");
    assert_eq!(plan.goal, "locate handler");
    let normalized = normalized.expect("real tool preserved");
    assert_eq!(normalized.name, "search_files");
    assert_eq!(
        normalized.arguments,
        serde_json::json!({"pattern":"/realize","dir":"src"}).to_string()
    );
}

#[test]
fn normalize_mistral_tool_call_extracts_inline_plan_with_function_wrapper_arguments() {
    let tc = ToolCallData {
        thought_signature: None,
        id: "call_1".to_string(),
        name: "plan><goal>locate handler</goal><steps>1) search src 2) read file</steps><acceptance>1) path identified</acceptance><risks>wrong file</risks><assumptions>repo is local</assumptions></plan>".to_string(),
        arguments: serde_json::json!({
            "type": "function",
            "function": {
                "name": "search_files",
                "parameters": {"pattern": "/realize", "dir": "src"}
            }
        })
        .to_string(),
    };

    let (blocks, normalized) = normalize_mistral_tool_call(&tc);
    assert_eq!(blocks.len(), 1);
    let plan = parse_plan_block(&blocks[0]).expect("plan parsed");
    assert_eq!(plan.goal, "locate handler");
    let normalized = normalized.expect("real tool preserved");
    assert_eq!(normalized.name, "search_files");
    assert_eq!(
        normalized.arguments,
        serde_json::json!({"pattern":"/realize","dir":"src"}).to_string()
    );
}

#[test]
fn normalize_mistral_tool_call_extracts_inline_plan_and_nested_think() {
    let tc = ToolCallData {
        thought_signature: None,
        id: "call_1".to_string(),
        name: "plan>goal: locate handler steps: 1) search src 2) read file acceptance: 1) path identified risks: wrong file assumptions: repo is local </plan>".to_string(),
        arguments: serde_json::json!({
            "think": "goal: inspect slash command\nstep: 1\ntool: search_files\nrisk: wrong path\ndoubt: maybe aliased\nnext: search /realize\nverify: find a match",
            "tool": "search_files",
            "arguments": {"pattern": "/realize", "dir": "src"}
        })
        .to_string(),
    };

    let (blocks, normalized) = normalize_mistral_tool_call(&tc);
    assert_eq!(blocks.len(), 2);
    let plan = parse_plan_block(&blocks[0]).expect("plan parsed");
    let think = parse_think_block(&blocks[1]).expect("think parsed");
    assert_eq!(plan.goal, "locate handler");
    assert_eq!(think.tool, "search_files");
    let normalized = normalized.expect("real tool preserved");
    assert_eq!(normalized.name, "search_files");
}

#[test]
fn normalize_mistral_tool_call_extracts_markdownish_plan_name_and_wrapper_tool() {
    let tc = ToolCallData {
        thought_signature: None,
        id: "call_1".to_string(),
        name: "plan**goal:** Locate where the `/realize` slash command is handled in the TUI. **•**steps:** 1) Use `search_files` to search for the literal string `/realize` in `src`. 2) Read the matching file. **•**acceptance:** 1) The file path handling `/realize` is identified. 2) The handler branch is confirmed by `read_file`. **•**risks:** wrong file. **•**assumptions:** observation tools are sufficient. <thinking> **goal:** Find direct matches. **step:** 1. **tool:** `search_files`. **risk:** wrong path. **doubt:** maybe aliased. **next:** Search `/realize` in `src`. **verify:** confirm a TUI match. </thinking>".to_string(),
        arguments: serde_json::json!({
            "type": "function",
            "function": {
                "name": "search_files",
                "parameters": {"pattern": "/realize", "dir": "src"}
            }
        })
        .to_string(),
    };

    let (blocks, normalized) = normalize_mistral_tool_call(&tc);
    assert_eq!(blocks.len(), 1);
    let plan = parse_plan_block(&blocks[0]).expect("plan parsed");
    assert!(plan.goal.contains("/realize"));
    let normalized = normalized.expect("real tool preserved");
    assert_eq!(normalized.name, "search_files");
    assert_eq!(
        normalized.arguments,
        serde_json::json!({"pattern":"/realize","dir":"src"}).to_string()
    );
}

#[test]
fn normalize_mistral_tool_call_extracts_embedded_plan_think_and_tool_name() {
    let tc = ToolCallData {
        thought_signature: None,
        id: "call_1".to_string(),
        name: "plan><goal>Locate the exact file and code context where the /realize slash command is handled in the TUI.</goal><steps>1) List the TUI-related directories to confirm structure and naming conventions.</steps><steps>2) Search for the literal \"/realize\" string across Rust files to pinpoint the handler.</steps><acceptance>1) The file path containing the /realize slash command handler is confirmed by search_files evidence.</acceptance><acceptance>2) The handling context is confirmed by read_file evidence.</acceptance><risks>wrong file</risks><assumptions>repo is local</assumptions></plan>teří<think><goal>Confirm TUI directory structure and naming.</goal><step>1</step><tool>list_dir</tool><risk>TUI directory may not exist.</risk><doubt>Directory names may vary.</doubt><next>list_dir src/</next><verify>Directory listing shows TUI directory.</verify></think>teřílist_dir".to_string(),
        arguments: serde_json::json!({"dir":"src"}).to_string(),
    };

    let (blocks, normalized) = normalize_mistral_tool_call(&tc);
    assert_eq!(blocks.len(), 2);
    let plan = parse_plan_block(&blocks[0]).expect("plan parsed");
    let think = parse_think_block(&blocks[1]).expect("think parsed");
    assert!(plan.goal.contains("/realize"));
    assert_eq!(think.tool, "list_dir");
    let normalized = normalized.expect("tool restored");
    assert_eq!(normalized.name, "list_dir");
    assert_eq!(
        normalized.arguments,
        serde_json::json!({"dir":"src"}).to_string()
    );
}

#[test]
fn consecutive_missing_plan_blocks_for_tool_counts_same_observation_tool() {
    let tc = ToolCallData {
        thought_signature: None,
        id: "call_3".to_string(),
        name: "search_files".to_string(),
        arguments: serde_json::json!({"pattern":"/realize","dir":"src"}).to_string(),
    };
    let messages = vec![
        json!({
            "role": "assistant",
            "content": "",
            "tool_calls": [{
                "id": "call_1",
                "type": "function",
                "function": {
                    "name": "search_files",
                    "arguments": serde_json::json!({"pattern":"/realize","dir":"src"}).to_string()
                }
            }]
        }),
        json!({
            "role": "tool",
            "tool_call_id": "call_1",
            "content": "GOVERNOR BLOCKED\n\n[Plan Gate] Missing valid <plan>.\n\ntool:\nsearch_files\narguments:\n{\"pattern\":\"/realize\",\"dir\":\"src\"}"
        }),
        json!({
            "role": "assistant",
            "content": "",
            "tool_calls": [{
                "id": "call_2",
                "type": "function",
                "function": {
                    "name": "search_files",
                    "arguments": serde_json::json!({"pattern":"/realize","dir":"src"}).to_string()
                }
            }]
        }),
        json!({
            "role": "tool",
            "tool_call_id": "call_2",
            "content": "GOVERNOR BLOCKED\n\n[Plan Gate] Missing valid <plan>.\n\ntool:\nsearch_files\narguments:\n{\"pattern\":\"/realize\",\"dir\":\"src\"}"
        }),
    ];

    assert_eq!(consecutive_missing_plan_blocks_for_tool(&messages, &tc), 2);
}

#[test]
fn consecutive_missing_plan_blocks_for_observation_counts_mixed_tools() {
    let messages = vec![
        json!({
            "role": "assistant",
            "content": "",
            "tool_calls": [{
                "id": "call_1",
                "type": "function",
                "function": {
                    "name": "search_files",
                    "arguments": serde_json::json!({"pattern":"prefs","dir":"src/tui"}).to_string()
                }
            }]
        }),
        json!({
            "role": "tool",
            "tool_call_id": "call_1",
            "content": "GOVERNOR BLOCKED\n\n[Plan Gate] Missing valid <plan>.\n\ntool:\nsearch_files\narguments:\n{\"pattern\":\"prefs\",\"dir\":\"src/tui\"}"
        }),
        json!({
            "role": "assistant",
            "content": "",
            "tool_calls": [{
                "id": "call_2",
                "type": "function",
                "function": {
                    "name": "list_dir",
                    "arguments": serde_json::json!({"dir":"src/tui"}).to_string()
                }
            }]
        }),
        json!({
            "role": "tool",
            "tool_call_id": "call_2",
            "content": "GOVERNOR BLOCKED\n\n[Plan Gate] Missing valid <plan>.\n\ntool:\nlist_dir\narguments:\n{\"dir\":\"src/tui\"}"
        }),
    ];

    assert_eq!(
        consecutive_missing_plan_blocks_for_observation(&messages),
        2
    );
}

#[test]
fn rescue_read_only_missing_plan_for_tool_turn_after_repeated_blocks() {
    let tc = ToolCallData {
        thought_signature: None,
        id: "call_2".to_string(),
        name: "search_files".to_string(),
        arguments: serde_json::json!({"pattern":"/realize","dir":"src"}).to_string(),
    };
    let messages = vec![
        json!({
            "role": "assistant",
            "content": "",
            "tool_calls": [{
                "id": "call_1",
                "type": "function",
                "function": {
                    "name": "search_files",
                    "arguments": serde_json::json!({"pattern":"/realize","dir":"src"}).to_string()
                }
            }]
        }),
        json!({
            "role": "tool",
            "tool_call_id": "call_1",
            "content": "GOVERNOR BLOCKED\n\n[Plan Gate] Missing valid <plan>.\n\ntool:\nsearch_files\narguments:\n{\"pattern\":\"/realize\",\"dir\":\"src\"}"
        }),
    ];

    let rescued = rescue_read_only_missing_plan_for_tool_turn(
        &messages,
        &tc,
        "Locate where the /realize slash command is handled in the TUI. Do not edit anything.",
        true,
        ProviderKind::OpenAiCompatible,
    )
    .expect("rescue plan");
    assert!(rescued.goal.contains("/realize"));
    assert!(rescued
        .acceptance_criteria
        .iter()
        .any(|item| item.contains("handler branch")));
}

#[test]
fn rescue_read_only_missing_plan_for_tool_turn_after_mixed_observation_blocks() {
    let tc = ToolCallData {
        thought_signature: None,
        id: "call_3".to_string(),
        name: "search_files".to_string(),
        arguments: serde_json::json!({"pattern":"prefs","dir":"src/tui"}).to_string(),
    };
    let messages = vec![
        json!({
            "role": "assistant",
            "content": "",
            "tool_calls": [{
                "id": "call_1",
                "type": "function",
                "function": {
                    "name": "search_files",
                    "arguments": serde_json::json!({"pattern":"prefs","dir":"src/tui"}).to_string()
                }
            }]
        }),
        json!({
            "role": "tool",
            "tool_call_id": "call_1",
            "content": "GOVERNOR BLOCKED\n\n[Plan Gate] Missing valid <plan>.\n\ntool:\nsearch_files\narguments:\n{\"pattern\":\"prefs\",\"dir\":\"src/tui\"}"
        }),
        json!({
            "role": "assistant",
            "content": "",
            "tool_calls": [{
                "id": "call_2",
                "type": "function",
                "function": {
                    "name": "list_dir",
                    "arguments": serde_json::json!({"dir":"src/tui"}).to_string()
                }
            }]
        }),
        json!({
            "role": "tool",
            "tool_call_id": "call_2",
            "content": "GOVERNOR BLOCKED\n\n[Plan Gate] Missing valid <plan>.\n\ntool:\nlist_dir\narguments:\n{\"dir\":\"src/tui\"}"
        }),
    ];

    let rescued = rescue_read_only_missing_plan_for_tool_turn(
        &messages,
        &tc,
        "Find where pane-scoped TUI preferences are serialized and restored. Do not edit anything.",
        true,
        ProviderKind::OpenAiCompatible,
    )
    .expect("rescue plan");
    assert!(rescued.goal.contains("pane-scoped TUI preferences"));
    assert!(rescued
        .steps
        .iter()
        .any(|step| step.contains("search_files(pattern=\"prefs\", dir=\"src/tui\")")));
}

#[test]
fn rescue_missing_plan_for_tool_turn_after_repeated_blocks_for_openai_actions() {
    let tc = ToolCallData {
        thought_signature: None,
        id: "call_2".to_string(),
        name: "read_file".to_string(),
        arguments: serde_json::json!({"path":"Cargo.toml"}).to_string(),
    };
    let messages = vec![
        json!({
            "role": "assistant",
            "content": "",
            "tool_calls": [{
                "id": "call_1",
                "type": "function",
                "function": {
                    "name": "read_file",
                    "arguments": serde_json::json!({"path":"Cargo.toml"}).to_string()
                }
            }]
        }),
        json!({
            "role": "tool",
            "tool_call_id": "call_1",
            "content": "GOVERNOR BLOCKED\n\n[Plan Gate] Missing valid <plan>.\n\ntool:\nread_file\narguments:\n{\"path\":\"Cargo.toml\"}"
        }),
    ];

    let rescued = rescue_missing_plan_for_tool_turn(
        &messages,
        &tc,
        "Fix the failing test with the smallest code change. Run tests before you finish.",
        TaskHarness::infer(
            "Fix the failing test with the smallest code change. Run tests before you finish.",
            false,
        ),
        false,
        true,
        ProviderKind::OpenAiCompatible,
        VerificationLevel::Behavioral,
        Some("cargo test 2>&1"),
    )
    .expect("synthetic action plan");

    assert!(rescued.goal.contains("Fix the failing test"));
    assert!(rescued
        .steps
        .iter()
        .any(|step| step.contains("read_file(path=Cargo.toml)")));
    assert!(rescued
        .steps
        .iter()
        .any(|step| step.contains("cargo test 2>&1")));
    assert!(rescued
        .acceptance_criteria
        .iter()
        .all(|item| item.contains("behavioral verification")));
}

#[test]
fn general_plan_rescue_pairs_with_valid_synthetic_think() {
    let tc = ToolCallData {
        thought_signature: None,
        id: "call_2".to_string(),
        name: "search_files".to_string(),
        arguments: serde_json::json!({"pattern":"test","dir":"src"}).to_string(),
    };
    let messages = vec![
        json!({
            "role": "assistant",
            "content": "",
            "tool_calls": [{
                "id": "call_1",
                "type": "function",
                "function": {
                    "name": "search_files",
                    "arguments": serde_json::json!({"pattern":"test","dir":"src"}).to_string()
                }
            }]
        }),
        json!({
            "role": "tool",
            "tool_call_id": "call_1",
            "content": "GOVERNOR BLOCKED\n\n[Plan Gate] Missing valid <plan>.\n\ntool:\nsearch_files\narguments:\n{\"pattern\":\"test\",\"dir\":\"src\"}"
        }),
    ];

    let plan = rescue_missing_plan_for_tool_turn(
        &messages,
        &tc,
        "Fix the failing test with the smallest code change. Run tests before you finish.",
        TaskHarness::infer(
            "Fix the failing test with the smallest code change. Run tests before you finish.",
            false,
        ),
        false,
        true,
        ProviderKind::OpenAiCompatible,
        VerificationLevel::Behavioral,
        Some("cargo test 2>&1"),
    )
    .expect("synthetic action plan");
    let think = compat_synthetic_think(&tc, &plan);

    validate_think(&think, &plan, &tc).expect("valid synthetic think");
    assert_eq!(think.tool, "search_files");
    assert!(think.next.contains("search"));
}

#[test]
fn rescue_missing_plan_for_create_file_accepts_write_file_turn() {
    let prompt =
        "Create `notes/todo.txt` containing exactly `ship it`. Verify it before you finish.";
    let tc = ToolCallData {
        thought_signature: None,
        id: "call_write_retry".to_string(),
        name: "write_file".to_string(),
        arguments: serde_json::json!({
            "path":"notes/todo.txt",
            "content":"ship it\n"
        })
        .to_string(),
    };
    let messages = vec![
        json!({
            "role": "assistant",
            "content": "",
            "tool_calls": [{
                "id": "call_write_1",
                "type": "function",
                "function": {
                    "name": "write_file",
                    "arguments": serde_json::json!({
                        "path":"notes/todo.txt",
                        "content":"ship it\n"
                    }).to_string()
                }
            }]
        }),
        json!({
            "role": "tool",
            "tool_call_id": "call_write_1",
            "content": "GOVERNOR BLOCKED\n\n[Plan Gate] Missing valid <plan>.\n\ntool:\nwrite_file\narguments:\n{\"path\":\"notes/todo.txt\",\"content\":\"ship it\\n\"}"
        }),
    ];

    let rescued = rescue_missing_plan_for_tool_turn(
        &messages,
        &tc,
        prompt,
        TaskHarness::infer(prompt, false),
        false,
        true,
        ProviderKind::OpenAiCompatible,
        VerificationLevel::Behavioral,
        Some("test -f notes/todo.txt && grep -Fx \"ship it\" notes/todo.txt"),
    )
    .expect("synthetic create-file plan");

    assert!(rescued
        .steps
        .first()
        .is_some_and(|step| step.contains("write_file(") && step.contains("notes/todo.txt")));
    assert!(rescued
        .acceptance_criteria
        .iter()
        .any(|item| item.contains("artifact")));
    assert!(rescued
        .acceptance_criteria
        .iter()
        .any(|item| item.contains("behavioral verification")));
}

#[test]
fn rescue_missing_think_for_tool_turn_after_repeated_blocks_for_openai_actions() {
    let tc = ToolCallData {
        thought_signature: None,
        id: "call_2".to_string(),
        name: "list_dir".to_string(),
        arguments: serde_json::json!({"dir":"."}).to_string(),
    };
    let plan = rescue_missing_plan_for_tool_turn(
        &[json!({
            "role": "assistant",
            "content": "",
            "tool_calls": [{
                "id": "call_1",
                "type": "function",
                "function": {
                    "name": "read_file",
                    "arguments": serde_json::json!({"path":"Cargo.toml"}).to_string()
                }
            }]
        }), json!({
            "role": "tool",
            "tool_call_id": "call_1",
            "content": "GOVERNOR BLOCKED\n\n[Plan Gate] Missing valid <plan>.\n\ntool:\nread_file\narguments:\n{\"path\":\"Cargo.toml\"}"
        })],
        &ToolCallData {
            thought_signature: None,
            id: "call_plan".to_string(),
            name: "read_file".to_string(),
            arguments: serde_json::json!({"path":"Cargo.toml"}).to_string(),
        },
        "Fix the failing test with the smallest code change. Run tests before you finish.",
        TaskHarness::infer(
            "Fix the failing test with the smallest code change. Run tests before you finish.",
            false,
        ),
        false,
        true,
        ProviderKind::OpenAiCompatible,
        VerificationLevel::Behavioral,
        Some("cargo test 2>&1"),
    )
    .expect("synthetic action plan");
    let messages = vec![
        json!({
            "role": "assistant",
            "content": "",
            "tool_calls": [{
                "id": "call_1",
                "type": "function",
                "function": {
                    "name": "list_dir",
                    "arguments": serde_json::json!({"dir":"."}).to_string()
                }
            }]
        }),
        json!({
            "role": "tool",
            "tool_call_id": "call_1",
            "content": "GOVERNOR BLOCKED\n\n[Think Gate] Missing <think>.\n\ntool:\nlist_dir\narguments:\n{\"dir\":\".\"}"
        }),
    ];

    let rescued = rescue_missing_think_for_tool_turn(
        &messages,
        &tc,
        &plan,
        false,
        true,
        ProviderKind::OpenAiCompatible,
        false,
    )
    .expect("synthetic think");

    validate_think(&rescued, &plan, &tc).expect("valid synthetic think");
    assert_eq!(rescued.tool, "list_dir");
}

#[test]
fn rescue_missing_think_for_tool_turn_is_immediate_for_patch_file() {
    let tc = ToolCallData {
        thought_signature: None,
        id: "call_patch".to_string(),
        name: "patch_file".to_string(),
        arguments: serde_json::json!({
            "path":"src/lib.rs",
            "search":"Hello, {}?",
            "replace":"Hello, {}!"
        })
        .to_string(),
    };
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
        risks: "wrong file or speculative fix; the chosen test may miss the requested behavior".to_string(),
        assumptions: "the current repo state matches the task and the configured verification command is relevant".to_string(),
    };
    let messages = vec![json!({
        "role": "tool",
        "tool_call_id": "call_patch",
        "content": "GOVERNOR BLOCKED\n\n[Think Gate] Missing <think>.\n\ntool:\npatch_file\narguments:\n{\"path\":\"src/lib.rs\",\"search\":\"Hello, {}?\",\"replace\":\"Hello, {}!\"}"
    })];

    let rescued = rescue_missing_think_for_tool_turn(
        &messages,
        &tc,
        &plan,
        false,
        true,
        ProviderKind::OpenAiCompatible,
        false,
    )
    .expect("synthetic think");

    validate_think(&rescued, &plan, &tc).expect("valid synthetic think");
    assert_eq!(rescued.tool, "patch_file");
}

#[test]
fn rescue_missing_think_for_tool_turn_is_immediate_for_exec() {
    let tc = ToolCallData {
        thought_signature: None,
        id: "call_exec".to_string(),
        name: "exec".to_string(),
        arguments: serde_json::json!({"command":"cargo test 2>&1"}).to_string(),
    };
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
        risks: "wrong file or speculative fix; the chosen test may miss the requested behavior".to_string(),
        assumptions: "the current repo state matches the task and the configured verification command is relevant".to_string(),
    };
    let messages = vec![json!({
        "role": "tool",
        "tool_call_id": "call_exec",
        "content": "GOVERNOR BLOCKED\n\n[Think Gate] Missing <think>.\n\ntool:\nexec\narguments:\n{\"command\":\"cargo test 2>&1\"}"
    })];

    let rescued = rescue_missing_think_for_tool_turn(
        &messages,
        &tc,
        &plan,
        false,
        true,
        ProviderKind::OpenAiCompatible,
        false,
    )
    .expect("synthetic think");

    validate_think(&rescued, &plan, &tc).expect("valid synthetic think");
    assert_eq!(rescued.tool, "exec");
}

#[test]
fn rescue_missing_think_for_tool_turn_is_immediate_for_done() {
    let tc = ToolCallData {
        thought_signature: None,
        id: "call_done".to_string(),
        name: "done".to_string(),
        arguments: serde_json::json!({
            "summary":"Fixed the bug and verified it.",
            "completed_acceptance":["1) the requested change is implemented"],
            "remaining_acceptance":["2) the final result cites the exact passing behavioral verification command"],
            "acceptance_evidence":[{"criterion":"1) the requested change is implemented","command":"cargo test 2>&1"}]
        })
        .to_string(),
    };
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
        risks: "wrong file or speculative fix; the chosen test may miss the requested behavior".to_string(),
        assumptions: "the current repo state matches the task and the configured verification command is relevant".to_string(),
    };

    let rescued = rescue_missing_think_for_tool_turn(
        &[],
        &tc,
        &plan,
        false,
        true,
        ProviderKind::OpenAiCompatible,
        false,
    )
    .expect("synthetic think");

    validate_think(&rescued, &plan, &tc).expect("valid synthetic think");
    assert_eq!(rescued.tool, "done");
    assert!(rescued.next.contains("done"));
}

#[test]
fn rescue_missing_reflection_for_tool_turn_synthesizes_valid_reflection_for_patch() {
    let tc = ToolCallData {
        thought_signature: None,
        id: "call_patch".to_string(),
        name: "patch_file".to_string(),
        arguments: serde_json::json!({
            "path":"src/lib.rs",
            "search":"Hello, {}?",
            "replace":"Hello, {}!"
        })
        .to_string(),
    };
    let mut mem = FailureMemory::default();
    mem.consecutive_failures = 1;

    let reflect = rescue_missing_reflection_for_tool_turn(
        "⚠ LOGIC ERROR: The code ran but produced wrong results.",
        &tc,
        false,
        true,
        ProviderKind::OpenAiCompatible,
        &mem,
        0,
    )
    .expect("synthetic reflection");

    validate_reflection(&reflect, &mem, 0).expect("valid synthetic reflection");
    assert_eq!(reflect.last_outcome, "failure");
    assert_eq!(reflect.strategy_change, StrategyChange::Adjust);
    assert!(reflect.next_minimal_action.contains("src/lib.rs"));
}

#[test]
fn rescue_missing_impact_for_tool_turn_synthesizes_valid_impact_after_patch() {
    let tc = ToolCallData {
        thought_signature: None,
        id: "call_exec".to_string(),
        name: "exec".to_string(),
        arguments: serde_json::json!({"command":"cargo test 2>&1"}).to_string(),
    };
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
        risks: "wrong file or speculative fix; the chosen test may miss the requested behavior".to_string(),
        assumptions: "the current repo state matches the task and the configured verification command is relevant".to_string(),
    };

    let impact = rescue_missing_impact_for_tool_turn(
        "successful mutation via patch_file: src/lib.rs",
        &tc,
        false,
        true,
        ProviderKind::OpenAiCompatible,
        Some(&plan),
        impact_recovery::UnreviewedMutation::from_steps(Some(1), None),
    )
    .expect("synthetic impact");

    validate_impact(&impact, Some(&plan)).expect("valid synthetic impact");
    assert!(impact.changed.contains("src/lib.rs"));
    assert!(impact.progress.contains("step 2"));
    assert!(impact.remaining_gap.contains("cargo test 2>&1"));
}

#[test]
fn rescue_missing_evidence_for_tool_turn_synthesizes_valid_evidence_for_patch() {
    let tc = ToolCallData {
        thought_signature: None,
        id: "call_patch".to_string(),
        name: "patch_file".to_string(),
        arguments: serde_json::json!({
            "path":"src/lib.rs",
            "search":"Hello, {}?",
            "replace":"Hello, {}!"
        })
        .to_string(),
    };
    let observations = ObservationEvidence {
        reads: vec![ObservationReadEvidence {
            command: "read_file(path=src/lib.rs)".to_string(),
            path: "src/lib.rs".to_string(),
        }],
        searches: Vec::new(),
        resolutions: Vec::new(),
    };

    let evidence = rescue_missing_evidence_for_tool_turn(
        &tc,
        false,
        true,
        ProviderKind::OpenAiCompatible,
        &observations,
    )
    .expect("synthetic evidence");

    validate_evidence_block(&evidence, &tc, &observations).expect("valid synthetic evidence");
    assert_eq!(evidence.target_files, vec!["src/lib.rs".to_string()]);
    assert!(evidence.evidence.contains("read_file(path=src/lib.rs)"));
}

#[test]
fn rescue_missing_evidence_for_tool_turn_accepts_truncated_patch_path() {
    let tc = ToolCallData {
        thought_signature: None,
        id: "call_patch".to_string(),
        name: "patch_file".to_string(),
        arguments:
            "{\"path\":\"src/observer/repo_rules.rs\",\"search\":\"const TUI_REPLAY_PATHS: &["
                .to_string(),
    };
    let observations = ObservationEvidence {
        reads: vec![ObservationReadEvidence {
            command: "read_file(path=src/observer/repo_rules.rs)".to_string(),
            path: "src/observer/repo_rules.rs".to_string(),
        }],
        searches: Vec::new(),
        resolutions: Vec::new(),
    };

    let evidence = rescue_missing_evidence_for_tool_turn(
        &tc,
        false,
        true,
        ProviderKind::OpenAiCompatible,
        &observations,
    )
    .expect("synthetic evidence for truncated patch");

    validate_evidence_block(&evidence, &tc, &observations).expect("valid synthetic evidence");
    assert_eq!(
        evidence.target_files,
        vec!["src/observer/repo_rules.rs".to_string()]
    );
}

#[test]
fn repair_truncated_patch_tool_call_from_recent_mismatch_recovers_smoke_fix() {
    let tc = ToolCallData {
        thought_signature: None,
        id: "call_patch".to_string(),
        name: "patch_file".to_string(),
        arguments: "{\"path\":\"src/lib.rs\",\"search\":\"pub fn greet(name: &str) -> String {\\n    format!(\\\"Hello, {}?\\\", name)\\n}\\n\",\"replace\":\"".to_string(),
    };
    let messages = vec![json!({
        "role": "tool",
        "tool_call_id": "call_exec",
        "content": "thread 'tests::greet_is_excited' panicked at src/lib.rs:12:9:\nassertion `left == right` failed\n  left: \"Hello, Ada?\"\n right: \"Hello, Ada!\""
    })];
    let observations = ObservationEvidence {
        reads: vec![ObservationReadEvidence {
            command: "read_file(path=src/lib.rs)".to_string(),
            path: "src/lib.rs".to_string(),
        }],
        searches: Vec::new(),
        resolutions: Vec::new(),
    };

    let mut signed = tc.clone();
    signed.thought_signature = crate::streaming::provider_metadata::ThoughtSignature::from_call(
        &json!({"extra_content":{"google":{"thought_signature":"opaque-test-signature"}}}),
    )
    .unwrap();
    assert!(
        repair_truncated_patch_tool_call_from_recent_mismatch(
            &messages,
            &signed,
            false,
            true,
            ProviderKind::OpenAiCompatible,
            &observations,
        )
        .is_none(),
        "A malformed signed call must be rejected, not silently rewritten"
    );

    let (rewritten, original, repaired) = repair_truncated_patch_tool_call_from_recent_mismatch(
        &messages,
        &tc,
        false,
        true,
        ProviderKind::OpenAiCompatible,
        &observations,
    )
    .expect("repair malformed patch tool call");

    assert!(original.starts_with("patch_file"));
    assert!(original.contains("\"replace\":\""));
    assert!(repaired.starts_with("patch_file("));
    assert!(repaired.contains("Hello, {}!"));
    assert_eq!(rewritten.name, "patch_file");
    let rewritten_args =
        serde_json::from_str::<serde_json::Value>(&rewritten.arguments).expect("valid json");
    assert_eq!(
        rewritten_args.get("path").and_then(|v| v.as_str()),
        Some("src/lib.rs")
    );
    assert_eq!(
        rewritten_args.get("replace").and_then(|v| v.as_str()),
        Some("pub fn greet(name: &str) -> String {\n    format!(\"Hello, {}!\", name)\n}\n")
    );
}

#[test]
fn consecutive_missing_think_blocks_for_tool_counts_same_observation_tool() {
    let tc = ToolCallData {
        thought_signature: None,
        id: "call_3".to_string(),
        name: "search_files".to_string(),
        arguments: serde_json::json!({"pattern":"realize","dir":"src"}).to_string(),
    };
    let messages = vec![
        json!({
            "role": "assistant",
            "content": "",
            "tool_calls": [{
                "id": "call_1",
                "type": "function",
                "function": {
                    "name": "search_files",
                    "arguments": serde_json::json!({"pattern":"realize","dir":"src"}).to_string()
                }
            }]
        }),
        json!({
            "role": "tool",
            "tool_call_id": "call_1",
            "content": "GOVERNOR BLOCKED\n\n[Think Gate] Missing <think>.\n\ntool:\nsearch_files\narguments:\n{\"pattern\":\"realize\",\"dir\":\"src\"}"
        }),
        json!({
            "role": "assistant",
            "content": "",
            "tool_calls": [{
                "id": "call_2",
                "type": "function",
                "function": {
                    "name": "search_files",
                    "arguments": serde_json::json!({"pattern":"realize","dir":"src"}).to_string()
                }
            }]
        }),
        json!({
            "role": "tool",
            "tool_call_id": "call_2",
            "content": "GOVERNOR BLOCKED\n\n[Think Gate] Missing <think>.\n\ntool:\nsearch_files\narguments:\n{\"pattern\":\"realize\",\"dir\":\"src\"}"
        }),
    ];

    assert_eq!(consecutive_missing_think_blocks_for_tool(&messages, &tc), 2);
}

#[test]
fn consecutive_missing_think_blocks_for_observation_counts_mixed_tools() {
    let messages = vec![
        json!({
            "role": "assistant",
            "content": "",
            "tool_calls": [{
                "id": "call_1",
                "type": "function",
                "function": {
                    "name": "search_files",
                    "arguments": serde_json::json!({"pattern":"prefs","dir":"src/tui"}).to_string()
                }
            }]
        }),
        json!({
            "role": "tool",
            "tool_call_id": "call_1",
            "content": "GOVERNOR BLOCKED\n\n[Think Gate] Missing <think>.\n\ntool:\nsearch_files\narguments:\n{\"pattern\":\"prefs\",\"dir\":\"src/tui\"}"
        }),
        json!({
            "role": "assistant",
            "content": "",
            "tool_calls": [{
                "id": "call_2",
                "type": "function",
                "function": {
                    "name": "list_dir",
                    "arguments": serde_json::json!({"dir":"src/tui"}).to_string()
                }
            }]
        }),
        json!({
            "role": "tool",
            "tool_call_id": "call_2",
            "content": "GOVERNOR BLOCKED\n\n[Think Gate] Missing <think>.\n\ntool:\nlist_dir\narguments:\n{\"dir\":\"src/tui\"}"
        }),
    ];

    assert_eq!(
        consecutive_missing_think_blocks_for_observation(&messages),
        2
    );
}

#[test]
fn rescue_read_only_missing_think_for_tool_turn_after_repeated_blocks() {
    let tc = ToolCallData {
        thought_signature: None,
        id: "call_2".to_string(),
        name: "search_files".to_string(),
        arguments: serde_json::json!({"pattern":"realize","dir":"src"}).to_string(),
    };
    let plan = synthetic_read_only_observation_plan(
        "Locate where the /realize slash command is handled in the TUI. Do not edit anything.",
    );
    let messages = vec![
        json!({
            "role": "assistant",
            "content": "",
            "tool_calls": [{
                "id": "call_1",
                "type": "function",
                "function": {
                    "name": "search_files",
                    "arguments": serde_json::json!({"pattern":"realize","dir":"src"}).to_string()
                }
            }]
        }),
        json!({
            "role": "tool",
            "tool_call_id": "call_1",
            "content": "GOVERNOR BLOCKED\n\n[Think Gate] Missing <think>.\n\ntool:\nsearch_files\narguments:\n{\"pattern\":\"realize\",\"dir\":\"src\"}"
        }),
    ];

    let rescued = rescue_read_only_missing_think_for_tool_turn(
        &messages,
        &tc,
        &plan,
        "Locate where the /realize slash command is handled in the TUI. Do not edit anything.",
        true,
        ProviderKind::OpenAiCompatible,
        &ObservationEvidence::default(),
    )
    .expect("synthetic think");
    assert_eq!(rescued.tool, "search_files");
    assert!(rescued.next.contains("search"));
}

#[test]
fn rescue_read_only_missing_think_for_read_file_is_immediate() {
    let tc = ToolCallData {
        thought_signature: None,
        id: "call_1".to_string(),
        name: "read_file".to_string(),
        arguments: serde_json::json!({"path":"src/tui/events.rs"}).to_string(),
    };
    let plan = synthetic_read_only_observation_plan(
        "Locate where the /realize slash command is handled in the TUI. Do not edit anything.",
    );

    let rescued = rescue_read_only_missing_think_for_tool_turn(
        &[],
        &tc,
        &plan,
        "Locate where the /realize slash command is handled in the TUI. Do not edit anything.",
        true,
        ProviderKind::OpenAiCompatible,
        &ObservationEvidence::default(),
    )
    .expect("synthetic think");
    assert_eq!(rescued.tool, "read_file");
    assert!(rescued.next.contains("read"));
}

#[test]
fn rescue_read_only_missing_think_for_done_is_immediate_with_strong_evidence() {
    let tc = ToolCallData {
        thought_signature: None,
        id: "call_done".to_string(),
        name: "done".to_string(),
        arguments: serde_json::json!({
            "summary":"Found prefs storage",
            "completed_acceptance":["1) file identified","2) context confirmed"],
            "remaining_acceptance":[],
            "acceptance_evidence":[{"criterion":"file identified","command":"read_file(path=src/tui/prefs.rs)"}]
        })
        .to_string(),
    };
    let plan = synthetic_read_only_observation_plan(
        "Find where pane-scoped TUI preferences are serialized and restored. Do not edit anything.",
    );
    let evidence = ObservationEvidence {
        searches: vec![ObservationSearchEvidence {
            command: "search_files(dir=src/tui, pattern=prefs)".to_string(),
            pattern: "prefs".to_string(),
            hit_count: 2,
            paths: vec!["src/tui/prefs.rs".to_string()],
        }],
        reads: vec![ObservationReadEvidence {
            command: "read_file(path=src/tui/prefs.rs)".to_string(),
            path: "src/tui/prefs.rs".to_string(),
        }],
        resolutions: Vec::new(),
    };

    let rescued = rescue_read_only_missing_think_for_tool_turn(
        &[],
        &tc,
        &plan,
        "Find where pane-scoped TUI preferences are serialized and restored. Do not edit anything.",
        true,
        ProviderKind::OpenAiCompatible,
        &evidence,
    )
    .expect("synthetic think");
    assert_eq!(rescued.tool, "done");
}

#[test]
fn coerce_read_only_observation_tool_call_rewrites_search_pattern_after_repeated_gate_misses() {
    let tc = ToolCallData {
        thought_signature: None,
        id: "call_3".to_string(),
        name: "search_files".to_string(),
        arguments: serde_json::json!({"pattern":"pane","dir":"src"}).to_string(),
    };
    let messages = vec![
        json!({
            "role": "assistant",
            "content": "",
            "tool_calls": [{
                "id": "call_1",
                "type": "function",
                "function": {
                    "name": "search_files",
                    "arguments": serde_json::json!({"pattern":"pane","dir":"src"}).to_string()
                }
            }]
        }),
        json!({
            "role": "tool",
            "tool_call_id": "call_1",
            "content": "GOVERNOR BLOCKED\n\n[Plan Gate] Missing valid <plan>.\n\ntool:\nsearch_files\narguments:\n{\"pattern\":\"pane\",\"dir\":\"src\"}"
        }),
    ];

    let (rewritten, original, coerced) = coerce_read_only_observation_tool_call(
        &messages,
        &tc,
        "Find where pane-scoped TUI preferences are serialized and restored. Do not edit anything.",
        true,
        &ObservationEvidence::default(),
    )
    .expect("coerced");

    assert_eq!(original, "search_files(dir=src, pattern=pane)");
    assert_eq!(rewritten.name, "search_files");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&rewritten.arguments).expect("json")["pattern"]
            .as_str(),
        Some("prefs")
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&rewritten.arguments).expect("json")["dir"]
            .as_str(),
        Some("src/tui")
    );
    assert_eq!(coerced, "search_files(dir=src/tui, pattern=prefs)");
}

#[test]
fn coerce_read_only_observation_tool_call_rewrites_list_dir_to_preferred_search() {
    let tc = ToolCallData {
        thought_signature: None,
        id: "call_3".to_string(),
        name: "list_dir".to_string(),
        arguments: serde_json::json!({"dir":"src"}).to_string(),
    };
    let messages = vec![
        json!({
            "role": "assistant",
            "content": "",
            "tool_calls": [{
                "id": "call_1",
                "type": "function",
                "function": {
                    "name": "search_files",
                    "arguments": serde_json::json!({"pattern":"agent","dir":"src"}).to_string()
                }
            }]
        }),
        json!({
            "role": "tool",
            "tool_call_id": "call_1",
            "content": "GOVERNOR BLOCKED\n\n[Think Gate] Missing <think>.\n\ntool:\nsearch_files\narguments:\n{\"pattern\":\"agent\",\"dir\":\"src\"}"
        }),
    ];

    let (rewritten, _original, coerced) = coerce_read_only_observation_tool_call(
        &messages,
        &tc,
        "Locate where the coder-side repo-map read_file fallback is wired into the TUI agent flow. Do not edit anything.",
        true,
        &ObservationEvidence::default(),
    )
    .expect("coerced");

    assert_eq!(rewritten.name, "search_files");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&rewritten.arguments).expect("json")["pattern"]
            .as_str(),
        Some("lazy_read_fallback")
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&rewritten.arguments).expect("json")["dir"]
            .as_str(),
        Some("src/tui")
    );
    assert_eq!(
        coerced,
        "search_files(dir=src/tui, pattern=lazy_read_fallback)"
    );
}

#[test]
fn validate_think_rejects_tool_mismatch() {
    let plan = PlanBlock {
        goal: "ship fix".to_string(),
        steps: vec![
            "inspect".to_string(),
            "edit".to_string(),
            "verify".to_string(),
        ],
        acceptance_criteria: vec!["cargo test passes".to_string()],
        risks: "wrong file".to_string(),
        assumptions: "tests exist".to_string(),
    };
    let think = ThinkBlock {
        goal: "verify".to_string(),
        step: 3,
        tool: "read_file".to_string(),
        risk: "wrong target".to_string(),
        doubt: "might need tests".to_string(),
        next: "cargo test".to_string(),
        verify: "exit zero".to_string(),
    };
    let tc = ToolCallData {
        thought_signature: None,
        id: "call_1".to_string(),
        name: "exec".to_string(),
        arguments: "{\"command\":\"cargo test\"}".to_string(),
    };
    assert!(validate_think(&think, &plan, &tc).is_err());
}

#[test]
fn validate_think_rejects_exec_next_mismatch() {
    let plan = PlanBlock {
        goal: "ship fix".to_string(),
        steps: vec![
            "inspect".to_string(),
            "edit".to_string(),
            "verify".to_string(),
        ],
        acceptance_criteria: vec!["cargo test passes".to_string()],
        risks: "wrong file".to_string(),
        assumptions: "tests exist".to_string(),
    };
    let think = ThinkBlock {
        goal: "verify".to_string(),
        step: 3,
        tool: "exec".to_string(),
        risk: "wrong target".to_string(),
        doubt: "might hit wrong crate".to_string(),
        next: "cargo check".to_string(),
        verify: "exit zero".to_string(),
    };
    let tc = ToolCallData {
        thought_signature: None,
        id: "call_1".to_string(),
        name: "exec".to_string(),
        arguments: "{\"command\":\"cargo test --lib\"}".to_string(),
    };
    assert!(validate_think(&think, &plan, &tc).is_err());
}

#[test]
fn validate_think_accepts_matching_exec_prefix() {
    let plan = PlanBlock {
        goal: "ship fix".to_string(),
        steps: vec![
            "inspect".to_string(),
            "edit".to_string(),
            "verify".to_string(),
        ],
        acceptance_criteria: vec!["cargo test passes".to_string()],
        risks: "wrong file".to_string(),
        assumptions: "tests exist".to_string(),
    };
    let think = ThinkBlock {
        goal: "verify".to_string(),
        step: 3,
        tool: "exec".to_string(),
        risk: "wrong target".to_string(),
        doubt: "might need workspace".to_string(),
        next: "cargo test".to_string(),
        verify: "exit zero".to_string(),
    };
    let tc = ToolCallData {
        thought_signature: None,
        id: "call_1".to_string(),
        name: "exec".to_string(),
        arguments: "{\"command\":\"cargo test --lib\"}".to_string(),
    };
    assert!(validate_think(&think, &plan, &tc).is_ok());
}

#[test]
fn validate_reflection_rejects_missing_fields() {
    let mem = FailureMemory::default();
    let r = ReflectionBlock {
        last_outcome: "failure".to_string(),
        goal_delta: GoalDelta::Same,
        wrong_assumption: "".to_string(),
        strategy_change: StrategyChange::Adjust,
        next_minimal_action: "".to_string(),
    };
    assert!(validate_reflection(&r, &mem, 0).is_err());
}

#[test]
fn validate_reflection_rejects_keep_on_repeated_failure() {
    let mem = FailureMemory {
        consecutive_failures: 2,
        last_command_sig: None,
        same_command_repeats: 3,
        last_error_sig: None,
        same_error_repeats: 2,
        last_output_hash: None,
        same_output_repeats: 2,
        last_error_class: ErrorClass::Unknown,
    };
    let r = ReflectionBlock {
        last_outcome: "failure".to_string(),
        goal_delta: GoalDelta::Same,
        wrong_assumption: "I assumed the command would work unchanged.".to_string(),
        strategy_change: StrategyChange::Keep,
        next_minimal_action: "try again".to_string(),
    };
    assert!(validate_reflection(&r, &mem, 0).is_err());
}

#[test]
fn validate_reflection_rejects_keep_on_file_tool_repeats() {
    let mem = FailureMemory::default();
    let r = ReflectionBlock {
        last_outcome: "failure".to_string(),
        goal_delta: GoalDelta::Closer,
        wrong_assumption: "I assumed the patch would apply cleanly.".to_string(),
        strategy_change: StrategyChange::Keep,
        next_minimal_action: "retry patch_file with same args".to_string(),
    };
    assert!(validate_reflection(&r, &mem, 2).is_err());
}

#[test]
fn rebuilds_failure_memory_from_session_messages() {
    let messages = vec![
        json!({"role":"system","content":"sys"}),
        json!({"role":"user","content":"do thing"}),
        json!({"role":"assistant","content":"","tool_calls":[{"id":"call_1","type":"function","function":{"name":"exec","arguments":"{\"command\":\"git status\"}"}}]}),
        json!({"role":"tool","tool_call_id":"call_1","content":"FAILED (exit_code: 1)\nstderr:\nfatal: nope"}),
        json!({"role":"assistant","content":"","tool_calls":[{"id":"call_2","type":"function","function":{"name":"exec","arguments":"{\"command\":\"git status\"}"}}]}),
        json!({"role":"tool","tool_call_id":"call_2","content":"FAILED (exit_code: 1)\nstderr:\nfatal: nope"}),
    ];

    let mem = FailureMemory::from_recent_messages(&messages, None);
    assert_eq!(mem.consecutive_failures, 2);
    assert_eq!(mem.same_command_repeats, 2);
    assert_eq!(mem.same_error_repeats, 2);
}

#[test]
fn classify_exec_kind_treats_git_status_as_diagnostic() {
    assert_eq!(classify_exec_kind("git status", None), ExecKind::Diagnostic);
    assert_eq!(classify_exec_kind("cargo check", None), ExecKind::Verify);
}

#[test]
fn classify_verify_level_distinguishes_build_and_behavioral() {
    assert_eq!(
        classify_verify_level("cargo check", None),
        Some(VerificationLevel::Build)
    );
    assert_eq!(
        classify_verify_level("cargo test --lib", None),
        Some(VerificationLevel::Behavioral)
    );
    assert_eq!(
        classify_verify_level("npm run lint", None),
        Some(VerificationLevel::Build)
    );
    assert_eq!(
        classify_verify_level("python -m pytest -q", None),
        Some(VerificationLevel::Behavioral)
    );
    assert_eq!(classify_verify_level("git status", None), None);
}

#[test]
fn classify_verify_level_rejects_failure_suppressed_commands() {
    assert_eq!(classify_verify_level("cargo test || true", None), None);
    assert_eq!(
        classify_verify_level(
            "cd maze_game_pygame && SDL_VIDEODRIVER=dummy python3 -m unittest -q 2>&1 || true",
            Some("cd maze_game_pygame && SDL_VIDEODRIVER=dummy python3 -m unittest -q 2>&1"),
        ),
        None
    );
}

#[test]
fn classify_verify_level_requires_full_configured_test_command_match() {
    let configured = "test -d maze_game_pygame/.git && test -f maze_game_pygame/README.md && test -f maze_game_pygame/game.py && test -f maze_game_pygame/main.py && test -f maze_game_pygame/test_game.py && cd maze_game_pygame && SDL_VIDEODRIVER=dummy python3 -m unittest -q 2>&1";
    let weak_probe = "test -d maze_game_pygame/.git && test -f maze_game_pygame/readme.md && test -f maze_game_pygame/game.py && test -f maze_game_pygame/main.py && test -f maze_game_pygame/test_game.py || true";

    assert_eq!(classify_verify_level(weak_probe, Some(configured)), None);
    assert_eq!(
        classify_verify_level(configured, Some(configured)),
        Some(VerificationLevel::Behavioral)
    );
}

#[test]
fn restore_done_gate_from_messages_ignores_weak_prefixed_test_probe() {
    let configured = "test -d maze_game_pygame/.git && test -f maze_game_pygame/README.md && test -f maze_game_pygame/game.py && test -f maze_game_pygame/main.py && test -f maze_game_pygame/test_game.py && cd maze_game_pygame && SDL_VIDEODRIVER=dummy python3 -m unittest -q 2>&1";
    let weak_probe = "test -d maze_game_pygame/.git && test -f maze_game_pygame/readme.md && test -f maze_game_pygame/game.py && test -f maze_game_pygame/main.py && test -f maze_game_pygame/test_game.py || true";
    let messages = vec![
        json!({
            "role": "assistant",
            "tool_calls": [{
                "id": "call_readme",
                "type": "function",
                "function": {
                    "name": "write_file",
                    "arguments": "{\"path\":\"maze_game_pygame/README.md\",\"content\":\"# maze_game_pygame\\n\"}"
                }
            }]
        }),
        json!({
            "role": "tool",
            "tool_call_id": "call_readme",
            "content": "OK: wrote 'maze_game_pygame/README.md' (1 lines, 19 bytes)\n[hash] before=0 after=1"
        }),
        json!({
            "role": "assistant",
            "tool_calls": [{
                "id": "call_probe",
                "type": "function",
                "function": {
                    "name": "exec",
                    "arguments": format!("{{\"command\":{}}}", serde_json::to_string(weak_probe).expect("json"))
                }
            }]
        }),
        json!({
            "role": "tool",
            "tool_call_id": "call_probe",
            "content": "OK (exit_code: 0)\nstdout:\n"
        }),
    ];

    let (_, last_mutation_step, last_build_verify_ok_step, last_behavioral_verify_ok_step, _) =
        restore_done_gate_from_messages(&messages, Some(configured));

    assert_eq!(last_mutation_step, Some(2));
    assert_eq!(last_build_verify_ok_step, None);
    assert_eq!(last_behavioral_verify_ok_step, None);
}

#[test]
fn effective_verify_ok_step_requires_behavioral_when_requested() {
    assert_eq!(
        effective_verify_ok_step(VerificationLevel::Build, Some(3), Some(5)),
        Some(5)
    );
    assert_eq!(
        effective_verify_ok_step(VerificationLevel::Behavioral, Some(3), Some(5)),
        Some(5)
    );
    assert_eq!(
        effective_verify_ok_step(VerificationLevel::Behavioral, Some(3), None),
        None
    );
}

#[test]
fn infer_required_verification_level_keeps_docs_build_only() {
    assert_eq!(
        infer_required_verification_level("Update README wording and docs only", None),
        VerificationLevel::Build
    );
    assert_eq!(
        infer_required_verification_level("Fix runtime bug in agent loop", None),
        VerificationLevel::Behavioral
    );
}

#[test]
fn goal_check_policy_respects_command_approval() {
    assert!(should_auto_run_goal_checks(false, DEFAULT_MAX_ITERS));
    assert!(!should_auto_run_goal_checks(true, DEFAULT_MAX_ITERS));
}

#[test]
fn recovery_gate_allows_new_file_write_during_diagnose() {
    let recovery = RecoveryGovernor {
        stage: Some(RecoveryStage::Diagnose),
        required_verification: VerificationLevel::Behavioral,
        ..Default::default()
    };
    let tc = ToolCallData {
        thought_signature: None,
        id: "call_write".to_string(),
        name: "write_file".to_string(),
        arguments: serde_json::json!({
            "path":"notes/todo.txt",
            "content":"ship it\n"
        })
        .to_string(),
    };

    assert!(recovery
        .maybe_block_tool(
            &tc,
            Some("test -f notes/todo.txt && grep -Fx \"ship it\" notes/todo.txt"),
            TaskHarness::infer(
                "Create `notes/todo.txt` containing exactly `ship it`.",
                false,
            ),
            false,
        )
        .is_none());
}

#[test]
fn verification_requirement_prompt_is_idle_when_no_verify_pressure() {
    let recovery = RecoveryGovernor::default();
    let goal_checks = GoalCheckTracker::default();
    assert!(!should_emit_verification_requirement_prompt(
        AgentState::Planning,
        &recovery,
        Some(2),
        Some(2),
        &goal_checks,
    ));
}

#[test]
fn verification_requirement_prompt_activates_for_pending_verify_or_goal_checks() {
    let recovery = RecoveryGovernor::default();
    let goal_checks = GoalCheckTracker {
        tests: GoalCheckStatus {
            attempts: 1,
            ok: false,
        },
        ..GoalCheckTracker::default()
    };
    assert!(should_emit_verification_requirement_prompt(
        AgentState::Planning,
        &recovery,
        Some(3),
        Some(2),
        &GoalCheckTracker::default(),
    ));
    assert!(should_emit_verification_requirement_prompt(
        AgentState::Planning,
        &recovery,
        Some(2),
        Some(2),
        &goal_checks,
    ));
}

#[test]
fn goal_check_runner_command_uses_shared_catalog() {
    let td = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        td.path().join("Cargo.toml"),
        "[package]\nname='x'\nversion='0.1.0'\n",
    )
    .expect("write cargo");
    let cmd = goal_check_runner_command(
        VerificationLevel::Behavioral,
        td.path().to_string_lossy().as_ref(),
        None,
    )
    .expect("runner command");
    assert_eq!(cmd, "cargo test -q");
}

#[test]
fn goal_check_runner_command_prefers_configured_test_cmd() {
    let td = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        td.path().join("Cargo.toml"),
        "[package]\nname='x'\nversion='0.1.0'\n",
    )
    .expect("write cargo");

    let cmd = goal_check_runner_command(
        VerificationLevel::Behavioral,
        td.path().to_string_lossy().as_ref(),
        Some("cargo test -q demo::tests:: 2>&1 && bash scripts/smoke.sh"),
    )
    .expect("runner command");

    assert_eq!(
        cmd,
        "cargo test -q demo::tests:: 2>&1 && bash scripts/smoke.sh"
    );
}

#[test]
fn goal_check_result_preserves_verification_command_case_for_evidence() {
    let td = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir_all(td.path().join("demo_repo/.git")).expect("mkdir .git");
    std::fs::write(td.path().join("demo_repo/README.md"), "# demo\n").expect("write readme");
    std::fs::write(td.path().join("demo_repo/.gitignore"), "target/\n").expect("write gitignore");
    let command =
        "test -d demo_repo/.git && test -f demo_repo/README.md && test -f demo_repo/.gitignore";
    let rt = tokio::runtime::Runtime::new().expect("rt");
    let (_tx, mut _rx) = tokio::sync::mpsc::channel(4);
    let result = rt.block_on(run_goal_check_command(
        "repo",
        command,
        td.path().to_string_lossy().as_ref(),
        &_tx,
    ));

    assert!(result.passed);
    assert_eq!(result.command, command);
}

#[test]
fn repo_goal_missing_labels_follow_shared_requirements() {
    let td = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir_all(td.path().join(".git")).expect("mkdir .git");
    let rt = tokio::runtime::Runtime::new().expect("rt");
    let missing = rt.block_on(repo_goal_missing_labels(
        td.path().to_string_lossy().as_ref(),
    ));
    assert!(missing.contains(&"HEAD (commit)".to_string()));
    assert!(missing.contains(&"README.md".to_string()));
    assert!(!missing.contains(&".git".to_string()));
}

#[test]
fn verification_examples_include_shared_goal_check_runners() {
    let build = verification_examples(VerificationLevel::Build);
    let behavioral = verification_examples(VerificationLevel::Behavioral);

    assert!(build.contains("cargo build -q"));
    assert!(behavioral.contains("python -m pytest -q"));
}

#[test]
fn restore_done_gate_does_not_count_git_status_as_verification() {
    let messages = vec![
        json!({"role":"assistant","content":"","tool_calls":[{"id":"call_1","type":"function","function":{"name":"exec","arguments":"{\"command\":\"touch foo.txt\"}"}}]}),
        json!({"role":"tool","tool_call_id":"call_1","content":"OK (exit_code: 0)\nstdout:\n"}),
        json!({"role":"assistant","content":"","tool_calls":[{"id":"call_2","type":"function","function":{"name":"exec","arguments":"{\"command\":\"git status\"}"}}]}),
        json!({"role":"tool","tool_call_id":"call_2","content":"OK (exit_code: 0)\nstdout:\nOn branch main\n"}),
    ];

    let (_, last_mutation_step, _last_build_verify_ok_step, last_behavioral_verify_ok_step, _) =
        restore_done_gate_from_messages(&messages, None);

    assert_eq!(last_mutation_step, Some(1));
    assert_eq!(last_behavioral_verify_ok_step, None);
}

#[test]
fn upgrade_required_verification_from_messages_detects_code_mutation() {
    let messages = vec![
        json!({"role":"assistant","content":"","tool_calls":[{"id":"call_1","type":"function","function":{"name":"patch_file","arguments":"{\"path\":\"src/tui/agent.rs\",\"search\":\"a\",\"replace\":\"b\"}"}}]}),
        json!({"role":"tool","tool_call_id":"call_1","content":"OK: patched 'src/tui/agent.rs'\n[hash] before=1 after=2"}),
    ];

    assert_eq!(
        upgrade_required_verification_from_messages(&messages, VerificationLevel::Build),
        VerificationLevel::Behavioral
    );
}

#[test]
fn working_memory_rebuilds_from_messages() {
    let messages = vec![
        json!({
            "role":"assistant",
            "content":"<plan>\ngoal: ship fix\nsteps: 1) inspect files 2) edit agent 3) verify build\nacceptance: 1) cargo check passes 2) runtime gate blocks bad tool calls\nrisks: wrong file, wrong command\nassumptions: cargo works\n</plan>\n<think>\ngoal: inspect file\nstep: 1\ntool: read_file\nrisk: wrong path\ndoubt: maybe wrong target\nnext: read src/tui/agent.rs\nverify: file opens\n</think>",
            "tool_calls":[{"id":"call_1","type":"function","function":{"name":"read_file","arguments":"{\"path\":\"src/tui/agent.rs\"}"}}]
        }),
        json!({"role":"tool","tool_call_id":"call_1","content":"[src/tui/agent.rs] (10 lines, 100 bytes)\nfn main() {}\n"}),
        json!({
            "role":"assistant",
            "content":"<think>\ngoal: verify build\nstep: 3\ntool: exec\nrisk: wrong target\ndoubt: maybe workspace differs\nnext: cargo check\nverify: exit code is zero\n</think>",
            "tool_calls":[{"id":"call_2","type":"function","function":{"name":"exec","arguments":"{\"command\":\"cargo check\"}"}}]
        }),
        json!({"role":"tool","tool_call_id":"call_2","content":"OK (exit_code: 0)\ncwd: /repo\nstdout:\nFinished dev [unoptimized] target(s) in 0.10s\n"}),
        json!({"role":"assistant","content":"<reflect>\nlast_outcome: partial\ngoal_delta: closer\nwrong_assumption: cargo test was necessary first\nstrategy_change: adjust\nnext_minimal_action: run targeted tests\n</reflect>"}),
    ];

    let mem = WorkingMemory::from_messages(&messages, None);
    assert!(mem.facts.iter().any(|fact| fact.contains("cwd: /repo")));
    assert!(mem
        .completed_steps
        .iter()
        .any(|step| step == "inspect files"));
    assert!(mem
        .completed_steps
        .iter()
        .any(|step| step == "verify build"));
    assert!(mem
        .successful_verifications
        .iter()
        .any(|cmd| cmd == "cargo check"));
    assert_eq!(mem.chosen_strategy.as_deref(), Some("run targeted tests"));
}

#[test]
fn working_memory_sync_to_plan_drops_stale_steps() {
    let mut mem = WorkingMemory::default();
    mem.remember_completed_step("inspect files");
    mem.remember_completed_step("verify build");

    let plan = PlanBlock {
        goal: "ship fix".to_string(),
        steps: vec!["verify build".to_string(), "ship".to_string()],
        acceptance_criteria: vec!["cargo check passes".to_string()],
        risks: "wrong file".to_string(),
        assumptions: "tests exist".to_string(),
    };

    mem.sync_to_plan(&plan);
    assert_eq!(mem.completed_steps, vec!["verify build".to_string()]);
}

#[test]
fn remember_successful_verification_preserves_long_configured_test_command() {
    let mut mem = WorkingMemory::default();
    let command = "test -d maze_game/.git && test -f maze_game/README.md && test -f maze_game/src/lib.rs && test -f maze_game/src/main.rs && cd maze_game && cargo test 2>&1";

    mem.remember_successful_verification(command);

    assert_eq!(mem.successful_verifications.len(), 1);
    assert!(mem.successful_verifications[0].contains("maze_game/src/main.rs"));
    assert!(mem.successful_verifications[0].contains("cargo test 2>&1"));
}

#[test]
fn remember_successful_verification_preserves_pygame_unittest_tail() {
    let mut mem = WorkingMemory::default();
    let command = "test -d maze_game_pygame/.git && test -f maze_game_pygame/README.md && test -f maze_game_pygame/game.py && test -f maze_game_pygame/main.py && test -f maze_game_pygame/test_game.py && cd maze_game_pygame && SDL_VIDEODRIVER=dummy python3 -m unittest -q 2>&1";

    mem.remember_successful_verification(command);

    assert_eq!(mem.successful_verifications.len(), 1);
    assert!(mem.successful_verifications[0].contains("maze_game_pygame/test_game.py"));
    assert!(mem.successful_verifications[0]
        .contains("SDL_VIDEODRIVER=dummy python3 -m unittest -q 2>&1"));
}

#[test]
fn collect_known_acceptance_commands_recovers_test_cmd_from_auto_test_pass() {
    let test_cmd = "test -d maze_game_pygame/.git && test -f maze_game_pygame/README.md && test -f maze_game_pygame/game.py && test -f maze_game_pygame/main.py && test -f maze_game_pygame/test_game.py && cd maze_game_pygame && SDL_VIDEODRIVER=dummy python3 -m unittest -q 2>&1";
    let messages = vec![
        json!({
            "role": "assistant",
            "tool_calls": [{
                "id": "call_test_game",
                "type": "function",
                "function": {
                    "name": "write_file",
                    "arguments": "{\"path\":\"maze_game_pygame/test_game.py\",\"content\":\"print('ok')\\n\"}"
                }
            }]
        }),
        json!({
            "role": "tool",
            "tool_call_id": "call_test_game",
            "content": "OK: wrote 'maze_game_pygame/test_game.py' (1 lines, 12 bytes)\n[hash] before=0 after=1\n[auto-test] ✓ PASSED (exit 0)\nRan 3 tests in 0.10s\nOK"
        }),
    ];

    let commands =
        collect_known_acceptance_commands(&messages, &WorkingMemory::default(), Some(test_cmd));

    assert!(commands.iter().any(|command| command == test_cmd));
}

#[test]
fn wrap_exec_with_pwd_preserves_nonzero_exit_code_on_posix() {
    if cfg!(target_os = "windows") {
        return;
    }

    let wrapped = wrap_exec_with_pwd("false");
    let rt = tokio::runtime::Runtime::new().expect("rt");
    let result = rt
        .block_on(exec::run_command(&wrapped, None))
        .expect("run wrapped command");
    let (_stdout, pwd_after) = strip_pwd_marker(&result.stdout);

    assert_ne!(result.exit_code, 0);
    assert!(pwd_after.is_some());
}

#[test]
fn last_impact_step_tracks_post_mutation_assessment() {
    let messages = vec![
        json!({"role":"assistant","content":"","tool_calls":[{"id":"call_1","type":"function","function":{"name":"write_file","arguments":"{\"path\":\"a.txt\",\"content\":\"x\"}"}}]}),
        json!({"role":"tool","tool_call_id":"call_1","content":"OK\n[hash] before=0 after=1"}),
        json!({"role":"assistant","content":"<impact>\nchanged: wrote a.txt\nprogress: step 2 moved\nremaining_gap: run cargo check\n</impact>\n"}),
    ];

    assert_eq!(last_impact_step_from_messages(&messages), Some(1));
}

#[test]
fn parse_realize_block_extracts_reason() {
    let text = "<realize>\nreason: enough evidence to commit this plan\n</realize>";
    assert_eq!(
        parse_realize_block(text).as_deref(),
        Some("enough evidence to commit this plan")
    );
}

#[test]
fn non_done_tool_realizes_latent_but_done_does_not() {
    let read = ToolCallData {
        thought_signature: None,
        id: "call_read".to_string(),
        name: "read_file".to_string(),
        arguments: "{\"path\":\"src/tui/events.rs\"}".to_string(),
    };
    let done = ToolCallData {
        thought_signature: None,
        id: "call_done".to_string(),
        name: "done".to_string(),
        arguments: "{\"summary\":\"ok\",\"completed_acceptance\":[],\"remaining_acceptance\":[],\"acceptance_evidence\":[]}".to_string(),
    };

    assert!(tool_call_realizes_latent(None, Some(&read)));
    assert!(!tool_call_realizes_latent(None, Some(&done)));
    assert!(tool_call_realizes_latent(
        Some("enough evidence to commit"),
        Some(&done)
    ));
}

#[test]
fn build_realize_done_gate_message_mentions_commit_before_done() {
    let latent = LatentPlanBuffer {
        raw_text: "<plan>...</plan>".to_string(),
        plan: PlanBlock {
            goal: "Locate the /realize handler".to_string(),
            steps: vec!["search for /realize".to_string()],
            acceptance_criteria: vec!["identify the handler file".to_string()],
            risks: "wrong file".to_string(),
            assumptions: "observation tools are enough".to_string(),
        },
        summary: "goal: locate /realize; steps: search | read".to_string(),
        latest_intent: Some("tool: read_file; next: inspect src/tui/events.rs".to_string()),
        anchor_baseline: "goal: locate /realize".to_string(),
        created_iter: 2,
        defer_score: 0.62,
        tail_updates: 1,
    };

    let msg = build_realize_done_gate_message(&latent);
    assert!(msg.contains("[Realize Gate]"));
    assert!(msg.contains("before `done`"));
    assert!(msg.contains("goal: locate /realize"));
    assert!(msg.contains("<realize>reason: enough evidence to commit</realize>"));
}

#[test]
fn strip_known_latent_tags_preserves_surrounding_text() {
    let text = "before\n<plan>\ngoal: x\nsteps: 1) a 2) b\nacceptance: 1) ok\nrisks: r\nassumptions: a\n</plan>\nafter";
    assert_eq!(strip_known_latent_tags(text), "before\nafter");
}

#[test]
fn cosine_drift_is_lower_for_nearby_text() {
    let anchor = "fix repo map fallback for read_file and list_dir";
    let near = "fix repo map fallback for read_file";
    let far = "compose a critique about unrelated observer prose";
    assert!(cosine_token_distance(anchor, near) < cosine_token_distance(anchor, far));
}

#[test]
fn latest_intent_anchor_baseline_reads_system_anchor() {
    let messages = vec![
        json!({"role":"system","content":"base system"}),
        json!({"role":"system","content":"[Intent Anchor]\nrevision: 2\nbaseline: goal: locate slash handler | target: src/tui/events.rs | constraints: do not edit | success: include file path | opt: -\n"}),
        json!({"role":"user","content":"continue"}),
    ];
    assert_eq!(
        latest_intent_anchor_baseline(&messages).as_deref(),
        Some(
            "goal: locate slash handler | target: src/tui/events.rs | constraints: do not edit | success: include file path | opt: -"
        )
    );
}

#[test]
fn build_anchor_baseline_prefers_intent_anchor_over_root_prompt() {
    let messages = vec![json!({
        "role":"system",
        "content":"[Intent Anchor]\nrevision: 3\nbaseline: goal: stabilize coder loop | target: src/tui/agent.rs | constraints: keep scope narrow | success: preserve user intent | opt: improve readability\n"
    })];
    let baseline = build_anchor_baseline(
        &messages,
        "something vague like make it better",
        None,
        &WorkingMemory::default(),
    );
    assert!(baseline.contains("goal: stabilize coder loop"));
    assert!(!baseline.contains("something vague like make it better"));
}

#[test]
fn repair_mistral_plan_for_tool_turn_fills_read_only_observation_shape() {
    let parsed = PlanBlock {
        goal: "Locate the exact file and context where `/realize` is handled in the TUI."
            .to_string(),
        steps: Vec::new(),
        acceptance_criteria: Vec::new(),
        risks: "unknown risks; inspect carefully before editing".to_string(),
        assumptions: "current repo scan reflects the active workspace".to_string(),
    };
    let tc = ToolCallData {
        thought_signature: None,
        id: "call_1".to_string(),
        name: "read_file".to_string(),
        arguments: "{\"path\":\"src/tui/events.rs\"}".to_string(),
    };
    let repaired = repair_mistral_plan_for_tool_turn(
        Some(&parsed),
        None,
        &tc,
        "Locate where the /realize slash command is handled in the TUI. Do not edit anything.",
        true,
    )
    .expect("repaired plan");
    assert!(repaired.steps.len() >= 2);
    assert!(!repaired.acceptance_criteria.is_empty());
    assert!(repaired.goal.contains("/realize"));
}

#[test]
fn select_think_for_tool_turn_prefers_synth_over_invalid_last_for_mistral() {
    let plan = PlanBlock {
        goal: "Locate the /realize handler".to_string(),
        steps: vec![
            "search for /realize".to_string(),
            "read the matching file".to_string(),
        ],
        acceptance_criteria: vec!["report the file path".to_string()],
        risks: "wrong file".to_string(),
        assumptions: "observation tools are sufficient".to_string(),
    };
    let tc = ToolCallData {
        thought_signature: None,
        id: "call_2".to_string(),
        name: "done".to_string(),
        arguments: "{\"summary\":\"ok\",\"completed_acceptance\":[],\"remaining_acceptance\":[],\"acceptance_evidence\":[]}".to_string(),
    };
    let invalid_last = ThinkBlock {
        goal: String::new(),
        step: 1,
        tool: "done".to_string(),
        risk: "r".to_string(),
        doubt: "d".to_string(),
        next: "finish".to_string(),
        verify: "confirm".to_string(),
    };
    let synth = compat_synthetic_think(&tc, &plan);
    let (selected, used_synth) = select_think_for_tool_turn(
        None,
        Some(&invalid_last),
        Some(&synth),
        &plan,
        &tc,
        ProviderKind::Mistral,
    );
    assert!(used_synth);
    assert_eq!(selected.expect("selected think").goal, plan.goal);
}

#[test]
fn select_think_for_tool_turn_prefers_synth_over_invalid_last_for_openai_compatible() {
    let plan = PlanBlock {
        goal: "Locate the failing test".to_string(),
        steps: vec![
            "search for the test".to_string(),
            "read the matching file".to_string(),
        ],
        acceptance_criteria: vec!["report the file path".to_string()],
        risks: "wrong file".to_string(),
        assumptions: "observation tools are sufficient".to_string(),
    };
    let tc = ToolCallData {
        thought_signature: None,
        id: "call_3".to_string(),
        name: "search_files".to_string(),
        arguments: "{\"pattern\":\"test\",\"dir\":\"src\"}".to_string(),
    };
    let invalid_last = ThinkBlock {
        goal: String::new(),
        step: 1,
        tool: "search_files".to_string(),
        risk: "r".to_string(),
        doubt: "d".to_string(),
        next: "search".to_string(),
        verify: "confirm".to_string(),
    };
    let synth = compat_synthetic_think(&tc, &plan);
    let (selected, used_synth) = select_think_for_tool_turn(
        None,
        Some(&invalid_last),
        Some(&synth),
        &plan,
        &tc,
        ProviderKind::OpenAiCompatible,
    );
    assert!(used_synth);
    assert_eq!(selected.expect("selected think").goal, plan.goal);
}

#[test]
fn canonicalize_evidence_command_accepts_natural_language_search_and_read() {
    assert_eq!(
        canonicalize_evidence_command("search_files for `/realize` in `src/`"),
        "search_files(dir=src, pattern=/realize)"
    );
    assert_eq!(
        canonicalize_evidence_command("read_file of `src/tui/events.rs`"),
        "read_file(path=src/tui/events.rs)"
    );
    assert_eq!(
        canonicalize_evidence_command(
            "search_files with pattern \"/realize\" in src/ (succeeded with matches in src/tui/events.rs)"
        ),
        "search_files(dir=src, pattern=/realize)"
    );
    assert_eq!(
        canonicalize_evidence_command(
            "read_file src/tui/events.rs (succeeded with confirmation of handling logic)"
        ),
        "read_file(path=src/tui/events.rs)"
    );
}

#[test]
fn resolution_memory_resolves_suffix_alias_to_canonical_path() {
    let mut evidence = ObservationEvidence::default();
    evidence.remember_resolution("tui/events.rs", "src/tui/events.rs", "repo_map:read_file");

    assert_eq!(
        evidence.resolve_path_alias("tui/events.rs").as_deref(),
        Some("src/tui/events.rs")
    );
    assert_eq!(
        evidence.resolve_path_alias("./tui/events.rs").as_deref(),
        Some("src/tui/events.rs")
    );
}

#[test]
fn rewrite_tool_call_with_resolution_rewrites_read_file_path() {
    let mut evidence = ObservationEvidence::default();
    evidence.remember_resolution("tui/events.rs", "src/tui/events.rs", "repo_map:read_file");
    let tc = ToolCallData {
        thought_signature: None,
        id: "call_1".to_string(),
        name: "read_file".to_string(),
        arguments: serde_json::json!({"path":"tui/events.rs"}).to_string(),
    };

    let (rewritten, original, canonical) =
        rewrite_tool_call_with_resolution(&tc, &evidence).expect("rewritten");
    assert_eq!(original, "tui/events.rs");
    assert_eq!(canonical, "src/tui/events.rs");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&rewritten.arguments)
            .ok()
            .and_then(|v| v.get("path").and_then(|v| v.as_str()).map(str::to_string))
            .as_deref(),
        Some("src/tui/events.rs")
    );
}

#[test]
fn realize_preset_parser_accepts_named_levels() {
    assert_eq!(
        "off".parse::<RealizePreset>().ok(),
        Some(RealizePreset::Off)
    );
    assert_eq!(
        "low".parse::<RealizePreset>().ok(),
        Some(RealizePreset::Low)
    );
    assert_eq!(
        "mid".parse::<RealizePreset>().ok(),
        Some(RealizePreset::Mid)
    );
    assert_eq!(
        "medium".parse::<RealizePreset>().ok(),
        Some(RealizePreset::Mid)
    );
    assert_eq!(
        "high".parse::<RealizePreset>().ok(),
        Some(RealizePreset::High)
    );
}

#[test]
fn realize_preset_mid_matches_tui_default_shape() {
    let cfg = RealizeOnDemandConfig::resolve(Some(RealizePreset::Mid));
    assert!(cfg.enabled);
    assert_eq!(cfg.defer_threshold, 0.45);
    assert_eq!((cfg.window_start, cfg.window_end), (1, 3));
    assert_eq!(cfg.lambda_min, 0.15);
    assert_eq!(cfg.lambda_max, 0.90);
}

#[test]
fn parses_evidence_block_fields() {
    let text = "\
<evidence>\n\
target_files: 1) src/tui/agent.rs\n\
target_symbols: 1) run_agentic_json\n\
evidence: read_file showed the recovery gate in src/tui/agent.rs\n\
open_questions: none\n\
next_probe: patch the recovery branch in run_agentic_json\n\
</evidence>\n";

    let block = parse_evidence_block(text).expect("evidence parsed");
    assert_eq!(block.target_files, vec!["src/tui/agent.rs".to_string()]);
    assert_eq!(block.target_symbols, vec!["run_agentic_json".to_string()]);
    assert_eq!(
        block.evidence,
        "read_file showed the recovery gate in src/tui/agent.rs"
    );
}

#[test]
fn validate_evidence_block_accepts_observed_patch_target() {
    let evidence_block = EvidenceBlock {
        target_files: vec!["src/tui/agent.rs".to_string()],
        target_symbols: vec!["run_agentic_json".to_string()],
        evidence: "read_file confirmed the target branch in src/tui/agent.rs".to_string(),
        open_questions: "none".to_string(),
        next_probe: "patch the recovery branch".to_string(),
    };
    let tc = ToolCallData {
        thought_signature: None,
        id: "call_1".to_string(),
        name: "patch_file".to_string(),
        arguments: serde_json::json!({
            "path": "src/tui/agent.rs",
            "search": "old",
            "replace": "new"
        })
        .to_string(),
    };
    let observations = ObservationEvidence {
        reads: vec![ObservationReadEvidence {
            command: "read_file(path=src/tui/agent.rs)".to_string(),
            path: "src/tui/agent.rs".to_string(),
        }],
        searches: Vec::new(),
        resolutions: Vec::new(),
    };

    assert!(validate_evidence_block(&evidence_block, &tc, &observations).is_ok());
}

#[test]
fn validate_evidence_block_rejects_unobserved_patch_target() {
    let evidence_block = EvidenceBlock {
        target_files: vec!["src/tui/agent.rs".to_string()],
        target_symbols: vec!["run_agentic_json".to_string()],
        evidence: "assumed the recovery branch lived there".to_string(),
        open_questions: "exact line span".to_string(),
        next_probe: "patch the recovery branch".to_string(),
    };
    let tc = ToolCallData {
        thought_signature: None,
        id: "call_1".to_string(),
        name: "patch_file".to_string(),
        arguments: serde_json::json!({
            "path": "src/tui/agent.rs",
            "search": "old",
            "replace": "new"
        })
        .to_string(),
    };
    let observations = ObservationEvidence {
        reads: vec![ObservationReadEvidence {
            command: "read_file(path=src/server.rs)".to_string(),
            path: "src/server.rs".to_string(),
        }],
        searches: Vec::new(),
        resolutions: Vec::new(),
    };

    let err = validate_evidence_block(&evidence_block, &tc, &observations)
        .expect_err("target without observation should fail");
    assert!(err.to_string().contains("lacks prior read/search evidence"));
}

#[test]
fn build_task_contract_prompt_mentions_read_only_constraint() {
    let contract = derive_task_contract(
        "Locate the slash command handler without editing files",
        true,
        false,
        VerificationLevel::Build,
    );
    let prompt = build_task_contract_prompt(&contract, None, VerificationLevel::Build);
    assert!(prompt.contains("inspection-only: do not edit files"));
}

#[test]
fn validate_plan_against_task_contract_rejects_drift() {
    let contract = derive_task_contract(
        "Fix the reflection gate in src/tui/agent.rs",
        false,
        true,
        VerificationLevel::Behavioral,
    );
    let plan = PlanBlock {
        goal: "rewrite README prose".to_string(),
        steps: vec!["edit README".to_string(), "proofread docs".to_string()],
        acceptance_criteria: vec!["README wording is updated".to_string()],
        risks: "missing context".to_string(),
        assumptions: "docs are the target".to_string(),
    };

    assert!(validate_plan_against_task_contract(&plan, &contract).is_err());
}

#[test]
fn continuation_contract_alignment_terms_admit_resumed_task_vocabulary() {
    let mut contract = derive_task_contract(
        "Continue from the previous session state. Finish the smallest safe fix \
         and run tests before you finish. Final answer must include the changed file path.",
        false,
        true,
        VerificationLevel::Behavioral,
    );
    let plan = PlanBlock {
        goal: "Restore greeting punctuation so the failing test passes".to_string(),
        steps: vec![
            "Inspect the code".to_string(),
            "Apply the punctuation edit".to_string(),
            "Verify the behavior".to_string(),
        ],
        acceptance_criteria: vec!["the previously failing test now passes".to_string()],
        risks: "wrong file".to_string(),
        assumptions: "fixture compiles".to_string(),
    };

    // A meta resume prompt cannot prove alignment on its own wording.
    assert!(validate_plan_against_task_contract(&plan, &contract).is_err());

    contract.alignment_terms = vec![
        "Fix the failing test with the smallest code change. Run tests before you finish."
            .to_string(),
    ];
    assert!(validate_plan_against_task_contract(&plan, &contract).is_ok());
}

#[test]
fn is_continuation_request_detects_resume_and_bridge_roots() {
    for text in [
        "Continue from the previous session state. Finish the smallest safe fix.",
        crate::task_origin::continuation_prompt("en"),
        crate::task_origin::continuation_prompt("ja"),
        crate::task_origin::continuation_prompt("fr"),
        "Resume the interrupted session repair",
    ] {
        assert!(is_continuation_request(text), "{text}");
    }
    for text in [
        "Fix the failing test with the smallest code change.",
        "Locate the slash command handler without editing files",
        "rewrite README prose",
    ] {
        assert!(!is_continuation_request(text), "{text}");
    }
}

#[test]
fn governor_block_relief_counts_recovered_distinct_blocks_only() {
    let plan_block = json!({"role":"tool","tool_call_id":"c1","content":"GOVERNOR BLOCKED\n\n[Plan Gate] Missing valid <plan>."});
    let shape_block = json!({"role":"tool","tool_call_id":"c2","content":"GOVERNOR BLOCKED\n\n[Plan Gate] Invalid <plan>: plan must include at least 2 numbered steps in `steps:`"});
    let turn = json!({"role":"assistant","content":"next turn"});
    let tool_ok = json!({"role":"tool","tool_call_id":"ok","content":"[src/lib.rs] (13 lines)"});

    // A block with no later successful tool turn earns nothing yet.
    let trailing = vec![
        json!({"role":"user","content":"task"}),
        turn.clone(),
        plan_block.clone(),
        turn.clone(),
    ];
    assert_eq!(governor_block_iter_relief(&trailing), 0);
    assert_eq!(count_governor_blocked_tool_turns(&trailing), 1);

    // Once a later tool result lands, each distinct gate reason earns one
    // iteration; identical repeats of a reason do not stack.
    let recovered = vec![
        json!({"role":"user","content":"task"}),
        turn.clone(),
        plan_block.clone(),
        tool_ok.clone(),
        turn.clone(),
        shape_block.clone(),
        tool_ok.clone(),
        turn.clone(),
        plan_block.clone(),
        tool_ok.clone(),
        turn.clone(),
    ];
    assert_eq!(governor_block_iter_relief(&recovered), 2);
    assert_eq!(count_governor_blocked_tool_turns(&recovered), 3);

    // Blocks from a prior task do not leak into the current task window.
    let prior_task = vec![
        json!({"role":"user","content":"first task"}),
        turn.clone(),
        plan_block.clone(),
        tool_ok.clone(),
        json!({"role":"user","content":"new task"}),
        turn.clone(),
    ];
    assert_eq!(governor_block_iter_relief(&prior_task), 0);
    assert_eq!(count_governor_blocked_tool_turns(&prior_task), 0);
}

#[test]
fn prior_task_texts_drops_only_the_current_root() {
    let messages = vec![
        crate::task_origin::user_message(
            "Fix the failing test",
            crate::task_origin::MessageOrigin::User,
        ),
        json!({"role":"assistant","content":"working"}),
        crate::task_origin::user_message(
            "runtime note",
            crate::task_origin::MessageOrigin::Runtime,
        ),
        json!({"role":"user","content":"Continue from the previous session state"}),
    ];
    assert_eq!(
        crate::task_origin::prior_task_texts(&messages),
        vec!["Fix the failing test"]
    );
    assert_eq!(
        crate::task_origin::prior_task_texts(&messages[..1]),
        Vec::<&str>::new()
    );
}

#[test]
fn instruction_priority_uses_authority_then_explicit_then_locality_then_sequence() {
    let root = InstructionPriority {
        authority: InstructionAuthority::Root,
        explicit: false,
        locality: 0,
        sequence: 1,
    };
    let execution = InstructionPriority {
        authority: InstructionAuthority::Execution,
        explicit: true,
        locality: 4,
        sequence: 99,
    };
    let more_local = InstructionPriority {
        authority: InstructionAuthority::System,
        explicit: true,
        locality: 3,
        sequence: 1,
    };
    let less_local = InstructionPriority {
        authority: InstructionAuthority::System,
        explicit: true,
        locality: 2,
        sequence: 9,
    };
    let newer = InstructionPriority {
        authority: InstructionAuthority::System,
        explicit: true,
        locality: 3,
        sequence: 2,
    };

    assert!(root.outranks(&execution));
    assert!(more_local.outranks(&less_local));
    assert!(newer.outranks(&more_local));
}

#[test]
fn instruction_resolver_blocks_read_only_mutation_tool() {
    let resolver =
        InstructionResolver::new("Locate the handler without editing files", true, false);
    let tc = ToolCallData {
        thought_signature: None,
        id: "call_1".to_string(),
        name: "patch_file".to_string(),
        arguments: serde_json::json!({
            "path": "src/tui/agent.rs",
            "search": "old",
            "replace": "new"
        })
        .to_string(),
    };

    let conflict = resolver
        .tool_conflict(&tc, None)
        .expect("read-only mutation should conflict");
    assert!(conflict
        .render()
        .contains("Higher-authority instruction wins"));
    assert!(conflict.render().contains("system/task_contract"));
}

#[test]
fn instruction_resolver_rejects_read_only_plan_violation() {
    let resolver =
        InstructionResolver::new("Locate the handler without editing files", true, false);
    let plan = PlanBlock {
        goal: "patch the slash handler".to_string(),
        steps: vec![
            "edit src/tui/events.rs".to_string(),
            "run cargo test".to_string(),
        ],
        acceptance_criteria: vec!["handler is patched".to_string()],
        risks: "wrong file".to_string(),
        assumptions: "editing is allowed".to_string(),
    };

    let err = validate_plan_against_instruction_resolver(&plan, &resolver)
        .expect_err("read-only plan should be rejected");
    assert!(err.to_string().contains("[Instruction Resolver]"));
}

#[test]
fn assumption_ledger_marks_refuted_and_confirms_from_working_memory() {
    let mut ledger = AssumptionLedger::default();
    ledger.remember_unknown("reflection prompt exists");
    ledger.mark_refuted(
        "cargo check works unchanged",
        Some("run targeted tests instead"),
    );

    let mut mem = WorkingMemory::default();
    mem.remember_fact("reflection prompt exists in src/tui/agent.rs");
    ledger.refresh_confirmations(&mem);

    assert!(ledger.entries.iter().any(|entry| {
        entry.text == "reflection prompt exists" && entry.status == AssumptionStatus::Confirmed
    }));
    assert!(ledger.entries.iter().any(|entry| {
        entry.text == "cargo check works unchanged" && entry.status == AssumptionStatus::Refuted
    }));
}

#[test]
fn refuted_assumption_conflict_blocks_reuse() {
    let mut ledger = AssumptionLedger::default();
    ledger.mark_refuted("cargo fix works unchanged", Some("exit code was 1"));

    let think = ThinkBlock {
        goal: "verify the build quickly".to_string(),
        step: 2,
        tool: "exec".to_string(),
        risk: "same build failure".to_string(),
        doubt: "might still fail".to_string(),
        next: "cargo fix".to_string(),
        verify: "exit code is zero".to_string(),
    };
    let tc = ToolCallData {
        thought_signature: None,
        id: "call_1".to_string(),
        name: "exec".to_string(),
        arguments: serde_json::json!({"command":"cargo fix"}).to_string(),
    };

    let msg = refuted_assumption_conflict(
        &ledger,
        &think,
        &tc,
        &exec_verification::ExecVerificationContext::from_root(None, ""),
    )
    .expect("refuted assumption should conflict");
    assert!(msg.contains("cargo fix works unchanged"));
}

#[test]
fn render_cached_prompt_uses_compact_after_unchanged_full_prompt() {
    let mut slot = None;
    let full = "[Task Contract]
- task: tighten token usage"
        .to_string();
    let compact = "[Task Contract cache]
hash: deadbeef"
        .to_string();

    assert_eq!(
        render_cached_prompt(&mut slot, full.clone(), compact.clone()),
        full
    );
    assert_eq!(
        render_cached_prompt(&mut slot, full.clone(), compact.clone()),
        compact
    );
    assert_eq!(
        render_cached_prompt(
            &mut slot,
            "[Task Contract]
- task: changed"
                .to_string(),
            compact,
        ),
        "[Task Contract]
- task: changed"
    );
}

#[test]
fn prune_old_assistant_messages_compacts_stale_structured_turns() {
    let mut messages = Vec::new();
    for idx in 0..(KEEP_RECENT_ASSISTANT_TURNS + 1) {
        messages.push(json!({
            "role": "assistant",
            "content": format!(
                "<plan>
goal: inspect token flow {idx}
steps: 1) inspect
acceptance: 1) token flow understood
risks: drift
assumptions: cache exists
</plan>"
            )
        }));
    }

    let original_last = messages
        .last()
        .and_then(|msg| msg["content"].as_str())
        .unwrap()
        .to_string();
    prune_old_assistant_messages(&mut messages);

    assert!(messages[0]["content"]
        .as_str()
        .unwrap_or("")
        .contains("[assistant-summary]"));
    assert_eq!(
        messages
            .last()
            .and_then(|msg| msg["content"].as_str())
            .unwrap_or(""),
        original_last
    );
}
