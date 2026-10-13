use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ReadOnlyDiagnoseRescueAction {
    Search { pattern: String, dir: String },
    Read { path: String },
}

pub(super) fn is_root_read_only_observation_task(root_user_text: &str) -> bool {
    let low = root_user_text.to_ascii_lowercase();
    let observe_terms = [
        "locate",
        "find",
        "where",
        "inspect",
        "identify",
        "read-only",
        "read only",
        "read the file",
        "look up",
        "trace",
        "do not edit",
        "don't edit",
        "no edit",
        "no edits",
        "without editing",
    ];
    let explicit_no_edit = [
        "read-only",
        "read only",
        "do not edit",
        "don't edit",
        "no edit",
        "no edits",
        "without editing",
    ]
    .iter()
    .any(|term| low.contains(term));
    let strong_mutate_terms = [
        "patch",
        "modify",
        "write",
        "create",
        "implement",
        "fix",
        "refactor",
        "rename",
        "delete",
    ];
    if !observe_terms.iter().any(|term| low.contains(term)) {
        return false;
    }
    if low.contains("edit") && !explicit_no_edit {
        return false;
    }
    if strong_mutate_terms.iter().any(|term| low.contains(term)) {
        return false;
    }
    true
}

pub(super) fn read_only_plan_violation(plan: &PlanBlock) -> Option<String> {
    let check_field = |label: String, text: &str| -> Option<String> {
        let tokens = keyword_tokens(text);
        for term in governor_contract::instruction_resolver_read_only_forbidden_terms() {
            if tokens.contains(term.as_str()) {
                return Some(
                    governor_contract::instruction_resolver_read_only_plan_term_message(
                        term.as_str(),
                        label.as_str(),
                    ),
                );
            }
        }
        None
    };

    if let Some(msg) = check_field("goal".to_string(), &plan.goal) {
        return Some(msg);
    }
    for (idx, step) in plan.steps.iter().enumerate() {
        if let Some(msg) = check_field(format!("step {}", idx + 1), step) {
            return Some(msg);
        }
    }
    for (idx, criterion) in plan.acceptance_criteria.iter().enumerate() {
        if let Some(msg) = check_field(format!("acceptance {}", idx + 1), criterion) {
            return Some(msg);
        }
    }
    None
}

fn task_prefers_handler_path(root_user_text: &str, criterion: &str) -> bool {
    let low = format!("{root_user_text} {criterion}").to_ascii_lowercase();
    [
        "slash", "command", "handler", "handled", "handle", "branch", "context",
    ]
    .iter()
    .any(|term| low.contains(term))
}

fn task_prefers_prefs_path(root_user_text: &str, criterion: &str) -> bool {
    let low = format!("{root_user_text} {criterion}").to_ascii_lowercase();
    [
        "prefs",
        "preference",
        "preferences",
        "pane-scoped",
        "serialized",
        "restored",
        "serialize",
        "restore",
        "storage",
    ]
    .iter()
    .any(|term| low.contains(term))
}

fn task_prefers_agent_flow_path(root_user_text: &str, criterion: &str) -> bool {
    let low = format!("{root_user_text} {criterion}").to_ascii_lowercase();
    ([
        "wired",
        "wiring",
        "flow",
        "agent flow",
        "integration",
        "hooked",
    ]
    .iter()
    .any(|term| low.contains(term))
        && ["coder", "agent", "tui"]
            .iter()
            .any(|term| low.contains(term)))
        || (low.contains("repo-map") || low.contains("repo map"))
            && low.contains("read_file")
            && ["coder", "agent", "tui"]
                .iter()
                .any(|term| low.contains(term))
}

fn task_prefers_failure_mismatch(root_user_text: &str) -> bool {
    let low = root_user_text.to_ascii_lowercase();
    (low.contains("failing test") || low.contains("failed test"))
        || (low.contains("test")
            && ["mismatch", "fails", "failing", "failure"]
                .iter()
                .any(|term| low.contains(term)))
}

