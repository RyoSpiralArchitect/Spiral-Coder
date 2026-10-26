//! New native verification failures yield a Coder slice without granting proof.
use super::{exec_verification::ExecVerificationContext, ExecKind};
use crate::execution_evidence::{auto_test_outcome, exec_outcome, AutoTestOutcome};
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Serialize)]
pub(super) struct Failure {
    call_id: String,
    tool: String,
    command: Option<String>,
    automatic_test: bool,
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
    })
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
