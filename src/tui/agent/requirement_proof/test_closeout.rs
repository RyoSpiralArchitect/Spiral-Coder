//! Authored tests against the requirements that link them.
//!
//! The original task can name one test artifact for several requirements. The
//! authored-test view keeps those links visible, but it is advisory: once the
//! exact checks pass, `done` is admissible even if the test file exercises only
//! a fraction of them. Three one-time steps keep the linked cases in front of
//! the Coder while it can still act, each only while it can afford an edit,
//! every declared check and the report:
//!
//! - when a linked test file has been written, the first file change whose
//!   automatic test did not fail carries a plan: each linked requirement in
//!   full beside the current test functions, and a request for a case-to-test
//!   map;
//! - the first completion claim gets the same review before acceptance;
//! - after that review, the first completion claim whose `done.summary` lacks a
//!   valid `Test coverage:` section is returned once.
//!
//! A coverage map supplies no completion evidence. When the author explicitly
//! reports a mandatory enumerated test case as uncovered, a successful report
//! contradicts that admission. An honest blocked report remains available.
use super::{ArtifactKind, Contract, Requirement};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

pub(super) const PLAN_MARKER: &str = "[Test coverage plan]";
pub(super) const MARKER: &str = "[Authored-test closeout]";
pub(super) const COVERAGE_MARKER: &str = "[Test coverage map]";
const DECLARED_CASE_MARKER: &str = "[Declared test cases]";
// Whole requirement text: S28's review clipped at 160 characters and cut the
// sentence that named the four fault families to cover.
const MAX_DESCRIPTION_CHARS: usize = 1200;
const MAX_LISTED_TESTS: usize = 24;
const MAP_FORMAT: &str =
    "- <requirement id>: `test_function_name`, `another_test`\n- <requirement id>: none (reason)";
// Shared by the plan and the review.
const CASES: &str = "for each requirement, list every distinct case its text names (each rejection, mode, boundary or output rule) and the test function that exercises it, or `none`. Add a focused public test for each `none` case that the public command can exercise, starting with cases a requirement explicitly asks tests to cover, in one edit where possible; each starts from a passing control and changes one invariant.";

fn shown(messages: &[Value], marker: &str) -> bool {
    messages.iter().any(|message| {
        message["role"] == "tool"
            && message["content"]
                .as_str()
                .is_some_and(|content| content.contains(marker))
    })
}

/// Test artifacts that the original task links to two or more requirements.
fn linked(contract: &Contract) -> BTreeMap<&str, Vec<&Requirement>> {
    let mut linked: BTreeMap<&str, Vec<&Requirement>> = BTreeMap::new();
    for requirement in &contract.requirements {
        let paths: BTreeSet<&str> = requirement
            .artifacts
            .iter()
            .filter(|artifact| matches!(artifact.kind, ArtifactKind::Test))
            .map(|artifact| artifact.path.trim_start_matches("./"))
            .collect();
        for path in paths {
            linked.entry(path).or_default().push(requirement);
        }
    }
    linked.retain(|_, requirements| {
        requirements.len() >= 2
            || requirements
                .iter()
                .any(|r| !r.explicit_test_cases().is_empty())
    });
    linked
}

fn current_tests(tool_root: Option<&str>, path: &str) -> Option<Vec<String>> {
    tool_root
        .and_then(|root| std::fs::read_to_string(Path::new(root).join(path)).ok())
        .map(|source| test_names(path, &source))
}

fn listed(names: &[String]) -> String {
    let shown = names
        .iter()
        .take(MAX_LISTED_TESTS)
        .cloned()
        .collect::<Vec<_>>();
    format!(
        "{}{}",
        shown.join(", "),
        if names.len() > shown.len() {
            ", …"
        } else {
            ""
        }
    )
}

/// One edit, every declared check after it, and the final report.
pub(super) fn floor(contract: &Contract) -> usize {
    let checks: BTreeSet<&str> = contract
        .requirements
        .iter()
        .flat_map(|requirement| &requirement.checks)
        .map(|check| check.command.trim())
        .collect();
    checks.len() + 2
}

fn allowance(coder_remaining: Option<usize>) -> String {
    coder_remaining
        .map(|left| format!(" Remaining Coder allowance: {left}."))
        .unwrap_or_default()
}

