//! A bounded artifact contract for an unambiguous literal creation request.
//!
//! This inspects the current file; a successful tool call or a line-oriented
//! test is not proof that its bytes equal the literal in the original request.
use std::io::Read;
use std::path::PathBuf;

const MAX_PATH_BYTES: usize = 4096;
const MAX_LITERAL_BYTES: usize = 64 * 1024;

#[derive(Debug, PartialEq, Eq)]
struct ExactContentTask<'a> {
    path: &'a str,
    literal: &'a str,
}

fn strip_ascii_prefix<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    let head = text.get(..prefix.len())?;
    head.eq_ignore_ascii_case(prefix)
        .then(|| &text[prefix.len()..])
}

fn supported_suffix(suffix: &str) -> bool {
    if suffix.is_empty() {
        return true;
    }
    let Some(rest) = suffix
        .strip_prefix('.')
        .or_else(|| suffix.strip_prefix('!'))
    else {
        return false;
    };
    // Do not silently ignore a later imperative that changes the bytes. Keep
    // this deliberately small instead of interpreting arbitrary English prose.
    rest.trim()
        .split(". ")
        .flat_map(|sentence| sentence.split("! "))
        .flat_map(str::lines)
        .all(|sentence| {
            let sentence = sentence.trim().trim_end_matches(['.', '!']);
            sentence.is_empty()
                || sentence.eq_ignore_ascii_case("verify it")
                || sentence.eq_ignore_ascii_case("verify it before you finish")
                || strip_ascii_prefix(sentence, "final answer must include ")
                    .is_some_and(|items| !items.trim().is_empty())
        })
}

fn parse_exact_content_task(root_user_text: &str) -> Option<ExactContentTask<'_>> {
    // Only a leading imperative is interpreted. Quoted examples, broader prose,
    // and conflicting/multiple creation requests retain the ordinary contract.
    let rest = strip_ascii_prefix(root_user_text.trim_start(), "create ")?;
    let (path, rest) = rest.trim_start().strip_prefix('`')?.split_once('`')?;
    if path.is_empty()
        || path.len() > MAX_PATH_BYTES
        || path.trim() != path
        || path.chars().any(char::is_control)
    {
        return None;
    }
    let rest = strip_ascii_prefix(rest.trim_start(), "containing exactly ")?;
    let (literal, suffix) = rest.trim_start().strip_prefix('`')?.split_once('`')?;
    if literal.len() > MAX_LITERAL_BYTES {
        return None;
    }
    let suffix = suffix.trim_start();
    if !supported_suffix(suffix) {
        return None;
    }
    let suffix_lower = suffix.to_ascii_lowercase();
    if suffix_lower.contains("containing exactly") || suffix_lower.contains("create `") {
        return None;
    }
    Some(ExactContentTask { path, literal })
}

fn correction_hint(task: &ExactContentTask<'_>, reason: &str) -> String {
    format!(
        "[Exact content] Cannot complete the literal creation request for `{}`: {}\n\
Expected exactly {} UTF-8 bytes from the backtick literal in the original request. \
Do not add a newline, trim whitespace, or interpret backslash escapes. \
Correct the artifact; `write_file` preserves the supplied content bytes. \
Then rerun the required verification before calling `done`.",
        task.path,
        reason,
        task.literal.len(),
    )
}

