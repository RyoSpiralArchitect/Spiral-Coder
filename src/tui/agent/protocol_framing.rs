//! Read a complete leading plan whose opening delimiter was cut by a provider.
//! Never modify retained prose, tool arguments, fields or semantic validation.
use super::{governor_contract, protocol_fields};

pub(super) fn leading_plan_body(text: &str) -> Option<&str> {
    let text = text.trim_start();
    let rest = text
        .strip_prefix("plan>")
        .or_else(|| text.strip_prefix('>'))?;
    // A standalone broken opener, not a Markdown quotation or an inline example.
    if !rest.starts_with(['\n', '\r']) {
        return None;
    }
    let end = rest.find("</plan>")?;
    let body = &rest[..end];
    if !body.rsplit('\n').next()?.trim().is_empty()
        || body.contains('<')
        || body.contains("```")
        || rest[end + "</plan>".len()..].contains("</plan>")
    {
        return None;
    }
    let fields = protocol_fields::parse_tag_fields(body, "plan");
    let first = fields.first()?;
    if governor_contract::block_field("plan", &first.0)?.key != "goal" {
        return None;
    }
    let mut seen = std::collections::BTreeSet::new();
    for (key, _) in fields {
        if !seen.insert(governor_contract::block_field("plan", &key)?.key.clone()) {
            return None;
        }
    }
    ["goal", "steps", "acceptance", "risks", "assumptions"]
        .iter()
        .all(|key| seen.contains(*key))
        .then_some(body.trim())
}

#[cfg(test)]
mod tests {
    use super::super::*;

    #[test]
    fn retained_live_openers_preserve_the_authored_fields_and_tool_turn() {
        let texts: Vec<String> = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/plan-revision/truncated-openers.json"
        )))
        .unwrap();
        assert_eq!(texts.len(), 1);
        // The retained second refusal plus an explicit missing-< variant.
        for text in texts
            .into_iter()
            .flat_map(|text| [format!("plan{text}"), text])
        {
            let parsed = parse_plan_block(&text).unwrap();
            validate_plan(&parsed).unwrap();
            let (_, rest) = text.split_once('>').unwrap();
            let ordinary = parse_plan_block(&format!("<plan>{rest}")).unwrap();
            assert_eq!(json!(parsed), json!(ordinary));
            let with_too_many = text.replace(
                "6) Call done",
                "6) Extra action.\n7) Extra action.\n8) Call done",
            );
            assert!(validate_plan(&parse_plan_block(&with_too_many).unwrap()).is_err());
            let missing = text.replace("risks:", "comment:");
            assert!(parse_plan_block(&missing).is_none());
            for unrelated in [
                format!("Example:\n{text}"),
                format!("```text\n{text}\n```"),
                format!("{text}\n</plan>"),
                text.replacen('>', "> quoted ", 1),
                text.replacen("goal:", "goal: Other\ngoal:", 1),
            ] {
                assert!(parse_plan_block(&unrelated).is_none(), "{unrelated}");
            }
        }
    }
}
