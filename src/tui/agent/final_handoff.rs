//! Final-answer requirements are checked separately from execution proof.
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

const MARKER: &str = "final answer must include";

fn instructions(root: &str) -> Vec<&str> {
    root.lines()
        .filter_map(|line| {
            line.to_ascii_lowercase()
                .find(MARKER)
                .map(|i| &line[i + MARKER.len()..])
        })
        .collect()
}

pub(super) fn requires_authored_final_answer(root: &str) -> bool {
    !instructions(root).is_empty()
}

pub(super) fn authored_final_answer_hint(root: &str) -> Option<String> {
    let clauses = instructions(root);
    (!clauses.is_empty()).then(|| format!(
        "[Final handoff] Call `done` with a nonempty `summary` answering the original final-answer instruction. Cite full artifact paths and exact verification commands from recorded evidence. Preserve explicitly named paths and short required phrases verbatim when supported by that evidence. State limitations truthfully; do not rerun successful tools just to obtain a summary.\nOriginal final-answer instruction:\n{}",
        clauses.iter().map(|clause| format!("Final answer must include{clause}")).collect::<Vec<_>>().join("\n")
    ))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PathKind {
    Any,
    Rust,
    Documentation,
    Spec,
}

impl PathKind {
    fn matches(self, path: &str) -> bool {
        match self {
            Self::Any => true,
            Self::Rust => path.ends_with(".rs"),
            Self::Documentation => {
                path.starts_with("docs/")
                    || [".md", ".rst", ".adoc"]
                        .iter()
                        .any(|ext| path.ends_with(ext))
            }
            Self::Spec => [".json", ".yaml", ".yml", ".toml"]
                .iter()
                .any(|ext| path.ends_with(ext)),
        }
    }
    fn label(self) -> &'static str {
        match self {
            Self::Any => "artifact path",
            Self::Rust => "changed Rust file path",
            Self::Documentation => "documentation path",
            Self::Spec => "specification path",
        }
    }
}

fn path_kinds(clauses: &[&str]) -> Vec<PathKind> {
    let mut kinds = Vec::new();
    for clause in clauses {
        // These are deliberately bounded English categories, not a general
        // natural-language interpretation or a requirement to list every edit.
        let lower = clause.to_ascii_lowercase();
        for item in lower.split(',').flat_map(|item| item.split(" and ")) {
            if !item.contains("path") {
                continue;
            }
            let kind = if item.contains("rust") {
                PathKind::Rust
            } else if item.contains("docs") || item.contains("documentation") {
                PathKind::Documentation
            } else if item.contains("spec") {
                PathKind::Spec
            } else {
                PathKind::Any
            };
            if !kinds.contains(&kind) {
                kinds.push(kind);
            }
        }
    }
    kinds
}

fn quoted_literals(clauses: &[&str]) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for clause in clauses {
        let mut rest = *clause;
        while let Some((_, opened)) = rest.split_once('`') {
            let Some((literal, closed)) = opened.split_once('`') else {
                break;
            };
            if !literal.is_empty() {
                out.insert(literal.to_string());
            }
            rest = closed;
        }
    }
    out
}

fn explicit_paths(clauses: &[&str]) -> BTreeSet<String> {
    clauses
        .iter()
        .flat_map(|clause| clause.split_whitespace())
        .filter_map(|token| {
            let path = token
                .trim_matches(['`', '"', '\'', '(', ')', '[', ']', ','])
                .trim_end_matches(['.', ',', ';', ':', '!', '?']);
            let filename = path.rsplit_once('.').is_some_and(|(stem, ext)| {
                !stem.is_empty()
                    && ext.chars().all(|c| c.is_ascii_alphabetic())
                    && (ext.len() >= 2 || matches!(ext, "c" | "h" | "r" | "R"))
            });
            (!path.is_empty() && (path.contains('/') || path.contains('\\') || filename))
                .then(|| path.to_string())
        })
        .collect()
}

/// A bounded comma/and list can name plain values as well as paths. Only
/// recognize short, unqualified values in a list that explicitly names a path;
/// descriptive requests such as "a summary" remain authored prose, not literals.
fn named_values(clauses: &[&str]) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for clause in clauses {
        if explicit_paths(&[clause]).is_empty() {
            continue;
        }
        for item in clause.split(',').flat_map(|item| item.split(" and ")) {
            let item = item.trim().trim_end_matches(['.', '!', ';']);
            let lower = item.to_ascii_lowercase();
            let words: Vec<_> = lower.split_whitespace().collect();
            if !(2..=5).contains(&words.len())
                || item.contains('`')
                || !explicit_paths(&[item]).is_empty()
            {
                continue;
            }
            if [
                "a",
                "an",
                "the",
                "your",
                "what",
                "how",
                "why",
                "whether",
                "explain",
                "describe",
                "summarize",
                "list",
                "mention",
                "show",
                "state",
            ]
            .contains(&words[0])
                || words.iter().any(|word| {
                    [
                        "path",
                        "paths",
                        "file",
                        "files",
                        "command",
                        "commands",
                        "summary",
                        "explanation",
                        "details",
                        "steps",
                    ]
                    .contains(word)
                })
            {
                continue;
            }
            out.insert(item.to_string());
        }
    }
    out
}

#[derive(Default)]
struct HandoffEvidence {
    paths: BTreeSet<String>,
    commands: BTreeSet<String>,
}

