//! New native verification failures yield a Coder slice without granting proof.
use super::{exec_verification::ExecVerificationContext, ActionIdentity, ExecKind, ToolCallData};
use crate::execution_evidence::{auto_test_outcome, exec_outcome, AutoTestOutcome};
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Serialize)]
pub(super) struct Failure {
    call_id: String,
    tool: String,
    command: Option<String>,
    automatic_test: bool,
    #[serde(skip)]
    trigger: Option<ActionIdentity>,
}

pub(super) fn latest(messages: &[Value], configured: Option<&str>) -> Option<Failure> {
    let messages = crate::task_origin::current_task_messages(messages);
    let (index, result) = messages
        .iter()
        .enumerate()
        .rev()
        .find(|(_, m)| m["role"] == "tool")?;
    let id = result["tool_call_id"].as_str()?;
    let content = result["content"].as_str()?;
    let call = messages[..index]
        .iter()
        .rev()
        .filter(|m| m["role"] == "assistant")
        .filter_map(|m| m["tool_calls"].as_array())
        .flat_map(|calls| calls.iter().rev())
        .find(|call| call["id"] == id)?;
    let tool = call["function"]["name"].as_str()?;
    let mut command = None;
    let automatic_test = match tool {
        "exec" => {
            if exec_outcome(content) != Some(false) {
                return None;
            }
            let args: Value = serde_json::from_str(call["function"]["arguments"].as_str()?).ok()?;
            let authored = args["command"].as_str()?;
            if ExecVerificationContext::from_messages(configured, messages)
                .classify(authored)
                .kind
                != ExecKind::Verify
            {
                return None;
            }
            command = Some(authored.to_owned());
            false
        }
        "write_file" | "patch_file" | "apply_diff" => {
            if auto_test_outcome(tool, content) != AutoTestOutcome::Failed {
                return None;
            }
            true
        }
        _ => return None,
    };
    Some(Failure {
        call_id: id.to_owned(),
        tool: tool.to_owned(),
        command,
        automatic_test,
        trigger: super::tool_call_action_sig(&ToolCallData {
            id: id.to_owned(),
            name: tool.to_owned(),
            arguments: call["function"]["arguments"].as_str()?.to_owned(),
            thought_signature: None,
        }),
    })
}

/// A handoff cannot discharge the reflection required by a failed native exec.
/// Automatic tests on successful edits retain their existing diagnosis/impact
/// gates; they did not impose this exec reflection requirement before yielding.
pub(super) fn pending_exec_reflection(
    messages: &[Value],
    configured: Option<&str>,
) -> Option<(String, Option<ActionIdentity>)> {
    // Refusals execute no action and cannot discharge the original obligation.
    // Keep the early-yield detector strict: skipping them there would repeatedly
    // yield the same historical failure after a newly blocked action.
    let messages = crate::task_origin::current_task_messages(messages);
    let index = messages.iter().rposition(|m| {
        m["role"] == "tool"
            && m["content"]
                .as_str()
                .is_none_or(crate::execution_evidence::exec_may_have_run)
    })?;
    let failure = latest(&messages[..=index], configured)?;
    if failure.automatic_test {
        return None;
    }
    Some((
        format!(
            "Native verification failed before continuation: {}. Diagnose the failure before retrying.",
            failure.command.as_deref().unwrap_or("verification")
        ),
        failure.trigger,
    ))
}

