use super::{
    coerce_existing_followup_tool_call, matches_required_existing_followup,
    required_existing_followup_no_tool_hint,
};
use crate::streaming::ToolCallData;
use serde_json::json;

fn native_read(path: &str, body: &str) -> String {
    format!(
        "[{path}] ({} lines, {} bytes)\n{body}",
        body.lines().count(),
        body.len()
    )
}

#[test]
fn coerces_missing_docs_followup_to_read_docs_first() {
    let messages = vec![json!({
        "role": "tool",
        "content": "[auto-test] ✗ FAILED (exit 1)\nmissing docs follow-up for src/tui/review_panel.rs"
    })];
    let tc = ToolCallData {
        thought_signature: None,
        id: "call_search".to_string(),
        name: "search_files".to_string(),
        arguments: json!({"pattern":"review_panel"}).to_string(),
    };

    let (rewritten, original, coerced) = coerce_existing_followup_tool_call(
        &messages,
        &tc,
        "Update docs/runtime-architecture.md and .spiral-coder/tui_replay.json too.",
        None,
    )
    .expect("docs followup coercion");

    assert_eq!(original, "search_files(pattern=review_panel)");
    assert_eq!(rewritten.name, "read_file");
    assert_eq!(coerced, "read_file(path=docs/runtime-architecture.md)");
}

#[test]
fn coerces_missing_replay_followup_to_patch_after_read() {
    let messages = vec![
        json!({
            "role":"assistant",
            "tool_calls":[{
                "id":"call_read_replay",
                "type":"function",
                "function":{"name":"read_file","arguments":"{\"path\":\".spiral-coder/tui_replay.json\"}"}
            }]
        }),
        json!({
            "role":"tool",
            "tool_call_id":"call_read_replay",
            "content":native_read(".spiral-coder/tui_replay.json", "{\n  \"version\": 1,\n  \"paths\": [\n    \"src/tui/events.rs\"\n  ]\n}\n")
        }),
        json!({
            "role":"tool",
            "content":"[auto-test] ✗ FAILED (exit 1)\nmissing tui replay follow-up for src/tui/review_panel.rs"
        }),
    ];
    let tc = ToolCallData {
        thought_signature: None,
        id: "call_exec".to_string(),
        name: "exec".to_string(),
        arguments: json!({"command":"cargo test 2>&1"}).to_string(),
    };

    let (rewritten, _original, coerced) = coerce_existing_followup_tool_call(
        &messages,
        &tc,
        "Update .spiral-coder/tui_replay.json too.",
        None,
    )
    .expect("replay followup coercion");

    assert_eq!(rewritten.name, "patch_file");
    assert!(rewritten
        .arguments
        .contains("\"path\":\".spiral-coder/tui_replay.json\""));
    assert!(rewritten.arguments.contains("src/tui/review_panel.rs"));
    assert!(coerced.starts_with("patch_file("));
}

#[test]
fn coerces_missing_runtime_eval_followup_to_read_spec_first() {
    let messages = vec![json!({
        "role": "tool",
        "content": "[auto-test] ✗ FAILED (exit 1)\nmissing runtime eval follow-up for src/tui/agent/session_bridge.rs"
    })];
    let tc = ToolCallData {
        thought_signature: None,
        id: "call_search".to_string(),
        name: "search_files".to_string(),
        arguments: json!({"pattern":"session_bridge"}).to_string(),
    };

    let (rewritten, _original, coerced) = coerce_existing_followup_tool_call(
        &messages,
        &tc,
        "Update docs/state-schema.md and .spiral-coder/runtime_eval.json to include `src/tui/agent/session_bridge.rs`.",
        None,
    )
    .expect("runtime eval followup coercion");

    assert_eq!(rewritten.name, "read_file");
    assert_eq!(coerced, "read_file(path=.spiral-coder/runtime_eval.json)");
}

