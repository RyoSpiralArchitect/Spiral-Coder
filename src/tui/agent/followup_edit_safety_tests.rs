use super::super::{
    exec_verification::ExecVerificationContext, EditFailureMemory, RecoveryGovernor, RecoveryStage,
    TaskHarness,
};
use super::*;

const SOURCE: &str = "src/example.rs";
const DOCS: &str = "docs/followup.md";
const CHECK: &str = "cargo test --locked --bin spiral-coder -q";
const ROOT: &str = "Repair src/example.rs and update docs/followup.md to include `src/example.rs`. Run exactly: cargo test --locked --bin spiral-coder -q";

fn call(name: &str, args: Value) -> ToolCallData {
    ToolCallData {
        id: "native-next".into(),
        name: name.into(),
        arguments: args.to_string(),
        thought_signature: None,
    }
}

fn append(messages: &mut Vec<Value>, call: &ToolCallData, output: String) {
    let id = format!("native-{}", messages.len());
    messages.push(json!({"role":"assistant","tool_calls":[{
        "id":id,"type":"function","function":{"name":call.name,"arguments":call.arguments}
    }]}));
    messages.push(json!({"role":"tool","tool_call_id":id,"content":output}));
}

fn setup(body: &str) -> (tempfile::TempDir, Vec<Value>) {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("src")).unwrap();
    std::fs::create_dir(root.path().join("docs")).unwrap();
    std::fs::write(root.path().join(SOURCE), "const OLD: bool = false;\n").unwrap();
    std::fs::write(root.path().join(DOCS), body).unwrap();
    let patch = call(
        "patch_file",
        json!({"path":SOURCE,"search":"false","replace":"true"}),
    );
    let (output, failed) =
        crate::file_tools::tool_patch_file(SOURCE, "false", "true", root.path().to_str());
    assert!(!failed);
    let mut messages = vec![json!({"role":"user","content":ROOT})];
    append(&mut messages, &patch, output);
    (root, messages)
}

fn read(root: &std::path::Path, messages: &mut Vec<Value>, args: Value) -> String {
    let (output, failed) = crate::file_tools::read_range::execute_args(&args, root.to_str());
    assert!(!failed);
    append(messages, &call("read_file", args), output.clone());
    output
}

fn long_docs() -> String {
    (1..=400)
        .map(|line| {
            format!("Line {line}: preserve this documentation and its surrounding content.\n")
        })
        .collect()
}

#[test]
fn native_truncated_read_keeps_authored_localized_edit_and_never_becomes_an_anchor() {
    let body = long_docs();
    let (root, mut messages) = setup(&body);
    let output = read(root.path(), &mut messages, json!({"path":DOCS}));
    assert!(output.contains("[…truncated —"));
    let search = "Line 1: preserve this documentation and its surrounding content.";
    let replace = format!("{search}\nRecovery owner: `{SOURCE}`.");
    let authored = call(
        "patch_file",
        json!({"path":DOCS,"search":search,"replace":replace}),
    );
    assert!(coerce_existing_followup_tool_call(&messages, &authored, ROOT, None).is_none());
    assert!(matches_required_existing_followup(
        &messages, &authored, ROOT
    ));
    let verify = call("exec", json!({"command":CHECK}));
    assert!(coerce_existing_followup_tool_call(&messages, &verify, ROOT, None).is_none());
    let (result, failed) =
        crate::file_tools::tool_patch_file(DOCS, search, &replace, root.path().to_str());
    assert!(!failed, "{result}");
    assert_eq!(
        std::fs::read_to_string(root.path().join(DOCS)).unwrap(),
        body.replacen(search, &replace, 1)
    );
    for name in ["apply_diff", "write_file"] {
        let authored = call(
            name,
            json!({"path":DOCS,"diff":"authored","content":"authored"}),
        );
        assert!(coerce_existing_followup_tool_call(&messages, &authored, ROOT, None).is_none());
    }
}

