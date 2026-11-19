//! Cite the setup and generator branch of a localized Rust integration test.
//! This is source navigation. The helper and generator are never executed here.
use crate::file_tools::read_range::bounded_workspace_text;
use serde::Serialize;
use std::path::Path;

const MAX_FILE_BYTES: u64 = 64 * 1024;
const MAX_CASE_LINES: usize = 36;
const MAX_HELPER_LINES: usize = 24;
const MAX_BRANCH_LINES: usize = 16;
const MAX_LINE_CHARS: usize = 200;

pub(crate) const GUIDANCE: &str = "\nThe authored_test_failure.fixture_context quotes the failing case's setup, its local helper and a branch of the called fixture generator. Trace the literal case argument through the helper. Compare inherited fields and files from the generator with what the test changes before invocation. If a pointer, identity or count disagrees, cite both values and the exact test-only correction in the primary reason to Coder. Ask for one coherent successful input and one clone changing only the intended condition before changing a validator. Once the native read is complete, put the correction and the exact test path in the primary reason so Coder can use the retained read in its structured evidence: target_files, target_symbols, evidence, open_questions and next_probe. If a fact is still missing, ask for its bounded native observation. Use the required diagnostic read first. The quoted source is advisory and supplies no execution receipt or edit authority.\n";

#[derive(Debug, Serialize)]
pub(super) struct FixtureContext {
    case: Excerpt,
    helper: Excerpt,
    fixture_argument: String,
    generator: Excerpt,
}

#[derive(Debug, Serialize)]
struct Excerpt {
    path: String,
    start_line: usize,
    end_line: usize,
    source: String,
}

impl Excerpt {
    fn new(path: &str, lines: &[&str], start: usize, end: usize) -> Self {
        let source = lines[start..end]
            .iter()
            .map(|line| {
                let mut chars = line.chars();
                let mut shown: String = chars.by_ref().take(MAX_LINE_CHARS).collect();
                if chars.next().is_some() {
                    shown.push('…');
                }
                shown
            })
            .collect::<Vec<_>>()
            .join("\n");
        Self {
            path: path.into(),
            start_line: start + 1,
            end_line: end,
            source,
        }
    }
}

fn function(line: &str) -> Option<(&str, &str)> {
    line.strip_prefix("fn ")?.split_once('(')
}

fn identifier(value: &str) -> bool {
    let mut chars = value.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn literal_helper_call(line: &str) -> Option<(&str, &str)> {
    let (_, expression) = line.trim().strip_prefix("let ")?.split_once(" = ")?;
    let (name, args) = expression.split_once('(')?;
    let (arg, suffix) = args.split_once(')')?;
    let value = arg.strip_prefix('"')?.strip_suffix('"')?;
    (identifier(name)
        && matches!(suffix, ";" | "?;" | ".unwrap();")
        && !value.is_empty()
        && value.len() <= 64
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-')))
    .then_some((name, value))
}

fn script_path(path: &str) -> bool {
    path.len() <= 200
        && path.ends_with(".py")
        && path.split('/').all(|part| {
            !part.is_empty()
                && !part.starts_with('.')
                && part
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
        })
}

fn helper<'a>(lines: &[&'a str], name: &str) -> Option<(usize, usize, &'a str)> {
    let mut matches = lines.iter().enumerate().filter_map(|(index, line)| {
        let (function_name, parameters) = function(line)?;
        (function_name == name).then_some((index, parameters))
    });
    let (start, parameters) = matches.next()?;
    if matches.next().is_some() {
        return None;
    }
    let param = parameters.split_once(':')?.0.trim();
    if !identifier(param) {
        return None;
    }
    let end = lines[start + 1..]
        .iter()
        .take(MAX_HELPER_LINES - 1)
        .position(|line| *line == "}")?
        + start
        + 2;
    let body = &lines[start + 1..end];
    if !body.iter().any(|line| {
        line.contains("Command::new(\"python3\")") || line.contains("Command::new(\"python\")")
    }) || !body
        .iter()
        .any(|line| line.trim_start().starts_with(&format!(".arg({param})")))
    {
        return None;
    }
    let mut paths = body
        .iter()
        .filter(|line| line.contains(".arg(") || line.contains(".args("))
        .flat_map(|line| line.split('"').skip(1).step_by(2))
        .filter(|path| script_path(path));
    let path = paths.next()?;
    paths.next().is_none().then_some((start, end, path))
}