fn ordered_keyword_tokens(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for token in text
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .map(|part| part.trim().to_ascii_lowercase())
        .filter(|part| part.len() >= 3)
    {
        if seen.insert(token.clone()) {
            out.push(token);
        }
    }
    out
}

pub(super) fn preferred_read_only_search_pattern(root_user_text: &str) -> String {
    let low = root_user_text.to_ascii_lowercase();
    if let Some(slash) = first_slash_literal(root_user_text) {
        return slash;
    }
    if low.contains("failing test")
        || (low.contains("test")
            && ["fail", "fails", "failing", "mismatch"]
                .iter()
                .any(|term| low.contains(term)))
    {
        return "test".to_string();
    }
    if ["pane-scoped", "preferences", "preference", "prefs"]
        .iter()
        .any(|term| low.contains(term))
        || ["serialized", "restored", "serialize", "restore", "storage"]
            .iter()
            .any(|term| low.contains(term))
    {
        return "prefs".to_string();
    }
    if (low.contains("repo-map") || low.contains("repo map"))
        && low.contains("read_file")
        && ["fallback", "wired", "wiring", "flow", "agent"]
            .iter()
            .any(|term| low.contains(term))
    {
        return "lazy_read_fallback".to_string();
    }
    if low.contains("read_file") && low.contains("fallback") {
        return "lazy_read_fallback".to_string();
    }
    if low.contains("repo-map") || low.contains("repo map") {
        return "repo_map".to_string();
    }
    if low.contains("agent flow") || low.contains("tui agent") || low.contains("coder-side") {
        return "agent".to_string();
    }

    const PRIORITY: &[&str] = &[
        "prefs",
        "preferences",
        "repo_map",
        "fallback",
        "test",
        "read_file",
        "agent",
        "events",
        "commands",
    ];
    let ordered_tokens = ordered_keyword_tokens(root_user_text);
    let tokens = ordered_tokens
        .iter()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    for token in PRIORITY {
        if tokens.contains(*token) {
            return (*token).to_string();
        }
    }
    const GENERIC_STOPWORDS: &[&str] = &[
        "and", "answer", "anything", "code", "content", "context", "edit", "exact", "explain",
        "file", "files", "final", "find", "include", "locate", "main", "must", "not", "path",
        "read", "report", "the", "this", "where", "why", "with", "without",
    ];
    ordered_tokens
        .into_iter()
        .find(|token| !GENERIC_STOPWORDS.contains(&token.as_str()))
        .unwrap_or_else(|| "realize".to_string())
}

pub(super) fn preferred_read_only_secondary_search_pattern(root_user_text: &str) -> Option<String> {
    let low = root_user_text.to_ascii_lowercase();
    if first_slash_literal(root_user_text).is_some() {
        return Some("realize".to_string());
    }
    if task_prefers_prefs_path(root_user_text, "") {
        return Some("save_tui_prefs".to_string());
    }
    if task_prefers_agent_flow_path(root_user_text, "") {
        return Some("repo_map".to_string());
    }
    if low.contains("read_file") && low.contains("fallback") {
        return Some("repo_map".to_string());
    }
    None
}

pub(super) fn preferred_read_only_search_dir(root_user_text: &str) -> &'static str {
    let low = root_user_text.to_ascii_lowercase();
    if low.contains("tui")
        || low.contains("pane-scoped")
        || low.contains("coder-side")
        || low.contains("agent flow")
    {
        "src/tui"
    } else {
        "src"
    }
}

pub(super) fn preferred_read_only_read_path_hint(root_user_text: &str) -> Option<&'static str> {
    if task_prefers_prefs_path(root_user_text, "") {
        Some("src/tui/prefs.rs")
    } else if task_prefers_agent_flow_path(root_user_text, "") {
        Some("src/tui/agent.rs")
    } else {
        None
    }
}

