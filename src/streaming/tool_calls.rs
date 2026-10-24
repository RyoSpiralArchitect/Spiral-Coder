//! Keep streamed tool-call fragments separate until the response finishes.
use super::ToolCallData;
use anyhow::{bail, Result};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Default)]
pub(crate) struct ToolCallAccumulator {
    calls: BTreeMap<u64, ToolCallData>,
    encoding: CallEncoding,
    id_slots: BTreeMap<String, u64>,
}

#[derive(Default, PartialEq)]
enum CallEncoding {
    #[default]
    Unknown,
    Indexed,
    // Gemini can send multiple complete calls in separate unindexed deltas.
    // Their explicit IDs, not the one-element array position, identify them.
    ById,
}

impl ToolCallAccumulator {
    pub(crate) fn push(&mut self, delta: &Value) -> Result<()> {
        let Some(calls) = delta.as_array() else {
            return Ok(());
        };
        for call in calls {
            let id = call
                .get("id")
                .and_then(Value::as_str)
                .filter(|id| !id.is_empty());
            let index = self.slot(call, id, calls.len())?;
            if let Some(id) = id {
                if self
                    .calls
                    .iter()
                    .any(|(slot, prior)| *slot != index && prior.id == id)
                {
                    bail!("streamed tool-call id reused across indices");
                }
            }
            let pending = self.calls.entry(index).or_insert_with(|| ToolCallData {
                id: String::new(),
                name: String::new(),
                arguments: String::new(),
                thought_signature: None,
            });
            if let Some(id) = id {
                if !pending.id.is_empty() && pending.id != id {
                    bail!("streamed tool-call id changed at index {index}");
                }
                pending.id = id.to_string();
            }
            if let Some(name) = call.pointer("/function/name").and_then(Value::as_str) {
                // Compatible providers may repeat the complete name alongside
                // each argument fragment instead of sending it only once.
                if pending.name != name {
                    pending.name.push_str(name);
                }
            }
            if let Some(arguments) = call.pointer("/function/arguments").and_then(Value::as_str) {
                pending.arguments.push_str(arguments);
            }
            if let Some(signature) = super::provider_metadata::ThoughtSignature::from_call(call)? {
                if pending
                    .thought_signature
                    .as_ref()
                    .is_some_and(|prior| prior != &signature)
                {
                    bail!("streamed tool-call thought signature changed at index {index}");
                }
                pending.thought_signature = Some(signature);
            }
        }
        Ok(())
    }

    fn slot(&mut self, call: &Value, id: Option<&str>, count: usize) -> Result<u64> {
        if let Some(value) = call.get("index") {
            if self.encoding == CallEncoding::ById {
                bail!("mixed indexed and ID-only streamed tool calls");
            }
            self.encoding = CallEncoding::Indexed;
            return value
                .as_u64()
                .ok_or_else(|| anyhow::anyhow!("invalid streamed tool-call index"));
        }
        if self.encoding == CallEncoding::Unknown && id.is_some() {
            self.encoding = CallEncoding::ById;
        }
        if self.encoding == CallEncoding::ById {
            if let Some(id) = id {
                let next = self.id_slots.len() as u64;
                return Ok(*self.id_slots.entry(id.to_owned()).or_insert(next));
            }
        }
        // An anonymous continuation is safe only while exactly one call can
        // own it. Explicit indexed conflicts remain errors, never new calls.
        if count == 1 && self.calls.len() <= 1 {
            if self.encoding == CallEncoding::Unknown {
                self.encoding = CallEncoding::Indexed;
            }
            return Ok(self.calls.keys().next().copied().unwrap_or(0));
        }
        bail!("ambiguous streamed tool-call fragment without index or ID")
    }

