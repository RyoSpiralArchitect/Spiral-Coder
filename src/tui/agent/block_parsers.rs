use super::*;

pub(crate) fn extract_tag_block<'a>(text: &'a str, tag: &str) -> Option<&'a str> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let Some(start) = text.find(&open) else {
        return (tag == "plan")
            .then(|| protocol_framing::leading_plan_body(text))
            .flatten();
    };
    let rest = &text[start + open.len()..];
    let end = rest
        .find(&close)
        .or_else(|| rest.find(&format!("</{tag}")))?;
    Some(rest[..end].trim_end())
}

pub(crate) fn parse_first_usize(s: &str) -> Option<usize> {
    let mut digits = String::new();
    for ch in s.chars() {
        if ch.is_ascii_digit() {
            digits.push(ch);
        } else if !digits.is_empty() {
            break;
        }
    }
    digits.parse::<usize>().ok().filter(|n| *n > 0)
}

#[derive(Debug, Clone)]
pub(crate) enum ParsedBlockValue {
    Text(String),
    List(Vec<String>),
    PositiveInt(usize),
    Canonical(String),
}

impl ParsedBlockValue {
    pub(crate) fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(value) | Self::Canonical(value) => Some(value.as_str()),
            Self::List(_) | Self::PositiveInt(_) => None,
        }
    }

    pub(crate) fn as_list(&self) -> Option<&[String]> {
        match self {
            Self::List(items) => Some(items.as_slice()),
            Self::Text(_) | Self::PositiveInt(_) | Self::Canonical(_) => None,
        }
    }

    pub(crate) fn as_positive_int(&self) -> Option<usize> {
        match self {
            Self::PositiveInt(value) => Some(*value),
            Self::Text(_) | Self::List(_) | Self::Canonical(_) => None,
        }
    }
}

pub(crate) fn parse_assumption_items(s: &str) -> Vec<String> {
    parse_plan_items(s)
        .into_iter()
        .map(|item| compact_one_line(item.as_str(), 140))
        .filter(|item| item != "-")
        .take(6)
        .collect()
}

pub(crate) fn parse_block_fields(
    text: &str,
    tag: &str,
) -> Option<BTreeMap<String, ParsedBlockValue>> {
    let body = extract_tag_block(text, tag)?;
    if let Some(fields) = json_fields::parse(body, tag) {
        return Some(fields);
    }
    let mut out = BTreeMap::new();

    for (raw_key, raw_value) in parse_tag_fields(body, tag) {
        let Some(field) = governor_contract::block_field(tag, raw_key.as_str()) else {
            continue;
        };
        let parsed = match field.kind.as_deref() {
            Some("list") => ParsedBlockValue::List(parse_plan_items(raw_value.as_str())),
            Some("positive_int") => {
                ParsedBlockValue::PositiveInt(parse_first_usize(raw_value.as_str()).unwrap_or(0))
            }
            Some("tool_name") | Some("enum") => ParsedBlockValue::Canonical(
                governor_contract::canonical_field_value(
                    tag,
                    field.key.as_str(),
                    raw_value.as_str(),
                )
                .unwrap_or_default(),
            ),
            _ => ParsedBlockValue::Text(raw_value),
        };
        out.insert(field.key.clone(), parsed);
    }

    Some(out)
}

pub(crate) fn block_text_value(fields: &BTreeMap<String, ParsedBlockValue>, key: &str) -> String {
    fields
        .get(key)
        .and_then(ParsedBlockValue::as_text)
        .unwrap_or_default()
        .to_string()
}

pub(crate) fn block_list_value(
    fields: &BTreeMap<String, ParsedBlockValue>,
    key: &str,
) -> Vec<String> {
    fields
        .get(key)
        .and_then(ParsedBlockValue::as_list)
        .map(|items| items.to_vec())
        .unwrap_or_default()
}

pub(crate) fn block_usize_value(fields: &BTreeMap<String, ParsedBlockValue>, key: &str) -> usize {
    fields
        .get(key)
        .and_then(ParsedBlockValue::as_positive_int)
        .unwrap_or(0)
}

pub(crate) fn plan_field_min_items(key: &str) -> Option<usize> {
    governor_contract::block_field("plan", key).and_then(|field| field.min_items)
}

pub(crate) fn plan_field_max_items(key: &str) -> Option<usize> {
    governor_contract::block_field("plan", key).and_then(|field| field.max_items)
}

pub(crate) fn parse_plan_block(text: &str) -> Option<PlanBlock> {
    let fields = parse_block_fields(text, "plan")?;

    Some(PlanBlock {
        goal: block_text_value(&fields, "goal"),
        steps: block_list_value(&fields, "steps"),
        acceptance_criteria: block_list_value(&fields, "acceptance"),
        risks: block_text_value(&fields, "risks"),
        assumptions: block_text_value(&fields, "assumptions"),
    })
}

pub(crate) fn parse_think_block(text: &str) -> Option<ThinkBlock> {
    let fields = parse_block_fields(text, "think")?;

    Some(ThinkBlock {
        goal: block_text_value(&fields, "goal"),
        step: block_usize_value(&fields, "step"),
        tool: block_text_value(&fields, "tool"),
        risk: block_text_value(&fields, "risk"),
        doubt: block_text_value(&fields, "doubt"),
        next: block_text_value(&fields, "next"),
        verify: block_text_value(&fields, "verify"),
    })
}

