use super::{ObserverSuggestion, ObserverSuggestionKind};
use crate::tui::agent;
use serde_json::{json, Value};

// Derive names and argument types from the actual Coder tool definitions.
pub(crate) fn tool_schemas() -> Value {
    tool_schemas_for("")
}

pub(crate) fn tool_schemas_for(base_url: &str) -> Value {
    Value::Array(
        [
            agent::exec_tool_def(),
            agent::read_file_tool_def(),
            agent::write_file_tool_def(),
            agent::patch_file_tool_def(),
            agent::apply_diff_tool_def(),
            agent::search_files_tool_def(),
            agent::glob_tool_def(),
            agent::list_dir_tool_def(),
            agent::done_tool_def(base_url),
        ]
        .into_iter()
        .map(|definition| {
            json!({
                "name":definition["function"]["name"],
                "parameters":definition["function"]["parameters"],
            })
        })
        .collect(),
    )
}

pub(super) fn usable(suggestion: &ObserverSuggestion) -> bool {
    validation_issue(suggestion).is_none()
}

pub(super) fn validation_issue(suggestion: &ObserverSuggestion) -> Option<String> {
    use ObserverSuggestionKind::*;
    let Some(args) = suggestion.suggested_args.as_object() else {
        return Some("suggested_args must be a JSON object".into());
    };
    let Some(tool) = suggestion.suggested_tool.as_deref() else {
        return if matches!(suggestion.kind, Clarify | AbandonPath | Done) && args.is_empty() {
            None
        } else {
            Some(format!("suggested_tool is required for kind={}; no-tool clarify/abandon_path/done requires empty suggested_args", suggestion.kind.label()))
        };
    };
    let compatible = match suggestion.kind {
        Search => matches!(tool, "search_files" | "glob" | "list_dir"),
        Read => tool == "read_file",
        Edit => matches!(tool, "patch_file" | "apply_diff" | "write_file"),
        Verify => tool == "exec",
        Done => tool == "done",
        Clarify | AbandonPath => false,
    };
    if !compatible {
        return Some(format!(
            "suggested_tool={tool} is incompatible with kind={}",
            suggestion.kind.label()
        ));
    }
    let catalog = tool_schemas();
    let Some(schema) = catalog
        .as_array()
        .and_then(|schemas| schemas.iter().find(|schema| schema["name"] == tool))
    else {
        return Some(format!("suggested_tool={tool} is not in available_tools"));
    };
    let mut normalized = suggestion.suggested_args.clone();
    if tool == "done" {
        if let Some(rows) = normalized["acceptance_evidence"].as_array_mut() {
            for row in rows {
                if let Some((criterion, command)) = row.as_str().and_then(|s| s.split_once('|')) {
                    *row = json!({"criterion":criterion.trim(),"command":command.trim()});
                }
            }
        }
    }
    let patch_shape = tool != "patch_file" || {
        let structured = ["json_pointer", "json_value", "json_expected"]
            .iter()
            .any(|key| args.contains_key(*key));
        if structured {
            args.get("search").is_none()
                && args.get("replace").is_none()
                && args
                    .get("json_pointer")
                    .and_then(Value::as_str)
                    .is_some_and(|s| s.starts_with('/'))
                && args
                    .get("json_value")
                    .and_then(Value::as_str)
                    .is_some_and(|s| serde_json::from_str::<Value>(s).is_ok())
                && if args["json_pointer"]
                    .as_str()
                    .is_some_and(|s| s.ends_with("/-"))
                {
                    args.get("json_expected").is_none()
                } else {
                    args.get("json_expected")
                        .and_then(Value::as_str)
                        .is_some_and(|s| serde_json::from_str::<Value>(s).is_ok())
                }
        } else {
            args.get("search").is_some_and(Value::is_string)
                && args.get("replace").is_some_and(Value::is_string)
        }
    };
    let read_shape = tool != "read_file"
        || serde_json::from_value::<crate::file_tools::read_range::ReadRequest>(
            suggestion.suggested_args.clone(),
        )
        .is_ok_and(|request| !request.is_range() || request.bounds().is_ok());
    let exec_shape = tool != "exec"
        || if args.contains_key("check_id") {
            args.len() == 1
                && args["check_id"]
                    .as_str()
                    .is_some_and(|id| !id.trim().is_empty())
        } else {
            args.get("command")
                .and_then(Value::as_str)
                .is_some_and(|command| !command.trim().is_empty())
        };
    let valid = exec_shape
        && read_shape
        && patch_shape
        && matches_schema(&normalized, &schema["parameters"])
        && ["path", "command", "pattern", "search", "diff"]
            .iter()
            .all(|key| {
                args.get(*key)
                    .is_none_or(|value| value.as_str().is_some_and(|text| !text.trim().is_empty()))
            });
    (!valid).then(|| format!("suggested_args for {tool} must satisfy its available_tools schema, with nonempty path/command/pattern/search/diff fields"))
}

