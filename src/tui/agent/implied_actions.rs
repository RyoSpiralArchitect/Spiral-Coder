//! A named fence contains file data. Only unnamed command blocks can execute.
use super::{ImpliedExecScript, ImpliedWriteFile};

fn is_shell_fence_lang(lang_hint: &str) -> bool {
    matches!(
        lang_hint,
        "bash" | "sh" | "shell" | "zsh" | "powershell" | "pwsh" | "ps1" | "ps" | "console"
    )
}

fn looks_like_shell_command(script: &str) -> bool {
    let low = script.to_ascii_lowercase();
    let pats = [
        // PowerShell
        "new-item",
        "set-content",
        "add-content",
        "remove-item",
        "copy-item",
        "move-item",
        "get-content",
        "test-path",
        // Common CLIs
        "\ngit ",
        "git ",
        "\ncargo ",
        "cargo ",
        "\npython",
        "python ",
        "\nnode ",
        "node ",
        "\nnpm ",
        "npm ",
        "\npnpm ",
        "pnpm ",
        "\nyarn ",
        "yarn ",
        // Shell-ish
        "\ncd ",
        "\nmkdir",
        "mkdir ",
    ];
    pats.iter().any(|p| low.contains(p))
}

fn sanitize_implied_path_candidate(raw: &str) -> Option<String> {
    let mut p = raw.trim().to_string();
    if p.is_empty() {
        return None;
    }

    // Strip common wrappers.
    if p.len() >= 2
        && ((p.starts_with('"') && p.ends_with('"')) || (p.starts_with('\'') && p.ends_with('\'')))
    {
        p = p[1..p.len().saturating_sub(1)].to_string();
    }
    if p.starts_with('`') && p.ends_with('`') && p.len() >= 2 {
        p = p[1..p.len().saturating_sub(1)].to_string();
    }
    p = p.trim().to_string();
    if p.is_empty() {
        return None;
    }
    if let Some(rest) = p.strip_prefix("./") {
        p = rest.to_string();
    }
    if p.ends_with(':') {
        p.pop();
        p = p.trim().to_string();
    }

    // Avoid obvious false positives and dangerous strings.
    if p.len() > 240 {
        return None;
    }
    let low = p.to_ascii_lowercase();
    if low.starts_with("http://") || low.starts_with("https://") {
        return None;
    }
    if p.contains("```") || p.contains("<plan>") || p.contains("</plan>") {
        return None;
    }
    // Avoid Windows-forbidden characters and other obvious junk.
    if p.chars()
        .any(|c| matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*'))
    {
        return None;
    }
    // Keep it simple: paths with spaces are too ambiguous in free-form text.
    if p.chars().any(|c| c.is_whitespace()) {
        return None;
    }

    // Must look like a file path, not a version number / heading.
    let has_sep = p.contains('/') || p.contains('\\');
    let has_dot = p.contains('.');
    if !has_sep && !has_dot {
        // Common "no extension" files.
        let low2 = p.to_ascii_lowercase();
        let ok = matches!(
            low2.as_str(),
            "license" | "makefile" | "dockerfile" | "readme"
        );
        if !ok {
            return None;
        }
    } else if has_dot && !has_sep {
        // Reject things like "v0.2.0" (last segment has no alphabetic chars).
        if let Some((_, ext)) = p.rsplit_once('.') {
            let ext = ext.trim();
            if ext.is_empty() {
                return None;
            }
            let has_alpha = ext.chars().any(|c| c.is_ascii_alphabetic());
            if !has_alpha {
                return None;
            }
        }
    }
    Some(p)
}

enum Target {
    File(String),
    InvalidFile,
}

fn parse_implied_path_line(line: &str) -> Option<Target> {
    let s = line.trim();
    if s.is_empty() {
        return None;
    }

    // Examples:
    // - File: src/main.rs
    // - Path: README.md
    // - ### src/main.rs
    // - src/main.rs
    let s = s.strip_prefix("- ").unwrap_or(s);
    let s = s.strip_prefix("* ").unwrap_or(s);

    // "File: ..."
    let low = s.to_ascii_lowercase();
    for key in ["file:", "path:", "filepath:", "filename:"] {
        if let Some(rest) = low.strip_prefix(key) {
            // Use the original string slice to preserve case.
            let orig_rest = s[s.len().saturating_sub(rest.len())..].trim();
            return Some(
                sanitize_implied_path_candidate(orig_rest)
                    .map(Target::File)
                    .unwrap_or(Target::InvalidFile),
            );
        }
    }

    // Markdown headers: ### path
    if s.starts_with('#') {
        let rest = s.trim_start_matches('#').trim();
        return sanitize_implied_path_candidate(rest).map(Target::File);
    }

    None
}

pub(super) fn extract(text: &str) -> (Vec<ImpliedExecScript>, Vec<ImpliedWriteFile>) {
    let mut scripts = Vec::new();
    let mut files = Vec::new();
    let mut prompt_lines = Vec::new();
    let mut target = None;
    let mut language: Option<String> = None;
    let mut lines = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim_start();
        if let Some(marker) = trimmed.strip_prefix("```") {
            let Some(lang) = language.take() else {
                language = Some(marker.trim().to_ascii_lowercase());
                continue;
            };
            let body = lines.join("\n");
            lines.clear();
            let file = target.take();
            if body.trim().is_empty() {
                continue;
            }
            match file {
                Some(Target::File(path)) if !matches!(lang.as_str(), "diff" | "patch") => {
                    files.push(ImpliedWriteFile {
                        path,
                        lang_hint: lang,
                        content: body.trim_end().to_owned(),
                    });
                }
                None if scripts.len() < 3
                    && (is_shell_fence_lang(&lang) || looks_like_shell_command(&body)) =>
                {
                    scripts.push(ImpliedExecScript {
                        lang_hint: lang,
                        script: body.trim().to_owned(),
                    });
                }
                _ => {}
            }
        } else if language.is_some() {
            lines.push(line);
        } else if let Some(file) = parse_implied_path_line(line) {
            target = Some(file);
        } else if prompt_lines.len() < 24
            && ["PS>", "$ ", "> "]
                .iter()
                .any(|prefix| trimmed.starts_with(prefix))
        {
            prompt_lines.push(trimmed);
        }
    }
    if scripts.is_empty() {
        let joined = prompt_lines.join("\n");
        if looks_like_shell_command(&joined) {
            scripts.push(ImpliedExecScript {
                lang_hint: "powershell".into(),
                script: joined.trim().to_owned(),
            });
        }
    }
    (scripts, files)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_shell_fences_never_execute_and_every_named_target_reaches_scope_validation() {
        let mut text = (1..=6)
            .map(|i| format!("File: src/good{i}.rs\n```rust\nfn f() {{}}\n```\n"))
            .collect::<String>();
        text.push_str("File: scratch.sh\n```bash\n$ git init\necho edited > protected.txt\n```\n");
        let (scripts, files) = extract(&text);
        assert!(scripts.is_empty());
        assert_eq!(files.len(), 7);
        assert!(super::super::explicit_edit_scope::implied_write_issue(
            &format!(
                "Allowed paths only:\n{}",
                (1..=6)
                    .map(|i| format!("- src/good{i}.rs\n"))
                    .collect::<String>()
            ),
            &files,
            None
        )
        .unwrap()
        .contains("scratch.sh"));
        text.push_str("File: invalid name.sh\n```sh\ngit init never\n```\n```sh\npwd\n```");
        let (scripts, _) = extract(&text);
        assert_eq!(scripts.len(), 1);
        assert_eq!(scripts[0].script, "pwd");
    }
}
