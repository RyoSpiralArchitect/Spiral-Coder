use crate::observer::analyzer;
use crate::observer::benchmark_plan;
use crate::observer::coder_diagnostic;
use crate::observer::detector;
use crate::observer::memory::{self, CritiqueMemory};
use crate::observer::scorer;
use crate::observer::{
    Critique, DevPhase, HealthScore, Proposal, ProposalStatus, RiskAxis, Severity,
};
use serde_json::Value;

const MAX_DISPLAY_PROPOSALS: usize = 5;

fn status_rank(s: ProposalStatus) -> u8 {
    match s {
        ProposalStatus::Escalated => 0,
        ProposalStatus::Unresolved => 1,
        ProposalStatus::New => 2,
        ProposalStatus::Addressed => 3,
    }
}

fn severity_rank(s: Severity) -> u8 {
    match s {
        Severity::Crit => 0,
        Severity::Warn => 1,
        Severity::Info => 2,
    }
}

fn phase_rank(p: DevPhase) -> u8 {
    match p {
        DevPhase::Core => 0,
        DevPhase::Feature => 1,
        DevPhase::Any => 2,
        DevPhase::Polish => 3,
    }
}

fn phase_from_proposals(ps: &[Proposal]) -> DevPhase {
    if ps.iter().any(|p| p.phase == DevPhase::Core) {
        return DevPhase::Core;
    }
    if ps.iter().any(|p| p.phase == DevPhase::Feature) {
        return DevPhase::Feature;
    }
    if ps.is_empty() {
        DevPhase::Polish
    } else {
        DevPhase::Any
    }
}

fn summarize_from_counts(tool_results: usize, failures: usize, files_written: usize) -> String {
    if tool_results == 0 && failures == 0 && files_written == 0 {
        return "No tool activity detected.".to_string();
    }
    format!(
        "Observed {tool_results} tool results, {failures} failures, {files_written} file edits."
    )
}

fn slow_commands_summary(events: &[detector::Event]) -> Option<String> {
    let mut timings: Vec<(String, u64)> = events
        .iter()
        .filter_map(|e| match e {
            detector::Event::CommandTiming {
                command,
                duration_ms,
            } => {
                let cmd = command.trim();
                if cmd.is_empty() {
                    return None;
                }
                let cmd1 = cmd.lines().next().unwrap_or(cmd).trim();
                if cmd1.is_empty() {
                    return None;
                }
                Some((cmd1.to_string(), *duration_ms))
            }
            _ => None,
        })
        .collect();

    timings.sort_by(|a, b| b.1.cmp(&a.1));
    let max_ms = timings.first().map(|x| x.1).unwrap_or(0);
    if max_ms < 2000 {
        return None;
    }

    let mut out = String::new();
    out.push_str("Slow commands (top 3):\n");
    for (cmd, ms) in timings.iter().take(3) {
        let clipped = if cmd.len() > 120 {
            format!("{}…", &cmd[..120])
        } else {
            cmd.to_string()
        };
        out.push_str(&format!("- {clipped} ({ms}ms)\n"));
    }
    Some(out.trim_end().to_string())
}

fn edited_files_summary(events: &[detector::Event]) -> Option<String> {
    let mut uniq: Vec<String> = Vec::new();
    for e in events {
        if let detector::Event::FileWritten { path } = e {
            let p = path.trim();
            if p.is_empty() {
                continue;
            }
            if !uniq.iter().any(|x| x == p) {
                uniq.push(p.to_string());
            }
            if uniq.len() >= 3 {
                break;
            }
        }
    }
    if uniq.is_empty() {
        return None;
    }
    Some(format!("Edited files (up to 3):\n- {}", uniq.join("\n- ")))
}

fn summarize(
    events: &[detector::Event],
    tool_results: usize,
    failures: usize,
    files_written: usize,
) -> String {
    let mut out = summarize_from_counts(tool_results, failures, files_written);
    if let Some(extra) = slow_commands_summary(events) {
        out.push('\n');
        out.push_str(&extra);
    }
    if let Some(extra) = edited_files_summary(events) {
        out.push('\n');
        out.push_str(&extra);
    }
    out
}

