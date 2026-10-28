use super::*;

#[test]
fn authored_baseline_failure_count_must_match_the_native_receipt_or_be_omitted() {
    let root = "Fix settings.py. Finish with an authored report naming the changed file and the actual before/after test outcomes; disclose any remaining work.";
    let check = "python3 -B -S -m unittest discover -s tests -v";
    let mut messages = vec![json!({"role":"user","content":root})];
    for (id, name, args, result) in [
        (
            "baseline",
            "exec",
            json!({"command":check}),
            "FAILED (exit_code: 1)",
        ),
        (
            "edit",
            "patch_file",
            json!({"path":"settings.py"}),
            "OK: patched 'settings.py'",
        ),
        (
            "final",
            "exec",
            json!({"command":check}),
            "OK (exit_code: 0)",
        ),
    ] {
        messages.extend([
            json!({"role":"assistant","tool_calls":[{"id":id,"function":{"name":name,"arguments":args.to_string()}}]}),
            json!({"role":"tool","tool_call_id":id,"content":result}),
        ]);
    }
    let rows = report_outcomes::required_rows(root, &messages, Some(check)).join("\n");
    let summary = format!("Changed settings.py. The before-check failed on the unchanged baseline with 9 failures.\n{rows}\nNo remaining work.");
    // A retained status with no native count is unknown, not evidence for nine.
    assert!(
        validate_authored_done_summary(root, &summary, &summary, &messages, Some(check)).is_err()
    );
    messages[2]["test_counts"] =
        json!({"runner":"unittest","tests_run":10,"failure_events":14,"errors":0,"skipped":0});
    assert!(
        validate_authored_done_summary(root, &summary, &summary, &messages, Some(check)).is_err()
    );
    let correct = summary.replace("9 failures", "14 failures");
    validate_authored_done_summary(root, &correct, &correct, &messages, Some(check)).unwrap();
    let no_count = format!("Changed settings.py.\n{rows}\nNo remaining work.");
    validate_authored_done_summary(root, &no_count, &no_count, &messages, Some(check)).unwrap();
}

#[test]
fn counts_keep_runner_units_and_unknown_or_multiple_summaries_stay_unknown() {
    let message = json!({"role":"tool","tool_call_id":"check","content":"FAILED (exit_code: 1)"});
    let failed = report_counts::capture(
        message.clone(),
        "",
        "Ran 10 tests in 0.003s\n\nFAILED (failures=14, errors=2, skipped=1)\n",
    );
    assert_eq!(
        failed["test_counts"],
        json!({"runner":"unittest","tests_run":10,"failure_events":14,"errors":2,"skipped":1})
    );
    assert!(failed["test_counts"].get("passed").is_none());
    let rust = report_counts::capture(message.clone(), "test result: ok. 7 passed; 0 failed; 2 ignored; 0 measured; 3 filtered out; finished in 0.02s\n", "");
    assert_eq!(
        rust["test_counts"],
        json!({"runner":"libtest","passed":7,"failure_events":0,"skipped":2})
    );
    for text in [
        "FAILED (failures=14)\n",
        "Ran 10 tests in 0.003s\ncleanup\nFAILED (failures=14)",
        "Ran 10 tests in 0.003s\nOK (expected failures=2)",
        "Ran 10 tests in 0.003s\nOK (failures=3)",
        "Ran 10 tests in 0.003s\nOK\nRan 2 tests in 0.001s\nOK",
        "Ran 10 tests in 0.003s\nOK\nRan 2 tests in 0.001s\ninterrupted",
    ] {
        assert!(
            report_counts::capture(message.clone(), "", text)
                .get("test_counts")
                .is_none(),
            "{text}"
        );
    }
    let unbounded = format!("{}\nRan 10 tests in 0.003s\nOK", "x".repeat(70_000));
    assert!(report_counts::capture(message, "", &unbounded)
        .get("test_counts")
        .is_none());
}

#[test]
fn count_assertions_distinguish_phases_subtest_events_and_non_count_literals() {
    let facts = vec![
        json!({"phase":"Before","counts":{"runner":"unittest","tests_run":10,"failure_events":14,"errors":0,"skipped":0}}),
        json!({"phase":"After","counts":{"runner":"unittest","tests_run":10,"failure_events":0,"errors":0,"skipped":0}}),
    ];
    for text in [
        "The baseline had 14 failures. After editing there were no failures.",
        "Before, 10 tests ran with fourteen failures, including subtests.",
        "Before status is failed; the count is unknown.",
        "Before fixing it, input `line 3` raised ValueError. Empty input now returns `0` for each level.",
        "Before there were not 9 failures.",
    ] { assert!(report_counts::issues(text, &facts).is_empty(), "{text}"); }
    for text in [
        "The baseline had 9 failures.",
        "After editing, 14 failures remained.",
        "Before editing, 14 failed tests were recorded.",
        "Before 14 tests failed.",
        "Before there were no failures.",
        "Before and after there were 9 failures.",
        "The baseline had nine failures.",
    ] {
        assert!(!report_counts::issues(text, &facts).is_empty(), "{text}");
    }
    let unknown = vec![json!({"phase":"After","counts":null})];
    assert!(!report_counts::issues("After editing, 0 failures remained.", &unknown).is_empty());
    let multiple = vec![
        json!({"phase":"Before","command":"python a.py","counts":{"failure_events":14}}),
        json!({"phase":"Before","command":"python b.py","counts":{"failure_events":3}}),
    ];
    assert!(report_counts::issues(
        "Before: `python a.py` had 14 failures; before: `python b.py` had 3 failures.",
        &multiple
    )
    .is_empty());
    assert!(!report_counts::issues("Before: `python b.py` had 14 failures.", &multiple).is_empty());
    assert!(!report_counts::issues("Before there were 14 failures.", &multiple).is_empty());
}

