//! Same-file lexical leads. Never infer name resolution or authorize an import.
use std::path::Path;

const MAX_SOURCE_BYTES: usize = 512 * 1024;
const MAX_NAME_BYTES: usize = 64;
const MAX_LOCATIONS: usize = 4;

pub(super) fn note(
    file: &Path,
    pattern: &str,
    source: &str,
    literal_hits: usize,
) -> Option<String> {
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
        if literal_hits != 0 {
            return None;
        }
        let (first, _) = pattern.split_once('-')?;
        return sibling_note(&identifier[..first.len()], source);
    }
    Some(format!(
        "[navigation] Related Rust spelling `{identifier}` appears in this file on lines {}. For CLI changes, inspect a working sibling's argument definition and follow its dispatch to the handler. Try the requested invocation with real arguments and check its output.\n",
        lines.join(", ")
    ))
}

struct CommandSites<'a> {
    owner: &'a str,
    name: &'a str,
    definition: usize,
    arm: usize,
}

/// A directory search can locate an exact Rust CLI spelling in a file
/// already visited, even when the literal hits occur only in callers.
pub(super) fn exact_command_note(
    file: &Path,
    display_path: &str,
    spelling: &str,
    source: &str,
) -> Option<String> {
    if file.extension()?.to_str()? != "rs"
        || source.len() > MAX_SOURCE_BYTES
        || !source.contains(spelling)
    {
        return None;
    }
    let sites = command_sites(source, |name| name == spelling)?;
    Some(format!("[navigation] In `{display_path}`, enum `{}` declares `{}` at line {} and its qualified match arm is at line {}.\n",sites.owner,sites.name,sites.definition,sites.arm))
}

/// When a new command is absent, show a nearby spelling only if the same file
/// contains one matching qualified arm and a variant in the named enum.
fn sibling_note(prefix: &str, source: &str) -> Option<String> {
    if prefix.len() < 3 {
        return None;
    }
    let sites = command_sites(source, |name| {
        name.strip_prefix(prefix).is_some_and(|rest| {
            rest.starts_with(|c: char| c.is_ascii_uppercase() || c.is_ascii_digit())
        })
    })?;
    Some(format!("[navigation] A related spelling `{}` appears in enum `{}` at line {} and in a qualified match arm at line {}. Inspect these lines for how an existing command is wired.\n", sites.name,sites.owner,sites.definition,sites.arm))
}

fn command_sites(source: &str, matches_name: impl Fn(&str) -> bool) -> Option<CommandSites<'_>> {
    let separator = |c: char| !c.is_alphanumeric() && c != '_';
    let mut arm = None;
    for (i, line) in source.lines().enumerate() {
        let Some((pattern, _)) = line.split_once("=>") else {
            continue;
        };
        if pattern.contains(['/', '*', '"', '\'']) {
            continue;
        }
        for (at, _) in pattern.match_indices("::") {
            let owner = pattern[..at].rsplit(separator).next()?;
            let name = pattern[at + 2..].split(separator).next()?;
            if !matches_name(name) || !owner.starts_with(|c: char| c.is_ascii_uppercase()) {
                continue;
            }
            if arm.replace((owner, name, i + 1)).is_some() {
                return None;
            }
        }
    }
    let (owner, name, arm_line) = arm?;
    let mut enum_indent = None;
    let mut definition = None;
    for (i, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        let indent = line.len() - trimmed.len();
        if let Some(header) = trimmed.trim_end().strip_suffix('{') {
            let mut words = header.split_whitespace().rev();
            if words.next() == Some(owner) && words.next() == Some("enum") {
                enum_indent = Some(indent);
                continue;
            }
        }
        if enum_indent.is_some_and(|start| indent <= start) && trimmed.starts_with('}') {
            enum_indent = None;
        }
        let variant = trimmed.strip_prefix(name).is_some_and(|rest| {
            rest.trim_start().starts_with(['(', '{', ',']) && !rest.contains("=>")
        });
        if !variant || enum_indent.is_none_or(|start| indent <= start) {
            continue;
        }
        if definition.replace(i + 1).is_some() {
            return None;
        }
    }
    Some(CommandSites {
        owner,
        name,
        definition: definition?,
        arm: arm_line,
    })
}

pub(super) fn rust_spelling(pattern: &str) -> Option<String> {
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
