//! A literal fixture input from an observed producer, while its requested test is unwritten.
use super::AuthoredTests;
use crate::execution_evidence::{exec_may_have_run, file_edit_succeeded};
use crate::file_tools::read_range::fresh_full_body;
use crate::tui::agent::{
    exec_verification::ExecVerificationContext, recovery_focus::correlated_results, ExecKind,
};
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Serialize)]
pub(in crate::tui::agent) struct FixtureInput {
    requirement_id: String,
    test_path: String,
    helper_path: String,
    helper_read_call_id: String,
    source_line: usize,
    data_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data_read_call_id: Option<String>,
}

fn producer_named(description: &str, path: &str) -> bool {
    let Some(stem) = path.strip_suffix(".py") else {
        return false;
    };
    let module = stem.replace('/', ".");
    description
        .split(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '/' | '-')))
        .map(|token| token.trim_end_matches('.'))
        .any(|token| {
            token == path
                || token == module
                || token
                    .strip_prefix(&module)
                    .is_some_and(|suffix| suffix.starts_with('.') && suffix.len() > 1)
        })
}

/// Recognize a simple assignment from a literal sibling. Never evaluate Python.
fn literal_input(source: &str) -> Option<(usize, &str)> {
    let mut quoted = None::<&str>;
    let mut inputs = source.lines().enumerate().filter_map(|(i, line)| {
        if let Some(marker) = quoted {
            if line.matches(marker).count() % 2 == 1 {
                quoted = None;
            }
            return None;
        }
        if line.trim_start().starts_with('#') {
            return None;
        }
        for marker in ["\"\"\"", "'''"] {
            if line.contains(marker) {
                quoted = (line.matches(marker).count() % 2 == 1).then_some(marker);
                return None;
            }
        }
        let (binding, value) = line.trim().split_once('=')?;
        let binding = binding.trim();
        if !binding.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
            || !binding
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'_')
        {
            return None;
        }
        let token = value
            .trim()
            .strip_prefix("json.loads(Path(__file__).with_name(")?
            .strip_suffix(").read_text())")?;
        let quote = token.chars().next().filter(|c| matches!(c, '\'' | '"'))?;
        let name = token.strip_prefix(quote)?.strip_suffix(quote)?;
        let stem = name.strip_suffix(".json")?;
        (name.len() <= 120
            && !stem.is_empty()
            && stem
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_')))
        .then_some((i + 1, name))
    });
    let input = inputs.next()?;
    inputs.next().is_none().then_some(input)
}

pub(super) fn project(
    tests: &AuthoredTests,
    messages: &[Value],
    context: &ExecVerificationContext<'_>,
    root: Option<&str>,
) -> Option<FixtureInput> {
    let pending: Vec<_> = tests
        .test_only_requirements
        .iter()
        .filter(|r| !r.explicit_cases.is_empty())
        .flat_map(|r| {
            tests
                .targets
                .iter()
                .filter(|t| t.latest_changed_call_id.is_none() && t.requirement_ids.contains(&r.id))
                .map(move |t| (r, t))
        })
        .collect();
    if pending.is_empty() {
        return None;
    }
    let mut input: Option<FixtureInput> = None;
    let mut data_reads = BTreeMap::new();
    for (call, result) in correlated_results(messages) {
        let Some(output) = result["content"].as_str() else {
            continue;
        };
        let Ok(args) = serde_json::from_str::<Value>(&call.arguments) else {
            continue;
        };
        if call.name == "exec"
            && exec_may_have_run(output)
            && !output.trim_start().starts_with("Awaiting approval")
            && args["command"]
                .as_str()
                .is_some_and(|c| context.classify(c).kind == ExecKind::Action)
        {
            input = None;
            data_reads.clear();
        }
        let Some(raw) = args["path"].as_str() else {
            continue;
        };
        let Some(path) = crate::task_scope::relative_path(raw, root) else {
            continue;
        };
        if file_edit_succeeded(&call.name, output) {
            data_reads.remove(&path);
            if input.as_ref().is_some_and(|i| i.helper_path == path) {
                input = None;
            }
        }
        if call.name != "read_file" {
            continue;
        }
        if path.ends_with(".json") {
            if let Some(body) = fresh_full_body(raw, output) {
                if serde_json::from_str::<Value>(body).is_ok() {
                    data_reads.insert(path, call.id);
                } else {
                    data_reads.remove(&path);
                }
            } else if output.starts_with("ERROR") {
                data_reads.remove(&path);
            }
            continue;
        }
        let mut linked = pending
            .iter()
            .filter(|(r, _)| producer_named(&r.description, &path));
        let Some((r, t)) = linked.next() else {
            continue;
        };
        if linked.next().is_some() {
            continue;
        }
        let Some(body) = fresh_full_body(raw, output) else {
            if output.starts_with("ERROR") && input.as_ref().is_some_and(|i| i.helper_path == path)
            {
                input = None;
            }
            continue;
        };
        input = literal_input(body).map(|(source_line, name)| FixtureInput {
            requirement_id: r.id.clone(),
            test_path: t.path.clone(),
            helper_read_call_id: call.id,
            source_line,
            data_path: path
                .rsplit_once('/')
                .map_or_else(|| name.to_owned(), |(dir, _)| format!("{dir}/{name}")),
            helper_path: path,
            data_read_call_id: None,
        });
    }
    let mut input = input?;
    input.data_read_call_id = data_reads.remove(&input.data_path);
    Some(input)
}
