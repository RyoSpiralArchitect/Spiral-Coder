//! Detect error text from commands that returned zero without treating passed
//! libtest case names as failures. This is output classification, not test proof.

const ERROR_MARKERS: &[&str] = &[
    "parsererror",
    "unexpected token",
    "missing expression",
    "commandnotfoundexception",
    "not recognized",
    "error:",
    "fatal:",
    "exception",
    "traceback",
    "access is denied",
    "permission denied",
    "does not have a commit checked out",
    "unable to index file",
    "could not find a part of the path",
];

pub(super) fn suspicious_success_reason(stdout: &str, stderr: &str) -> Option<String> {
    // Non-terminating PowerShell errors and shell chains can report exit zero.
    // A passing test row exempts that row only, never the rest of either stream.
    for stream in [stderr, stdout] {
        for raw in stream.lines() {
            let line = strip_sgr(raw);
            let line = line.trim();
            if passed_rust_test_row(line) {
                continue;
            }
            let lower = line.to_ascii_lowercase();
            if ERROR_MARKERS.iter().any(|marker| lower.contains(marker)) {
                // Report the line that actually triggered this decision, rather
                // than an independently selected (possibly passing) test name.
                let evidence = super::compact_one_line(line, 220);
                return Some(format!(
                    "exit_code was 0, but output contained error markers (e.g. `{evidence}`)"
                ));
            }
        }
    }
    None
}

/// Deliberately narrow: unit/integration libtest names are ASCII Rust identifier
/// paths, optionally containing raw identifiers. Doctest paths, arbitrary prose,
/// ignored/failed rows and rows with trailing diagnostics do not match.
fn passed_rust_test_row(line: &str) -> bool {
    let Some(name) = line
        .strip_prefix("test ")
        .and_then(|line| line.strip_suffix(" ... ok"))
    else {
        return false;
    };
    name.split("::").all(|part| {
        let part = part.strip_prefix("r#").unwrap_or(part);
        let mut bytes = part.bytes();
        matches!(bytes.next(), Some(b'a'..=b'z' | b'A'..=b'Z' | b'_'))
            && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    })
}

/// Strip only ANSI SGR styling. Other escape/control sequences cannot turn an
/// arbitrary line into an accepted test row.
fn strip_sgr(mut text: &str) -> String {
    let mut result = String::new();
    while let Some(start) = text.find("\u{1b}[") {
        result.push_str(&text[..start]);
        let rest = &text[start + 2..];
        let end = rest
            .bytes()
            .position(|byte| !byte.is_ascii_digit() && byte != b';');
        match end {
            Some(end) if rest.as_bytes()[end] == b'm' => text = &rest[end + 1..],
            _ => {
                result.push_str("\u{1b}[");
                text = rest;
            }
        }
    }
    result.push_str(text);
    result
}

/// Raw output has already been classified before the runtime renders its OK
/// result. Do not leave half of a recognized passing row in the stored preview:
/// its name might contain an error marker while its omitted suffix said `ok`.
pub(super) fn stdout_preview(stdout: &str, max_chars: usize) -> String {
    let trimmed = stdout.trim_end();
    let Some((boundary, _)) = trimmed.char_indices().nth(max_chars) else {
        return super::truncate_output(stdout, max_chars);
    };
    let start = trimmed[..boundary].rfind('\n').map_or(0, |index| index + 1);
    let end = trimmed[boundary..]
        .find('\n')
        .map_or(trimmed.len(), |index| boundary + index);
    if start < boundary
        && boundary < end
        && passed_rust_test_row(strip_sgr(&trimmed[start..end]).trim())
    {
        return format!(
            "{}\n[…truncated — {} lines total, first {max_chars} chars shown; partial passing test row omitted]",
            trimmed[..start].trim_end(),
            trimmed.lines().count(),
        );
    }
    super::truncate_output(stdout, max_chars)
}

/// History has an additional per-line budget. Preserve the meaning of a long
/// passing row instead of keeping an error-like name and dropping its status.
pub(super) fn compact_passed_test_row(line: &str, max_chars: usize) -> Option<String> {
    let normalized = strip_sgr(line);
    let normalized = normalized.trim();
    passed_rust_test_row(normalized).then(|| {
        if normalized.chars().count() > max_chars {
            "[passed Rust test; long name omitted from history]".to_string()
        } else {
            normalized.to_string()
        }
    })
}

#[cfg(test)]
#[path = "success_output_tests.rs"]
mod tests;