pub(super) fn newly_executed(
    messages: &[Value],
    configured: Option<&str>,
    initial_steps: usize,
    current_steps: usize,
) -> Option<Failure> {
    // The caller's monotonic local execution counter survives message pruning
    // and repeated provider call IDs. A resumed historical failure cannot yield.
    if current_steps <= initial_steps
        || !crate::work_budget::current().is_some_and(|context| {
            context.role == "coder" && context.budget.can_return_to_observer()
        })
    {
        return None;
    }
    latest(messages, configured)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn pending_exec_reflection_preserves_trigger_and_stops_after_diagnosis() {
        let args = json!({"command":"cargo test -q","cwd":"crate1"});
        let mut messages = transcript("exec", args.clone(), "FAILED (exit_code: 1)");
        let (reason, trigger) = pending_exec_reflection(&messages, None).unwrap();
        assert!(reason.contains("cargo test -q"));
        assert_eq!(
            trigger,
            Some(ActionIdentity::Exec {
                command: "cargo test -q".into(),
                cwd: Some("crate1".into())
            })
        );
        for content in [
            "GOVERNOR BLOCKED\nReflection/action mismatch blocked.",
            "REJECTED BY USER",
        ] {
            messages.extend([
                json!({"role":"assistant","tool_calls":[{"id":"blocked","function":{"name":"exec","arguments":args.to_string()}}]}),
                json!({"role":"tool","tool_call_id":"blocked","content":content}),
            ]);
            assert!(pending_exec_reflection(&messages, None).is_some());
            assert!(
                latest(&messages, None).is_none(),
                "old failure must not cause another early yield"
            );
        }
        messages.extend([
            json!({"role":"assistant","tool_calls":[{"id":"read","function":{"name":"read_file","arguments":"{\"path\":\"src/lib.rs\"}"}}]}),
            json!({"role":"tool","tool_call_id":"read","content":"[src/lib.rs]\nsource"}),
        ]);
        assert!(pending_exec_reflection(&messages, None).is_none());
        assert!(pending_exec_reflection(
            &transcript(
                "patch_file",
                json!({"path":"x"}),
                "OK: patched 'x'\n[auto-test] ✗ FAILED (exit 1)"
            ),
            None
        )
        .is_none());
    }

    #[tokio::test]
    async fn continuation_rejects_invalid_reflection_before_a_diagnostic_read() {
        use super::super::*;
        use clap::Parser;
        use httpmock::prelude::*;
        for (blocked_first, valid) in [(false, false), (false, true), (true, false), (true, true)] {
            let server = MockServer::start();
            let reflection = format!("<reflect>\nlast_outcome: failure\ngoal_delta: same\nwrong_assumption: the test would pass\nstrategy_change: {}\nnext_minimal_action: read fixture.txt\n</reflect>", if valid {"adjust"} else {"keep"});
            let content = format!("{reflection}\n<think>\ngoal: fix fixture.txt\nstep: 1\ntool: read_file\nrisk: none\ndoubt: cause is unknown\nnext: read fixture.txt\nverify: cargo test -q\n</think>");
            let mock = server.mock(|when, then| {
                when.method(POST).path("/chat/completions").body_contains("Native verification failed before continuation");
                then.status(200).header("content-type", "text/event-stream")
                    .body(format!("data: {}\n\ndata: [DONE]\n\n", json!({"choices":[{"delta":{"content":content,"tool_calls":[{"index":0,"id":"resumed-read","function":{"name":"read_file","arguments":"{\"path\":\"fixture.txt\"}"}}]},"finish_reason":"tool_calls"}]})));
            });
            let root = tempfile::tempdir().unwrap();
            std::fs::write(root.path().join("fixture.txt"), "before\n").unwrap();
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
            let mut messages = transcript(
                "exec",
                json!({"command":"cargo test -q"}),
                "FAILED (exit_code: 1)",
            );
            messages[0]["content"] =
                json!("Fix fixture.txt and verify it.\nRun exactly: cargo test -q");
            messages[1]["accepted_plan"] = json!({"goal":"Fix fixture.txt","steps":["Read fixture.txt","Repair fixture.txt","Run cargo test -q"],"acceptance_criteria":["cargo test -q passes"],"risks":"preserve coverage","assumptions":"fixture exists"});
            if blocked_first {
                let retry_server = MockServer::start();
                let retry_content = "<reflect>\nlast_outcome: failure\ngoal_delta: same\nwrong_assumption: the test would pass\nstrategy_change: adjust\nnext_minimal_action: inspect fixture.txt\n</reflect>\n<think>\ngoal: fix fixture.txt\nstep: 3\ntool: exec\nrisk: none\ndoubt: test failed\nnext: run cargo test -q\nverify: cargo test -q\n</think>";
                let retry = retry_server.mock(|when, then| {
                    when.method(POST).path("/chat/completions");
                    then.status(200).header("content-type", "text/event-stream")
                        .body(format!("data: {}\n\ndata: [DONE]\n\n", json!({"choices":[{"delta":{"content":retry_content,"tool_calls":[{"index":0,"id":"blocked-retry","function":{"name":"exec","arguments":"{\"command\":\"cargo test -q\"}"}}]},"finish_reason":"tool_calls"}]})));
                });
                let mut retry_cfg = cfg.clone();
                retry_cfg.base_url = retry_server.base_url();
                let (tx, _rx) = mpsc::channel(512);
                let retry_start = AgenticStartState {
                    messages,
                    checkpoint: None,
                    cur_cwd: None,
                    observation_cache: None,
                    session_bridge: None,
                    create_checkpoint: false,
                };
                let retried = Box::pin(run_agentic_json(
                    retry_start,
                    &retry_cfg,
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
                retry.assert_hits(1);
                assert!(
                    retried.messages.iter().any(|m| m["role"] == "tool"
                        && m["tool_call_id"] == "blocked-retry"
                        && m["content"]
                            .as_str()
                            .is_some_and(|s| s.starts_with("GOVERNOR BLOCKED")
                                && s.contains("Reflection/action mismatch blocked"))),
                    "{:?}",
                    retried.messages
                );
                // Serialize the native transcript as a real second continuation.
                messages = serde_json::from_str(&serde_json::to_string(&retried.messages).unwrap())
                    .unwrap();
            }
            let start = AgenticStartState {
                messages,
                checkpoint: None,
                cur_cwd: None,
                observation_cache: None,
                session_bridge: None,
                create_checkpoint: false,
            };
            let (tx, mut rx) = mpsc::channel(512);
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
            assert_eq!(
                result.messages.iter().any(|m| m["role"] == "tool"
                    && m["tool_call_id"] == "resumed-read"
                    && m["content"].as_str().is_some_and(|s| s.contains("before"))),
                valid
            );
            let mut rejected = false;
            while let Some(token) = rx.recv().await {
                if let StreamToken::Telemetry(event) = token {
                    if event.event == "pre_tool_gate_rejected" && event.data["gate"] == "reflection"
                    {
                        rejected = true;
                    }
                }
            }
            assert_eq!(rejected, !valid);
        }
    }

    fn transcript(tool: &str, args: Value, result: &str) -> Vec<Value> {
        vec![
            json!({"role":"user","content":"Repair the failure. Run exactly: cargo test -q."}),
            json!({"role":"assistant","tool_calls":[{"id":"same-id","function":{"name":tool,"arguments":args.to_string()}}]}),
            json!({"role":"tool","tool_call_id":"same-id","content":result}),
        ]
    }

    #[test]
    fn only_executed_verification_and_failed_automatic_tests_can_yield() {
        for content in [
            "GOVERNOR BLOCKED\nFAILED (exit_code: 1)",
            "REJECTED BY USER",
            "Awaiting approval",
            "OK (exit_code: 0)",
            "unconfirmed",
        ] {
            assert!(
                latest(
                    &transcript("exec", json!({"command":"cargo test -q"}), content),
                    None
                )
                .is_none(),
                "{content}"
            );
        }
        assert!(latest(
            &transcript(
                "exec",
                json!({"command":"cargo test -q"}),
                "FAILED (exit_code: 1)"
            ),
            None
        )
        .is_some());
        assert!(latest(
            &transcript(
                "exec",
                json!({"command":"cat absent.txt"}),
                "FAILED (exit_code: 1)"
            ),
            None
        )
        .is_none());
        assert!(latest(
            &transcript("read_file", json!({"path":"x"}), "FAILED (exit_code: 1)"),
            None
        )
        .is_none());
        let automatic = format!(
            "OK: patched 'x'\n{}\n[auto-test] ✗ FAILED (exit 1)",
            "diff context\n".repeat(500)
        );
        assert!(
            latest(
                &transcript("patch_file", json!({"path":"x"}), &automatic),
                None
            )
            .unwrap()
            .automatic_test
        );
        let mut old = transcript(
            "exec",
            json!({"command":"cargo test -q"}),
            "FAILED (exit_code: 1)",
        );
        old.push(json!({"role":"user","content":"Start a new task"}));
        assert!(latest(&old, None).is_none());
    }

    #[tokio::test]
    async fn resume_and_budget_do_not_create_repeated_or_unfunded_handoffs() {
        let messages = transcript(
            "exec",
            json!({"command":"cargo test -q"}),
            "FAILED (exit_code: 1)",
        );
        let budget = crate::work_budget::WorkBudget::new("task", 4, None);
        assert!(newly_executed(&messages, None, 0, 1).is_none());
        crate::work_budget::scope(
            Some(crate::work_budget::Context {
                budget: budget.with_observer_return(2),
                role: "coder",
            }),
            async {
                assert!(newly_executed(&messages, None, 1, 1).is_none());
                assert!(newly_executed(&messages, None, 1, 2).is_some());
                for _ in 0..3 {
                    drop(budget.begin("coder", &Value::Null).unwrap());
                }
                assert!(newly_executed(&messages, None, 1, 2).is_none());
            },
        )
        .await;
    }
}
