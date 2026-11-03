use super::*;
use crate::tui::agent;
use tempfile::TempDir;

const TASK: &str = "Repair a.rs and update the guide.\nAllowed paths only:\n- a.rs\n- guide.md";
const SOURCE: &str = "unchanged\nwrong\nblock\nend\n";
fn patch() -> Value {
    json!({"path":"a.rs","search":"wrong\nblock","replace":"right\nblock"})
}
fn snapshot() -> Value {
    json!({"version":2, "state":{"pending":{"path":"","location":"unknown","observation":"unlocalized","check":"cargo test","diagnostic":"compile failed"}}})
}
fn exchange(m: &mut Vec<Value>, id: &str, name: &str, args: Value, content: &str) {
    m.push(json!({"role":"assistant","content":"", "tool_calls":[{"id":id,"function":{"name":name,"arguments":args.to_string()}}]}));
    m.push(json!({"role":"tool","tool_call_id":id,"content":content,"recovery_focus":snapshot()}));
}
fn setup() -> (TempDir, Vec<Value>) {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("a.rs"), SOURCE).unwrap();
    let mut m = vec![crate::task_origin::user_message(
        TASK,
        crate::task_origin::MessageOrigin::User,
    )];
    exchange(
        &mut m,
        "mutation",
        "patch_file",
        json!({"path":"a.rs"}),
        "OK: patched 'a.rs'\n[auto-test] ✗ FAILED (exit 101)\nerror[E0774]: invalid derive",
    );
    let block=format!("GOVERNOR BLOCKED\ncwd: {}\n\n[Recovery Gate] stage=diagnose\nInspect the failure\n\ntool:\npatch_file\narguments:\n{}",root.path().display(),patch());
    exchange(&mut m, "held", "patch_file", patch(), &block);
    m.last_mut()
        .unwrap()
        .as_object_mut()
        .unwrap()
        .remove("recovery_focus");
    (root, m)
}
fn read(m: &mut Vec<Value>, root: &Path, first: usize, last: usize) {
    let args = json!({"path":"a.rs","start_line":first,"end_line":last});
    let (result, failed) = crate::file_tools::read_range::execute_args(&args, root.to_str());
    assert!(!failed);
    exchange(m, "inspect", "read_file", args, &result);
}
fn view(m: &[Value], root: &Path) -> Value {
    json!(project(m, root.to_str().unwrap()))
}

#[test]
fn literal_proposal_returns_after_a_fresh_read_compaction_and_session_reload() {
    let (root, mut m) = setup();
    let pending = view(&m, root.path());
    assert_eq!(
        pending["suggested_args"],
        json!({"path":"a.rs","start_line":2,"end_line":3})
    );
    assert_eq!(pending["suggested_tool"], "read_file");
    assert!(!pending.to_string().contains("right"));
    read(&mut m, root.path(), 2, 3);
    for n in 0..60 {
        exchange(
            &mut m,
            &format!("search{n}"),
            "search_files",
            json!({"pattern":"todo"}),
            "[search_files: todo — 0 matches]",
        );
    }
    m.push(crate::task_origin::user_message(
        "Continue.",
        crate::task_origin::MessageOrigin::Runtime,
    ));
    agent::prune_old_tool_results(&mut m);
    agent::message_window::prune_message_window(&mut m);
    let session = crate::agent_session::AgentSession::new(None, None, None, None, m);
    let resumed: crate::agent_session::AgentSession =
        serde_json::from_slice(&serde_json::to_vec(&session).unwrap()).unwrap();
    let ready = view(&resumed.messages, root.path());
    assert_eq!(ready["suggested_args"], patch());
    assert_eq!(ready["confirming_read_call_id"], "inspect");
    assert_eq!(ready["proposal_call_id"], "held");
    assert_eq!(ready["failed_check"], "cargo test");
    assert_eq!(
        std::fs::read_to_string(root.path().join("a.rs")).unwrap(),
        SOURCE
    );
}

