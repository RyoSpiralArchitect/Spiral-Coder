/// Structured file-editing tools for the Coder agent.
///
/// These complement the `exec` tool for file I/O:
///   read_file  — read file content with truncation (safe, no shell)
///   write_file — atomic create/overwrite (handles encoding correctly)
///   patch_file — exact SEARCH/REPLACE (no quoting issues)
///
/// All paths are validated to stay within `base` (tool_root).
use anyhow::{anyhow, Result};
use std::path::{Component, Path, PathBuf};

mod diagnostics;
#[cfg(test)]
mod diagnostics_tests;

// ── Path safety ───────────────────────────────────────────────────────────────

fn normalize_component(s: String) -> String {
    if cfg!(target_os = "windows") {
        s.to_ascii_lowercase()
    } else {
        s
    }
}

fn components_vec(path: &Path) -> Vec<String> {
    path.components()
        .filter_map(|c| match c {
            Component::CurDir => None,
            Component::Prefix(pre) => Some(normalize_component(
                pre.as_os_str().to_string_lossy().into_owned(),
            )),
            Component::RootDir => Some("/".to_string()),
            Component::Normal(os) => Some(normalize_component(os.to_string_lossy().into_owned())),
            Component::ParentDir => Some("..".to_string()),
        })
        .collect()
}

fn is_within_root(p: &Path, root: &Path) -> bool {
    let p_vec = components_vec(p);
    let root_vec = components_vec(root);
    !root_vec.is_empty() && p_vec.starts_with(&root_vec)
}

/// Resolve a file path safely within `base` (tool_root).
///
/// - Relative paths: joined with base; `..` components are rejected.
/// - Absolute paths: allowed only if they start with base (when base is set).
pub fn resolve_safe_path(rel: &str, base: Option<&str>) -> Result<PathBuf> {
    let rel = rel.trim();
    if rel.is_empty() {
        return Err(anyhow!("path cannot be empty"));
    }

    let p = Path::new(rel);

    if p.is_absolute() {
        // Absolute paths must still be traversal-free (e.g. "/root/../escape").
        if p.components().any(|c| matches!(c, Component::ParentDir)) {
            return Err(anyhow!("path traversal ('..') not allowed: {}", rel));
        }
        if let Some(root) = base {
            let root_p = Path::new(root);
            if !is_within_root(p, root_p) {
                return Err(anyhow!(
                    "absolute path '{}' is outside tool_root '{}'",
                    rel,
                    root
                ));
            }
        }
        return Ok(p.to_path_buf());
    }

    // Relative: accept only normal path components (no absolute-ish prefixes, no "..").
    for comp in p.components() {
        match comp {
            Component::CurDir | Component::Normal(_) => {}
            Component::ParentDir => {
                return Err(anyhow!("path traversal ('..') not allowed: {}", rel));
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(anyhow!("absolute path not allowed: {}", rel));
            }
        }
    }

    let abs = match base {
        Some(root) => PathBuf::from(root).join(p),
        None => std::env::current_dir().unwrap_or_default().join(p),
    };

    Ok(abs)
}

// ── read_file ─────────────────────────────────────────────────────────────────

/// Maximum characters returned from a single read_file call.
/// ~8000 chars ≈ 2000 tokens; enough for most source files.
const MAX_READ_CHARS: usize = 8000;

/// Read a file's content. Returns `(result_text, is_error)`.
///
/// On success: header line + file content (truncated if large).
/// On error:   human-readable error message.
pub fn tool_read_file(path: &str, base: Option<&str>) -> (String, bool) {
    let abs_path = match resolve_safe_path(path, base) {
        Ok(p) => p,
        Err(e) => return (format!("ERROR: {e}"), true),
    };

    match std::fs::read_to_string(&abs_path) {
        Ok(content) => {
            let line_count = content.lines().count();
            let byte_count = content.len();
            let char_count = content.chars().count();

            let (shown, truncated) = if char_count > MAX_READ_CHARS {
                let s: String = content.chars().take(MAX_READ_CHARS).collect();
                (s, true)
            } else {
                (content, false)
            };

            let header = format!("[{path}] ({line_count} lines, {byte_count} bytes)\n");
            let trunc_note = if truncated {
                let hidden = line_count.saturating_sub(shown.lines().count());
                format!("\n[…truncated — {hidden} more lines not shown; use exec to view specific ranges]")
            } else {
                String::new()
            };

            (format!("{header}{shown}{trunc_note}"), false)
        }
        Err(e) => (format!("ERROR reading '{path}': {e}"), true),
    }
}

// ── write_file ────────────────────────────────────────────────────────────────

