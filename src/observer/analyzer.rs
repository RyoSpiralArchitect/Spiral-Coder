use crate::observer::detector::{Event, FileScanTool};
use crate::observer::memory;
use crate::observer::repo_rules;
use crate::observer::{Cost, DevPhase, Proposal, ProposalStatus, Risk, RiskAxis, Severity};
use std::collections::{BTreeSet, HashSet};

const RISK_NESTED_GIT: &str = "Nested git repository detected";
const RISK_LOOP: &str = "Loop/stall detected";
const RISK_SANDBOX_BREACH: &str = "Command escaped tool_root";
const RISK_DANGEROUS_COMMAND: &str = "Dangerous command detected";
const RISK_LARGE_DIFF: &str = "Large diff applied";
const RISK_MISSING_VERIFICATION: &str = "No verification command after edits";
const RISK_LARGE_OUTPUT: &str = "Excessive command output";
const RISK_BROAD_FILE_SCAN: &str = "Unbounded file scan";
const RISK_HEAVY_COMMANDS: &str = "Repeated heavy verification";
const RISK_SLOW_COMMANDS: &str = "Slow commands detected";

fn looks_like_heavy_command(command: &str) -> bool {
    let cmd = command.trim().to_ascii_lowercase();
    if cmd.is_empty() {
        return false;
    }
    // Prefer "build/test" style commands that can be slow on large repos.
    cmd.contains("cargo build")
        || cmd.contains("cargo test")
        || cmd.contains("npm test")
        || cmd.contains("pnpm test")
        || cmd.contains("yarn test")
        || cmd.contains("npm run build")
        || cmd.contains("pnpm build")
        || cmd.contains("yarn build")
        || cmd.contains("go test")
        || cmd.contains("dotnet test")
        || cmd.contains("dotnet build")
        || cmd.contains("mvn test")
        || cmd.contains("gradle test")
}

fn is_broad_glob_pattern(pattern: &str) -> bool {
    let p = pattern.trim();
    if p.is_empty() {
        return false;
    }
    if matches!(p, "*" | "**" | "**/*" | "**/*.*" | "*.*") {
        return true;
    }
    // If it contains `**` and no extension, it usually scans too much.
    p.contains("**") && !p.contains('.')
}

fn is_broad_search_pattern(pattern: &str) -> bool {
    let p = pattern.trim();
    if p.is_empty() {
        return false;
    }
    if matches!(p, "." | ".*" | "*") {
        return true;
    }
    // Single-char searches are almost always huge.
    p.chars().count() <= 1
}

fn looks_like_verification_command(command: &str) -> bool {
    let cmd = command.trim().to_ascii_lowercase();
    if cmd.is_empty() {
        return false;
    }

    // Git sanity checks.
    if cmd == "git status" || cmd.starts_with("git status ") {
        return true;
    }
    if cmd == "git diff" || cmd.starts_with("git diff ") {
        return true;
    }

    // Rust.
    if cmd.contains("cargo test")
        || cmd.contains("cargo check")
        || cmd.contains("cargo clippy")
        || cmd.contains("cargo fmt")
    {
        return true;
    }

    // Node.
    if cmd.contains("npm test")
        || cmd.contains("pnpm test")
        || cmd.contains("yarn test")
        || cmd.contains("bun test")
        || cmd.contains("vitest")
        || cmd.contains("jest")
    {
        return true;
    }

    // Python.
    if cmd.contains("pytest") || cmd.contains("python -m pytest") {
        return true;
    }

    // Go.
    if cmd.contains("go test") {
        return true;
    }

    // .NET / Java.
    if cmd.contains("dotnet test") || cmd.contains("mvn test") || cmd.contains("gradle test") {
        return true;
    }

    false
}

