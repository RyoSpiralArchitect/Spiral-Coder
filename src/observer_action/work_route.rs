//! Whole-task planning facts derived from native obligations. A route is advice,
//! not a new plan, completion certificate, request allowance or execution gate.
use crate::tui::suggestion::ObserverSuggestionEnvelope;
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Serialize)]
struct Listed<T> {
    items: Vec<T>,
    total: usize,
}

impl<T> Listed<T> {
    fn new(items: impl IntoIterator<Item = T>, limit: usize) -> Self {
        let all: Vec<_> = items.into_iter().collect();
        Self {
            total: all.len(),
            items: all.into_iter().take(limit).collect(),
        }
    }
}

#[derive(Debug, Serialize)]
struct Excerpt {
    text: String,
    truncated: bool,
}

impl Excerpt {
    fn new(text: &str, limit: usize) -> Self {
        Self {
            text: text.chars().take(limit).collect(),
            truncated: text.chars().count() > limit,
        }
    }
}

#[derive(Debug, Serialize)]
struct Artifact<'a> {
    path: &'a str,
    kinds: BTreeSet<&'a str>,
    requirement_ids: BTreeSet<&'a str>,
    native_edit_recorded: Option<bool>,
}

#[derive(Debug, Serialize)]
struct Case<'a> {
    id: &'a str,
    description: Excerpt,
}

#[derive(Debug, Serialize)]
struct TestReview<'a> {
    dedicated_cases: Listed<Case<'a>>,
    shared_behavior_rules: Listed<Case<'a>>,
    current_source_available: bool,
}

#[derive(Debug, Serialize)]
struct Verification<'a> {
    pending_baseline_checks: Option<Listed<&'a str>>,
    pending_now: Listed<&'a str>,
    checks_after_next_edit: Listed<&'a str>,
    current_checks_invalidated_by_edit: usize,
    minimum_edit_then_finish_requests: Option<usize>,
    requests_before_full_sweep_and_report: Option<usize>,
    full_sweep_and_report_shortfall: Option<usize>,
}

#[derive(Debug, Serialize)]
struct Route<'a> {
    version: u8,
    current_prerequisites: Vec<&'static str>,
    artifacts: Option<Listed<Artifact<'a>>>,
    test_review: Option<TestReview<'a>>,
    verification: Option<Verification<'a>>,
    report_outcome_rows: Option<Listed<&'a str>>,
}

fn strings(value: &Value) -> Option<BTreeSet<&str>> {
    value.as_array()?.iter().map(Value::as_str).collect()
}

fn artifacts(obligations: &Value) -> Option<Listed<Artifact<'_>>> {
    let status = &obligations["task_requirements"];
    if status["declared"] != true {
        return None;
    }
    let pending = strings(&status["pending_artifact_paths"]);
    let mut paths: BTreeMap<&str, Artifact<'_>> = BTreeMap::new();
    for row in status["requirements"].as_array()? {
        let requirement = &row["requirement"];
        let id = requirement["id"].as_str()?;
        for item in requirement["artifacts"].as_array()? {
            let path = item["path"].as_str()?;
            let kind = item["kind"].as_str()?;
            let artifact = paths.entry(path).or_insert_with(|| Artifact {
                path,
                kinds: BTreeSet::new(),
                requirement_ids: BTreeSet::new(),
                native_edit_recorded: pending.as_ref().map(|p| !p.contains(path)),
            });
            artifact.kinds.insert(kind);
            artifact.requirement_ids.insert(id);
        }
    }
    Some(Listed::new(paths.into_values(), 16))
}

fn cases(value: &Value) -> Listed<Case<'_>> {
    Listed::new(
        value.as_array().into_iter().flatten().filter_map(|row| {
            Some(Case {
                id: row["id"].as_str()?,
                description: Excerpt::new(row["description"].as_str()?, 1200),
            })
        }),
        12,
    )
}

fn verification<'a>(obligations: &'a Value, next: Option<usize>) -> Option<Verification<'a>> {
    let required = strings(&obligations["required_exact_checks"])?;
    let pending = strings(&obligations["pending_exact_checks"])?;
    if required.is_empty() && pending.is_empty() {
        return None;
    }
    let all: BTreeSet<_> = required.union(&pending).copied().collect();
    let reserve = all.len().saturating_add(1);
    let edits = strings(&obligations["task_requirements"]["pending_artifact_paths"]);
    let baselines = strings(&obligations["task_requirements"]["pending_baselines"]);
    Some(Verification {
        current_checks_invalidated_by_edit: all.difference(&pending).count(),
        minimum_edit_then_finish_requests: edits.as_ref().zip(baselines.as_ref()).map(|(e, b)| {
            reserve
                .saturating_add(e.len().max(1))
                .saturating_add(b.len())
        }),
        pending_baseline_checks: baselines.map(|checks| Listed::new(checks, 12)),
        requests_before_full_sweep_and_report: next.map(|n| n.saturating_sub(reserve)),
        full_sweep_and_report_shortfall: next.map(|n| reserve.saturating_sub(n)),
        pending_now: Listed::new(pending, 12),
        checks_after_next_edit: Listed::new(all, 12),
    })
}

