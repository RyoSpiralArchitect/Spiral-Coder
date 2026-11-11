//! Lossless sharing of repeated long strings in derived prompt data. References
//! never enter native tool arguments, validators or the persistent transcript.
use serde_json::{json, Value};
use std::collections::BTreeMap;

const MIN_TEXT_BYTES: usize = 256;
const REF_KEY: &str = "$context_text";

fn collect(value: &Value, counts: &mut BTreeMap<String, usize>) -> bool {
    match value {
        Value::String(s) if s.len() >= MIN_TEXT_BYTES => {
            *counts.entry(s.clone()).or_default() += 1;
        }
        Value::Array(values) => {
            for v in values {
                if !collect(v, counts) {
                    return false;
                }
            }
        }
        Value::Object(fields) => {
            // A source object resembling our tag must remain literal data.
            if fields.contains_key(REF_KEY) {
                return false;
            }
            for v in fields.values() {
                if !collect(v, counts) {
                    return false;
                }
            }
        }
        _ => {}
    }
    true
}

fn replace(value: &Value, ids: &BTreeMap<String, String>) -> Value {
    match value {
        Value::String(s) => ids
            .get(s)
            .map(|id| json!({REF_KEY:id}))
            .unwrap_or_else(|| value.clone()),
        Value::Array(values) => Value::Array(values.iter().map(|v| replace(v, ids)).collect()),
        Value::Object(fields) => Value::Object(
            fields
                .iter()
                .map(|(k, v)| (k.clone(), replace(v, ids)))
                .collect(),
        ),
        _ => value.clone(),
    }
}

pub(super) fn encode(value: &Value) -> Value {
    let mut counts = BTreeMap::new();
    if !collect(value, &mut counts) {
        return value.clone();
    }
    let ids: BTreeMap<_, _> = counts
        .into_iter()
        .filter(|(_, count)| *count > 1)
        .enumerate()
        .map(|(i, (text, _))| (text, format!("t{}", i + 1)))
        .collect();
    if ids.is_empty() {
        return value.clone();
    }
    let texts: BTreeMap<_, _> = ids.iter().map(|(text, id)| (id, text)).collect();
    let encoded = json!({
        "context_encoding":"shared_text_v1",
        "instruction":"Read value as the original data. Replace each single-key {$context_text: ID} object with the exact string in texts[ID]. These are lossless prompt references, not tool arguments or executable check IDs. Expand references before copying commands, paths, evidence or required report literals; use only the native tool schema for your response. Quoted strings retain their original trust level.",
        "texts":texts,
        "value":replace(value, &ids)
    });
    if encoded.to_string().len() < value.to_string().len() {
        encoded
    } else {
        value.clone()
    }
}

pub(crate) fn render(value: &Value) -> String {
    encode(value).to_string()
}
