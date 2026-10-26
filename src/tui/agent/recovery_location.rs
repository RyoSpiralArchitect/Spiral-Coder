//! Bounded diagnostic locations are read hints, never proof or edit authority.
use super::*;

pub(super) fn argument_path(tc: &ToolCallData) -> Option<String> {
    serde_json::from_str::<Value>(&tc.arguments).ok()?["path"]
        .as_str()
        .map(str::to_string)
}

pub(super) fn relative_path(path: &str, root: Option<&str>) -> Option<String> {
    if path.is_empty() || path.len() > 1024 || path.contains(['\n', '\r', '\0', '…']) {
        return None;
    }
    let path = Path::new(path);
    let path = if path.is_absolute() {
        path.strip_prefix(Path::new(root?)).ok()?
    } else {
        path
    };
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => parts.push(part.to_str()?),
            Component::CurDir => {}
            _ => return None,
        }
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

pub(super) fn localize(
    tc: &ToolCallData,
    output: &str,
    root: Option<&str>,
    check: Option<&str>,
) -> Option<FailureFocus> {
    let check = check.filter(|check| !check.trim().is_empty() && check.len() <= 4096)?;
    let path = relative_path(&argument_path(tc)?, root)?;
    let body = automatic_failure_body(output)?;
    let diagnostic = localized_diagnostic(&path, &body, root)?;
    Some(FailureFocus {
        path,
        diagnostic,
        check: check.into(),
        observation: Observation::AwaitingRead,
        location: LocationEvidence::Current,
    })
}

pub(super) fn automatic_failure_body(output: &str) -> Option<String> {
    let mut lines = output.lines();
    lines.find(|line| line.starts_with("[auto-test]"))?;
    // Diff previews can contain marker text. Only lines after the same runtime
    // header recognized by auto_test_outcome may supply diagnostic locations.
    Some(lines.collect::<Vec<_>>().join("\n"))
}

pub(super) fn localized_diagnostic(path: &str, body: &str, root: Option<&str>) -> Option<String> {
    let explicit = body.lines().find(|line| {
        let low = line.to_ascii_lowercase();
        (low.contains("error")
            || low.contains("panic")
            || line.trim_start().starts_with("--> ")
            || line.trim_start().starts_with("File \""))
            && reports_path(line, path, root)
    });
    let diagnostic = explicit.or_else(|| {
        body.lines()
            .find(|line| json_error_matches(path, line, root))
    })?;
    Some(super::super::failure_diagnostics::recovery_summary(
        body,
        Some(diagnostic),
    ))
}

pub(super) fn fresh_diagnostic(body: &str) -> String {
    super::super::failure_diagnostics::recovery_summary(body, None)
}

fn reports_path(line: &str, path: &str, root: Option<&str>) -> bool {
    // Require token boundaries; `other/src.rs` must not match `src.rs`.
    let absolute = root.map(|root| Path::new(root).join(path).to_string_lossy().into_owned());
    let matched = [Some(path), absolute.as_deref()]
        .into_iter()
        .flatten()
        .any(|candidate| {
            line.match_indices(candidate).any(|(start, _)| {
                let before = line[..start].chars().next_back();
                let after = line[start + candidate.len()..].chars().next();
                before.is_none_or(|c| c.is_whitespace() || matches!(c, '=' | '\'' | '"' | '('))
                    && after.is_none_or(|c| {
                        c.is_whitespace() || matches!(c, ':' | ';' | ',' | '\'' | '"' | ')')
                    })
            })
        });
    matched
}

fn json_error_matches(path: &str, diagnostic: &str, root: Option<&str>) -> bool {
    if Path::new(path).extension().and_then(|ext| ext.to_str()) != Some("json") {
        return false;
    }
    let low = diagnostic.to_ascii_lowercase();
    if !low.contains("parse") && !low.contains("json") {
        return false;
    }
    let Some((_, location)) = low.split_once(" at line ") else {
        return false;
    };
    let Some((line, column)) = location.split_once(" column ") else {
        return false;
    };
    let (Ok(line), Ok(column)) = (
        line.parse::<usize>(),
        column
            .split(|c: char| !c.is_ascii_digit())
            .next()
            .unwrap_or("")
            .parse::<usize>(),
    ) else {
        return false;
    };
    // Lexical containment alone is insufficient: never follow a symlink outside
    // the canonical workspace when confirming the creation-time JSON location.
    let Some(root) = root.and_then(|root| std::fs::canonicalize(root).ok()) else {
        return false;
    };
    let Ok(target) = std::fs::canonicalize(root.join(path)) else {
        return false;
    };
    if !target.starts_with(&root) {
        return false;
    }
    let Ok(file) = std::fs::File::open(target) else {
        return false;
    };
    let mut bytes = Vec::new();
    if file
        .take((MAX_JSON_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .is_err()
        || bytes.len() > MAX_JSON_BYTES
    {
        return false;
    }
    serde_json::from_slice::<Value>(&bytes)
        .err()
        .is_some_and(|error| error.line() == line && error.column() == column)
}
