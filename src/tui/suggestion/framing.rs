//! Recover bounded envelope defects while preserving authored text and tool arguments.
use serde::Serialize;
use serde_json::Value;

mod repeated_summary;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Recovery {
    OpeningBrace,
    OpeningBraceAndQuote,
    RepeatedIdenticalSummary,
    SingleHandoffReference,
}

pub(super) fn object(raw: &str) -> Result<(Value, Option<Recovery>), String> {
    if raw.len() > 64 * 1024 {
        return Err("structured output exceeds 64 KiB".into());
    }
    let original_error = match crate::structured_output::object(raw) {
        Ok(parsed) => {
            let mut value = parsed.value;
            let recovered = parsed.prefix.trim().is_empty()
                && parsed.suffix.trim().is_empty()
                && single_handoff_reference(&mut value);
            return Ok((value, recovered.then_some(Recovery::SingleHandoffReference)));
        }
        Err(error) => error,
    };
    // The first field must be an exact known envelope key, at the start of the
    // response. No prose, fences, arrays, inner-object extraction or suffix repair.
    let text = raw.trim();
    // A complete object may repeat its display summary exactly once. Do not
    // combine this exception with a missing opener, a fence or surrounding prose.
    if text.starts_with('{') {
        if let Ok(value) = repeated_summary::object(raw) {
            return Ok((value, Some(Recovery::RepeatedIdenticalSummary)));
        }
        return Err(original_error);
    }
    for key in [
        "summary",
        "primary_blocker",
        "scores",
        "response_contract",
        "suggestions",
        "quickest_check",
        "why_this_first",
        "fallback",
    ] {
        for (prefix, head, recovery) in [
            ("{", format!("\"{key}\""), Recovery::OpeningBrace),
            ("{\"", format!("{key}\""), Recovery::OpeningBraceAndQuote),
        ] {
            if text
                .strip_prefix(&head)
                .is_some_and(|rest| rest.trim_start().starts_with(':'))
            {
                // The shared parser rejects duplicate keys at every depth,
                // competing objects, truncation and non-JSON trailing content.
                let repaired = format!("{prefix}{text}");
                let parsed = crate::structured_output::object(&repaired)?;
                if !parsed.prefix.is_empty() || !parsed.suffix.trim().is_empty() {
                    return Err("recovered envelope must contain exactly one JSON object".into());
                }
                return Ok((parsed.value, Some(recovery)));
            }
        }
    }
    Err(original_error)
}

