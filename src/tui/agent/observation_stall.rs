//! Bound identical observations without treating reading as repair evidence.
use crate::streaming::ToolCallData;
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Default)]
struct Observation {
    output: String,
    count: usize,
}

fn key(name: &str, args: &Value) -> Option<String> {
    (matches!(name, "read_file" | "search_files") && args.is_object())
        .then(|| format!("{name}:{}", args))
}

/// Cache annotations do not constitute new information. Pruned results cannot
/// establish that the same contents were seen twice.
fn native_observation<'a>(name: &str, args: &Value, text: &'a str) -> Option<&'a str> {
    if name == "search_files" {
        let header = text.lines().next()?;
        return ((header.starts_with("[search_files: '")
            || header.starts_with("[search_files] No matches for '"))
            && !header.contains(" [pruned "))
        .then_some(text);
    }
    let path = args["path"].as_str()?;
    let (header, body) = text.split_once('\n')?;
    let rest = header.strip_prefix(&format!("[{path}] ("))?;
    let (lines, rest) = rest.split_once(" lines, ")?;
    let (bytes, suffix) = rest.split_once(" bytes)")?;
    lines.parse::<usize>().ok()?;
    bytes.parse::<usize>().ok()?;
    if suffix.is_empty()
        || (crate::file_tools::read_range::matches_range_header(&args.to_string(), header)
            && !suffix.contains(" [pruned "))
    {
        return Some(text);
    }
    let marker = " [⚡ cached — unchanged since last read]";
    if suffix == marker {
        let original_header = header.strip_suffix(marker)?;
        return body
            .strip_prefix(original_header)
            .is_some_and(|rest| rest.starts_with('\n'))
            .then_some(body);
    }
    None
}

fn history(messages: &[Value]) -> BTreeMap<String, Observation> {
    let mut pending = BTreeMap::<&str, (&str, Value)>::new();
    let mut observations = BTreeMap::<String, Observation>::new();
    for message in crate::task_origin::current_task_messages(messages) {
        if message["role"] == "assistant" {
            for call in message["tool_calls"].as_array().into_iter().flatten() {
                if let (Some(id), Some(name), Some(args)) = (
                    call["id"].as_str().filter(|id| !id.is_empty()),
                    call["function"]["name"].as_str(),
                    call["function"]["arguments"]
                        .as_str()
                        .and_then(|s| serde_json::from_str::<Value>(s).ok()),
                ) {
                    pending.insert(id, (name, args));
                }
            }
        }
        if message["role"] != "tool" {
            continue;
        }
        let Some((name, args)) = message["tool_call_id"]
            .as_str()
            .and_then(|id| pending.remove(id))
        else {
            continue;
        };
        let text = message["content"].as_str().unwrap_or("");
        if matches!(name, "exec" | "patch_file" | "write_file" | "apply_diff")
            && crate::execution_evidence::file_edit_outcome(name, text)
                != Some(crate::execution_evidence::FileEditOutcome::Unchanged)
            && !text.trim_start().starts_with("Awaiting approval")
            && crate::execution_evidence::exec_may_have_run(text)
        {
            // Even a failed attempt can introduce new diagnostic information
            // or partial changes. A rejection cannot reset observation pressure.
            observations.clear();
            continue;
        }
        let Some(key) = key(name, &args) else {
            continue;
        };
        let Some(output) = native_observation(name, &args, text) else {
            // Refusals do not undo known reads. Actual errors/unknown results
            // invalidate comparison and allow a fresh diagnostic observation.
            if crate::execution_evidence::exec_may_have_run(text)
                && !text.trim_start().starts_with("Awaiting approval")
            {
                observations.remove(&key);
            }
            continue;
        };
        let previous = observations.entry(key).or_default();
        previous.count = if previous.output == output {
            previous.count + 1
        } else {
            1
        };
        previous.output = output.to_owned();
    }
    observations
}

pub(super) fn previously_observed(messages: &[Value], call: &ToolCallData) -> bool {
    let args = serde_json::from_str::<Value>(&call.arguments).unwrap_or_default();
    key(&call.name, &args).is_some_and(|key| history(messages).contains_key(&key))
}

