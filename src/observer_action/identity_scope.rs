//! Source navigation for an Observer reviewing the scope of an identifier.
//! Quotes locate a possible writer. Its lifetime and the test remain model judgments.
use crate::file_tools::read_range::{bounded_workspace_text, header_counts};
use serde::Serialize;
use serde_json::Value;
use std::{collections::BTreeSet, path::Path};

#[derive(Serialize)]
pub(crate) struct Review {
    reference_paths: Vec<String>,
    workspace_excerpts: Vec<Writer>,
}

#[derive(Serialize)]
struct Writer {
    path: String,
    selected_by_read_call_id: String,
    assignment: Line,
    #[serde(skip_serializing_if = "Option::is_none")]
    following_use: Option<Line>,
    possible_initializers: Vec<Line>,
}

#[derive(Serialize)]
struct Line {
    number: usize,
    text: String,
}

fn quote(index: usize, line: &str) -> Line {
    Line {
        number: index + 1,
        text: line.trim().chars().take(240).collect(),
    }
}

fn uniqueness_rule(description: &str) -> bool {
    let mut identity = false;
    let mut uniqueness = false;
    for word in description.split(|c: char| !c.is_alphanumeric()) {
        match word.to_ascii_lowercase().as_str() {
            "id" | "ids" | "identifier" | "identifiers" => identity = true,
            "unique" | "uniqueness" | "duplicate" | "duplicates" => uniqueness = true,
            _ => {}
        }
    }
    identity && uniqueness
}

fn writes_id(line: &str) -> Option<&str> {
    let (binding, expression) = line.trim().strip_prefix("let ")?.split_once('=')?;
    let binding = binding
        .trim()
        .strip_prefix("mut ")
        .unwrap_or(binding.trim());
    let name = binding.split(':').next()?.trim();
    if name != "id" && !name.ends_with("_id") {
        return None;
    }
    let expression = expression.trim().strip_suffix(';')?;
    (expression.ends_with(".len()")
        || expression.contains(".fetch_add(")
        || expression.contains("Uuid::new_")
        || expression.contains("next_id("))
    .then_some(expression)
}

fn writer_sites(path: &str, call: &str, body: &str) -> Vec<Writer> {
    let lines: Vec<_> = body.lines().collect();
    lines
        .iter()
        .enumerate()
        .filter_map(|(index, line)| {
            let expression = writes_id(line)?;
            let collection = expression
                .strip_suffix(".len()")
                .and_then(|owner| owner.rsplit('.').next());
            let mut initializers = Vec::new();
            let mut following_use = None;
            if let Some(field) = collection {
                let prefix = format!("{field}:");
                initializers = lines
                    .iter()
                    .enumerate()
                    .filter(|(_, line)| {
                        line.trim().strip_prefix(&prefix).is_some_and(|value| {
                            matches!(
                                value.trim().trim_end_matches(','),
                                "Vec::new()" | "vec![]" | "Default::default()"
                            )
                        })
                    })
                    .take(2)
                    .map(|(i, line)| quote(i, line))
                    .collect();
                let owner = expression.strip_suffix(".len()")?;
                if let Some(next) = lines
                    .get(index + 1)
                    .filter(|line| line.trim().starts_with(&format!("{owner}.push(")))
                {
                    following_use = Some(quote(index + 1, next));
                }
            }
            Some(Writer {
                path: path.into(),
                selected_by_read_call_id: call.into(),
                assignment: quote(index, line),
                following_use,
                possible_initializers: initializers,
            })
        })
        .take(3)
        .collect()
}

pub(crate) fn project(packet: &Value, task: &str, workspace: &str) -> Option<Review> {
    let status = &packet["task_obligations"]["task_requirements"];
    let rows = status["requirements"].as_array()?;
    let requirements = || rows.iter().map(|row| &row["requirement"]);
    if status["declared"] != true
        || !requirements().any(|r| {
            r["description"].as_str().is_some_and(uniqueness_rule)
                && r["artifacts"]
                    .as_array()
                    .is_some_and(|artifacts| artifacts.iter().any(|a| a["kind"] == "test"))
        })
    {
        return None;
    }
    let outputs: BTreeSet<_> = requirements()
        .flat_map(|r| r["artifacts"].as_array().into_iter().flatten())
        .filter_map(|a| crate::task_scope::relative_path(a["path"].as_str()?, Some(workspace)))
        .collect();
    let mut paths = Vec::new();
    for token in task.split_whitespace() {
        let token = token
            .trim_start_matches(['`', '"', '\'', '(', '['])
            .trim_end_matches(['`', '"', '\'', ')', ']', ',', ';', ':', '.']);
        if !token.ends_with(".rs") {
            continue;
        }
        let Some(path) = crate::task_scope::relative_path(token, Some(workspace)) else {
            continue;
        };
        if !outputs.contains(&path) && !paths.contains(&path) {
            paths.push(path);
            if paths.len() == 4 {
                break;
            }
        }
    }
    if paths.is_empty() {
        return None;
    }
    let root = Path::new(workspace).canonicalize().ok()?;
    let actions = packet["execution_evidence"]["actions"].as_array()?;
    let mut excerpts = Vec::new();
    for path in &paths {
        let read = actions.iter().rev().find_map(|a| {
            if a["tool"] != "read_file" || a["executed"] != true {
                return None;
            }
            let argument = a["arguments"]["path"].as_str()?;
            if crate::task_scope::relative_path(argument, Some(workspace))?.as_str() != path {
                return None;
            }
            header_counts(argument, a["result"].as_str()?.lines().next()?)?;
            a["call_id"].as_str()
        });
        if let Some(call) = read {
            if let Some(body) = bounded_workspace_text(&root, path, 64 * 1024) {
                excerpts.extend(writer_sites(path, call, &body));
            }
        }
    }
    Some(Review {
        reference_paths: paths,
        workspace_excerpts: excerpts,
    })
}

pub(super) fn guidance(packet: &Value) -> String {
    let Some(review) = packet.get("identity_scope_review") else {
        return String::new();
    };
    format!("[Identity scope review]\n{review}\nUse these current-workspace quotes as source navigation. The cited read selected the path and may have covered a different range. Trace the writer, its owning state and when that state is constructed. Matching field names alone do not prove a reset. If the writer is missing or lifetime unclear, name the exact source observation needed. Once established, put the writer citation and its discriminating public test in the primary reason sent to the Coder before the final sweep. For an owner-local ID, describe how to build two distinct, independently valid finalized owners in one input, preserve the same native ID in both, retain ordering, linkage and counts, and assert success with both represented. Use the same valid control for a single within-owner duplicate and assert rejection. For a shared allocator follow its actual lifetime. If the tests already exercise both directions, cite them and advance. Honor the current action and budget restrictions.\n")
}

#[cfg(test)]
mod tests;