#[test]
fn coerces_missing_runtime_eval_followup_to_patch_after_read() {
    let messages = vec![
        json!({
            "role":"assistant",
            "tool_calls":[{
                "id":"call_read_eval",
                "type":"function",
                "function":{"name":"read_file","arguments":"{\"path\":\".spiral-coder/runtime_eval.json\"}"}
            }]
        }),
        json!({
            "role":"tool",
            "tool_call_id":"call_read_eval",
            "content":native_read(".spiral-coder/runtime_eval.json", "{\n  \"version\": 1,\n  \"paths\": [\n    \"src/tui/agent/task_harness.rs\"\n  ]\n}\n")
        }),
        json!({
            "role":"tool",
            "content":"[auto-test] ✗ FAILED (exit 1)\nmissing runtime eval follow-up for src/tui/agent/session_bridge.rs"
        }),
    ];
    let tc = ToolCallData {
        thought_signature: None,
        id: "call_exec".to_string(),
        name: "exec".to_string(),
        arguments: json!({"command":"cargo test 2>&1"}).to_string(),
    };

    let (rewritten, _original, coerced) = coerce_existing_followup_tool_call(
        &messages,
        &tc,
        "Update .spiral-coder/runtime_eval.json to include `src/tui/agent/session_bridge.rs`.",
        None,
    )
    .expect("runtime eval patch followup");

    assert_eq!(rewritten.name, "patch_file");
    assert!(rewritten
        .arguments
        .contains("\"path\":\".spiral-coder/runtime_eval.json\""));
    assert!(rewritten
        .arguments
        .contains("src/tui/agent/session_bridge.rs"));
    assert!(coerced.starts_with("patch_file("));
}

#[test]
fn no_tool_hint_blocks_done_when_runtime_eval_followup_was_only_read() {
    let messages = vec![
        json!({
            "role":"assistant",
            "tool_calls":[{
                "id":"call_patch_src",
                "type":"function",
                "function":{"name":"patch_file","arguments":"{\"path\":\"src/tui/agent/followup_requirements.rs\",\"search\":\"old\",\"replace\":\"src/tui/agent/session_bridge.rs\"}"}
            }]
        }),
        json!({
            "role":"tool",
            "tool_call_id":"call_patch_src",
            "content":"OK: patched 'src/tui/agent/followup_requirements.rs' (+1 lines, 25 total)"
        }),
        json!({
            "role":"assistant",
            "tool_calls":[{
                "id":"call_read_eval",
                "type":"function",
                "function":{"name":"read_file","arguments":"{\"path\":\".spiral-coder/runtime_eval.json\"}"}
            }]
        }),
        json!({
            "role":"tool",
            "tool_call_id":"call_read_eval",
            "content":native_read(".spiral-coder/runtime_eval.json", "{\n  \"version\": 1,\n  \"paths\": [\n    \"src/tui/agent/task_harness.rs\"\n  ]\n}\n")
        }),
    ];

    let hint = required_existing_followup_no_tool_hint(
        &messages,
        "Update .spiral-coder/runtime_eval.json to include `src/tui/agent/session_bridge.rs`.",
    )
    .expect("pending hint");

    assert!(hint.contains("patch_file(path=.spiral-coder/runtime_eval.json)"));
    assert!(hint.contains("src/tui/agent/session_bridge.rs"));
}

#[test]
fn coerces_prompt_required_docs_followup_to_patch_after_docs_read() {
    let messages = vec![
        json!({
            "role":"assistant",
            "tool_calls":[{
                "id":"call_patch_rules",
                "type":"function",
                "function":{"name":"patch_file","arguments":"{\"path\":\"src/observer/repo_rules.rs\",\"search\":\"old\",\"replace\":\"new\"}"}
            }]
        }),
        json!({
            "role":"tool",
            "tool_call_id":"call_patch_rules",
            "content":"OK: patched 'src/observer/repo_rules.rs' (+1 lines, 3 total)"
        }),
        json!({
            "role":"assistant",
            "tool_calls":[{
                "id":"call_read_docs",
                "type":"function",
                "function":{"name":"read_file","arguments":"{\"path\":\"docs/runtime-architecture.md\"}"}
            }]
        }),
        json!({
            "role":"tool",
            "tool_call_id":"call_read_docs",
            "content":native_read("docs/runtime-architecture.md", "# Runtime architecture\n\n- `src/tui/events.rs`\n- `src/tui/app.rs`\n")
        }),
    ];
    let tc = ToolCallData {
        thought_signature: None,
        id: "call_read_again".to_string(),
        name: "read_file".to_string(),
        arguments: json!({"path":"docs/runtime-architecture.md"}).to_string(),
    };

    let (rewritten, _original, coerced) = coerce_existing_followup_tool_call(
        &messages,
        &tc,
        "Update docs/runtime-architecture.md and .spiral-coder/tui_replay.json to include `src/tui/review_panel.rs`.",
        None,
    )
    .expect("prompt-driven docs followup");

    assert_eq!(rewritten.name, "patch_file");
    assert!(rewritten
        .arguments
        .contains("\"path\":\"docs/runtime-architecture.md\""));
    assert!(rewritten.arguments.contains("src/tui/review_panel.rs"));
    assert!(coerced.starts_with("patch_file("));
}