/// Atomically write `content` to a file (temp → rename).
/// Creates parent directories as needed. Returns `(result_text, is_error)`.
pub fn tool_write_file(path: &str, content: &str, base: Option<&str>) -> (String, bool) {
    let abs_path = match resolve_safe_path(path, base) {
        Ok(p) => p,
        Err(e) => return (format!("ERROR: {e}"), true),
    };

    // Create parent directories.
    if let Some(parent) = abs_path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            return (format!("ERROR creating parent directories: {e}"), true);
        }
    }

    // Atomic write: temp file → rename.
    let mut tmp_os = abs_path.as_os_str().to_owned();
    tmp_os.push(".__spiral_coder_tmp");
    let tmp_path = PathBuf::from(tmp_os);

    if let Err(e) = std::fs::write(&tmp_path, content) {
        return (format!("ERROR writing temp file: {e}"), true);
    }

    if let Err(e) = std::fs::rename(&tmp_path, &abs_path) {
        let _ = std::fs::remove_file(&tmp_path);
        return (format!("ERROR finalizing write to '{path}': {e}"), true);
    }

    let line_count = content.lines().count();
    let byte_count = content.len();
    (
        format!("OK: wrote '{path}' ({line_count} lines, {byte_count} bytes)"),
        false,
    )
}

// ── patch_file ────────────────────────────────────────────────────────────────

/// Apply a SEARCH/REPLACE edit. `search` must appear **exactly once**.
/// Returns `(result_text, is_error)`.
pub fn tool_patch_file(
    path: &str,
    search: &str,
    replace: &str,
    base: Option<&str>,
) -> (String, bool) {
    if search.is_empty() {
        return ("ERROR: search string cannot be empty".to_string(), true);
    }

    let abs_path = match resolve_safe_path(path, base) {
        Ok(p) => p,
        Err(e) => return (format!("ERROR: {e}"), true),
    };

    let content = match std::fs::read_to_string(&abs_path) {
        Ok(c) => c,
        Err(e) => return (format!("ERROR reading '{path}': {e}"), true),
    };

    let count = content.matches(search).count();
    if count == 0 {
        if let Some(new_content) = replace_loose_line_block_once(&content, search, replace) {
            return write_patch_result(path, &abs_path, &content, &new_content);
        }

        let context = diagnostics::missing_anchor(&content, search);
        return (
            format!(
                "ERROR: search string not found in '{path}'.\n\
                 {context}\n\
                 Tip: call read_file first to inspect exact content, then retry with the exact text."
            ),
            true,
        );
    }
    if count > 1 {
        let context = diagnostics::ambiguous_anchor(&content, search);
        return (
            format!(
                "ERROR: search string found {count} times in '{path}' — must be unique.\n\
                 {context}\n\
                 Tip: include more surrounding lines to make the match unique."
            ),
            true,
        );
    }

    let new_content = content.replacen(search, replace, 1);

    write_patch_result(path, &abs_path, &content, &new_content)
}

fn write_patch_result(
    path: &str,
    abs_path: &Path,
    old_content: &str,
    new_content: &str,
) -> (String, bool) {
    // Atomic write.
    let mut tmp_os = abs_path.as_os_str().to_owned();
    tmp_os.push(".__spiral_coder_tmp");
    let tmp_path = PathBuf::from(tmp_os);

    if let Err(e) = std::fs::write(&tmp_path, new_content) {
        return (format!("ERROR writing temp file: {e}"), true);
    }
    if let Err(e) = std::fs::rename(&tmp_path, &abs_path) {
        let _ = std::fs::remove_file(&tmp_path);
        return (format!("ERROR finalizing patch to '{path}': {e}"), true);
    }

    let old_lines = old_content.lines().count();
    let new_lines = new_content.lines().count();
    let delta = new_lines as i64 - old_lines as i64;
    let delta_str = if delta >= 0 {
        format!("+{delta}")
    } else {
        format!("{delta}")
    };

    (
        format!("OK: patched '{path}' ({delta_str} lines, {new_lines} total)"),
        false,
    )
}

#[derive(Debug, Clone, Copy)]
struct LineSpan<'a> {
    text: &'a str,
    start: usize,
    end_with_newline: usize,
}

fn replace_loose_line_block_once(content: &str, search: &str, replace: &str) -> Option<String> {
    let search_lines = search
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(str::trim)
        .collect::<Vec<_>>();
    if search_lines.len() < 3 {
        return None;
    }

    let lines = line_spans(content);
    let mut matches = Vec::new();
    for start_idx in 0..lines.len() {
        let Some((start, end)) = loose_line_match_at(&lines, &search_lines, start_idx) else {
            continue;
        };
        matches.push((start, end));
        if matches.len() > 1 {
            return None;
        }
    }

    let (start, end) = matches.pop()?;
    let mut adjusted_replace = replace.to_string();
    if content[start..end].ends_with('\n') && !adjusted_replace.ends_with('\n') {
        adjusted_replace.push('\n');
    }
    Some(format!(
        "{}{}{}",
        &content[..start],
        adjusted_replace,
        &content[end..]
    ))
}