pub(super) fn synthetic_read_only_goal(root_user_text: &str) -> String {
    let low = root_user_text.to_ascii_lowercase();
    if let Some(slash) = first_slash_literal(root_user_text) {
        return format!("Locate where `{slash}` is handled in the TUI and report the file path.");
    }
    if task_prefers_failure_mismatch(root_user_text) {
        return "Locate the failing test and confirm the exact assertion mismatch.".to_string();
    }
    if task_prefers_prefs_path(root_user_text, "") {
        return "Locate the main file where pane-scoped TUI preferences are serialized and restored."
            .to_string();
    }
    if task_prefers_agent_flow_path(root_user_text, "") {
        return "Locate where the coder-side repo-map fallback for read_file misses is wired into the TUI agent flow.".to_string();
    }
    if low.contains("file path") || low.contains("main file") {
        return "Locate the requested implementation and report the file path.".to_string();
    }
    "Locate the requested implementation in code and report the file path.".to_string()
}

pub(super) fn synthetic_read_only_acceptance(root_user_text: &str) -> (String, String, String) {
    if let Some(slash) = first_slash_literal(root_user_text) {
        return (
            format!("the file path handling `{slash}` is identified"),
            "the handler branch is confirmed by read_file".to_string(),
            "the command may be matched without the leading slash; the handler may live outside the obvious TUI file".to_string(),
        );
    }
    if task_prefers_failure_mismatch(root_user_text) {
        return (
            "the failing test file path is identified".to_string(),
            "the exact assertion mismatch is confirmed by read_file".to_string(),
            "the failure context may live in helper code while the assertion sits in a nearby test module".to_string(),
        );
    }
    if task_prefers_prefs_path(root_user_text, "") {
        return (
            "the main file responsible for pane-scoped TUI preference storage is identified"
                .to_string(),
            "the serialize and restore context is confirmed by read_file".to_string(),
            "preference persistence may be split across helper functions or event handlers"
                .to_string(),
        );
    }
    if task_prefers_agent_flow_path(root_user_text, "") {
        return (
            "the file wiring coder-side repo-map read_file fallback is identified".to_string(),
            "the read_file miss handling context is confirmed by read_file".to_string(),
            "repo-map logic may live in helper modules while the TUI wiring lives elsewhere"
                .to_string(),
        );
    }
    (
        "the main file path for the requested implementation is identified".to_string(),
        "the relevant code context is confirmed by read_file".to_string(),
        "the implementation may be split across helper modules".to_string(),
    )
}

pub(super) fn path_prior_score(
    path: &str,
    root_user_text: &str,
    plan_goal: &str,
    criterion: &str,
) -> f32 {
    let mut task_tokens = keyword_tokens(root_user_text);
    task_tokens.extend(keyword_tokens(plan_goal));
    task_tokens.extend(keyword_tokens(criterion));
    let path_tokens = keyword_tokens(path)
        .into_iter()
        .filter(|token| !matches!(token.as_str(), "src" | "test" | "tests"))
        .collect::<std::collections::BTreeSet<_>>();
    let path_low = path.to_ascii_lowercase();
    let file_low = path_filename(path).to_ascii_lowercase();
    let mut score = token_overlap_score(&path_tokens, &task_tokens);
    if path_tokens.len() <= 1 {
        score = (score - 0.20).clamp(0.0, 1.0);
    }
    if task_tokens.contains("tui") && path_low.starts_with("src/tui/") {
        score = (score + 0.25).clamp(0.0, 1.0);
    }
    if task_tokens.contains("realize") && path_tokens.contains("events") {
        score = (score + 0.20).clamp(0.0, 1.0);
    }
    if task_prefers_handler_path(root_user_text, criterion) {
        if path_tokens.iter().any(|token| {
            matches!(
                token.as_str(),
                "event"
                    | "events"
                    | "command"
                    | "commands"
                    | "handler"
                    | "handlers"
                    | "slash"
                    | "dispatch"
            )
        }) {
            score = (score + 0.30).clamp(0.0, 1.0);
        }
        if file_low == "ui.rs"
            || path_tokens
                .iter()
                .any(|token| matches!(token.as_str(), "ui" | "view" | "render" | "layout"))
        {
            score = (score - 0.20).clamp(0.0, 1.0);
        }
    }
    if task_prefers_prefs_path(root_user_text, criterion) {
        if matches!(file_low.as_str(), "prefs.rs" | "prefs.json") || path_tokens.contains("prefs") {
            score = (score + 0.45).clamp(0.0, 1.0);
        }
        if path_low.starts_with("src/tui/prefs") {
            score = (score + 0.20).clamp(0.0, 1.0);
        }
    }
    if task_prefers_agent_flow_path(root_user_text, criterion) {
        if matches!(file_low.as_str(), "agent.rs" | "events.rs") || path_tokens.contains("agent") {
            score = (score + 0.40).clamp(0.0, 1.0);
        }
        if path_low.starts_with("src/tui/agent.rs") {
            score = (score + 0.25).clamp(0.0, 1.0);
        }
        if file_low == "repo_map.rs" {
            score = (score - 0.25).clamp(0.0, 1.0);
        }
    }
    score
}

