use super::*;

const SOURCE: &str = "src/value.py";

fn add_source_contract(trace: &mut Trace, baseline_command: &str) {
    trace.messages[0]["content"] = json!(format!(
        "{TASK}\n<task_requirements>\n{}\n</task_requirements>",
        json!({
            "version": 1,
            "requirements": [{
                "id": "boundary",
                "description": "Keep the exact-limit control and reject the over-limit input.",
                "artifacts": [{"path": SOURCE, "kind": "source"}],
                "checks": [{"command": baseline_command, "baseline_failure": true}]
            }]
        })
    ));
    std::fs::create_dir_all(trace.path.join("src")).unwrap();
}

fn read_source(trace: &mut Trace, id: &str, args: Value) {
    let (output, failed) = read_range::execute_args(&args, trace.root());
    assert!(!failed, "{output}");
    trace.append(id, "read_file", args, output);
}

#[test]
fn baseline_source_read_yields_once_with_the_test_and_current_source() {
    let mut trace = Trace::new();
    add_source_contract(&mut trace, CHECK);
    let text = "def validate(value):\n    return value\n";
    std::fs::write(trace.path.join(SOURCE), text).unwrap();
    trace.fail("failed", 2, "0.01");
    read_source(&mut trace, "too-early", json!({"path":SOURCE}));
    trace.read("test");
    assert!(trace.view().is_none());
    read_source(&mut trace, "source", json!({"path":SOURCE}));

    let followup = trace.view().unwrap();
    assert!(followup.should_yield());
    let view = serde_json::to_value(&followup).unwrap();
    assert_eq!(view["latest_failure_call_id"], "failed");
    assert_eq!(view["diagnostic_read_call_id"], "test");
    let source = &view["source_repair"];
    assert_eq!(source["path"], SOURCE);
    assert_eq!(source["read_call_id"], "source");
    assert!(source["test_read"]
        .as_str()
        .unwrap()
        .ends_with("assert actual == expected\n"));
    assert!(source["implementation_read"]
        .as_str()
        .unwrap()
        .ends_with(text));

    let packet = json!({"task_obligations":{"diagnostic_followup":view}});
    let observer = crate::observer_action::intervention::prompt(&packet);
    assert!(observer.contains("Quote the invariant to preserve in the primary reason"));
    assert!(observer.contains("propose one exact patch_file"));
    assert!(observer.contains("def validate(value)"));
    let coder =
        agent::task_obligations::prompt(&trace.messages, Some(CHECK), trace.root()).unwrap();
    assert!(coder.contains("Use known evidence for validation"));
    assert!(coder.contains("read_call_id"));
    assert!(
        !coder.contains("def validate(value)"),
        "the Coder already receives the native read"
    );

    read_source(&mut trace, "repeated", json!({"path":SOURCE}));
    assert!(!trace.view().unwrap().should_yield());
    trace.fail("same", 2, "0.12");
    trace.read("test-again");
    read_source(&mut trace, "read-again", json!({"path":SOURCE}));
    assert!(!trace.view().is_some_and(|followup| followup.should_yield()));
}

#[test]
fn only_a_complete_declared_source_read_can_supply_the_pre_edit_return() {
    let mut trace = Trace::new();
    add_source_contract(&mut trace, CHECK);
    std::fs::write(
        trace.path.join(SOURCE),
        "line of implementation\n".repeat(600),
    )
    .unwrap();
    std::fs::write(trace.path.join("src/notes.py"), "# unrelated\n").unwrap();
    trace.fail("failed", 2, "0.01");
    trace.read("test");
    read_source(&mut trace, "unrelated", json!({"path":"src/notes.py"}));
    read_source(&mut trace, "prefix", json!({"path":SOURCE}));
    assert!(trace.view().is_none());

    read_source(
        &mut trace,
        "range",
        json!({"path":SOURCE,"start_line":450,"end_line":460}),
    );
    let view = trace.view().unwrap();
    assert!(view.should_yield());
    assert!(
        serde_json::to_value(view).unwrap()["source_repair"]["implementation_read"]
            .as_str()
            .unwrap()
            .contains("range: lines 450-460")
    );

    trace.append(
        "potential-write",
        "exec",
        json!({"command":"python3 side_effect.py"}),
        format!("FAILED (exit_code: 1)\ncwd: {}", trace.path.display()),
    );
    assert!(
        trace.view().is_none(),
        "an executed process can stale both reads"
    );
}