fn loose_line_match_at(
    lines: &[LineSpan<'_>],
    search_lines: &[&str],
    start_idx: usize,
) -> Option<(usize, usize)> {
    let mut content_idx = start_idx;
    let mut search_idx = 0;
    let mut first_start = None;
    let mut last_end = None;

    while search_idx < search_lines.len() {
        let line = *lines.get(content_idx)?;
        let trimmed = line.text.trim();
        content_idx += 1;

        if trimmed.is_empty() {
            continue;
        }
        if trimmed != search_lines[search_idx] {
            return None;
        }
        first_start.get_or_insert(line.start);
        last_end = Some(line.end_with_newline);
        search_idx += 1;
    }

    Some((first_start?, last_end?))
}

fn line_spans(content: &str) -> Vec<LineSpan<'_>> {
    let mut out = Vec::new();
    let mut start = 0;
    for segment in content.split_inclusive('\n') {
        let end = start + segment.len();
        let text = segment.strip_suffix('\n').unwrap_or(segment);
        out.push(LineSpan {
            text,
            start,
            end_with_newline: end,
        });
        start = end;
    }
    if start < content.len() || content.is_empty() {
        let text = &content[start..];
        out.push(LineSpan {
            text,
            start,
            end_with_newline: content.len(),
        });
    }
    out
}

// ── search_files ──────────────────────────────────────────────────────────────

const MAX_SEARCH_RESULTS: usize = 50;
const MAX_SEARCH_OUT_CHARS: usize = 6000;
const MAX_LINE_DISPLAY: usize = 200;

/// True for directory names that should never be searched.
fn skip_dir(name: &str) -> bool {
    matches!(
        name,
        "target"
            | "node_modules"
            | ".git"
            | "__pycache__"
            | "dist"
            | "build"
            | ".next"
            | ".nuxt"
            | "vendor"
            | ".venv"
            | "venv"
            | ".tox"
            | "coverage"
            | ".cache"
            | ".idea"
            | ".vscode"
            | "out"
            | ".svn"
    )
}

/// True for file extensions that are clearly binary — skip to avoid UTF-8 errors.
fn skip_extension(ext: &str) -> bool {
    matches!(
        ext,
        "exe"
            | "dll"
            | "so"
            | "dylib"
            | "bin"
            | "o"
            | "a"
            | "obj"
            | "wasm"
            | "zip"
            | "tar"
            | "gz"
            | "bz2"
            | "7z"
            | "rar"
            | "xz"
            | "zst"
            | "jpg"
            | "jpeg"
            | "png"
            | "gif"
            | "bmp"
            | "ico"
            | "webp"
            | "avif"
            | "mp3"
            | "mp4"
            | "wav"
            | "avi"
            | "mov"
            | "mkv"
            | "flac"
            | "ogg"
            | "pdf"
            | "doc"
            | "docx"
            | "xls"
            | "xlsx"
            | "ppt"
            | "pptx"
            | "db"
            | "sqlite"
            | "sqlite3"
            | "parquet"
            | "arrow"
            | "lock" // Cargo.lock / package-lock.json can be enormous
    )
}

fn finalize_search_output(
    pattern: &str,
    results: &[String],
    truncated: bool,
    note: Option<&str>,
) -> String {
    let count = results.len();
    let cap_note = if truncated {
        format!(" (first {MAX_SEARCH_RESULTS} shown — more may exist)")
    } else {
        String::new()
    };
    let header = format!(
        "[search_files: '{}' — {} match(es){}]\n",
        pattern, count, cap_note
    );
    let body = results.join("\n");
    let out = if body.chars().count() > MAX_SEARCH_OUT_CHARS {
        let trunc: String = body.chars().take(MAX_SEARCH_OUT_CHARS).collect();
        format!("{header}{trunc}\n[…output truncated]")
    } else {
        format!("{header}{body}")
    };
    match note {
        Some(note) if !note.is_empty() => format!("{out}\n[note] {note}"),
        _ => out,
    }
}

fn display_search_path(path: &Path, search_root: &Path, base: Option<&str>) -> String {
    if let Some(root) = base {
        if let Ok(rel) = path.strip_prefix(root) {
            return rel.to_string_lossy().replace('\\', "/");
        }
    }
    path.strip_prefix(search_root)
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| path.display().to_string())
}