#[test]
fn failed_anchor_diagnosis_read_reaches_recovery_without_becoming_another_patch() {
    let (root, mut messages) = setup(&long_docs());
    read(root.path(), &mut messages, json!({"path":DOCS}));
    let patch = call(
        "patch_file",
        json!({"path":DOCS,"search":"absent anchor","replace":"repair"}),
    );
    let (output, failed) =
        crate::file_tools::tool_patch_file(DOCS, "absent anchor", "repair", root.path().to_str());
    assert!(failed);
    append(&mut messages, &patch, output);
    let mut recovery = RecoveryGovernor::default();
    recovery.on_fix_result(false, None);
    for args in [
        json!({"path":DOCS}),
        json!({"path":DOCS,"start_line":355,"end_line":368}),
    ] {
        let diagnostic = call("read_file", args);
        assert!(
            coerce_existing_followup_tool_call(&messages, &diagnostic, ROOT, recovery.stage)
                .is_none()
        );
        assert!(recovery
            .maybe_block_tool(
                &diagnostic,
                Some(CHECK),
                TaskHarness::infer(ROOT, false),
                false
            )
            .is_none());
    }
    let args = json!({"path":DOCS,"start_line":355,"end_line":368});
    let output = read(root.path(), &mut messages, args.clone());
    assert!(output.contains("Line 355:"));
    let edits = EditFailureMemory::from_messages(
        &messages,
        &ExecVerificationContext::from_messages(Some(CHECK), &messages),
    );
    assert_eq!(edits.count(), 1);
    assert!(edits.target_was_read());
    recovery.on_diagnostic_result(true);
    assert_eq!(recovery.stage, Some(RecoveryStage::Fix));
    assert!(
        super::super::evidence_packet::project(&messages, Some(CHECK))
            .pending_exact_checks
            .contains(&CHECK.to_owned())
    );
}

#[test]
fn range_and_pruned_reads_survive_roundtrip_without_empty_or_partial_search_synthesis() {
    let (root, mut messages) = setup(&long_docs());
    let args = json!({"path":DOCS,"start_line":355,"end_line":368});
    read(root.path(), &mut messages, args.clone());
    let verify = call("exec", json!({"command":CHECK}));
    assert!(coerce_existing_followup_tool_call(&messages, &verify, ROOT, None).is_none());
    let range = call("read_file", args);
    assert!(
        coerce_existing_followup_tool_call(&messages, &range, ROOT, Some(RecoveryStage::Fix))
            .is_none()
    );
    read(root.path(), &mut messages, json!({"path":DOCS}));
    let read_index = messages.len() - 1;
    for _ in 0..super::super::KEEP_RECENT_TOOL_TURNS {
        read(root.path(), &mut messages, json!({"path":SOURCE}));
    }
    super::super::prune_old_tool_results(&mut messages);
    assert!(messages[read_index]["content"]
        .as_str()
        .unwrap()
        .contains("[pruned "));
    messages.push(crate::task_origin::user_message(
        "Continue.",
        crate::task_origin::MessageOrigin::Runtime,
    ));
    let restored: Vec<Value> =
        serde_json::from_str(&serde_json::to_string(&messages).unwrap()).unwrap();
    assert!(coerce_existing_followup_tool_call(&restored, &verify, ROOT, None).is_none());
    assert!(coerce_existing_followup_tool_call(
        &restored,
        &range,
        ROOT,
        Some(RecoveryStage::Diagnose)
    )
    .is_none());
    assert!(successful_read_contents(&restored)
        .get(DOCS)
        .unwrap()
        .snapshot
        .is_none());
}

#[test]
fn honest_incomplete_done_is_not_replaced_and_pending_proof_is_preserved() {
    let (root, mut messages) = setup("# Follow-up\n\nExisting documentation.\n");
    read(root.path(), &mut messages, json!({"path":DOCS}));
    let done = call(
        "done",
        json!({"summary":"The source changed; docs and exact verification remain.","completed_acceptance":[],"remaining_acceptance":["documentation","verification"],"acceptance_evidence":[]}),
    );
    for stage in [
        None,
        Some(RecoveryStage::Diagnose),
        Some(RecoveryStage::Fix),
        Some(RecoveryStage::Verify),
    ] {
        assert!(coerce_existing_followup_tool_call(&messages, &done, ROOT, stage).is_none());
    }
    assert!(required_existing_followup_no_tool_hint(&messages, ROOT).is_some());
    assert!(
        super::super::evidence_packet::project(&messages, Some(CHECK))
            .pending_exact_checks
            .contains(&CHECK.to_owned())
    );
}

