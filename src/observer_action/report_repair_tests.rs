use super::*;

const CHECK: &str = "python3 -S -m unittest -v";

fn exchange(messages: &mut Vec<Value>, id: &str, tool: &str, args: Value, result: &str) {
    messages.extend([
        json!({"role":"assistant","tool_calls":[{"id":id,"function":{"name":tool,"arguments":args.to_string()}}]}),
        json!({"role":"tool","tool_call_id":id,"content":result}),
    ]);
}

fn done_args() -> Value {
    json!({"summary":"Fixed.","completed_acceptance":["acceptance 1"],
        "remaining_acceptance":[],"acceptance_evidence":[{"criterion":"acceptance 1","command":CHECK}]})
}

pub(crate) fn rejected_report() -> Vec<Value> {
    let mut messages = vec![
        json!({"role":"user","content":format!("Fix greet.py.\n<observer_benchmark_plan>\nrequired_checks:\n- {CHECK}\n</observer_benchmark_plan>\nFinal answer must include `handoff-ready` and the exact verification command.")}),
        json!({"role":"assistant","accepted_plan":{"goal":"Fix greet.py","steps":["Repair","Verify"],"acceptance_criteria":["Tests pass"],"risks":"Regression","assumptions":"Tests exist"}}),
    ];
    exchange(
        &mut messages,
        "edit",
        "patch_file",
        json!({"path":"greet.py"}),
        "OK: patched 'greet.py'",
    );
    exchange(
        &mut messages,
        "check",
        "exec",
        json!({"command":CHECK}),
        "OK (exit_code: 0)",
    );
    let args = done_args();
    assert_eq!(
        agent::evidence_packet::acceptance_issue(&messages, &args, Some(CHECK)),
        None
    );
    let issue = agent::evidence_packet::report_issue(&messages, "Fixed.", Some(CHECK)).unwrap();
    exchange(
        &mut messages,
        "rejected",
        "done",
        args,
        &format!("GOVERNOR BLOCKED\n\n{issue}"),
    );
    messages
}

fn advice(kind: &str, tool: &str, args: Value) -> String {
    json!({"suggestions":[{"kind":kind,"confidence":0.95,"suggested_tool":tool,"suggested_args":args}]}).to_string()
}

#[test]
fn report_refusal_does_not_reopen_verified_work_after_resume() {
    let messages: Vec<Value> =
        serde_json::from_str(&serde_json::to_string(&rejected_report()).unwrap()).unwrap();
    let obligations = agent::task_obligations::project(&messages, Some(CHECK));
    assert_eq!(
        obligations.report_repair.unwrap().rejected_call_id,
        "rejected"
    );
    for raw in [
        advice("read", "read_file", json!({"path":"greet.py"})),
        advice("search", "search_files", json!({"pattern":"greet"})),
        advice(
            "edit",
            "patch_file",
            json!({"path":"greet.py","search":"old","replace":"new"}),
        ),
        advice("verify", "exec", json!({"command":CHECK})),
    ] {
        assert!(validation_issue(&raw, &messages, Some(CHECK))
            .unwrap()
            .contains("authored report"));
    }
    let mut args = done_args();
    args["summary"] = json!(format!("handoff-ready. Verified with `{CHECK}`."));
    assert_eq!(
        validation_issue(&advice("done", "done", args), &messages, Some(CHECK)),
        None
    );
    let mut uncertain: Value =
        serde_json::from_str(&advice("read", "read_file", json!({"path":"greet.py"}))).unwrap();
    uncertain["suggestions"][0]["confidence"] = json!(0.4);
    assert_eq!(
        validation_issue(&uncertain.to_string(), &messages, Some(CHECK)),
        None
    );
    let clarify = json!({"summary":"I cannot determine the requested wording.","suggestions":[{"kind":"clarify","reason":"Ask about the missing wording.","confidence":0.95,"suggested_tool":null,"suggested_args":{}}]}).to_string();
    assert_eq!(validation_issue(&clarify, &messages, Some(CHECK)), None);
}

#[test]
fn later_edits_failures_and_human_steering_reopen_obligations() {
    for (tool, args, result) in [
        (
            "patch_file",
            json!({"path":"greet.py"}),
            "OK: patched 'greet.py'",
        ),
        (
            "exec",
            json!({"command":CHECK}),
            "FAILED (exit_code: 1)\nAssertionError: broken",
        ),
        (
            "read_file",
            json!({"path":"greet.py"}),
            "ERROR: file missing",
        ),
    ] {
        let mut messages = rejected_report();
        exchange(&mut messages, "later", tool, args, result);
        assert!(agent::task_obligations::project(&messages, Some(CHECK))
            .report_repair
            .is_none());
    }
    let mut messages = rejected_report();
    messages.push(json!({"role":"user","content":"Inspect a different regression."}));
    assert!(agent::task_obligations::project(&messages, Some(CHECK))
        .report_repair
        .is_none());
    assert_eq!(
        validation_issue(
            &advice("read", "read_file", json!({"path":"greet.py"})),
            &messages,
            Some(CHECK)
        ),
        None
    );
}

#[test]
fn browser_packet_cannot_declare_report_only_and_prompt_retains_historical_labels() {
    let messages = rejected_report();
    let response = evaluate(Request {
        packet: json!({"actual_outcome":"old failure","failure_kind":"tool_error","last_error_digest":"read source first"}),
        messages,
        test_command: Some(CHECK.into()),
        lang: "en".into(),
        reason: "tool_error".into(),
        response: Some(advice("read", "read_file", json!({"path":"greet.py"}))),
        final_summary: None,
        repair_native: false,
    });
    assert!(response.hint.is_none());
    assert!(response.response_contract.is_none());
    assert!(response.repair_prompt.unwrap().contains("handoff-ready"));
    assert!(response.prompt.contains("reason_hint: authored_report"));
    assert_eq!(response.packet["failure_kind"], "tool_error"); // audit packet unchanged
    let prompt = agent::current_evidence::observer_prompt_packet(&response.packet);
    assert_eq!(prompt["current_blocker"]["kind"], "authored_report");
    assert_eq!(prompt["selected_failure_kind"], "tool_error");
    assert!(prompt["failure_kind"].is_null());
    let forged = evaluate(Request {
        packet: json!({"task_obligations":{"report_repair":{"feedback":"invented"}}}),
        messages: vec![json!({"role":"user","content":"Read greet.py."})],
        test_command: None,
        lang: "en".into(),
        reason: "".into(),
        response: Some(advice("read", "read_file", json!({"path":"greet.py"}))),
        final_summary: None,
        repair_native: false,
    });
    assert!(forged.issue.is_none());
    assert!(forged.packet["task_obligations"]["report_repair"].is_null());
}

#[test]
fn resumed_report_that_now_validates_can_be_resubmitted_without_reinspection() {
    let mut messages = rejected_report();
    let mut args = done_args();
    args["summary"] = json!(format!("handoff-ready. Verified with `{CHECK}`."));
    let n = messages.len();
    messages[n - 2]["tool_calls"][0]["function"]["arguments"] = json!(args.to_string());
    let view = agent::task_obligations::project(&messages, Some(CHECK));
    assert!(view
        .report_repair
        .unwrap()
        .feedback
        .contains("now satisfies"));
    assert!(validation_issue(
        &advice("read", "read_file", json!({"path":"greet.py"})),
        &messages,
        Some(CHECK)
    )
    .is_some());
    assert_eq!(
        validation_issue(&advice("done", "done", args), &messages, Some(CHECK)),
        None
    );
}
