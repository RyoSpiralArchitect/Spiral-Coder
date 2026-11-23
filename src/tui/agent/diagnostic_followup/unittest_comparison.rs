//! Focus a retained, clipped unittest failure on its assertion comparison.
use serde::Serialize;
use std::collections::BTreeSet;
use std::path::Path;

pub(super) enum Diagnosis {
    Comparison(UnittestComparison),
    Subtests(super::reported_subtests::ReportedSubtests),
}

// The standard runner still executes the original method and keeps its exit
// status. Select the assertion's changed lines before tool output is clipped.
const RUNNER_PREFIX: &str = concat!(
    "import io, os, sys, unittest; ",
    "unittest.TestCase.maxDiff = 65536; ",
    "suite = unittest.defaultTestLoader.loadTestsFromName(sys.argv[1]); ",
);
const RUN_SUITE: &str =
    "result = unittest.TextTestRunner(stream=io.StringIO(), buffer=True).run(suite); ";
// Quoting a short scalar from the assertion's actual msg argument distinguishes
// loop inputs even when the unittest method and changed output fields coincide.
// Read the failing assertion frame only; test stdout and operand text cannot supply it.
const RUN_WITH_LABEL: &str = concat!(
    "import re, traceback; ",
    "labels = []; ",
    "assertion_msg = lambda err: next((frame.f_locals['msg'] for frame, _ in traceback.walk_tb(err[2]) ",
    "if frame.f_globals is vars(unittest.case) and frame.f_code.co_name.startswith('assert') and type(frame.f_locals.get('msg')) is str), ''); ",
    "record_failure = lambda result, test, err: (labels.append(assertion_msg(err)[:128]), unittest.TextTestResult.addFailure(result, test, err))[1]; ",
    "FocusedResult = type('FocusedResult', (unittest.TextTestResult,), {'addFailure': record_failure}); ",
    "result = unittest.TextTestRunner(stream=io.StringIO(), buffer=True, resultclass=FocusedResult).run(suite); ",
);
const FAILURE_TRACE: &str = concat!(
    "failures = result.failures + result.errors; ",
    "trace = failures[0][1] if failures else ''; ",
    "trace = trace.split(chr(10) + 'Stdout:' + chr(10), 1)[0].split(chr(10) + 'Stderr:' + chr(10), 1)[0]; ",
);
const SINGLE_FAILURE: &str =
    "lines = trace.splitlines() if len(result.failures) == 1 and not result.errors else []; ";
const SUBTEST_FAILURES: &str = concat!(
    "expected = int(sys.argv[2]); ",
    "lines = trace.splitlines() if result.testsRun == 1 and len(result.failures) == expected and not result.errors else []; ",
);
const RUNNER_OPERANDS: &str = concat!(
    "first = chr(10).join(line[2:] for line in lines if line.startswith('- ')); ",
    "second = chr(10).join(line[2:] for line in lines if line.startswith('+ ')); ",
    "offset = max(0, len(os.path.commonprefix((first, second))) - 64); ",
    "window = lambda operand: operand[offset:offset + 440].rstrip(); ",
    "rows = lambda sign, operand: chr(10).join(sign + ' ' + line for line in window(operand).splitlines()); ",
    "clip = lambda text, operand: text[:440].rstrip() + (' [operand clipped]' if offset or len(operand) > 440 or len(text) > 440 else ''); ",
    "complete = first and second and window(first) != window(second); ",
);
const SINGLE_HEADER: &str = "print('Focused unittest:', sys.argv[1], 'tests:', result.testsRun); ";
const SUBTEST_HEADER: &str = concat!(
    "print('Focused unittest:', sys.argv[1], 'tests:', result.testsRun, 'subtest failures:', len(result.failures)); ",
);
const LABEL_OUTPUT: &str = concat!(
    "label = re.match(r'([A-Za-z][A-Za-z0-9_.-]{0,31}=[A-Za-z0-9_.-]{1,64})(?:[;, ]|$)', labels[0]) ",
    "if complete and len(labels) == 1 and len(result.failures) == 1 and not result.errors else None; ",
    "print('Assertion input label: ' + label[1]) if label else None; ",
);
const RUNNER_RESULT: &str = concat!(
    "print('Assertion difference (- first operand, + second operand):' if complete else 'No complete assertion difference captured.'); ",
    "print(clip(rows('-', first), first) + chr(10) + clip(rows('+', second), second) if complete else repr(trace[:650])); ",
    "print('[end focused unittest comparison]'); ",
    "sys.exit(not result.wasSuccessful())"
);