// Only a no-tool clarification may express its one advisory reference as a
// string. Preserve the exact text; do not coerce fields on executable advice.
fn single_handoff_reference(value: &mut serde_json::Value) -> bool {
    use serde_json::Value;
    if value["response_contract"]["required"] != false {
        return false;
    }
    let Some(suggestions) = value.get_mut("suggestions").and_then(Value::as_array_mut) else {
        return false;
    };
    let [entry] = suggestions.as_mut_slice() else {
        return false;
    };
    if entry["kind"] != "clarify"
        || !entry["reason"]
            .as_str()
            .is_some_and(|s| !s.trim().is_empty())
        || entry.get("suggested_tool") != Some(&Value::Null)
        || !entry["suggested_args"]
            .as_object()
            .is_some_and(|args| args.is_empty())
    {
        return false;
    }
    let Some(reference) = entry.get_mut("based_on") else {
        return false;
    };
    if !reference
        .as_str()
        .is_some_and(|text| !text.trim().is_empty() && text.chars().count() <= 800)
    {
        return false;
    }
    *reference = Value::Array(vec![reference.take()]);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn only_the_opening_delimiters_change() {
        let value = json!({"summary":"Inspect 🐈‍⬛ {literal}", "fallback":null,
            "suggestions":[{"kind":"read","confidence":0.9,"suggested_tool":"read_file",
                "suggested_args":{"path":"src/lib.rs","start_line":140,"end_line":200}}]});
        // Serialize this field first to exercise both observed truncations.
        let raw = format!(
            "{{\"summary\":{},\"fallback\":null,\"suggestions\":{}}}",
            value["summary"], value["suggestions"]
        );
        assert_eq!(object(&raw).unwrap(), (value.clone(), None));
        for (offset, kind) in [
            (1, Recovery::OpeningBrace),
            (2, Recovery::OpeningBraceAndQuote),
        ] {
            assert_eq!(object(&raw[offset..]).unwrap(), (value.clone(), Some(kind)));
            let env = super::super::parse_observer_suggestion_envelope(&raw[offset..]).unwrap();
            assert_eq!(
                env.suggestions[0].suggested_args,
                value["suggestions"][0]["suggested_args"]
            );
            assert_eq!(super::super::validation_issue_for_raw(&raw[offset..]), None);
        }
    }

    #[test]
    fn incomplete_ambiguous_and_duplicate_values_are_not_repaired() {
        for raw in [
            "\"summary\":\"x\"", "summary:\"x\"}", "\"unknown\":1}",
            "Here: \"summary\":\"x\"}", "```json\n\"summary\":\"x\"}\n```",
            "\"summary\":\"x\"} trailing", "\"summary\":\"x\"} {}",
            "\"summary\":\"x\",\"summary\":\"y\"}",
            "\"summary\":\"x\",\"suggestions\":[{\"suggested_args\":{\"path\":\"a\",\"path\":\"b\"}}]}",
            "[\"summary\":\"x\"}]", "{\"summary\":\"x\",}",
        ] {
            assert!(object(raw).is_err(), "{raw}");
        }
        let oversized = format!("\"summary\":\"{}\"}}", "x".repeat(64 * 1024));
        assert!(object(&oversized).is_err());
    }

    #[test]
    fn framing_does_not_supply_required_arguments_or_raise_confidence() {
        for (kind, tool, args) in [
            ("edit", "patch_file", json!({"path":"src/lib.rs"})),
            ("read", "read_file", json!({"path":42})),
            ("read", "exec", json!({"command":"cargo test"})),
        ] {
            let raw = format!(
                "\"suggestions\":[{}]}}",
                json!({"kind":kind,"suggested_tool":tool,"suggested_args":args,"confidence":0.9})
            );
            assert!(object(&raw).is_ok());
            assert!(super::super::validation_issue_for_raw(&raw).is_some());
        }
        let low = "\"suggestions\":[{\"kind\":\"read\",\"suggested_tool\":\"read_file\",\"suggested_args\":{\"path\":\"a\"},\"confidence\":0.2}]}";
        let env = super::super::parse_observer_suggestion_envelope(low).unwrap();
        assert_eq!(env.suggestions[0].confidence, 0.2);
        assert!(crate::tui::events::build_observer_suggestion_soft_hint(&env).is_none());
    }

    #[test]
    fn recovered_web_advice_cannot_invent_native_completion_evidence() {
        let raw = format!(
            "\"summary\":\"done\",\"suggestions\":[{}]}}",
            json!({
            "kind":"done","confidence":0.95,"suggested_tool":"done","suggested_args":{
                "summary":"all tests passed","completed_acceptance":["tests pass"],"remaining_acceptance":[],
                "acceptance_evidence":[{"criterion":"tests pass","command":"cargo test -q"}]}})
        );
        let response = crate::observer_action::evaluate(crate::observer_action::Request {
            packet: json!({}),
            messages: vec![json!({"role":"user","content":"Fix the bug and run cargo test -q"})],
            test_command: None,
            lang: "en".into(),
            reason: "verification_failure".into(),
            response: Some(raw),
            final_summary: None,
            repair_native: false,
        });
        assert_eq!(response.format_recovery, Some(Recovery::OpeningBrace));
        assert!(response.envelope.is_some());
        assert!(response.issue.is_some());
        assert!(response.hint.is_none());
        assert!(response.response_contract.is_none());
    }
}

#[cfg(test)]
mod handoff_reference_tests {
    use super::*;
    use serde_json::{json, Value};

    #[test]
    fn actual_scalar_handoff_references_retain_their_text_and_only_advisory_authority() {
        let samples: Vec<String> = serde_json::from_str(include_str!(
            "../../../tests/fixtures/observer-handoff/single-reference.json"
        ))
        .unwrap();
        for sample in samples {
            let original: Value = serde_json::from_str(&sample).unwrap();
            let reference = original["suggestions"][0]["based_on"].clone();
            let (parsed, repair) = object(&sample).unwrap();
            assert_eq!(repair, Some(Recovery::SingleHandoffReference));
            let mut expected = original.clone();
            expected["suggestions"][0]["based_on"] = json!([reference]);
            assert_eq!(parsed, expected);
            assert_eq!(super::super::transport_issue_for_raw(&sample), None);
            assert_eq!(super::super::validation_issue_for_raw(&sample), None);
            let mut rejected = vec![];
            let mut tool = original.clone();
            tool["suggestions"][0]["suggested_tool"] = json!("done");
            rejected.push(tool);
            let mut required = original.clone();
            required["response_contract"]["required"] = json!(true);
            rejected.push(required);
            let mut mapped = original.clone();
            mapped["suggestions"][0]["based_on"] = json!({"task_horizon":true});
            rejected.push(mapped);
            let mut multiple = original.clone();
            multiple["suggestions"]
                .as_array_mut()
                .unwrap()
                .push(original["suggestions"][0].clone());
            rejected.push(multiple);
            let mut empty_reason = original.clone();
            empty_reason["suggestions"][0]["reason"] = json!("");
            rejected.push(empty_reason);
            for raw in rejected {
                assert!(super::super::transport_issue_for_raw(&raw.to_string()).is_some());
            }
        }
    }
}