#[test]
fn other_baseline_ownership_and_a_prior_edit_cannot_start_a_source_return() {
    for prior_edit in [false, true] {
        let mut trace = Trace::new();
        let declared_check = if prior_edit {
            CHECK
        } else {
            "python3 other.py"
        };
        add_source_contract(&mut trace, declared_check);
        std::fs::write(trace.path.join(SOURCE), "value = None\n").unwrap();
        if prior_edit {
            trace.append(
                "edit",
                "patch_file",
                json!({"path":SOURCE}),
                format!("OK: patched '{SOURCE}'"),
            );
        }
        trace.fail("failed", 2, "0.01");
        trace.read("test");
        read_source(&mut trace, "source", json!({"path":SOURCE}));
        assert!(trace.view().is_none());
    }
}

// The transcript marker is written by native file dispatch only after the
// exact accepted Observer action succeeds.
fn accepted_source_read(trace: &mut Trace, id: &str, start: usize, end: usize) {
    read_source(
        trace,
        id,
        json!({"path":SOURCE,"start_line":start,"end_line":end}),
    );
    trace.messages.last_mut().unwrap()[METADATA_KEY]["accepted_observer_read"] = json!(true);
}

#[test]
fn accepted_additional_read_returns_only_for_previously_unseen_lines() {
    let mut trace = Trace::new();
    add_source_contract(&mut trace, CHECK);
    let text = (1..=50)
        .map(|n| format!("value_{n} = {n}\n"))
        .collect::<String>();
    std::fs::write(trace.path.join(SOURCE), text).unwrap();
    trace.fail("failed", 2, "0.01");
    trace.read("test");
    read_source(
        &mut trace,
        "first",
        json!({"path":SOURCE,"start_line":1,"end_line":10}),
    );
    assert!(trace.view().unwrap().should_yield());
    accepted_source_read(&mut trace, "contained", 4, 10);
    assert!(!trace.view().unwrap().should_yield());

    // Reading new lines on Coder's own initiative is useful, but it is not
    // execution of the Observer's additional read.
    read_source(
        &mut trace,
        "independent",
        json!({"path":SOURCE,"start_line":11,"end_line":20}),
    );
    assert!(!trace.view().unwrap().should_yield());
    accepted_source_read(&mut trace, "already-covered", 8, 18);
    assert!(!trace.view().unwrap().should_yield());
    accepted_source_read(&mut trace, "additional", 19, 25);

    let updated = trace.view().unwrap();
    assert!(updated.should_yield());
    assert!(updated.is_additional_source_return());
    let view = serde_json::to_value(updated).unwrap();
    let repair = &view["source_repair"];
    assert_eq!(repair["previous_read_call_id"], "first");
    assert_eq!(repair["read_call_id"], "additional");
    assert_eq!(view["diagnostic_read_call_id"], "test");
    assert!(repair["implementation_read"]
        .as_str()
        .unwrap()
        .contains("value_25 = 25"));
    assert!(crate::observer_action::intervention::prompt(
        &json!({"task_obligations":{"diagnostic_followup":view}})
    )
    .contains("The additional read you requested has returned with new lines"));

    accepted_source_read(&mut trace, "third-window", 26, 35);
    let retained = trace.view().unwrap();
    assert!(!retained.should_yield());
    assert_eq!(
        serde_json::to_value(retained).unwrap()["source_repair"]["read_call_id"],
        "additional"
    );
}