pub(crate) fn json_string_field(
    obj: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Option<String> {
    match obj.get(key) {
        Some(serde_json::Value::String(s)) => Some(s.trim().to_string()),
        Some(serde_json::Value::Number(n)) => Some(n.to_string()),
        Some(serde_json::Value::Bool(b)) => Some(b.to_string()),
        _ => None,
    }
}

pub(crate) fn json_list_field(
    obj: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Vec<String> {
    obj.get(key)
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|item| match item {
                    serde_json::Value::String(s) => Some(compact_one_line(s.trim(), 120)),
                    serde_json::Value::Number(n) => Some(n.to_string()),
                    serde_json::Value::Bool(b) => Some(b.to_string()),
                    _ => None,
                })
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn pseudo_block_text_from_named_args(
    name: &str,
    args: &serde_json::Value,
) -> Option<String> {
    let obj = args.as_object()?;

    match name.trim_matches(|c| c == '<' || c == '>') {
        "plan" => {
            let goal = json_string_field(obj, "goal")?;
            let steps = json_list_field(obj, "steps");
            let acceptance = json_list_field(obj, "acceptance");
            let risks = json_string_field(obj, "risks")
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "unknown risks; inspect carefully before editing".to_string());
            let assumptions = json_string_field(obj, "assumptions")
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "current repo scan reflects the active workspace".to_string());
            let steps_line = steps
                .iter()
                .enumerate()
                .map(|(i, s)| format!("{} ) {}", i + 1, s))
                .collect::<Vec<_>>()
                .join(" ")
                .replace(" )", ")");
            let acceptance_line = acceptance
                .iter()
                .enumerate()
                .map(|(i, s)| format!("{} ) {}", i + 1, s))
                .collect::<Vec<_>>()
                .join(" ")
                .replace(" )", ")");
            Some(format!(
                "<plan>\n\
goal: {goal}\n\
steps: {steps_line}\n\
acceptance: {acceptance_line}\n\
risks: {risks}\n\
assumptions: {assumptions}\n\
</plan>\n"
            ))
        }
        "think" => {
            let goal = json_string_field(obj, "goal")?;
            let step = json_string_field(obj, "step").unwrap_or_default();
            let tool = json_string_field(obj, "tool").unwrap_or_default();
            let risk = json_string_field(obj, "risk")
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "wrong file or target".to_string());
            let doubt = json_string_field(obj, "doubt")
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "may need a broader check".to_string());
            let next = json_string_field(obj, "next").unwrap_or_default();
            let verify = json_string_field(obj, "verify")
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "confirm the result matches the request".to_string());
            Some(format!(
                "<think>\n\
goal: {goal}\n\
step: {step}\n\
tool: {tool}\n\
risk: {risk}\n\
doubt: {doubt}\n\
next: {next}\n\
verify: {verify}\n\
</think>\n"
            ))
        }
        _ => None,
    }
}

pub(crate) fn pseudo_tool_call_to_block_text(tc: &ToolCallData) -> Option<String> {
    let args: serde_json::Value = serde_json::from_str(&tc.arguments).ok()?;
    pseudo_block_text_from_named_args(&tc.name, &args)
}

pub(crate) fn known_runtime_tool_name_from_text(text: &str) -> Option<String> {
    const TOOL_NAMES: [&str; 9] = [
        "exec",
        "read_file",
        "write_file",
        "patch_file",
        "apply_diff",
        "search_files",
        "list_dir",
        "glob",
        "done",
    ];

    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    let low = trimmed.to_ascii_lowercase();
    for tool in TOOL_NAMES {
        if low == tool {
            return Some(tool.to_string());
        }
    }
    for tool in TOOL_NAMES {
        if let Some(prefix) = low.strip_suffix(tool) {
            let boundary_ok = prefix
                .chars()
                .last()
                .map(|ch| !ch.is_ascii_alphanumeric() && ch != '_')
                .unwrap_or(true);
            if boundary_ok {
                return Some(tool.to_string());
            }
        }
    }
    None
}

pub(crate) fn parse_reflection_block(text: &str) -> Option<ReflectionBlock> {
    let fields = parse_block_fields(text, "reflect")?;

    Some(ReflectionBlock {
        last_outcome: block_text_value(&fields, "last_outcome"),
        goal_delta: GoalDelta::parse(block_text_value(&fields, "goal_delta").as_str()),
        wrong_assumption: block_text_value(&fields, "wrong_assumption"),
        strategy_change: StrategyChange::parse(
            block_text_value(&fields, "strategy_change").as_str(),
        ),
        next_minimal_action: block_text_value(&fields, "next_minimal_action"),
    })
}

pub(crate) fn last_think_from_messages(messages: &[serde_json::Value]) -> Option<ThinkBlock> {
    for msg in messages.iter().rev() {
        if msg.get("role").and_then(|r| r.as_str()) != Some("assistant") {
            continue;
        }
        let content = msg.get("content").and_then(|c| c.as_str()).unwrap_or("");
        if let Some(think) = parse_think_block(content) {
            return Some(think);
        }
    }
    None
}

pub(crate) fn last_reflection_from_messages(
    messages: &[serde_json::Value],
) -> Option<ReflectionBlock> {
    for (index, msg) in messages.iter().enumerate().rev() {
        if super::blocked_report::is_accepted_closeout(messages, index) {
            continue;
        }
        if msg.get("role").and_then(|r| r.as_str()) != Some("assistant") {
            continue;
        }
        let content = msg.get("content").and_then(|c| c.as_str()).unwrap_or("");
        if let Some(r) = parse_reflection_block(content) {
            return Some(r);
        }
    }
    None
}