#[test]
fn redirects_repeated_fix_patch_to_required_docs_followup() {
    let messages = vec![
        json!({
            "role":"assistant",
            "tool_calls":[{
                "id":"call_patch_rules",
                "type":"function",
                "function":{"name":"patch_file","arguments":"{\"path\":\"src/observer/repo_rules.rs\",\"search\":\"old\",\"replace\":\"new\"}"}
            }]
        }),
        json!({
            "role":"tool",
            "tool_call_id":"call_patch_rules",
            "content":"OK: patched 'src/observer/repo_rules.rs' (+1 lines, 3 total)"
        }),
        json!({
            "role":"tool",
            "content":"FAILED (exit_code: 1)\nstderr:\nmissing docs follow-up for src/tui/review_panel.rs"
        }),
    ];
    let tc = ToolCallData {
        thought_signature: None,
        id: "call_patch_rules_again".to_string(),
        name: "patch_file".to_string(),
        arguments: json!({"path":"src/observer/repo_rules.rs","search":"old","replace":"new"})
            .to_string(),
    };

    let (rewritten, _original, coerced) = coerce_existing_followup_tool_call(
        &messages,
        &tc,
        "Update docs/runtime-architecture.md and .spiral-coder/tui_replay.json to include `src/tui/review_panel.rs`.",
        None,
    )
    .expect("docs followup from repeated patch");

    assert_eq!(rewritten.name, "read_file");
    assert!(rewritten
        .arguments
        .contains("\"path\":\"docs/runtime-architecture.md\""));
    assert_eq!(coerced, "read_file(path=docs/runtime-architecture.md)");
}

#[test]
fn advances_from_docs_patch_to_replay_patch_after_successful_followup() {
    let messages = vec![
        json!({
            "role":"assistant",
            "tool_calls":[{
                "id":"call_patch_rules",
                "type":"function",
                "function":{"name":"patch_file","arguments":"{\"path\":\"src/observer/repo_rules.rs\",\"search\":\"old\",\"replace\":\"new\"}"}
            }]
        }),
        json!({
            "role":"tool",
            "tool_call_id":"call_patch_rules",
            "content":"OK: patched 'src/observer/repo_rules.rs' (+1 lines, 3 total)"
        }),
        json!({
            "role":"assistant",
            "tool_calls":[{
                "id":"call_read_docs",
                "type":"function",
                "function":{"name":"read_file","arguments":"{\"path\":\"docs/runtime-architecture.md\"}"}
            }]
        }),
        json!({
            "role":"tool",
            "tool_call_id":"call_read_docs",
            "content":native_read("docs/runtime-architecture.md", "# Runtime architecture\n\n- `src/tui/events.rs`\n- `src/tui/app.rs`\n")
        }),
        json!({
            "role":"assistant",
            "tool_calls":[{
                "id":"call_patch_docs",
                "type":"function",
                "function":{"name":"patch_file","arguments":"{\"path\":\"docs/runtime-architecture.md\",\"search\":\"- `src/tui/app.rs`\\n\",\"replace\":\"- `src/tui/app.rs`\\n- `src/tui/review_panel.rs`\\n\"}"}
            }]
        }),
        json!({
            "role":"tool",
            "tool_call_id":"call_patch_docs",
            "content":"OK: patched 'docs/runtime-architecture.md' (+1 lines, 4 total)"
        }),
        json!({
            "role":"assistant",
            "tool_calls":[{
                "id":"call_read_replay",
                "type":"function",
                "function":{"name":"read_file","arguments":"{\"path\":\".spiral-coder/tui_replay.json\"}"}
            }]
        }),
        json!({
            "role":"tool",
            "tool_call_id":"call_read_replay",
            "content":native_read(".spiral-coder/tui_replay.json", "{\n  \"paths\": [\n    \"src/tui/events.rs\"\n  ]\n}\n")
        }),
    ];
    let tc = ToolCallData {
        thought_signature: None,
        id: "call_exec".to_string(),
        name: "exec".to_string(),
        arguments: json!({"command":"cargo test 2>&1"}).to_string(),
    };

    let (rewritten, _original, coerced) = coerce_existing_followup_tool_call(
        &messages,
        &tc,
        "Update docs/runtime-architecture.md and .spiral-coder/tui_replay.json to include `src/tui/review_panel.rs`.",
        None,
    )
    .expect("replay followup after docs patch");

    assert_eq!(rewritten.name, "patch_file");
    assert!(rewritten
        .arguments
        .contains("\"path\":\".spiral-coder/tui_replay.json\""));
    assert!(rewritten.arguments.contains("src/tui/review_panel.rs"));
    assert!(coerced.starts_with("patch_file("));
}