#[tokio::test]
async fn additional_source_return_preserves_the_delivery_allowance() {
    let mut trace = Trace::new();
    add_source_contract(&mut trace, CHECK);
    std::fs::write(trace.path.join(SOURCE), "value = 1\nlimit = 2\n").unwrap();
    trace.fail("failed", 2, "0.01");
    trace.read("test");
    read_source(
        &mut trace,
        "first",
        json!({"path":SOURCE,"start_line":1,"end_line":1}),
    );
    accepted_source_read(&mut trace, "additional", 2, 2);

    for (remaining, expected) in [(3, false), (4, true)] {
        let budget = crate::work_budget::WorkBudget::new(trace.root().unwrap(), remaining, None)
            .with_observer_return(1);
        let ready = crate::work_budget::scope(
            Some(crate::work_budget::Context {
                budget,
                role: "coder",
            }),
            async {
                agent::observer_handoff::after_diagnostic_read(
                    &trace.focus,
                    &trace.messages,
                    trace.root(),
                    Some(CHECK),
                    3,
                    4,
                )
                .is_some()
            },
        )
        .await;
        assert_eq!(ready, expected, "remaining task requests: {remaining}");
    }
}

#[test]
fn grounded_rust_comparison_uses_the_observed_binding() {
    const RUST: &str = "src/value.rs";
    let search = "    if output.map_or(false, |n| n > limit)\n    {\n        return false;\n    }";
    let source = format!("fn within_limit(output: Option<u32>, limit: u32) -> bool {{\n    let wire_total = 3;\n{search}\n    true\n}}\n");
    let mut trace = Trace::new();
    add_source_contract(&mut trace, CHECK);
    trace.messages[0]["content"] = json!(trace.messages[0]["content"]
        .as_str()
        .unwrap()
        .replace(SOURCE, RUST));
    std::fs::write(trace.path.join(RUST), source).unwrap();
    trace.fail("failed", 2, "0.01");
    trace.read("test");
    read_source(
        &mut trace,
        "known",
        json!({"path":RUST,"start_line":1,"end_line":2}),
    );
    read_source(
        &mut trace,
        "guard",
        json!({"path":RUST,"start_line":3,"end_line":9}),
    );
    let resolution = |name: &str| {
        let replacement = search.replace("    {", &format!("        || {name} > limit\n    {{"));
        let raw = json!({
            "summary":"Keep unknown output and validate the known total",
            "suggestions":[{"kind":"edit","reason":"Add a strict guard","confidence":0.96,
                "suggested_tool":"patch_file","suggested_args":{"path":RUST,"search":search,"replace":replacement}}]
        }).to_string();
        crate::observer_action::resolve_advice(
            &raw,
            &trace.messages,
            Some(CHECK),
            trace.root(),
            Some(12),
        )
    };
    let wrong = resolution("finalized_wire_total");
    assert!(wrong
        .issue
        .as_deref()
        .unwrap()
        .contains("`finalized_wire_total`"));
    assert!(
        resolution("total").issue.is_some(),
        "substring hits cannot prove an identifier"
    );
    let correct = resolution("wire_total");
    assert!(correct.issue.is_none());
    assert_eq!(
        crate::tui::suggestion::primary_suggestion(correct.envelope.as_ref().unwrap())
            .unwrap()
            .suggested_tool
            .as_deref(),
        Some("patch_file")
    );
}

