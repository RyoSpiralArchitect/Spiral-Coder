//! Keep report instructions out of the authored final answer. This only removes
//! an exact echo of the current context; it never writes missing report prose.
use serde_json::Value;

pub(super) fn authored<'a>(summary: &'a str, report: &Value) -> &'a str {
    let summary = summary.trim();
    if report["clauses"].as_array().is_none_or(Vec::is_empty) {
        return summary;
    }
    let Ok(object) = crate::structured_output::object(summary) else {
        return summary;
    };
    if object.value != *report {
        return summary;
    }
    let prefix = object.prefix.trim();
    if prefix.is_empty() {
        return object.suffix.trim();
    }
    if matches!(prefix, "```json" | "```") {
        if let Some(rest) = object.suffix.trim_start().strip_prefix("```") {
            return rest.trim();
        }
    }
    summary
}

pub(super) fn guidance(report: &Value) -> String {
    let mut text = "Write your own nonempty final report: changes, full artifact paths, and remaining work. Write the following recorded outcome lines verbatim, each on its own line with its exact command. Commands in acceptance_evidence do not substitute for report text. Keep NOT RECORDED / NOT VERIFIED when shown. Do not copy the report-context JSON into your answer.\n".to_string();
    for (field, label) in [
        ("clauses", "Report requirements"),
        ("required_literals", "Required exact text"),
        ("path_categories", "Path categories"),
        ("outcome_rows", "Recorded outcomes"),
    ] {
        if let Some(items) = report[field].as_array().filter(|items| !items.is_empty()) {
            text.push_str(&format!("{label}:\n"));
            for item in items.iter().filter_map(Value::as_str) {
                text.push_str(item.trim());
                text.push('\n');
            }
        }
    }
    if report["cite_current_verification_command"] == true {
        text.push_str("Cite the current exact verification command in the report.\n");
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn only_exact_current_context_echo_is_removed_and_missing_prose_stays_missing() {
        let report =
            json!({"clauses":["include changed paths"],"outcome_rows":["After: PASSED — `check`"]});
        let text = "Changed runner.py.\nAfter: PASSED — `check`\nNo remaining work.";
        for raw in [
            format!("{report}\n{text}"),
            format!("```json\n{report}\n```\n{text}"),
        ] {
            assert_eq!(authored(&raw, &report), text);
        }
        assert_eq!(authored(&report.to_string(), &report), "");
        for raw in [
            format!("Explanation\n{report}\n{text}"),
            format!("{{\"my_data\":1}}\n{text}"),
            format!("{report}\n{{}}\n{text}"),
        ] {
            assert_eq!(authored(&raw, &report), raw);
        }
        let stale =
            json!({"clauses":["include changed paths"],"outcome_rows":["After: NOT VERIFIED"]});
        assert_eq!(authored(&report.to_string(), &stale), report.to_string());
    }

    #[test]
    fn echoed_report_context_cannot_satisfy_authored_report_gate() {
        let root = "Final answer must include a.py.";
        let context = super::super::final_handoff::report_context(root, &[], None);
        let raw = context.to_string();
        assert!(super::super::validate_authored_done_summary(root, &raw, &raw, &[], None).is_err());
        assert!(super::super::validate_authored_done_summary(
            root,
            &format!("{raw}\nUpdated a.py."),
            &format!("{raw}\nUpdated a.py."),
            &[],
            None
        )
        .is_ok());
    }
}