#[test]
fn advances_from_docs_patch_to_replay_read_before_verify() {
    let messages = vec![
        json!({
            "role":"assistant",
            "tool_calls":[{
                "id":"call_patch_rules",
                "type":"function",
                "function":{"name":"patch_file","arguments":"{\"path\":\"src/observer/repo_rules.rs\",\"search\":\"old\",\"replace\":\"new\"}"}
            }]
        }),
        json!({
            "role":"tool",
            "tool_call_id":"call_patch_rules",
            "content":"OK: patched 'src/observer/repo_rules.rs' (+1 lines, 3 total)"
        }),
        json!({
            "role":"assistant",
            "tool_calls":[{
                "id":"call_read_docs",
                "type":"function",
                "function":{"name":"read_file","arguments":"{\"path\":\"docs/runtime-architecture.md\"}"}
            }]
        }),
        json!({
            "role":"tool",
            "tool_call_id":"call_read_docs",
            "content":native_read("docs/runtime-architecture.md", "# Runtime architecture\n\n- `src/tui/events.rs`\n- `src/tui/app.rs`\n")
        }),
        json!({
            "role":"assistant",
            "tool_calls":[{
                "id":"call_patch_docs",
                "type":"function",
                "function":{"name":"patch_file","arguments":"{\"path\":\"docs/runtime-architecture.md\",\"search\":\"- `src/tui/app.rs`\\n\",\"replace\":\"- `src/tui/app.rs`\\n- `src/tui/review_panel.rs`\\n\"}"}
            }]
        }),
        json!({
            "role":"tool",
            "tool_call_id":"call_patch_docs",
            "content":"OK: patched 'docs/runtime-architecture.md' (+1 lines, 4 total)"
        }),
    ];
    let tc = ToolCallData {
        thought_signature: None,
        id: "call_exec".to_string(),
        name: "exec".to_string(),
        arguments: json!({"command":"cargo test 2>&1"}).to_string(),
    };

    let (rewritten, _original, coerced) = coerce_existing_followup_tool_call(
        &messages,
        &tc,
        "Update docs/runtime-architecture.md and .spiral-coder/tui_replay.json to include `src/tui/review_panel.rs`.",
        None,
    )
    .expect("replay read followup after docs patch");

    assert_eq!(rewritten.name, "read_file");
    assert_eq!(coerced, "read_file(path=.spiral-coder/tui_replay.json)");
}

#[test]
fn allows_pending_replay_followup_patch_during_verify() {
    let messages = vec![
        json!({
            "role":"assistant",
            "tool_calls":[{
                "id":"call_patch_rules",
                "type":"function",
                "function":{"name":"patch_file","arguments":"{\"path\":\"src/observer/repo_rules.rs\",\"search\":\"old\",\"replace\":\"new\"}"}
            }]
        }),
        json!({
            "role":"tool",
            "tool_call_id":"call_patch_rules",
            "content":"OK: patched 'src/observer/repo_rules.rs' (+1 lines, 3 total)"
        }),
        json!({
            "role":"assistant",
            "tool_calls":[{
                "id":"call_read_docs",
                "type":"function",
                "function":{"name":"read_file","arguments":"{\"path\":\"docs/runtime-architecture.md\"}"}
            }]
        }),
        json!({
            "role":"tool",
            "tool_call_id":"call_read_docs",
            "content":native_read("docs/runtime-architecture.md", "# Runtime architecture\n\n- `src/tui/events.rs`\n- `src/tui/app.rs`\n")
        }),
        json!({
            "role":"assistant",
            "tool_calls":[{
                "id":"call_patch_docs",
                "type":"function",
                "function":{"name":"patch_file","arguments":"{\"path\":\"docs/runtime-architecture.md\",\"search\":\"- `src/tui/app.rs`\\n\",\"replace\":\"- `src/tui/app.rs`\\n- `src/tui/review_panel.rs`\\n\"}"}
            }]
        }),
        json!({
            "role":"tool",
            "tool_call_id":"call_patch_docs",
            "content":"OK: patched 'docs/runtime-architecture.md' (+1 lines, 4 total)"
        }),
        json!({
            "role":"assistant",
            "tool_calls":[{
                "id":"call_read_replay",
                "type":"function",
                "function":{"name":"read_file","arguments":"{\"path\":\".spiral-coder/tui_replay.json\"}"}
            }]
        }),
        json!({
            "role":"tool",
            "tool_call_id":"call_read_replay",
            "content":native_read(".spiral-coder/tui_replay.json", "{\n  \"paths\": [\n    \"src/tui/events.rs\"\n  ]\n}\n")
        }),
    ];
    let tc = ToolCallData {
        thought_signature: None,
        id: "call_patch_replay".to_string(),
        name: "patch_file".to_string(),
        arguments: json!({
            "path":".spiral-coder/tui_replay.json",
            "search":"\"src/tui/events.rs\"",
            "replace":"\"src/tui/events.rs\",\n    \"src/tui/review_panel.rs\""
        })
        .to_string(),
    };

    assert!(matches_required_existing_followup(
        &messages,
        &tc,
        "Update docs/runtime-architecture.md and .spiral-coder/tui_replay.json to include `src/tui/review_panel.rs`.",
    ));
}

