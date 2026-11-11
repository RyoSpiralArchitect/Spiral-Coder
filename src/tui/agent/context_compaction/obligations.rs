//! Remove duplicated task data from prompt copies.
use serde_json::{json, Value};

pub(super) fn reference_human_task(view: &mut Value, root: &str) {
    let task = &mut view["work_progress"]["original_human_task"];
    let Some(text) = task["text"].as_str() else {
        return;
    };
    if text.len() >= 1024 && root.starts_with(text) {
        let prefix: String = text.chars().take(160).collect();
        *task = json!({
            "source": "conversation_user_message",
            "starts_with": prefix,
            "total_chars": task["total_chars"]
        });
    }
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
