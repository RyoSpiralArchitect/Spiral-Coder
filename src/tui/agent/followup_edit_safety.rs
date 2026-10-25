//! Follow-up pressure cannot replace diagnosis or turn display text into edits.
use super::super::RecoveryStage;
use crate::streaming::ToolCallData;
use serde_json::Value;
use std::collections::BTreeMap;

pub(super) struct ReadObservation {
    pub(super) body: String,
    pub(super) snapshot: Option<String>,
}

pub(super) fn preserve_diagnosis_or_closeout(
    call: &ToolCallData,
    stage: Option<RecoveryStage>,
) -> bool {
    if stage == Some(RecoveryStage::Diagnose) {
        return true;
    }
    let args: Value = serde_json::from_str(&call.arguments).unwrap_or_default();
    (call.name == "read_file"
        && (args.get("start_line").is_some() || args.get("end_line").is_some()))
        || (call.name == "done"
            && args["remaining_acceptance"]
                .as_array()
                .is_some_and(|remaining| !remaining.is_empty()))
}

pub(super) fn is_authored_followup_edit(call: &ToolCallData, path: &str) -> bool {
    matches!(
        call.name.as_str(),
        "patch_file" | "apply_diff" | "write_file"
    ) && serde_json::from_str::<Value>(&call.arguments)
        .ok()
        .is_some_and(|args| args["path"].as_str() == Some(path))
}

pub(super) fn successful_read_contents(messages: &[Value]) -> BTreeMap<String, ReadObservation> {
    let mut pending = BTreeMap::new();
    let mut reads = BTreeMap::new();
    for message in messages {
        if message["role"] == "assistant" {
            for call in message["tool_calls"].as_array().into_iter().flatten() {
                if let (Some(id), Some("read_file"), Some(arguments)) = (
                    call["id"].as_str().filter(|id| !id.is_empty()),
                    call["function"]["name"].as_str(),
                    call["function"]["arguments"].as_str(),
                ) {
                    pending.insert(id, arguments);
                }
            }
        }
        if message["role"] != "tool" {
            continue;
        }
        let Some(arguments) = message["tool_call_id"]
            .as_str()
            .and_then(|id| pending.remove(id))
        else {
            continue;
        };
        let args: Value = serde_json::from_str(arguments).unwrap_or_default();
        let Some(path) = args["path"].as_str().filter(|path| !path.is_empty()) else {
            continue;
        };
        if let Some(read) = observation(path, arguments, message["content"].as_str().unwrap_or(""))
        {
            reads.insert(path.to_owned(), read);
        }
    }
    reads
}

fn observation(path: &str, arguments: &str, output: &str) -> Option<ReadObservation> {
    let (header, mut body) = output.split_once('\n').unwrap_or((output, ""));
    let (lines, bytes, suffix) = header_counts(path, header)?;
    let pruned = suffix
        .strip_prefix(" [pruned ")
        .and_then(|s| s.strip_suffix("L]"))
        .is_some_and(|count| count.parse::<usize>().is_ok());
    let ranged = crate::file_tools::read_range::matches_range_header(arguments, header);
    let cached = suffix == " [⚡ cached — unchanged since last read]";
    if !suffix.is_empty() && !pruned && !ranged && !cached {
        return None;
    }
    if cached {
        // Native cache output wraps the original read, including its header.
        let (inner_header, inner_body) = body.split_once('\n')?;
        if header_counts(path, inner_header)? != (lines, bytes, "") {
            return None;
        }
        body = inner_body;
    }
    let snapshot = (!pruned && !ranged && bytes > 0)
        .then(|| complete_body(path, body, lines, bytes))
        .flatten()
        .map(str::to_owned);
    Some(ReadObservation {
        body: if pruned {
            String::new()
        } else {
            body.to_owned()
        },
        snapshot,
    })
}

fn header_counts<'a>(path: &str, header: &'a str) -> Option<(usize, usize, &'a str)> {
    let header = header.strip_prefix(&format!("[{path}] ("))?;
    let (lines, header) = header.split_once(" lines, ")?;
    let (bytes, suffix) = header.split_once(" bytes)")?;
    Some((lines.parse().ok()?, bytes.parse().ok()?, suffix))
}

fn complete_body<'a>(path: &str, body: &'a str, lines: usize, bytes: usize) -> Option<&'a str> {
    let exact = body.get(..bytes)?;
    // Native full reads are bounded to 8000 characters. Byte/line counts bind
    // the snapshot to its header without banning marker-like user file text.
    if exact.chars().count() > 8000 || exact.lines().count() != lines {
        return None;
    }
    let suffix = &body[bytes..];
    let json_guidance = path.ends_with(".json")
        && suffix.starts_with("\n[JSON append guidance — diagnostic only]\n")
        && serde_json::from_str::<Value>(exact).is_ok();
    (suffix.is_empty() || json_guidance).then_some(exact)
}
