//! Separate execution of an Observer's exact action from its whole continuation.
use super::{agent::evidence_packet::ActionEvidence, suggestion::ObserverSuggestion};
use crate::work_budget::SuggestedActionReceipt;

pub(super) fn project(
    primary: Option<&ObserverSuggestion>,
    actions: &[&ActionEvidence],
) -> Option<SuggestedActionReceipt> {
    let primary = primary?;
    let tool = primary
        .suggested_tool
        .as_deref()
        .filter(|tool| !tool.is_empty())?;
    // This receipt is deliberately stricter than the legacy decision inference:
    // Apart from the defined check-ID expansion, no subset matching, path alias
    // resolution or invented/default arguments.
    let matches: Vec<_> = actions
        .iter()
        .copied()
        .filter(|action| {
            action.tool == tool
                && (action.arguments == primary.suggested_args
                    || resolved_check_matches(action, &primary.suggested_args))
        })
        .collect();
    let executed_matches: Vec<_> = matches
        .iter()
        .copied()
        .filter(|action| action.executed == Some(true))
        .collect();
    let executed = if !executed_matches.is_empty() {
        Some(true)
    } else if matches.iter().any(|action| action.executed.is_none()) {
        None
    } else {
        Some(false)
    };
    let succeeded = if executed_matches.is_empty() {
        // Only confirmed rejections establish an unsuccessful attempt. Native
        // approval-pending results also say executed=false, but have no outcome.
        (!matches.is_empty()
            && executed == Some(false)
            && matches.iter().all(|action| action.succeeded == Some(false)))
        .then_some(false)
    } else if executed_matches
        .iter()
        .any(|action| action.succeeded.is_none())
    {
        None
    } else {
        Some(
            executed_matches
                .iter()
                .all(|action| action.succeeded == Some(true)),
        )
    };
    Some(SuggestedActionReceipt {
        tool: tool.into(),
        arguments: primary.suggested_args.clone(),
        matching_call_ids: matches
            .iter()
            .map(|action| action.call_id.clone())
            .collect(),
        executed,
        succeeded,
    })
}

