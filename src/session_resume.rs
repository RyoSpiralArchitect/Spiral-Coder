//! Recover a complete tool-call prefix before resuming an interrupted session.
use std::collections::BTreeSet;

use serde_json::Value;

struct InvalidTail {
    start: usize,
    reason: &'static str,
}

fn invalid_tail(messages: &[Value]) -> Option<InvalidTail> {
    let mut pending: Option<(usize, BTreeSet<&str>)> = None;
    for (idx, message) in messages.iter().enumerate() {
        let role = message.get("role").and_then(Value::as_str).unwrap_or("");
        match role {
            "assistant" => {
                if let Some((start, _)) = pending.as_ref() {
                    return Some(InvalidTail {
                        start: *start,
                        reason: "assistant message arrived before all pending tool results",
                    });
                }
                let calls = match message.get("tool_calls") {
                    None | Some(Value::Null) => continue,
                    Some(Value::Array(calls)) => calls,
                    Some(_) => {
                        return Some(InvalidTail {
                            start: idx,
                            reason: "assistant tool_calls was not an array",
                        });
                    }
                };
                let mut ids = BTreeSet::new();
                for call in calls {
                    let Some(id) = call
                        .get("id")
                        .and_then(Value::as_str)
                        .filter(|id| !id.trim().is_empty())
                    else {
                        return Some(InvalidTail {
                            start: idx,
                            reason: "assistant tool_call has a missing or empty id",
                        });
                    };
                    if !ids.insert(id) {
                        return Some(InvalidTail {
                            start: idx,
                            reason: "assistant tool_calls contain duplicate ids",
                        });
                    }
                }
                if !ids.is_empty() {
                    pending = Some((idx, ids));
                }
            }
            "tool" => {
                let Some((start, ids)) = pending.as_mut() else {
                    return Some(InvalidTail {
                        start: idx,
                        reason: "tool result appeared without a pending assistant tool_call",
                    });
                };
                let Some(id) = message.get("tool_call_id").and_then(Value::as_str) else {
                    return Some(InvalidTail {
                        start: *start,
                        reason: "tool result has no tool_call_id",
                    });
                };
                if !ids.remove(id) {
                    return Some(InvalidTail {
                        start: *start,
                        reason: "tool result id did not match a pending tool_call",
                    });
                }
                if ids.is_empty() {
                    pending = None;
                }
            }
            _ => {
                if let Some((start, _)) = pending.as_ref() {
                    return Some(InvalidTail {
                        start: *start,
                        reason: "non-tool message arrived before all pending tool results",
                    });
                }
            }
        }
    }
    pending.map(|(start, _)| InvalidTail {
        start,
        reason: "session ended with missing tool results",
    })
}

pub(super) fn repair(messages: &mut Vec<Value>) -> Option<String> {
    let InvalidTail { start, reason } = invalid_tail(messages)?;
    let removed = messages.len() - start;
    messages.truncate(start);
    Some(format!(
        "repaired session: truncated {removed} message(s) from index {start} ({reason})"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn calls(ids: &[&str]) -> Value {
        json!({"role":"assistant","tool_calls":ids.iter().map(|id| json!({
            "id":id,"function":{"name":"exec","arguments":"{}"}
        })).collect::<Vec<_>>()})
    }

    fn result(id: &str) -> Value {
        json!({"role":"tool","tool_call_id":id,"content":"observed output"})
    }

    fn prefix() -> Vec<Value> {
        vec![
            json!({"role":"system","content":"base"}),
            json!({"role":"user","content":"task"}),
            calls(&["complete"]),
            result("complete"),
        ]
    }

    #[test]
    fn invalid_results_roll_back_the_entire_pending_exchange_once() {
        for bad in [
            result("other"),
            result(""),
            json!({"role":"tool","content":"missing id"}),
            json!({"role":"tool","tool_call_id":7}),
            json!({"role":"assistant","content":"premature finish"}),
            calls(&["next"]),
            json!({"role":"user","content":"new request"}),
        ] {
            let expected = prefix();
            let mut messages = expected.clone();
            messages.extend([calls(&["a", "b"]), result("a"), bad]);
            assert!(repair(&mut messages).is_some());
            assert_eq!(messages, expected);
            assert_eq!(repair(&mut messages), None);
        }
    }

    #[test]
    fn malformed_calls_do_not_become_resumable_history() {
        for bad in [
            calls(&["", "valid"]),
            calls(&["   "]),
            calls(&["duplicate", "duplicate"]),
            json!({"role":"assistant","tool_calls":[{"function":{"name":"exec"}}]}),
            json!({"role":"assistant","tool_calls":[{"id":5}]}),
            json!({"role":"assistant","tool_calls":"invalid"}),
        ] {
            let expected = prefix();
            let mut messages = expected.clone();
            messages.extend([bad, result("valid")]);
            assert!(repair(&mut messages).is_some());
            assert_eq!(messages, expected);
            assert_eq!(repair(&mut messages), None);
        }
    }

    #[test]
    fn interrupted_and_duplicate_results_preserve_only_complete_exchanges() {
        for tail in [
            vec![calls(&["a"])],
            vec![calls(&["a", "b"]), result("a")],
            vec![calls(&["a", "b"]), result("a"), result("a")],
            vec![result("orphan")],
        ] {
            let expected = prefix();
            let mut messages = expected.clone();
            messages.extend(tail);
            assert!(repair(&mut messages).is_some());
            assert_eq!(messages, expected);
            assert_eq!(repair(&mut messages), None);
        }
    }

    #[test]
    fn valid_multi_tool_results_can_arrive_in_any_order() {
        let mut messages = prefix();
        messages.extend([
            calls(&["a", "b"]),
            result("b"),
            result("a"),
            json!({"role":"assistant","content":"done","tool_calls":null}),
            json!({"role":"assistant","content":"done","tool_calls":[]}),
        ]);
        let original = messages.clone();
        assert_eq!(repair(&mut messages), None);
        assert_eq!(messages, original);
    }

    #[test]
    fn extra_result_after_a_completed_exchange_retains_that_exchange() {
        let expected = prefix();
        let mut messages = expected.clone();
        messages.push(result("complete"));
        assert!(repair(&mut messages).is_some());
        assert_eq!(messages, expected);
        assert_eq!(repair(&mut messages), None);
    }
}
