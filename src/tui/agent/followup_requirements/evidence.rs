use super::*;

pub(crate) fn followup_requirement_is_satisfied(
    requirement: &FollowupRequirement,
    reads: &BTreeMap<String, ReadObservation>,
    mutations: &BTreeMap<String, String>,
) -> bool {
    reads
        .get(requirement.file_path.as_str())
        .is_some_and(|read| read.body.contains(requirement.missing_literal.as_str()))
        || mutations
            .get(requirement.file_path.as_str())
            .is_some_and(|body| body.contains(requirement.missing_literal.as_str()))
}

pub(crate) fn has_successful_primary_source_mutation(mutations: &BTreeMap<String, String>) -> bool {
    mutations
        .keys()
        .any(|path| path.starts_with("src/") && path.ends_with(".rs"))
}

pub(crate) fn successful_literal_mutations(messages: &[Value]) -> BTreeMap<String, String> {
    let mut pending: BTreeMap<String, (String, String)> = BTreeMap::new();
    let mut out = BTreeMap::new();

    for msg in messages {
        match msg.get("role").and_then(|v| v.as_str()).unwrap_or("") {
            "assistant" => {
                let Some(tool_calls) = msg.get("tool_calls").and_then(|v| v.as_array()) else {
                    continue;
                };
                for tc in tool_calls {
                    let id = tc.get("id").and_then(|v| v.as_str()).unwrap_or("").trim();
                    let name = tc
                        .get("function")
                        .and_then(|v| v.get("name"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .trim();
                    if id.is_empty() || !matches!(name, "patch_file" | "write_file") {
                        continue;
                    }
                    let Some(args) = tc
                        .get("function")
                        .and_then(|v| v.get("arguments"))
                        .and_then(|v| v.as_str())
                        .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
                    else {
                        continue;
                    };
                    let Some(path) = args.get("path").and_then(|v| v.as_str()) else {
                        continue;
                    };
                    let payload = match name {
                        "patch_file" => args
                            .get("replace")
                            .or_else(|| args.get("json_value"))
                            .and_then(|v| v.as_str()),
                        "write_file" => args.get("content").and_then(|v| v.as_str()),
                        _ => None,
                    };
                    if let Some(payload) = payload {
                        pending.insert(id.to_string(), (path.to_string(), payload.to_string()));
                    }
                }
            }
            "tool" => {
                let tool_call_id = msg
                    .get("tool_call_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim();
                let Some((path, payload)) = pending.remove(tool_call_id) else {
                    continue;
                };
                let content = msg.get("content").and_then(|v| v.as_str()).unwrap_or("");
                if content.starts_with("OK:") {
                    out.insert(path, payload);
                }
            }
            _ => {}
        }
    }

    out
}

pub(crate) fn path_literals_in_text(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for token in text.split_whitespace() {
        let trimmed = token
            .trim_matches(|c: char| {
                (c.is_ascii_punctuation() && !matches!(c, '.' | '/' | '\\' | '_' | '-'))
                    || matches!(
                        c,
                        '「' | '」'
                            | '『'
                            | '』'
                            | '（'
                            | '）'
                            | '('
                            | ')'
                            | '['
                            | ']'
                            | '{'
                            | '}'
                            | '`'
                            | '"'
                            | '\''
                    )
            })
            .trim_end_matches(|c: char| matches!(c, '.' | ',' | ';' | ':' | '!' | '?'))
            .trim_matches(|c: char| {
                matches!(
                    c,
                    '「' | '」'
                        | '『'
                        | '』'
                        | '（'
                        | '）'
                        | '('
                        | ')'
                        | '['
                        | ']'
                        | '{'
                        | '}'
                        | '`'
                        | '"'
                        | '\''
                )
            });
        let has_path_sep = trimmed.contains('/') || trimmed.contains('\\');
        let has_extension = trimmed
            .split('/')
            .next_back()
            .is_some_and(|segment| segment.contains('.'));
        if (has_path_sep || has_extension) && !trimmed.is_empty() {
            let literal = trimmed.replace('\\', "/");
            if !out.contains(&literal) {
                out.push(literal);
            }
        }
    }
    out
}