/// Validate supported literal creation requests at the point of completion.
/// Other prompts keep their existing acceptance contract. This never writes a
/// file, chooses the creation tool, or treats a verification label as evidence.
pub(super) fn validate_exact_content_task(
    root_user_text: &str,
    tool_root: Option<&str>,
) -> Result<(), String> {
    let Some(task) = parse_exact_content_task(root_user_text) else {
        return Ok(());
    };
    let path = crate::file_tools::resolve_safe_path(task.path, tool_root).map_err(|_| {
        correction_hint(
            &task,
            "the requested path is outside the allowed tool root or is invalid",
        )
    })?;
    let root = tool_root
        .map(PathBuf::from)
        .map_or_else(std::env::current_dir, Ok)
        .and_then(std::fs::canonicalize)
        .map_err(|_| correction_hint(&task, "the tool root cannot be inspected"))?;
    let canonical_path = std::fs::canonicalize(&path).map_err(|_| {
        correction_hint(
            &task,
            "the requested file is missing or cannot be inspected",
        )
    })?;
    // The shared resolver checks lexical traversal. Check the existing target's
    // canonical location too, so this new read cannot follow an outside symlink.
    if !canonical_path.starts_with(&root) {
        return Err(correction_hint(
            &task,
            "the requested file resolves outside the tool root",
        ));
    }
    if !std::fs::metadata(&canonical_path).is_ok_and(|metadata| metadata.is_file()) {
        return Err(correction_hint(
            &task,
            "the requested path is not a regular file",
        ));
    }
    let file = std::fs::File::open(&canonical_path)
        .map_err(|_| correction_hint(&task, "the requested file cannot be read"))?;
    if !file.metadata().is_ok_and(|metadata| metadata.is_file()) {
        return Err(correction_hint(
            &task,
            "the requested path is not a regular file",
        ));
    }
    // Reading one extra byte is sufficient to reject arbitrarily large files.
    let mut observed = Vec::new();
    file.take(task.literal.len() as u64 + 1)
        .read_to_end(&mut observed)
        .map_err(|_| correction_hint(&task, "the requested file cannot be read completely"))?;
    if observed != task.literal.as_bytes() {
        return Err(correction_hint(
            &task,
            "the current file bytes differ from the requested literal",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automatic_closeout_checks_current_bytes_despite_passing_line_test() {
        use super::super::{
            maybe_build_verified_action_closeout_text, ObservationEvidence, PlanBlock,
            VerificationLevel, WorkingMemory,
        };
        use serde_json::json;

        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("note.txt");
        let exact_prompt = "Create `note.txt` containing exactly `ship it`.";
        let command = "test -f note.txt && grep -Fx \"ship it\" note.txt";
        let plan = PlanBlock {
            goal: "Create and verify note.txt".into(),
            steps: vec!["write note.txt".into(), "verify its content".into()],
            acceptance_criteria: vec![
                "note.txt is created".into(),
                "the configured verification command passes".into(),
            ],
            risks: "extra newline".into(),
            assumptions: "the directory is writable".into(),
        };
        let messages = vec![
            json!({"role":"assistant", "tool_calls":[{
                "id":"write", "type":"function", "function":{
                    "name":"write_file",
                    "arguments":json!({"path":"note.txt","content":"ship it"}).to_string()
                }
            }]}),
            json!({"role":"tool", "tool_call_id":"write", "content":"OK: wrote 'note.txt' (1 lines, 7 bytes)"}),
            json!({"role":"assistant", "tool_calls":[{
                "id":"verify", "type":"function", "function":{
                    "name":"exec", "arguments":json!({"command":command}).to_string()
                }
            }]}),
            json!({"role":"tool", "tool_call_id":"verify", "content":"OK (exit_code: 0)\nstdout:\nship it"}),
        ];
        let mut memory = WorkingMemory::default();
        memory.remember_successful_verification(command);
        let closeout = |prompt| {
            maybe_build_verified_action_closeout_text(
                prompt,
                root.path().to_str(),
                Some(&plan),
                &messages,
                &memory,
                &ObservationEvidence::default(),
                VerificationLevel::Behavioral,
                Some(command),
                Some(1),
                Some(2),
            )
        };

        std::fs::write(&path, b"ship it").unwrap();
        assert!(closeout(exact_prompt).is_some());
        std::fs::write(&path, b"ship it\n").unwrap();
        assert!(closeout(exact_prompt).is_none());
        assert!(closeout("Create note.txt and verify it.").is_some());
    }

    #[test]
    fn exact_file_accepts_any_creation_method_but_rejects_added_newline() {
        let root = tempfile::tempdir().unwrap();
        let root_str = root.path().to_str().unwrap();
        let path = root.path().join("notes/todo.txt");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let prompt = "Create `notes/todo.txt` containing exactly `ship it`. Verify it before you finish. Final answer must include the created file path.";

        std::fs::write(&path, b"ship it\n").unwrap();
        let hint = validate_exact_content_task(prompt, Some(root_str)).unwrap_err();
        assert!(hint.contains("exactly 7 UTF-8 bytes"));
        assert!(hint.contains("write_file"));
        assert!(hint.contains("Do not add a newline"));

        std::fs::write(&path, b"ship it").unwrap();
        assert!(validate_exact_content_task(prompt, Some(root_str)).is_ok());
        // Previously valid evidence must not substitute for the current file.
        std::fs::write(&path, b"changed").unwrap();
        assert!(validate_exact_content_task(prompt, Some(root_str)).is_err());
    }

    #[test]
    fn utf8_whitespace_and_backslashes_are_literal_bytes() {
        let root = tempfile::tempdir().unwrap();
        let root_str = root.path().to_str().unwrap();
        let literal = "  猫\\n\n ";
        let prompt = format!("Create `exact.txt` containing exactly `{literal}`.");
        std::fs::write(root.path().join("exact.txt"), literal.as_bytes()).unwrap();
        assert!(validate_exact_content_task(&prompt, Some(root_str)).is_ok());
        std::fs::write(root.path().join("exact.txt"), literal.replace("\\n", "\n")).unwrap();
        assert!(validate_exact_content_task(&prompt, Some(root_str)).is_err());
    }

    #[test]
    fn empty_literal_requires_an_existing_empty_file() {
        let root = tempfile::tempdir().unwrap();
        let root_str = root.path().to_str().unwrap();
        let prompt = "Create `empty.txt` containing exactly ``.";
        assert!(validate_exact_content_task(prompt, Some(root_str)).is_err());
        std::fs::write(root.path().join("empty.txt"), b"").unwrap();
        assert!(validate_exact_content_task(prompt, Some(root_str)).is_ok());
        std::fs::write(root.path().join("empty.txt"), b"\n").unwrap();
        assert!(validate_exact_content_task(prompt, Some(root_str)).is_err());
    }

    #[test]
    fn outside_paths_and_non_files_never_satisfy_the_contract() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let root_str = root.path().to_str().unwrap();
        let outside_path = outside.path().join("exact.txt");
        std::fs::write(&outside_path, b"ship it").unwrap();
        for path in [outside_path.to_str().unwrap(), "../exact.txt", "."] {
            let prompt = format!("Create `{path}` containing exactly `ship it`.");
            assert!(validate_exact_content_task(&prompt, Some(root_str)).is_err());
        }
    }

    #[cfg(unix)]
    #[test]
    fn symlink_outside_root_is_rejected() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("exact.txt"), b"ship it").unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join("linked")).unwrap();
        let hint = validate_exact_content_task(
            "Create `linked/exact.txt` containing exactly `ship it`.",
            root.path().to_str(),
        )
        .unwrap_err();
        assert!(hint.contains("resolves outside"));
    }

    #[test]
    fn unrecognized_or_ambiguous_prompts_keep_the_existing_contract() {
        for prompt in [
            "Explain how to Create `exact.txt` containing exactly `ship it`.",
            "Create `exact.txt` containing `ship it`.",
            "Create `exact.txt` containing exactly `ship it` plus a newline.",
            "Create `exact.txt` containing exactly `ship it`. Actually append a newline.",
            "Create `exact.txt` containing exactly `ship it`. Verify it and append a newline.",
            "Create `exact.txt` containing exactly `ship it`. Verify it. Append a newline.",
            "Create `a.txt` containing exactly `a`. Create `b.txt` containing exactly `b`.",
            "Create `a.txt` containing exactly `a` and `b.txt` containing exactly `b`.",
            "Create ``a.txt`` containing exactly ``a``.",
            "Create `a.txt` containing exactly `unclosed",
        ] {
            assert_eq!(parse_exact_content_task(prompt), None, "{prompt}");
            assert!(validate_exact_content_task(prompt, Some("/nonexistent")).is_ok());
        }
        let oversized = format!(
            "Create `a.txt` containing exactly `{}`.",
            "a".repeat(MAX_LITERAL_BYTES + 1)
        );
        assert_eq!(parse_exact_content_task(&oversized), None);
        assert!(parse_exact_content_task("CREATE `a.txt` CONTAINING EXACTLY `A`!").is_some());
    }
}