fn matches_schema(value: &Value, schema: &Value) -> bool {
    match schema["type"].as_str() {
        Some("string") => value.is_string(),
        Some("boolean") => value.is_boolean(),
        Some("number") => value.is_number(),
        Some("integer") => value.is_i64() || value.is_u64(),
        Some("array") => value.as_array().is_some_and(|items| {
            items
                .iter()
                .all(|item| matches_schema(item, &schema["items"]))
        }),
        Some("object") => value.as_object().is_some_and(|object| {
            schema["required"]
                .as_array()
                .into_iter()
                .flatten()
                .all(|key| key.as_str().is_some_and(|key| object.contains_key(key)))
                && object.iter().all(|(key, value)| {
                    schema["properties"]
                        .get(key)
                        .is_none_or(|property| matches_schema(value, property))
                })
        }),
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn observer_can_suggest_small_json_edits_and_flat_coder_evidence() {
        let catalog = tool_schemas_for("https://generativelanguage.googleapis.com/v1beta/openai");
        let done = catalog
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["name"] == "done")
            .unwrap();
        assert_eq!(
            done["parameters"]["properties"]["acceptance_evidence"]["items"]["type"],
            "string"
        );
        let make = |tool: &str, kind: &str, args: Value| {
            serde_json::from_value::<ObserverSuggestion>(json!({
                "kind":kind, "reason":"next bounded action", "confidence":0.9,
                "suggested_tool":tool,"suggested_args":args
            }))
            .unwrap()
        };
        let evidence = "acceptance 1 | printf 'Case  Sensitive' | grep Case";
        let done = make(
            "done",
            "done",
            json!({
                "summary":"verified", "completed_acceptance":["acceptance 1"],
                "remaining_acceptance":[], "acceptance_evidence":[evidence]
            }),
        );
        assert!(validation_issue(&done).is_none());
        assert_eq!(done.suggested_args["acceptance_evidence"][0], evidence);
        let mut edit = make(
            "patch_file",
            "edit",
            json!({
                "path":"replay.json","json_pointer":"/cases/-","json_value":"{\"id\":\"new\"}"
            }),
        );
        assert!(validation_issue(&edit).is_none());
        edit.suggested_args["json_expected"] = json!("null");
        assert!(
            validation_issue(&edit).is_some(),
            "append must omit expected"
        );
        edit.suggested_args
            .as_object_mut()
            .unwrap()
            .remove("json_expected");
        edit.suggested_args["json_pointer"] = json!("/cases/0");
        assert!(
            validation_issue(&edit).is_some(),
            "replacement requires expected"
        );
        edit.suggested_args["json_expected"] = json!("{\"id\":\"old\"}");
        assert!(validation_issue(&edit).is_none());
        edit.suggested_args["search"] = json!("whole file");
        assert!(validation_issue(&edit).is_some());
    }
}