fn collect_evidence(root: &str, messages: &[Value], test_cmd: Option<&str>) -> HandoffEvidence {
    let context = super::exec_verification::ExecVerificationContext::from_root(test_cmd, root);
    let mut pending = BTreeMap::new();
    let mut evidence = HandoffEvidence::default();
    for message in messages {
        match message["role"].as_str() {
            Some("assistant") => {
                for call in message["tool_calls"].as_array().into_iter().flatten() {
                    let Some(id) = call["id"].as_str().filter(|id| !id.is_empty()) else {
                        continue;
                    };
                    let args = call["function"]["arguments"]
                        .as_str()
                        .and_then(|s| serde_json::from_str::<Value>(s).ok());
                    pending.insert(
                        id.to_string(),
                        (
                            call["function"]["name"].as_str().unwrap_or("").to_string(),
                            args,
                        ),
                    );
                }
            }
            Some("tool") => {
                let Some((name, Some(args))) = message["tool_call_id"]
                    .as_str()
                    .and_then(|id| pending.remove(id))
                else {
                    continue;
                };
                let content = message["content"].as_str().unwrap_or("");
                if crate::execution_evidence::file_edit_succeeded(&name, content) {
                    if let Some(path) = args["path"].as_str().filter(|path| !path.trim().is_empty())
                    {
                        evidence
                            .paths
                            .insert(path.replace('\\', "/").trim_start_matches("./").to_string());
                    }
                    evidence.commands.clear();
                    if crate::execution_evidence::auto_test_succeeded(&name, content) {
                        if let Some(command) = test_cmd.filter(|cmd| !cmd.trim().is_empty()) {
                            evidence.commands.insert(command.trim().to_string());
                        }
                    }
                } else if name == "exec" && crate::execution_evidence::exec_may_have_run(content) {
                    let command = args["command"].as_str().unwrap_or("");
                    match context.classify(command).kind {
                        super::ExecKind::Action => evidence.commands.clear(),
                        super::ExecKind::Verify => {
                            if crate::execution_evidence::exec_succeeded(content) {
                                evidence.commands.insert(command.trim().to_string());
                            } else {
                                evidence.commands.clear();
                            }
                        }
                        super::ExecKind::Diagnostic => {}
                    }
                }
            }
            _ => {}
        }
    }
    evidence
}

/// Validate the rendered answer: truthful citations in Acceptance count, but
/// hints, previous assistant messages, and unrendered tool arguments do not.
pub(super) fn validate_authored_done_summary(
    root: &str,
    summary: &str,
    rendered: &str,
    messages: &[Value],
    test_cmd: Option<&str>,
) -> Result<(), String> {
    let Some(hint) = authored_final_answer_hint(root) else {
        return Ok(());
    };
    let clauses = instructions(root);
    let evidence = collect_evidence(root, messages, test_cmd);
    let mut missing = Vec::new();
    if summary.trim().is_empty() {
        missing.push("nonempty authored summary".to_string());
    }
    let mut required_literals = quoted_literals(&clauses);
    required_literals.extend(explicit_paths(&clauses));
    required_literals.extend(named_values(&clauses));
    for literal in required_literals {
        if !rendered.contains(literal.as_str()) {
            missing.push(format!("explicit item `{literal}`"));
        }
    }
    for kind in path_kinds(&clauses) {
        let candidates: Vec<_> = evidence
            .paths
            .iter()
            .filter(|path| kind.matches(path))
            .collect();
        // No invented path or success claim when the transcript lacks an edit.
        // Execution/acceptance gates retain responsibility for artifact proof.
        if !candidates.is_empty()
            && !candidates
                .iter()
                .any(|path| rendered.contains(path.as_str()))
        {
            missing.push(format!(
                "{} (recorded: {})",
                kind.label(),
                candidates
                    .iter()
                    .map(|p| format!("`{p}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
    }
    if clauses
        .iter()
        .any(|clause| clause.to_ascii_lowercase().contains("verification command"))
        && !evidence.commands.is_empty()
        && !evidence
            .commands
            .iter()
            .any(|command| rendered.contains(command))
    {
        missing.push(format!(
            "exact verification command (fresh: {})",
            evidence
                .commands
                .iter()
                .map(|cmd| format!("`{cmd}`"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!("{hint}\nMissing from the final answer:\n- {}\nRevise the `done` answer using this evidence; successful verification remains valid.", missing.join("\n- ")))
    }
}

/// Legacy text-only finalizers may add a factual receipt. They must never
/// fabricate a requested status/approval label or copy unrelated root literals.
pub(super) fn enrich_text_final_handoff(
    content: &str,
    root: &str,
    messages: &[Value],
    test_cmd: Option<&str>,
) -> Option<String> {
    if !content.trim_start().starts_with("[DONE]") || !requires_authored_final_answer(root) {
        return None;
    }
    let evidence = collect_evidence(root, messages, test_cmd);
    let kinds = path_kinds(&instructions(root));
    let paths: Vec<_> = evidence
        .paths
        .iter()
        .filter(|path| {
            kinds.iter().any(|kind| kind.matches(path)) && !content.contains(path.as_str())
        })
        .collect();
    let mut enriched = content.to_string();
    if !paths.is_empty() {
        enriched.push_str("\n\nRecorded file edits:\n");
        for path in paths {
            enriched.push_str(&format!("- `{path}`\n"));
        }
    }
    if instructions(root)
        .iter()
        .any(|clause| clause.to_ascii_lowercase().contains("verification command"))
    {
        for command in evidence
            .commands
            .iter()
            .filter(|command| !content.contains(command.as_str()))
        {
            enriched.push_str(&format!(
                "\nRecorded successful verification: `{command}`\n"
            ));
        }
    }
    (enriched != content).then_some(enriched)
}

#[cfg(test)]
mod tests;