fn critical_path_from_proposals(ps: &[Proposal]) -> String {
    let blockers: Vec<&Proposal> = ps
        .iter()
        .filter(|p| {
            p.status != ProposalStatus::Addressed
                && (p.severity == Severity::Crit
                    || p.score >= 80
                    || p.status == ProposalStatus::Escalated
                    || p.status == ProposalStatus::Unresolved)
        })
        .take(2)
        .collect();
    if !blockers.is_empty() {
        return blockers
            .iter()
            .map(|p| proposal_critical_fragment(p))
            .collect::<Vec<_>>()
            .join("; then ");
    }

    let Some(top) = ps
        .iter()
        .find(|p| p.status != ProposalStatus::Addressed)
        .or_else(|| ps.first())
    else {
        return "none".to_string();
    };
    proposal_critical_fragment(top)
}

fn proposal_critical_fragment(p: &Proposal) -> String {
    if p.impact.trim().is_empty() {
        p.title.trim().to_string()
    } else {
        format!("{} — {}", p.title.trim(), p.impact.trim())
    }
}

fn sort_proposals_for_display(proposals: &mut [Proposal]) {
    proposals.sort_by(|a, b| {
        status_rank(a.status)
            .cmp(&status_rank(b.status))
            .then(severity_rank(a.severity).cmp(&severity_rank(b.severity)))
            .then(b.score.cmp(&a.score))
            .then(phase_rank(a.phase).cmp(&phase_rank(b.phase)))
            .then_with(|| {
                a.title
                    .to_ascii_lowercase()
                    .cmp(&b.title.to_ascii_lowercase())
            })
    });
}

fn select_display_proposals(mut proposals: Vec<Proposal>) -> Vec<Proposal> {
    sort_proposals_for_display(&mut proposals);
    proposals.truncate(MAX_DISPLAY_PROPOSALS);
    proposals
}

fn health_from_proposals(
    ps: &[Proposal],
    failure_count: usize,
    top_risk_axis: Option<RiskAxis>,
) -> HealthScore {
    let max_score = ps.iter().map(|p| p.score).max().unwrap_or(0);

    let mut health: i32 = 100;
    health -= (max_score as i32 * 75) / 100; // 0..75
    health -= (failure_count as i32 * 3).min(20);

    let score = health.clamp(0, 100) as u32;

    let rationale = if let Some(p) = ps.first() {
        if !p.impact.trim().is_empty() {
            p.impact.trim().to_string()
        } else {
            format!("Top issue: {}", p.title.trim())
        }
    } else if let Some(axis) = top_risk_axis {
        format!("Primary risk axis: {:?}", axis)
    } else {
        "No significant issues detected.".to_string()
    };

    HealthScore { score, rationale }
}

fn run_from_events(
    events: Vec<detector::Event>,
    tool_results_count: usize,
    memory_opt: Option<&mut CritiqueMemory>,
) -> Critique {
    let mut memory_opt = memory_opt;
    let mut risks = analyzer::analyze(&events);
    if let Some(mem) = memory_opt.as_deref() {
        let candidate_proposals = analyzer::generate_proposals(&risks);
        risks.extend(memory::recurring_unresolved_risks(
            mem,
            &candidate_proposals,
        ));
    }

    let mut proposals = analyzer::generate_proposals(&risks);

    for p in &mut proposals {
        scorer::score_proposal(p);
    }

    if let Some(mem) = memory_opt.as_deref_mut() {
        memory::apply_memory(mem, &mut proposals);
    }

    sort_proposals_for_display(&mut proposals);
    let full_proposals = proposals.clone();

    let failures = events
        .iter()
        .filter(|e| matches!(e, detector::Event::CommandFailure { .. }))
        .count();
    let files_written = events
        .iter()
        .filter(|e| matches!(e, detector::Event::FileWritten { .. }))
        .count();

    let phase = phase_from_proposals(&full_proposals);
    let critical_path = critical_path_from_proposals(&full_proposals);
    let top_axis = risks.first().map(|r| r.axis);
    let health = health_from_proposals(&full_proposals, failures, top_axis);
    let summary = summarize(&events, tool_results_count, failures, files_written);

    let full_critique = Critique {
        summary: summary.clone(),
        risks: risks.clone(),
        proposals: full_proposals.clone(),
        phase,
        critical_path: critical_path.clone(),
        health: health.clone(),
        coder_diagnostic: None,
        benchmark_plan: None,
    };
    let mut full_critique = full_critique;
    full_critique.coder_diagnostic = coder_diagnostic::diagnose(&events, &full_critique);
    let benchmark_plan = benchmark_plan::plan(&full_critique);

    Critique {
        summary,
        risks,
        proposals: select_display_proposals(full_proposals),
        phase,
        critical_path,
        health,
        coder_diagnostic: full_critique.coder_diagnostic,
        benchmark_plan,
    }
}

