//! Pure edit preparation shared by direct tools and approval previews.
use super::*;

pub(crate) fn unchanged_message(path: &str) -> String {
    format!("OK: unchanged '{path}' (identical contents; no write performed)\nNo edit progress or verification was produced. Pending failures and checks remain.")
}

pub(crate) fn patch(path: &str, content: &str, search: &str, replace: &str) -> Result<String> {
    anyhow::ensure!(!search.is_empty(), "search string cannot be empty");
    let count = content.matches(search).count();
    if count == 0 {
        if let Some(new_content) = replace_loose_line_block_once(&content, search, replace) {
            return Ok(new_content);
        }

        let context = diagnostics::missing_anchor(&content, search);
        return Err(anyhow!(
                "ERROR: search string not found in '{path}'.\n\
                 {context}\n\
                 Tip: call read_file first to inspect exact content, then retry with the exact text."
        ));
    }
    if count > 1 {
        let context = diagnostics::ambiguous_anchor(&content, search);
        return Err(anyhow!(
            "ERROR: search string found {count} times in '{path}' — must be unique.\n\
                 {context}\n\
                 Tip: include more surrounding lines to make the match unique."
        ));
    }

    Ok(content.replacen(search, replace, 1))
}

pub(crate) fn diff(path: &str, content: &str, diff: &str) -> Result<(String, String)> {
    anyhow::ensure!(!diff.trim().is_empty(), "diff cannot be empty");
    let repaired = without_patch_trailer(diff, content)?;
    let hunks = parse_diff_hunks(repaired.unwrap_or(diff));
    if hunks.is_empty() {
        anyhow::bail!("ERROR: no valid @@ hunks found in diff — make sure to include @@ markers");
    }

    let mut new_content = content.to_string();
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
        } else if repaired.is_some() && !whole_line_anchor(&new_content, &old_block) {
            errors.push(format!(
                "hunk {}: expected a unique, nonblank, complete line anchor",
                i + 1
            ));
        } else {
            new_content = new_content.replacen(&old_block, &new_block, 1);
            applied += 1;
        }
    }

    if repaired.is_some() && !errors.is_empty() {
        anyhow::bail!(
            "ERROR: could not safely ignore trailing `*** End Patch`; no hunks applied. {}\n{}\nSend only the intended @@ hunks with unique, complete context lines.",
            errors.join("; "), diagnostic_contexts.join("\n")
        );
    }
    if applied == 0 {
        return Err(anyhow!(
            "ERROR: no hunks applied ({}).\n{}\nTip: call read_file to inspect exact content.",
            errors.join("; "),
            diagnostic_contexts.join("\n")
        ));
    }

    let warn = if repaired.is_some() {
        "\nFormat repair: ignored trailing `*** End Patch`.".to_string()
    } else if errors.is_empty() {
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

    Ok((
        new_content,
        format!(
            "OK: applied {applied}/{} hunk(s) to '{path}'{warn}",
            hunks.len()
        ),
    ))
}

/// Offer only an unambiguous single hunk that changes the recorded full-file read.
pub(crate) fn resumable(path: &str, source: &str, proposal: &str) -> bool {
    let Ok(repaired) = without_patch_trailer(proposal, source) else {
        return false;
    };
    let mut lines = repaired.unwrap_or(proposal).lines();
    if !lines
        .next()
        .is_some_and(|header| header == "@@" || header.starts_with("@@ "))
    {
        return false;
    }
    let mut original = Vec::new();
    for line in lines {
        if line.starts_with("---") || line.starts_with("+++") {
            return false;
        }
        if let Some(text) = line.strip_prefix([' ', '-']) {
            original.push(text);
        } else if !line.starts_with('+') {
            return false;
        }
    }
    whole_line_anchor(source, &original.join("\n"))
        && diff(path, source, proposal).is_ok_and(|(after, _)| after != source)
}

/// Recognize only a bare footer on single-file, explicitly prefixed hunks.
/// A marker with a diff prefix is file content and never enters this repair.
fn without_patch_trailer<'a>(diff: &'a str, content: &str) -> Result<Option<&'a str>> {
    const END: &str = "*** End Patch";
    let directive = |line: &str| {
        matches!(
            line,
            "*** Begin Patch" | "*** End Patch" | "*** End of File"
        ) || [
            "*** Update File:",
            "*** Add File:",
            "*** Delete File:",
            "*** Move to:",
        ]
        .iter()
        .any(|prefix| line.starts_with(prefix))
    };
    if !diff.lines().any(directive) {
        return Ok(None);
    }

    let diff = diff
        .strip_suffix("\r\n")
        .or_else(|| diff.strip_suffix('\n'))
        .unwrap_or(diff);
    let hunk_header = |line: &str| line == "@@" || line.starts_with("@@ ");
    if let Some(body) = diff.strip_suffix(END).filter(|body| body.ends_with('\n')) {
        if body.lines().next().is_some_and(hunk_header)
            && body.lines().all(|line| {
                hunk_header(line)
                    || (line.starts_with([' ', '+', '-'])
                        && !line.starts_with("---")
                        && !line.starts_with("+++"))
            })
        {
            anyhow::ensure!(!content.lines().any(|line| line == END),
                "ERROR: ambiguous bare `*** End Patch`. Prefix literal context with a space, or remove the trailer and send only the intended @@ hunks.");
            return Ok(Some(body));
        }
    }
    anyhow::bail!(
        "ERROR: unsupported *** patch framing. Send one path in `path` and only its @@ hunks in `diff`. Prefix literal context with a space."
    )
}

fn whole_line_anchor(content: &str, block: &str) -> bool {
    let Some((before, after)) = content.split_once(block) else {
        return false;
    };
    // The ordinary match count skips overlapping occurrences (e.g. two rows in three identical rows).
    let mut from_match = content[before.len()..].chars();
    from_match.next();
    !block.trim().is_empty()
        && (before.is_empty() || before.ends_with('\n'))
        && (after.is_empty() || after.starts_with('\n') || after.starts_with("\r\n"))
        && !from_match.as_str().contains(block)
}
