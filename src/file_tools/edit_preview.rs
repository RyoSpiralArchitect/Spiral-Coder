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
    let hunks = parse_diff_hunks(diff);
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
        } else {
            new_content = new_content.replacen(&old_block, &new_block, 1);
            applied += 1;
        }
    }

    if applied == 0 {
        return Err(anyhow!(
            "ERROR: no hunks applied ({}).\n{}\nTip: call read_file to inspect exact content.",
            errors.join("; "),
            diagnostic_contexts.join("\n")
        ));
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

    Ok((
        new_content,
        format!(
            "OK: applied {applied}/{} hunk(s) to '{path}'{warn}",
            hunks.len()
        ),
    ))
}
