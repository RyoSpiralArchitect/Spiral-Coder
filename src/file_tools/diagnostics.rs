//! Bounded, display-only context for rejected edit anchors.
//!
//! Similarity ranks excerpts only. It must never select text to replace.

const MAX_LINE_CHARS: usize = 140;
const CONTEXT_LINES: usize = 6;
const MAX_MATCHES: usize = 3;
const DISPLAY_GUIDE: &str =
    "Line numbers and truncation markers are display-only; do not copy them into search text.\n";

pub(super) fn missing_anchor(content: &str, search: &str) -> String {
    let lines: Vec<_> = content.lines().collect();
    if lines.is_empty() {
        return "Current file is empty.\n".to_string();
    }
    // Punctuation-only anchors carry no useful location information. In
    // particular, showing the prefix for a missing closing block hides its tail.
    let needles: Vec<_> = search
        .lines()
        .map(str::trim)
        .filter(|line| line.chars().any(char::is_alphanumeric))
        .take(16)
        .map(|line| line.chars().take(256).collect::<String>())
        .collect();
    let mut closest = None;
    let mut best_score = 0;
    for (index, line) in lines.iter().enumerate() {
        let line: String = line.trim().chars().take(256).collect();
        let score = needles
            .iter()
            .map(|needle| similarity(&line, needle))
            .max()
            .unwrap_or(0);
        if score > best_score {
            best_score = score;
            closest = Some(index);
        }
    }

    let mut output = String::new();
    let mut shown_end = 0;
    if let Some(index) = closest {
        let start = index.saturating_sub(2);
        shown_end = (start + CONTEXT_LINES).min(lines.len());
        append_window(
            &mut output,
            "Closest anchor context (diagnostic only)",
            &lines,
            start,
            shown_end,
        );
    }
    let tail_start = lines.len().saturating_sub(CONTEXT_LINES);
    if shown_end < lines.len() {
        append_window(
            &mut output,
            "Current file tail",
            &lines,
            tail_start.max(shown_end),
            lines.len(),
        );
    }
    if !content.ends_with('\n') {
        output.push_str("[No final newline]\n");
    }
    output.push_str(DISPLAY_GUIDE);
    output
}

pub(super) fn ambiguous_anchor(content: &str, search: &str) -> String {
    let lines: Vec<_> = content.lines().collect();
    let mut output = String::new();
    for (ordinal, (offset, _)) in content.match_indices(search).take(MAX_MATCHES).enumerate() {
        let index = content[..offset]
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count();
        append_window(
            &mut output,
            &format!("Match {} at line {}", ordinal + 1, index + 1),
            &lines,
            index.saturating_sub(1),
            (index + 2).min(lines.len()),
        );
    }
    output.push_str(DISPLAY_GUIDE);
    output
}

fn similarity(line: &str, needle: &str) -> usize {
    if line == needle {
        return 1_000 + needle.chars().count();
    }
    needle
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|token| token.chars().count() >= 3)
        .filter(|token| {
            line.split(|c: char| !c.is_alphanumeric() && c != '_')
                .any(|word| word == *token)
        })
        .map(str::len)
        .sum()
}

fn append_window(output: &mut String, label: &str, lines: &[&str], start: usize, end: usize) {
    output.push_str(&format!("{label} (lines {}-{end}):\n", start + 1));
    for (index, line) in lines.iter().enumerate().take(end).skip(start) {
        let mut chars = line.chars();
        let excerpt: String = chars.by_ref().take(MAX_LINE_CHARS).collect();
        let suffix = if chars.next().is_some() {
            "… [line truncated]"
        } else {
            ""
        };
        output.push_str(&format!("{} | {excerpt}{suffix}\n", index + 1));
    }
}