fn search_single_file(
    pattern: &str,
    file_path: &Path,
    case_insensitive: bool,
    display_path: &str,
) -> (String, bool) {
    let content = match std::fs::read_to_string(file_path) {
        Ok(c) => c,
        Err(e) => {
            return (
                format!("ERROR reading '{}': {e}", file_path.display()),
                true,
            )
        }
    };

    let needle = if case_insensitive {
        pattern.to_ascii_lowercase()
    } else {
        pattern.to_string()
    };

    let mut results: Vec<String> = Vec::new();
    let mut truncated = false;

    for (ln, line) in content.lines().enumerate() {
        let cmp = if case_insensitive {
            line.to_ascii_lowercase()
        } else {
            line.to_string()
        };
        if cmp.contains(&needle) {
            let display: String = line.trim_end().chars().take(MAX_LINE_DISPLAY).collect();
            results.push(format!("{}:{}: {}", display_path, ln + 1, display));
            if results.len() >= MAX_SEARCH_RESULTS {
                truncated = true;
                break;
            }
        }
    }

    if results.is_empty() {
        return (
            format!(
                "[search_files] No matches for '{}' in file '{}'\n[note] dir resolved to a file, so the search was scoped to that file",
                pattern, display_path
            ),
            false,
        );
    }

    (
        finalize_search_output(
            pattern,
            &results,
            truncated,
            Some("dir resolved to a file, so the search was scoped to that file"),
        ),
        false,
    )
}

/// Recursive file search (literal, no regex dependency).
/// Returns "relative/path:line_no: content" for each matching line.
pub fn tool_search_files(
    pattern: &str,
    dir: &str,
    case_insensitive: bool,
    base: Option<&str>,
) -> (String, bool) {
    if pattern.is_empty() {
        return ("ERROR: search pattern cannot be empty".to_string(), true);
    }

    // Resolve search root.
    let search_root = if dir.trim().is_empty() {
        base.map(PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
    } else {
        match resolve_safe_path(dir, base) {
            Ok(p) => p,
            Err(e) => return (format!("ERROR: {e}"), true),
        }
    };

    if search_root.is_file() {
        let display_path = match base {
            Some(root) => search_root
                .strip_prefix(root)
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .unwrap_or_else(|_| search_root.display().to_string()),
            None => search_root.display().to_string(),
        };
        return search_single_file(pattern, &search_root, case_insensitive, &display_path);
    }

    if !search_root.is_dir() {
        return (
            format!("ERROR: '{}' is not a directory", search_root.display()),
            true,
        );
    }

    let needle = if case_insensitive {
        pattern.to_ascii_lowercase()
    } else {
        pattern.to_string()
    };

    let mut results: Vec<String> = Vec::new();
    let mut stack: Vec<PathBuf> = vec![search_root.clone()];
    let mut truncated = false;

    'outer: while let Some(dir_path) = stack.pop() {
        let rd = match std::fs::read_dir(&dir_path) {
            Ok(r) => r,
            Err(_) => continue,
        };

        let mut subdirs: Vec<PathBuf> = Vec::new();
        let mut files: Vec<PathBuf> = Vec::new();

        for entry in rd.flatten() {
            let path = entry.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name.starts_with('.') && name != ".spiral-coder.md" {
                continue; // skip hidden
            }
            if path.is_dir() {
                if !skip_dir(name) {
                    subdirs.push(path);
                }
            } else {
                let ext = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_ascii_lowercase();
                if !skip_extension(&ext) {
                    files.push(path);
                }
            }
        }

        for file_path in &files {
            let content = match std::fs::read_to_string(file_path) {
                Ok(c) => c,
                Err(_) => continue,
            };
            let rel = display_search_path(file_path, &search_root, base);

            for (ln, line) in content.lines().enumerate() {
                let cmp = if case_insensitive {
                    line.to_ascii_lowercase()
                } else {
                    line.to_string()
                };
                if cmp.contains(&needle) {
                    let display: String = line.trim_end().chars().take(MAX_LINE_DISPLAY).collect();
                    results.push(format!("{}:{}: {}", rel, ln + 1, display));
                    if results.len() >= MAX_SEARCH_RESULTS {
                        truncated = true;
                        break 'outer;
                    }
                }
            }
        }

        stack.extend(subdirs);
    }

    if results.is_empty() {
        return (
            format!(
                "[search_files] No matches for '{}' in '{}'",
                pattern,
                search_root.display()
            ),
            false,
        );
    }

    (
        finalize_search_output(pattern, &results, truncated, None),
        false,
    )
}

// ── apply_diff ────────────────────────────────────────────────────────────────

/// One line in a diff hunk.
enum DiffLine {
    Context(String),
    Remove(String),
    Add(String),
}

struct DiffHunk {
    lines: Vec<DiffLine>,
}

/// Parse standard unified diff into hunks.
/// Skips `--- a/…` / `+++ b/…` header lines automatically.
fn parse_diff_hunks(diff: &str) -> Vec<DiffHunk> {
    let mut hunks: Vec<DiffHunk> = Vec::new();
    let mut current: Option<DiffHunk> = None;

    for line in diff.lines() {
        if line.starts_with("@@") {
            if let Some(h) = current.take() {
                hunks.push(h);
            }
            current = Some(DiffHunk { lines: Vec::new() });
        } else if let Some(ref mut hunk) = current {
            if line.starts_with("---") || line.starts_with("+++") {
                // Skip file header lines that may appear inside diff output.
                continue;
            } else if let Some(rest) = line.strip_prefix('-') {
                hunk.lines.push(DiffLine::Remove(rest.to_string()));
            } else if let Some(rest) = line.strip_prefix('+') {
                hunk.lines.push(DiffLine::Add(rest.to_string()));
            } else {
                // Context line — may start with a space or be empty.
                let ctx = if line.starts_with(' ') {
                    &line[1..]
                } else {
                    line
                };
                hunk.lines.push(DiffLine::Context(ctx.to_string()));
            }
        }
    }
    if let Some(h) = current {
        hunks.push(h);
    }
    hunks
}

