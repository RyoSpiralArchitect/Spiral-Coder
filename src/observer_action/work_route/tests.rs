use super::*;
use crate::tui::{agent::task_obligations, events::build_observer_suggestion_soft_hint};
use serde_json::json;

fn task() -> Vec<Value> {
    vec![json!({"role":"user","content":format!(
        "Add a ledger reader, public tests and a guide.\n<task_requirements>\n{}\n</task_requirements>",
        json!({"version":1,"requirements":[
            {"id":"reader","description":"Reject duplicate request IDs without changing valid counts.",
             "artifacts":[{"path":"src/ledger.rs","kind":"source"},{"path":"tests/ledger.rs","kind":"test"}],
             "checks":[{"command":"cargo test --test ledger"}]},
            {"id":"negative_cases","description":"Use a passing control; replace one ID with an existing ID and check the public rejection.",
             "artifacts":[{"path":"tests/ledger.rs","kind":"test"}],"checks":[{"command":"cargo test --test ledger"}]},
            {"id":"guide","description":"Document unknown pending usage.",
             "artifacts":[{"path":"guide.md","kind":"documentation"}],"checks":[{"command":"cargo test"}]}
        ]})
    )})]
}

fn exchange(messages: &mut Vec<Value>, id: &str, tool: &str, arguments: Value, content: &str) {
    messages.extend([
        json!({"role":"assistant","tool_calls":[{"id":id,"function":{"name":tool,"arguments":arguments.to_string()}}]}),
        json!({"role":"tool","tool_call_id":id,"content":content}),
    ]);
}

fn packet(messages: &[Value], remaining: Option<usize>) -> Value {
    json!({"task_obligations":task_obligations::project(messages,None),
        "work_budget":{"next_coder_requests":remaining,"next_coder_turn":"work"}})
}

fn route(packet: &Value) -> Value {
    serde_json::to_value(project(packet)).unwrap()
}

#[test]
fn whole_task_keeps_shared_test_links_and_verification_cost_after_proof_is_current() {
    let mut messages = task();
    let initial = route(&packet(&messages, Some(12)));
    assert_eq!(initial["artifacts"]["total"], 3);
    let test = &initial["artifacts"]["items"][2];
    assert_eq!(test["path"], "tests/ledger.rs");
    assert_eq!(test["requirement_ids"], json!(["negative_cases", "reader"]));
    assert_eq!(test["native_edit_recorded"], false);
    assert_eq!(
        initial["verification"]["minimum_edit_then_finish_requests"],
        6
    );
    for (i, path) in ["src/ledger.rs", "tests/ledger.rs", "guide.md"]
        .iter()
        .enumerate()
    {
        exchange(
            &mut messages,
            &format!("edit-{i}"),
            "write_file",
            json!({"path":path,"content":"new"}),
            &format!("OK: wrote '{path}'"),
        );
    }
    for (i, command) in ["cargo test --test ledger", "cargo test"]
        .iter()
        .enumerate()
    {
        exchange(
            &mut messages,
            &format!("check-{i}"),
            "exec",
            json!({"command":command}),
            "OK (exit_code: 0)\ncwd: /work\nstdout:\nall passed",
        );
    }
    let source = packet(&messages, Some(8));
    let original = source.clone();
    let after = route(&source);
    assert_eq!(
        source, original,
        "route projection cannot change native proof"
    );
    assert_eq!(after["verification"]["pending_now"]["items"], json!([]));
    assert_eq!(
        after["verification"]["current_checks_invalidated_by_edit"],
        2
    );
    assert_eq!(
        after["verification"]["minimum_edit_then_finish_requests"],
        4
    );
    assert_eq!(
        after["verification"]["requests_before_full_sweep_and_report"],
        5
    );
    assert_eq!(after["artifacts"]["items"][2]["native_edit_recorded"], true);
    assert_eq!(
        after["test_review"], initial["test_review"],
        "passing checks cannot invent test coverage"
    );
    assert_eq!(
        after["test_review"]["dedicated_cases"]["items"][0]["id"],
        "negative_cases"
    );
    assert_eq!(
        after["test_review"]["shared_behavior_rules"]["items"][0]["id"],
        "reader"
    );

    exchange(
        &mut messages,
        "last-edit",
        "patch_file",
        json!({"path":"guide.md","search":"old","replace":"new"}),
        "OK: patched 'guide.md'",
    );
    let changed = route(&packet(&messages, Some(2)));
    assert_eq!(changed["verification"]["pending_now"]["total"], 2);
    assert_eq!(
        changed["verification"]["current_checks_invalidated_by_edit"],
        0
    );
    assert_eq!(
        changed["verification"]["requests_before_full_sweep_and_report"],
        0
    );
    assert_eq!(
        changed["verification"]["full_sweep_and_report_shortfall"],
        1
    );
}

#[test]
fn missing_and_malformed_accounting_stays_unknown() {
    let p = packet(&task(), None);
    assert!(route(&p)["verification"]["requests_before_full_sweep_and_report"].is_null());
    assert!(route(&json!({}))["verification"].is_null());
    let mut p = p;
    p["task_obligations"]["pending_exact_checks"] = json!([false]);
    assert!(route(&p)["verification"].is_null());
    p["task_obligations"]["task_requirements"]["pending_artifact_paths"] = Value::Null;
    assert!(route(&p)["artifacts"]["items"][0]["native_edit_recorded"].is_null());
}