#[test]
fn complete_native_reads_still_synthesize_exact_edits_including_marker_like_user_text() {
    let body = "# Follow-up\n\nLiteral example: [pruned 3L]\n[…truncated — this is actual user-authored text]\n";
    let (root, mut messages) = setup(body);
    let output = read(root.path(), &mut messages, json!({"path":DOCS}));
    let header = output.lines().next().unwrap();
    let cached = format!("{header} [⚡ cached — unchanged since last read]\n{output}");
    append(
        &mut messages,
        &call("read_file", json!({"path":DOCS})),
        cached,
    );
    let next = call("read_file", json!({"path":DOCS}));
    let (patch, _, _) = coerce_existing_followup_tool_call(&messages, &next, ROOT, None).unwrap();
    let args: Value = serde_json::from_str(&patch.arguments).unwrap();
    assert_eq!(args["search"], body);
    let (result, failed) = crate::file_tools::tool_patch_file(
        DOCS,
        body,
        args["replace"].as_str().unwrap(),
        root.path().to_str(),
    );
    assert!(!failed, "{result}");
}

#[test]
fn empty_invalid_or_uncorrelated_reads_never_supply_a_whole_file_anchor() {
    for output in [
        "[docs/followup.md] (0 lines, 0 bytes)\n",
        "[docs/followup.md] (3 lines, 999 bytes)\nshort",
        "ERROR: invalid read_file arguments",
    ] {
        let (_root, mut messages) = setup("");
        append(
            &mut messages,
            &call("read_file", json!({"path":DOCS})),
            output.into(),
        );
        let next = call("exec", json!({"command":CHECK}));
        if let Some((rewritten, _, _)) =
            coerce_existing_followup_tool_call(&messages, &next, ROOT, None)
        {
            assert_ne!(rewritten.name, "patch_file");
        }
    }
    let (_root, mut messages) = setup("");
    messages.push(json!({"role":"tool","tool_call_id":"unmatched","content":"[docs/followup.md] (1 lines, 6 bytes)\nhello\n"}));
    assert!(successful_read_contents(&messages).is_empty());
}

#[test]
fn native_json_guidance_is_not_part_of_a_synthetic_search() {
    let root = tempfile::tempdir().unwrap();
    let path = ".spiral-coder/tui_replay.json";
    std::fs::create_dir(root.path().join(".spiral-coder")).unwrap();
    let body = "{\"paths\":[\"src/old.rs\"]}\n";
    std::fs::write(root.path().join(path), body).unwrap();
    let mut messages = Vec::new();
    let output = read(root.path(), &mut messages, json!({"path":path}));
    assert!(output.contains("[JSON append guidance"));
    let reads = successful_read_contents(&messages);
    assert_eq!(reads[path].snapshot.as_deref(), Some(body));
}

#[test]
fn absolute_read_then_relative_cache_preserves_followup_provenance() {
    let body = "# Follow-up\n\nCurrent documentation.\n";
    let (root, mut messages) = setup(body);
    let absolute = root.path().join(DOCS);
    let native = read(
        root.path(),
        &mut messages,
        json!({"path":absolute.to_str().unwrap()}),
    );
    let cached =
        crate::file_tools::read_cache::for_request(&native, DOCS, root.path().to_str()).unwrap();
    append(
        &mut messages,
        &call("read_file", json!({"path":DOCS})),
        cached,
    );
    let reads = successful_read_contents(&messages);
    assert_eq!(reads[DOCS].snapshot.as_deref(), Some(body));
    let verify = call("exec", json!({"command":CHECK}));
    let (patch, _, _) = coerce_existing_followup_tool_call(&messages, &verify, ROOT, None).unwrap();
    let args: Value = serde_json::from_str(&patch.arguments).unwrap();
    assert_eq!(patch.name, "patch_file");
    assert_eq!(args["path"], DOCS);
    assert_eq!(args["search"], body);
    let (result, failed) = crate::file_tools::tool_patch_file(
        DOCS,
        body,
        args["replace"].as_str().unwrap(),
        root.path().to_str(),
    );
    assert!(!failed, "{result}");
}

