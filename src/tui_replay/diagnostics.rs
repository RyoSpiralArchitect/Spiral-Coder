//! Bounded replay diagnostics that survive the agent's generic error digest.
use super::{TuiReplayCase, TuiReplayCheck, TuiReplayMessageRole, TuiReplayReport};
use serde_json::json;
use std::path::Path;

const MAX_FAILED_CHECKS: usize = 8;

fn input_label(path: &Path, root: &Path) -> serde_json::Value {
    // Keep the actual input identity ahead of the cause and within short error
    // digests, even when the workspace itself has a long absolute path.
    json!(path.strip_prefix(root).unwrap_or(path))
}

pub(super) fn case_input_error(
    path: &Path,
    root: &Path,
    case: &TuiReplayCase,
    error: anyhow::Error,
) -> anyhow::Error {
    anyhow::anyhow!(
        "tui replay input failed; spec={} case={}; {error:#}",
        input_label(path, root),
        json!(bounded(&case.id, 80)),
    )
}

pub(super) fn empty_selection(
    path: &Path,
    root: &Path,
    filter: Option<&str>,
    cases: &[TuiReplayCase],
) -> anyhow::Error {
    anyhow::anyhow!(
        "tui replay selected 0 cases; spec={}\nError: tui-replay selection {}\nError: inspect the input spec's case IDs and tags against the requested filter. A source-file path in the task does not imply that file exists in the replay workspace. No case or check ran.",
        input_label(path, root),
        json!({"filter":filter.map(|s| bounded(s, 80)),"available_case_ids":cases.iter().take(8).map(|c| bounded(&c.id, 80)).collect::<Vec<_>>(),"total_cases":cases.len()}),
    )
}

#[cfg(test)]
pub(crate) fn parse_error_for_test(source: &str, error: serde_json::Error) -> anyhow::Error {
    parse_error(Path::new("replay.json"), source, error)
}

fn bounded(text: &str, limit: usize) -> String {
    let mut chars = text.chars();
    let mut result: String = chars.by_ref().take(limit).collect();
    if chars.next().is_some() {
        result.push('…');
    }
    result
}

pub(super) fn missing_target(case: &TuiReplayCase) -> anyhow::Error {
    let (mut user, mut assistant, mut tool) = (0, 0, 0);
    for message in &case.coder_messages {
        match message.role {
            TuiReplayMessageRole::User => user += 1,
            TuiReplayMessageRole::Assistant => assistant += 1,
            TuiReplayMessageRole::Tool => tool += 1,
        }
    }
    anyhow::anyhow!(
        "could not infer a stuck target from coder_messages; {}\n\
Error: tui-replay target requires a completed failure-like assistant message in top-level coder_messages (such as the recorded [GOVERNOR BLOCK] or [error] response).\n\
Error: tui-replay without selector, the last nonempty assistant must be failure-like. selector=msg:coder-<index> selects an existing failure-like assistant; user/tool messages cannot be targets.\n\
Error: tui-replay checks only assert the selected target. Changing target_message_contains.value cannot create or select a target.",
        json!({"case":bounded(&case.id, 80), "roles":{"user":user,"assistant":assistant,"tool":tool}})
    )
}

pub(crate) fn failed_report(report: &TuiReplayReport) -> String {
    let mut lines = vec![format!(
        "tui replay failed: {}/{} case(s) failed; spec={}",
        report.summary.failed,
        report.summary.total,
        json!(report.spec_path)
    )];
    let failures: Vec<_> = report
        .cases
        .iter()
        .flat_map(|case| {
            case.checks
                .iter()
                .filter(|check| !check.ok)
                .map(move |check| (case, check))
        })
        .collect();
    for (case, check) in failures.iter().take(MAX_FAILED_CHECKS) {
        // One physical line per record keeps data separate from diagnostics.
        lines.push(format!(
            "Error: tui-replay {}",
            json!({
                "case": bounded(&case.id, 120),
                "check": bounded(&check.label, 240),
                "detail": bounded(&check.detail, 240),
            })
        ));
    }
    if failures.len() > MAX_FAILED_CHECKS {
        lines.push(format!(
            "Error: tui-replay {} additional failed checks; see report",
            failures.len() - MAX_FAILED_CHECKS
        ));
    }
    if failures
        .iter()
        .any(|(_, check)| check.label == "suggestion_parsed")
    {
        lines.push("Error: tui-replay observer_response needs a nonempty summary, primary_blocker, or suggestions array in its JSON object. Empty or malformed responses do not parse.".to_string());
        lines.push("Error: tui-replay suggestions[].kind: search|read|done|clarify|abandon_path. Each suggestion is an object; include reason, confidence, suggested_tool and suggested_args as needed. Preserve intended assertions.".to_string());
    }
    if failures
        .iter()
        .any(|(_, check)| check.label == "hint_queued" || check.label == "contract_queued")
    {
        lines.push("Error: tui-replay inspect observer_response.suggestions: a queued hint needs a primary suggestion with confidence >= 0.75. Include suggested_tool and suggested_args for a concrete tool action; quickest_check alone does not queue a hint.".to_string());
    }
    if failures
        .iter()
        .any(|(_, check)| check.label == "contract_queued")
    {
        lines.push("Error: tui-replay contract_queued also requires observer_response.response_contract.required=true with a supported response contract.".to_string());
    }
    if failures
        .iter()
        .any(|(_, check)| check.label.starts_with("target_message_contains:"))
    {
        lines.push("Error: tui-replay target_message_contains compares the selected message ID, not the case ID or source-file text. Inspect selector and coder_messages against the intended assertion; do not remove failing assertions.".to_string());
    }
    if failures
        .iter()
        .any(|(_, check)| check.label.starts_with("coder_system_contains:"))
    {
        lines.push("Error: tui-replay coder_system_contains checks generated Coder system messages, not case IDs or source files. Inspect coder_preview_messages.json and observer_response against the intended assertion.".to_string());
    }
    // Emit the real artifact path, not a prefix-truncated path that cannot be read.
    lines.push(format!(
        "Error: tui-replay report={}",
        report.out_dir.join("report.json").display()
    ));
    lines.join("\n")
}

