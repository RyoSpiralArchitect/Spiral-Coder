//! Read authored protocol fields from JSON using the existing field catalog.
//! Normalization repairs representation only; semantic validation still runs.
use super::{governor_contract, parse_plan_items, protocol_fields, ParsedBlockValue};
use serde_json::Value;
use std::collections::BTreeMap;

pub(super) fn parse(body: &str, tag: &str) -> Option<BTreeMap<String, ParsedBlockValue>> {
    let start = body.find('{')?;
    let prefix = &body[..start];
    // A JSON fixture inside a legacy field is part of that field, not a second
    // protocol block. Once JSON is selected, errors cannot fall back to mining
    // nested strings for a different set of fields.
    if prefix.trim_start().starts_with('<')
        || prefix
            .split_once('[')
            .is_some_and(|(key, _)| governor_contract::block_field(tag, key.trim()).is_some())
        || !protocol_fields::parse_tag_fields(prefix, tag).is_empty()
    {
        return None;
    }
    Some(fields(body, tag).unwrap_or_default())
}

fn fields(body: &str, tag: &str) -> Option<BTreeMap<String, ParsedBlockValue>> {
    let object = crate::structured_output::object(body).ok()?;
    let mut out = BTreeMap::new();
    for (key, value) in object.value.as_object()? {
        let Some(field) = governor_contract::block_field(tag, key) else {
            continue; // Extra metadata is not a protocol claim.
        };
        if out.contains_key(&field.key) {
            return None; // Canonical key + alias is also an ambiguous duplicate.
        }
        let parsed = match field.kind.as_deref() {
            Some("list") => ParsedBlockValue::List(match value {
                Value::String(text) => parse_plan_items(text),
                Value::Array(_) => strings(value)?,
                _ => return None,
            }),
            Some("positive_int") => {
                let number = match value {
                    Value::String(text) => text.trim().parse::<usize>().ok(),
                    Value::Number(n) => n.as_u64().and_then(|n| usize::try_from(n).ok()),
                    _ => None,
                }?;
                ParsedBlockValue::PositiveInt(number)
            }
            Some("tool_name") | Some("enum") => ParsedBlockValue::Canonical(
                governor_contract::canonical_field_value(tag, &field.key, value.as_str()?)
                    .unwrap_or_default(),
            ),
            _ => ParsedBlockValue::Text(match value {
                Value::String(text) => text.clone(),
                Value::Array(_) => strings(value)?.join("\n"),
                _ => return None,
            }),
        };
        out.insert(field.key.clone(), parsed);
    }
    Some(out)
}

fn strings(value: &Value) -> Option<Vec<String>> {
    value
        .as_array()?
        .iter()
        .map(|v| v.as_str().map(str::to_string))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn plan() -> Value {
        json!({"goal":"Repair runner.py", "steps":["Read runner.py", "Repair and verify runner.py"],
            "acceptance_criteria":["Original check passes"], "risks":["default regression"],
            "assumptions":["supplied tests define behavior"], "commentary":"extra metadata"})
    }

    #[test]
    fn wrapped_json_plan_preserves_authored_lists_and_resumes() {
        let value = plan();
        let text = format!("<plan>Here is the plan.\n```json\n{value}\n```\n</plan>");
        let parsed = super::super::parse_plan_block(&text).unwrap();
        super::super::validate_plan(&parsed).unwrap();
        assert_eq!(json!(parsed.steps), value["steps"]);
        assert_eq!(
            json!(parsed.acceptance_criteria),
            value["acceptance_criteria"]
        );
        let messages = vec![
            json!({"role":"user","content":"Repair runner.py"}),
            json!({"role":"assistant","content":text}),
        ];
        let resumed: Vec<Value> =
            serde_json::from_str(&serde_json::to_string(&messages).unwrap()).unwrap();
        assert_eq!(
            json!(super::super::plan_resume::latest_valid(&resumed, |p| {
                super::super::validate_plan(p).is_ok()
            })),
            json!(parsed)
        );
        assert_eq!(resumed[1]["content"], text);
    }

    #[test]
    fn typed_think_and_reflect_fields_use_existing_contract_without_inference() {
        let think = json!({"goal":"Read source", "step":"2", "tool":"read_file", "risk":"none", "doubt":"default branch", "next":"read runner.py", "verify":"inspect output", "extra":true});
        let parsed =
            super::super::parse_think_block(&format!("<think>```json\n{think}\n```</think>"))
                .unwrap();
        assert_eq!(parsed.step, 2);
        assert_eq!(parsed.tool, "read_file");
        assert_eq!(parsed.next, "read runner.py");
        let fields = parse(r#"{"goal_delta":"same","strategy_change":"adjust","wrong_assumption":"default behavior lost"}"#, "reflect").unwrap();
        assert_eq!(fields["strategy_change"].as_text(), Some("adjust"));
        assert!(!fields.contains_key("last_outcome"));
    }

    #[test]
    fn invalid_ambiguous_and_excessive_fields_still_fail() {
        let mut value = plan();
        value["acceptance_criteria"] = json!(["1", "2", "3", "4", "5"]);
        let parsed = super::super::parse_plan_block(&format!("<plan>{value}</plan>")).unwrap();
        assert_eq!(parsed.acceptance_criteria.len(), 5);
        assert!(super::super::validate_plan(&parsed).is_err());
        for body in [
            "{}",
            r#"{"goal":"one","goal":"two"}"#,
            r#"{"goal":false}"#,
            r#"{"acceptance":["one"],"acceptance_criteria":["two"]}"#,
            r#"{"goal":"<goal>nested</goal>","steps":false}"#,
            r#"{"goal":"unfinished"#,
            r#"{"step":"2 then 3"}"#,
            r#"{"step":true}"#,
            "{} {}",
        ] {
            let fields = parse(body, "plan").unwrap();
            assert!(fields.is_empty(), "{body}");
        }
        assert!(parse(
            "goal: Read fixture\nrisks: JSON {\"goal\":\"nested\"}",
            "plan"
        )
        .is_none());
        for body in [
            "<goal>Read {key}</goal><steps>Read then verify</steps>",
            "goal[\"Read {key}\"] steps[\"Read then verify\"]",
        ] {
            assert!(parse(body, "plan").is_none(), "{body}");
            let parsed = super::super::parse_plan_block(&format!("<plan>{body}</plan>")).unwrap();
            assert!(parsed.goal.contains("Read {key}"), "{}", parsed.goal);
        }
    }
}