/// A linked test file, its requirements in full, and its current tests.
fn section(path: &str, requirements: &[&Requirement], tool_root: Option<&str>) -> String {
    let mut text = format!(
        "\n\n{path} is linked by the original task to {} requirements:",
        requirements.len()
    );
    for requirement in requirements {
        text.push_str(&format!(
            "\n- {}: {}",
            requirement.id,
            clip(&requirement.description)
        ));
        for case in requirement.explicit_test_cases() {
            text.push_str(&format!("\n  Map this explicitly requested case in the same section: - {} / {case}: `current_test_function` or none (remaining work)", requirement.id));
        }
    }
    match current_tests(tool_root, path) {
        Some(names) if !names.is_empty() => text.push_str(&format!(
            "\nCurrent test functions ({}): {}",
            names.len(),
            listed(&names)
        )),
        Some(_) => text.push_str("\nCurrent test functions: none recognized."),
        None => text.push_str("\nCurrent test functions: the file could not be read."),
    }
    let controls = crate::tui::agent::task_obligations::test_controls(
        requirements
            .iter()
            .any(|r| super::authored_tests::names_record_order(&r.description)),
        requirements.iter().flat_map(|r| r.explicit_test_cases()),
    );
    if !controls.is_empty() {
        text.push('\n');
        text.push_str(controls.trim_end());
    }
    text
}

/// Linked test files with a successful native change in the current task,
/// including the change being reported now.
fn written<'a>(
    linked: &BTreeMap<&'a str, Vec<&'a Requirement>>,
    messages: &[Value],
    changed_path: &str,
) -> BTreeSet<&'a str> {
    let earlier = crate::tui::agent::recovery_focus::correlated_results(messages)
        .filter(|(call, message)| {
            crate::execution_evidence::file_edit_succeeded(
                &call.name,
                message["content"].as_str().unwrap_or(""),
            )
        })
        .filter_map(|(call, _)| {
            serde_json::from_str::<Value>(&call.arguments).ok()?["path"]
                .as_str()
                .map(str::to_owned)
        });
    std::iter::once(changed_path.to_owned())
        .chain(earlier)
        .filter_map(|path| {
            linked
                .get_key_value(path.trim_start_matches("./"))
                .map(|(path, _)| *path)
        })
        .collect()
}

/// Once a linked test file has been written in the current task, the first
/// successful file change whose automatic test did not fail carries one plan
/// (the caller checks the change). S30 wrote its test file with a full
/// iteration ahead but claimed completion too late for the review, and its
/// tests missed three of the four fault families the task named. `messages`
/// are the current task's messages, before the reported change's result.
pub(super) fn plan(
    contract: &Contract,
    messages: &[Value],
    tool_root: Option<&str>,
    changed_path: &str,
    coder_remaining: Option<usize>,
) -> Option<String> {
    if shown(messages, PLAN_MARKER) || shown(messages, MARKER) {
        return None;
    }
    let linked = linked(contract);
    if linked.is_empty() || coder_remaining.is_some_and(|left| left < floor(contract)) {
        return None;
    }
    let written = written(&linked, messages, changed_path);
    if written.is_empty() {
        return None;
    }
    let mut text = format!(
        "{PLAN_MARKER} A test file linked to several requirements has been written. Plan its cases while edit budget remains."
    );
    for (path, requirements) in linked.iter().filter(|(path, _)| written.contains(*path)) {
        text.push_str(&section(path, requirements, tool_root));
    }
    text.push_str(&format!(
        "\n\nMap the cases now: {CASES} Follow any current recovery obligation first, then run every declared check after the last edit. When you call `done`, put the final map in `done.summary` under a `Test coverage:` line, one line per requirement ID, test functions in backticks:\n{MAP_FORMAT}\nThis plan is shown once per task and is not completion evidence.{}",
        allowance(coder_remaining)
    ));
    Some(text)
}

/// `messages` are the current task's messages.
pub(super) fn review(
    contract: &Contract,
    messages: &[Value],
    tool_root: Option<&str>,
    coder_remaining: Option<usize>,
) -> Option<String> {
    if shown(messages, MARKER) {
        return None;
    }
    let linked = linked(contract);
    if linked.is_empty() || coder_remaining.is_some_and(|left| left < floor(contract)) {
        return None;
    }

    let mut text = format!(
        "{MARKER} Before completion, map each requirement linked to an authored test to the tests that exercise it."
    );
    for (path, requirements) in &linked {
        text.push_str(&section(path, requirements, tool_root));
    }
    text.push_str(&format!(
        "\n\nWrite the coverage map before editing: {CASES} Then run every declared check again after the last edit, and call `done` with the final map in `done.summary` under a `Test coverage:` line, one line per requirement ID, test functions in backticks:\n{MAP_FORMAT}\nThis review is shown once per task and is not completion evidence.{}",
        allowance(coder_remaining)
    ));
    Some(text)
}

