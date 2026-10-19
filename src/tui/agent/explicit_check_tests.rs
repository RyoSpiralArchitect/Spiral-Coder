use super::{pending_commands, required_commands};
use serde_json::json;

#[test]
fn prose_exact_checks_preserve_identity_and_exclude_negation_or_examples() {
    let root = "Use the Python-only configuration. Run exactly: python3 -S -m unittest discover -s tests -p 'test_coupled_pilot.py'\nAlso run exactly: `git diff --check`\nRun exactly: git diff --check\nDo not run exactly: cargo test Nope\nExample: Run exactly: cargo test Nope\nFor example, use this instruction. Run exactly: cargo test Nope\n```text\nRun exactly: cargo test Nope\n```\n> Run exactly: cargo test Nope\nRun exactly: printf changed > source.rs\nThen run exactly: cargo test 'KeepCase'";
    assert_eq!(
        required_commands(root),
        [
            "python3 -S -m unittest discover -s tests -p 'test_coupled_pilot.py'",
            "git diff --check",
            "cargo test 'KeepCase'"
        ]
    );
}

#[test]
fn prose_checks_need_current_individual_native_proof_through_resume() {
    let root = "Run exactly: python3 -S -m unittest\nAlso run exactly: git diff --check";
    let mut messages = vec![json!({"role":"user","content":root})];
    let mut add = |name: &str, args: serde_json::Value, content: &str| {
        let id = format!("call-{}", messages.len());
        messages.push(json!({"role":"assistant","tool_calls":[{"id":id,"function":{"name":name,"arguments":args.to_string()}}]}));
        messages.push(json!({"role":"tool","tool_call_id":id,"content":content}));
    };
    add(
        "patch_file",
        json!({"path":"a.py"}),
        "OK: patched 'a.py'\n[auto-test] ✓ PASSED (exit 0)",
    );
    assert_eq!(
        pending_commands(&messages, root, Some("python3 -S -m unittest")),
        ["python3 -S -m unittest", "git diff --check"]
    );
    for command in required_commands(root) {
        let id = format!("check-{}", messages.len());
        messages.push(json!({"role":"assistant","tool_calls":[{"id":id,"function":{"name":"exec","arguments":json!({"command":command}).to_string()}}]}));
        messages.push(json!({"role":"tool","tool_call_id":id,"content":"OK (exit_code: 0)"}));
    }
    let resumed: Vec<serde_json::Value> =
        serde_json::from_str(&serde_json::to_string(&messages).unwrap()).unwrap();
    assert_eq!(
        pending_commands(&resumed, root, Some("python3 -S -m unittest")),
        Vec::<String>::new()
    );
    messages.push(json!({"role":"assistant","tool_calls":[{"id":"later-edit","function":{"name":"patch_file","arguments":"{\"path\":\"a.py\"}"}}]}));
    messages
        .push(json!({"role":"tool","tool_call_id":"later-edit","content":"OK: patched 'a.py'"}));
    assert_eq!(
        pending_commands(&messages, root, Some("python3 -S -m unittest")),
        required_commands(root)
    );
}
