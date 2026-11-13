//! Preserve a test scenario when its proposed edit contains only a path.
use crate::tui::{agent, suggestion};
use serde_json::{json, Value};

pub(super) fn keep_as_advice(
    envelope: &mut suggestion::ObserverSuggestionEnvelope,
    messages: &[Value],
    test_command: Option<&str>,
    tool_root: Option<&str>,
    remaining: Option<usize>,
) -> bool {
    let Some(primary) = envelope.suggestions.first_mut() else {
        return false;
    };
    if primary.kind != suggestion::ObserverSuggestionKind::Edit
        || !matches!(
            primary.suggested_tool.as_deref(),
            Some("write_file" | "patch_file" | "apply_diff")
        )
        || primary.reason.is_empty()
        || !primary
            .suggested_args
            .as_object()
            .is_some_and(|args| args.len() == 1)
    {
        return false;
    }
    let Some(path) = primary.suggested_args["path"].as_str() else {
        return false;
    };
    let obligations = agent::task_obligations::project_advisory(messages, test_command, tool_root);
    if !obligations.can_offer_test_recipe(path, remaining)
        || obligations
            .explicit_edit_paths
            .as_ref()
            .is_some_and(|scope| {
                scope
                    .issue(
                        primary.suggested_tool.as_deref().unwrap_or_default(),
                        &primary.suggested_args,
                        None,
                    )
                    .is_some()
            })
    {
        return false;
    }
    primary.kind = suggestion::ObserverSuggestionKind::Clarify;
    primary.suggested_tool = None;
    primary.suggested_args = json!({});
    envelope.suggestions.truncate(1);
    envelope.response_contract = Default::default();
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::observer_action::{evaluate, resolve_advice, Request};

    const TEST: &str = "tests/ledger.rs";
    const REASON: &str =
        "Run the CLI on the valid fixture first. Set one incorrect parent ID and assert rejection.";

    fn messages() -> Vec<Value> {
        vec![json!({"role":"user","content":format!(
            "Extend the ledger tests.\nAllowed paths only: {TEST}\n<task_requirements>\n{}\n</task_requirements>",
            json!({"version":1,"requirements":[{"id":"parent","description":REASON,
                "artifacts":[{"path":TEST,"kind":"test"}],
                "checks":[{"command":"cargo test --test ledger"}]}]})
        )})]
    }

    fn reply(args: Value, confidence: f32) -> String {
        json!({"summary":"Add the required scenario.","response_contract":{"required":true},
            "suggestions":[{"kind":"edit","reason":REASON,"confidence":confidence,
              "suggested_tool":"patch_file","suggested_args":args}]})
        .to_string()
    }

    #[test]
    fn returns_path_only_edit_explanation_without_a_tool_or_confidence_change() {
        for confidence in [0.92, 0.42] {
            let raw = reply(json!({"path":TEST}), confidence);
            let response = evaluate(Request {
                packet: json!({"tool_root":"/workspace","work_budget":{"limit":16,"used":3}}),
                messages: messages(),
                test_command: None,
                lang: "en".into(),
                reason: "incomplete_task".into(),
                response: Some(raw.clone()),
                final_summary: None,
                repair_native: false,
            });
            assert!(response.test_recipe_retained && response.issue.is_none());
            let envelope = response.envelope.unwrap();
            let primary = suggestion::primary_suggestion(&envelope).unwrap();
            assert_eq!(primary.kind, suggestion::ObserverSuggestionKind::Clarify);
            assert_eq!(primary.reason, REASON);
            assert_eq!(primary.confidence, confidence);
            assert_eq!(primary.suggested_tool, None);
            assert_eq!(primary.suggested_args, json!({}));
            assert!(!envelope.response_contract.required && response.response_contract.is_none());
            assert_eq!(
                response
                    .hint
                    .as_ref()
                    .is_some_and(|hint| hint.contains(REASON)),
                confidence >= 0.75
            );
            assert!(response.repair_prompt.is_none());
            assert_eq!(
                suggestion::parse_observer_suggestion_envelope(&raw)
                    .unwrap()
                    .suggestions[0]
                    .suggested_tool
                    .as_deref(),
                Some("patch_file")
            );
        }
    }

    #[test]
    fn leaves_other_invalid_edits_and_executable_edits_on_the_native_path() {
        let messages = messages();
        let resolve = |args, remaining| {
            resolve_advice(
                &reply(args, 0.92),
                &messages,
                None,
                Some("/workspace"),
                Some(remaining),
            )
        };
        for (args, remaining) in [
            (json!({"path":"src/ledger.rs"}), 8),
            (json!({"path":TEST,"replace":"incomplete"}), 8),
            (json!({"path":TEST}), 0),
        ] {
            let resolved = resolve(args, remaining);
            assert!(!resolved.test_recipe_retained && resolved.issue.is_some());
        }
        let resolved = resolve(json!({"path":TEST,"search":"old","replace":"new"}), 8);
        assert!(!resolved.test_recipe_retained && resolved.issue.is_none());
    }
}
