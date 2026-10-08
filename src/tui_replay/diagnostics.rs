//! Bounded replay diagnostics that survive the agent's generic error digest.
use super::{TuiReplayCase, TuiReplayCheck, TuiReplayMessageRole, TuiReplayReport};
use serde_json::json;
use std::path::Path;

const MAX_FAILED_CHECKS: usize = 8;

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
        "tui replay failed: {}/{} case(s) failed",
        report.summary.failed, report.summary.total
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
        message.push_str(&format!(
            "\nError: tui-replay valid top-level checks shapes: {}",
            serde_json::to_string(&checks).expect("check examples serialize")
        ));
        message.push_str("\nError: tui-replay target_message_contains matches the selected message ID (for example coder-0), not source-file content. Choose checks for the behavior the case is meant to prove; do not remove failing assertions.");
    }
    anyhow::anyhow!(message)
}