pub fn analyze(events: &[Event]) -> Vec<Risk> {
    let mut out: Vec<Risk> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    let mut push = |r: Risk| {
        let key = format!("{:?}|{:?}|{}", r.axis, r.severity, r.description);
        if seen.contains(&key) {
            return;
        }
        seen.insert(key);
        out.push(r);
    };

    let mut sandbox_breaches: Vec<String> = Vec::new();
    let mut dangerous_security: Vec<String> = Vec::new();
    let mut dangerous_reliability: Vec<String> = Vec::new();
    let mut large_diffs: Vec<String> = Vec::new();
    let mut large_outputs: Vec<String> = Vec::new();
    let mut broad_scans: Vec<String> = Vec::new();
    let mut scan_count: usize = 0;
    let mut heavy_commands: Vec<String> = Vec::new();
    let mut command_timings: Vec<(String, u64)> = Vec::new();

    for e in events {
        match e {
            Event::CommandExecuted { command } => {
                if looks_like_heavy_command(command) {
                    let c = command.trim();
                    if !c.is_empty() {
                        heavy_commands.push(c.to_string());
                    }
                }
            }
            Event::CommandTiming {
                command,
                duration_ms,
            } => {
                let cmd = command.trim();
                if cmd.is_empty() {
                    continue;
                }
                let cmd1 = cmd.lines().next().unwrap_or(cmd).trim();
                if cmd1.is_empty() {
                    continue;
                }
                command_timings.push((cmd1.to_string(), *duration_ms));
            }
            Event::CommandFailure { command, stderr } => {
                let stderr_low = stderr.to_ascii_lowercase();
                if stderr_low.contains("embedded git repository")
                    || stderr_low.contains("adding embedded git repository")
                {
                    push(Risk {
                        axis: RiskAxis::Correctness,
                        severity: Severity::Crit,
                        description: RISK_NESTED_GIT.to_string(),
                        evidence: Some(stderr.trim().to_string()),
                    });
                } else {
                    let cmd = command.trim();
                    let desc = if cmd.is_empty() {
                        "Command failed".to_string()
                    } else {
                        let clipped = if cmd.len() > 120 {
                            format!("{}…", &cmd[..120])
                        } else {
                            cmd.to_string()
                        };
                        format!("Command failed: {clipped}")
                    };
                    push(Risk {
                        axis: RiskAxis::Reliability,
                        severity: Severity::Warn,
                        description: desc,
                        evidence: Some(stderr.trim().to_string()),
                    });
                }
            }
            Event::SandboxBreach { command, detail } => {
                let cmd = command.trim();
                let clipped = if cmd.len() > 120 {
                    format!("{}…", &cmd[..120])
                } else {
                    cmd.to_string()
                };
                sandbox_breaches.push(format!(
                    "command: {clipped}\n{detail}",
                    detail = detail.trim()
                ));
            }
            Event::DangerousCommand { command, reason } => {
                let cmd = command.trim();
                let clipped = if cmd.len() > 120 {
                    format!("{}…", &cmd[..120])
                } else {
                    cmd.to_string()
                };
                let r = reason.trim();
                if r.starts_with("reliability:") {
                    dangerous_reliability.push(format!("command: {clipped}\nreason: {r}"));
                } else {
                    dangerous_security.push(format!("command: {clipped}\nreason: {r}"));
                }
            }
            Event::LargeDiffApplied {
                path,
                diff_chars,
                hunks,
            } => {
                let p = path.trim();
                let p = if p.len() > 160 {
                    format!("{}…", &p[..160])
                } else {
                    p.to_string()
                };
                large_diffs.push(format!(
                    "path: {p}\ndiff_chars: {diff_chars}\nhunks: {hunks}"
                ));
            }
            Event::LargeCommandOutput {
                command,
                lines_total,
                truncated_to_chars,
            } => {
                let cmd = command.trim();
                let clipped = if cmd.len() > 120 {
                    format!("{}…", &cmd[..120])
                } else {
                    cmd.to_string()
                };
                let mut meta = Vec::new();
                if let Some(n) = lines_total {
                    meta.push(format!("lines_total: {n}"));
                }
                if let Some(n) = truncated_to_chars {
                    meta.push(format!("truncated_to_chars: {n}"));
                }
                let meta = if meta.is_empty() {
                    String::new()
                } else {
                    format!("\n{}", meta.join("\n"))
                };
                large_outputs.push(format!("command: {clipped}{meta}"));
            }
            Event::FileScan { tool, pattern, dir } => {
                scan_count = scan_count.saturating_add(1);
                let dir0 = dir.trim();
                let dir0 = if dir0.is_empty() { "." } else { dir0 };
                match tool {
                    FileScanTool::Glob => {
                        if is_broad_glob_pattern(pattern) {
                            broad_scans.push(format!(
                                "glob: {pattern}\ndir: {dir0}",
                                pattern = pattern.trim()
                            ));
                        }
                    }
                    FileScanTool::SearchFiles => {
                        if is_broad_search_pattern(pattern) {
                            broad_scans.push(format!(
                                "search_files: {pattern}\ndir: {dir0}",
                                pattern = pattern.trim()
                            ));
                        }
                    }
                    FileScanTool::ListDir => {
                        // list_dir is generally cheap; ignore for now.
                    }
                }
            }
            Event::LoopDetected { reason } => {
                push(Risk {
                    axis: RiskAxis::Reliability,
                    severity: Severity::Warn,
                    description: RISK_LOOP.to_string(),
                    evidence: Some(reason.clone()),
                });
            }
            _ => {}
        }
    }

    if !sandbox_breaches.is_empty() {
        push(Risk {
            axis: RiskAxis::Reliability,
            severity: Severity::Crit,
            description: RISK_SANDBOX_BREACH.to_string(),
            evidence: Some(sandbox_breaches.join("\n\n")),
        });
    }
    if !dangerous_security.is_empty() {
        push(Risk {
            axis: RiskAxis::Security,
            severity: Severity::Crit,
            description: RISK_DANGEROUS_COMMAND.to_string(),
            evidence: Some(dangerous_security.join("\n\n")),
        });
    }
    if !dangerous_reliability.is_empty() {
        push(Risk {
            axis: RiskAxis::Reliability,
            severity: Severity::Warn,
            description: RISK_DANGEROUS_COMMAND.to_string(),
            evidence: Some(dangerous_reliability.join("\n\n")),
        });
    }
    if !large_diffs.is_empty() {
        push(Risk {
            axis: RiskAxis::Maintainability,
            severity: Severity::Warn,
            description: RISK_LARGE_DIFF.to_string(),
            evidence: Some(large_diffs.join("\n\n")),
        });
    }
    if !large_outputs.is_empty() {
        let evidence = large_outputs
            .iter()
            .take(3)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n\n");
        push(Risk {
            axis: RiskAxis::Performance,
            severity: Severity::Warn,
            description: RISK_LARGE_OUTPUT.to_string(),
            evidence: Some(evidence),
        });
    }
    if !broad_scans.is_empty() || scan_count >= 6 {
        let mut evidence: Vec<String> = Vec::new();
        if scan_count >= 6 {
            evidence.push(format!("scan_count: {}", scan_count));
        }
        evidence.extend(broad_scans.iter().take(4).cloned());
        push(Risk {
            axis: RiskAxis::Performance,
            severity: if broad_scans.len() >= 2 || scan_count >= 8 {
                Severity::Warn
            } else {
                Severity::Info
            },
            description: RISK_BROAD_FILE_SCAN.to_string(),
            evidence: Some(evidence.join("\n\n")),
        });
    }
    if heavy_commands.len() >= 3 {
        let mut uniq: Vec<String> = Vec::new();
        for c in &heavy_commands {
            if uniq.len() >= 3 {
                break;
            }
            if !uniq.contains(c) {
                uniq.push(c.clone());
            }
        }
        push(Risk {
            axis: RiskAxis::Performance,
            severity: if heavy_commands.len() >= 5 {
                Severity::Warn
            } else {
                Severity::Info
            },
            description: RISK_HEAVY_COMMANDS.to_string(),
            evidence: Some(format!(
                "count: {}\nexamples:\n- {}",
                heavy_commands.len(),
                uniq.join("\n- ")
            )),
        });
    }

    if !command_timings.is_empty() {
        let mut sorted = command_timings;
        sorted.sort_by(|a, b| b.1.cmp(&a.1));
        let max_ms = sorted.first().map(|x| x.1).unwrap_or(0);

        // Heuristic: show a risk only when at least one command is meaningfully slow.
        if max_ms >= 2000 {
            let sev = if max_ms >= 10_000
                || sorted.iter().take(3).filter(|(_, ms)| *ms >= 5000).count() >= 2
            {
                Severity::Warn
            } else {
                Severity::Info
            };

            let mut evidence_lines: Vec<String> = Vec::new();
            evidence_lines.push(format!("count: {}", sorted.len()));
            evidence_lines.push(format!("max_ms: {}", max_ms));
            evidence_lines.push("top:".to_string());
            for (cmd, ms) in sorted.iter().take(3) {
                let clipped = if cmd.len() > 120 {
                    format!("{}…", &cmd[..120])
                } else {
                    cmd.to_string()
                };
                evidence_lines.push(format!("- {clipped} ({ms}ms)"));
            }
            push(Risk {
                axis: RiskAxis::Performance,
                severity: sev,
                description: RISK_SLOW_COMMANDS.to_string(),
                evidence: Some(evidence_lines.join("\n")),
            });
        }
    }

    let last_edit = events.iter().rposition(|e| {
        matches!(
            e,
            Event::FileWritten { .. } | Event::LargeDiffApplied { .. }
        )
    });
    if let Some(idx) = last_edit {
        let has_verify_after = events.iter().skip(idx.saturating_add(1)).any(|e| match e {
            Event::CommandExecuted { command } => looks_like_verification_command(command),
            _ => false,
        });
        if !has_verify_after {
            let has_failures = events
                .iter()
                .any(|e| matches!(e, Event::CommandFailure { .. }));
            push(Risk {
                axis: RiskAxis::Reliability,
                severity: if has_failures {
                    Severity::Warn
                } else {
                    Severity::Info
                },
                description: RISK_MISSING_VERIFICATION.to_string(),
                evidence: Some(
                    "No tests/build/status command observed after the last edit.".to_string(),
                ),
            });
        }
    }

    for risk in repo_rules::detect(events) {
        push(risk);
    }

    out
}

