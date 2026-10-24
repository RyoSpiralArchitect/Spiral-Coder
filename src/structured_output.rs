//! Format-only recovery of one complete authored JSON object. No missing values
//! are invented, no candidate is selected from competing answers, and duplicate
//! keys (including nested ones) cannot silently use serde's last-wins behavior.
use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};
use serde_json::{Map, Value};
use std::fmt;

const MAX_BYTES: usize = 64 * 1024;

pub(crate) struct Object<'a> {
    pub value: Value,
    pub prefix: &'a str,
    pub suffix: &'a str,
}

pub(crate) fn object(raw: &str) -> Result<Object<'_>, String> {
    if raw.len() > MAX_BYTES {
        return Err("structured output exceeds 64 KiB".into());
    }
    let start = raw.find('{').ok_or("missing JSON object")?;
    let mut stream = serde_json::Deserializer::from_str(&raw[start..]).into_iter::<UniqueValue>();
    let value = stream
        .next()
        .ok_or("missing JSON object")?
        .map_err(|error| error.to_string())?
        .0;
    let prefix = &raw[..start];
    let suffix = &raw[start + stream.byte_offset()..];
    // Prose and Markdown fences may surround the object. JSON containers or
    // strings outside it are ambiguous; never extract an element from an array
    // or a nested/escaped answer, or choose among multiple complete objects.
    if [prefix, suffix]
        .iter()
        .any(|s| s.contains(['{', '}', '[', ']', '"']))
    {
        return Err("expected one unambiguous JSON object".into());
    }
    Ok(Object {
        value,
        prefix,
        suffix,
    })
}

struct UniqueValue(Value);
impl<'de> Deserialize<'de> for UniqueValue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct UniqueVisitor;
        impl<'de> Visitor<'de> for UniqueVisitor {
            type Value = UniqueValue;
            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("JSON with unique object keys")
            }
            fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
                let mut out = Map::new();
                while let Some(key) = map.next_key::<String>()? {
                    if out.contains_key(&key) {
                        return Err(de::Error::custom(format!("duplicate JSON field: {key}")));
                    }
                    out.insert(key, map.next_value::<UniqueValue>()?.0);
                }
                Ok(UniqueValue(Value::Object(out)))
            }
            fn visit_seq<S: SeqAccess<'de>>(self, mut seq: S) -> Result<Self::Value, S::Error> {
                let mut out = Vec::new();
                while let Some(value) = seq.next_element::<UniqueValue>()? {
                    out.push(value.0);
                }
                Ok(UniqueValue(Value::Array(out)))
            }
            fn visit_bool<E: de::Error>(self, value: bool) -> Result<Self::Value, E> {
                Ok(UniqueValue(Value::Bool(value)))
            }
            fn visit_i64<E: de::Error>(self, value: i64) -> Result<Self::Value, E> {
                Ok(UniqueValue(value.into()))
            }
            fn visit_u64<E: de::Error>(self, value: u64) -> Result<Self::Value, E> {
                Ok(UniqueValue(value.into()))
            }
            fn visit_f64<E: de::Error>(self, value: f64) -> Result<Self::Value, E> {
                Ok(UniqueValue(value.into()))
            }
            fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
                Ok(UniqueValue(value.into()))
            }
            fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
                Ok(UniqueValue(Value::Null))
            }
        }
        deserializer.deserialize_any(UniqueVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extracts_complete_object_without_changing_authored_values() {
        let value = json!({"text":"literal { braces }, escaped \"quote\", 🐈‍⬛", "items":[1,true,null,{"n":-0.5}]});
        for raw in [
            value.to_string(),
            format!("Here is the answer.\n```json\n{value}\n```\nEnd."),
        ] {
            assert_eq!(object(&raw).unwrap().value, value);
        }
    }

    #[test]
    fn ambiguity_duplicates_truncation_and_nested_answers_are_not_repaired() {
        for raw in [
            "{} {}",
            "[{}]",
            "{\"unfinished\":{}",
            "{\"x\":1,\"x\":2}",
            "{\"x\":[{\"tool\":\"read_file\",\"tool\":\"exec\"}]}",
            "\"{}\"",
            "{bad} {}",
            "{\"value\":1,}",
        ] {
            assert!(object(raw).is_err(), "{raw}");
        }
        assert!(object(&format!("{}{{}}", "x".repeat(MAX_BYTES))).is_err());
    }
}
