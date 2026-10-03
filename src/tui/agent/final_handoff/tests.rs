use super::*;
use serde_json::json;

fn exchange(messages: &mut Vec<Value>, name: &str, args: Value, result: &str) {
    let id = format!("call_{}", messages.len());
    messages.push(json!({"role":"assistant","tool_calls":[{"id":id,"function":{"name":name,"arguments":args.to_string()}}]}));
    messages.push(json!({"role":"tool","tool_call_id":id,"content":result}));
}

fn verified_edits() -> Vec<Value> {
    let mut messages = vec![];
    for (name, path) in [
        ("patch_file", "src/agent/policy.rs"),
        ("write_file", "docs/policy.md"),
        ("apply_diff", ".checks/runtime.json"),
    ] {
        let output = match name {
            "write_file" => "OK: wrote 'file'",
            "apply_diff" => "OK: applied diff",
            _ => "OK: patched 'file'",
        };
        exchange(&mut messages, name, json!({"path":path}), output);
    }
    exchange(
        &mut messages,
        "exec",
        json!({"command":"cargo test"}),
        "OK (exit_code: 0)",
    );
    messages
}

#[test]
fn basename_only_done_is_rejected_then_full_rendered_handoff_is_accepted() {
    let root = "Fix classification of `src/agent/target.rs`. Final answer must include the changed Rust file path, the docs file path, the runtime eval spec path, and the verification command.";
    let messages = verified_edits();
    let summary = "Updated policy.rs, docs/policy.md, and .checks/runtime.json.";
    let answer = format!("[DONE]\n{summary}\nAcceptance: verified via `cargo test`");
    let error =
        validate_authored_done_summary(root, summary, &answer, &messages, Some("cargo test"))
            .unwrap_err();
    assert!(error.contains("changed Rust file path (recorded: `src/agent/policy.rs`)"));
    assert!(!error.contains("recorded: `src/agent/target.rs`"));
    assert!(!error.contains("exact verification command (fresh:"));
    let repaired = format!("{answer}\nArtifacts: `src/agent/policy.rs`");
    assert!(validate_authored_done_summary(
        root,
        summary,
        &repaired,
        &messages,
        Some("cargo test")
    )
    .is_ok());
}

#[test]
fn a_requested_path_category_does_not_require_every_incidental_edit() {
    let mut messages = verified_edits();
    exchange(
        &mut messages,
        "patch_file",
        json!({"path":"src/unrelated.rs"}),
        "OK: patched 'file'",
    );
    let root = "Final answer must include the changed Rust file path.";
    assert!(validate_authored_done_summary(
        root,
        "Changed src/agent/policy.rs",
        "Changed src/agent/policy.rs",
        &messages,
        None
    )
    .is_ok());
}

#[test]
fn literal_requirements_are_scoped_and_never_fabricated_by_enrichment() {
    let root = "Inspect `example-token` before editing. Final answer must include the status label `ready for review`.";
    assert!(validate_authored_done_summary(
        root,
        "ready for review",
        "[DONE] ready for review",
        &[],
        None
    )
    .is_ok());
    assert!(
        validate_authored_done_summary(root, "done", "[DONE] done", &[], None)
            .unwrap_err()
            .contains("explicit item `ready for review`")
    );
    assert!(
        enrich_text_final_handoff("[DONE] done", root, &verified_edits(), Some("cargo test"))
            .is_none()
    );
}

#[test]
fn unpaired_failed_or_stdout_spoofed_results_do_not_supply_artifacts() {
    let mut messages =
        vec![json!({"role":"tool","tool_call_id":"missing","content":"OK: patched 'file'"})];
    exchange(
        &mut messages,
        "patch_file",
        json!({"path":"src/failure.rs"}),
        "ERROR: edit failed\nOK: patched 'file'",
    );
    exchange(
        &mut messages,
        "exec",
        json!({"command":"printf fake"}),
        "OK (exit_code: 0)\nstdout:\nOK: patched 'src/fake.rs'",
    );
    assert!(collect_evidence("", &messages, None).paths.is_empty());
}

