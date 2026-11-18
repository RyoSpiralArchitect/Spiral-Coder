//! One review of authored tests against the requirements that link them.
//!
//! The original task can name one test artifact for several requirements. The
//! authored-test view keeps those links visible, but it is advisory: once the
//! exact checks pass, `done` is admissible even if the test file exercises only
//! a fraction of them. Before the first completion claim of a task, while the
//! Coder can still afford an edit and every declared check, show each linked
//! requirement in full beside the current test functions once, and ask for a
//! case-to-test coverage map. After that review, the first completion claim
//! whose `done.summary` lacks a valid `Test coverage:` section is returned once.
//! Neither step adds completion evidence or a new pass condition: the map is the
//! Coder's own claim, checked only for its shape and for test functions that
//! exist, and the next `done` proceeds.
use super::{ArtifactKind, Contract, Requirement};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

pub(super) const MARKER: &str = "[Authored-test closeout]";
pub(super) const COVERAGE_MARKER: &str = "[Test coverage map]";
// Whole requirement text: S28's review clipped at 160 characters and cut the
// sentence that named the four fault families to cover.
const MAX_DESCRIPTION_CHARS: usize = 1200;
const MAX_LISTED_TESTS: usize = 24;
const MAP_FORMAT: &str =
    "- <requirement id>: `test_function_name`, `another_test`\n- <requirement id>: none (reason)";

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
    linked.retain(|_, requirements| requirements.len() >= 2);
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
    if linked.is_empty() {
        return None;
    }
    // One edit, every declared check after it, and the final report.
    let checks: BTreeSet<&str> = contract
        .requirements
        .iter()
        .flat_map(|requirement| &requirement.checks)
        .map(|check| check.command.trim())
        .collect();
    let floor = checks.len() + 2;
    if coder_remaining.is_some_and(|left| left < floor) {
        return None;
    }

    let mut text = format!(
        "{MARKER} Before completion, map each requirement linked to an authored test to the tests that exercise it."
    );
    for (path, requirements) in linked {
        text.push_str(&format!(
            "\n\n{path} is linked by the original task to {} requirements:",
            requirements.len()
        ));
        for requirement in requirements {
            text.push_str(&format!(
                "\n- {}: {}",
                requirement.id,
                clip(&requirement.description)
            ));
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
    }
    text.push_str(&format!(
        "\n\nWrite the coverage map before editing: for each requirement, list every distinct case its text names (each rejection, mode, boundary or output rule) and the test function that exercises it, or `none`. Add a focused public test for each `none` case that the public command can exercise, starting with cases a requirement explicitly asks tests to cover, in one edit where possible; each starts from a passing control and changes one invariant. Then run every declared check again after the last edit, and call `done` with the final map in `done.summary` under a `Test coverage:` line, one line per requirement ID, test functions in backticks:\n{MAP_FORMAT}\nThis review is shown once per task and is not completion evidence.{}",
        coder_remaining
            .map(|left| format!(" Remaining Coder allowance: {left}."))
            .unwrap_or_default()
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
    if !shown(messages, MARKER) || shown(messages, COVERAGE_MARKER) || coder_remaining == Some(0) {
        return None;
    }
    let linked = linked(contract);
    if linked.is_empty() {
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
    let ids: BTreeSet<&str> = linked
        .values()
        .flatten()
        .map(|requirement| requirement.id.as_str())
        .collect();
    let entries = coverage_entries(summary);
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

    const S28_PROMPT: &str =
        include_str!("../../../../benchmarks/self-dogfood-v2/coupled-status/prompt.txt");
    // The test file when S28 iteration 5 first claimed completion (step 96).
    const S28_TESTS: &str = "#[test]\nfn public_cli_matches_protected_fixture_states() {}\n\n#[test]\nfn duplicate_request_id_is_rejected() {}\n\n#[test]\nfn wrong_parent_linkage_is_rejected() {}\n";

    fn blocked(id: &str, text: &str) -> [Value; 2] {
        [
            json!({"role":"assistant","tool_calls":[{"id":id,"function":{"name":"done","arguments":"{}"}}]}),
            json!({"role":"tool","tool_call_id":id,"content":format!("GOVERNOR BLOCKED\n\n{text}")}),
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