pub(super) fn project(
    workspace: &str,
    test_path: &str,
    panic_line: usize,
    case_name: &str,
) -> Option<FixtureContext> {
    let workspace = Path::new(workspace).canonicalize().ok()?;
    let test = bounded_workspace_text(&workspace, test_path, MAX_FILE_BYTES)?;
    let lines: Vec<_> = test.lines().collect();
    let panic = panic_line.checked_sub(1)?;
    let start = lines.get(..=panic)?.iter().rposition(|line| {
        // A top-level closing brace delimits the previous test or helper.
        *line == "}" || function(line).is_some()
    })?;
    if function(lines[start])?.0 != case_name || panic - start >= MAX_CASE_LINES {
        return None;
    }
    let mut helpers = lines[start + 1..panic]
        .iter()
        .filter_map(|line| literal_helper_call(line))
        .filter_map(|(name, arg)| Some((helper(&lines, name)?, arg)));
    let ((helper_start, helper_end, path), state) = helpers.next()?;
    if helpers.next().is_some() {
        return None;
    }
    let producer = bounded_workspace_text(&workspace, path, MAX_FILE_BYTES)?;
    let producer_lines: Vec<_> = producer.lines().collect();
    let single_quoted = format!("'{state}'");
    let double_quoted = format!("\"{state}\"");
    let branch = producer_lines.iter().position(|line| {
        let line = line.trim_start();
        (line.starts_with("if ") || line.starts_with("elif "))
            && (line.contains(&single_quoted) || line.contains(&double_quoted))
    })?;
    Some(FixtureContext {
        case: Excerpt::new(test_path, &lines, start, panic + 1),
        helper: Excerpt::new(test_path, &lines, helper_start, helper_end),
        fixture_argument: state.into(),
        generator: Excerpt::new(
            path,
            &producer_lines,
            branch,
            (branch + MAX_BRANCH_LINES).min(producer_lines.len()),
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::super::super::RecoveryFocus;
    use super::*;
    use crate::tui::agent::{
        exec_verification::ExecVerificationContext, task_obligations, ToolCallData,
    };
    use serde_json::{json, Value};
    use std::fs;

    const TEST_PATH: &str = "tests/import.rs";
    const GENERATOR: &str = "scripts/receipt.py";
    const CHECK: &str = "cargo test --locked --test import -q";
    const TEST: &str = r#"fn example(state: &str) -> PathBuf {
    Command::new("python3")
        .args(["-S", "scripts/receipt.py"])
        .arg(state)
        .status().unwrap();
    root
}

#[test]
fn reads_empty_import() {
    let root = example("retry");
    manifest["segments"] = json!([]);
    write(root.join("segment-01/records.json"), "[]");
    assert!(open(root).is_ok());
}

fn unrelated_test() {
    unreachable!("ignore this other case");
}
"#;
    const PRODUCER: &str = r#"def prepare(state):
    if state == 'retry':
        directory = 'segment-02'
        manifest['pending'] = directory
        write(directory + '/records.json')
"#;

    fn workspace() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("tests")).unwrap();
        fs::create_dir(dir.path().join("scripts")).unwrap();
        // Source is larger than the complete authored-test readback limit.
        let source = format!("{}{TEST}", "// leading source\n".repeat(600));
        fs::write(dir.path().join(TEST_PATH), source).unwrap();
        fs::write(dir.path().join(GENERATOR), PRODUCER).unwrap();
        dir
    }

    #[test]
    fn native_panic_brings_current_case_and_generator_to_both_roles() {
        let dir = workspace();
        let root = dir.path().canonicalize().unwrap();
        let workspace = Some(root.to_str().unwrap());
        let task = format!(
            "Repair the import.\n<task_requirements>\n{}\n</task_requirements>",
            json!({"version":1,"requirements":[{"id":"import","description":"Verify valid and invalid inputs.",
                "artifacts":[{"path":TEST_PATH,"kind":"test"}],"checks":[{"command":CHECK}]}]})
        );
        let failed = format!(
            "FAILED (exit_code: 101)\ncwd: {}\nthread 'reads_empty_import' (123) panicked at {TEST_PATH}:614:5:\ninvalid directory",
            root.display()
        );
        let call = ToolCallData {
            id: "check".into(),
            name: "exec".into(),
            arguments: json!({"command":CHECK,"cwd":root}).to_string(),
            thought_signature: None,
        };
        let mut messages = vec![json!({"role":"user","content":task})];
        let mut focus = RecoveryFocus::default();
        let context = ExecVerificationContext::from_messages(Some(CHECK), &messages);
        focus.record_result(&call, &failed, workspace, Some(CHECK), &context);
        messages.extend([
            json!({"role":"assistant","tool_calls":[{"id":call.id,"function":{
                "name":call.name,"arguments":call.arguments}}]}),
            focus.tool_message(&call, failed),
        ]);
        let view = serde_json::to_value(task_obligations::project_at(
            &messages,
            Some(CHECK),
            workspace,
        ))
        .unwrap();
        assert_eq!(view["required_diagnostic_read"]["path"], TEST_PATH);
        let context = &view["authored_test_failure"]["fixture_context"];
        assert_eq!(context["fixture_argument"], "retry");
        assert_eq!(context["case"]["start_line"], 610);
        assert_eq!(context["case"]["end_line"], 614);
        assert_eq!(context["generator"]["path"], GENERATOR);
        assert_eq!(context["generator"]["start_line"], 2);
        assert_eq!(
            serde_json::to_value(task_obligations::project_advisory(
                &messages,
                Some(CHECK),
                workspace
            ))
            .unwrap()["authored_test_failure"]["fixture_context"],
            Value::Null
        );

        let coder = task_obligations::prompt(&messages, Some(CHECK), workspace).unwrap();
        let observer = crate::tui::events::observer_assist::build_tui_next_action_prompt(
            &json!({"task_obligations":view}),
            "en",
            "verification_failure",
        );
        for prompt in [coder, observer] {
            assert!(prompt.contains("segment-01/records.json"));
            assert!(prompt.contains("directory = 'segment-02'"));
            assert!(prompt.contains("cite both values"));
            assert!(!prompt.contains("ignore this other case"));
        }
        for (id, start, expected) in [
            ("clipped_prefix", None, false),
            ("targeted", Some(610), true),
            ("again", Some(610), false),
        ] {
            let args = start.map_or_else(
                || json!({"path":TEST_PATH}),
                |start| json!({"path":TEST_PATH,"start_line":start,"end_line":616}),
            );
            let read = ToolCallData {
                id: id.into(),
                name: "read_file".into(),
                arguments: args.to_string(),
                thought_signature: None,
            };
            let (output, failed) = crate::file_tools::read_range::execute_args(&args, workspace);
            assert!(!failed);
            let context = ExecVerificationContext::from_messages(Some(CHECK), &messages);
            focus.record_result(&read, &output, workspace, Some(CHECK), &context);
            messages.extend([
                json!({"role":"assistant","tool_calls":[{"id":read.id,"function":{
                    "name":read.name,"arguments":read.arguments}}]}),
                focus.tool_message(&read, output),
            ]);
            let handoff = focus.fixture_read_handoff(&messages, workspace.unwrap(), Some(CHECK));
            assert_eq!(handoff.is_some(), expected);
            if let Some(handoff) = handoff {
                let handoff = serde_json::to_value(handoff).unwrap();
                assert_eq!(handoff["diagnostic_read_call_id"], id);
                assert_eq!(handoff["test_name"], "reads_empty_import");
            }
        }
    }

    #[test]
    fn stale_names_and_files_outside_the_workspace_supply_no_fixture_context() {
        let dir = workspace();
        let root = dir.path().to_str().unwrap();
        assert!(project(root, TEST_PATH, 614, "unrelated_test").is_none());
        #[cfg(unix)]
        {
            let outside = tempfile::NamedTempFile::new().unwrap();
            fs::write(outside.path(), PRODUCER).unwrap();
            fs::remove_file(dir.path().join(GENERATOR)).unwrap();
            std::os::unix::fs::symlink(outside.path(), dir.path().join(GENERATOR)).unwrap();
            assert!(project(root, TEST_PATH, 614, "reads_empty_import").is_none());
        }
    }
}
