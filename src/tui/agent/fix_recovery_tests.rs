use super::*;
use serde_json::json;

fn setup() -> (TaskHarness, Vec<Value>, String) {
    let body = "const PATHS: &[&str] = &[\n    \"src/runtime/existing.rs\",\n];\n\npub fn requires_review(path: &str) -> bool { PATHS.contains(&path) }\n";
    let messages = vec![
        json!({"role":"assistant","tool_calls":[{"id":"read","type":"function","function":{
            "name":"read_file","arguments":json!({"path":"src/runtime/registry.rs"}).to_string()
        }}]}),
        json!({"role":"tool","tool_call_id":"read","content":format!("[src/runtime/registry.rs] (6 lines, 150 bytes)\n{body}")}),
    ];
    (
        TaskHarness {
            lane: TaskLane::FixExisting,
            artifact_mode: ArtifactMode::ExistingFiles,
        },
        messages,
        "Fix the existing registry so `src/runtime/missing.rs` requires review.".into(),
    )
}

#[test]
fn literal_patch_coercion_preserves_diagnosis_and_verification_stages() {
    let (harness, messages, prompt) = setup();
    for name in ["list_dir", "read_file"] {
        let tc = ToolCallData {
            id: "next".into(),
            name: name.into(),
            arguments: json!({"dir":"src/runtime","path":"src/runtime/registry.rs"}).to_string(),
        };
        for stage in [None, Some(RecoveryStage::Fix)] {
            assert!(coerce_fix_existing_literal_mutation_tool_call(
                harness, &messages, &tc, &prompt, stage
            )
            .is_some());
        }
        for stage in [RecoveryStage::Diagnose, RecoveryStage::Verify] {
            assert!(
                coerce_fix_existing_literal_mutation_tool_call(
                    harness,
                    &messages,
                    &tc,
                    &prompt,
                    Some(stage)
                )
                .is_none(),
                "{name} must not become a mutation during {stage:?}"
            );
        }
    }
}

#[test]
fn blocked_patch_restoration_preserves_diagnosis_and_verification_stages() {
    let (harness, mut messages, prompt) = setup();
    let patch = json!({"path":"src/runtime/registry.rs","search":"    \"src/runtime/existing.rs\",\n];",
        "replace":"    \"src/runtime/existing.rs\",\n    \"src/runtime/missing.rs\",\n];"});
    messages.push(
        json!({"role":"assistant","tool_calls":[{"id":"patch","type":"function","function":{
            "name":"patch_file","arguments":patch.to_string()
        }}]}),
    );
    messages.push(
        json!({"role":"tool","tool_call_id":"patch","content":"GOVERNOR BLOCKED\n[Evidence Gate]"}),
    );
    let tc = ToolCallData {
        id: "next".into(),
        name: "list_dir".into(),
        arguments: json!({"dir":"src/runtime"}).to_string(),
    };
    assert!(coerce_fix_existing_blocked_mutation_tool_call(
        harness,
        &messages,
        &tc,
        &prompt,
        Some(RecoveryStage::Fix)
    )
    .is_some());
    for stage in [RecoveryStage::Diagnose, RecoveryStage::Verify] {
        assert!(coerce_fix_existing_blocked_mutation_tool_call(
            harness,
            &messages,
            &tc,
            &prompt,
            Some(stage)
        )
        .is_none());
    }
}