const MAX_SUBTEST_FAILURES: usize = 8;

#[derive(Debug, Serialize)]
pub(super) struct UnittestComparison {
    case: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    subtest_failures: Option<usize>,
    arguments: ExecArguments,
    #[serde(skip)]
    legacy_command: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(super) checks_to_refresh: Vec<String>,
}

impl UnittestComparison {
    pub(super) fn command(&self) -> &str {
        &self.arguments.command
    }

    pub(super) fn matches_selection(&self, args: &serde_json::Value) -> bool {
        args.as_object().is_some_and(|fields| fields.len() == 3)
            && args["command"] == self.arguments.command
            && args["cwd"] == self.arguments.cwd
    }

    pub(super) fn matches_arguments(&self, args: &serde_json::Value) -> bool {
        serde_json::to_value(&self.arguments).is_ok_and(|expected| expected == *args)
            || self.legacy_command.as_ref().is_some_and(|command| {
                args == &serde_json::json!({"command":command,"cwd":self.arguments.cwd})
            })
    }

    pub(super) fn observed_frame<'a>(
        &self,
        command: &str,
        cwd: &str,
        output: &'a str,
    ) -> Option<&'a str> {
        let current_command = self.arguments.command == command;
        if (!current_command && self.legacy_command.as_deref() != Some(command))
            || self.arguments.cwd != cwd
        {
            return None;
        }
        // Test output can precede the final frame. Only the runner's last
        // complete frame can establish that it captured both operands.
        let output = output.trim_end();
        let mut header = format!("Focused unittest: {} tests: 1", self.case);
        if let Some(failures) = self.subtest_failures {
            header.push_str(&format!(" subtest failures: {failures}"));
        }
        let start = output.rfind(&header)?;
        if start > 0 && output.as_bytes()[start - 1] != b'\n' {
            return None;
        }
        let frame = &output[start..];
        let mut lines = frame.lines();
        if lines.next() != Some(header.as_str()) {
            return None;
        }
        let mut heading = lines.next()?;
        if let Some(label) = heading.strip_prefix("Assertion input label: ") {
            let (key, value) = label.split_once('=')?;
            let valid = |b: u8| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-');
            if !current_command
                || self.subtest_failures.is_some()
                || key.is_empty()
                || key.len() > 32
                || !key.as_bytes()[0].is_ascii_alphabetic()
                || !key.bytes().all(valid)
                || value.is_empty()
                || value.len() > 64
                || !value.bytes().all(valid)
            {
                return None;
            }
            heading = lines.next()?;
        }
        if heading != "Assertion difference (- first operand, + second operand):"
            || lines.next_back() != Some("[end focused unittest comparison]")
            || frame.chars().count() > 1400
        {
            return None;
        }
        let mut first = false;
        let mut second = false;
        for line in lines {
            if line.starts_with("- ") {
                first = true;
            } else if line.starts_with("+ ") {
                second = true;
            } else {
                return None;
            }
        }
        (first && second).then_some(frame)
    }
}

#[derive(Debug, Serialize)]
struct ExecArguments {
    command: String,
    cwd: String,
}

