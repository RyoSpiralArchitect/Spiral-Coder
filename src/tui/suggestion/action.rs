use super::{ObserverSuggestion, ObserverSuggestionKind};
use crate::tui::agent;
use serde_json::{json, Value};

// Derive names and argument types from the actual Coder tool definitions.
pub(crate) fn tool_schemas() -> Value {
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
            agent::done_tool_def(),
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
    use ObserverSuggestionKind::*;
    let Some(args) = suggestion.suggested_args.as_object() else {
        return false;
    };
    let Some(tool) = suggestion.suggested_tool.as_deref() else {
        return matches!(suggestion.kind, Clarify | AbandonPath | Done) && args.is_empty();
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
        return false;
    }
    let catalog = tool_schemas();
    let Some(schema) = catalog
        .as_array()
        .and_then(|schemas| schemas.iter().find(|schema| schema["name"] == tool))
    else {
        return false;
    };
    matches_schema(&suggestion.suggested_args, &schema["parameters"])
        && ["path", "command", "pattern", "search", "diff"]
            .iter()
            .all(|key| {
                args.get(*key)
                    .is_none_or(|value| value.as_str().is_some_and(|text| !text.trim().is_empty()))
            })
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
