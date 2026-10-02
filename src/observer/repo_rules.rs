use crate::observer::detector::Event;
use crate::observer::{Risk, RiskAxis, Severity};

const RISK_MISSING_STATE_SCHEMA_DOC: &str = "Missing state ownership doc follow-up";
const RISK_MISSING_TUI_REPLAY_PROOF: &str = "Missing TUI replay proof";
const RISK_MISSING_RUNTIME_EVAL_PROOF: &str = "Missing runtime eval proof";

const STATE_OWNER_PATHS: &[&str] = &[
    "src/config.rs",
    "src/tui/prefs.rs",
    "src/agent_session.rs",
    "src/tui/app.rs",
    "src/tui/intent.rs",
    "src/progress_state.rs",
    "src/reflection_ledger.rs",
];

pub fn detect(events: &[Event]) -> Vec<Risk> {
    let mut out = Vec::new();

    if let Some(risk) = missing_state_schema_doc_risk(events) {
        out.push(risk);
    }
    if let Some(risk) = missing_tui_replay_proof_risk(events) {
        out.push(risk);
    }
    if let Some(risk) = missing_runtime_eval_proof_risk(events) {
        out.push(risk);
    }

    out
}

pub fn is_missing_state_schema_doc(description: &str) -> bool {
    description == RISK_MISSING_STATE_SCHEMA_DOC
}

pub fn is_missing_tui_replay_proof(description: &str) -> bool {
    description == RISK_MISSING_TUI_REPLAY_PROOF
}

pub fn is_missing_runtime_eval_proof(description: &str) -> bool {
    description == RISK_MISSING_RUNTIME_EVAL_PROOF
}

fn missing_state_schema_doc_risk(events: &[Event]) -> Option<Risk> {
    let last_state_owner_edit = last_relevant_edit(events, |path| {
        STATE_OWNER_PATHS.iter().any(|candidate| path == *candidate)
    })?;
    if edited_after(events, last_state_owner_edit, "docs/state-schema.md") {
        return None;
    }

    let path = edited_path_at(events, last_state_owner_edit)?;
    Some(Risk {
        axis: RiskAxis::Maintainability,
        severity: Severity::Warn,
        description: RISK_MISSING_STATE_SCHEMA_DOC.to_string(),
        evidence: Some(format!(
            "edited owner file: {path}\nmissing follow-up: docs/state-schema.md"
        )),
    })
}

fn missing_tui_replay_proof_risk(events: &[Event]) -> Option<Risk> {
    let last_tui_edit = last_relevant_edit(events, requires_tui_replay)?;
    if command_seen_after(events, last_tui_edit, is_tui_replay_command) {
        return None;
    }

    let path = edited_path_at(events, last_tui_edit)?;
    Some(Risk {
        axis: RiskAxis::Reliability,
        severity: Severity::Warn,
        description: RISK_MISSING_TUI_REPLAY_PROOF.to_string(),
        evidence: Some(format!(
            "edited TUI path: {path}\nmissing follow-up: `cargo run -- ... tui-replay --spec .spiral-coder/tui_replay.json`"
        )),
    })
}

fn missing_runtime_eval_proof_risk(events: &[Event]) -> Option<Risk> {
    let last_runtime_edit = last_relevant_edit(events, requires_runtime_eval)?;
    if command_seen_after(events, last_runtime_edit, is_runtime_eval_command) {
        return None;
    }

    let path = edited_path_at(events, last_runtime_edit)?;
    Some(Risk {
        axis: RiskAxis::Reliability,
        severity: Severity::Warn,
        description: RISK_MISSING_RUNTIME_EVAL_PROOF.to_string(),
        evidence: Some(format!(
            "edited runtime path: {path}\nmissing follow-up: `cargo run -- ... eval --spec .spiral-coder/runtime_eval.json`"
        )),
    })
}

fn last_relevant_edit(events: &[Event], predicate: impl Fn(&str) -> bool) -> Option<usize> {
    events
        .iter()
        .enumerate()
        .rev()
        .find_map(|(idx, event)| match event {
            Event::FileWritten { path } if predicate(path.trim()) => Some(idx),
            _ => None,
        })
}

