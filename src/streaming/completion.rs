//! A terminal wire frame is not necessarily a complete authored response.
//! These statuses preserve the existing receipt shape and remain task charges.
pub(super) fn status(finish_reason: &str, saw_output: bool) -> &'static str {
    match finish_reason {
        "length" => "output_limited",
        "content_filter" => "output_filtered",
        reason if reason.starts_with("function_call_filter:") => "output_filtered",
        "" | "stop" | "tool_calls" if saw_output => "completed",
        "" | "stop" | "tool_calls" => "empty_response",
        _ => "provider_stopped",
    }
}

pub(super) fn empty_finish_note(finish_reason: &str, saw_output: bool) -> Option<String> {
    if saw_output
        || finish_reason.is_empty()
        || finish_reason == "stop"
        || finish_reason == "tool_calls"
    {
        return None;
    }
    Some(format!(
        "\n[provider] Empty response: finish_reason={finish_reason}. \
No response content was emitted. Keep the task incomplete; this supplies no \
executable action or successful verification.\n"
    ))
}
