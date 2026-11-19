use super::*;
use serde_json::json;
use std::fs;

fn packet(rule: &str, path: &str, result: &str) -> Value {
    let messages = vec![
        json!({"role":"assistant","tool_calls":[{"id":"early-read","function":{"name":"read_file",
            "arguments":json!({"path":path,"start_line":1,"end_line":2}).to_string()}}]}),
        json!({"role":"tool","tool_call_id":"early-read","content":result}),
    ];
    json!({"task_obligations":{"task_requirements":{"declared":true,"requirements":[{"requirement":{
        "description":rule,"artifacts":[{"path":"tests/public.rs","kind":"test"},{"path":"validator.rs","kind":"source"}]
    }}]}}, "execution_evidence":crate::tui::agent::evidence_packet::project(&messages, None)})
}

const RULE: &str = "Reject duplicate event IDs.";
const TASK: &str = "Use ./writer.rs, validator.rs and tests/public.rs.";

#[test]
fn partial_read_selects_current_writer_and_cites_reset_without_assigning_scope() {
    let workspace = tempfile::tempdir().unwrap();
    let source = "fn begin() {\n    let owner = Owner {\n        events: Vec::new(),\n    };\n}\nfn issue() {\n    let id: usize = owner.events.len();\n    owner.events.push(Event { id });\n}\n";
    fs::write(workspace.path().join("writer.rs"), source).unwrap();
    let header = format!("[writer.rs] (9 lines, {} bytes) [range: lines 1-2; partial file]\nfn begin() {{\n    let owner = Owner {{", source.len());
    let mut packet = packet(RULE, "writer.rs", &header);
    let review = project(&packet, TASK, workspace.path().to_str().unwrap()).unwrap();
    assert_eq!(review.reference_paths, ["writer.rs"]);
    assert_eq!(
        serde_json::to_value(&review.workspace_excerpts).unwrap(),
        json!([{
            "path":"writer.rs","selected_by_read_call_id":"early-read",
            "assignment":{"number":7,"text":"let id: usize = owner.events.len();"},
            "following_use":{"number":8,"text":"owner.events.push(Event { id });"},
            "possible_initializers":[{"number":3,"text":"events: Vec::new(),"}]
        }])
    );
    packet["identity_scope_review"] = json!(review);
    let prompt =
        crate::tui::events::observer_assist::build_tui_next_action_prompt(&packet, "en", "");
    for evidence in [
        "owner.events.len()",
        "events: Vec::new()",
        "two distinct",
        "within-owner duplicate",
        "primary reason",
    ] {
        assert!(prompt.contains(evidence), "missing {evidence}");
    }
}

#[test]
fn shared_issuer_has_no_collection_reset_even_if_a_different_field_starts_empty() {
    let sites = writer_sites(
        "events.rs",
        "r",
        "events: Vec::new(),\nlet event_id = GLOBAL.fetch_add(1, Ordering::Relaxed);",
    );
    assert_eq!(sites.len(), 1);
    assert!(sites[0].possible_initializers.is_empty());
    assert!(sites[0].following_use.is_none());
}

#[test]
fn requires_linked_identity_rule_named_reference_and_matching_successful_read() {
    let workspace = tempfile::tempdir().unwrap();
    let root = workspace.path().to_str().unwrap();
    fs::write(
        workspace.path().join("writer.rs"),
        "let id = queue.events.len();",
    )
    .unwrap();
    let good = packet(RULE, "writer.rs", "[writer.rs] (1 lines, 28 bytes)");
    for task in [
        "tests/public.rs validator.rs",
        "other/writer.rs",
        "../writer.rs",
    ] {
        assert!(project(&good, task, root).is_none_or(|r| r.workspace_excerpts.is_empty()));
    }
    assert!(project(
        &packet("The unique output is ready.", "writer.rs", ""),
        TASK,
        root
    )
    .is_none());
    for result in [
        "ERROR reading writer.rs",
        "[other.rs] (1 lines, 28 bytes)",
        "Awaiting approval",
    ] {
        assert!(project(&packet(RULE, "writer.rs", result), TASK, root)
            .unwrap()
            .workspace_excerpts
            .is_empty());
    }
    fs::write(
        workspace.path().join("writer.rs"),
        "a".repeat(64 * 1024 + 1),
    )
    .unwrap();
    assert!(project(&good, TASK, root)
        .unwrap()
        .workspace_excerpts
        .is_empty());
}