/// Apply a unified diff to a file.  Each `@@` hunk is matched by content
/// (context + remove lines) and replaced with context + add lines.
/// Multiple hunks per file are supported; applied in order.
pub fn tool_apply_diff(path: &str, diff: &str, base: Option<&str>) -> (String, bool) {
    if diff.trim().is_empty() {
        return ("ERROR: diff cannot be empty".to_string(), true);
    }

    let abs_path = match resolve_safe_path(path, base) {
        Ok(p) => p,
        Err(e) => return (format!("ERROR: {e}"), true),
    };

    let content = match std::fs::read_to_string(&abs_path) {
        Ok(c) => c,
        Err(e) => return (format!("ERROR reading '{path}': {e}"), true),
    };

    let hunks = parse_diff_hunks(diff);
    if hunks.is_empty() {
        return (
            "ERROR: no valid @@ hunks found in diff — make sure to include @@ markers".to_string(),
            true,
        );
    }

    let mut new_content = content.clone();
    let mut applied = 0usize;
    let mut errors: Vec<String> = Vec::new();
    let mut diagnostic_contexts: Vec<String> = Vec::new();

    for (i, hunk) in hunks.iter().enumerate() {
        // Build the "old block" (context + remove lines) and "new block" (context + add lines).
        let old_lines: Vec<&str> = hunk
            .lines
            .iter()
            .filter_map(|l| match l {
                DiffLine::Context(s) | DiffLine::Remove(s) => Some(s.as_str()),
                DiffLine::Add(_) => None,
            })
            .collect();
        let new_lines: Vec<&str> = hunk
            .lines
            .iter()
            .filter_map(|l| match l {
                DiffLine::Context(s) | DiffLine::Add(s) => Some(s.as_str()),
                DiffLine::Remove(_) => None,
            })
            .collect();

        if old_lines.is_empty() {
            errors.push(format!("hunk {}: no context/remove lines", i + 1));
            continue;
        }

        let old_block = old_lines.join("\n");
        let new_block = new_lines.join("\n");

        let count = new_content.matches(&old_block).count();
        if count == 0 {
            let preview: String = old_lines
                .iter()
                .take(3)
                .map(|line| line.chars().take(120).collect::<String>())
                .collect::<Vec<_>>()
                .join("\\n");
            errors.push(format!(
                "hunk {}: old block not found (starts: {:?})",
                i + 1,
                preview
            ));
            if diagnostic_contexts.len() < 2 {
                diagnostic_contexts.push(format!(
                    "Hunk {} input (before this hunk; diagnostic only):\n{}",
                    i + 1,
                    diagnostics::missing_anchor(&new_content, &old_block)
                ));
            }
        } else if count > 1 {
            errors.push(format!(
                "hunk {}: old block not unique ({count} matches) — add more context lines",
                i + 1
            ));
            if diagnostic_contexts.len() < 2 {
                diagnostic_contexts.push(format!(
                    "Hunk {} input (before this hunk; diagnostic only):\n{}",
                    i + 1,
                    diagnostics::ambiguous_anchor(&new_content, &old_block)
                ));
            }
        } else {
            new_content = new_content.replacen(&old_block, &new_block, 1);
            applied += 1;
        }
    }

    if applied == 0 {
        return (
            format!(
                "ERROR: no hunks applied ({}).\n{}\nTip: call read_file to inspect exact content.",
                errors.join("; "),
                diagnostic_contexts.join("\n")
            ),
            true,
        );
    }

    // Atomic write.
    let mut tmp_os = abs_path.as_os_str().to_owned();
    tmp_os.push(".__spiral_coder_tmp");
    let tmp_path = PathBuf::from(tmp_os);

    if let Err(e) = std::fs::write(&tmp_path, &new_content) {
        return (format!("ERROR writing temp file: {e}"), true);
    }
    if let Err(e) = std::fs::rename(&tmp_path, &abs_path) {
        let _ = std::fs::remove_file(&tmp_path);
        return (format!("ERROR finalizing diff to '{path}': {e}"), true);
    }

    let warn = if errors.is_empty() {
        String::new()
    } else {
        format!(
            "\n⚠ {}/{} hunks skipped: {}\n{}",
            errors.len(),
            hunks.len(),
            errors.join("; "),
            diagnostic_contexts.join("\n")
        )
    };

    (
        format!(
            "OK: applied {applied}/{} hunk(s) to '{path}'{warn}",
            hunks.len()
        ),
        false,
    )
}

