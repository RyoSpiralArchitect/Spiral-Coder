//! The only exception to unique keys is one duplicate top-level `summary`
//! with the same JSON string literal (including escapes) as the first.
use crate::structured_output::UniqueValue;
use serde::de::{self, MapAccess, Visitor};
use serde::Deserializer;
use serde_json::{value::RawValue, Map, Value};
use std::fmt;

pub(super) fn object(raw: &str) -> serde_json::Result<Value> {
    let mut de = serde_json::Deserializer::from_str(raw);
    let object = de.deserialize_map(RepeatedSummary)?;
    de.end()?; // Never choose between a repeated field and another answer.
    Ok(object)
}

struct RepeatedSummary;
impl<'de> Visitor<'de> for RepeatedSummary {
    type Value = Value;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a single Observer object repeating one identical summary string")
    }

    fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Value, M::Error> {
        let mut fields = Map::new();
        let mut first_summary = None;
        let mut repeated = false;
        // Borrow the literal key. Escaped top-level keys are outside this fallback.
        while let Some(key) = map.next_key::<&'de str>()? {
            if key == "summary" {
                let literal = map.next_value::<&'de RawValue>()?.get();
                if let Some(first) = first_summary {
                    if repeated || literal != first {
                        return Err(de::Error::custom("ambiguous repeated summary"));
                    }
                    repeated = true;
                } else {
                    let text =
                        serde_json::from_str::<String>(literal).map_err(de::Error::custom)?;
                    fields.insert(key.into(), Value::String(text));
                    first_summary = Some(literal);
                }
            } else {
                if fields.contains_key(key) {
                    return Err(de::Error::custom(format!("duplicate JSON field: {key}")));
                }
                fields.insert(key.into(), map.next_value::<UniqueValue>()?.0);
            }
        }
        if !repeated {
            return Err(de::Error::custom("no repeated summary"));
        }
        Ok(Value::Object(fields))
    }
}

#[cfg(test)]
mod tests {
    use super::super::{object, Recovery};
    use serde_json::json;

    #[test]
    fn preserves_the_unique_envelope_and_native_tool_arguments() {
        // Strings containing quotes, braces, and key-like text are data.
        let literal = r#""🔎 \"summary\": {a,b} \u003f""#;
        let action = json!([{"kind":"read","confidence":0.8,"suggested_tool":"read_file",
            "suggested_args":{"path":"src/a,b.rs","start_line":10,"end_line":23}}]);
        let clean = format!(r#"{{"summary":{literal},"suggestions":{action}}}"#);
        let duplicate =
            format!(r#" {{"summary":{literal},"suggestions":{action},"summary": {literal} }} "#);
        let (expected, recovery) = object(&clean).unwrap();
        assert_eq!(recovery, None);
        assert_eq!(
            object(&duplicate).unwrap(),
            (expected, Some(Recovery::RepeatedIdenticalSummary))
        );
        // The shared reader for Coder and native arguments is still strict.
        assert!(crate::structured_output::object(&duplicate).is_err());
    }

    #[test]
    fn rejects_conflicts_framing_combinations_and_duplicates_outside_summary() {
        let pair = r#""summary":"x","summary":"x""#;
        for raw in [
            r#"{"summary":"x","summary":"X"}"#.into(),
            r#"{"summary":"x","summary":"\u0078"}"#.into(),
            r#"{"summary":"x","summ\u0061ry":"x"}"#.into(),
            r#"{"summary":0,"summary":0}"#.into(),
            r#"{"summary":null,"summary":null}"#.into(),
            r#"{"summary":{"x":1},"summary":{"x":1}}"#.into(),
            format!(r#"{{{pair},"summary":"x"}}"#),
            format!(r#"{{{pair},"suggestions":[],"suggestions":[]}}"#),
            format!(r#"{{{pair},"quickest_check":"ls","quickest_check":"ls"}}"#),
            format!(r#"{{{pair},"suggestions":[{{"suggested_args":{{"path":"a","path":"a"}}}}]}}"#),
            format!(r#"{{{pair},"unknown":[{{"summary":"x","summary":"x"}}]}}"#),
            format!(r#"{{{pair},"scores":{{"correctness":1,"correctness":1}}}}"#),
            format!("{pair}}}"),
            format!("{{{pair}}} {{}}"),
            format!("prefix {{{pair}}}"),
            format!("{{{pair}}} trailing"),
            format!("[{{{pair}}}]"),
            format!("{{{pair}"),
            format!("{{{pair},}}"),
            format!("{{{pair}}}{}", " ".repeat(64 * 1024)),
        ] {
            assert!(object(&raw).is_err(), "unexpected recovery: {:.90}", raw);
        }
    }

    #[test]
    fn summary_recovery_does_not_make_invalid_or_unproven_actions_eligible() {
        for (suggestion, semantic_issue) in [
            (
                json!({"kind":"verify","confidence":0.99,"suggested_tool":"exec","suggested_args":{}}),
                true,
            ),
            (
                json!({"kind":"done","confidence":0.99,"suggested_tool":"done","suggested_args":{
                "summary":"all tests passed","completed_acceptance":["tests pass"],"remaining_acceptance":[],
                "acceptance_evidence":[{"criterion":"tests pass","command":"cargo test"}]}}),
                true,
            ),
            (
                json!({"kind":"read","confidence":0.2,"suggested_tool":"read_file","suggested_args":{"path":"a"}}),
                false,
            ),
        ] {
            let raw =
                format!(r#"{{"summary":"ready","suggestions":[{suggestion}],"summary":"ready"}}"#);
            assert!(crate::tui::suggestion::transport_issue_for_raw(&raw).is_none());
            let response = crate::observer_action::evaluate(crate::observer_action::Request {
                packet: json!({}),
                messages: vec![json!({"role":"user","content":"Fix the bug and run cargo test"})],
                test_command: None,
                lang: "en".into(),
                reason: "verification_failure".into(),
                response: Some(raw),
                final_summary: None,
                repair_native: false,
            });
            assert_eq!(
                response.format_recovery,
                Some(Recovery::RepeatedIdenticalSummary)
            );
            assert_eq!(response.issue.is_some(), semantic_issue);
            assert!(response.hint.is_none() && response.response_contract.is_none());
        }
    }
}
