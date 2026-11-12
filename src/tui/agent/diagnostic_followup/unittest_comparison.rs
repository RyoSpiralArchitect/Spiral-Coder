//! Focus a retained, clipped unittest failure on its assertion comparison.
use serde::Serialize;
use std::path::Path;

// The standard runner still executes the original method and keeps its exit
// status. Select the assertion's changed lines before tool output is clipped.
const RUNNER: &str = concat!(
    "import io, sys, unittest; ",
    "unittest.TestCase.maxDiff = 8192; ",
    "suite = unittest.defaultTestLoader.loadTestsFromName(sys.argv[1]); ",
    "result = unittest.TextTestRunner(stream=io.StringIO()).run(suite); ",
    "failures = result.failures + result.errors; ",
    "trace = failures[0][1] if failures else ''; ",
    "diff = [line for line in trace.splitlines() if line.startswith(('- ', '+ '))]; ",
    "print('Focused unittest:', sys.argv[1], 'tests:', result.testsRun); ",
    "print('Assertion difference (- first operand, + second operand):' if diff else 'No assertion diff lines.'); ",
    "preview = chr(10).join(diff) or trace; ",
    "print(preview[:1000] + (' [comparison clipped]' if len(preview) > 1000 else '')); ",
    "sys.exit(not result.wasSuccessful())"
);

#[derive(Debug, Serialize)]
pub(super) struct UnittestComparison {
    case: String,
    arguments: ExecArguments,
}

impl UnittestComparison {
    pub(super) fn observed(&self, command: &str, cwd: &str, output: &str) -> bool {
        self.arguments.command == command
            && self.arguments.cwd == cwd
            && output
                .lines()
                .any(|line| line == format!("Focused unittest: {} tests: 1", self.case))
            && output
                .lines()
                .any(|line| line == "Assertion difference (- first operand, + second operand):")
            && output.lines().any(|line| line.starts_with("- "))
            && output.lines().any(|line| line.starts_with("+ "))
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
) -> Option<UnittestComparison> {
    if !output.contains("[... output omitted ...]")
        && !output.contains("Set self.maxDiff to None to see it.")
    {
        return None;
    }
    if !output.lines().any(|line| line == "FAILED (failures=1)") {
        return None;
    }
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
    let case = single_failure_case(output)?;
    let (class, _) = case.rsplit_once('.')?;
    let (module, _) = class.rsplit_once('.')?;
    if (selection != module && selection != class && selection != case)
        || failure_path != format!("{}.py", module.replace('.', "/"))
    {
        return None;
    }
    let mut flags = flags.to_vec();
    if !flags.contains(&"-B") {
        flags.insert(0, "-B");
    }
    let flags = flags.join(" ");
    Some(UnittestComparison {
        case: case.into(),
        arguments: ExecArguments {
            command: format!("{python} {flags} -c \"{RUNNER}\" {case}"),
            cwd: root.to_str()?.into(),
        },
    })
}

fn single_failure_case(output: &str) -> Option<&str> {
    let mut found = None;
    let mut lines = output.lines().peekable();
    while let Some(line) = lines.next() {
        if line.starts_with("ERROR: ") {
            return None;
        }
        let Some(header) = line.strip_prefix("FAIL: ") else {
            continue;
        };
        let Some((method, case)) = header
            .split_once(" (")
            .and_then(|(method, case)| Some((method, case.strip_suffix(')')?)))
        else {
            if lines.peek() == Some(&"[... output omitted ...]") {
                continue;
            }
            return None;
        };
        if case.len() > 256
            || !case.split('.').all(identifier)
            || case.rsplit_once('.')?.1 != method
            || found.is_some_and(|previous| previous != case)
        {
            return None;
        }
        found = Some(case);
    }
    found
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

    const CHECK: &str = "python3 -S -m unittest -v test_value";
    const FILE: &str = "test_value.py";
    const FAILURE: &str =
        "FAIL: test_total (test_value.ValueTest.test_total)\nFAIL: test_tot\n[... output omitted ...]\nFAILED (failures=1)";

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
}
