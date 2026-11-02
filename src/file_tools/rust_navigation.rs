//! Same-file lexical leads for tracing a hyphenated name into its Rust source.
use std::path::Path;

const MAX_SOURCE_BYTES: usize = 512 * 1024;
const MAX_NAME_BYTES: usize = 64;
const MAX_LOCATIONS: usize = 4;

pub(super) fn note(file: &Path, pattern: &str, source: &str) -> Option<String> {
    if file.extension()?.to_str()? != "rs" || source.len() > MAX_SOURCE_BYTES {
        return None;
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
