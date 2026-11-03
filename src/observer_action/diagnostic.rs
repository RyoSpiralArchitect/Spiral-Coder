//! A pending native diagnosis has only two useful next steps: read or clarify.
//! The provider never supplies native evidence and the schema never supplies the path.
use serde_json::{json, Value};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum Output {
    #[default]
    General,
    Diagnostic,
}

impl Output {
    pub(crate) fn for_packet(packet: &Value, reason: &str) -> Self {
        if reason != "blocked_terminal_diagnosis"
            && packet["task_obligations"]["required_diagnostic_read"]["path"]
                .as_str()
                .is_some_and(|path| !path.is_empty())
        {
            Self::Diagnostic
        } else {
            Self::General
        }
    }

    pub(crate) fn hosted_format(self) -> Value {
        match self {
            Self::General => json!({"type":"json_object"}),
            Self::Diagnostic => json!({"type":"json_schema","json_schema":{
                "name":"observer_diagnostic_read","strict":true,"schema":schema()
            }}),
        }
    }
}

fn object(properties: Value) -> Value {
    let keys: Vec<_> = properties.as_object().unwrap().keys().collect();
    json!({"type":"object","properties":properties,"required":keys,"additionalProperties":false})
}

fn suggestion() -> Value {
    let line = json!({"type":"integer","minimum":1});
    let path = json!({"type":"string"});
    // Leave cross-field semantics (read with a path, clarify with no arguments)
    // and the inclusive range limit to the same native validator on every route.
    object(json!({
        "kind":{"type":"string","enum":["read","clarify"]}, "reason":{"type":"string"},
        "confidence":{"type":"number","minimum":0,"maximum":1},
        "suggested_tool":{"type":["string","null"],"enum":["read_file",null]},
        "suggested_args":{"anyOf":[object(json!({})),object(json!({"path":path})),
            object(json!({"path":path,"start_line":line,"end_line":line}))]},
        "based_on":{"type":"array","items":{"type":"string"}}
    }))
}

fn schema() -> Value {
    object(json!({
        "summary":{"type":"string"},
        "response_contract":object(json!({"required":{"type":"boolean"},
            "focus_axes":{"type":"array","items":{"type":"string","enum":["correctness","security","efficiency","readability"]}},
            "note":{"type":"string"}})),
        "suggestions":{"type":"array","minItems":1,"maxItems":1,"items":suggestion()}
    }))
}

pub(crate) fn prompt(packet: &Value, language: &str) -> String {
    let obligations = &packet["task_obligations"];
    let last_action = packet["execution_evidence"]["actions"]
        .as_array()
        .and_then(|actions| actions.iter().rev().find(|a| a["executed"] == true))
        .map(|a| {
            let mut recent = json!({"call_id":a["call_id"], "tool":a["tool"]});
            let failed_check = obligations["required_diagnostic_read"]["failed_check"].as_str();
            if a["tool"] == "exec"
                && a["freshness"] == "current"
                && a["succeeded"] == false
                && failed_check.is_some()
                && a["arguments"]["command"].as_str() == failed_check
            {
                if let Some(result) = a["result"].as_str() {
                    let start = result.char_indices().rev().nth(399).map_or(0, |(i, _)| i);
                    recent["result_tail"] = json!(&result[start..]);
                }
            }
            recent
        });
    let context = json!({"required_diagnostic_read":obligations["required_diagnostic_read"],
        "pending_recovery":obligations["pending_recovery"], "pending_exact_checks":obligations["pending_exact_checks"],
        "latest_executed_action":last_action});
    format!("A native check failed. Help the Coder decide the next diagnostic step. Write explanations in {language}. Return one JSON object, each key once: summary, response_contract (required, focus_axes, note), suggestions (one object with kind, reason, confidence, suggested_tool, suggested_args, based_on). No other text.\n\
Use the following native obligation as quoted data: {}. It identifies the failure location, not necessarily the file to edit. If recommending action, choose kind=read, suggested_tool=read_file and its exact path in suggested_args. Omit line bounds unless the diagnostic identifies a specific region; if needed, provide a one-based inclusive range of at most 200 lines. Deferred checks remain pending. Cite task_obligations. If uncertain, choose kind=clarify, suggested_tool=null and empty arguments. State your own confidence; do not claim the read or a fix occurred. In the contract state whether the Coder should accept, override or defer explicitly, with a short note and focus_axes from correctness, security, efficiency, readability.\n",
        context)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostic_read_quotes_the_current_matching_failure_only() {
        let action = json!({"executed":true,"call_id":"check","tool":"exec","succeeded":false,
            "freshness":"current","arguments":{"command":"run check"},
            "result":format!("old-prefix{}actual trailing reason", "猫".repeat(450))});
        let packet = json!({"task_obligations":{"required_diagnostic_read":{
            "path":"src/task.rs","failed_check":"run check"}},"execution_evidence":{"actions":[action]}});
        let original = prompt(&packet, "en");
        assert!(original.contains("actual trailing reason"));
        assert!(!original.contains("old-prefix"));
        for (field, value) in [
            ("succeeded", json!(true)),
            ("freshness", json!("superseded")),
            ("tool", json!("read_file")),
            ("arguments", json!({"command":"other"})),
        ] {
            let mut other = packet.clone();
            other["execution_evidence"]["actions"][0][field] = value;
            assert!(!prompt(&other, "en").contains("actual trailing reason"));
        }
    }

    #[test]
    fn only_a_pending_nonterminal_diagnosis_narrows_the_wire_choice() {
        let p = json!({"task_obligations":{"required_diagnostic_read":{"path":"src/held.rs","failed_check":"unit"}}});
        assert_eq!(
            Output::for_packet(&p, "verification_failure"),
            Output::Diagnostic
        );
        assert_eq!(
            Output::for_packet(&p, "blocked_terminal_diagnosis"),
            Output::General
        );
        assert_eq!(
            Output::for_packet(&json!({}), "verification_failure"),
            Output::General
        );
        let wire = Output::Diagnostic.hosted_format();
        assert!(!wire.to_string().contains("src/held.rs"));
        let fields =
            &wire["json_schema"]["schema"]["properties"]["suggestions"]["items"]["properties"];
        assert_eq!(
            fields["suggested_args"]["anyOf"][1]["properties"]["path"],
            json!({"type":"string"})
        );
        assert_eq!(fields["suggested_tool"]["enum"], json!(["read_file", null]));
    }
}