#[test]
fn baseline_reproduction_is_additional_work_and_report_rows_keep_their_order() {
    let mut p = packet(&task(), Some(12));
    p["task_obligations"]["task_requirements"]["pending_baselines"] = json!(["cargo test"]);
    p["task_obligations"]["final_answer"]["outcome_rows"] =
        json!(["Before: FAILED", "After: UNVERIFIED"]);
    let view = route(&p);
    assert_eq!(view["verification"]["minimum_edit_then_finish_requests"], 7);
    assert_eq!(
        view["verification"]["pending_baseline_checks"]["items"],
        json!(["cargo test"])
    );
    assert_eq!(
        view["report_outcome_rows"]["items"],
        json!(["Before: FAILED", "After: UNVERIFIED"])
    );
}

#[test]
fn every_mode_keeps_the_whole_task_but_preserves_native_precedence() {
    for mode in ["general", "diagnostic", "handoff", "report"] {
        let mut p = packet(&task(), Some(12));
        match mode {
            "diagnostic" => {
                p["task_obligations"]["required_diagnostic_read"] =
                    json!({"path":"checks/assertion.py","failed_check":"cargo test"})
            }
            "handoff" => p["work_budget"]["next_coder_turn"] = json!("exhausted"),
            "report" => {
                p["task_obligations"]["report_repair"] = json!({"feedback":"missing literal"})
            }
            _ => (),
        }
        let prompt = crate::tui::events::observer_assist::build_tui_next_action_prompt(
            &p,
            "en",
            "incomplete_task",
        );
        assert!(prompt.contains("[Whole-task route]"));
        assert!(prompt.contains("tests/ledger.rs") && prompt.contains("negative_cases"));
        assert!(prompt.contains("guide.md") && prompt.contains("checks_after_next_edit"));
        if mode == "diagnostic" {
            assert!(prompt.contains("kind=read") && prompt.contains("checks/assertion.py"));
        }
        if mode == "handoff" {
            assert!(
                prompt.contains("no Coder request") && prompt.contains("future_iteration_only")
            );
        }
        if mode == "report" {
            assert!(
                prompt.contains("report_repair") && prompt.contains("reason_hint: authored_report")
            );
        }
    }
}

#[test]
fn optional_route_reaches_coder_only_with_the_validated_confident_advice() {
    let raw = json!({"suggestions":[{"kind":"read","reason":"Read the public case fixture.","confidence":0.95,"suggested_tool":"read_file","suggested_args":{"path":"tests/ledger.rs"}}],
        "why_this_first":"Establish a valid control, test duplicate IDs, then finish the guide and checks.",
        "fallback":"If counts differ, repair the fixture before touching the reader."}).to_string();
    let env = crate::tui::suggestion::parse_observer_suggestion_envelope(&raw).unwrap();
    let hint = build_observer_suggestion_soft_hint(&env).unwrap();
    assert!(hint.contains("test duplicate IDs") && hint.contains("If counts differ"));
    let mut low = env.clone();
    low.suggestions[0].confidence = 0.5;
    assert!(build_observer_suggestion_soft_hint(&low).is_none());
    let request = |raw| {
        serde_json::from_value(json!({"packet":{"work_budget":{"remaining":10,"coder_remaining":8}},"messages":task(),"response":raw})).unwrap()
    };
    let valid = super::super::evaluate(request(raw.clone()));
    assert!(valid.issue.is_none() && valid.hint.unwrap().contains("If counts differ"));
    let mut invalid: Value = serde_json::from_str(&raw).unwrap();
    invalid["suggestions"][0]["suggested_args"] = json!({"path":false});
    let invalid = super::super::evaluate(request(invalid.to_string()));
    assert!(invalid.issue.is_some() && invalid.hint.is_none());

    // The current system hint is saved verbatim and survives wire compaction;
    // it still has no effect on native requirement or exact-check proof.
    let mut messages = task();
    for i in 0..20 {
        exchange(
            &mut messages,
            &format!("read-{i}"),
            "read_file",
            json!({"path":"notes.md"}),
            &format!(
                "[notes.md] (200 lines, 8000 bytes)\n{}",
                "old data\n".repeat(1000)
            ),
        );
    }
    let before = serde_json::to_value(task_obligations::project(&messages, None)).unwrap();
    messages.push(json!({"role":"system","content":hint}));
    let restored: Vec<Value> =
        serde_json::from_str(&serde_json::to_string(&messages).unwrap()).unwrap();
    assert_eq!(
        serde_json::to_value(task_obligations::project(&restored, None)).unwrap(),
        before
    );
    let (wire, metrics) = crate::tui::agent::context_compaction::coder(&restored, None);
    assert!(metrics.compacted_exchanges > 0);
    assert!(wire.iter().any(|m| m["content"] == hint));
}

#[test]
fn advice_and_large_case_lists_are_explicitly_bounded() {
    let mut env: ObserverSuggestionEnvelope = serde_json::from_value(json!({})).unwrap();
    assert!(coder_context(&env).is_empty());
    env.why_this_first = "猫".repeat(1000);
    let context = coder_context(&env);
    assert!(!context.contains(&"猫".repeat(701)));
    assert!(context.contains("\"truncated\":true"));
    let mut p = packet(&task(), Some(12));
    p["task_obligations"]["task_requirements"]["authored_tests"]["test_only_requirements"] =
        json!((0..15)
            .map(|i| json!({"id":format!("case-{i}"),"description":"猫".repeat(1201)}))
            .collect::<Vec<_>>());
    let data = route(&p);
    let cases = &data["test_review"]["dedicated_cases"];
    assert_eq!(cases["total"], 15);
    assert_eq!(cases["items"].as_array().unwrap().len(), 12);
    assert_eq!(cases["items"][0]["description"]["truncated"], true);
}
