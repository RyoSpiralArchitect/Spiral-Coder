//! One review of authored tests against the requirements that link them.
//!
//! The original task can name one test artifact for several requirements. The
//! authored-test view keeps those links visible, but it is advisory: once the
//! exact checks pass, `done` is admissible even if the test file exercises only
//! a fraction of them. Before the first completion claim of a task, while the
//! Coder can still afford an edit and every declared check, show the linked
//! requirements beside the current test functions once. The review adds no
//! completion evidence and no new pass condition; a second `done` proceeds.
use super::{ArtifactKind, Contract, Requirement};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

pub(super) const MARKER: &str = "[Authored-test closeout]";
const MAX_DESCRIPTION_CHARS: usize = 160;
const MAX_LISTED_TESTS: usize = 24;

/// `messages` are the current task's messages.
pub(super) fn review(
    contract: &Contract,
    messages: &[Value],
    tool_root: Option<&str>,
    coder_remaining: Option<usize>,
) -> Option<String> {
    if messages.iter().any(|message| {
        message["role"] == "tool"
            && message["content"]
                .as_str()
                .is_some_and(|content| content.contains(MARKER))
    }) {
        return None;
    }
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
        "{MARKER} Before completion, compare each requirement linked to an authored test with the tests that exercise it."
    );
    for (path, requirements) in linked {
        let names = tool_root
            .and_then(|root| std::fs::read_to_string(Path::new(root).join(path)).ok())
            .map(|source| test_names(path, &source));
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
        match names {
            Some(names) if !names.is_empty() => {
                let shown = names
                    .iter()
                    .take(MAX_LISTED_TESTS)
                    .cloned()
                    .collect::<Vec<_>>();
                text.push_str(&format!(
                    "\nCurrent test functions ({}): {}{}",
                    names.len(),
                    shown.join(", "),
                    if names.len() > shown.len() {
                        ", …"
                    } else {
                        ""
                    }
                ));
            }
            Some(_) => text.push_str("\nCurrent test functions: none recognized."),
            None => text.push_str("\nCurrent test functions: the file could not be read."),
        }
    }
    text.push_str(&format!(
        "\n\nFor any linked requirement case that no test exercises, add a focused public test that starts from a passing control and changes one invariant, then run every declared check again after the last edit. If each requirement is already exercised, call `done` again unchanged. This review is shown once per task and is not completion evidence.{}",
        coder_remaining
            .map(|left| format!(" Remaining Coder allowance: {left}."))
            .unwrap_or_default()
    ));
    Some(text)
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
    use super::*;
    use serde_json::json;

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