#[cfg(not(target_os = "windows"))]
#[tokio::test]
async fn native_exec_preserves_stderr_counts_before_preview_and_projects_them_to_both_roles() {
    use httpmock::prelude::*;
    for failed in [false, true] {
        let server = MockServer::start();
        let command = "python3 -B -S acceptance.py";
        let plan = "<plan>\ngoal: Repair source.txt and verify behavior\nsteps:\n1) Record baseline\n2) Repair source and verify behavior\nacceptance: Source behavior passes the exact check\nrisks: regression\nassumptions: check exposes missing behavior\n</plan>\n<think>\ngoal: Record baseline\nstep: 1\ntool: exec\nrisk: expected baseline failure\ndoubt: none\nnext: python3 -B -S acceptance.py\nverify: python3 -B -S acceptance.py\n</think>";
        let wire = server.mock(|when, then| {
            when.method(POST).path("/chat/completions");
            let call = json!([{"index":0,"id":"baseline","function":{"name":"exec","arguments":json!({"command":command}).to_string()}}]);
            then.status(200).header("content-type","text/event-stream").body(format!("data: {}\n\ndata: [DONE]\n\n", json!({"choices":[{"delta":{"content":plan,"tool_calls":call},"finish_reason":"tool_calls"}]})));
        });
        let dir = tempfile::tempdir().unwrap();
        let status = if failed { "FAILED (failures=14)" } else { "OK" };
        std::fs::write(dir.path().join("acceptance.py"), format!("import sys\nsys.stderr.write('diagnostic progress\\n' * 300 + 'Ran 10 tests in 0.003s\\n\\n{status}\\n')\nsys.exit({})\n", if failed {1} else {0})).unwrap();
        std::fs::write(dir.path().join("source.txt"), "old\n").unwrap();
        let goal = format!("Repair source.txt. Finish with an authored report naming the changed file and the actual before/after test outcomes.\n<task_requirements>\n{}\n</task_requirements>", json!({"version":1,"requirements":[{"id":"repair","description":"Repair behavior","artifacts":[{"path":"source.txt","kind":"source"}],"checks":[{"command":command,"baseline_failure":true}]}]}));
        let cfg = RunConfig {
            provider: ProviderKind::OpenAiCompatible,
            model: "test".into(),
            chat_model: "test".into(),
            code_model: "test".into(),
            api_key: None,
            base_url: server.base_url(),
            mode: crate::modes::Mode::Vibe,
            persona: String::new(),
            temperature: 0.0,
            max_tokens: 512,
            timeout_seconds: 5,
            hf_device: "cpu".into(),
            hf_local_only: true,
        };
        let start = AgenticStartState {
            messages: vec![crate::task_origin::user_message(
                &goal,
                crate::task_origin::MessageOrigin::User,
            )],
            checkpoint: None,
            cur_cwd: None,
            observation_cache: None,
            session_bridge: None,
            create_checkpoint: false,
        };
        let (tx, _rx) = mpsc::channel(512);
        let result = run_agentic_json(
            start,
            &cfg,
            dir.path().to_str(),
            1,
            tx,
            None,
            None,
            None,
            true,
            Some(RealizePreset::Off),
            None,
            &crate::approvals::AutoApprover,
        )
        .await
        .unwrap();
        wire.assert_hits(1);
        let native = result
            .messages
            .iter()
            .find(|m| m["role"] == "tool" && m["tool_call_id"] == "baseline")
            .unwrap();
        assert_eq!(native["test_counts"]["tests_run"], 10, "{native}");
        assert_eq!(
            native["test_counts"]["failure_events"],
            if failed { 14 } else { 0 }
        );
        assert!(!native["content"].as_str().unwrap().contains("Ran 10 tests"));
        let mut resumed = result.messages;
        for _ in 0..KEEP_RECENT_TOOL_TURNS + 1 {
            resumed.push(json!({"role":"tool","content":"OK (exit_code: 0)"}));
        }
        prune_old_tool_results(&mut resumed);
        let resumed: Vec<serde_json::Value> =
            serde_json::from_str(&serde_json::to_string(&resumed).unwrap()).unwrap();
        let shared = task_obligations::project(&resumed, None);
        assert_eq!(
            shared.final_answer["check_facts"][0]["counts"]["tests_run"],
            10
        );
        assert_eq!(
            shared.final_answer["check_facts"][0]["status"],
            if failed { "FAILED" } else { "PASSED" }
        );
        assert!(requirement_proof::issue(&resumed, None).is_some());
        assert!(crate::task_origin::provider_messages(&resumed)
            .iter()
            .all(|m| m.get("test_counts").is_none()));
        let mut tools = json!([done_tool_def(&server.base_url())]);
        acceptance_schema::bind_summary(&mut tools, &shared.final_answer);
        let guidance = tools[0]["function"]["parameters"]["properties"]["summary"]["description"]
            .as_str()
            .unwrap();
        assert!(guidance.contains("input condition") && guidance.contains("failure_events"));
    }
}