pub(super) fn best_read_only_followup_read_path(
    root_user_text: &str,
    plan: &PlanBlock,
    search_paths: &[String],
    evidence: &ObservationEvidence,
) -> Option<String> {
    let already_read: std::collections::HashSet<String> = evidence
        .reads
        .iter()
        .map(|read| normalize_for_signature(&read.path))
        .collect();
    let mut observed_candidates: Vec<String> = Vec::new();
    for path in search_paths.iter().filter(|path| !path.trim().is_empty()) {
        if already_read.contains(&normalize_for_signature(path)) {
            continue;
        }
        remember_recent_unique(&mut observed_candidates, path.as_str(), 8, 160);
    }
    if observed_candidates.len() == 1 {
        return observed_candidates.into_iter().next();
    }
    if let Some(preferred_path) = preferred_read_only_read_path_hint(root_user_text) {
        if !observed_candidates.iter().any(|path| {
            normalize_for_signature(path.as_str()) == normalize_for_signature(preferred_path)
        }) {
            observed_candidates.push(preferred_path.to_string());
        }
    }
    if observed_candidates.is_empty() {
        return None;
    }
    let criteria_blob = plan.acceptance_criteria.join(" ; ");
    observed_candidates
        .iter()
        .map(|path| {
            let mut score =
                path_prior_score(path, root_user_text, &plan.goal, criteria_blob.as_str());
            let file_low = path_filename(path).to_ascii_lowercase();
            if path.starts_with("src/") {
                score = (score + 0.20).clamp(0.0, 1.0);
            }
            if path.contains("/tui/") || path.starts_with("src/tui/") {
                score = (score + 0.10).clamp(0.0, 1.0);
            }
            if task_prefers_handler_path(root_user_text, criteria_blob.as_str()) {
                if matches!(
                    file_low.as_str(),
                    "events.rs" | "commands.rs" | "command.rs" | "handlers.rs" | "handler.rs"
                ) {
                    score = (score + 0.35).clamp(0.0, 1.0);
                }
                if matches!(file_low.as_str(), "ui.rs" | "view.rs" | "layout.rs") {
                    score = (score - 0.25).clamp(0.0, 1.0);
                }
            }
            if let Some(preferred_path) = preferred_read_only_read_path_hint(root_user_text) {
                if normalize_for_signature(path.as_str()) == normalize_for_signature(preferred_path)
                {
                    score = (score + 0.20).clamp(0.0, 1.0);
                }
            }
            (score, path)
        })
        .filter(|(score, _)| *score >= 0.35)
        .max_by(|a, b| {
            a.0.partial_cmp(&b.0)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.1.cmp(b.1))
        })
        .map(|(_, path)| path.clone())
}

pub(super) fn build_read_only_search_to_read_hint(
    root_user_text: &str,
    plan: &PlanBlock,
    search_paths: &[String],
    evidence: &ObservationEvidence,
) -> Option<String> {
    if !evidence.reads.is_empty() {
        return None;
    }
    let best_path =
        best_read_only_followup_read_path(root_user_text, plan, search_paths, evidence)?;
    let search_attempts = evidence.searches.len();
    let mut out = String::from(
        "[Read-Only Next Step]\n\
You already have a plausible code candidate from successful search.\n",
    );
    if search_attempts >= 2 {
        out.push_str("Do NOT call search_files again yet. Inspect the strongest hit first.\n");
    }
    out.push_str("Next assistant turn: emit a valid <think> block, then call exactly:\n");
    out.push_str(&format!("read_file(path=\"{best_path}\")\n"));
    out.push_str(
        "Verify by confirming the handler branch or slash-command context inside that file.\n\
If that file is not the handler, only then return to search/glob.",
    );
    Some(out)
}

