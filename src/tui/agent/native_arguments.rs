//! Reject malformed native transport data before it can become tool execution.
use crate::streaming::ToolCallData;
use serde_json::Value;

pub(super) fn block_invalid_turn(
    messages: &mut Vec<Value>,
    text: &str,
    calls: &[ToolCallData],
) -> Option<String> {
    let hint = calls.iter().find_map(rejection)?;
    messages.push(serde_json::json!({
        "role": "assistant", "content": text,
        "tool_calls": calls.iter().map(ToolCallData::to_json).collect::<Vec<_>>()
    }));
    for call in calls {
        messages.push(serde_json::json!({
            "role": "tool", "tool_call_id": call.id,
            "content": format!("GOVERNOR BLOCKED\n\n{hint}")
        }));
    }
    Some(hint)
}

pub(super) fn rejection(call: &ToolCallData) -> Option<String> {
    let issue = match serde_json::from_str::<Value>(&call.arguments) {
        Ok(Value::Object(_)) => return None,
        Ok(_) => "arguments must be a JSON object, not a scalar or array".to_string(),
        Err(error) => {
            let end = if error.is_eof() {
                " The arguments ended before the JSON value was complete, possibly at the output limit."
            } else {
                ""
            };
            format!("invalid JSON: {error}.{end}")
        }
    };
    Some(format!(
        "[Tool arguments] {}: {issue}\nNo tool was executed and no workspace failure was observed. Retry one native tool call with a complete JSON object matching its schema. For edits, prefer a small patch_file search/replace over rewriting a long file; split large edits across calls. Do not send a raw shell command as exec arguments. Preserve the existing task, checks and report obligations.",
        call.name
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(name: &str, arguments: &str) -> ToolCallData {
        ToolCallData {
            id: "native".into(),
            name: name.into(),
            arguments: arguments.into(),
            thought_signature: None,
        }
    }

    #[test]
    fn rejects_truncation_and_non_objects_without_guessing_arguments() {
        for name in [
            "exec",
            "write_file",
            "patch_file",
            "apply_diff",
            "read_file",
            "done",
        ] {
            for arguments in [
                "{\"path\":\"x\",\"content\":\"unfinished",
                "[]",
                "null",
                "7",
                "\"pwd\"",
                "printf danger > receipt.txt",
            ] {
                let hint = rejection(&call(name, arguments)).unwrap();
                assert!(hint.contains("[Tool arguments]"));
                assert!(hint.contains("No tool was executed"));
                assert!(
                    !hint.contains("danger"),
                    "Do not repeat unparsed payload as an executable suggestion"
                );
            }
        }
        assert!(rejection(&call("write_file", "{\"content\":\"unfinished"))
            .unwrap()
            .contains("output limit"));
    }

    #[test]
    fn valid_objects_and_literal_source_escapes_are_left_unchanged() {
        for arguments in [
            "{}",
            "{\"command\":\"printf test\"}",
            r#"{"path":"a.py","search":"print(\"\\n\")","replace":"print(\"\\t\")"}"#,
        ] {
            assert!(rejection(&call("patch_file", arguments)).is_none());
        }
    }

    #[test]
    fn a_malformed_turn_preserves_every_call_and_matches_every_refusal() {
        let valid = call("read_file", "{\"path\":\"note.txt\"}");
        let mut bad = call("exec", "printf unsafe");
        bad.id = "bad".into();
        bad.thought_signature = crate::streaming::provider_metadata::ThoughtSignature::from_call(
            &serde_json::json!({"extra_content":{"google":{"thought_signature":"opaque-marker"}}}),
        )
        .unwrap();
        let calls = [valid, bad];
        let mut messages = Vec::new();
        assert!(block_invalid_turn(&mut messages, "response", &calls).is_some());
        assert_eq!(
            messages[0]["tool_calls"],
            serde_json::json!(calls.iter().map(ToolCallData::to_json).collect::<Vec<_>>())
        );
        assert_eq!(messages.len(), 3);
        for (index, call) in calls.iter().enumerate() {
            assert_eq!(messages[index + 1]["tool_call_id"], call.id);
            assert!(messages[index + 1]["content"]
                .as_str()
                .unwrap()
                .starts_with("GOVERNOR BLOCKED"));
        }
    }
}
