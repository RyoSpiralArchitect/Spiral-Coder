//! Recover an authored one-file proposal across the exact verification that held it.
//! This is a task-local advisory. Only a new ordinary file-tool call can edit the file.
use super::{exec_proof::ExecProofResult, recovery_focus, requirement_proof};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Serialize)]
pub(crate) struct Proposal {
    proposal_call_id: String,
    verification_call_id: String,
    verified_command: String,
    suggested_tool: &'static str,
    suggested_args: Edit,
    instruction: &'static str,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Edit {
    path: String,
    diff: String,
}

#[derive(Deserialize)]
struct Verification {
    command: String,
    cwd: Option<String>,
}

const INSTRUCTION: &str = "The exact check that held this one-file proposal passed. Review the target. If the edit still fits, submit suggested_tool with suggested_args unchanged as one new call. Send edits to other files separately. The proposal has not executed.";

struct Exchange<'a> {
    id: &'a str,
    name: &'a str,
    args: &'a str,
    result: &'a Value,
    output: &'a str,
}

// Consume only adjacent, correlated single-call exchanges. System context and an
// explicit runtime continuation or assistant prose can separate exchanges.
fn take_exchange<'a>(history: &mut &'a [Value]) -> Option<Exchange<'a>> {
    loop {
        let (result, rest) = history.split_last()?;
        *history = rest;
        match result["role"].as_str()? {
            "system" => continue,
            "assistant"
                if result
                    .get("tool_calls")
                    .is_none_or(|v| v.as_array().is_some_and(Vec::is_empty)) =>
            {
                continue
            }
            "user"
                if result["origin"] == "runtime"
                    && result["content"].as_str().is_some_and(|text| {
                        text == "Continue."
                            || ["en", "ja", "fr"]
                                .iter()
                                .any(|lang| text == crate::task_origin::continuation_prompt(lang))
                    }) =>
            {
                continue
            }
            "tool" => {}
            _ => return None,
        }
        let (assistant, rest) = history.split_last()?;
        *history = rest;
        let [call] = assistant["tool_calls"].as_array()?.as_slice() else {
            return None;
        };
        let id = result["tool_call_id"].as_str()?;
        if assistant["role"] != "assistant" || id.is_empty() || id.len() > 240 || call["id"] != id {
            return None;
        }
        return Some(Exchange {
            id,
            name: call["function"]["name"].as_str()?,
            args: call["function"]["arguments"].as_str()?,
            result,
            output: result["content"].as_str()?,
        });
    }
}

fn full_read<'a>(read: &Exchange<'a>, path: &str) -> Option<&'a str> {
    if read.name != "read_file"
        || serde_json::from_str::<crate::file_tools::read_range::ReadRequest>(read.args)
            .ok()?
            .path
            != path
    {
        return None;
    }
    let (header, body) = read.output.split_once('\n')?;
    (body.len() <= 32 * 1024
        && header
            == format!(
                "[{path}] ({} lines, {} bytes)",
                body.lines().count(),
                body.len()
            ))
    .then_some(body)
}

pub(super) fn project(messages: &[Value]) -> Option<Proposal> {
    let mut history = crate::task_origin::current_task_messages(messages);
    let last = take_exchange(&mut history)?;
    // One fresh reread of the unchanged target may occur before resubmission.
    let (check, reread) = if last.name == "read_file" {
        (take_exchange(&mut history)?, Some(last))
    } else {
        (last, None)
    };
    if check.name != "exec"
        || !ExecProofResult::from_history(check.output).succeeded
        || recovery_focus::native_snapshot(check.result)?.is_pending()
    {
        return None;
    }
    let verified: Verification = serde_json::from_str(check.args).ok()?;
    let blocked = take_exchange(&mut history)?;
    if blocked.name != "apply_diff" || blocked.args.len() > 4608 || blocked.id == check.id {
        return None;
    }
    let read = take_exchange(&mut history)?;
    if read.id == blocked.id || read.id == check.id {
        return None;
    }
    let held = recovery_focus::native_snapshot(read.result)?;
    if !held.repaired() || !held.check_matches(&verified.command) {
        return None;
    }
    let (cwd, reason) = blocked
        .output
        .strip_prefix("GOVERNOR BLOCKED\ncwd: ")?
        .split_once("\n\n")?;
    if !std::path::Path::new(cwd).is_absolute()
        || check.output.lines().nth(1)?.strip_prefix("cwd: ")? != cwd
        || verified
            .cwd
            .as_deref()
            .is_some_and(|selected| selected != cwd)
        || reason
            != format!(
                "[Recovery Gate] stage=verify\n{}\n\ntool:\napply_diff\narguments:\n{}",
                held.hint()?,
                blocked.args
            )
    {
        return None;
    }
    let edit: Edit = serde_json::from_str(blocked.args).ok()?;
    if edit.path.is_empty()
        || edit.path.len() > 240
        || edit.path.contains(['\\', ':'])
        || edit.path.chars().any(char::is_control)
        || edit.path.split('/').any(|s| matches!(s, "" | "." | ".."))
        || edit.diff.len() > 4096
    {
        return None;
    }
    let task = crate::task_origin::root_user_text(messages);
    let eligible_path = if let Some(scope) = super::explicit_edit_scope::Scope::parse(task) {
        scope.allows(&edit.path, None)
    } else {
        requirement_proof::parse(task)
            .ok()?
            .is_none_or(|c| c.names_artifact(&edit.path))
    };
    if task.is_empty()
        || !eligible_path
        || crate::task_scope::artifact_scope(task)
            .is_some_and(|scope| !scope.target_files.contains(&edit.path))
    {
        return None;
    }
    let original = full_read(&read, &edit.path)?;
    if let Some(reread) = reread {
        if full_read(&reread, &edit.path)? != original {
            return None;
        }
    }
    // Preview the bytes the model actually saw. The live tool rechecks its
    // target and its normal approval policy when a new call is submitted.
    if !crate::file_tools::edit_preview::resumable(&edit.path, original, &edit.diff) {
        return None;
    }
    Some(Proposal {
        proposal_call_id: blocked.id.into(),
        verification_call_id: check.id.into(),
        verified_command: verified.command,
        suggested_tool: "apply_diff",
        suggested_args: edit,
        instruction: INSTRUCTION,
    })
}

#[cfg(test)]
mod tests;