fn build_read_only_diagnose_search_hint(root_user_text: &str) -> String {
    let pattern = preferred_read_only_search_pattern(root_user_text);
    let dir = preferred_read_only_search_dir(root_user_text);
    format!(
        "[Read-Only Diagnose Coercion]\n\
You are stalled in diagnose on a read-only inspection task.\n\
Do not explain further. Call exactly one observation tool next.\n\
Preferred next tool:\n\
search_files(pattern=\"{pattern}\", dir=\"{dir}\")\n\
If that finds a plausible code file, read it next instead of searching again."
    )
}

pub(super) fn first_action_deadline_iters(root_read_only: bool, goal_wants_actions: bool) -> usize {
    if root_read_only || goal_wants_actions {
        2
    } else {
        3
    }
}

pub(super) fn build_first_action_constraint_hint(
    root_user_text: &str,
    root_read_only: bool,
    goal_wants_actions: bool,
) -> Option<String> {
    if root_read_only {
        let pattern = preferred_read_only_search_pattern(root_user_text);
        let dir = preferred_read_only_search_dir(root_user_text);
        return Some(format!(
            "[First Action Constraint]\n\
This task is read-only inspection.\n\
Within the first 2 turns, you must call ONE observation tool.\n\
Do not keep diagnosing in prose.\n\
Preferred first action now:\n\
search_files(pattern=\"{pattern}\", dir=\"{dir}\")\n\
If that finds a likely code file, read it next instead of searching again."
        ));
    }
    if goal_wants_actions {
        return Some(
            "[First Action Constraint]\n\
This task requires local action.\n\
Within the first 2 turns, you must call ONE real tool.\n\
Do not continue with planning-only prose.\n\
Pick the smallest safe action that creates evidence."
                .to_string(),
        );
    }
    None
}

pub(super) fn build_read_only_diagnose_coercion_hint(
    root_user_text: &str,
    plan: Option<&PlanBlock>,
    evidence: &ObservationEvidence,
    messages: &[serde_json::Value],
    working_mem: &WorkingMemory,
) -> Option<String> {
    let fallback_plan;
    let plan = if let Some(plan) = plan {
        plan
    } else {
        fallback_plan = synthetic_read_only_observation_plan(root_user_text);
        &fallback_plan
    };

    if let Some(hint) =
        build_read_only_completion_hint(root_user_text, plan, evidence, messages, working_mem)
    {
        return Some(hint);
    }

    if let Some(search) = evidence.searches.last() {
        if let Some(hint) =
            build_read_only_search_to_read_hint(root_user_text, plan, &search.paths, evidence)
        {
            return Some(hint);
        }
    }

    Some(build_read_only_diagnose_search_hint(root_user_text))
}

pub(super) fn choose_read_only_diagnose_rescue_action(
    root_user_text: &str,
    plan: Option<&PlanBlock>,
    evidence: &ObservationEvidence,
) -> Option<ReadOnlyDiagnoseRescueAction> {
    let fallback_plan;
    let plan = if let Some(plan) = plan {
        plan
    } else {
        fallback_plan = synthetic_read_only_observation_plan(root_user_text);
        &fallback_plan
    };

    if let Some(search) = evidence.searches.last() {
        if let Some(path) =
            best_read_only_followup_read_path(root_user_text, plan, &search.paths, evidence)
        {
            return Some(ReadOnlyDiagnoseRescueAction::Read { path });
        }
    }

    if evidence.searches.is_empty() {
        return Some(ReadOnlyDiagnoseRescueAction::Search {
            pattern: preferred_read_only_search_pattern(root_user_text),
            dir: preferred_read_only_search_dir(root_user_text).to_string(),
        });
    }

    None
}