/// After the review, the first completion claim needs the requested map: a
/// `Test coverage:` section with a line for every linked requirement that names
/// current test functions in backticks or says `none`. The map is not checked
/// for adequacy, only for shape and for test functions that exist. Returned
/// once per task, and only while the Coder can still answer.
pub(super) fn coverage(
    contract: &Contract,
    messages: &[Value],
    tool_root: Option<&str>,
    summary: &str,
    coder_remaining: Option<usize>,
) -> Option<String> {
    if !shown(messages, MARKER) {
        return None;
    }
    let linked = linked(contract);
    if linked.is_empty() {
        return None;
    }
    let entries = coverage_entries(summary);
    let case_keys: BTreeSet<String> = linked
        .values()
        .flatten()
        .flat_map(|r| {
            r.explicit_test_cases()
                .into_iter()
                .map(|case| format!("{} / {case}", r.id))
        })
        .collect();
    let mut open = BTreeSet::new();
    if !case_keys.is_empty() {
        for (call, result) in crate::tui::agent::recovery_focus::correlated_results(messages) {
            if call.name != "done"
                || !result["content"]
                    .as_str()
                    .is_some_and(|s| s.starts_with("GOVERNOR BLOCKED\n\n[Declared test cases]"))
            {
                continue;
            }
            let Ok(args) = serde_json::from_str::<Value>(&call.arguments) else {
                continue;
            };
            if let Some(old) = args["summary"].as_str().and_then(coverage_entries) {
                open.extend(
                    case_keys
                        .iter()
                        .filter(|key| old.get(*key).is_some_and(|v| explicit_none(v)))
                        .cloned(),
                );
            }
        }
        if let Some(entries) = &entries {
            if !open.is_empty() {
                let available: Option<Vec<_>> =
                    linked.keys().map(|p| current_tests(tool_root, p)).collect();
                open.retain(|key| {
                    !entries.get(key).is_some_and(|v| {
                        let named = backticked(v);
                        !named.is_empty()
                            && available.as_ref().is_none_or(|sets| {
                                named
                                    .iter()
                                    .all(|name| sets.iter().flatten().any(|test| test == name))
                            })
                    })
                });
            }
            open.extend(
                case_keys
                    .iter()
                    .filter(|key| entries.get(*key).is_some_and(|v| explicit_none(v)))
                    .cloned(),
            );
        }
    }
    if !open.is_empty() {
        return Some(format!("{DECLARED_CASE_MARKER} A completion report marks requested tests as uncovered: {}. Implement or cite the current public tests for these cases, then rerun the declared checks after an edit. If they remain open, submit a blocked report with the affected remaining_acceptance and next_steps.",open.into_iter().collect::<Vec<_>>().join(", ")));
    }
    if shown(messages, COVERAGE_MARKER) || coder_remaining == Some(0) {
        return None;
    }
    let mut known = BTreeSet::new();
    let mut readable = true;
    let mut current = Vec::new();
    for path in linked.keys() {
        match current_tests(tool_root, path) {
            Some(names) => {
                current.push(format!("{path}: {}", listed(&names)));
                known.extend(names);
            }
            None => readable = false,
        }
    }
    let mut ids: BTreeSet<&str> = linked
        .values()
        .flatten()
        .map(|requirement| requirement.id.as_str())
        .collect();
    ids.extend(case_keys.iter().map(String::as_str));
    let mut problems = Vec::new();
    match &entries {
        None => problems.push("There is no `Test coverage:` section.".to_string()),
        Some(entries) => {
            let missing: Vec<&str> = ids
                .iter()
                .copied()
                .filter(|id| !entries.contains_key(*id))
                .collect();
            let unnamed: Vec<&str> = ids
                .iter()
                .copied()
                .filter(|id| {
                    entries
                        .get(*id)
                        .is_some_and(|value| backticked(value).is_empty() && !says_none(value))
                })
                .collect();
            let unknown: BTreeSet<&str> = if readable {
                entries
                    .iter()
                    .filter(|(id, _)| ids.contains(id.as_str()))
                    .flat_map(|(_, value)| backticked(value))
                    .filter(|name| !known.contains(*name))
                    .collect()
            } else {
                BTreeSet::new()
            };
            if !missing.is_empty() {
                problems.push(format!(
                    "Missing requirement lines: {}.",
                    missing.join(", ")
                ));
            }
            if !unnamed.is_empty() {
                problems.push(format!(
                    "Lines that name no test function and do not say none: {}.",
                    unnamed.join(", ")
                ));
            }
            if !unknown.is_empty() {
                problems.push(format!(
                    "Not current test functions: {}.",
                    unknown
                        .iter()
                        .map(|name| format!("`{name}`"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
        }
    }
    if problems.is_empty() {
        return None;
    }
    let mut text = format!(
        "{COVERAGE_MARKER} The authored-test review asked for the coverage map in `done.summary`. Add a `Test coverage:` section with one line per linked requirement:\n{MAP_FORMAT}\n{}",
        problems.join("\n")
    );
    if !current.is_empty() {
        text.push_str(&format!(
            "\nCurrent test functions in {}.",
            current.join("; ")
        ));
    }
    text.push_str("\nIf a case in the map has no test yet, add it and rerun every declared check after the last edit before calling `done` again. This check is shown once per task and is not completion evidence.");
    Some(text)
}

/// Lines of the `Test coverage:` section, keyed by requirement ID.
fn coverage_entries(summary: &str) -> Option<BTreeMap<String, String>> {
    let mut lines = summary.lines();
    lines.find(|line| {
        line.trim()
            .trim_start_matches(['#', '*', ' '])
            .to_ascii_lowercase()
            .replace('*', "")
            .starts_with("test coverage:")
    })?;
    let mut entries = BTreeMap::new();
    for line in lines {
        let item = line.trim();
        if item.is_empty() {
            if entries.is_empty() {
                continue;
            }
            break;
        }
        let item = item
            .strip_prefix("- ")
            .or_else(|| item.strip_prefix("* "))
            .unwrap_or(item);
        let Some((id, value)) = item.split_once(':') else {
            break;
        };
        entries.insert(
            id.trim().trim_matches(['`', '*']).trim().to_string(),
            value.trim().to_string(),
        );
    }
    Some(entries)
}

fn backticked(value: &str) -> Vec<&str> {
    value
        .split('`')
        .skip(1)
        .step_by(2)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .collect()
}

fn says_none(value: &str) -> bool {
    value
        .to_ascii_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .any(|word| word == "none")
}

fn explicit_none(value: &str) -> bool {
    backticked(value).is_empty()
        && value
            .split(|c: char| !c.is_alphanumeric())
            .next()
            .is_some_and(|w| w.eq_ignore_ascii_case("none"))
}

fn clip(description: &str) -> String {
    let one_line = description.split_whitespace().collect::<Vec<_>>().join(" ");
    match one_line.char_indices().nth(MAX_DESCRIPTION_CHARS) {
        Some((cut, _)) => format!("{}…", &one_line[..cut]),
        None => one_line,
    }
}

/// Test function names recognized by language; unknown file types list none.
fn test_names(path: &str, source: &str) -> Vec<String> {
    let identifier = |text: &str| -> String {
        text.chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect()
    };
    let extension = Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("");
    let mut names = Vec::new();
    match extension {
        "rs" => {
            let mut attributed = false;
            for line in source.lines().map(str::trim) {
                if line.starts_with("#[test]") || line.starts_with("#[tokio::test") {
                    attributed = true;
                } else if attributed && !line.starts_with("#[") && !line.is_empty() {
                    let line = line.strip_prefix("pub ").unwrap_or(line);
                    let line = line.strip_prefix("async ").unwrap_or(line);
                    if let Some(rest) = line.strip_prefix("fn ") {
                        let name = identifier(rest);
                        if !name.is_empty() {
                            names.push(name);
                        }
                    }
                    attributed = false;
                }
            }
        }
        "py" => {
            for line in source.lines().map(str::trim_start) {
                let line = line.strip_prefix("async ").unwrap_or(line);
                if let Some(rest) = line.strip_prefix("def test") {
                    names.push(format!("test{}", identifier(rest)));
                }
            }
        }
        "js" | "mjs" | "cjs" | "ts" | "tsx" | "jsx" => {
            for line in source.lines().map(str::trim_start) {
                for call in ["test(", "it("] {
                    let Some(rest) = line.strip_prefix(call) else {
                        continue;
                    };
                    let rest = rest.trim_start();
                    let Some(quote) = rest.chars().next().filter(|c| "'\"`".contains(*c)) else {
                        continue;
                    };
                    if let Some(end) = rest[1..].find(quote) {
                        names.push(rest[1..1 + end].to_string());
                    }
                }
            }
        }
        _ => {}
    }
    names
}

#[cfg(test)]
mod tests {
    use super::super::test_closeout as closeout_entry;
    use super::super::test_coverage as coverage_entry;
    use super::*;
    use serde_json::json;

    fn plan_entry(
        messages: &[Value],
        root: Option<&str>,
        path: &str,
        remaining: Option<usize>,
    ) -> Option<String> {
        super::super::test_plan(messages, root, path, remaining).map(|plan| plan.text)
    }

    // S20–S30 share this task text byte for byte.
    const S28_PROMPT: &str =
        include_str!("../../../../benchmarks/self-dogfood-v2/coupled-status/prompt.txt");
    // The test file when S28 iteration 5 first claimed completion (step 96).
    const S28_TESTS: &str = "#[test]\nfn public_cli_matches_protected_fixture_states() {}\n\n#[test]\nfn duplicate_request_id_is_rejected() {}\n\n#[test]\nfn wrong_parent_linkage_is_rejected() {}\n";
    // The test functions S30 iteration 5 first wrote (step 123).
    const S30_TESTS: &str = "use std::process::Command;\n\n#[test]\nfn completed_fixture_is_reported_by_public_cli() {}\n\n#[test]\nfn interrupted_fixture_preserves_pending_summary() {}\n\n#[test]\nfn unknown_fixture_reports_null_usage() {}\n\n#[test]\nfn malformed_manifest_fails_without_stdout() {}\n";

    fn blocked(id: &str, text: &str) -> [Value; 2] {
        [
            json!({"role":"assistant","tool_calls":[{"id":id,"function":{"name":"done","arguments":"{}"}}]}),
            json!({"role":"tool","tool_call_id":id,"content":format!("GOVERNOR BLOCKED\n\n{text}")}),
        ]
    }

    fn edit(id: &str, path: &str, result: &str) -> [Value; 2] {
        [
            json!({"role":"assistant","tool_calls":[{"id":id,"function":{"name":"write_file","arguments":json!({"path":path,"content":"…"}).to_string()}}]}),
            json!({"role":"tool","tool_call_id":id,"content":result}),
        ]
    }

    const TESTS: &str = "tests/coupled_status.rs";

    // The S22 shape: one test artifact linked to eight requirement IDs and two
    // authored tests when the exact checks first all passed.
    fn root(links: usize, checks: &[&str]) -> String {
        let requirements: Vec<Value> = (1..=links)
            .map(|n| {
                json!({"id":format!("behavior-{n}"),
                    "description":format!("Behavior rule {n}: reject the contradictory record and keep stdout empty."),
                    "artifacts":[{"path":"src/coupled_status.rs","kind":"source"},{"path":TESTS,"kind":"test"}],
                    "checks":checks.iter().map(|c| json!({"command":c})).collect::<Vec<_>>()})
            })
            .collect();
        format!(
            "Implement coupled-status.\n<task_requirements>\n{}\n</task_requirements>",
            json!({"version":1,"requirements":requirements})
        )
    }

    fn workspace(source: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("tests")).unwrap();
        std::fs::write(dir.path().join(TESTS), source).unwrap();
        dir
    }

    const TWO_TESTS: &str = "use std::process::Command;\n\n#[test]\nfn completed_fixture_is_reported_by_public_cli() {}\n\n#[test]\nfn unsupported_manifest_version_is_rejected_without_stdout() {}\n";

    fn messages(root: &str) -> Vec<Value> {
        vec![crate::task_origin::user_message(
            root,
            crate::task_origin::MessageOrigin::User,
        )]
    }

    #[test]
    fn linked_requirements_are_reviewed_once_before_completion() {
        let checks = [
            "cargo fmt --all -- --check",
            "cargo test --locked --test coupled_status -q",
        ];
        let dir = workspace(TWO_TESTS);
        let root_dir = dir.path().to_str();
        let mut history = messages(&root(8, &checks));
        let review =
            closeout_entry(&history, root_dir, Some(26)).expect("first completion is reviewed");
        assert!(review.starts_with(MARKER), "{review}");
        assert!(review
            .contains("tests/coupled_status.rs is linked by the original task to 8 requirements"));
        assert!(
            review.contains("- behavior-1: Behavior rule 1") && review.contains("- behavior-8:")
        );
        assert!(review.contains(
            "Current test functions (2): completed_fixture_is_reported_by_public_cli, unsupported_manifest_version_is_rejected_without_stdout"
        ));
        assert!(review.contains("Remaining Coder allowance: 26."));

        history.extend([
            json!({"role":"assistant","tool_calls":[{"id":"done-1","function":{"name":"done","arguments":"{}"}}]}),
            json!({"role":"tool","tool_call_id":"done-1","content":format!("GOVERNOR BLOCKED\n\n{review}")}),
        ]);
        assert_eq!(closeout_entry(&history, root_dir, Some(25)), None);

        // A new task in the same session gets its own single review.
        history.push(crate::task_origin::user_message(
            &root(8, &checks),
            crate::task_origin::MessageOrigin::User,
        ));
        assert!(closeout_entry(&history, root_dir, Some(25)).is_some());
    }

    #[test]
    fn s30_first_test_write_carries_one_plan_with_each_linked_requirement_in_full() {
        // Sealed S30 iteration 5, step 123: the first write of the test file, with
        // 11 Coder requests left in the iteration. The first completion claim came
        // in iteration 6 with 5 left, below the review's floor of 8, so the four
        // tests (they detect only missing pending data) were never compared with
        // the fault families the task names.
        let dir = workspace(S30_TESTS);
        let root_dir = dir.path().to_str();
        let mut history = messages(S28_PROMPT);
        let plan = plan_entry(&history, root_dir, "tests/coupled_status.rs", Some(11))
            .expect("the first test write carries the plan");
        assert!(plan.starts_with(PLAN_MARKER), "{plan}");
        assert!(plan
            .contains("tests/coupled_status.rs is linked by the original task to 9 requirements"));
        assert!(plan.contains(
            "Cover missing pending components, dangling symlinks, duplicate request IDs and actual wrong parent linkage"
        ));
        assert!(plan.contains(
            "Current test functions (4): completed_fixture_is_reported_by_public_cli, interrupted_fixture_preserves_pending_summary, unknown_fixture_reports_null_usage, malformed_manifest_fails_without_stdout"
        ));
        assert!(plan.contains(
            "Map the cases now: for each requirement, list every distinct case its text names"
        ));
        assert!(plan.contains("starting with cases a requirement explicitly asks tests to cover"));
        assert!(plan.contains("under a `Test coverage:` line"));
        assert!(plan.contains("Remaining Coder allowance: 11."));
        // Below the floor (one edit, six declared checks, the report) it waits.
        assert_eq!(
            plan_entry(&history, root_dir, "tests/coupled_status.rs", Some(7)),
            None
        );

        // Once shown, later changes carry none; the completion review still follows.
        history.extend(edit(
            "write-tests",
            TESTS,
            &format!("OK: wrote 'tests/coupled_status.rs' (61 lines, 2347 bytes)\n[auto-test] ✓ PASSED (exit 0)\n\n{plan}"),
        ));
        assert_eq!(
            plan_entry(&history, root_dir, "tests/coupled_status.rs", Some(10)),
            None
        );
        assert!(closeout_entry(&history, root_dir, Some(10)).is_some());
    }

    #[test]
    fn the_plan_needs_a_written_linked_test_file_and_comes_once_per_task() {
        let checks = ["check-a", "check-b"];
        let dir = workspace(TWO_TESTS);
        let root_dir = dir.path().to_str();
        let task = root(3, &checks);
        let mut history = messages(&task);
        // A source change before any linked test file was written carries none,
        // and so does a test file that the task does not link twice.
        assert_eq!(
            plan_entry(&history, root_dir, "src/coupled_status.rs", Some(20)),
            None
        );
        assert_eq!(
            plan_entry(&history, root_dir, "tests/other.rs", Some(20)),
            None
        );
        // The caller skips a change whose automatic test failed. The next change
        // whose automatic test did not fail carries the plan, even in source.
        history.extend(edit(
            "write-tests",
            "./tests/coupled_status.rs",
            "OK: wrote 'tests/coupled_status.rs' (9 lines, 200 bytes)\n[auto-test] ✗ FAILED (exit 101)",
        ));
        let plan = plan_entry(&history, root_dir, "src/coupled_status.rs", Some(20))
            .expect("a written test file carries the plan");
        assert!(plan
            .contains("tests/coupled_status.rs is linked by the original task to 3 requirements"));
        assert!(plan.contains("Remaining Coder allowance: 20."));
        // Failed and unchanged writes are not authoring.
        let mut unwritten = messages(&task);
        unwritten.extend(edit("failed", TESTS, "ERROR: write rejected"));
        unwritten.extend(edit(
            "unchanged",
            TESTS,
            "OK: unchanged 'tests/coupled_status.rs' (identical contents; no write performed)",
        ));
        assert_eq!(
            plan_entry(&unwritten, root_dir, "src/coupled_status.rs", Some(20)),
            None
        );
        // A report-only closeout counts as no allowance; an unknown one plans.
        assert_eq!(plan_entry(&messages(&task), root_dir, TESTS, Some(0)), None);
        assert!(plan_entry(&messages(&task), root_dir, TESTS, None).is_some());
        // After the completion review no plan follows.
        let review = closeout_entry(&history, root_dir, Some(20)).unwrap();
        history.extend(blocked("done-1", &review));
        assert_eq!(plan_entry(&history, root_dir, TESTS, Some(19)), None);
        // A new task in the same session gets its own plan.
        history.push(crate::task_origin::user_message(
            &task,
            crate::task_origin::MessageOrigin::User,
        ));
        assert!(plan_entry(&history, root_dir, TESTS, Some(19)).is_some());
    }

    #[test]
    fn the_review_needs_shared_links_and_room_for_an_edit_every_check_and_a_report() {
        let checks = ["check-a", "check-b"];
        let dir = workspace(TWO_TESTS);
        let root_dir = dir.path().to_str();
        assert_eq!(
            closeout_entry(&messages(&root(8, &checks)), root_dir, Some(3)),
            None
        );
        assert!(closeout_entry(&messages(&root(8, &checks)), root_dir, Some(4)).is_some());
        assert!(closeout_entry(&messages(&root(8, &checks)), root_dir, None).is_some());
        assert_eq!(
            closeout_entry(&messages(&root(1, &checks)), root_dir, Some(26)),
            None
        );
        assert_eq!(
            closeout_entry(&messages("No declared requirements."), root_dir, Some(26)),
            None
        );
        let unreadable = closeout_entry(&messages(&root(2, &checks)), None, Some(26)).unwrap();
        assert!(unreadable.contains("the file could not be read"));
    }

    #[test]
    fn s28_review_shows_each_linked_requirement_in_full_and_asks_for_a_coverage_map() {
        // Sealed S28 iteration 5, step 96: nine requirements linked to the test
        // file and three test functions. The 160-character clip hid the
        // native-tests sentence that names the four fault families; the tests
        // then missed the symlink fault.
        let dir = workspace(S28_TESTS);
        let review = closeout_entry(&messages(S28_PROMPT), dir.path().to_str(), Some(14))
            .expect("the first completion claim is reviewed");
        assert!(review
            .contains("tests/coupled_status.rs is linked by the original task to 9 requirements"));
        assert!(review.contains(
            "Cover missing pending components, dangling symlinks, duplicate request IDs and actual wrong parent linkage"
        ));
        assert!(
            review.contains("and any symlink below the run directory on a manifest/ledger path")
        );
        assert!(review.contains("Missing pending ledger is valid and means unknown usage"));
        assert!(review.contains(
            "Current test functions (3): public_cli_matches_protected_fixture_states, duplicate_request_id_is_rejected, wrong_parent_linkage_is_rejected"
        ));
        assert!(review.contains("list every distinct case its text names"));
        assert!(review.contains("starting with cases a requirement explicitly asks tests to cover"));
        assert!(review.contains("under a `Test coverage:` line"));
        assert!(review.contains("Remaining Coder allowance: 14."));
    }

    #[test]
    fn the_completion_claim_after_the_review_carries_the_coverage_map_once() {
        let checks = [
            "cargo fmt --all -- --check",
            "cargo test --locked --test coupled_status -q",
        ];
        let dir = workspace(TWO_TESTS);
        let root_dir = dir.path().to_str();
        let task = root(3, &checks);
        let mut history = messages(&task);
        // Without the review, no map is asked for.
        assert_eq!(
            coverage_entry(&history, root_dir, "Implemented.", Some(20)),
            None
        );
        let review = closeout_entry(&history, root_dir, Some(26)).unwrap();
        history.extend(blocked("done-1", &review));

        let complete = "Implemented.\n\nTest coverage:\n- behavior-1: `completed_fixture_is_reported_by_public_cli`\n- behavior-2: `unsupported_manifest_version_is_rejected_without_stdout`\n- behavior-3: none (exercised by the visible check)\n";
        assert_eq!(coverage_entry(&history, root_dir, complete, Some(20)), None);
        let missing = coverage_entry(&history, root_dir, "Implemented.", Some(20)).unwrap();
        assert!(missing.starts_with(COVERAGE_MARKER), "{missing}");
        assert!(missing.contains("There is no `Test coverage:` section."));
        let partial = "Test coverage:\n- behavior-1: `invented_symlink_case`\n- behavior-2: covered elsewhere\n";
        let partial = coverage_entry(&history, root_dir, partial, Some(20)).unwrap();
        assert!(
            partial.contains("Missing requirement lines: behavior-3."),
            "{partial}"
        );
        assert!(
            partial.contains("Lines that name no test function and do not say none: behavior-2.")
        );
        assert!(partial.contains("Not current test functions: `invented_symlink_case`."));
        assert!(partial.contains(
            "Current test functions in tests/coupled_status.rs: completed_fixture_is_reported_by_public_cli, unsupported_manifest_version_is_rejected_without_stdout."
        ));
        // A report-only closeout or a spent allowance cannot answer: it proceeds.
        assert_eq!(
            coverage_entry(&history, root_dir, "Implemented.", Some(0)),
            None
        );
        // Returned once per task; the next claim proceeds.
        history.extend(blocked("done-2", &partial));
        assert_eq!(
            coverage_entry(&history, root_dir, "Implemented.", Some(19)),
            None
        );
        // A new task needs its own review before any map is asked for.
        history.push(crate::task_origin::user_message(
            &task,
            crate::task_origin::MessageOrigin::User,
        ));
        assert_eq!(
            coverage_entry(&history, root_dir, "Implemented.", Some(19)),
            None
        );
    }

    #[test]
    fn a_general_test_mapping_cannot_hide_an_explicitly_uncovered_test_case() {
        let file = "tests/coupled_status.rs";
        let mut contract: Contract=serde_json::from_value(json!({"version":1,"requirements":[{
            "id":"boundaries","description":"Exercise the public CLI. Cover empty input, absent input, and invalid encoding; use real data.",
            "artifacts":[{"kind":"test","path":file}],"checks":[{"command":"cargo test"}]}]})).unwrap();
        let temp = workspace(TWO_TESTS);
        let dir = temp.path().to_str();
        let name = "completed_fixture_is_reported_by_public_cli";
        let broad = format!("Test coverage:\n- boundaries: `{name}`");
        let mut messages = vec![json!({"role":"user","content":"test"})];
        messages.extend(blocked("review", MARKER));
        let advice = coverage(&contract, &messages, dir, &broad, Some(18)).unwrap();
        for case in ["empty input", "absent input", "invalid encoding"] {
            assert!(advice.contains(&format!("boundaries / {case}")));
        }
        let mapped=format!("{broad}\n- boundaries / empty input: `{name}`\n- boundaries / absent input: `{name}`\n- boundaries / invalid encoding: `{name}`");
        assert!(
            coverage(&contract, &messages, dir, &mapped, Some(18)).is_none(),
            "parameterized tests can exercise more than one case"
        );
        messages.extend(blocked("map", &advice));
        let uncovered = mapped.replace(
            &format!("invalid encoding: `{name}`"),
            "invalid encoding: none (still missing)",
        );
        assert!(coverage(&contract, &messages, dir, &uncovered, Some(18))
            .unwrap()
            .contains("[Declared test cases]"));
        assert!(
            coverage(&contract, &messages, dir, &broad, Some(18)).is_none(),
            "a formatting reminder is bounded"
        );
        let gap = coverage(&contract, &messages, dir, &uncovered, Some(18)).unwrap();
        messages.extend([json!({"role":"assistant","tool_calls":[{"id":"known-gap","function":{"name":"done","arguments":json!({"summary":uncovered}).to_string()}}]}),json!({"role":"tool","tool_call_id":"known-gap","content":format!("GOVERNOR BLOCKED\n\n{gap}")})]);
        assert!(
            coverage(&contract, &messages, dir, &broad, Some(18))
                .unwrap()
                .contains(DECLARED_CASE_MARKER),
            "dropping the previously admitted gap does not correct it"
        );
        assert!(coverage(
            &contract,
            &messages,
            dir,
            &mapped.replace(name, "unwritten_case"),
            Some(18)
        )
        .is_some());
        assert!(coverage(&contract, &messages, dir, &mapped, Some(18)).is_none());
        for d in [
            "Consider covering empty, absent, and invalid input.",
            "Cover ascending, descending, or empty.",
            "Cover tests, guards and ordering if needed.",
        ] {
            contract.requirements[0].description = d.into();
            assert!(
                contract.requirements[0].explicit_test_cases().is_empty(),
                "{d}"
            );
        }
        contract.requirements[0].description = "Cover one, two and three.".into();
        contract.requirements[0].artifacts[0].kind = ArtifactKind::Source;
        assert!(contract.requirements[0].explicit_test_cases().is_empty());
    }

    #[test]
    fn coverage_sections_parse_common_markdown_without_report_count_claims() {
        for header in [
            "Test coverage:",
            "## Test coverage:",
            "**Test coverage:**",
            "test coverage:",
        ] {
            let summary = format!("Before: FAILED\nAfter: PASSED\n\n{header}\n- behavior-1: `a_test`, `b_test`\n* behavior-2: none (documentation is checked by the guide)\n\nNext: report.");
            let entries = coverage_entries(&summary).expect(header);
            assert_eq!(entries.len(), 2, "{header}: {entries:?}");
            assert_eq!(backticked(&entries["behavior-1"]), ["a_test", "b_test"]);
            assert!(says_none(&entries["behavior-2"]));
            assert!(
                super::super::super::report_counts::issues(&summary, &[]).is_empty(),
                "{header}"
            );
        }
        assert!(coverage_entries("No map here.").is_none());
        assert!(!says_none("nonexistent"));
    }

    #[test]
    fn test_functions_are_recognized_per_language() {
        assert_eq!(
            test_names(
                "t.rs",
                "#[test]\nfn plain() {}\n#[tokio::test]\n#[ignore]\nasync fn awaited() {}\nfn helper() {}\n#[test]\npub fn public_case() {}\n"
            ),
            ["plain", "awaited", "public_case"]
        );
        assert_eq!(
            test_names("t.py", "def helper():\n    pass\ndef test_one():\n    pass\n    async def test_two(self):\n"),
            ["test_one", "test_two"]
        );
        assert_eq!(
            test_names(
                "t.mjs",
                "test('routes the command', () => {});\n  it(\"rejects links\", () => {});\n"
            ),
            ["routes the command", "rejects links"]
        );
        assert!(test_names("notes.txt", "#[test]\nfn x() {}").is_empty());
    }
}