fn catalog_contract() -> String {
    let contract = json!({"version":1,"requirements":[{
        "id":"catalog","description":"Add a command and explain how to use it",
        "artifacts":[{"path":SOURCE,"kind":"source"},{"path":DOCS,"kind":"documentation"}],
        "checks":[{"command":CHECK}]
    }]});
    format!("Add a feature to {SOURCE}. Input spec: .spiral-coder/tui_replay.json . Document usage in {DOCS}.\n<task_requirements>\n{contract}\n</task_requirements>")
}

#[test]
fn declared_order_does_not_invent_a_source_literal_after_an_edit() {
    let (_dir, mut messages) = setup("# Guide\n");
    let root = catalog_contract();
    messages[0]["content"] = json!(root);
    for action in [
        call(
            "patch_file",
            json!({"path":SOURCE,"search":"true","replace":"ready"}),
        ),
        call("exec", json!({"command":"cargo test 2>&1"})),
    ] {
        assert!(
            coerce_existing_followup_tool_call(&messages, &action, &root, None).is_none(),
            "replaced authored {} with a speculative follow-up",
            action.name
        );
    }
    assert!(required_existing_followup_no_tool_hint(&messages, &root).is_none());
    let status = super::super::requirement_proof::project(&messages, Some(CHECK));
    assert!(status.pending_artifact_paths.contains(&DOCS.into()));
    assert!(status.pending.iter().any(|p| p.contains(CHECK)));
}

#[test]
fn declared_order_keeps_current_checks_and_requested_document_reads() {
    let (dir, mut messages) = setup("# Guide\n");
    let root = catalog_contract();
    messages[0]["content"] = json!(root);
    std::fs::write(dir.path().join("src/main.rs"), "mod unrelated;\n").unwrap();
    std::fs::write(dir.path().join("src/unrelated.rs"), "// unrelated\n").unwrap();
    read(dir.path(), &mut messages, json!({"path":"src/main.rs"}));
    let harness = TaskHarness::infer("Fix an existing Rust project", false);
    for action in [
        call("exec", json!({"command":"cargo test 2>&1"})),
        call("read_file", json!({"path":DOCS})),
    ] {
        let redirected = super::super::coerce_fix_existing_tool_call(
            harness,
            &messages,
            &action,
            Some(CHECK),
            dir.path().to_str(),
        );
        assert!(
            redirected.is_none(),
            "redirected {} to {:?}",
            action.name,
            redirected.map(|r| r.2)
        );
    }
}

#[test]
fn declared_order_diagnostics_distinguish_required_outputs_from_input_specs() {
    let (_dir, mut messages) = setup("# Guide\n");
    let root = catalog_contract();
    messages[0]["content"] = json!(root);
    append(
        &mut messages,
        &call("read_file", json!({"path":DOCS})),
        format!("[{DOCS}] (1 lines, 120 bytes)\nmissing docs follow-up for {SOURCE}"),
    );
    assert!(
        required_existing_followup_no_tool_hint(&messages, &root).is_none(),
        "file contents cannot supply a failed verification"
    );
    append(
        &mut messages,
        &call("exec", json!({"command":CHECK})),
        format!("FAILED (exit_code: 1)\nmissing tui replay follow-up for {SOURCE}"),
    );
    assert!(
        required_existing_followup_no_tool_hint(&messages, &root).is_none(),
        "an input spec is not a declared output"
    );
    append(
        &mut messages,
        &call("exec", json!({"command":CHECK})),
        format!("FAILED (exit_code: 1)\nmissing docs follow-up for {SOURCE}"),
    );
    let hint = required_existing_followup_no_tool_hint(&messages, &root)
        .expect("failed check for declared docs");
    assert!(hint.contains(DOCS) && hint.contains(SOURCE));
    append(
        &mut messages,
        &call("exec", json!({"command":CHECK})),
        "OK (exit_code: 0)".into(),
    );
    assert!(
        required_existing_followup_no_tool_hint(&messages, &root).is_none(),
        "a later successful exact command supersedes its own diagnostic"
    );
}
