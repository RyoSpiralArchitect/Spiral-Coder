//! Remove duplicated task data from prompt copies.
use serde_json::{json, Value};

pub(super) fn reference_human_task(view: &mut Value, messages: &[Value]) {
    let Some(task) = view.pointer_mut("/work_progress/original_human_task") else {
        return;
    };
    let Some(text) = task["text"].as_str() else {
        return;
    };
    let root = crate::task_origin::root_user_text(messages);
    if text.len() < 1024 || !root.starts_with(text) {
        return;
    }
    let prefix: String = text.chars().take(160).collect();
    // Origin metadata is stripped before transport. Count runtime and earlier
    // user turns too, since they share the same wire role as the task.
    let mut matching = messages
        .iter()
        .filter(|message| message["role"] == "user")
        .filter_map(|message| message["content"].as_str())
        .filter(|content| content.starts_with(&prefix));
    if matching.next() != Some(root) || matching.next().is_some() {
        return;
    }
    *task = json!({
        "source": "conversation_user_message",
        "starts_with": prefix,
        "total_chars": task["total_chars"],
        "instruction": "Read the full user message with this opening."
    });
}

pub(crate) fn remove_repeated_pending_items(view: &mut Value) {
    let Some(status) = view
        .get_mut("task_requirements")
        .and_then(Value::as_object_mut)
    else {
        return;
    };
    let Some(pending) = status.get("pending").and_then(Value::as_array) else {
        return;
    };
    if pending
        .iter()
        .filter_map(Value::as_str)
        .map(str::len)
        .sum::<usize>()
        < 512
    {
        return;
    }
    let Some(requirements) = status.get("requirements").and_then(Value::as_array) else {
        return;
    };
    let mut aggregate = pending.iter();
    for row in requirements {
        let (Some(id), Some(items)) =
            (row["requirement"]["id"].as_str(), row["pending"].as_array())
        else {
            return;
        };
        for item in items {
            let Some(item) = item.as_str() else {
                return;
            };
            let matching_item = aggregate
                .next()
                .and_then(Value::as_str)
                .and_then(|entry| entry.strip_prefix(id))
                .and_then(|entry| entry.strip_prefix(": "));
            if matching_item != Some(item) {
                return;
            }
        }
    }
    if aggregate.next().is_none() {
        status.remove("pending");
    }
}