#[test]
fn pr_ready_merge_prompt_requires_docs_followup_after_source_patch() {
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

    let hint = required_existing_followup_no_tool_hint(
        &messages,
        "Because PR-ready merge approval paths are documented and benchmarked in this repo, update `docs/state-schema.md` and `.spiral-coder/runtime_eval.json` to match. Final answer must include `src/tui/agent/merge_approval.rs`.",
    )
    .expect("docs followup should be pending");

    assert!(hint.contains("read_file(path=docs/state-schema.md)"));
    assert!(hint.contains("src/tui/agent/merge_approval.rs"));
}

#[test]
fn pr_ready_merge_done_tool_is_redirected_to_docs_followup() {
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
    let tc = ToolCallData {
        thought_signature: None,
        id: "call_done".to_string(),
        name: "done".to_string(),
        arguments: json!({
            "summary":"done",
            "completed_acceptance":[],
            "remaining_acceptance":[],
            "acceptance_evidence":[]
        })
        .to_string(),
    };

    let (rewritten, original, coerced) = coerce_existing_followup_tool_call(
        &messages,
        &tc,
        "Because PR-ready merge approval paths are documented and benchmarked in this repo, update `docs/state-schema.md` and `.spiral-coder/runtime_eval.json` to match. Final answer must include `src/tui/agent/merge_approval.rs`.",
        None,
    )
    .expect("done should be redirected to pending followup");

    assert!(original.starts_with("done("));
    assert_eq!(rewritten.name, "read_file");
    assert_eq!(coerced, "read_file(path=docs/state-schema.md)");
}

#[test]
fn pr_ready_merge_advances_from_docs_to_runtime_eval_followup() {
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
            "content":"OK: patched 'src/tui/agent/merge_approval.rs' (+1 lines, 99 total)"
        }),
        json!({
            "role":"assistant",
            "tool_calls":[{
                "id":"call_read_docs",
                "type":"function",
                "function":{"name":"read_file","arguments":"{\"path\":\"docs/state-schema.md\"}"}
            }]
        }),
        json!({
            "role":"tool",
            "tool_call_id":"call_read_docs",
            "content":native_read("docs/state-schema.md", "# State schema\n\n- `src/tui/agent/session_bridge.rs`\n")
        }),
        json!({
            "role":"assistant",
            "tool_calls":[{
                "id":"call_patch_docs",
                "type":"function",
                "function":{"name":"patch_file","arguments":"{\"path\":\"docs/state-schema.md\",\"search\":\"- `src/tui/agent/session_bridge.rs`\\n\",\"replace\":\"- `src/tui/agent/session_bridge.rs`\\n- `src/tui/agent/merge_approval.rs`\\n\"}"}
            }]
        }),
        json!({
            "role":"tool",
            "tool_call_id":"call_patch_docs",
            "content":"OK: patched 'docs/state-schema.md' (+1 lines, 5 total)"
        }),
    ];
    let tc = ToolCallData {
        thought_signature: None,
        id: "call_exec".to_string(),
        name: "exec".to_string(),
        arguments: json!({"command":"cargo test -q tui::agent::merge_approval::tests:: 2>&1 && bash scripts/pr-ready-smoke.sh"}).to_string(),
    };

    let (rewritten, _original, coerced) = coerce_existing_followup_tool_call(
        &messages,
        &tc,
        "Because PR-ready merge approval paths are documented and benchmarked in this repo, update `docs/state-schema.md` and `.spiral-coder/runtime_eval.json` to match. Final answer must include `src/tui/agent/merge_approval.rs`.",
        None,
    )
    .expect("runtime eval followup should be next");

    assert_eq!(rewritten.name, "read_file");
    assert_eq!(coerced, "read_file(path=.spiral-coder/runtime_eval.json)");
}