pub(super) fn parse_error(path: &Path, source: &str, error: serde_json::Error) -> anyhow::Error {
    let mut message = format!(
        "failed to parse tui replay spec: {error}; path={}",
        path.display()
    );
    if error.is_syntax() || error.is_eof() {
        // Use the exact source that failed parsing, not a second filesystem read.
        // JSON-escaped physical lines cannot inject new runtime status headers.
        let target = error.line().max(1);
        for (index, line) in source
            .lines()
            .enumerate()
            .skip(target.saturating_sub(3))
            .take(4)
        {
            message.push_str(&format!(
                "\nError: tui-replay source {}",
                json!({
                    "line": index + 1, "text": bounded(line, 140),
                })
            ));
        }
        message.push_str("\nError: tui-replay source excerpts are diagnostic data. Preserve surrounding JSON delimiters and existing cases when repairing the syntax.");
    }
    if error.is_data() {
        // Serialize the actual enum so examples cannot silently drift from the parser.
        let text = "expected text".to_string();
        let checks = [
            TuiReplayCheck::SuggestionParsed,
            TuiReplayCheck::HintQueued,
            TuiReplayCheck::ContractQueued,
            TuiReplayCheck::ObserverContains {
                value: text.clone(),
            },
            TuiReplayCheck::CoderSystemContains { value: text },
            TuiReplayCheck::TargetMessageContains {
                value: "coder-0".to_string(),
            },
        ];
        // Put the failing shape first. A list of every example on one long
        // line loses its later `value` fields in automatic-test/history digests.
        message.push_str(&check_context(source, &checks));
        message.push_str(&format!(
            "\nError: tui-replay valid top-level checks shapes: {}",
            serde_json::to_string(&checks).expect("check examples serialize")
        ));
        message.push_str("\nError: tui-replay target_message_contains matches the selected message ID (for example coder-0), not source-file content. Choose checks for the behavior the case is meant to prove; do not remove failing assertions.");
    }
    anyhow::anyhow!(message)
}

fn check_context(source: &str, examples: &[TuiReplayCheck]) -> String {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(source) else {
        return String::new();
    };
    for (case_index, case) in value["cases"].as_array().into_iter().flatten().enumerate() {
        for (check_index, check) in case["checks"].as_array().into_iter().flatten().enumerate() {
            let Err(error) = serde_json::from_value::<TuiReplayCheck>(check.clone()) else {
                continue;
            };
            let mut lines = format!(
                "\nError: tui-replay cases[{case_index}].checks[{check_index}]: {}",
                // Escape user-controlled unknown variants into one physical line.
                serde_json::to_string(&bounded(&error.to_string(), 120)).expect("error serializes")
            );
            let shapes: Vec<_> = examples
                .iter()
                .map(|example| serde_json::to_value(example).expect("check example serializes"))
                .collect();
            if let Some(shape) = shapes.iter().find(|shape| shape["kind"] == check["kind"]) {
                lines.push_str(&format!(
                    "\nError: tui-replay check shape (example value; preserve intended assertion): {shape}"
                ));
            } else {
                // Both lines fit inside the bounded automatic-test digest.
                lines.push_str("\nError: tui-replay check kinds: suggestion_parsed, hint_queued, contract_queued (no value field).");
                lines.push_str("\nError: tui-replay check kinds: observer_contains, coder_system_contains, target_message_contains (require a string value).");
            }
            return lines;
        }
    }
    String::new()
}