    pub(crate) fn drain(&mut self) -> Vec<ToolCallData> {
        self.encoding = CallEncoding::Unknown;
        self.id_slots.clear();
        std::mem::take(&mut self.calls)
            .into_values()
            .filter(|call| !call.id.is_empty() && !call.name.is_empty())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn interleaved_parallel_calls_keep_distinct_ids_names_and_arguments() {
        let mut calls = ToolCallAccumulator::default();
        for delta in [
            json!([{"index":1,"id":"read-b","function":{"name":"read_","arguments":"{\"path\":\""}}, {"index":0,"id":"exec-a","function":{"name":"exec","arguments":"{\"command\":\""}}]),
            json!([{"index":0,"function":{"arguments":"cargo "}}, {"index":1,"function":{"name":"file","arguments":"src/lib.rs"}}]),
            json!([{"index":1,"function":{"arguments":"\"}"}}]),
            json!([{"index":0,"function":{"arguments":"test\"}"}}]),
        ] {
            calls.push(&delta).unwrap();
        }
        let ready = calls.drain();
        assert_eq!(
            ready.len(),
            2,
            "the governor must see two calls, not one merged call"
        );
        assert_eq!(
            (&*ready[0].id, &*ready[0].name, &*ready[0].arguments),
            ("exec-a", "exec", "{\"command\":\"cargo test\"}")
        );
        assert_eq!(
            (&*ready[1].id, &*ready[1].name, &*ready[1].arguments),
            ("read-b", "read_file", "{\"path\":\"src/lib.rs\"}")
        );
        assert!(
            calls.drain().is_empty(),
            "finish_reason then DONE must not duplicate calls"
        );
    }

    #[test]
    fn single_call_fragmentation_and_next_response_do_not_share_state() {
        let mut calls = ToolCallAccumulator::default();
        calls
            .push(&json!([{"id":"first","function":{"name":"patch_","arguments":"{\"path\":\""}}]))
            .unwrap();
        calls
            .push(&json!([{"function":{"name":"file","arguments":"a.rs\"}"}}]))
            .unwrap();
        calls
            .push(&json!([{"id":"first","function":{"name":"patch_file","arguments":""}}]))
            .unwrap();
        let first = calls.drain();
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].name, "patch_file");
        assert_eq!(first[0].arguments, "{\"path\":\"a.rs\"}");
        calls.push(&json!([{"index":0,"id":"next","function":{"name":"exec","arguments":"{\"command\":\"pwd\"}"}}])).unwrap();
        let next = calls.drain();
        assert_eq!(next[0].id, "next");
        assert_eq!(next[0].name, "exec");
        assert_eq!(next[0].arguments, "{\"command\":\"pwd\"}");
    }

    #[test]
    fn ambiguous_or_reused_indices_fail_before_a_merged_call_can_execute() {
        let mut calls = ToolCallAccumulator::default();
        calls
            .push(&json!([{"index":0,"id":"a"},{"index":1,"id":"b"}]))
            .unwrap();
        assert!(calls
            .push(&json!([{"function":{"arguments":"unattributed"}}]))
            .is_err());
        assert!(calls.push(&json!([{"index":0,"id":"other"}])).is_err());
    }

    #[test]
    fn unindexed_parallel_calls_use_explicit_ids_and_keep_arrival_order() {
        let mut calls = ToolCallAccumulator::default();
        calls.push(&json!([{"id":"z-first","function":{"name":"read_file","arguments":"{\"path\":\"a\"}"}}])).unwrap();
        calls.push(&json!([{"id":"a-second","function":{"name":"read_file","arguments":"{\"path\":\"b\"}"},"extra_content":{"google":{"thought_signature":"opaque-second"}}}])).unwrap();
        let ready = calls.drain();
        assert_eq!(ready.len(), 2);
        assert_eq!(ready[0].id, "z-first");
        assert_eq!(ready[0].arguments, r#"{"path":"a"}"#);
        assert_eq!(ready[1].id, "a-second");
        assert_eq!(ready[1].arguments, r#"{"path":"b"}"#);
        assert!(ready[0].can_rewrite());
        assert!(!ready[1].can_rewrite());
        assert!(calls.drain().is_empty());
        calls
            .push(&json!([{"index":0,"id":"next","function":{"name":"done","arguments":"{}"}}]))
            .unwrap();
        assert_eq!(calls.drain()[0].id, "next");
    }

    #[test]
    fn unindexed_fragments_require_an_id_after_parallel_calls_begin() {
        let mut calls = ToolCallAccumulator::default();
        calls
            .push(&json!([
                {"id":"a","function":{"name":"read_file","arguments":"{\"path\":\""}},
                {"id":"b","function":{"name":"exec","arguments":"{\"command\":\"pwd\"}"}}
            ]))
            .unwrap();
        calls
            .push(&json!([{"id":"a","function":{"arguments":"a.rs\"}"}}]))
            .unwrap();
        assert!(calls
            .push(&json!([{"function":{"arguments":"unattributed"}}]))
            .is_err());
        assert!(calls.push(&json!([{"index":0,"id":"a"}])).is_err());
        let ready = calls.drain();
        assert_eq!(ready[0].arguments, r#"{"path":"a.rs"}"#);
        assert_eq!(ready[1].arguments, r#"{"command":"pwd"}"#);
    }

    #[test]
    fn duplicate_ids_cannot_name_two_indexed_calls() {
        let mut calls = ToolCallAccumulator::default();
        assert!(calls
            .push(&json!([{"index":0,"id":"same"},{"index":1,"id":"same"}]))
            .is_err());
    }

    #[test]
    fn late_signatures_stay_with_their_parallel_call_and_are_not_logged() {
        let mut calls = ToolCallAccumulator::default();
        calls.push(&json!([
            {"index":1,"id":"b","function":{"name":"read_file","arguments":"{\"path\":\"b.rs\"}"}},
            {"index":0,"id":"a","function":{"name":"read_file","arguments":"{\"path\":\"a.rs\"}"}}
        ])).unwrap();
        let metadata =
            json!([{"index":1,"extra_content":{"google":{"thought_signature":"opaque-b"}}}]);
        calls.push(&metadata).unwrap();
        calls.push(&metadata).unwrap(); // Complete signatures may be repeated, not concatenated.
        let ready = calls.drain();
        assert!(ready[0].can_rewrite());
        assert!(!ready[1].can_rewrite());
        assert_eq!(
            ready[1].to_json(),
            json!({"id":"b","type":"function",
            "function":{"name":"read_file","arguments":"{\"path\":\"b.rs\"}"},
            "extra_content":{"google":{"thought_signature":"opaque-b"}}})
        );
        assert!(!format!("{ready:?}").contains("opaque-b"));
        calls
            .push(&json!([{"index":0,"id":"next","function":{"name":"done","arguments":"{}"}}]))
            .unwrap();
        assert!(calls.drain()[0].thought_signature.is_none());
    }

    #[test]
    fn conflicting_or_malformed_signatures_fail_without_exposing_the_value() {
        for value in [
            json!("different-private-value"),
            json!(null),
            json!(42),
            json!(""),
        ] {
            let mut calls = ToolCallAccumulator::default();
            calls.push(&json!([{"index":0,"extra_content":{"google":{"thought_signature":"original-private-value"}}}])).unwrap();
            let error = calls
                .push(&json!([{"index":0,"extra_content":{"google":{"thought_signature":value}}}]))
                .unwrap_err();
            assert!(!error.to_string().contains("private-value"));
        }
    }
}
