use super::*;
use serde_json::json;
use tempfile::tempdir;

#[test]
fn infer_task_harness_detects_fix_lane() {
    let harness = TaskHarness::infer("Fix the failing test with the smallest code change.", false);
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
        thought_signature: None,
        id: "call_search".to_string(),
        name: "search_files".to_string(),
        arguments: json!({"pattern":"runtime eval","path":"src"}).to_string(),
    };

    let (rewritten, original, coerced) =
        coerce_benchmark_plan_tool_call(harness, &[], &tc, "lane: runtime_eval", None, None, None)
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
        thought_signature: None,
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
        thought_signature: None,
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
        thought_signature: None,
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
        thought_signature: None,
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
        thought_signature: None,
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
        thought_signature: None,
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
        thought_signature: None,
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
        thought_signature: None,
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
        thought_signature: None,
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
        thought_signature: None,
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
        thought_signature: None,
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
            thought_signature: None,
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
        thought_signature: None,
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
        thought_signature: None,
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
        thought_signature: None,
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

    assert!(block.contains("read `src/maze.rs` now to inspect the implementation before patching."));
    assert!(
        block.contains("Do NOT rerun verification or widen search until `src/maze.rs` is read.")
    );
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
        thought_signature: None,
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
        thought_signature: None,
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
        None,
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
        thought_signature: None,
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
        thought_signature: None,
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
        thought_signature: None,
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
        thought_signature: None,
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
        thought_signature: None,
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
        thought_signature: None,
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
        thought_signature: None,
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
        thought_signature: None,
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
        None,
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
        thought_signature: None,
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
        thought_signature: None,
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
        None,
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
        thought_signature: None,
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
        None,
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
        thought_signature: None,
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
        thought_signature: None,
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
        thought_signature: None,
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
        thought_signature: None,
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
        Some(
            "test -d demo_repo/.git && test -f demo_repo/README.md && test -f demo_repo/.gitignore",
        ),
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
        thought_signature: None,
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
        Some(
            "test -d demo_repo/.git && test -f demo_repo/README.md && test -f demo_repo/.gitignore",
        ),
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
        thought_signature: None,
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
fn coerce_repo_goal_completion_tool_call_advances_to_missing_required_file_after_binary_crate() {
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
        thought_signature: None,
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
        thought_signature: None,
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
        Some(
            "test -d demo_repo/.git && test -f demo_repo/README.md && test -f demo_repo/.gitignore",
        ),
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
        thought_signature: None,
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
        Some(
            "test -d demo_repo/.git && test -f demo_repo/README.md && test -f demo_repo/.gitignore",
        ),
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
        thought_signature: None,
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
        Some(
            "test -d demo_repo/.git && test -f demo_repo/README.md && test -f demo_repo/.gitignore",
        ),
        None,
    )
    .expect("rewritten");

    assert_eq!(rewritten.name, "write_file");
    assert!(rewritten.arguments.contains("demo_repo/README.md"));
    assert!(rewritten.arguments.contains("# demo_repo\\n"));
    assert!(coerced.contains("README.md"));
}
