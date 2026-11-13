//! Preserve one bounded test revision from correlated native results.
use super::AuthoredTests;
use crate::execution_evidence::{exec_may_have_run, file_edit_succeeded};
use crate::file_tools::read_range::fresh_full_body;
use crate::tui::agent::{
    exec_verification::ExecVerificationContext, recovery_focus::correlated_results, ExecKind,
};
use serde::Serialize;
use serde_json::Value;

const MAX_SOURCE_BYTES: usize = 8192;

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum Origin {
    Read,
    Write,
}

#[derive(Debug, Serialize)]
pub(super) struct ReviewSource {
    path: String,
    call_id: String,
    origin: Origin,
    source: String,
}

pub(super) fn project(
    tests: &AuthoredTests,
    messages: &[Value],
    context: &ExecVerificationContext<'_>,
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
        let Some(authored_path) = args["path"].as_str().filter(|path| tests.contains(path)) else {
            continue;
        };
        let path = authored_path.trim_start_matches("./");
        let attempted = exec_may_have_run(output) && !output.starts_with("Awaiting approval");
        let changed = file_edit_succeeded(&call.name, output);
        let source = if call.name == "read_file" {
            fresh_full_body(authored_path, output).map(|body| (Origin::Read, body))
        } else if call.name == "write_file" && changed {
            args["content"].as_str().and_then(|body| {
                let header = format!(
                    "OK: wrote '{authored_path}' ({} lines, {} bytes)",
                    body.lines().count(),
                    body.len()
                );
                (output.lines().next() == Some(header.as_str())).then_some((Origin::Write, body))
            })
        } else {
            None
        };
        if let Some((origin, source)) =
            source.filter(|(_, source)| source.len() <= MAX_SOURCE_BYTES)
        {
            observation = Some(ReviewSource {
                path: path.to_owned(),
                call_id: call.id,
                origin,
                source: source.to_owned(),
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
