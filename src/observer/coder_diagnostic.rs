use crate::observer::detector::Event;
use crate::observer::{Critique, Proposal, Risk};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MutationAnchor {
    pub path: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,

    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RequiredFollowup {
    pub path: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required_literal: Option<String>,

    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CoderAction {
    pub tool: String,

    #[serde(default)]
    pub args: Value,

    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CoderDiagnostic {
    pub failure_mode: String,
    pub confidence: u8,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mutation_anchor: Option<MutationAnchor>,

    #[serde(default)]
    pub required_followups: Vec<RequiredFollowup>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verification_cmd: Option<String>,

    #[serde(default)]
    pub final_handoff_literals: Vec<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_coder_action: Option<CoderAction>,

    #[serde(default)]
    pub evidence: Vec<String>,
}

pub fn diagnose(events: &[Event], critique: &Critique) -> Option<CoderDiagnostic> {
    if critique.risks.is_empty() && critique.proposals.is_empty() {
        return None;
    }

    let mutation_anchor = mutation_anchor(events);
    let required_followups = required_followups(&critique.proposals, mutation_anchor.as_ref());
    let verification_cmd = verification_cmd(events, mutation_anchor.as_ref(), &required_followups);
    let failure_mode = failure_mode(events, &critique.risks, &required_followups);
    let next_coder_action = next_coder_action(
        mutation_anchor.as_ref(),
        &required_followups,
        verification_cmd.as_deref(),
    );
    let evidence = evidence(events, &critique.risks, &critique.proposals);

    if mutation_anchor.is_none()
        && required_followups.is_empty()
        && verification_cmd.is_none()
        && evidence.is_empty()
        && critique.risks.is_empty()
        && critique.proposals.is_empty()
    {
        return None;
    }

    let confidence = confidence_score(
        mutation_anchor.as_ref(),
        &required_followups,
        verification_cmd.as_deref(),
        next_coder_action.as_ref(),
    );
    let final_handoff_literals = final_handoff_literals(
        mutation_anchor.as_ref(),
        &required_followups,
        verification_cmd.as_deref(),
    );

    Some(CoderDiagnostic {
        failure_mode,
        confidence,
        mutation_anchor,
        required_followups,
        verification_cmd,
        final_handoff_literals,
        next_coder_action,
        evidence,
    })
}

fn mutation_anchor(events: &[Event]) -> Option<MutationAnchor> {
    if let Some(path) = latest_written_path(events, |path| path.starts_with("src/")) {
        return Some(MutationAnchor {
            path,
            symbol: None,
            reason: "latest source file edited by the coder".to_string(),
        });
    }
    if let Some(path) = latest_written_path(events, |path| path.starts_with("docs/")) {
        return Some(MutationAnchor {
            path,
            symbol: None,
            reason: "latest documentation follow-up edited by the coder".to_string(),
        });
    }
    if let Some(path) = latest_written_path(events, |path| path.starts_with(".spiral-coder/")) {
        return Some(MutationAnchor {
            path,
            symbol: None,
            reason: "latest repo-local harness artifact edited by the coder".to_string(),
        });
    }
    latest_written_path(events, |_| true).map(|path| MutationAnchor {
        path,
        symbol: None,
        reason: "latest file edited by the coder".to_string(),
    })
}

fn latest_written_path(events: &[Event], predicate: impl Fn(&str) -> bool) -> Option<String> {
    for event in events.iter().rev() {
        let path = match event {
            Event::FileWritten { path } | Event::LargeDiffApplied { path, .. } => path.trim(),
            _ => continue,
        };
        if path.is_empty() || !predicate(path) {
            continue;
        }
        return Some(path.to_string());
    }
    None
}

fn required_followups(
    proposals: &[Proposal],
    anchor: Option<&MutationAnchor>,
) -> Vec<RequiredFollowup> {
    let haystack = proposals
        .iter()
        .map(|p| {
            format!(
                "{}\n{}\n{}\n{}",
                p.title.trim(),
                p.to_coder.trim(),
                p.impact.trim(),
                p.quote.trim()
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    let mut out = Vec::new();
    push_required_followup(
        &mut out,
        &haystack,
        "docs/state-schema.md",
        anchor.map(|a| a.path.as_str()),
        "state ownership docs must stay in the same change as typed state changes",
    );
    push_required_followup(
        &mut out,
        &haystack,
        "docs/runtime-architecture.md",
        anchor.map(|a| a.path.as_str()),
        "runtime architecture docs must track control-flow or Observer/Coder contract changes",
    );
    push_required_followup(
        &mut out,
        &haystack,
        ".spiral-coder/runtime_eval.json",
        Some("eval --spec .spiral-coder/runtime_eval.json"),
        "coder-loop changes need runtime eval proof before closeout",
    );
    push_required_followup(
        &mut out,
        &haystack,
        ".spiral-coder/tui_replay.json",
        Some("tui-replay --spec .spiral-coder/tui_replay.json"),
        "TUI-visible changes need replay proof before closeout",
    );

    out
}

fn push_required_followup(
    out: &mut Vec<RequiredFollowup>,
    haystack: &str,
    path: &str,
    literal: Option<&str>,
    reason: &str,
) {
    if !haystack.contains(path) {
        return;
    }
    if out.iter().any(|f| f.path == path) {
        return;
    }
    out.push(RequiredFollowup {
        path: path.to_string(),
        required_literal: literal.map(|s| s.to_string()),
        reason: reason.to_string(),
    });
}

fn verification_cmd(
    events: &[Event],
    anchor: Option<&MutationAnchor>,
    required_followups: &[RequiredFollowup],
) -> Option<String> {
    if required_followups
        .iter()
        .any(|f| f.path == ".spiral-coder/runtime_eval.json")
    {
        return Some(
            "cargo run --quiet -- eval --spec .spiral-coder/runtime_eval.json --max-cases 1".to_string(),
        );
    }
    if required_followups
        .iter()
        .any(|f| f.path == ".spiral-coder/tui_replay.json")
    {
        return Some("cargo run --quiet -- tui-replay --spec .spiral-coder/tui_replay.json".to_string());
    }

    for event in events.iter().rev() {
        if let Event::CommandFailure { command, .. } = event {
            if looks_like_verification_command(command) {
                return Some(command.trim().to_string());
            }
        }
    }
    for event in events.iter().rev() {
        if let Event::CommandExecuted { command } = event {
            if looks_like_verification_command(command) {
                return Some(command.trim().to_string());
            }
        }
    }

    anchor.and_then(|a| suggested_verification_for_path(a.path.as_str()))
}

fn suggested_verification_for_path(path: &str) -> Option<String> {
    if path == "src/merge_gate.rs" || path == "src/eval_merge_gate.rs" {
        return Some("cargo test -q merge_gate::tests::".to_string());
    }
    if path == "src/observer/repo_rules.rs" {
        return Some("cargo test -q observer::repo_rules::tests::".to_string());
    }
    if path.starts_with("src/observer/") {
        return Some("cargo test -q observer::".to_string());
    }
    if path.starts_with("src/tui/agent/") || path == "src/tui/agent.rs" {
        return Some("cargo test -q tui::agent::tests::".to_string());
    }
    if path == "src/tui/events.rs" || path == "src/tui/suggestion.rs" {
        return Some("cargo test -q tui::events::tests::".to_string());
    }
    if path.starts_with("src/") {
        return Some("cargo test -q".to_string());
    }
    None
}

fn looks_like_verification_command(command: &str) -> bool {
    let cmd = command.trim().to_ascii_lowercase();
    if cmd.is_empty() {
        return false;
    }
    cmd == "git status"
        || cmd.starts_with("git status ")
        || cmd == "git diff"
        || cmd.starts_with("git diff ")
        || cmd.contains("cargo test")
        || cmd.contains("cargo check")
        || cmd.contains("cargo clippy")
        || cmd.contains("cargo fmt")
        || cmd.contains("tui-replay")
        || cmd.contains("runtime_eval.json")
        || cmd.contains("pytest")
        || cmd.contains("python -m pytest")
        || cmd.contains("npm test")
        || cmd.contains("pnpm test")
        || cmd.contains("yarn test")
        || cmd.contains("go test")
}

fn failure_mode(
    events: &[Event],
    risks: &[Risk],
    required_followups: &[RequiredFollowup],
) -> String {
    if !required_followups.is_empty() {
        return "missing_required_followup".to_string();
    }
    if risks
        .iter()
        .any(|r| r.description.eq_ignore_ascii_case("Loop/stall detected"))
    {
        return "loop_or_stall".to_string();
    }
    if events
        .iter()
        .any(|e| matches!(e, Event::CommandFailure { .. }))
    {
        return "failing_verification".to_string();
    }
    if risks.iter().any(|r| {
        r.description
            .eq_ignore_ascii_case("No verification command after edits")
    }) {
        return "missing_verification".to_string();
    }
    if risks
        .iter()
        .any(|r| r.description.eq_ignore_ascii_case("Unbounded file scan"))
    {
        return "noisy_observation".to_string();
    }
    "needs_coder_followup".to_string()
}

fn next_coder_action(
    anchor: Option<&MutationAnchor>,
    required_followups: &[RequiredFollowup],
    verification_cmd: Option<&str>,
) -> Option<CoderAction> {
    if let Some(doc) = required_followups
        .iter()
        .find(|f| f.path.starts_with("docs/"))
    {
        return Some(CoderAction {
            tool: "read_file".to_string(),
            args: json!({ "path": doc.path }),
            reason: format!("inspect required follow-up before patching: {}", doc.reason),
        });
    }
    if let Some(cmd) = verification_cmd {
        if required_followups
            .iter()
            .any(|f| f.path.starts_with(".spiral-coder/"))
        {
            return Some(CoderAction {
                tool: "exec".to_string(),
                args: json!({ "command": cmd }),
                reason: "produce the required replay/eval proof before closeout".to_string(),
            });
        }
    }
    if let Some(anchor) = anchor {
        return Some(CoderAction {
            tool: "read_file".to_string(),
            args: json!({ "path": anchor.path }),
            reason: "re-open the mutation anchor before making the next targeted change"
                .to_string(),
        });
    }
    verification_cmd.map(|cmd| CoderAction {
        tool: "exec".to_string(),
        args: json!({ "command": cmd }),
        reason: "verify the edited artifact before closeout".to_string(),
    })
}

fn final_handoff_literals(
    anchor: Option<&MutationAnchor>,
    required_followups: &[RequiredFollowup],
    verification_cmd: Option<&str>,
) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    if let Some(anchor) = anchor {
        push_literal(&mut out, &mut seen, anchor.path.as_str());
    }
    for followup in required_followups {
        push_literal(&mut out, &mut seen, followup.path.as_str());
        if let Some(lit) = followup.required_literal.as_deref() {
            push_literal(&mut out, &mut seen, lit);
        }
    }
    if let Some(cmd) = verification_cmd {
        push_literal(&mut out, &mut seen, cmd);
    }
    out
}

fn push_literal(out: &mut Vec<String>, seen: &mut BTreeSet<String>, literal: &str) {
    let lit = literal.trim();
    if lit.is_empty() {
        return;
    }
    if seen.insert(lit.to_string()) {
        out.push(lit.to_string());
    }
}

fn evidence(events: &[Event], risks: &[Risk], proposals: &[Proposal]) -> Vec<String> {
    let mut out = Vec::new();
    for event in events {
        match event {
            Event::FileWritten { path } => out.push(format!("edited: {}", path.trim())),
            Event::CommandFailure { command, stderr } => {
                let stderr = first_line(stderr);
                if stderr.is_empty() {
                    out.push(format!("failed: {}", command.trim()));
                } else {
                    out.push(format!("failed: {} :: {}", command.trim(), stderr));
                }
            }
            Event::LoopDetected { reason } => out.push(format!("loop: {}", reason.trim())),
            _ => {}
        }
    }
    for risk in risks {
        if !risk.description.trim().is_empty() {
            out.push(format!("risk: {}", risk.description.trim()));
        }
    }
    for proposal in proposals {
        if !proposal.title.trim().is_empty() {
            out.push(format!("proposal: {}", proposal.title.trim()));
        }
    }

    let mut seen = BTreeSet::new();
    out.into_iter()
        .filter_map(|entry| {
            let entry = entry.trim();
            if entry.is_empty() || !seen.insert(entry.to_string()) {
                None
            } else {
                Some(entry.chars().take(180).collect::<String>())
            }
        })
        .take(10)
        .collect()
}

fn first_line(s: &str) -> String {
    s.lines().next().unwrap_or("").trim().to_string()
}

fn confidence_score(
    anchor: Option<&MutationAnchor>,
    required_followups: &[RequiredFollowup],
    verification_cmd: Option<&str>,
    next_action: Option<&CoderAction>,
) -> u8 {
    let mut score = 50u8;
    if anchor.is_some() {
        score = score.saturating_add(15);
    }
    if !required_followups.is_empty() {
        score = score.saturating_add(15);
    }
    if verification_cmd.is_some() {
        score = score.saturating_add(10);
    }
    if next_action.is_some() {
        score = score.saturating_add(5);
    }
    score.min(95)
}

#[cfg(test)]
mod tests {
    use crate::observer::engine::run_observer_from_transcript;

    #[test]
    fn diagnoses_runtime_eval_followup_for_agent_edit() {
        let transcript = r#"
✎ patch_file: src/tui/agent/task_harness.rs
```bash
$ cargo test -q tui::agent::tests::
exit: 0
```
"#;

        let critique = run_observer_from_transcript(transcript, None);
        let diagnostic = critique.coder_diagnostic.expect("coder diagnostic");

        assert_eq!(diagnostic.failure_mode, "missing_required_followup");
        assert_eq!(
            diagnostic.mutation_anchor.as_ref().map(|a| a.path.as_str()),
            Some("src/tui/agent/task_harness.rs")
        );
        assert!(diagnostic
            .required_followups
            .iter()
            .any(|f| f.path == ".spiral-coder/runtime_eval.json"));
        assert_eq!(
            diagnostic.verification_cmd.as_deref(),
            Some("cargo run --quiet -- eval --spec .spiral-coder/runtime_eval.json --max-cases 1")
        );
        assert_eq!(
            diagnostic
                .next_coder_action
                .as_ref()
                .map(|a| a.tool.as_str()),
            Some("exec")
        );
    }

    #[test]
    fn diagnoses_doc_followup_before_replay_for_tui_state_edit() {
        let transcript = r#"
✎ patch_file: src/tui/prefs.rs
```bash
$ cargo test -q tui::events::tests::
exit: 0
```
"#;

        let critique = run_observer_from_transcript(transcript, None);
        let diagnostic = critique.coder_diagnostic.expect("coder diagnostic");

        assert_eq!(diagnostic.failure_mode, "missing_required_followup");
        assert!(diagnostic
            .required_followups
            .iter()
            .any(|f| f.path == "docs/state-schema.md"));
        assert!(diagnostic
            .required_followups
            .iter()
            .any(|f| f.path == ".spiral-coder/tui_replay.json"));
        assert_eq!(
            diagnostic
                .next_coder_action
                .as_ref()
                .map(|a| a.tool.as_str()),
            Some("read_file")
        );
        assert_eq!(
            diagnostic
                .next_coder_action
                .as_ref()
                .and_then(|a| a.args.get("path"))
                .and_then(|v| v.as_str()),
            Some("docs/state-schema.md")
        );
    }

    #[test]
    fn keeps_source_anchor_even_after_doc_followup_edit() {
        let transcript = r#"
✎ patch_file: src/tui/prefs.rs
✎ patch_file: docs/state-schema.md
```bash
$ cargo test -q tui::events::tests::
exit: 0
```
"#;

        let critique = run_observer_from_transcript(transcript, None);
        let diagnostic = critique.coder_diagnostic.expect("coder diagnostic");

        assert_eq!(
            diagnostic.mutation_anchor.as_ref().map(|a| a.path.as_str()),
            Some("src/tui/prefs.rs")
        );
        assert!(!diagnostic
            .required_followups
            .iter()
            .any(|f| f.path == "docs/state-schema.md"));
        assert!(diagnostic
            .required_followups
            .iter()
            .any(|f| f.path == ".spiral-coder/tui_replay.json"));
    }

    #[test]
    fn skips_diagnostic_for_clean_verified_edit() {
        let transcript = r#"
✎ patch_file: docs/runtime-architecture.md
```bash
$ git status --short
exit: 0
```
"#;

        let critique = run_observer_from_transcript(transcript, None);

        assert!(critique.risks.is_empty());
        assert!(critique.proposals.is_empty());
        assert!(critique.coder_diagnostic.is_none());
    }
}