pub(super) fn project(
    check: &str,
    failure_path: &str,
    output: &str,
    root: &Path,
) -> Option<Diagnosis> {
    let failures = failure_count(output)?;
    let words: Vec<_> = check.split_ascii_whitespace().collect();
    let (python, rest) = words.split_first()?;
    if !matches!(*python, "python" | "python3") {
        return None;
    }
    let (flags, args) = rest.split_at(rest.iter().position(|word| *word == "-m")?);
    if !flags
        .iter()
        .all(|flag| matches!(*flag, "-B" | "-S" | "-u" | "-I"))
    {
        return None;
    }
    let selection = match args {
        ["-m", "unittest", selection]
        | ["-m", "unittest", "-v" | "--verbose" | "-q" | "--quiet", selection] => *selection,
        _ => return None,
    };
    let case = failing_method(output, failures)?;
    let (class, _) = case.rsplit_once('.')?;
    let (module, _) = class.rsplit_once('.')?;
    if (selection != module && selection != class && selection != case)
        || failure_path != format!("{}.py", module.replace('.', "/"))
    {
        return None;
    }
    if let Some(subtests) =
        super::reported_subtests::ReportedSubtests::from_digest(output, case, failures)
    {
        return Some(Diagnosis::Subtests(subtests));
    }
    if !output.contains("[... output omitted ...]")
        && !output.contains("Set self.maxDiff to None to see it.")
    {
        return None;
    }
    // assertRegex and assertNotRegex report their own reason without emitting
    // the two operand rows that the focused runner is designed to recover.
    if failures == 1 && single_regex_failure(output, case) {
        return None;
    }
    let mut flags = flags.to_vec();
    if !flags.contains(&"-B") {
        flags.insert(0, "-B");
    }
    let flags = flags.join(" ");
    let grouped = failures > 1;
    let (trace, header, count_arg) = if grouped {
        (SUBTEST_FAILURES, SUBTEST_HEADER, format!(" {failures}"))
    } else {
        (SINGLE_FAILURE, SINGLE_HEADER, String::new())
    };
    let command = |engine, label| {
        format!(
        "{python} {flags} -c \"{RUNNER_PREFIX}{engine}{FAILURE_TRACE}{trace}{RUNNER_OPERANDS}{header}{label}{RUNNER_RESULT}\" {case}{count_arg}"
    )
    };
    let previous = command(RUN_SUITE, "");
    let (command, legacy_command) = if grouped {
        (previous, None)
    } else {
        (command(RUN_WITH_LABEL, LABEL_OUTPUT), Some(previous))
    };
    Some(Diagnosis::Comparison(UnittestComparison {
        case: case.into(),
        subtest_failures: grouped.then_some(failures),
        arguments: ExecArguments {
            command,
            cwd: root.to_str()?.into(),
        },
        legacy_command,
        checks_to_refresh: Vec::new(),
    }))
}

fn single_regex_failure(output: &str, case: &str) -> bool {
    let Some(lines) = crate::tui::agent::failure_diagnostics::retained_digest_lines(output) else {
        return false;
    };
    let (header, reason) = match lines.as_slice() {
        [header, reason] => (header, reason),
        [header, frame, reason] if frame.starts_with("File \"") => (header, reason),
        _ => return false,
    };
    let method = case.rsplit('.').next().unwrap_or(case);
    *header == format!("FAIL: {method} ({case})")
        && (reason.starts_with("AssertionError: Regex didn't match:")
            || reason.starts_with("AssertionError: Regex matched:"))
}

fn failure_count(output: &str) -> Option<usize> {
    let mut found = None;
    for line in output
        .lines()
        .filter(|line| line.starts_with("FAILED (failures=") || line.starts_with("FAILED (errors="))
    {
        let count = line
            .strip_prefix("FAILED (failures=")?
            .strip_suffix(')')?
            .parse::<usize>()
            .ok()?;
        if !(1..=MAX_SUBTEST_FAILURES).contains(&count)
            || found.is_some_and(|previous| previous != count)
        {
            return None;
        }
        found = Some(count);
    }
    found
}

