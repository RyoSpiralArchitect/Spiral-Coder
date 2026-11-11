//! An explicit incomplete terminal report is not successful `done` evidence.
//! It preserves native obligations and validates every claimed completed item.
use super::*;
use serde_json::Value;

/// A native accepted incomplete closeout is historical reporting, not a work
/// strategy for resume. Keep its transcript/obligations; exclude only the
/// correlated assistant turn from executable plan/reflection projections.
pub(super) fn is_accepted_closeout(messages: &[Value], index: usize) -> bool {
    let Some(message) = messages.get(index) else {
        return false;
    };
    let Some(result) = messages.get(index + 1) else {
        return false;
    };
    let Some(calls) = message["tool_calls"].as_array() else {
        return false;
    };
    let [call] = calls.as_slice() else {
        return false;
    };
    message["role"] == "assistant"
        && call["function"]["name"] == "done"
        && call["id"].as_str().is_some_and(|id| !id.is_empty())
        && result["role"] == "tool"
        && result["tool_call_id"] == call["id"]
        && result["content"] == "OK: blocked"
        && result["blocked_report"]["remaining_acceptance"]
            .as_array()
            .is_some_and(|remaining| !remaining.is_empty())
}

#[derive(Debug, serde::Serialize)]
pub(super) struct BlockedReport {
    pub summary: String,
    pub remaining_acceptance: Vec<String>,
    pub next_steps: String,
    pub pending_exact_checks: Vec<String>,
    pub pending_recovery: Vec<String>,
}

pub(super) fn prepare(
    calls: &[ToolCallData],
    plan: Option<&PlanBlock>,
    messages: &[Value],
    test_cmd: Option<&str>,
    tool_root: Option<&str>,
    observation: &ObservationEvidence,
) -> Option<Result<BlockedReport>> {
    let [call] = calls else { return None };
    if call.name != "done" {
        return None;
    }
    let args: Value = serde_json::from_str(&call.arguments).ok()?;
    let remaining = args["remaining_acceptance"].as_array()?;
    if remaining.is_empty() {
        return None;
    }
    Some((|| {
        anyhow::ensure!(
            remaining
                .iter()
                .all(|v| v.as_str().is_some_and(|s| !s.trim().is_empty())),
            "remaining_acceptance must contain nonempty criterion references"
        );
        let summary = args["summary"].as_str().unwrap_or("").trim();
        let next_steps = args["next_steps"].as_str().unwrap_or("").trim();
        anyhow::ensure!(
            !summary.is_empty(),
            "An incomplete report needs a blocker in summary"
        );
        let completed = parse_string_list_arg(&args["completed_acceptance"]);
        let remaining_acceptance = parse_string_list_arg(&args["remaining_acceptance"]);
        let acceptance =
            acceptance_report::CurrentEvidence::from_native(messages, test_cmd, tool_root);
        let evidence =
            acceptance_report::resolve_evidence(&args["acceptance_evidence"], &acceptance)?;
        validate_done_acceptance(
            plan,
            &completed,
            &remaining_acceptance,
            &evidence,
            &acceptance.commands,
            observation,
        )?;
        let obligations = task_obligations::project(messages, test_cmd);
        // A blocked report needs a live blocker. When an explicit requirements
        // contract exists and every declared requirement and pending check is
        // already discharged, the honest closeout is `done` — not an
        // incomplete report. Undeclared prose tasks are out of scope: without a
        // declared contract we cannot prove nothing remains, so a truthful
        // incomplete report stays admissible.
        let requirements = requirement_proof::project(messages, test_cmd);
        anyhow::ensure!(
            !(requirements.declared
                && requirements.pending.is_empty()
                && obligations.pending_exact_checks.is_empty()
                && obligations.pending_recovery.is_empty()),
            "Blocked report claims remaining work, but every declared requirement and pending check is already discharged. Call `done` with the accurate completed/remaining split, or execute the one missing step first."
        );
        Ok(BlockedReport {
            summary: summary.into(),
            remaining_acceptance,
            next_steps: next_steps.into(),
            pending_exact_checks: obligations.pending_exact_checks,
            pending_recovery: obligations.pending_recovery,
        })
    })())
}