// A resolved native selection records the ID plus its runtime-resolved command
// and root. This is one defined normalization, not arbitrary subset matching.
fn resolved_check_matches(action: &ActionEvidence, proposed: &serde_json::Value) -> bool {
    action.tool == "exec"
        && proposed.as_object().is_some_and(|args| args.len() == 1)
        && proposed["check_id"]
            .as_str()
            .is_some_and(|id| action.arguments["check_id"] == id)
        && action.arguments.as_object().is_some_and(|args| {
            args.len() == 3
                && args
                    .get("command")
                    .is_some_and(serde_json::Value::is_string)
                && args.get("cwd").is_some_and(serde_json::Value::is_string)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::agent::current_evidence::Freshness;
    use crate::tui::suggestion::ObserverSuggestionKind;
    use crate::work_budget::AssistReceipt;
    use serde_json::{json, Value};

    fn suggestion(tool: Option<&str>, arguments: Value) -> ObserverSuggestion {
        ObserverSuggestion {
            kind: ObserverSuggestionKind::Edit,
            reason: "repair the source".into(),
            confidence: 0.9,
            suggested_tool: tool.map(str::to_owned),
            suggested_args: arguments,
            based_on: vec!["native evidence".into()],
        }
    }

    fn action(
        id: &str,
        tool: &str,
        arguments: Value,
        executed: Option<bool>,
        succeeded: Option<bool>,
    ) -> ActionEvidence {
        ActionEvidence {
            edit_outcome: None,
            call_id: id.into(),
            tool: tool.into(),
            arguments,
            result: None,
            executed,
            succeeded,
            result_step: None,
            freshness: Freshness::Current,
        }
    }

    fn project_actions(
        proposal: &ObserverSuggestion,
        actions: &[ActionEvidence],
    ) -> SuggestedActionReceipt {
        project(Some(proposal), &actions.iter().collect::<Vec<_>>()).unwrap()
    }

    #[test]
    fn rejected_and_pending_check_selections_keep_their_call_ids_review_regression() {
        let proposal = suggestion(Some("exec"), json!({"check_id":"required-1"}));
        for outcome in [Some(false), None] {
            let resolved = action(
                "check",
                "exec",
                json!({"check_id":"required-1","command":"cargo test","cwd":"/workspace"}),
                Some(false),
                outcome,
            );
            let receipt = project_actions(&proposal, &[resolved]);
            assert_eq!(receipt.matching_call_ids, ["check"]);
            assert_eq!(receipt.executed, Some(false));
            assert_eq!(receipt.succeeded, outcome);
        }
    }

    #[test]
    fn a_check_id_matches_only_its_executed_runtime_resolution() {
        let proposal = suggestion(Some("exec"), json!({"check_id":"required-1"}));
        let mut resolved = action(
            "check",
            "exec",
            json!({"check_id":"required-1", "command":"cargo test", "cwd":"/workspace"}),
            Some(true),
            Some(true),
        );
        assert_eq!(
            project_actions(&proposal, std::slice::from_ref(&resolved)).executed,
            Some(true)
        );
        resolved.executed = Some(false);
        assert_eq!(
            project_actions(&proposal, std::slice::from_ref(&resolved)).executed,
            Some(false)
        );
        resolved.executed = Some(true);
        resolved.arguments["check_id"] = json!("required-2");
        assert_eq!(
            project_actions(&proposal, &[resolved]).executed,
            Some(false)
        );
    }

    #[test]
    fn unrelated_continuation_does_not_execute_the_suggested_action() {
        let args = json!({"path":"src/main.rs","search":"old","replace":"new"});
        let proposal = suggestion(Some("patch_file"), args.clone());
        let read = action(
            "read",
            "read_file",
            json!({"path":"src/main.rs"}),
            Some(true),
            None,
        );
        assert_eq!(
            project_actions(&proposal, &[read]),
            SuggestedActionReceipt {
                tool: "patch_file".into(),
                arguments: args,
                matching_call_ids: vec![],
                executed: Some(false),
                succeeded: None,
            }
        );
    }

    #[test]
    fn no_tool_or_primary_suggestion_has_unknown_execution() {
        assert_eq!(project(None, &[]), None);
        for tool in [None, Some("")] {
            assert_eq!(project(Some(&suggestion(tool, Value::Null)), &[]), None);
        }
    }

    #[test]
    fn exact_arguments_reject_subset_defaults_and_path_aliases() {
        let proposal = suggestion(Some("read_file"), json!({"path":"src/main.rs"}));
        for args in [
            json!({"path":"src/main.rs","start_line":1}),
            json!({"path":"./src/main.rs"}),
            json!({}),
            Value::Null,
        ] {
            let actual = action("different", "read_file", args, Some(true), None);
            assert_eq!(project_actions(&proposal, &[actual]).executed, Some(false));
        }
        let empty = suggestion(Some("read_file"), json!({}));
        let actual = action(
            "not-a-wildcard",
            "read_file",
            json!({"path":"src/main.rs"}),
            Some(true),
            None,
        );
        assert_eq!(project_actions(&empty, &[actual]).executed, Some(false));
    }

    #[test]
    fn object_key_order_is_irrelevant_but_tool_name_is_exact() {
        let proposal = suggestion(
            Some("exec"),
            json!({"command":"cargo test","timeout_ms":10}),
        );
        let args: Value =
            serde_json::from_str(r#"{"timeout_ms":10,"command":"cargo test"}"#).unwrap();
        let good = action("good", "exec", args.clone(), Some(true), Some(true));
        let wrong_tool = action("wrong-tool", "shell", args, Some(true), Some(true));
        assert_eq!(
            project_actions(&proposal, &[good, wrong_tool]).matching_call_ids,
            ["good"]
        );
    }

    #[test]
    fn blocked_only_and_pending_matches_do_not_claim_execution() {
        let proposal = suggestion(Some("exec"), json!({"command":"cargo test"}));
        for (output, executed, succeeded) in [
            (
                Some("GOVERNOR BLOCKED\nMissing plan"),
                Some(false),
                Some(false),
            ),
            (Some("REJECTED BY USER"), Some(false), Some(false)),
            (Some("Awaiting approval"), Some(false), None),
            (None, None, None),
        ] {
            let mut messages = vec![
                json!({"role":"user","content":"Run cargo test."}),
                json!({"role":"assistant","tool_calls":[{
                    "id":"native","type":"function","function":{
                        "name":"exec","arguments":proposal.suggested_args.to_string()
                    }
                }]}),
            ];
            if let Some(output) = output {
                messages.push(json!({"role":"tool","tool_call_id":"native","content":output}));
            }
            let packet = crate::tui::agent::evidence_packet::project(&messages, None);
            assert_eq!(packet.actions.len(), 1);
            assert_eq!(
                (packet.actions[0].executed, packet.actions[0].succeeded),
                (executed, succeeded)
            );
            assert_eq!(
                project_actions(&proposal, &packet.actions),
                SuggestedActionReceipt {
                    tool: "exec".into(),
                    arguments: proposal.suggested_args.clone(),
                    matching_call_ids: vec!["native".into()],
                    executed,
                    succeeded,
                }
            );
        }
    }

    #[test]
    fn a_pending_approval_does_not_inherit_an_earlier_rejection() {
        let proposal = suggestion(Some("exec"), json!({"command":"cargo test"}));
        let mut messages = vec![json!({"role":"user","content":"Run cargo test."})];
        for (id, output) in [
            ("blocked", "GOVERNOR BLOCKED\nMissing plan"),
            ("pending", "Awaiting approval"),
        ] {
            messages.push(json!({"role":"assistant","tool_calls":[{
                "id":id,"type":"function","function":{
                    "name":"exec","arguments":proposal.suggested_args.to_string()
                }
            }]}));
            messages.push(json!({"role":"tool","tool_call_id":id,"content":output}));
        }
        let packet = crate::tui::agent::evidence_packet::project(&messages, None);
        let receipt = project_actions(&proposal, &packet.actions);
        assert_eq!(receipt.matching_call_ids, ["blocked", "pending"]);
        assert_eq!((receipt.executed, receipt.succeeded), (Some(false), None));
    }

    #[test]
    fn read_execution_does_not_invent_a_success_outcome() {
        let proposal = suggestion(Some("read_file"), json!({"path":"src/main.rs"}));
        let actual = action(
            "read",
            "read_file",
            proposal.suggested_args.clone(),
            Some(true),
            None,
        );
        let receipt = project_actions(&proposal, &[actual]);
        assert_eq!((receipt.executed, receipt.succeeded), (Some(true), None));
    }

    #[test]
    fn blocked_then_success_aggregates_only_executed_matches() {
        let proposal = suggestion(Some("exec"), json!({"command":"cargo test"}));
        let blocked = action(
            "blocked",
            "exec",
            proposal.suggested_args.clone(),
            Some(false),
            Some(false),
        );
        let success = action(
            "success",
            "exec",
            proposal.suggested_args.clone(),
            Some(true),
            Some(true),
        );
        let receipt = project_actions(&proposal, &[blocked, success]);
        assert_eq!(receipt.matching_call_ids, ["blocked", "success"]);
        assert_eq!(
            (receipt.executed, receipt.succeeded),
            (Some(true), Some(true))
        );
    }

    #[test]
    fn failed_then_success_retains_the_unsuccessful_execution() {
        let proposal = suggestion(Some("exec"), json!({"command":"cargo test"}));
        for outcomes in [
            [Some(false), Some(true)],
            [Some(true), Some(false)],
            [None, Some(true)],
        ] {
            let actions = [
                action(
                    "first",
                    "exec",
                    proposal.suggested_args.clone(),
                    Some(true),
                    outcomes[0],
                ),
                action(
                    "second",
                    "exec",
                    proposal.suggested_args.clone(),
                    Some(true),
                    outcomes[1],
                ),
            ];
            let receipt = project_actions(&proposal, &actions);
            assert_eq!(receipt.matching_call_ids, ["first", "second"]);
            assert_eq!(receipt.executed, Some(true));
            assert_eq!(
                receipt.succeeded,
                if outcomes.contains(&None) {
                    None
                } else {
                    Some(false)
                }
            );
        }
    }

    #[test]
    fn historical_receipts_do_not_invent_suggestion_execution() {
        let raw = json!({
            "assist_id":1,"continuation_id":2,"decision":"accept","call_ids":["read"],
            "executed":true,"succeeded":null,"completed":false,"pending_exact_checks":["cargo test"]
        });
        let receipt: AssistReceipt = serde_json::from_value(raw.clone()).unwrap();
        assert_eq!(receipt.suggestion_execution, None);
        let mut roundtrip = serde_json::to_value(receipt).unwrap();
        assert_eq!(roundtrip["suggestion_execution"], Value::Null);
        roundtrip
            .as_object_mut()
            .unwrap()
            .remove("suggestion_execution");
        assert_eq!(roundtrip, raw);
    }
}
