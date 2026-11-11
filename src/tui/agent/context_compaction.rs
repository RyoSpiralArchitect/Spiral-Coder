//! Request-only context projection. Native messages remain the sole input to
//! recovery, authorization, evidence, persistence and completion gates.
use serde::Serialize;
use serde_json::Value;

mod history;
mod obligations;
#[cfg(test)]
mod tests;
mod text_pool;

pub(super) use obligations::remove_repeated_pending_items;
pub(crate) use text_pool::render;

const TARGET_BYTES: usize = 64 * 1024;

pub(super) fn render_coder_obligations(obligations: &impl Serialize, messages: &[Value]) -> String {
    let mut view = serde_json::to_value(obligations).expect("task projection serializes");
    obligations::reference_human_task(&mut view, messages);
    remove_repeated_pending_items(&mut view);
    render(&view)
}

#[derive(Debug, Serialize)]
pub(crate) struct Metrics {
    pub version: u8,
    pub role: &'static str,
    pub before_bytes: usize,
    pub after_bytes: usize,
    pub target_bytes: usize,
    pub compacted_exchanges: usize,
    pub compacted_messages: usize,
    pub omitted_history_entries: usize,
    pub protected_overflow: bool,
}

/// Byte counts use the same metadata-free message view as the provider. They
/// exclude tool schemas and are not tokenizer counts or provider billing.
fn wire_bytes(messages: &[Value]) -> usize {
    serde_json::to_vec(&crate::task_origin::provider_messages(messages))
        .expect("messages serialize")
        .len()
}

pub(crate) fn coder(messages: &[Value], test_cmd: Option<&str>) -> (Vec<Value>, Metrics) {
    let before_bytes = wire_bytes(messages);
    let mut metrics = Metrics {
        version: 1,
        role: "coder",
        before_bytes,
        after_bytes: before_bytes,
        target_bytes: TARGET_BYTES,
        compacted_exchanges: 0,
        compacted_messages: 0,
        omitted_history_entries: 0,
        protected_overflow: before_bytes > TARGET_BYTES,
    };
    if before_bytes <= TARGET_BYTES {
        return (messages.to_vec(), metrics);
    }
    let projected = history::project(messages, test_cmd, &mut metrics);
    let after_bytes = wire_bytes(&projected);
    if after_bytes >= before_bytes {
        metrics.compacted_exchanges = 0;
        metrics.compacted_messages = 0;
        metrics.omitted_history_entries = 0;
        return (messages.to_vec(), metrics);
    }
    metrics.after_bytes = after_bytes;
    metrics.protected_overflow = after_bytes > TARGET_BYTES;
    (projected, metrics)
}