// ── glob_files ────────────────────────────────────────────────────────────────

const MAX_LIST_DIR_ENTRIES: usize = 200;

/// List a single directory (non-recursive).
/// Returns a compact, sorted listing suitable for LLM context.
pub fn tool_list_dir(
    dir: &str,
    max_entries: usize,
    include_hidden: bool,
    base: Option<&str>,
) -> (String, bool) {
    let max_entries = if max_entries == 0 {
        MAX_LIST_DIR_ENTRIES
    } else {
        max_entries
    }
    .clamp(1, 500);

    let (dir_label, dir_path) = if dir.trim().is_empty() {
        (
            ".".to_string(),
            base.map(PathBuf::from)
                .unwrap_or_else(|| std::env::current_dir().unwrap_or_default()),
        )
    } else {
        match resolve_safe_path(dir, base) {
            Ok(p) => (dir.to_string(), p),
            Err(e) => return (format!("ERROR: {e}"), true),
        }
    };

    if !dir_path.is_dir() {
        return (
            format!("ERROR: '{}' is not a directory", dir_path.display()),
            true,
        );
    }

    let rd = match std::fs::read_dir(&dir_path) {
        Ok(r) => r,
        Err(e) => {
            return (
                format!("ERROR: cannot read dir '{}': {e}", dir_path.display()),
                true,
            )
        }
    };

    let mut dirs: Vec<String> = Vec::new();
    let mut files: Vec<String> = Vec::new();

    for entry in rd.flatten() {
        let path = entry.path();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        if name.is_empty() {
            continue;
        }
        if !include_hidden && name.starts_with('.') && name != ".spiral-coder.md" {
            continue;
        }
        if path.is_dir() {
            dirs.push(format!("{name}/"));
        } else {
            let size = entry.metadata().ok().map(|m| m.len()).unwrap_or(0);
            files.push(format!("{name} ({size} bytes)"));
        }
    }

    dirs.sort();
    files.sort();

    let total = dirs.len() + files.len();
    let mut out: Vec<String> = Vec::new();
    for d in dirs {
        out.push(format!("d {d}"));
        if out.len() >= max_entries {
            break;
        }
    }
    if out.len() < max_entries {
        for f in files {
            out.push(format!("f {f}"));
            if out.len() >= max_entries {
                break;
            }
        }
    }

    let shown = out.len();
    let trunc_note = if shown < total {
        format!(" (showing first {shown}/{total}; set max_entries higher to see more)")
    } else {
        String::new()
    };

    let mut s = String::new();
    s.push_str(&format!(
        "[list_dir: '{}' ・ {} item(s){}]\n",
        dir_label, total, trunc_note
    ));
    for line in out {
        s.push_str(&line);
        s.push('\n');
    }
    (s.trim_end().to_string(), false)
}

const MAX_GLOB_RESULTS: usize = 200;

/// Minimal glob matcher (no regex crate required).
///
/// Supports:  `*`  → any chars except `/`
///            `**` → any chars including `/`
///            `?`  → single char except `/`
///            literal chars
fn glob_match(pattern: &str, path: &str) -> bool {
    glob_inner(pattern.as_bytes(), path.as_bytes())
}

fn glob_inner(pat: &[u8], s: &[u8]) -> bool {
    if pat.is_empty() {
        return s.is_empty();
    }
    // `**` — matches any sequence including path separators.
    if pat.starts_with(b"**") {
        let rest = if pat.get(2) == Some(&b'/') {
            &pat[3..]
        } else {
            &pat[2..]
        };
        if rest.is_empty() {
            return true;
        }
        for i in 0..=s.len() {
            if glob_inner(rest, &s[i..]) {
                return true;
            }
        }
        return false;
    }
    // `*` — matches any chars except `/`.
    if pat[0] == b'*' {
        if s.is_empty() || s[0] == b'/' {
            return glob_inner(&pat[1..], s);
        }
        return glob_inner(&pat[1..], s) || glob_inner(pat, &s[1..]);
    }
    // `?` — single non-separator char.
    if pat[0] == b'?' {
        return !s.is_empty() && s[0] != b'/' && glob_inner(&pat[1..], &s[1..]);
    }
    // Literal.
    !s.is_empty() && pat[0] == s[0] && glob_inner(&pat[1..], &s[1..])
}