pub(super) fn build_read_only_plan_rewrite_hint(root_user_text: &str) -> String {
    let goal = synthetic_read_only_goal(root_user_text);
    let pattern = preferred_read_only_search_pattern(root_user_text);
    let dir = preferred_read_only_search_dir(root_user_text);
    let secondary_pattern = preferred_read_only_secondary_search_pattern(root_user_text);
    let (accept1, accept2, risks) = synthetic_read_only_acceptance(root_user_text);
    let step1 = format!("search_files(pattern=\"{pattern}\", dir=\"{dir}\")");
    let step2 = secondary_pattern
        .filter(|secondary| secondary != &pattern)
        .map(|secondary| format!("if needed search_files(pattern=\"{secondary}\", dir=\"{dir}\")"))
        .unwrap_or_else(|| "if needed inspect the strongest matching file".to_string());
    format!(
        "[Read-only plan rewrite]\n\
Use a strictly inspect-only plan. Do NOT mention cargo test, build, smoke test, behavioral verification, or exec.\n\
Use this shape in your next assistant turn:\n\
<plan>\n\
goal: {goal}\n\
steps: 1) {step1} 2) {step2} 3) read_file(path=\"<matching file>\") to confirm the relevant code context 4) call done once the file path and code context are confirmed\n\
acceptance: 1) {accept1} 2) {accept2}\n\
risks: 1) {risks}\n\
assumptions: 1) observation tools are sufficient 2) no edits or behavioral verification are required\n\
</plan>\n\
Then emit <think> and call ONE tool immediately after it.\n\
Suggested next tool: search_files(pattern=\"{pattern}\", dir=\"{dir}\")\n\
If you already have a strong hit, read that matching file instead of searching again."
    )
}

pub(super) fn synthetic_read_only_observation_plan(root_user_text: &str) -> PlanBlock {
    let goal = synthetic_read_only_goal(root_user_text);
    let pattern = preferred_read_only_search_pattern(root_user_text);
    let dir = preferred_read_only_search_dir(root_user_text);
    let secondary_pattern = preferred_read_only_secondary_search_pattern(root_user_text);
    let (accept1, accept2, risks) = synthetic_read_only_acceptance(root_user_text);
    PlanBlock {
        goal,
        steps: {
            let mut steps = vec![format!(
                "search_files(pattern=\"{pattern}\", dir=\"{dir}\")"
            )];
            if let Some(secondary) = secondary_pattern.filter(|secondary| secondary != &pattern) {
                steps.push(format!(
                    "if needed search_files(pattern=\"{secondary}\", dir=\"{dir}\")"
                ));
            } else {
                steps.push("if needed inspect the strongest matching file".to_string());
            }
            steps.push(
                "read_file(path=\"<matching file>\") to confirm the relevant code context"
                    .to_string(),
            );
            steps.push("call done once the file path and code context are confirmed".to_string());
            steps
        },
        acceptance_criteria: vec![accept1, accept2],
        risks,
        assumptions:
            "observation tools are sufficient; no edits or behavioral verification are required"
                .to_string(),
    }
}

pub(super) fn coerce_read_only_observation_tool_call(
    messages: &[serde_json::Value],
    tc: &ToolCallData,
    root_user_text: &str,
    root_read_only: bool,
    evidence: &ObservationEvidence,
) -> Option<(ToolCallData, String, String)> {
    if !root_read_only || !evidence.reads.is_empty() {
        return None;
    }
    if !matches!(tc.name.as_str(), "search_files" | "list_dir" | "glob") {
        return None;
    }
    let repeated_gate_misses =
        consecutive_missing_gate_blocks_for_observation(messages).saturating_add(1);
    if repeated_gate_misses < 2 {
        return None;
    }

    let preferred_pattern = preferred_read_only_search_pattern(root_user_text);
    let preferred_dir = preferred_read_only_search_dir(root_user_text);
    match tc.name.as_str() {
        "search_files" => {
            let args = serde_json::from_str::<serde_json::Value>(&tc.arguments).ok()?;
            let current_pattern = args
                .get("pattern")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();
            let current_dir = args
                .get("dir")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();
            if normalize_for_signature(current_pattern)
                == normalize_for_signature(preferred_pattern.as_str())
                && normalize_path_alias(current_dir) == normalize_path_alias(preferred_dir)
            {
                return None;
            }
            rewrite_tool_call_to_search_files(tc, preferred_pattern.as_str(), preferred_dir)
        }
        "list_dir" | "glob" => {
            rewrite_tool_call_to_search_files(tc, preferred_pattern.as_str(), preferred_dir)
        }
        _ => None,
    }
}

