//! Resume a Coder-authored literal patch after the diagnosis that held it.
//! The transcript owns the proposal. Only a new, ordinarily governed call can write.
use super::{eligible_target, full_read, take_exchange, Exchange};
use crate::execution_evidence::{auto_test_outcome, AutoTestOutcome};
use crate::file_tools::read_range::{matches_range_header, ReadRequest};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::Read;
use std::path::Path;

const FILE_LIMIT: u64 = 512 * 1024;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Patch {
    path: String,
    search: String,
    replace: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct NextAction {
    proposal_call_id: String,
    failed_check: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    confirming_read_call_id: Option<String>,
    suggested_tool: &'static str,
    suggested_args: Value,
    instruction: &'static str,
}

struct History<'a> {
    failure: Exchange<'a>,
    proposal: Exchange<'a>,
    read: Option<Exchange<'a>>,
    patch: Patch,
    check: String,
}

fn observation(tool: &str) -> bool {
    matches!(tool, "read_file" | "search_files" | "glob" | "list_dir")
}

fn held(output: &str) -> bool {
    output.starts_with("GOVERNOR BLOCKED\ncwd: ")
        && output
            .split_once("\n\n")
            .is_some_and(|(_, s)| s.starts_with("[Recovery Gate] stage=diagnose\n"))
}

fn history(messages: &[Value]) -> Option<History<'_>> {
    let mut remaining = crate::task_origin::current_task_messages(messages);
    let suffix = remaining;
    let proposal = loop {
        let event = take_exchange(&mut remaining)?;
        if !observation(event.name) {
            break event;
        }
        if !event.output.starts_with('[') {
            return None;
        }
    };
    if proposal.name != "patch_file"
        || !held(proposal.output)
        || proposal.args.len() > 4608
        || !proposal.output.ends_with(&format!(
            "\n\ntool:\npatch_file\narguments:\n{}",
            proposal.args
        ))
    {
        return None;
    }
    let patch: Patch = serde_json::from_str(proposal.args).ok()?;
    if patch.search.trim().is_empty()
        || patch.search == patch.replace
        || patch.search.len() + patch.replace.len() > 4096
        || !eligible_target(crate::task_origin::root_user_text(messages), &patch.path)
    {
        return None;
    }
    let failure = loop {
        let event = take_exchange(&mut remaining)?;
        if !observation(event.name) {
            break event;
        }
        if !event.output.starts_with('[') {
            return None;
        }
    };
    let focus = super::recovery_focus::native_snapshot(failure.result)?;
    let check = focus.pending_check()?.to_owned();
    let failed_edit = auto_test_outcome(failure.name, failure.output) == AutoTestOutcome::Failed
        && serde_json::from_str::<Value>(failure.args).ok()?["path"] == patch.path;
    let failed_exec = failure.name == "exec"
        && crate::execution_evidence::exec_outcome(failure.output) == Some(false)
        && serde_json::from_str::<Value>(failure.args).ok()?["command"] == check
        && focus
            .required_read()
            .is_some_and(|read| read.path == patch.path)
        && crate::execution_evidence::exec_cwd(failure.output)
            == crate::execution_evidence::exec_cwd(proposal.output);
    if failure.id == proposal.id
        || !(failed_edit || failed_exec)
        || focus.required_read().is_some_and(|r| r.path != patch.path)
    {
        return None;
    }
    let mut after = suffix;
    let read = loop {
        let event = take_exchange(&mut after)?;
        if event.id == proposal.id {
            break None;
        }
        if event.name == "read_file"
            && serde_json::from_str::<ReadRequest>(event.args).ok()?.path == patch.path
        {
            break Some(event);
        }
    };
    Some(History {
        failure,
        proposal,
        read,
        patch,
        check,
    })
}

// Protect the last state-changing exchange even if the proposal has expired.
// Dropping that boundary could otherwise reveal an older blocked patch again.
pub(in crate::tui::agent) fn protected_ids(messages: &[Value]) -> Vec<String> {
    let task = crate::task_origin::current_task_messages(messages);
    if !task
        .iter()
        .any(|m| m["role"] == "tool" && m["content"].as_str().is_some_and(held))
    {
        return Vec::new();
    }
    let mut ids: Vec<_> = task
        .iter()
        .rev()
        .filter_map(|m| m["tool_calls"].as_array())
        .find(|calls| {
            calls
                .iter()
                .any(|c| !c["function"]["name"].as_str().is_some_and(observation))
        })
        .into_iter()
        .flatten()
        .filter_map(|c| c["id"].as_str().map(str::to_owned))
        .collect();
    if let Some(h) = history(task) {
        ids.push(h.failure.id.to_owned());
        ids.extend(h.read.map(|r| r.id.to_owned()));
    }
    ids
}

pub(in crate::tui::agent) fn retained_read(messages: &[Value]) -> Option<String> {
    let read = history(messages)?.read?;
    (read.output.len() <= 32 * 1024).then(|| read.id.to_owned())
}

