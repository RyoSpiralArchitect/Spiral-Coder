use super::*;
use serde_json::json;

const PATH: &str = "src/entry.rs";
const CHECK: &str = "cargo test --locked";
const DIFF: &str = "@@\n-old\n+new\n*** End Patch";

fn call(id: &str, name: &str, args: Value) -> Value {
    json!({"role":"assistant","tool_calls":[{"id":id,"type":"function",
        "function":{"name":name,"arguments":args.to_string()}}]})
}

fn read(back: &str, id: &str, snapshot: Value) -> [Value; 2] {
    [
        call(id, "read_file", json!({"path":PATH})),
        json!({"role":"tool","tool_call_id":id,
        "content":format!("[{PATH}] ({} lines, {} bytes)\n{back}",back.lines().count(),back.len()),
        "recovery_focus":snapshot}),
    ]
}

fn history(diff: &str) -> Vec<Value> {
    let cwd = std::env::temp_dir().display().to_string();
    let pending = json!({"version":2,"state":{"pending":{"path":"src/a.rs","diagnostic":"failed assertion",
        "check":CHECK,"observation":"repair_attempted","location":"current"}}});
    let clear = json!({"version":2,"state":{"pending":null}});
    let mut m = vec![json!({"role":"user","content":"Repair src/entry.rs and verify it."})];
    m.extend(read("old\n", "read", pending));
    let held = recovery_focus::native_snapshot(m.last().unwrap()).unwrap();
    let edit = call("prior", "apply_diff", json!({"path":PATH,"diff":diff}));
    let output = format!("GOVERNOR BLOCKED\ncwd: {cwd}\n\n[Recovery Gate] stage=verify\n{}\n\ntool:\napply_diff\narguments:\n{}",
        held.hint().unwrap(),edit["tool_calls"][0]["function"]["arguments"].as_str().unwrap());
    m.extend([edit,json!({"role":"tool","tool_call_id":"prior","content":output}),
        json!({"role":"system","content":"Observer advisory"}),json!({"role":"user","origin":"runtime","content":"Continue."}),
        call("pass","exec",json!({"command":CHECK,"cwd":cwd})),
        json!({"role":"tool","tool_call_id":"pass","content":format!("OK (exit_code: 0)\ncwd: {cwd}"),"recovery_focus":clear})]);
    m
}

#[test]
fn both_role_contexts_offer_the_original_bytes_only_after_the_native_pass() {
    let mut m = history(DIFF);
    assert!(project(&m[..5]).is_none());
    let view = serde_json::to_value(super::super::task_obligations::project(&m, None)).unwrap();
    let prompt = super::super::task_obligations::prompt(&m, None).unwrap();
    let coder: Value = serde_json::from_str(prompt.lines().last().unwrap()).unwrap();
    assert_eq!(coder["deferred_edit"], view["deferred_edit"]);
    assert_eq!(
        view["deferred_edit"]["suggested_args"],
        json!({"path":PATH,"diff":DIFF})
    );
    assert_eq!(view["deferred_edit"]["proposal_call_id"], "prior");
    assert_eq!(view["deferred_edit"]["verification_call_id"], "pass");
    let clear = m.last().unwrap()["recovery_focus"].clone();
    let [pending, observed] = read("old\n", "recheck", clear);
    m.push(pending.clone());
    assert!(confirming_read(&m, "recheck", PATH));
    assert!(!confirming_read(&m, "other", PATH));
    assert!(!confirming_read(&m, "recheck", "other.rs"));
    m.push(observed);
    assert_eq!(project(&m).unwrap().suggested_args.diff, DIFF);
    let mut twice = m.clone();
    twice.push(pending);
    assert!(!confirming_read(&twice, "recheck", PATH));
    m.last_mut().unwrap()["content"] = json!(format!("[{PATH}] (1 lines, 4 bytes)\nBAD\n"));
    assert!(project(&m).is_none());
}

#[test]
fn failed_wrong_or_uncorrelated_checks_and_unrelated_turns_cannot_release_the_edit() {
    let m = history(DIFF);
    for (index, key, replacement) in [
        (
            8,
            "content",
            json!("FAILED (exit_code: 1)\nstdout:\nOK (exit_code: 0)"),
        ),
        (8, "tool_call_id", json!("other")),
        (8, "recovery_focus", Value::Null),
        (8, "recovery_focus", m[2]["recovery_focus"].clone()),
        (4, "content", json!("REJECTED BY USER")),
        (4, "content", json!("GOVERNOR BLOCKED\n\n[Evidence Gate]")),
        (
            2,
            "content",
            json!("[src/entry.rs] (2 lines, 99 bytes) [pruned 2L]\nold\n"),
        ),
        (2, "recovery_focus", Value::Null),
    ] {
        let mut changed = m.clone();
        changed[index][key] = replacement;
        assert!(project(&changed).is_none(), "{index} {key}");
    }
    for args in [
        json!({"command":"cargo test other"}),
        json!({"command":CHECK,"cwd":"elsewhere"}),
    ] {
        let mut changed = m.clone();
        changed[7] = call("pass", "exec", args);
        assert!(project(&changed).is_none());
    }
    for extra in [
        json!({"role":"user","content":"New task"}),
        json!({"role":"user","origin":"runtime","content":"[implied_write_file]\nchanged"}),
        call("pending", "exec", json!({"command":"edit unknown"})),
        json!({"role":"tool","tool_call_id":"orphan","content":"OK"}),
    ] {
        let mut changed = m.clone();
        changed.push(extra);
        assert!(project(&changed).is_none());
    }
    let mut refused = m.clone();
    refused.extend([
        call("new", "apply_diff", json!({"path":PATH,"diff":DIFF})),
        json!({"role":"tool","tool_call_id":"new","content":"REJECTED BY USER"}),
    ]);
    assert!(project(&refused).is_none());
}

#[test]
fn superseded_ambiguous_out_of_scope_and_mixed_proposals_are_withheld_whole() {
    for d in [
        "@@\n-old\n+new\n*** Update File: elsewhere\n@@\n+data\n*** End Patch",
        "@@\n-old\n+new\n@@\n-other\n+next",
        "@@\n+no anchor",
        "@@\n-missing\n+new",
        "@@\n-old\n+old",
    ] {
        assert!(project(&history(d)).is_none(), "{d}");
    }
    let mut m = history(DIFF);
    m[2]["content"] = json!(format!("[{PATH}] (2 lines, 8 bytes)\nold\nold\n"));
    assert!(project(&m).is_none());
    let mut m = history(DIFF);
    m[0]["content"]=json!("Repair the declared file.\n<task_requirements>\n{\"version\":1,\"requirements\":[{\"id\":\"repair\",\"description\":\"Edit the source\",\"artifacts\":[{\"path\":\"other.rs\",\"kind\":\"source\"}]}]}\n</task_requirements>");
    assert!(project(&m).is_none());
}

#[test]
fn a_unique_substring_inside_a_different_line_is_not_a_retry_anchor() {
    let mut m = history("@@\n-old\n+new");
    m[2]["content"] = json!(format!("[{PATH}] (1 lines, 5 bytes)\ncold\n"));
    assert!(project(&m).is_none());
}