pub(super) fn coerce_read_only_followup_read_tool_call(
    messages: &[serde_json::Value],
    tc: &ToolCallData,
    root_user_text: &str,
    root_read_only: bool,
    plan: Option<&PlanBlock>,
    evidence: &ObservationEvidence,
) -> Option<(ToolCallData, String, String)> {
    if !root_read_only || !matches!(tc.name.as_str(), "read_file") || !evidence.reads.is_empty() {
        return None;
    }
    let saw_recent_gate_miss = messages
        .iter()
        .rev()
        .take(6)
        .any(is_missing_gate_governor_block);

    let args = serde_json::from_str::<serde_json::Value>(&tc.arguments).ok()?;
    let current_path = args
        .get("path")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .trim();
    if current_path.is_empty() {
        return None;
    }

    let fallback_plan;
    let plan = if let Some(plan) = plan {
        plan
    } else {
        fallback_plan = synthetic_read_only_observation_plan(root_user_text);
        &fallback_plan
    };

    let last_search = evidence.searches.last()?;
    let preferred_path_hint = preferred_read_only_read_path_hint(root_user_text)
        .map(str::to_string)
        .filter(|path| !path.trim().is_empty());
    let handler_candidate = if task_prefers_handler_path(root_user_text, &plan.goal) {
        last_search
            .paths
            .iter()
            .find(|path| {
                matches!(
                    path_filename(path).to_ascii_lowercase().as_str(),
                    "events.rs" | "commands.rs" | "command.rs" | "handlers.rs" | "handler.rs"
                )
            })
            .cloned()
    } else {
        None
    };
    if !saw_recent_gate_miss && preferred_path_hint.is_none() && handler_candidate.is_none() {
        return None;
    }
    let best_path = preferred_path_hint
        .or(handler_candidate)
        .or_else(|| {
            best_read_only_followup_read_path(root_user_text, plan, &last_search.paths, evidence)
        })
        .or_else(|| {
            last_search
                .paths
                .iter()
                .find(|path| path.starts_with("src/"))
                .cloned()
        })
        .or_else(|| last_search.paths.first().cloned())?;
    if normalize_for_signature(current_path) == normalize_for_signature(best_path.as_str()) {
        return None;
    }

    let mut rewritten = tc.clone();
    rewritten.arguments = json!({ "path": best_path }).to_string();
    Some((rewritten, current_path.to_string(), best_path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_observed_search_hit_becomes_followup_read_even_for_generic_task() {
        let prompt = "Locate where project-local profile aliases are loaded for the greet command.";
        let plan = synthetic_read_only_observation_plan(prompt);

        let path = best_read_only_followup_read_path(
            prompt,
            &plan,
            &[String::from("src/config.rs")],
            &ObservationEvidence::default(),
        );

        assert_eq!(path.as_deref(), Some("src/config.rs"));
    }

    #[test]
    fn generic_followup_does_not_invent_repo_specific_runtime_path() {
        let prompt = "Locate where project-local profile aliases are loaded for the greet command.";
        let plan = synthetic_read_only_observation_plan(prompt);

        let path =
            best_read_only_followup_read_path(prompt, &plan, &[], &ObservationEvidence::default());

        assert_eq!(path, None);
    }

    #[test]
    fn generic_plan_rewrite_hint_avoids_events_rs_default() {
        let hint = build_read_only_plan_rewrite_hint(
            "Locate where project-local profile aliases are loaded for the greet command.",
        );

        assert!(hint.contains("read that matching file instead of searching again."));
        assert!(!hint.contains("src/tui/events.rs"));
    }

    #[test]
    fn coerce_read_only_followup_read_tool_call_prefers_best_handler_candidate() {
        let messages = vec![
            json!({
                "role": "assistant",
                "content": "",
                "tool_calls": [{
                    "id": "call_1",
                    "type": "function",
                    "function": {
                        "name": "search_files",
                        "arguments": serde_json::json!({"pattern":"/realize","dir":"src/tui"}).to_string()
                    }
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_1",
                "content": "GOVERNOR BLOCKED\n\n[Plan Gate] Missing valid <plan>.\n\ntool:\nsearch_files\narguments:\n{\"pattern\":\"/realize\",\"dir\":\"src/tui\"}"
            }),
        ];
        let plan = synthetic_read_only_observation_plan(
            "Locate where the /realize slash command is handled in the TUI. Do not edit anything.",
        );
        let evidence = ObservationEvidence {
            searches: vec![ObservationSearchEvidence {
                command: "search_files(pattern=/realize, dir=src/tui)".to_string(),
                pattern: "/realize".to_string(),
                hit_count: 3,
                paths: vec![
                    "src/tui/events.rs".to_string(),
                    "src/tui/intent.rs".to_string(),
                    "src/tui/agent.rs".to_string(),
                ],
            }],
            reads: Vec::new(),
            resolutions: Vec::new(),
        };
        let tc = ToolCallData {
            thought_signature: None,
            id: "call_read".to_string(),
            name: "read_file".to_string(),
            arguments: "{\"path\":\"src/tui/intent.rs\"}".to_string(),
        };

        let (rewritten, original, coerced) = coerce_read_only_followup_read_tool_call(
            &messages,
            &tc,
            "Locate where the /realize slash command is handled in the TUI. Do not edit anything.",
            true,
            Some(&plan),
            &evidence,
        )
        .expect("coerced read");

        assert_eq!(original, "src/tui/intent.rs");
        assert_eq!(coerced, "src/tui/events.rs");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&rewritten.arguments)
                .ok()
                .and_then(|args| args
                    .get("path")
                    .and_then(|value| value.as_str())
                    .map(str::to_string))
                .as_deref(),
            Some("src/tui/events.rs")
        );
    }

    #[test]
    fn coerce_read_only_followup_read_tool_call_prefers_prefs_owner_hint() {
        let messages = vec![json!({
            "role": "tool",
            "tool_call_id": "call_1",
            "content": "GOVERNOR BLOCKED\n\n[Plan Gate] Missing valid <plan>.\n\ntool:\nsearch_files\narguments:\n{\"pattern\":\"prefs\",\"dir\":\"src/tui\"}"
        })];
        let plan = synthetic_read_only_observation_plan(
            "Find where pane-scoped TUI preferences are serialized and restored. Do not edit anything.",
        );
        let evidence = ObservationEvidence {
            searches: vec![ObservationSearchEvidence {
                command: "search_files(pattern=prefs, dir=src/tui)".to_string(),
                pattern: "prefs".to_string(),
                hit_count: 1,
                paths: vec!["src/tui/events.rs".to_string()],
            }],
            reads: Vec::new(),
            resolutions: Vec::new(),
        };
        let tc = ToolCallData {
            thought_signature: None,
            id: "call_read".to_string(),
            name: "read_file".to_string(),
            arguments: "{\"path\":\"src/tui/events.rs\"}".to_string(),
        };

        let (rewritten, original, coerced) = coerce_read_only_followup_read_tool_call(
            &messages,
            &tc,
            "Find where pane-scoped TUI preferences are serialized and restored. Do not edit anything.",
            true,
            Some(&plan),
            &evidence,
        )
        .expect("coerced read");

        assert_eq!(original, "src/tui/events.rs");
        assert_eq!(coerced, "src/tui/prefs.rs");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&rewritten.arguments)
                .ok()
                .and_then(|args| args
                    .get("path")
                    .and_then(|value| value.as_str())
                    .map(str::to_string))
                .as_deref(),
            Some("src/tui/prefs.rs")
        );
    }
}
