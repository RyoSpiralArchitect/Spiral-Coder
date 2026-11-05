//! Same-file lexical leads. Never infer name resolution or authorize an import.
use std::path::Path;

const MAX_SOURCE_BYTES: usize = 512 * 1024;
const MAX_NAME_BYTES: usize = 64;
const MAX_LOCATIONS: usize = 4;

pub(super) fn note(file: &Path, pattern: &str, source: &str) -> Option<String> {
    if file.extension()?.to_str()? != "rs" || source.len() > MAX_SOURCE_BYTES {
        return None;
    }
    if !pattern.contains('-') {
        return function_note(pattern, source);
    }
    let identifier = rust_spelling(pattern)?;
    let lines: Vec<_> = source
        .lines()
        .enumerate()
        .filter(|(_, line)| {
            // Treat a non-ASCII neighbor conservatively as part of the word.
            line.split(|c: char| c.is_ascii() && !c.is_ascii_alphanumeric() && c != '_')
                .any(|word| word == identifier)
        })
        .take(MAX_LOCATIONS)
        .map(|(i, _)| (i + 1).to_string())
        .collect();
    if lines.is_empty() {
        return None;
    }
    Some(format!(
        "[navigation] Related Rust spelling `{identifier}` appears in this file on lines {}. For CLI changes, inspect a working sibling's argument definition and follow its dispatch to the handler. Try the requested invocation with real arguments and check its output.\n",
        lines.join(", ")
    ))
}

fn rust_spelling(pattern: &str) -> Option<String> {
    if pattern.len() > MAX_NAME_BYTES || !pattern.contains('-') {
        return None;
    }
    let mut identifier = String::with_capacity(pattern.len());
    for part in pattern.split('-') {
        let first = *part.as_bytes().first()?;
        if !first.is_ascii_lowercase()
            || !part
                .bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        {
            return None;
        }
        identifier.push(char::from(first.to_ascii_uppercase()));
        identifier.push_str(&part[1..]);
    }
    Some(identifier)
}

/// Advisory line locations only: comments, macros and homonyms can resemble code.
fn function_note(name: &str, source: &str) -> Option<String> {
    if name.len() > MAX_NAME_BYTES
        || !name.as_bytes().first()?.is_ascii_lowercase()
        || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        return None;
    }
    let mut declarations = Vec::new();
    let mut calls = Vec::new();
    let mut seen = false;
    let word = |c: char| c == '_' || c.is_alphanumeric() || !c.is_ascii();
    for (idx, line) in source.lines().enumerate() {
        for (start, _) in line.match_indices(name) {
            let before = &line[..start];
            let after = &line[start + name.len()..];
            if before.ends_with(word) || after.starts_with(word) {
                continue;
            }
            seen = true;
            let is_decl = before.split(|c| !word(c)).rfind(|s| !s.is_empty()) == Some("fn")
                && before.trim_end().ends_with("fn")
                && before.ends_with(char::is_whitespace);
            let group = if is_decl {
                &mut declarations
            } else if after.trim_start().starts_with('(') {
                &mut calls
            } else {
                continue;
            };
            if group.len() < MAX_LOCATIONS && group.last() != Some(&(idx + 1)) {
                group.push(idx + 1);
            }
        }
        if declarations.len() == MAX_LOCATIONS && calls.len() == MAX_LOCATIONS {
            break;
        }
    }
    if declarations.is_empty() && calls.is_empty() {
        return (!seen).then(|| format!("[navigation] No exact identifier spelling `{name}` was observed in this file. Inspect the existing caller for inline behavior or locate the definition elsewhere before importing it.\n"));
    }
    Some(format!("[navigation] Lexical leads for `{name}` in this file: fn-name lines {declarations:?}; call-like lines {calls:?}. Read the locations to verify the definition and caller; these are not resolved references.\n"))
}