#[test]
fn different_or_ambiguous_source_and_superseding_work_do_not_revive_a_patch() {
    let (root, mut m) = setup();
    read(&mut m, root.path(), 1, 1);
    assert_eq!(view(&m, root.path())["suggested_tool"], "read_file");
    let dir = root.path().join("a.rs");
    for changed in [
        "different",
        "wrong\nblock\nwrong\nblock",
        &format!("{}wrong\nblock", "x".repeat(9000)),
    ] {
        std::fs::write(&dir, changed).unwrap();
        assert!(view(&m, root.path()).is_null());
    }
    std::fs::write(&dir, SOURCE).unwrap();
    for (tool, output) in [
        ("exec", "OK (exit_code: 0)"),
        ("patch_file", "OK: unchanged 'a.rs'"),
        ("done", "OK: blocked"),
        ("read_file", "REJECTED BY USER"),
    ] {
        let mut stale = m.clone();
        exchange(&mut stale, "barrier", tool, json!({"path":"a.rs"}), output);
        for i in 0..51 {
            exchange(
                &mut stale,
                &format!("later{i}"),
                "list_dir",
                json!({}),
                "[list_dir: .]",
            );
        }
        agent::message_window::prune_message_window(&mut stale);
        assert!(view(&stale, root.path()).is_null(), "{tool}");
    }
    m.push(json!({"role":"user","content":"Inspect another file"}));
    assert!(view(&m, root.path()).is_null());
}

#[test]
fn refusals_missing_correlations_scope_and_escaped_targets_never_offer_a_patch() {
    let (root, m) = setup();
    let mut wrong = m.clone();
    wrong[0]["content"] = json!("Repair only the guide.\nAllowed paths only:\n- guide.md");
    assert!(view(&wrong, root.path()).is_null());
    for (index, key, value) in [
        (4, "tool_call_id", json!("wrong")),
        (4, "content", json!("REJECTED BY USER")),
        (2, "recovery_focus", Value::Null),
    ] {
        let mut wrong = m.clone();
        wrong[index][key] = value;
        assert!(view(&wrong, root.path()).is_null());
    }
    let other = tempfile::tempdir().unwrap();
    std::fs::write(other.path().join("a.rs"), SOURCE).unwrap();
    assert!(view(&m, other.path()).is_null());
    #[cfg(unix)]
    {
        std::fs::remove_file(root.path().join("a.rs")).unwrap();
        std::os::unix::fs::symlink(other.path().join("a.rs"), root.path().join("a.rs")).unwrap();
        assert!(view(&m, root.path()).is_null());
    }
}

#[test]
fn only_a_new_matching_call_can_use_the_completed_diagnosis() {
    let (root, mut history) = setup();
    read(&mut history, root.path(), 2, 3);
    let focus = super::super::recovery_focus::native_snapshot(history.last().unwrap()).unwrap();
    let edits = agent::EditFailureMemory::default();
    let call = crate::streaming::ToolCallData {
        id: "retry".into(),
        name: "patch_file".into(),
        arguments: patch().to_string(),
        thought_signature: None,
    };
    let proposed = json!({"role":"assistant","tool_calls":[{"id":"retry","function":{"name":"patch_file","arguments":call.arguments}}]});
    history.push(proposed.clone());
    assert!(matches_retry(
        &history,
        &call,
        root.path().to_str(),
        &focus,
        &edits
    ));
    let mut changed = call.clone();
    changed.arguments =
        json!({"path":"a.rs","search":"wrong\nblock","replace":"other replacement"}).to_string();
    assert!(!matches_retry(
        &history,
        &changed,
        root.path().to_str(),
        &focus,
        &edits
    ));
    history.pop();
    history.truncate(5);
    history.push(proposed);
    assert!(!matches_retry(
        &history,
        &call,
        root.path().to_str(),
        &focus,
        &edits
    ));
}
