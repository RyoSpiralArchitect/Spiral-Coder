//! Reuse a retained full read only after comparing it with the current file.
//! This creates no tool receipt and cannot recover pruned or truncated context.
use serde_json::Value;
use std::collections::HashMap;

pub(super) fn matching_snapshot(
    messages: &[Value],
    path: &str,
    base: Option<&str>,
) -> Option<String> {
    let base = base?;
    let root = std::path::Path::new(base).canonicalize().ok()?;
    let target = crate::file_tools::resolve_safe_path(path, Some(base))
        .ok()?
        .canonicalize()
        .ok()?;
    if !target.starts_with(&root) {
        return None;
    }
    let mut pending = HashMap::new();
    let mut retained = None;
    for message in crate::task_origin::current_task_messages(messages) {
        if message["role"] == "assistant" {
            for call in message["tool_calls"].as_array().into_iter().flatten() {
                let Some(id) = call["id"].as_str() else {
                    continue;
                };
                if call["function"]["name"] != "read_file" {
                    continue;
                }
                let Some(args) = call["function"]["arguments"]
                    .as_str()
                    .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
                else {
                    continue;
                };
                let Some(read_path) = args["path"].as_str() else {
                    continue;
                };
                let resolved = crate::file_tools::resolve_safe_path(read_path, Some(base))
                    .ok()
                    .and_then(|path| path.canonicalize().ok());
                if resolved.as_ref() == Some(&target) {
                    pending.insert(id, read_path.to_string());
                }
            }
        } else if message["role"] == "tool" {
            let Some(read_path) = message["tool_call_id"]
                .as_str()
                .and_then(|id| pending.remove(id))
            else {
                continue;
            };
            // A later failed or pruned read invalidates the older snapshot too.
            retained = message["content"].as_str().map(|text| (read_path, text));
        }
    }
    let (read_path, retained) = retained?;
    let (fresh, error) = crate::file_tools::tool_read_file(&read_path, Some(base));
    if error || fresh.contains("\n[…truncated — ") {
        return None;
    }
    let cached_prefix = format!(
        "{} [⚡ cached — unchanged since last read]\n",
        fresh.lines().next().unwrap_or("")
    );
    let retained = retained.strip_prefix(&cached_prefix).unwrap_or(retained);
    (retained == fresh).then_some(fresh)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn history(base: &str, path: &str) -> Vec<Value> {
        let (text, error) = crate::file_tools::tool_read_file(path, Some(base));
        assert!(!error);
        vec![
            json!({"role":"user","content":"Repair the source file."}),
            json!({"role":"assistant","tool_calls":[{"id":"read","function":{
                "name":"read_file","arguments":json!({"path":path}).to_string()
            }}]}),
            json!({"role":"tool","tool_call_id":"read","content":text}),
            crate::task_origin::user_message(
                "Continue",
                crate::task_origin::MessageOrigin::Runtime,
            ),
        ]
    }

    #[test]
    fn continuation_reuses_only_identical_full_native_context() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().to_str().unwrap();
        let source = dir.path().join("greet.py");
        std::fs::write(&source, "print('old')\n").unwrap();
        let mut messages = history(base, "./greet.py");
        let snapshot = matching_snapshot(&messages, "greet.py", Some(base)).unwrap();
        assert_eq!(snapshot, messages[2]["content"]);
        messages[2]["content"] = json!(format!(
            "{} [⚡ cached — unchanged since last read]\n{snapshot}",
            snapshot.lines().next().unwrap()
        ));
        assert_eq!(
            matching_snapshot(&messages, "greet.py", Some(base)),
            Some(snapshot)
        );
        // Equal size and line count cannot hide changed content.
        std::fs::write(&source, "print('new')\n").unwrap();
        assert_eq!(matching_snapshot(&messages, "greet.py", Some(base)), None);
    }

    #[test]
    fn missing_pruned_truncated_or_previous_task_reads_do_not_authorize_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().to_str().unwrap();
        let source = dir.path().join("source.txt");
        std::fs::write(&source, "known contents").unwrap();
        let messages = history(base, "source.txt");
        let mut pruned = messages.clone();
        pruned[2]["content"] = json!("[source.txt] (1 lines, 14 bytes)");
        assert_eq!(matching_snapshot(&pruned, "source.txt", Some(base)), None);
        let mut orphan = messages.clone();
        orphan.remove(1);
        assert_eq!(matching_snapshot(&orphan, "source.txt", Some(base)), None);
        let mut new_task = messages;
        new_task.push(json!({"role":"user","content":"Start another task."}));
        assert_eq!(matching_snapshot(&new_task, "source.txt", Some(base)), None);
        std::fs::write(&source, "x".repeat(9000)).unwrap();
        let truncated = history(base, "source.txt");
        assert_eq!(
            matching_snapshot(&truncated, "source.txt", Some(base)),
            None
        );
        std::fs::remove_file(source).unwrap();
        assert_eq!(
            matching_snapshot(&truncated, "source.txt", Some(base)),
            None
        );
    }
}