pub fn run_observer(messages: &[Value], memory: Option<&mut CritiqueMemory>) -> Critique {
    let events = detector::detect_events(messages);
    let tool_results_count = messages
        .iter()
        .filter(|m| m.get("role").and_then(|r| r.as_str()) == Some("tool"))
        .count();
    run_from_events(events, tool_results_count, memory)
}

pub fn run_observer_from_transcript(
    transcript: &str,
    memory: Option<&mut CritiqueMemory>,
) -> Critique {
    let events = detector::detect_events_from_transcript(transcript);
    // In transcript mode, we don't have exact tool-result count; approximate with #execs.
    let tool_results_count = events
        .iter()
        .filter(|e| matches!(e, detector::Event::CommandExecuted { .. }))
        .count();
    run_from_events(events, tool_results_count, memory)
}

pub fn format_critique_as_observer_blocks(c: &Critique) -> String {
    let mut out = String::new();
    if !c.summary.trim().is_empty() {
        out.push_str(c.summary.trim_end());
        out.push_str("\n\n");
    }
    out.push_str("--- phase ---\n");
    out.push_str(match c.phase {
        crate::observer::DevPhase::Core => "core",
        crate::observer::DevPhase::Feature => "feature",
        crate::observer::DevPhase::Polish => "polish",
        crate::observer::DevPhase::Any => "any",
    });
    out.push_str("\n\n--- proposals ---\n");
    if c.proposals.is_empty() {
        out.push_str("(none)\n");
    } else {
        for (i, p) in c.proposals.iter().enumerate() {
            out.push_str(&format!("{}) title: {}\n", i + 1, p.title));
            out.push_str(&format!("   to_coder: {}\n", p.to_coder));
            out.push_str(&format!(
                "   severity: {}\n",
                match p.severity {
                    crate::observer::Severity::Info => "info",
                    crate::observer::Severity::Warn => "warn",
                    crate::observer::Severity::Crit => "crit",
                }
            ));
            out.push_str(&format!("   score: {}\n", p.score));
            out.push_str(&format!(
                "   phase: {}\n",
                match p.phase {
                    crate::observer::DevPhase::Core => "core",
                    crate::observer::DevPhase::Feature => "feature",
                    crate::observer::DevPhase::Polish => "polish",
                    crate::observer::DevPhase::Any => "any",
                }
            ));
            out.push_str(&format!("   impact: {}\n", p.impact));
            out.push_str(&format!(
                "   cost: {}\n",
                match p.cost {
                    crate::observer::Cost::Low => "low",
                    crate::observer::Cost::Medium => "medium",
                    crate::observer::Cost::High => "high",
                }
            ));
            out.push_str(&format!(
                "   status: {}\n",
                match p.status {
                    crate::observer::ProposalStatus::New => "new",
                    crate::observer::ProposalStatus::Unresolved => "[UNRESOLVED]",
                    crate::observer::ProposalStatus::Escalated => "[ESCALATED]",
                    crate::observer::ProposalStatus::Addressed => "addressed",
                }
            ));
            out.push_str(&format!("   quote: {}\n\n", p.quote));
        }
    }

    if let Some(diagnostic) = &c.coder_diagnostic {
        out.push_str("--- coder_diagnostic ---\n");
        match serde_json::to_string_pretty(diagnostic) {
            Ok(json) => out.push_str(json.trim_end()),
            Err(_) => out.push_str("(unavailable)"),
        }
        out.push_str("\n\n");
    }

    if let Some(plan) = &c.benchmark_plan {
        out.push_str("--- benchmark_plan ---\n");
        match serde_json::to_string_pretty(plan) {
            Ok(json) => out.push_str(json.trim_end()),
            Err(_) => out.push_str("(unavailable)"),
        }
        out.push_str("\n\n");
    }

    out.push_str("--- critical_path ---\n");
    let cp = c.critical_path.trim();
    out.push_str(if cp.is_empty() { "none" } else { cp });
    out.push_str("\n\n--- health ---\n");
    out.push_str(&format!("score: {}\n", c.health.score));
    if !c.health.rationale.trim().is_empty() {
        out.push_str(&format!("rationale: {}\n", c.health.rationale.trim()));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proposal(
        title: &str,
        severity: Severity,
        score: u32,
        status: ProposalStatus,
        impact: &str,
    ) -> Proposal {
        Proposal {
            title: title.to_string(),
            to_coder: format!("Fix {title}."),
            severity,
            score,
            phase: DevPhase::Core,
            impact: impact.to_string(),
            cost: crate::observer::Cost::Low,
            status,
            quote: "n/a".to_string(),
            axis: Some(RiskAxis::Reliability),
        }
    }

    #[test]
    fn observer_transcript_surfaces_repo_rule_followups_for_tui_state_change() {
        let transcript = r#"
✎ patch_file: src/tui/prefs.rs
```bash
$ cargo test -q tui::events::tests::
exit: 0
```
"#;

        let critique = run_observer_from_transcript(transcript, None);
        let formatted = format_critique_as_observer_blocks(&critique);

        assert!(formatted.contains("Update state ownership docs"));
        assert!(formatted.contains("docs/state-schema.md"));
        assert!(formatted.contains("Refresh TUI replay proof"));
        assert!(formatted.contains(".spiral-coder/tui_replay.json"));
    }

    #[test]
    fn observer_transcript_surfaces_runtime_eval_followup_for_coder_loop_change() {
        let transcript = r#"
✎ patch_file: src/tui/agent/task_harness.rs
```bash
$ cargo test -q tui::agent::tests::
exit: 0
```
"#;

        let critique = run_observer_from_transcript(transcript, None);
        let formatted = format_critique_as_observer_blocks(&critique);

        assert!(formatted.contains("Refresh runtime eval proof"));
        assert!(formatted.contains(".spiral-coder/runtime_eval.json"));
        assert!(formatted.contains("--- coder_diagnostic ---"));
        assert!(formatted.contains("--- benchmark_plan ---"));
        assert!(formatted.contains("\"failure_mode\": \"missing_required_followup\""));
        assert!(formatted.contains("\"mutation_anchor\""));
        assert!(
            formatted.contains("cargo run --quiet -- eval --spec .spiral-coder/runtime_eval.json")
        );
        assert!(formatted.contains("\"lane\": \"runtime_eval\""));
    }

    #[test]
    fn display_proposals_keep_highest_priority_items_explicitly() {
        let proposals = vec![
            proposal("low info", Severity::Info, 100, ProposalStatus::New, ""),
            proposal("warn one", Severity::Warn, 70, ProposalStatus::New, ""),
            proposal("crit one", Severity::Crit, 75, ProposalStatus::New, ""),
            proposal(
                "escalated",
                Severity::Warn,
                60,
                ProposalStatus::Escalated,
                "",
            ),
            proposal(
                "unresolved",
                Severity::Warn,
                65,
                ProposalStatus::Unresolved,
                "",
            ),
            proposal(
                "addressed crit",
                Severity::Crit,
                100,
                ProposalStatus::Addressed,
                "",
            ),
        ];

        let selected = select_display_proposals(proposals);
        let titles = selected
            .iter()
            .map(|p| p.title.as_str())
            .collect::<Vec<_>>();

        assert_eq!(selected.len(), MAX_DISPLAY_PROPOSALS);
        assert_eq!(titles[0], "escalated");
        assert_eq!(titles[1], "unresolved");
        assert!(titles.contains(&"crit one"));
        assert!(!titles.contains(&"addressed crit"));
    }

    #[test]
    fn critical_path_can_include_parallel_top_blockers() {
        let proposals = vec![
            proposal(
                "Fix verification gate",
                Severity::Crit,
                90,
                ProposalStatus::New,
                "done can pass without proof",
            ),
            proposal(
                "Refresh runtime eval proof",
                Severity::Warn,
                85,
                ProposalStatus::Unresolved,
                "self-dogfood regression is unbounded",
            ),
        ];

        let critical_path = critical_path_from_proposals(&proposals);

        assert!(critical_path.contains("Fix verification gate"));
        assert!(critical_path.contains("Refresh runtime eval proof"));
        assert!(critical_path.contains("; then "));
    }

    #[test]
    fn observer_memory_becomes_analyzer_risk_for_recurring_findings() {
        let transcript = r#"
✎ patch_file: src/tui/agent/task_harness.rs
```bash
$ cargo test -q tui::agent::tests::
exit: 0
```
"#;
        let mut mem = CritiqueMemory::default();

        let first = run_observer_from_transcript(transcript, Some(&mut mem));
        assert!(!first
            .risks
            .iter()
            .any(|r| memory::is_recurring_unresolved_proposal(r.description.as_str())));

        let second = run_observer_from_transcript(transcript, Some(&mut mem));
        let formatted = format_critique_as_observer_blocks(&second);

        assert!(second
            .risks
            .iter()
            .any(|r| memory::is_recurring_unresolved_proposal(r.description.as_str())));
        assert!(formatted.contains("Resolve recurring Observer finding"));
        assert!(formatted.contains("Refresh runtime eval proof"));
    }
}
