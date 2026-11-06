use super::*;
use serde_json::json;

#[test]
fn rescue_missing_plan_for_tool_turn_after_repeated_blocks_for_mistral_actions() {
    let tc = ToolCallData {
        thought_signature: None,
        id: "call_2".to_string(),
        name: "read_file".to_string(),
        arguments: json!({"path":"Cargo.toml"}).to_string(),
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
                    "arguments": json!({"path":"Cargo.toml"}).to_string()
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
        ProviderKind::Mistral,
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
}

#[test]
fn rescue_missing_plan_for_tool_turn_after_single_mixed_diagnostic_block_for_mistral() {
    let tc = ToolCallData {
        thought_signature: None,
        id: "call_2".to_string(),
        name: "read_file".to_string(),
        arguments: json!({"path":"Cargo.toml"}).to_string(),
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
                    "arguments": json!({"path":"src/lib.rs"}).to_string()
                }
            }]
        }),
        json!({
            "role": "tool",
            "tool_call_id": "call_1",
            "content": "GOVERNOR BLOCKED\n\n[Plan Gate] Missing valid <plan>.\n\ntool:\nread_file\narguments:\n{\"path\":\"src/lib.rs\"}"
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
        ProviderKind::Mistral,
        VerificationLevel::Behavioral,
        Some("cargo test 2>&1"),
    )
    .expect("synthetic action plan after mixed diagnostic miss");

    assert!(rescued
        .steps
        .iter()
        .any(|step| step.contains("read_file(path=Cargo.toml)")));
}

#[test]
fn rescue_missing_plan_for_tool_turn_after_single_diagnostic_block_for_openai_actions() {
    let tc = ToolCallData {
        thought_signature: None,
        id: "call_2".to_string(),
        name: "read_file".to_string(),
        arguments: json!({"path":"src/observer/repo_rules.rs"}).to_string(),
    };
    let messages = vec![
        json!({
            "role": "assistant",
            "content": "",
            "tool_calls": [{
                "id": "call_1",
                "type": "function",
                "function": {
                    "name": "list_dir",
                    "arguments": json!({"dir":"src"}).to_string()
                }
            }]
        }),
        json!({
            "role": "tool",
            "tool_call_id": "call_1",
            "content": "GOVERNOR BLOCKED\n\n[Plan Gate] Missing valid <plan>.\n\ntool:\nlist_dir\narguments:\n{\"dir\":\"src\"}"
        }),
    ];

    let rescued = rescue_missing_plan_for_tool_turn(
        &messages,
        &tc,
        "Fix the existing observer rule module with the smallest safe code change.",
        TaskHarness::infer(
            "Fix the existing observer rule module with the smallest safe code change.",
            false,
        ),
        false,
        true,
        ProviderKind::OpenAiCompatible,
        VerificationLevel::Behavioral,
        Some("cargo test 2>&1"),
    )
    .expect("synthetic action plan for openai after one diagnostic miss");

    assert!(rescued
        .steps
        .iter()
        .any(|step| step.contains("read_file(path=src/observer/repo_rules.rs)")));
}

#[test]
fn rescue_missing_think_for_tool_turn_after_repeated_blocks_for_mistral_actions() {
    let tc = ToolCallData {
        thought_signature: None,
        id: "call_2".to_string(),
        name: "read_file".to_string(),
        arguments: json!({"path":"Cargo.toml"}).to_string(),
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
                    "arguments": json!({"path":"Cargo.toml"}).to_string()
                }
            }]
        }),
        json!({
            "role": "tool",
            "tool_call_id": "call_1",
            "content": "GOVERNOR BLOCKED\n\n[Think Gate] Missing <think>.\n\ntool:\nread_file\narguments:\n{\"path\":\"Cargo.toml\"}"
        }),
    ];
    let plan = PlanBlock {
        goal: "Fix the failing test with the smallest code change.".to_string(),
        steps: vec![
            "read_file(path=Cargo.toml) or src/lib.rs to confirm the failing logic".to_string(),
            "patch the smallest confirmed bug".to_string(),
            "run cargo test 2>&1".to_string(),
        ],
        acceptance_criteria: vec![
            "the requested change is implemented and confirmed by a passing behavioral verification command".to_string(),
        ],
        risks: "wrong target or speculative fix".to_string(),
        assumptions: "the failing test still reflects the requested bug".to_string(),
    };

    let rescued = rescue_missing_think_for_tool_turn(
        &messages,
        &tc,
        &plan,
        false,
        true,
        ProviderKind::Mistral,
        false,
    )
    .expect("synthetic think");

    validate_think(&rescued, &plan, &tc).expect("valid synthetic think");
    assert_eq!(rescued.tool, "read_file");
    assert!(rescued.next.contains("read Cargo.toml"));
}

#[test]
fn rescue_missing_think_for_tool_turn_is_immediate_for_openai_read_file() {
    let tc = ToolCallData {
        thought_signature: None,
        id: "call_2".to_string(),
        name: "read_file".to_string(),
        arguments: json!({"path":"src/observer/repo_rules.rs"}).to_string(),
    };
    let plan = PlanBlock {
        goal: "Fix the observer rule change with the smallest code patch.".to_string(),
        steps: vec![
            "read_file(path=src/observer/repo_rules.rs) to confirm the rule list".to_string(),
            "patch the smallest confirmed rule entry".to_string(),
            "run cargo test 2>&1".to_string(),
        ],
        acceptance_criteria: vec![
            "the requested change is implemented and confirmed by a passing behavioral verification command".to_string(),
        ],
        risks: "wrong target or speculative fix".to_string(),
        assumptions: "the failing test still reflects the requested bug".to_string(),
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
    .expect("immediate synthetic think for read_file");

    validate_think(&rescued, &plan, &tc).expect("valid synthetic think");
    assert_eq!(rescued.tool, "read_file");
}