impl BlockedReport {
    pub(super) fn render(&self) -> String {
        format!("[BLOCKED — incomplete]\n{}\n\nRemaining acceptance: {}\nPending exact checks: {}\nNative recovery: {}\n\nNext: {}\nNo completion or qualification is claimed.",
            self.summary, self.remaining_acceptance.join("; "),
            self.pending_exact_checks.join("; "), self.pending_recovery.join("; "), if self.next_steps.is_empty() { "Not supplied; Observer diagnosis must identify the missing fact or next action." } else { &self.next_steps })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn restored_blocked_report_can_execute_diagnosis_without_closeout_strategy_hint() {
        use clap::Parser;
        use httpmock::prelude::*;
        let server = MockServer::start();
        let mock = server.mock(|when,then| {
            when.method(POST).path("/chat/completions").matches(|request| {
                let body: Value=serde_json::from_slice(request.body.as_deref().unwrap_or_default()).unwrap();
                !body["messages"].as_array().unwrap().iter().any(|m| m["role"]=="system" && m["content"].as_str().is_some_and(|s| s.contains("Execute only the new minimal action: report incomplete work")))
            });
            let text="<reflect>\nlast_outcome: failure\ngoal_delta: same\nwrong_assumption: test would pass\nstrategy_change: adjust\nnext_minimal_action: read fixture.txt\n</reflect>\n<think>\ngoal: repair fixture.txt\nstep: 1\ntool: read_file\nrisk: missing cause\ndoubt: cause unknown\nnext: read fixture.txt\nverify: cargo test -q\n</think>";
            then.status(200).header("content-type","text/event-stream").body(format!("data: {}\n\ndata: [DONE]\n\n",json!({"choices":[{"delta":{"content":text,"tool_calls":[{"index":0,"id":"diagnosis","function":{"name":"read_file","arguments":"{\"path\":\"fixture.txt\"}"}}]},"finish_reason":"tool_calls"}]})));
        });
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("fixture.txt"), "broken parser\n").unwrap();
        let cli = crate::Cli::try_parse_from([
            "spiral-coder",
            "--provider",
            "openai-compatible",
            "--base-url",
            &server.base_url(),
            "--api-key",
            "offline",
            "--timeout-seconds",
            "3",
        ])
        .unwrap();
        let cfg = cli.common.to_partial_config().resolve().unwrap();
        let messages = vec![
            json!({"role":"user","content":"Repair fixture.txt. Run exactly: cargo test -q"}),
            json!({"role":"assistant","accepted_plan":{"goal":"Repair fixture.txt","steps":["Read fixture.txt","Repair and test"],"acceptance_criteria":["cargo test -q passes"],"risks":"regression","assumptions":"fixture exists"},"tool_calls":[{"id":"failed","function":{"name":"exec","arguments":"{\"command\":\"cargo test -q\"}"}}]}),
            json!({"role":"tool","tool_call_id":"failed","content":"FAILED (exit_code: 1)"}),
            json!({"role":"assistant","content":"<reflect>\nlast_outcome: partial\ngoal_delta: same\nwrong_assumption: allowance ended\nstrategy_change: abandon\nnext_minimal_action: report incomplete work\n</reflect>","tool_calls":[{"id":"terminal","function":{"name":"done","arguments":"{}"}}]}),
            json!({"role":"tool","tool_call_id":"terminal","content":"OK: blocked","blocked_report":{"remaining_acceptance":["acceptance 1"],"next_steps":"read fixture.txt"}}),
            crate::task_origin::user_message(
                "Continue with a fresh allowance",
                crate::task_origin::MessageOrigin::Runtime,
            ),
        ];
        let start = AgenticStartState {
            messages,
            checkpoint: None,
            cur_cwd: None,
            observation_cache: None,
            session_bridge: None,
            create_checkpoint: false,
        };
        let (tx, _rx) = mpsc::channel(512);
        let result = Box::pin(run_agentic_json(
            start,
            &cfg,
            root.path().to_str(),
            1,
            tx,
            None,
            None,
            None,
            true,
            Some(RealizePreset::Off),
            None,
            &crate::approvals::AutoApprover,
        ))
        .await
        .unwrap();
        mock.assert_hits(1);
        assert!(result.messages.iter().any(|m| m["role"] == "tool"
            && m["tool_call_id"] == "diagnosis"
            && m["content"]
                .as_str()
                .is_some_and(|s| s.contains("broken parser"))));
        assert!(!outcome::TaskOutcome::default().completed(&result.messages));
        assert!(task_obligations::project(&result.messages, None)
            .pending_exact_checks
            .contains(&"cargo test -q".into()));
    }

