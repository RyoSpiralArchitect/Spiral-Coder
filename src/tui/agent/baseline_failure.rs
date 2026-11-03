//! Locate a failed explicit check before any edit. Diagnostic paths guide a
//! read; they never authorize an edit or provide successful execution evidence.
use super::*;

#[cfg(test)]
pub(super) fn localize(
    command: &str,
    output: &str,
    root: Option<&str>,
    context: &ExecVerificationContext<'_>,
) -> Option<FailureFocus> {
    if !is_check_failure(command, output, context) {
        return None;
    }
    let (path, diagnostic) = diagnostic(command, output, root).ok()??;
    Some(FailureFocus {
        path,
        diagnostic,
        check: command.trim().into(),
        observation: Observation::AwaitingRead,
        location: LocationEvidence::Current,
    })
}

pub(super) fn is_check_failure(
    command: &str,
    output: &str,
    context: &ExecVerificationContext<'_>,
) -> bool {
    !command.trim().is_empty()
        && command.len() <= 4096
        && context.classify(command).kind == ExecKind::Verify
        && super::super::exec_classification::preserves_working_directory(command)
        && super::super::parse_exit_code_from_tool_text(output).is_some_and(|code| code != 0)
}

/// None means no supported location was reported. Err means reported locations
/// were ambiguous or unsafe; neither the edited nor previous file may override it.
pub(super) fn diagnostic(
    command: &str,
    output: &str,
    root: Option<&str>,
) -> Result<Option<(String, String)>, ()> {
    let root_path = Path::new(root.ok_or(())?);
    let root = std::fs::canonicalize(root_path).map_err(|_| ())?;
    if cargo_fmt_check(command) {
        if let Some(location) = rustfmt_diff(output, root_path, &root)? {
            return Ok(Some(location));
        }
    }
    // Multiple different reported files are ambiguous. Do not guess that the
    // last edited/read source or a path mentioned in ordinary output is causal.
    let mut candidates = BTreeMap::new();
    let mut python_frame = None;
    for line in output.lines().map(str::trim) {
        if let Some(path) = rust_location(line) {
            candidates.insert(workspace_file(path, root_path, &root).ok_or(())?, line);
        }
        if let Some(path) = python_location(line) {
            // Tracebacks list callers first; only the final frame is the
            // immediate failure, including when that frame is outside root.
            python_frame = Some((path, line));
        }
    }
    if let Some((path, line)) = python_frame {
        candidates.insert(workspace_file(path, root_path, &root).ok_or(())?, line);
    }
    if candidates.is_empty() {
        return Ok(None);
    }
    if candidates.len() != 1 {
        return Err(());
    }
    let (path, diagnostic) = candidates.into_iter().next().expect("one location");
    Ok(Some((
        path,
        super::super::failure_diagnostics::recovery_summary(output, Some(diagnostic)),
    )))
}

pub(super) fn cargo_fmt_check(command: &str) -> bool {
    matches!(
        command
            .split_ascii_whitespace()
            .collect::<Vec<_>>()
            .as_slice(),
        ["cargo", "fmt", "--check"]
            | ["cargo", "fmt", "--", "--check"]
            | ["cargo", "fmt", "--all", "--check"]
            | ["cargo", "fmt", "--all", "--", "--check"]
    )
}

// Every rustfmt hunk describes a real formatting difference. Reading the first
// reported workspace file supplies a diagnosis even when other files also differ.
fn rustfmt_diff(
    output: &str,
    root_path: &Path,
    root: &Path,
) -> Result<Option<(String, String)>, ()> {
    let mut first_changed = None;
    let mut section = None;
    let mut saw_header = false;
    for line in output.lines() {
        if let Some(header) = line.strip_prefix("Diff in ") {
            let (path, row) = header
                .strip_suffix(':')
                .and_then(|h| h.rsplit_once(':'))
                .ok_or(())?;
            if !positive_number(row) || !path.ends_with(".rs") {
                return Err(());
            }
            let path = workspace_file(path, root_path, root).ok_or(())?;
            section = Some((path, line));
            saw_header = true;
        } else if first_changed.is_none() && line.starts_with(['+', '-']) {
            first_changed = section.take();
        }
    }
    match first_changed {
        Some((path, line)) => Ok(Some((
            path,
            super::super::failure_diagnostics::recovery_summary(output, Some(line)),
        ))),
        None if saw_header => Err(()),
        None => Ok(None),
    }
}

fn rust_location(line: &str) -> Option<&str> {
    let location = if let Some(location) = line.strip_prefix("--> ") {
        location
    } else {
        line.strip_prefix("thread '")?
            .split_once(" panicked at ")?
            .1
    };
    let (path_line, column) = location.trim_end_matches(':').rsplit_once(':')?;
    let (path, line) = path_line.rsplit_once(':')?;
    (positive_number(line) && positive_number(column)).then_some(path)
}

fn python_location(line: &str) -> Option<&str> {
    let (path, rest) = line.strip_prefix("File \"")?.split_once("\", line ")?;
    positive_number(rest.split(',').next()?).then_some(path)
}

fn positive_number(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|c| c.is_ascii_digit())
        && value.parse::<usize>().is_ok_and(|n| n > 0)
}

fn workspace_file(path: &str, root_path: &Path, root: &Path) -> Option<String> {
    // Check both lexical containment and the resolved target, including symlinks.
    // No source file needs to be opened to establish this bounded location hint.
    let relative =
        relative_path(path, root_path.to_str()).or_else(|| relative_path(path, root.to_str()))?;
    let target = std::fs::canonicalize(root.join(&relative)).ok()?;
    (target.starts_with(root) && target.is_file()).then_some(relative)
}

#[cfg(test)]
#[path = "baseline_failure_tests.rs"]
mod tests;