#[test]
fn only_an_unseen_declared_source_range_qualifies_for_selection_repair() {
    let mut trace = Trace::new();
    add_source_contract(&mut trace, CHECK);
    std::fs::write(
        trace.path.join(SOURCE),
        "value = 1\nlimit = 2\nreturn True\n",
    )
    .unwrap();
    trace.fail("failed", 2, "0.01");
    trace.read("test");
    read_source(
        &mut trace,
        "first",
        json!({"path":SOURCE,"start_line":1,"end_line":1}),
    );
    let followup = trace.view().unwrap();
    let root = trace.root().unwrap();
    for (path, start, end, expected) in [
        (SOURCE, 2, 3, true),
        (SOURCE, 1, 1, false),
        (SOURCE, 8, 9, false),
        (SOURCE, 1, 400, false),
        ("src/other.py", 2, 3, false),
    ] {
        let read = read_range::ReadRequest {
            path: path.into(),
            start_line: Some(start),
            end_line: Some(end),
        };
        assert_eq!(followup.accepts_source_range(&read, root), expected);
    }
    accepted_source_read(&mut trace, "second", 2, 2);
    let after = trace.view().unwrap();
    assert!(!after.accepts_source_range(
        &read_range::ReadRequest {
            path: SOURCE.into(),
            start_line: Some(3),
            end_line: Some(3)
        },
        trace.root().unwrap()
    ));
}

#[test]
fn previously_returned_prefix_supplies_the_observers_missing_range() {
    let mut trace = Trace::new();
    add_source_contract(&mut trace, CHECK);
    let lines: Vec<String> = (1..=400)
        .map(|n| format!("value_{n:03} = 'source line {n:03}'\r\n"))
        .collect();
    std::fs::write(trace.path.join(SOURCE), lines.concat()).unwrap();
    trace.fail("failed", 2, "0.01");
    trace.read("test");
    read_source(&mut trace, "prefix", json!({"path":SOURCE}));
    assert!(trace.view().is_none());
    read_source(
        &mut trace,
        "range",
        json!({"path":SOURCE,"start_line":190,"end_line":320}),
    );
    let view = serde_json::to_value(trace.view().unwrap()).unwrap();
    let excerpt = &view["source_repair"]["preceding_lines"];
    let first = excerpt["start_line"]
        .as_u64()
        .expect("complete preceding lines") as usize;
    assert!((70..=100).contains(&first));
    assert_eq!(excerpt["read_call_id"], "prefix");
    assert_eq!(excerpt["end_line"], 189);
    assert_eq!(excerpt["text"], lines[first - 1..189].concat());
    let request = |start, end| {
        json!({"summary":"Check the missing branch","suggestions":[
        {"kind":"read","reason":"Inspect the preceding branch","confidence":0.95,
         "suggested_tool":"read_file","suggested_args":{"path":SOURCE,"start_line":start,"end_line":end}}]}).to_string()
    };
    let issue = |start, end| {
        crate::observer_action::resolve_advice(
            &request(start, end),
            &trace.messages,
            Some(CHECK),
            trace.root(),
            Some(12),
        )
        .issue
    };
    assert!(issue(100, 210).unwrap().contains("already quoted"));
    assert!(issue(300, 350).is_none(), "an unseen line stays available");
    assert!(
        issue(1, 30).is_none(),
        "lines outside the delivered excerpt stay available"
    );
    let coder =
        agent::task_obligations::prompt(&trace.messages, Some(CHECK), trace.root()).unwrap();
    assert!(
        !coder.contains(&lines[105]),
        "no duplicate source bytes in Coder obligations"
    );
}

#[test]
fn edited_overlap_cannot_join_the_clipped_prefix_to_a_later_read() {
    let mut trace = Trace::new();
    add_source_contract(&mut trace, CHECK);
    let source = (1..=400)
        .map(|n| format!("value_{n:03} = 'source line {n:03}'\n"))
        .collect::<String>();
    std::fs::write(trace.path.join(SOURCE), &source).unwrap();
    trace.fail("failed", 2, "0.01");
    trace.read("test");
    read_source(&mut trace, "prefix", json!({"path":SOURCE}));
    std::fs::write(
        trace.path.join(SOURCE),
        source.replace("source line 200", "change line 200"),
    )
    .unwrap();
    read_source(
        &mut trace,
        "range",
        json!({"path":SOURCE,"start_line":190,"end_line":320}),
    );
    assert!(
        serde_json::to_value(trace.view().unwrap()).unwrap()["source_repair"]["preceding_lines"]
            .is_null()
    );
}