/// Walk `search_root` and return paths (relative, forward-slash) that match `pattern`.
pub fn tool_glob_files(pattern: &str, dir: &str, base: Option<&str>) -> (String, bool) {
    let pattern = pattern.trim();
    if pattern.is_empty() {
        return ("ERROR: glob pattern cannot be empty".to_string(), true);
    }

    let search_root = if dir.trim().is_empty() {
        base.map(PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
    } else {
        match resolve_safe_path(dir, base) {
            Ok(p) => p,
            Err(e) => return (format!("ERROR: {e}"), true),
        }
    };

    if !search_root.is_dir() {
        return (
            format!("ERROR: '{}' is not a directory", search_root.display()),
            true,
        );
    }

    let mut results: Vec<String> = Vec::new();
    let mut stack: Vec<PathBuf> = vec![search_root.clone()];
    let mut truncated = false;

    'outer: while let Some(dir_path) = stack.pop() {
        let rd = match std::fs::read_dir(&dir_path) {
            Ok(r) => r,
            Err(_) => continue,
        };

        let mut subdirs: Vec<PathBuf> = Vec::new();

        for entry in rd.flatten() {
            let path = entry.path();
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();
            if name.starts_with('.') {
                continue;
            }
            if path.is_dir() {
                if !skip_dir(&name) {
                    subdirs.push(path);
                }
            } else {
                let rel = path
                    .strip_prefix(&search_root)
                    .map(|p| p.to_string_lossy().replace('\\', "/"))
                    .unwrap_or_else(|_| path.display().to_string());
                if glob_match(pattern, &rel) {
                    results.push(rel);
                    if results.len() >= MAX_GLOB_RESULTS {
                        truncated = true;
                        break 'outer;
                    }
                }
            }
        }
        stack.extend(subdirs);
    }

    if results.is_empty() {
        return (
            format!(
                "[glob] No files matching '{}' in '{}'",
                pattern,
                search_root.display()
            ),
            false,
        );
    }

    results.sort();
    let cap_note = if truncated {
        format!(" (first {MAX_GLOB_RESULTS} shown)")
    } else {
        String::new()
    };
    let header = format!(
        "[glob: '{}' — {} file(s){}]\n",
        pattern,
        results.len(),
        cap_note
    );
    (format!("{}{}", header, results.join("\n")), false)
}

// ── make_patch_diff ───────────────────────────────────────────────────────────