fn edited_after(events: &[Event], idx: usize, target_path: &str) -> bool {
    events
        .iter()
        .skip(idx.saturating_add(1))
        .any(|event| match event {
            Event::FileWritten { path } => path.trim() == target_path,
            _ => false,
        })
}

fn command_seen_after(events: &[Event], idx: usize, predicate: impl Fn(&str) -> bool) -> bool {
    events
        .iter()
        .skip(idx.saturating_add(1))
        .any(|event| match event {
            Event::CommandExecuted { command } => predicate(command),
            _ => false,
        })
}

fn edited_path_at(events: &[Event], idx: usize) -> Option<String> {
    match events.get(idx)? {
        Event::FileWritten { path } => Some(path.trim().to_string()),
        _ => None,
    }
}

fn requires_tui_replay(path: &str) -> bool {
    matches!(
        path,
        "src/tui/events.rs"
            | "src/tui/app.rs"
            | "src/tui/prefs.rs"
            | "src/tui/ui.rs"
            | "src/tui/suggestion.rs"
    )
}

fn requires_runtime_eval(path: &str) -> bool {
    path == "src/tui/agent.rs"
        || path.starts_with("src/tui/agent/")
        || path == "src/runtime_eval.rs"
}

fn is_tui_replay_command(command: &str) -> bool {
    let low = command.trim().to_ascii_lowercase();
    low.contains("tui-replay") || low.contains(".spiral-coder/tui_replay.json")
}

fn is_runtime_eval_command(command: &str) -> bool {
    let low = command.trim().to_ascii_lowercase();
    (low.contains(" eval ") || low.starts_with("eval ") || low.contains("spiral-coder eval"))
        && low.contains("runtime_eval.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_missing_state_schema_doc_followup() {
        let events = vec![Event::FileWritten {
            path: "src/tui/prefs.rs".to_string(),
        }];
        let risks = detect(&events);
        assert!(risks
            .iter()
            .any(|risk| is_missing_state_schema_doc(risk.description.as_str())));
    }

    #[test]
    fn skips_state_schema_risk_when_doc_is_updated_afterwards() {
        let events = vec![
            Event::FileWritten {
                path: "src/tui/prefs.rs".to_string(),
            },
            Event::FileWritten {
                path: "docs/state-schema.md".to_string(),
            },
        ];
        let risks = detect(&events);
        assert!(!risks
            .iter()
            .any(|risk| is_missing_state_schema_doc(risk.description.as_str())));
    }

    #[test]
    fn detects_missing_tui_replay_proof() {
        let events = vec![
            Event::FileWritten {
                path: "src/tui/prefs.rs".to_string(),
            },
            Event::CommandExecuted {
                command: "cargo test -q tui::events::tests::".to_string(),
            },
        ];
        let risks = detect(&events);
        assert!(risks
            .iter()
            .any(|risk| is_missing_tui_replay_proof(risk.description.as_str())));
    }

    #[test]
    fn detects_missing_runtime_eval_proof() {
        let events = vec![
            Event::FileWritten {
                path: "src/tui/agent/task_harness.rs".to_string(),
            },
            Event::CommandExecuted {
                command: "cargo test -q tui::agent::tests::".to_string(),
            },
        ];
        let risks = detect(&events);
        assert!(risks
            .iter()
            .any(|risk| is_missing_runtime_eval_proof(risk.description.as_str())));
    }

    #[test]
    fn skips_runtime_eval_risk_when_eval_command_ran() {
        let events = vec![
            Event::FileWritten {
                path: "src/tui/agent/task_harness.rs".to_string(),
            },
            Event::CommandExecuted {
                command:
                    "cargo run --quiet -- eval --spec .spiral-coder/runtime_eval.json --filter demo"
                        .to_string(),
            },
        ];
        let risks = detect(&events);
        assert!(!risks
            .iter()
            .any(|risk| is_missing_runtime_eval_proof(risk.description.as_str())));
    }
}