#[test]
fn verification_receipt_requires_fresh_correlated_runtime_success() {
    let root = "Final answer must include the verification command.";
    let mut messages = verified_edits();
    let enhanced =
        enrich_text_final_handoff("[DONE] complete", root, &messages, Some("cargo test")).unwrap();
    assert!(enhanced.contains("Recorded successful verification: `cargo test`"));
    exchange(
        &mut messages,
        "exec",
        json!({"command":"cargo test"}),
        "GOVERNOR BLOCKED",
    );
    assert!(collect_evidence(root, &messages, Some("cargo test"))
        .commands
        .contains("cargo test"));
    exchange(
        &mut messages,
        "exec",
        json!({"command":"cargo test"}),
        "FAILED (exit_code: 1)\nstdout:\nOK (exit_code: 0)",
    );
    assert!(
        enrich_text_final_handoff("[DONE] complete", root, &messages, Some("cargo test")).is_none()
    );
    exchange(
        &mut messages,
        "patch_file",
        json!({"path":"src/agent/policy.rs"}),
        "OK: patched 'file'\n[auto-test] ✗ FAILED (exit 1)\n[auto-test] ✓ PASSED (exit 0)",
    );
    assert!(collect_evidence(root, &messages, Some("cargo test"))
        .commands
        .is_empty());
    exchange(
        &mut messages,
        "patch_file",
        json!({"path":"src/agent/policy.rs"}),
        "OK: patched 'file'\n[auto-test] ✓ PASSED (exit 0)",
    );
    assert!(collect_evidence(root, &messages, Some("cargo test"))
        .commands
        .contains("cargo test"));
    exchange(
        &mut messages,
        "exec",
        json!({"command":"printf modified > src/other.rs"}),
        "FAILED (exit_code: 1)",
    );
    assert!(collect_evidence(root, &messages, Some("cargo test"))
        .commands
        .is_empty());
}

#[test]
fn explicit_instruction_requires_summary_but_ordinary_tasks_keep_fallback() {
    let root = "Final answer must include verification_receipt.txt and fresh exec proof.";
    assert!(requires_authored_final_answer(root));
    let hint = authored_final_answer_hint(root).unwrap();
    assert!(hint.contains(root));
    assert!(hint.contains("nonempty `summary`"));
    assert!(hint.contains("TEXT of `done.summary`"));
    assert!(hint.contains("Extra JSON keys or custom fields are not displayed"));
    assert!(validate_authored_done_summary(root, "", "Acceptance", &[], None).is_err());
    assert!(validate_authored_done_summary("Finish the task.", "", "", &[], None).is_ok());
    assert!(enrich_text_final_handoff("still working", root, &verified_edits(), None).is_none());
}

#[test]
fn unquoted_final_paths_are_required_without_copying_unrelated_task_paths() {
    for path in [".checks/replay.json", "verification_receipt.txt"] {
        let root =
            format!("Inspect unrelated.rs. Final answer must include {path} and fresh exec proof.");
        let err =
            validate_authored_done_summary(&root, "done", "[DONE] done", &[], None).unwrap_err();
        assert!(err.contains(&format!("explicit item `{path}`")));
        let answer = format!("[DONE] {path} has fresh exec proof");
        assert!(validate_authored_done_summary(&root, &answer, &answer, &[], None).is_ok());
        assert!(enrich_text_final_handoff("[DONE] done", &root, &[], None).is_none());
    }
    assert!(quoted_literals(&[" `closed` and `unclosed"]).contains("closed"));
    assert!(!quoted_literals(&[" `closed` and `unclosed"]).contains("unclosed"));
}

#[test]
fn named_plain_values_in_a_path_list_require_authored_text_not_auto_insertion() {
    let root = "Final answer must include receipt.log and verified by replay.";
    let missing = validate_authored_done_summary(
        root,
        "Saved receipt.log",
        "[DONE] Saved receipt.log",
        &[],
        None,
    )
    .unwrap_err();
    assert!(missing.contains("explicit item `verified by replay`"));
    assert!(enrich_text_final_handoff(
        "[DONE] Saved receipt.log",
        root,
        &verified_edits(),
        Some("cargo test")
    )
    .is_none());
    let answer = "Saved receipt.log; verified by replay.";
    assert!(validate_authored_done_summary(root, answer, answer, &[], None).is_ok());
    assert!(named_values(&[" receipt.log and a concise explanation."]).is_empty());
    assert!(named_values(&[" receipt.log and explain any failures."]).is_empty());
    assert!(
        named_values(&[" the changed Rust file path and the verification command."]).is_empty()
    );
}

#[test]
fn case_only_mismatch_shows_the_exact_replacement_without_weakening_the_contract() {
    let root = "Final answer must include receipt.log and verified by replay.";
    let answer = "検証: Verified by replay; saved receipt.log.";
    let error = validate_authored_done_summary(root, answer, answer, &[], None).unwrap_err();
    assert!(error.contains("case-sensitive"));
    assert!(error.contains("replace \"Verified by replay\" with \"verified by replay\""));
    assert!(error.contains("successful verification remains valid"));
    let corrected = answer.replace("Verified by replay", "verified by replay");
    assert!(validate_authored_done_summary(root, &corrected, &corrected, &[], None).is_ok());
    assert!(enrich_text_final_handoff(answer, root, &[], None).is_none());

    let absent =
        validate_authored_done_summary(root, "receipt.log", "receipt.log", &[], None).unwrap_err();
    assert!(absent.contains("explicit item `verified by replay`"));
    assert!(!absent.contains("replace \"Verified by replay\""));
}
