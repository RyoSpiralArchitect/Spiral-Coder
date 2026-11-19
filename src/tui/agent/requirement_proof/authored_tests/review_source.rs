//! Preserve one bounded test revision from correlated native results.
use super::AuthoredTests;
use crate::execution_evidence::{exec_may_have_run, file_edit_succeeded};
use crate::file_tools::read_range::{bounded_workspace_text, fresh_full_body};
use crate::tui::agent::{
    exec_verification::ExecVerificationContext,
    recovery_focus::correlated_results,
    verification::{fmt_hash, hash_text},
    ExecKind,
};
use serde::Serialize;
use serde_json::Value;
use std::path::Path;

const MAX_SOURCE_BYTES: usize = 8192;

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum Origin {
    Read,
    Write,
    ReconstructedEdit,
    WorkspaceReadback,
}

pub(super) fn readback(
    tests: &AuthoredTests,
    messages: &[Value],
    workspace: &str,
) -> Option<ReviewSource> {
    let (path, call_id) = correlated_results(messages)
        .filter_map(|(call, result)| {
            if !file_edit_succeeded(&call.name, result["content"].as_str()?) {
                return None;
            }
            let args: Value = serde_json::from_str(&call.arguments).ok()?;
            let path =
                crate::task_scope::relative_path(args["path"].as_str()?.trim(), Some(workspace))?;
            tests.contains(&path).then_some((path, call.id))
        })
        .last()?;
    let root = Path::new(workspace).canonicalize().ok()?;
    let source = bounded_workspace_text(&root, &path, MAX_SOURCE_BYTES as u64)?;
    Some(ReviewSource {
        path,
        call_id,
        origin: Origin::WorkspaceReadback,
        source,
        rendered_read_hash: None,
    })
}

#[derive(Debug, Serialize)]
pub(super) struct ReviewSource {
    path: String,
    call_id: String,
    origin: Origin,
    source: String,
    #[serde(skip)]
    rendered_read_hash: Option<u64>,
}

impl ReviewSource {
    pub(super) fn line(&self, path: &str, line: usize) -> Option<&str> {
        (self.path == path).then_some(())?;
        self.source.lines().nth(line.checked_sub(1)?)
    }
}

pub(super) fn project(
    tests: &AuthoredTests,
    messages: &[Value],
    context: &ExecVerificationContext<'_>,
    tool_root: Option<&str>,
) -> Option<ReviewSource> {
    let mut observation: Option<ReviewSource> = None;
    for (call, result) in correlated_results(messages) {
        let Some(output) = result["content"].as_str() else {
            continue;
        };
        let Ok(args) = serde_json::from_str::<Value>(&call.arguments) else {
            continue;
        };
        if call.name == "exec"
            && exec_may_have_run(output)
            && !output.starts_with("Awaiting approval")
            && args["command"]
                .as_str()
                .is_some_and(|cmd| context.classify(cmd).kind == ExecKind::Action)
        {
            observation = None;
        }
        let Some(authored_path) = args["path"].as_str() else {
            continue;
        };
        let Some(path) = crate::task_scope::relative_path(authored_path.trim(), tool_root)
            .filter(|path| tests.contains(path))
        else {
            continue;
        };
        let attempted = exec_may_have_run(output) && !output.starts_with("Awaiting approval");
        let changed = file_edit_succeeded(&call.name, output);
        let source = match call.name.as_str() {
            "read_file" => fresh_full_body(authored_path, output)
                .filter(|body| body.len() <= MAX_SOURCE_BYTES)
                .map(|body| (Origin::Read, body.to_owned(), Some(hash_text(output)))),
            "write_file" if changed => args["content"]
                .as_str()
                .filter(|body| body.len() <= MAX_SOURCE_BYTES)
                .and_then(|body| {
                    let header = format!(
                        "OK: wrote '{authored_path}' ({} lines, {} bytes)",
                        body.lines().count(),
                        body.len()
                    );
                    (output.lines().next() == Some(header.as_str()))
                        .then(|| (Origin::Write, body.to_owned(), None))
                }),
            "patch_file" | "apply_diff"
                if changed && call.arguments.len() <= 2 * MAX_SOURCE_BYTES =>
            {
                observation
                    .as_ref()
                    .filter(|view| view.path == path)
                    .and_then(|view| reconstruct(&call.name, authored_path, &args, view, output))
                    .map(|source| (Origin::ReconstructedEdit, source, None))
            }
            _ => None,
        };
        if let Some((origin, source, rendered_read_hash)) =
            source.filter(|(_, source, _)| source.len() <= MAX_SOURCE_BYTES)
        {
            observation = Some(ReviewSource {
                path,
                call_id: call.id,
                origin,
                source,
                rendered_read_hash,
            });
        } else if attempted
            && (call.name == "read_file"
                || matches!(
                    call.name.as_str(),
                    "write_file" | "patch_file" | "apply_diff"
                ) && !output.starts_with("OK: unchanged '"))
            && observation.as_ref().is_some_and(|view| view.path == path)
        {
            observation = None;
        }
    }
    observation
}

/// Recompute only a recorded native file edit against a complete prior
/// revision. Tool hashes are diagnostic fingerprints, not verification proof.
fn reconstruct(
    tool: &str,
    path: &str,
    args: &Value,
    prior: &ReviewSource,
    result: &str,
) -> Option<String> {
    let mut fingerprints = result
        .lines()
        .filter_map(|line| line.strip_prefix("[hash] "));
    let recorded = fingerprints.next()?;
    if fingerprints.next().is_some() {
        return None;
    }
    let (before_hash, after_hash) = recorded.strip_prefix("before=")?.split_once(" after=")?;
    let before = prior.source.as_str();
    // The file tool's approval cache can hold a complete native read rendering
    // rather than bare contents. Compare only the rendering we actually observed.
    if before_hash != fmt_hash(hash_text(before))
        && prior
            .rendered_read_hash
            .is_none_or(|hash| before_hash != fmt_hash(hash))
    {
        return None;
    }
    let header = result.lines().next()?;
    let after = if tool == "patch_file" {
        if !header.starts_with(&format!("OK: patched '{path}' (")) {
            return None;
        }
        crate::file_tools::json_patch::prepare(path, before, args).ok()?
    } else {
        let (after, report) =
            crate::file_tools::edit_preview::diff(path, before, args["diff"].as_str()?).ok()?;
        if report.lines().next() != Some(header) {
            return None;
        }
        after
    };
    (after_hash == fmt_hash(hash_text(&after)) && before != after).then_some(after)
}