    #[test]
    fn accepted_blocked_closeout_does_not_replace_resumable_work_strategy() {
        let work = PlanBlock {
            goal: "Repair parser".into(),
            steps: vec!["Diagnose parser".into(), "Repair and verify".into()],
            acceptance_criteria: vec!["Tests pass".into()],
            risks: "regression".into(),
            assumptions: "fixture available".into(),
        };
        let mut report = work.clone();
        report.steps = vec!["Report incomplete work".into()];
        let reflection = |action: &str| {
            format!("<reflect>\nlast_outcome: failure\ngoal_delta: same\nwrong_assumption: previous approach failed\nstrategy_change: abandon\nnext_minimal_action: {action}\n</reflect>")
        };
        let mut messages = vec![
            crate::task_origin::user_message(
                "Repair parser",
                crate::task_origin::MessageOrigin::User,
            ),
            json!({"role":"assistant", "accepted_plan":work, "content":reflection("Inspect the failing parser branch")}),
            json!({"role":"assistant", "accepted_plan":report, "content":reflection("Report incomplete work only"),
                "tool_calls":[{"id":"closeout", "function":{"name":"done", "arguments":"{}"}}]}),
            json!({"role":"tool", "tool_call_id":"closeout", "content":"OK: blocked", "blocked_report":{"remaining_acceptance":["acceptance 1"], "next_steps":"Inspect the failing parser branch"}}),
            crate::task_origin::user_message(
                "Continue with a new allowance",
                crate::task_origin::MessageOrigin::Runtime,
            ),
        ];
        let restored: crate::agent_session::AgentSession = serde_json::from_value(
            serde_json::to_value(crate::agent_session::AgentSession::new(
                None,
                None,
                None,
                None,
                messages.clone(),
            ))
            .unwrap(),
        )
        .unwrap();
        assert_eq!(
            last_reflection_from_messages(&restored.messages)
                .unwrap()
                .next_minimal_action,
            "Inspect the failing parser branch"
        );
        assert_eq!(
            plan_resume::latest_valid(&restored.messages, |_| true)
                .unwrap()
                .steps,
            work.steps
        );
        assert_eq!(
            WorkingMemory::from_messages(&restored.messages, None)
                .chosen_strategy
                .as_deref(),
            Some("Inspect the failing parser branch")
        );
        assert_eq!(
            restored.messages, messages,
            "projection must not erase the terminal receipt"
        );
        let mut compacted = restored.messages.clone();
        // Put work anchors beyond the recent window, leaving the report later.
        compacted.splice(
            2..2,
            (0..80).map(|_| json!({"role":"assistant","content":"continuing"})),
        );
        super::super::message_window::prune_message_window(&mut compacted);
        assert_eq!(
            plan_resume::latest_valid(&compacted, |_| true)
                .unwrap()
                .steps,
            work.steps
        );
        assert_eq!(
            last_reflection_from_messages(&compacted)
                .unwrap()
                .next_minimal_action,
            "Inspect the failing parser branch"
        );
        assert_eq!(
            WorkingMemory::from_messages(&compacted, None)
                .chosen_strategy
                .as_deref(),
            Some("Inspect the failing parser branch")
        );
        for result in [
            json!({"role":"tool","tool_call_id":"other","content":"OK: blocked","blocked_report":{}}),
            json!({"role":"tool","tool_call_id":"closeout","content":"GOVERNOR BLOCKED"}),
            json!({"role":"tool","tool_call_id":"closeout","content":"OK: blocked"}),
        ] {
            messages[3] = result;
            assert_eq!(
                last_reflection_from_messages(&messages)
                    .unwrap()
                    .next_minimal_action,
                "Report incomplete work only",
                "unaccepted or uncorrelated reports must not be suppressed"
            );
        }
    }

    #[test]
    fn terminal_reports_cannot_be_cited_as_evidence_after_resume() {
        for result in ["OK: blocked", "OK: done"] {
            let messages = vec![
                crate::task_origin::user_message(
                    "Repair the parser",
                    crate::task_origin::MessageOrigin::User,
                ),
                json!({"role":"assistant", "tool_calls":[{"id":"terminal", "function":{"name":"done", "arguments":"{\"summary\":\"unverified\",\"remaining_acceptance\":[\"acceptance 1\"]}"}}]}),
                json!({"role":"tool","tool_call_id":"terminal","content":result}),
            ];
            assert!(
                acceptance_report::current_commands(&messages, None).is_empty(),
                "terminal narration is not a verification receipt"
            );
        }
    }