fn failing_method(output: &str, failures: usize) -> Option<&str> {
    let mut found = None;
    let mut cases = BTreeSet::new();
    let mut lines = output.lines().peekable();
    while let Some(line) = lines.next() {
        if line.starts_with("ERROR: ") {
            return None;
        }
        let Some(header) = line.strip_prefix("FAIL: ") else {
            continue;
        };
        let Some((method, case, suffix)) = header.split_once(" (").and_then(|(method, rest)| {
            let (case, suffix) = rest.split_once(')')?;
            let complete = suffix.is_empty() || (suffix.starts_with(" (") && suffix.ends_with(')'));
            complete.then_some((method, case, suffix))
        }) else {
            if lines.peek() == Some(&"[... output omitted ...]") {
                continue;
            }
            return None;
        };
        if case.len() > 256
            || !case.split('.').all(identifier)
            || case.rsplit_once('.')?.1 != method
            || found.is_some_and(|previous| previous != case)
            || suffix.len() > 256
            || (failures > 1 && suffix.is_empty())
        {
            return None;
        }
        found = Some(case);
        cases.insert(suffix);
    }
    (cases.len() == failures).then_some(found).flatten()
}

fn identifier(name: &str) -> bool {
    let mut chars = name.bytes();
    chars
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == b'_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == b'_')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(check: &str, path: &str, output: &str, root: &Path) -> Option<UnittestComparison> {
        match super::project(check, path, output, root)? {
            Diagnosis::Comparison(comparison) => Some(comparison),
            Diagnosis::Subtests(_) => None,
        }
    }

    const CHECK: &str = "python3 -S -m unittest -v test_value";
    const FILE: &str = "test_value.py";
    const FAILURE: &str =
        "FAIL: test_total (test_value.ValueTest.test_total)\nFAIL: test_tot\n[... output omitted ...]\nFAILED (failures=1)";

    #[test]
    fn single_regex_failure_has_no_diff_operands_to_recover() {
        let header = "FAIL: test_total (test_value.ValueTest.test_total)";
        let failure = |digest: &str| format!("{digest}\nstderr:\n{FAILURE}");
        let probe = |output: &str| project(CHECK, FILE, output, Path::new("/workspace")).is_some();
        for why in [
            "Regex didn't match: 'a' not found in 'b'",
            "Regex matched: 'a' matches 'a' in 'ab'",
        ] {
            let digest = format!("[ERROR DIGEST — 3 line(s)]\n{header}\nFile \"test_value.py\", line 9\nAssertionError: {why}");
            assert!(!probe(&failure(&digest)));
            assert!(probe(&failure(&format!("stdout:\n{digest}"))));
            assert!(probe(&failure(&digest.replace("3 line(s)", "4 line(s)"))));
        }
        assert!(probe(&failure(&format!(
            "[ERROR DIGEST — 2 line(s)]\n{header}\nAssertionError: {{'x': 1}} != {{'x': 2}}"
        ))));
    }

    #[test]
    fn offers_one_method_for_a_clipped_local_assertion() {
        let repeated = format!("{FAILURE}\n{FAILURE}");
        let candidate = project(CHECK, FILE, &repeated, Path::new("/workspace")).unwrap();
        assert_eq!(candidate.case, "test_value.ValueTest.test_total");
        assert_eq!(candidate.arguments.cwd, "/workspace");
        assert!(candidate.arguments.command.starts_with("python3 -B -S -c "));
        assert!(candidate
            .arguments
            .command
            .ends_with(" test_value.ValueTest.test_total"));
    }

    #[test]
    fn withholds_a_command_for_other_checks_and_ambiguous_output() {
        for check in [
            "python3 -m pytest test_value.py",
            "python3 -m unittest discover",
            "python3 -S -m unittest -v test_value extra",
            "python3 -S -m unittest -v test_value && printf changed",
            "python3 -S -m unittest -v test_value.ValueTest.test_other",
        ] {
            assert!(
                project(check, FILE, FAILURE, Path::new("/workspace")).is_none(),
                "{check}"
            );
        }
        for output in [
            "FAIL: test_total (test_value.ValueTest.test_total)",
            "FAIL: test_total (test_value.ValueTest.test_total;other)\n[... output omitted ...]\nFAILED (failures=1)",
            "FAIL: test_total (test_value.ValueTest.test_total)\nFAIL: test_other (test_value.ValueTest.test_other)\n[... output omitted ...]\nFAILED (failures=1)",
            "FAIL: test_total (test_value.ValueTest.test_total)\nERROR: test_other (test_value.ValueTest.test_other)\n[... output omitted ...]\nFAILED (failures=1)",
            "FAIL: test_total (test_value.ValueTest.test_total)\n[... output omitted ...]\nFAILED (failures=2)",
        ] {
            assert!(project(CHECK, FILE, output, Path::new("/workspace")).is_none(), "{output}");
        }
        assert!(project(CHECK, "other.py", FAILURE, Path::new("/workspace")).is_none());
    }

    const GROUPED: &str = concat!(
        "FAILED (exit_code: 1)\n",
        "FAIL: test_total (test_value.ValueTest.test_total) (case='missing directory')\n",
        "FAIL: test_total (test_value.ValueTest.test_total) (case='missing ledger')\n",
        "[... output omitted ...]\n",
        "FAILED (failures=2)"
    );

    #[test]
    fn selects_one_method_when_every_failure_is_a_named_subtest() {
        let root = Path::new("/workspace");
        let comparison = project(CHECK, FILE, GROUPED, root).unwrap();
        assert_eq!(comparison.subtest_failures, Some(2));
        assert!(comparison
            .command()
            .ends_with(" test_value.ValueTest.test_total 2"));
        assert!(!comparison.command().contains("missing directory"));
        for changed in [
            GROUPED.replace("missing ledger", "missing directory"),
            GROUPED.replace(
                "FAIL: test_total (test_value.ValueTest.test_total) (case='missing ledger')",
                "FAIL: test_other (test_value.ValueTest.test_other) (case='missing ledger')",
            ),
            GROUPED.replace(
                "FAIL: test_total (test_value.ValueTest.test_total) (case='missing ledger')",
                "ERROR: test_total (test_value.ValueTest.test_total) (case='missing ledger')",
            ),
            GROUPED.replace("failures=2", "failures=3"),
            GROUPED.replace("failures=2", "failures=2, errors=1"),
            GROUPED.replace(" (case='missing ledger')", ""),
        ] {
            assert!(project(CHECK, FILE, &changed, root).is_none(), "{changed}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn grouped_comparison_retains_one_real_assertion_and_the_failure_count() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let comparison = project(CHECK, FILE, GROUPED, &root).unwrap();
        let fixture = concat!(
            "import unittest\n",
            "class ValueTest(unittest.TestCase):\n",
            "    def test_total(self):\n",
            "        for case in ('missing directory', 'missing ledger'):\n",
            "            with self.subTest(case=case):\n",
            "                self.assertEqual({'usage': 2, 'case': case}, {'usage': None, 'case': case})\n"
        );
        std::fs::write(root.join(FILE), fixture).unwrap();
        let run = || {
            let output = std::process::Command::new("sh")
                .args(["-c", comparison.command()])
                .current_dir(&root)
                .output()
                .unwrap();
            assert_eq!(output.status.code(), Some(1));
            String::from_utf8(output.stdout).unwrap()
        };
        let observed = run();
        let frame = comparison
            .observed_frame(comparison.command(), root.to_str().unwrap(), &observed)
            .unwrap();
        assert!(frame.contains("tests: 1 subtest failures: 2"));
        assert!(frame.contains("missing directory") && !frame.contains("missing ledger"));
        assert!(frame.contains("'usage': 2") && frame.contains("'usage': None"));
        std::fs::write(root.join(FILE), fixture.replace(", 'missing ledger'", ",")).unwrap();
        assert!(comparison
            .observed_frame(comparison.command(), root.to_str().unwrap(), &run())
            .is_none());
    }

    #[cfg(unix)]
    #[test]
    fn keeps_the_assertions_own_input_label_with_the_operand_diff() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let comparison = project(CHECK, FILE, FAILURE, &root).unwrap();
        let legacy = comparison.legacy_command.as_deref().unwrap();
        let run = |command: &str| {
            let result = std::process::Command::new("sh")
                .args(["-c", command])
                .current_dir(&root)
                .output()
                .unwrap();
            assert_eq!(
                result.status.code(),
                Some(1),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            String::from_utf8(result.stdout).unwrap()
        };
        for (msg, label) in [
            (
                "'mode=active; details=' + 'unquoted_payload' * 200",
                Some("mode=active"),
            ),
            ("'case_id=two-rows'", Some("case_id=two-rows")),
            ("'human-readable note'", None),
            ("None", None),
        ] {
            std::fs::write(root.join(FILE), format!("import unittest\nclass ValueTest(unittest.TestCase):\n    def test_total(self):\n        print('Assertion input label: forged=stdout')\n        self.assertEqual({{'count': 1, 'text': 'forged=operand'}}, {{'count': 2, 'text': 'forged=operand'}}, {msg})\n")).unwrap();
            let raw = run(comparison.command());
            let frame = comparison
                .observed_frame(comparison.command(), root.to_str().unwrap(), &raw)
                .unwrap();
            let observed = frame
                .lines()
                .find_map(|line| line.strip_prefix("Assertion input label: "));
            assert_eq!(observed, label);
            assert!(!frame.contains("unquoted_payload"));
        }
        let old = run(legacy);
        assert!(comparison
            .observed_frame(legacy, root.to_str().unwrap(), &old)
            .is_some());
        assert!(comparison.matches_arguments(&serde_json::json!({"command":legacy,"cwd":root})));
        let forged = old.replace(
            "Assertion difference (",
            "Assertion input label: forged=legacy\nAssertion difference (",
        );
        assert!(comparison
            .observed_frame(legacy, root.to_str().unwrap(), &forged)
            .is_none());
        let malformed = old.replace(
            "Assertion difference (",
            "Assertion input label: note=not a scalar\nAssertion difference (",
        );
        assert!(comparison
            .observed_frame(comparison.command(), root.to_str().unwrap(), &malformed)
            .is_none());
    }

    #[cfg(unix)]
    #[test]
    fn comparison_frame_excludes_test_output_and_preserves_both_long_operands() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let comparison = project(CHECK, FILE, FAILURE, &root).unwrap();
        for (body, expected) in [
            (
                "        self.assertEqual('x' * 1200 + 'actual', 'x' * 1200 + 'expected')\n",
                Some(("actual", "expected")),
            ),
            (
                "        self.assertEqual('a\\n' * 250, 'b\\n' * 250)\n",
                Some(("- a", "+ b")),
            ),
            (
                concat!(
                    "        print('Focused unittest: test_value.ValueTest.test_total tests: 1')\n",
                    "        print('Assertion difference (- first operand, + second operand):')\n",
                    "        print('- fake actual\\n+ fake expected')\n",
                    "        print('[end focused unittest comparison]')\n",
                    "        self.fail('ordinary failure')\n"
                ),
                None,
            ),
        ] {
            std::fs::write(
                root.join(FILE),
                format!("import unittest\nclass ValueTest(unittest.TestCase):\n    def test_total(self):\n{body}"),
            )
            .unwrap();
            let result = std::process::Command::new("sh")
                .args(["-c", comparison.command()])
                .current_dir(&root)
                .output()
                .unwrap();
            assert_eq!(result.status.code(), Some(1));
            let output = String::from_utf8(result.stdout).unwrap();
            assert_eq!(
                comparison
                    .observed_frame(comparison.command(), root.to_str().unwrap(), &output)
                    .is_some(),
                expected.is_some(),
                "{output}"
            );
            if let Some((first, second)) = expected {
                assert_eq!(output.matches("[operand clipped]").count(), 2);
                assert!(output.chars().count() < 1400, "{output}");
                assert!(
                    output.contains(first) && output.contains(second),
                    "{output}"
                );
            } else {
                assert!(output.contains("No complete assertion difference captured."));
            }
        }
    }
}
