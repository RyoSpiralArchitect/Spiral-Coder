//! Check existing bindings in a small, source-backed Rust guard insertion.
use crate::tui::{agent::task_obligations::TaskObligations, suggestion::ObserverSuggestion};

pub(super) fn issue(
    suggestion: &ObserverSuggestion,
    obligations: &TaskObligations,
    root: Option<&str>,
) -> Option<String> {
    if suggestion.suggested_tool.as_deref() != Some("patch_file") {
        return None;
    }
    let followup = obligations.diagnostic_followup.as_ref()?;
    let arguments = &suggestion.suggested_args;
    let path = crate::task_scope::relative_path(arguments["path"].as_str()?, root)?;
    if !path.ends_with(".rs") {
        return None;
    }
    let search = arguments["search"].as_str()?;
    let replace = arguments["replace"].as_str()?;
    if !followup.observed_source_fragment(&path, search) {
        return None;
    }
    let missing = inserted_operands(search, replace)?
        .into_iter()
        .find(|name| !followup.observed_source_identifier(&path, name))?;
    Some(format!(
        "The added Rust guard refers to `{missing}`, which has no identifier occurrence in the retained native reads of {path}. Copy the existing binding from the observed source, or request the bounded region containing its declaration before proposing an exact patch."
    ))
}

/// This evidence check covers whole comparison lines inserted into an existing
/// if condition. Edits that add declarations or replace other code use the
/// usual action, approval, and verification paths.
fn inserted_operands<'a>(search: &str, replacement: &'a str) -> Option<Vec<&'a str>> {
    let mut original = search.lines().peekable();
    if !original.peek()?.trim_start().starts_with("if ") {
        return None;
    }
    let mut in_condition = true;
    let mut operands = Vec::new();
    for line in replacement.lines() {
        if original.peek() == Some(&line) {
            original.next();
            in_condition &= !line.contains('{');
        } else {
            if !in_condition {
                return None;
            }
            operands.extend(comparison_operands(line)?);
        }
    }
    original.next().is_none().then_some(operands)
}

fn comparison_operands(line: &str) -> Option<impl Iterator<Item = &str>> {
    let expression = line
        .trim_start()
        .strip_prefix("||")
        .or_else(|| line.trim_start().strip_prefix("&&"))?
        .trim_start();
    let end = expression.find(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))?;
    let lhs = identifier(&expression[..end])?;
    let rhs = [">=", "<=", "==", "!=", ">", "<"]
        .into_iter()
        .find_map(|operator| expression[end..].trim_start().strip_prefix(operator))?
        .trim();
    if rhs.is_empty() {
        return None;
    }
    Some([Some(lhs), identifier(rhs)].into_iter().flatten())
}

fn identifier(value: &str) -> Option<&str> {
    let mut letters = value.bytes();
    let first = letters.next()?;
    (first.is_ascii_alphabetic() || first == b'_')
        .then_some(())
        .filter(|_| !matches!(value, "true" | "false" | "_"))
        .filter(|_| letters.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_'))
        .map(|_| value)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEARCH: &str =
        "    if visible.map_or(false, |v| v > cap)\n    {\n        return Err(Error);\n    }";

    #[test]
    fn collects_bare_operands_only_from_added_comparison_lines() {
        let patch = SEARCH.replace(
            "    {",
            "        || wire_total> manifest.limit\n        && known >= cap\n    {",
        );
        assert_eq!(
            inserted_operands(SEARCH, &patch),
            Some(vec!["wire_total", "known", "cap"])
        );
        for patch in [
            format!("    let known = read_total();\n{SEARCH}"),
            SEARCH.replace("    {", "        || ledger.total() > cap\n    {"),
            SEARCH.replace("    {", "    {\n        || unrelated > cap"),
        ] {
            assert!(inserted_operands(SEARCH, &patch).is_none());
        }
    }
}