    #[test]
    fn truthful_incomplete_report_preserves_unmet_checks_and_cannot_claim_unproved_work() {
        let plan = PlanBlock {
            goal: "repair".into(),
            steps: vec!["fix".into()],
            acceptance_criteria: vec!["tests pass".into()],
            risks: "none".into(),
            assumptions: "none".into(),
        };
        let args = json!({"summary":"The regression still fails", "completed_acceptance":[], "remaining_acceptance":["acceptance 1"], "acceptance_evidence":[], "next_steps":"Inspect the failed assertion in src/a.rs"});
        let call = |args: Value| ToolCallData {
            id: "report".into(),
            name: "done".into(),
            arguments: args.to_string(),
            thought_signature: None,
        };
        let messages = vec![crate::task_origin::user_message(
            "Repair src/a.rs. Run exactly: cargo test -q",
            crate::task_origin::MessageOrigin::User,
        )];
        let report = prepare(
            &[call(args.clone())],
            Some(&plan),
            &messages,
            None,
            None,
            &ObservationEvidence::default(),
        )
        .unwrap()
        .unwrap();
        assert!(report.render().starts_with("[BLOCKED"));
        assert!(!report.pending_exact_checks.is_empty());
        let mut bad = args.clone();
        bad["completed_acceptance"] = json!(["acceptance 1"]);
        assert!(prepare(
            &[call(bad)],
            Some(&plan),
            &messages,
            None,
            None,
            &ObservationEvidence::default()
        )
        .unwrap()
        .is_err());
        let mut done = args;
        done["remaining_acceptance"] = json!([]);
        assert!(prepare(
            &[call(done)],
            Some(&plan),
            &messages,
            None,
            None,
            &ObservationEvidence::default()
        )
        .is_none());
    }

    #[test]
    fn undeclared_task_keeps_truthful_blocked_report_admissible() {
        let plan = PlanBlock {
            goal: "greet".into(),
            steps: vec!["edit".into()],
            acceptance_criteria: vec!["greeting updated".into(), "docs updated".into()],
            risks: "none".into(),
            assumptions: "none".into(),
        };
        let call = ToolCallData {
            id: "report".into(),
            name: "done".into(),
            arguments: json!({
                "summary": "Docs path was not confirmed",
                "completed_acceptance": [],
                "remaining_acceptance": ["acceptance 1", "acceptance 2"],
                "acceptance_evidence": [],
                "next_steps": "confirm docs"
            })
            .to_string(),
            thought_signature: None,
        };
        // No <task_requirements> contract and no executed tools: we cannot
        // prove nothing remains, so the honest report must be admitted.
        let messages = vec![crate::task_origin::user_message(
            "Update the greeting output",
            crate::task_origin::MessageOrigin::User,
        )];
        let report = prepare(
            &[call],
            Some(&plan),
            &messages,
            None,
            None,
            &ObservationEvidence::default(),
        )
        .unwrap()
        .expect("undeclared work cannot be proven discharged");
        assert!(report.render().contains("acceptance 2"));
    }

    #[test]
    fn blocked_report_rejected_when_every_declared_requirement_is_discharged() {
        let plan = PlanBlock {
            goal: "greet".into(),
            steps: vec!["edit".into(), "verify".into()],
            acceptance_criteria: vec!["greeting updated".into(), "verified".into()],
            risks: "none".into(),
            assumptions: "none".into(),
        };
        let root = "Fix the greeting.\n<task_requirements>\n{\"version\":1,\"requirements\":[{\"id\":\"greet\",\"description\":\"update greeting\",\"artifacts\":[{\"path\":\"src/greet.py\",\"kind\":\"source\"}],\"checks\":[{\"command\":\"python3 test_greet.py\",\"baseline_failure\":false}]}]}\n</task_requirements>";
        let mut messages = vec![crate::task_origin::user_message(
            root,
            crate::task_origin::MessageOrigin::User,
        )];
        for (id, name, arguments, content) in [
            (
                "edit1",
                "patch_file",
                json!({"path":"src/greet.py","search":"?","replace":"!"}).to_string(),
                "OK: patched 'src/greet.py'",
            ),
            (
                "exec1",
                "exec",
                json!({"command":"python3 test_greet.py"}).to_string(),
                "OK (exit_code: 0)\n1 passed",
            ),
        ] {
            messages.push(json!({"role":"assistant","tool_calls":[{"id":id,"type":"function","function":{"name":name,"arguments":arguments}}]}));
            messages.push(json!({"role":"tool","tool_call_id":id,"content":content}));
        }
        let call = ToolCallData {
            id: "report".into(),
            name: "done".into(),
            arguments: json!({
                "summary": "claimed unfinished",
                "completed_acceptance": [],
                "remaining_acceptance": ["acceptance 1", "acceptance 2"],
                "acceptance_evidence": [],
                "next_steps": "none"
            })
            .to_string(),
            thought_signature: None,
        };
        let error = prepare(
            &[call],
            Some(&plan),
            &messages,
            None,
            None,
            &ObservationEvidence::default(),
        )
        .unwrap()
        .unwrap_err();
        assert!(error.to_string().contains("already discharged"), "{error}");
    }
}