/// Generate a compact context diff showing what patch_file changed.
/// Returns an empty string if the search text can't be located in content.
pub fn make_patch_diff(content: &str, search: &str, replace: &str) -> String {
    let byte_pos = match content.find(search) {
        Some(p) => p,
        None => return String::new(),
    };

    const CTX: usize = 3;
    let lines_before: Vec<&str> = content[..byte_pos].lines().collect();
    let after_offset = byte_pos + search.len();
    let lines_after: Vec<&str> = content[after_offset..].lines().take(CTX).collect();

    let ctx_start = lines_before.len().saturating_sub(CTX);
    let line_no = lines_before.len() + 1; // 1-based line number of change

    let mut out = format!("[diff @line {}]\n", line_no);
    for line in &lines_before[ctx_start..] {
        out.push_str(&format!("  {line}\n"));
    }
    let remove_empty = search.trim_end_matches('\n').is_empty();
    if !remove_empty {
        for line in search.lines() {
            out.push_str(&format!("- {line}\n"));
        }
    }
    let add_empty = replace.trim_end_matches('\n').is_empty();
    if !add_empty {
        for line in replace.lines() {
            out.push_str(&format!("+ {line}\n"));
        }
    }
    for line in &lines_after {
        out.push_str(&format!("  {line}\n"));
    }
    out.trim_end().to_string()
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_path_rejects_dotdot() {
        assert!(resolve_safe_path("../etc/passwd", Some("/tmp/root")).is_err());
        assert!(resolve_safe_path("a/../../etc", Some("/tmp/root")).is_err());
    }

    #[test]
    fn safe_path_relative_ok() {
        let p = resolve_safe_path("src/main.rs", Some("/tmp/root")).unwrap();
        assert!(p.to_string_lossy().contains("src"));
    }

    #[test]
    fn safe_path_rejects_absolute_prefix_trick() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_string_lossy().into_owned();
        let outside = format!("{root}2");
        assert!(resolve_safe_path(&outside, Some(&root)).is_err());
    }

    #[test]
    fn safe_path_rejects_absolute_with_dotdot() {
        let dir = tempfile::tempdir().unwrap();
        let root_p = dir.path();
        let root = root_p.to_string_lossy().into_owned();
        let traversal = root_p.join("..").join("x");
        assert!(resolve_safe_path(&traversal.to_string_lossy(), Some(&root)).is_err());
    }

    #[test]
    fn write_then_read() {
        let dir = std::env::temp_dir().join("spiral_coder_test_wr");
        let _ = std::fs::create_dir_all(&dir);
        let base = dir.to_string_lossy().into_owned();

        let (r, err) = tool_write_file("hello.txt", "hello world\n", Some(&base));
        assert!(!err, "{r}");

        let (r2, err2) = tool_read_file("hello.txt", Some(&base));
        assert!(!err2, "{r2}");
        assert!(r2.contains("hello world"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn patch_basic() {
        let dir = std::env::temp_dir().join("spiral_coder_test_patch");
        let _ = std::fs::create_dir_all(&dir);
        let base = dir.to_string_lossy().into_owned();

        tool_write_file("f.txt", "line1\nfoo bar\nline3\n", Some(&base));
        let (r, err) = tool_patch_file("f.txt", "foo bar", "baz qux", Some(&base));
        assert!(!err, "{r}");

        let (content, _) = tool_read_file("f.txt", Some(&base));
        assert!(content.contains("baz qux"));
        assert!(!content.contains("foo bar"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn patch_not_found_shows_preview() {
        let dir = std::env::temp_dir().join("spiral_coder_test_notfound");
        let _ = std::fs::create_dir_all(&dir);
        let base = dir.to_string_lossy().into_owned();

        tool_write_file("f.txt", "hello world\n", Some(&base));
        let (r, err) = tool_patch_file("f.txt", "nonexistent", "x", Some(&base));
        assert!(err);
        assert!(r.contains("not found"));
        assert!(r.contains("hello world"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn patch_repairs_indent_only_multiline_search_miss() {
        let dir = std::env::temp_dir().join("spiral_coder_test_patch_indent_fallback");
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(dir.join("src"));
        let content = r#"const PR_READY_RUNTIME_PATHS: &[&str] = &[
    "src/tui/agent/task_harness.rs",
    "src/tui/agent/done_gate.rs",
];

pub fn requires_pr_ready_handoff(path: &str) -> bool {
    PR_READY_RUNTIME_PATHS.contains(&path)
}
"#;
        let _ = std::fs::write(dir.join("src").join("followup.rs"), content);
        let base = dir.to_string_lossy().into_owned();
        let search = "const PR_READY_RUNTIME_PATHS: &[&str] = &[\n\"src/tui/agent/task_harness.rs\",\n\"src/tui/agent/done_gate.rs\",\n];\npub fn requires_pr_ready_handoff(path: &str) -> bool {\nPR_READY_RUNTIME_PATHS.contains(&path)\n}";
        let replace = r#"const PR_READY_RUNTIME_PATHS: &[&str] = &[
    "src/tui/agent/task_harness.rs",
    "src/tui/agent/done_gate.rs",
    "src/tui/agent/session_bridge.rs",
];

pub fn requires_pr_ready_handoff(path: &str) -> bool {
    PR_READY_RUNTIME_PATHS.contains(&path)
}
"#;

        let (r, err) = tool_patch_file("src/followup.rs", search, replace, Some(&base));
        assert!(!err, "{r}");
        let updated = std::fs::read_to_string(dir.join("src").join("followup.rs")).unwrap();
        assert!(updated.contains("src/tui/agent/session_bridge.rs"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn patch_keeps_short_search_misses_strict() {
        let dir = std::env::temp_dir().join("spiral_coder_test_patch_short_strict");
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        let base = dir.to_string_lossy().into_owned();

        tool_write_file("f.txt", "    hello\n", Some(&base));
        let (r, err) = tool_patch_file("f.txt", "hello\nmissing", "hi", Some(&base));
        assert!(err, "{r}");
        assert!(r.contains("not found"), "{r}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn list_dir_basic() {
        let dir = std::env::temp_dir().join("spiral_coder_test_list_dir");
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(dir.join("a"));
        let _ = std::fs::write(dir.join("b.txt"), "hi");
        let base = dir.to_string_lossy().into_owned();

        let (r, err) = tool_list_dir("", MAX_LIST_DIR_ENTRIES, false, Some(&base));
        assert!(!err, "{r}");
        assert!(r.contains("d a/"), "{r}");
        assert!(r.contains("f b.txt"), "{r}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn search_files_accepts_file_scope_when_dir_is_file() {
        let dir = std::env::temp_dir().join("spiral_coder_test_search_file_scope");
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(dir.join("src"));
        let _ = std::fs::write(
            dir.join("src").join("events.rs"),
            "alpha\nmatch cmd_lc.as_str()\nomega\n",
        );
        let base = dir.to_string_lossy().into_owned();

        let (r, err) =
            tool_search_files("match cmd_lc.as_str()", "src/events.rs", false, Some(&base));
        assert!(!err, "{r}");
        assert!(r.contains("src/events.rs:2:"), "{r}");
        assert!(r.contains("scoped to that file"), "{r}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn search_files_reports_paths_relative_to_tool_root() {
        let dir = std::env::temp_dir().join("spiral_coder_test_search_tool_root_relative");
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(dir.join("src"));
        let _ = std::fs::write(
            dir.join("src").join("config.rs"),
            "pub struct AppConfig {\n    pub aliases: Vec<String>,\n}\n",
        );
        let base = dir.to_string_lossy().into_owned();

        let (r, err) = tool_search_files("aliases", "src", false, Some(&base));
        assert!(!err, "{r}");
        assert!(r.contains("src/config.rs:2:"), "{r}");
        assert!(!r.contains("\nconfig.rs:2:"), "{r}");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