fn project(packet: &Value) -> Route<'_> {
    let o = &packet["task_obligations"];
    let mut prerequisites = Vec::new();
    for (name, applies) in [
        (
            "future_iteration_only",
            packet["work_budget"]["next_coder_turn"] == "exhausted",
        ),
        (
            "native_report_turn",
            packet["work_budget"]["next_coder_turn"] == "report_only",
        ),
        (
            "required_diagnostic_read",
            o["required_diagnostic_read"].is_object(),
        ),
        ("required_recheck", o["required_recheck"].is_string()),
        ("report_repair", o["report_repair"].is_object()),
        (
            "pending_baselines",
            o["task_requirements"]["pending_baselines"]
                .as_array()
                .is_some_and(|v| !v.is_empty()),
        ),
        (
            "pending_recovery",
            o["pending_recovery"]
                .as_array()
                .is_some_and(|v| !v.is_empty()),
        ),
    ] {
        if applies {
            prerequisites.push(name);
        }
    }
    let tests = &o["task_requirements"]["authored_tests"];
    Route {
        version: 1,
        current_prerequisites: prerequisites,
        artifacts: artifacts(o),
        test_review: tests.as_object().map(|_| TestReview {
            dedicated_cases: cases(&tests["test_only_requirements"]),
            shared_behavior_rules: cases(&tests["shared_requirements"]),
            current_source_available: tests["review_source"]["source"].is_string(),
        }),
        verification: verification(
            o,
            packet["work_budget"]["next_coder_requests"]
                .as_u64()
                .and_then(|n| usize::try_from(n).ok()),
        ),
        report_outcome_rows: o["final_answer"]["outcome_rows"]
            .as_array()
            .and_then(|rows| rows.iter().map(Value::as_str).collect::<Option<Vec<_>>>())
            .map(|rows| Listed::new(rows, 8)),
    }
}

pub(crate) fn prompt(packet: &Value) -> String {
    let route = project(packet);
    format!(
        "[Whole-task route]\n{}\nPlan backward from the human's requested behavior and a truthful final report. This is a native obligation map, not a semantic assessment: an edit receipt, a test function or a passing check does not establish coverage of a requirement. Reassess the current strategy against the whole task, including implementation, public tests, documentation and final evidence. Identify the most consequential unfinished or uncertain requirement.\n\
Choose one achievable milestone and its next action. Compare continuing the current route with one feasible alternative, using the evidence each would produce and the requests left. Honor current_prerequisites and the native offered tools. Keep the admitted goal and acceptance criteria; this is not a request to replan on every turn.\n\
Review validation rules in both directions. From the declared contract and observed producer/schema, identify the unit that owns each identity or counter and where it resets. A negative test establishes a rejection for one input; it does not establish that independently valid units can still be composed. Look for one missing positive composition control across an ownership boundary, such as combining valid records, sessions or batches under the documented aggregation rules. Do not silently extend a local uniqueness rule across owners or retain per-unit state across a reset. If the ownership is unclear, identify the precise producer or schema observation needed. Prefer one scope-relevant control with a decision consequence over expanding into a general review. These are questions for source-grounded reasoning, not new requirements or inferred defects.\n\
Before more implementation or a final check sweep, inspect the test_review cases against the current test source. If tests have not been written, consider a small public behavior test for the next implementation milestone now; if they exist, examine a passing control and one changed invariant per negative case. Explain any deferred work. The budget counts are optimistic request lower bounds, not time estimates: unknown diagnosis, reads, failed attempts and semantic test gaps cost extra. checks_after_next_edit includes checks that a further edit would invalidate. Reserve room for those checks and one authored report after the last mutation. If finishing does not fit, choose a useful advance and a concrete continuation instead of implying completion.\n\
In the primary reason, name the current evidence, the milestone this action unlocks and what result would change the next action. On general turns, use why_this_first for the short route from that milestone through the remaining deliverables; use fallback for a specific observed failure condition and a different feasible route. These existing optional fields reach the Coder as advice. On diagnostic or exhausted turns, keep the restricted output schema and place the relevant milestone or continuation in the reason. Carry each report_outcome_rows item verbatim on its own line in the eventual summary.\n",
        crate::tui::agent::context_compaction::render(&serde_json::to_value(route).expect("route serializes"))
    )
}

/// Carry the Observer's existing strategy fields across the same validated,
/// confidence-gated hint as its primary action. No extra action is scheduled.
pub(crate) fn coder_context(env: &ObserverSuggestionEnvelope) -> String {
    if env.why_this_first.trim().is_empty() && env.fallback.trim().is_empty() {
        return String::new();
    }
    let route = serde_json::json!({
        "why_this_first": Excerpt::new(&env.why_this_first, 700),
        "fallback": Excerpt::new(&env.fallback, 700),
    });
    format!("Observer route (quoted advice, not native evidence or a queued action): {route}\nCheck its conditions against the current results before choosing a new action; current scope, diagnosis, approvals and completion checks still apply.\n")
}

#[cfg(test)]
mod tests;