pub fn generate_proposals(risks: &[Risk]) -> Vec<Proposal> {
    let mut out: Vec<Proposal> = Vec::new();
    let mut seen_titles: BTreeSet<String> = BTreeSet::new();

    let mut push = |p: Proposal| {
        let k = p.title.trim().to_ascii_lowercase();
        if k.is_empty() || seen_titles.contains(&k) {
            return;
        }
        seen_titles.insert(k);
        out.push(p);
    };

    for r in risks {
        if r.description == RISK_NESTED_GIT {
            push(Proposal {
                title: "Fix nested git repository".to_string(),
                to_coder: "Remove the accidentally nested `.git` directory (or convert it into a proper submodule), then retry `git add`. Do not commit a nested repo by accident.".to_string(),
                severity: Severity::Crit,
                score: 0,
                phase: DevPhase::Core,
                impact: "Git index/history can be corrupted; future diffs and CI become unreliable.".to_string(),
                cost: Cost::Low,
                status: ProposalStatus::New,
                quote: r
                    .evidence
                    .clone()
                    .unwrap_or_else(|| "embedded git repository".to_string())
                    .chars()
                    .take(40)
                    .collect(),
                axis: Some(r.axis),
            });
            continue;
        }

        if r.description == RISK_LOOP {
            push(Proposal {
                title: "Break the loop with diagnostics".to_string(),
                to_coder: "Stop repeating the same action. Run `pwd`, `ls`, and `git status` (or equivalent), then choose a materially different next step based on what you learn.".to_string(),
                severity: Severity::Warn,
                score: 0,
                phase: DevPhase::Core,
                impact: "Agent can burn iterations without making progress; risks accidental damage by blind retries.".to_string(),
                cost: Cost::Low,
                status: ProposalStatus::New,
                quote: r
                    .evidence
                    .clone()
                    .unwrap_or_else(|| "loop detected".to_string())
                    .chars()
                    .take(40)
                    .collect(),
                axis: Some(r.axis),
            });
            continue;
        }

        if memory::is_recurring_unresolved_proposal(r.description.as_str()) {
            push(Proposal {
                title: "Resolve recurring Observer finding".to_string(),
                to_coder: "A previous Observer finding has appeared again. Treat it as blocking: explicitly accept, override, or defer it, then make the smallest concrete patch or verification follow-up that closes the repeated finding before moving on.".to_string(),
                severity: r.severity,
                score: 0,
                phase: DevPhase::Core,
                impact: "Repeated unresolved findings indicate the Coder/Observer loop is not converting critique into durable progress.".to_string(),
                cost: Cost::Low,
                status: ProposalStatus::New,
                quote: r
                    .evidence
                    .clone()
                    .unwrap_or_else(|| "recurring proposal".to_string())
                    .chars()
                    .take(60)
                    .collect(),
                axis: Some(r.axis),
            });
            continue;
        }

        if r.description == RISK_SANDBOX_BREACH {
            push(Proposal {
                title: "Stay under tool_root".to_string(),
                to_coder: "Re-run commands under tool_root only. Avoid absolute paths and `cd ..`. First run `pwd` and `ls`, then retry the command with a safe relative path.".to_string(),
                severity: Severity::Crit,
                score: 0,
                phase: DevPhase::Core,
                impact: "Escaping tool_root can modify the wrong directory and cause irreversible repo damage.".to_string(),
                cost: Cost::Low,
                status: ProposalStatus::New,
                quote: r
                    .evidence
                    .clone()
                    .unwrap_or_else(|| "SANDBOX BREACH".to_string())
                    .chars()
                    .take(40)
                    .collect(),
                axis: Some(r.axis),
            });
            continue;
        }

        if r.description == RISK_DANGEROUS_COMMAND {
            let sev = r.severity;
            push(Proposal {
                title: "Avoid dangerous commands".to_string(),
                to_coder: "Do not run destructive or remote-script commands blindly. If truly intended, explain the goal and ask for explicit user confirmation, then use the safest equivalent command targeting a specific path.".to_string(),
                severity: sev,
                score: 0,
                phase: DevPhase::Core,
                impact: "Risk of data loss (rm/reset/clean/force) or remote code execution (curl|sh/iex).".to_string(),
                cost: Cost::Low,
                status: ProposalStatus::New,
                quote: r
                    .evidence
                    .clone()
                    .unwrap_or_else(|| "dangerous command".to_string())
                    .chars()
                    .take(40)
                    .collect(),
                axis: Some(r.axis),
            });
            continue;
        }

        if r.description == RISK_LARGE_DIFF {
            push(Proposal {
                title: "Split large diffs".to_string(),
                to_coder: "Break large apply_diff edits into smaller, reviewable hunks. Prefer 1–3 hunks per tool call, and verify each step with a quick build/test before continuing.".to_string(),
                severity: Severity::Warn,
                score: 0,
                phase: DevPhase::Feature,
                impact: "Large diffs are hard to review, easier to misapply, and often hide regressions.".to_string(),
                cost: Cost::Low,
                status: ProposalStatus::New,
                quote: r
                    .evidence
                    .clone()
                    .unwrap_or_else(|| "large diff".to_string())
                    .chars()
                    .take(40)
                    .collect(),
                axis: Some(r.axis),
            });
            continue;
        }

        if r.description == RISK_MISSING_VERIFICATION {
            push(Proposal {
                title: "Run verification after edits".to_string(),
                to_coder: "After editing files, run one verification command: `git status` and your project’s tests/build (e.g. `cargo test`, `npm test`, `pytest`). Only proceed if it’s green.".to_string(),
                severity: r.severity,
                score: 0,
                phase: DevPhase::Polish,
                impact: "Unverified edits can silently break builds/tests and waste iterations later.".to_string(),
                cost: Cost::Low,
                status: ProposalStatus::New,
                quote: "no tests/build/status".to_string(),
                axis: Some(r.axis),
            });
            continue;
        }

        if r.description == RISK_LARGE_OUTPUT {
            push(Proposal {
                title: "Reduce command output volume".to_string(),
                to_coder: "Avoid commands that print massive output. Add filters/quiet flags, or pipe through a line limiter (e.g. head/Select-Object) so the runtime stays responsive and the signal is visible.".to_string(),
                severity: r.severity,
                score: 0,
                phase: DevPhase::Polish,
                impact: "Huge stdout/stderr slows iteration, hides the real error, and can cause truncation/timeout.".to_string(),
                cost: Cost::Low,
                status: ProposalStatus::New,
                quote: "…truncated".to_string(),
                axis: Some(r.axis),
            });
            continue;
        }

        if r.description == RISK_BROAD_FILE_SCAN {
            push(Proposal {
                title: "Narrow file scans".to_string(),
                to_coder: "Avoid unbounded `glob`/`search_files` across the whole repo. Limit `dir` (e.g. `src/`) and narrow patterns (extensions, exact tokens) before scanning again.".to_string(),
                severity: r.severity,
                score: 0,
                phase: DevPhase::Feature,
                impact: "Broad scans waste time on large trees and can return truncated/noisy results.".to_string(),
                cost: Cost::Low,
                status: ProposalStatus::New,
                quote: "glob: **/*".to_string(),
                axis: Some(r.axis),
            });
            continue;
        }

        if r.description == RISK_HEAVY_COMMANDS {
            push(Proposal {
                title: "Use faster verification loops".to_string(),
                to_coder: "If you’re repeatedly running full builds/tests, switch to cheaper checks first (e.g. `cargo check`, targeted tests), then run the full suite only when the fix is likely correct.".to_string(),
                severity: r.severity,
                score: 0,
                phase: DevPhase::Polish,
                impact: "Repeated full builds/tests can dominate iteration time and reduce overall throughput.".to_string(),
                cost: Cost::Low,
                status: ProposalStatus::New,
                quote: "cargo test".to_string(),
                axis: Some(r.axis),
            });
            continue;
        }

        if r.description == RISK_SLOW_COMMANDS {
            push(Proposal {
                title: "Optimize slow commands".to_string(),
                to_coder: "Identify which commands are slow, then narrow scope or use cheaper alternatives first (targeted tests, incremental builds, smaller search dirs). Keep verification fast until you’re confident, then run the full suite.".to_string(),
                severity: r.severity,
                score: 0,
                phase: DevPhase::Polish,
                impact: "Slow commands reduce iteration throughput and can trigger timeouts/truncation.".to_string(),
                cost: Cost::Low,
                status: ProposalStatus::New,
                quote: r
                    .evidence
                    .clone()
                    .unwrap_or_else(|| "duration_ms".to_string())
                    .chars()
                    .take(40)
                    .collect(),
                axis: Some(r.axis),
            });
            continue;
        }

        if repo_rules::is_missing_state_schema_doc(r.description.as_str()) {
            push(Proposal {
                title: "Update state ownership docs".to_string(),
                to_coder: "This change touched a state-owner file. Update `docs/state-schema.md` in the same change so the owner and persistence boundary stay explicit.".to_string(),
                severity: r.severity,
                score: 0,
                phase: DevPhase::Core,
                impact: "State changes drift quickly when the code and ownership docs diverge, which makes self-dogfooding harder to trust.".to_string(),
                cost: Cost::Low,
                status: ProposalStatus::New,
                quote: r
                    .evidence
                    .clone()
                    .unwrap_or_else(|| "docs/state-schema.md".to_string())
                    .chars()
                    .take(60)
                    .collect(),
                axis: Some(r.axis),
            });
            continue;
        }

        if repo_rules::is_missing_tui_replay_proof(r.description.as_str()) {
            push(Proposal {
                title: "Refresh TUI replay proof".to_string(),
                to_coder: "This change touched TUI-visible or TUI-control-flow code. Run `cargo run -- ... tui-replay --spec .spiral-coder/tui_replay.json` and keep the proof green before calling the work done.".to_string(),
                severity: r.severity,
                score: 0,
                phase: DevPhase::Core,
                impact: "Without replay proof, TUI regressions can slip through even when targeted unit tests are green.".to_string(),
                cost: Cost::Low,
                status: ProposalStatus::New,
                quote: r
                    .evidence
                    .clone()
                    .unwrap_or_else(|| "tui-replay".to_string())
                    .chars()
                    .take(60)
                    .collect(),
                axis: Some(r.axis),
            });
            continue;
        }

        if repo_rules::is_missing_runtime_eval_proof(r.description.as_str()) {
            push(Proposal {
                title: "Refresh runtime eval proof".to_string(),
                to_coder: "This change touched coder-loop/runtime code. Run `cargo run -- ... eval --spec .spiral-coder/runtime_eval.json` for the affected case before closeout so the harness proof stays current.".to_string(),
                severity: r.severity,
                score: 0,
                phase: DevPhase::Core,
                impact: "Coder-loop changes without runtime-eval proof make self-dogfooding fragile because regressions show up only in live runs later.".to_string(),
                cost: Cost::Low,
                status: ProposalStatus::New,
                quote: r
                    .evidence
                    .clone()
                    .unwrap_or_else(|| "runtime_eval.json".to_string())
                    .chars()
                    .take(60)
                    .collect(),
                axis: Some(r.axis),
            });
            continue;
        }
    }

    out
}