fn contents(workspace: &Path, path: &str) -> Option<String> {
    let target = workspace.join(path).canonicalize().ok()?;
    if !target.starts_with(workspace) {
        return None;
    }
    let meta = target.metadata().ok()?;
    if !meta.is_file() || meta.len() > FILE_LIMIT {
        return None;
    }
    let mut bytes = String::new();
    std::fs::File::open(target)
        .ok()?
        .take(FILE_LIMIT + 1)
        .read_to_string(&mut bytes)
        .ok()?;
    (bytes.len() as u64 <= FILE_LIMIT).then_some(bytes)
}

fn confirmed(read: &Exchange<'_>, patch: &Patch, source: &str, first: usize, last: usize) -> bool {
    if full_read(read, &patch.path) == Some(source) {
        return true;
    }
    let Some((header, body)) = read.output.split_once('\n') else {
        return false;
    };
    let Some((start, end)) = serde_json::from_str::<ReadRequest>(read.args)
        .ok()
        .and_then(|r| r.bounds().ok())
    else {
        return false;
    };
    start <= first
        && last <= end
        && matches_range_header(read.args, header)
        && !header.contains(" [pruned ")
        && source
            .split_inclusive('\n')
            .skip(start - 1)
            .take(end - start + 1)
            .eq(body.split_inclusive('\n'))
}

pub(in crate::tui::agent) fn project(messages: &[Value], workspace: &str) -> Option<NextAction> {
    let h = history(messages)?;
    let root = Path::new(workspace).canonicalize().ok()?;
    if Path::new(crate::execution_evidence::exec_cwd(h.proposal.output)?)
        .canonicalize()
        .ok()?
        != root
    {
        return None;
    }
    let source = contents(&root, &h.patch.path)?;
    let mut matches = source.match_indices(&h.patch.search);
    let (at, _) = matches.next()?;
    if matches.next().is_some() {
        return None;
    }
    let first = source[..at].bytes().filter(|b| *b == b'\n').count() + 1;
    let last = first
        + h.patch
            .search
            .strip_suffix('\n')
            .unwrap_or(&h.patch.search)
            .bytes()
            .filter(|b| *b == b'\n')
            .count();
    if source
        .split_inclusive('\n')
        .skip(first - 1)
        .take(last - first + 1)
        .flat_map(str::chars)
        .nth(crate::file_tools::MAX_READ_CHARS)
        .is_some()
    {
        return None;
    }
    let range = json!({"path":h.patch.path,"start_line":first,"end_line":last});
    serde_json::from_value::<ReadRequest>(range.clone())
        .ok()?
        .bounds()
        .ok()?;
    let read = h.read.filter(|r| {
        r.id != h.failure.id
            && r.id != h.proposal.id
            && super::recovery_focus::native_snapshot(r.result)
                .is_some_and(|focus| focus.check_matches(&h.check) && !focus.requires_read())
            && confirmed(r, &h.patch, &source, first, last)
    });
    let (suggested_tool, suggested_args, instruction) = if read.is_some() {
        ("patch_file", json!(h.patch), "The diagnostic read covers your earlier proposal and its exact search still occurs once. Reconsider it against the objective. If appropriate, submit it as a new ordinary tool call, then verify the change.")
    } else {
        ("read_file", range, "Your earlier patch was held for diagnosis. Inspect this current source window. If it explains the failure, reconsider the proposal after the read.")
    };
    Some(NextAction {
        proposal_call_id: h.proposal.id.into(),
        failed_check: h.check,
        confirming_read_call_id: read.map(|r| r.id.to_owned()),
        suggested_tool,
        suggested_args,
        instruction,
    })
}

/// A resumed generic Diagnose can outlive its completed read. Admit only the
/// same newly submitted patch, independently of the model's interpretation.
pub(in crate::tui::agent) fn matches_retry(
    messages: &[Value],
    call: &crate::streaming::ToolCallData,
    root: Option<&str>,
    focus: &super::recovery_focus::RecoveryFocus,
    edits: &crate::tui::agent::EditFailureMemory,
) -> bool {
    if call.name != "patch_file" || focus.requires_read() || edits.count() != 0 {
        return false;
    }
    let Some((assistant, before)) = messages.split_last() else {
        return false;
    };
    let Some([current]) = assistant["tool_calls"].as_array().map(Vec::as_slice) else {
        return false;
    };
    if assistant["role"] != "assistant" || current["id"] != call.id {
        return false;
    }
    let Some(offer) = root.and_then(|root| project(before, root)) else {
        return false;
    };
    offer.confirming_read_call_id.is_some()
        && focus.check_matches(&offer.failed_check)
        && serde_json::from_str::<Value>(&call.arguments).ok().as_ref()
            == Some(&offer.suggested_args)
}

#[cfg(test)]
#[path = "diagnosis_tests.rs"]
mod tests;