pub(super) fn block(messages: &[Value], call: &ToolCallData) -> Option<String> {
    let args = serde_json::from_str::<Value>(&call.arguments).ok()?;
    let key = key(&call.name, &args)?;
    let history = history(messages);
    let observation = history.get(&key)?;
    if observation.count < 2 {
        return None;
    }
    Some(format!(
        "[Progress Gate]\nThe identical {} arguments returned the same native output {} times without an intervening edit or executed command. This repeated observation supplies no new evidence.\nName the specific missing fact and choose an unread range, a different targeted query, or a behavioral check that tests the repair hypothesis. If diagnosis is sufficient, make the smallest justified edit and verify it. If blocked, report the unresolved requirement honestly. Do not repeat this exact observation or claim that reading repairs the task. No edit is forced by this gate.",
        call.name, observation.count
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn call(args: Value) -> ToolCallData {
        ToolCallData {
            id: "next".into(),
            name: "read_file".into(),
            arguments: args.to_string(),
            thought_signature: None,
        }
    }
    fn tool(messages: &mut Vec<Value>, name: &str, args: Value, result: &str) {
        let id = format!("call{}", messages.len());
        messages.push(json!({"role":"assistant","tool_calls":[{"id":id,"function":{"name":name,"arguments":args.to_string()}}]}));
        messages.push(json!({"role":"tool","tool_call_id":id,"content":result}));
    }
    fn repeated() -> Vec<Value> {
        let mut messages = vec![crate::task_origin::user_message(
            "Repair src/a.rs",
            crate::task_origin::MessageOrigin::User,
        )];
        for _ in 0..2 {
            tool(
                &mut messages,
                "read_file",
                json!({"path":"src/a.rs","start_line":1,"end_line":20}),
                "[src/a.rs] (30 lines, 100 bytes) [range: lines 1-20; partial file]\nsame body",
            );
        }
        messages
    }

    #[test]
    fn unchanged_edits_preserve_observation_pressure_review_regression() {
        for name in ["write_file", "patch_file", "apply_diff"] {
            let mut messages = repeated();
            tool(
                &mut messages,
                name,
                json!({"path":"src/a.rs"}),
                "OK: unchanged 'src/a.rs'",
            );
            assert!(block(
                &messages,
                &call(json!({"path":"src/a.rs","start_line":1,"end_line":20}))
            )
            .is_some());
        }
    }

    #[test]
    fn identical_successful_ranges_block_but_new_range_and_arguments_do_not() {
        let messages = repeated();
        assert!(block(
            &messages,
            &call(json!({"end_line":20,"path":"src/a.rs","start_line":1}))
        )
        .is_some());
        for args in [
            json!({"path":"src/a.rs","start_line":21,"end_line":30}),
            json!({"path":"src/b.rs","start_line":1,"end_line":20}),
            json!({"path":"src/a.rs"}),
        ] {
            assert!(block(&messages, &call(args)).is_none());
        }
    }

    #[test]
    fn cache_wrapping_does_not_make_an_identical_full_read_new() {
        let mut messages = vec![];
        let args = json!({"path":"a"});
        tool(
            &mut messages,
            "read_file",
            args.clone(),
            "[a] (1 lines, 1 bytes)\nx",
        );
        tool(&mut messages, "read_file", args.clone(), "[a] (1 lines, 1 bytes) [⚡ cached — unchanged since last read]\n[a] (1 lines, 1 bytes)\nx");
        assert!(block(&messages, &call(args)).is_some());
    }

    #[test]
    fn changed_result_pruning_and_new_task_invalidate_old_pressure() {
        let args = json!({"path":"src/a.rs","start_line":1,"end_line":20});
        for result in [
            "[src/a.rs] (30 lines, 100 bytes) [range: lines 1-20; partial file]\nnew body",
            "[src/a.rs] (30 lines, 100 bytes) [range: lines 1-20; partial file] [pruned 21L]",
            "ERROR reading src/a.rs",
        ] {
            let mut messages = repeated();
            tool(&mut messages, "read_file", args.clone(), result);
            assert!(block(&messages, &call(args.clone())).is_none());
        }
        let mut messages = repeated();
        messages.push(crate::task_origin::user_message(
            "New task",
            crate::task_origin::MessageOrigin::User,
        ));
        assert!(block(&messages, &call(args)).is_none());
    }

    #[test]
    fn attempts_reset_but_refusals_and_runtime_handoffs_do_not() {
        let next = call(json!({"path":"src/a.rs","start_line":1,"end_line":20}));
        for (name, result, resets) in [
            ("exec", "OK (exit_code: 0)", true),
            ("exec", "FAILED (exit_code: 1)", true),
            ("patch_file", "ERROR: search not found", true),
            ("patch_file", "OK: patched 'a'", true),
            ("exec", "GOVERNOR BLOCKED\nOK (exit_code: 0)", false),
            ("patch_file", "REJECTED BY USER", false),
        ] {
            let mut messages = repeated();
            tool(
                &mut messages,
                name,
                json!({"command":"check","path":"a"}),
                result,
            );
            assert_eq!(
                block(&messages, &next).is_none(),
                resets,
                "{name}: {result}"
            );
        }
        let mut messages = repeated();
        messages.push(crate::task_origin::user_message(
            "Continue.",
            crate::task_origin::MessageOrigin::Runtime,
        ));
        assert!(block(&messages, &next).is_some());
    }

    #[test]
    fn only_correlated_native_results_count_including_searches() {
        let mut messages = vec![
            json!({"role":"assistant","content":"[search_files: 'x' — 0 match(es)]"}),
            json!({"role":"tool","tool_call_id":"missing","content":"[search_files: 'x' — 0 match(es)]"}),
        ];
        let next = ToolCallData {
            name: "search_files".into(),
            arguments: json!({"pattern":"x","dir":"src"}).to_string(),
            ..call(json!({}))
        };
        assert!(block(&messages, &next).is_none());
        for _ in 0..2 {
            tool(
                &mut messages,
                "search_files",
                json!({"pattern":"x","dir":"src"}),
                "[search_files: 'x' — 0 match(es)]\n",
            );
        }
        assert!(block(&messages, &next).is_some());
    }
}
