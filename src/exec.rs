use anyhow::anyhow;
use anyhow::{Context, Result};
use std::path::{Component, Path};
use std::process::Stdio;
use tokio::process::Command;

#[cfg(target_os = "windows")]
#[link(name = "kernel32")]
extern "system" {
    fn MultiByteToWideChar(
        CodePage: u32,
        dwFlags: u32,
        lpMultiByteStr: *const i8,
        cbMultiByte: i32,
        lpWideCharStr: *mut u16,
        cchWideChar: i32,
    ) -> i32;
}

pub struct ExecResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

/// Validate `cwd` passed into `run_command`.
///
/// We keep this conservative:
/// - Reject any path that contains `..` components (even if absolute).
/// - Reject "absolute-ish" Windows prefixes when the path is not absolute (e.g. `C:foo`).
pub fn validate_cwd(cwd: &str) -> Result<()> {
    let s = cwd.trim();
    if s.is_empty() {
        return Ok(());
    }
    if s.contains('\0') {
        return Err(anyhow!("cwd contains NUL byte"));
    }

    let p = Path::new(s);
    if p.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(anyhow!("cwd traversal ('..') not allowed: {s}"));
    }

    // On Windows, "C:foo" is NOT absolute but has a Prefix component.
    // Treat this as unsafe because it's ambiguous and can escape expected roots.
    if !p.is_absolute() {
        for c in p.components() {
            match c {
                Component::CurDir | Component::Normal(_) => {}
                Component::ParentDir => unreachable!("handled above"),
                Component::RootDir | Component::Prefix(_) => {
                    return Err(anyhow!("cwd must be a normal relative path: {s}"));
                }
            }
        }
    }

    Ok(())
}

/// Best-effort cleanup for LLM-produced shell transcripts.
///
/// Common failure modes:
/// - Prompt markers accidentally included in the command: `$ ...`, `PS> ...`, `> ...`
/// - Stray trailing braces when a model mixes tool-call syntax into a shell line.
fn sanitize_shellish_command(cmd: &str) -> String {
    let raw = cmd.replace("\r\n", "\n").trim().to_string();
    if raw.is_empty() {
        return String::new();
    }

    // If the input contains shell prompt markers, treat it as a transcript and keep ONLY
    // the prompt lines. This prevents accidentally executing copied stdout/stderr (e.g.
    // `Initialized empty Git repository...`) which is a common LLM failure mode.
    fn is_prompt_line(s: &str) -> bool {
        let t = s.trim_start();
        if t.starts_with("PS>") {
            return true;
        }
        if t.starts_with("PS ") {
            if let Some(idx) = t.find('>') {
                // "PS C:\path>" or "PS C:\path> cmd"
                let after = &t[idx + 1..];
                return after.is_empty() || after.chars().next().is_some_and(|c| c.is_whitespace());
            }
        }
        if t.starts_with('>') {
            return t.chars().nth(1).is_some_and(|c| c.is_whitespace());
        }
        if t.starts_with('$') {
            return t.chars().nth(1).is_some_and(|c| c.is_whitespace());
        }
        false
    }

    fn strip_prompt(s: &str) -> String {
        let t = s.trim_start();
        if let Some(rest) = t.strip_prefix("PS>") {
            return rest.trim_start().to_string();
        }
        if t.starts_with("PS ") {
            if let Some(idx) = t.find('>') {
                return t[idx + 1..].trim_start().to_string();
            }
        }
        if t.starts_with('>') && t.chars().nth(1).is_some_and(|c| c.is_whitespace()) {
            return t[1..].trim_start().to_string();
        }
        if t.starts_with('$') && t.chars().nth(1).is_some_and(|c| c.is_whitespace()) {
            return t[1..].trim_start().to_string();
        }
        s.trim().to_string()
    }

    fn looks_like_output_line_no_prompt(s: &str) -> bool {
        let t = s.trim();
        if t.is_empty() {
            return true;
        }
        let low = t.to_ascii_lowercase();

        if low.starts_with("stdout:") || low.starts_with("stderr:") {
            return true;
        }
        if low.starts_with("exit") {
            // "exit 1" / "exit: 0"
            if low == "exit" {
                return true;
            }
            if low.starts_with("exit ") || low.starts_with("exit:") {
                return true;
            }
        }
        if low.starts_with("fatal:")
            || low.starts_with("error:")
            || low.starts_with("warning:")
            || low.starts_with("hint:")
        {
            return true;
        }
        if low.starts_with("initialized empty git repository")
            || low.starts_with("on branch")
            || low.starts_with("your branch")
            || low.starts_with("changes to be committed:")
            || low.starts_with("untracked files:")
            || low.starts_with("nothing to commit")
        {
            return true;
        }
        if low.starts_with("directory:") {
            return true;
        }
        // Japanese "ディレクトリ:"
        if t.starts_with("ディレクトリ") {
            return true;
        }
        if low.starts_with("mode ") && low.contains("lastwritetime") {
            return true;
        }
        if low.starts_with("----") {
            return true;
        }
        if low.starts_with("modified:")
            || low.starts_with("new file:")
            || low.starts_with("deleted:")
        {
            return true;
        }

        false
    }

    let has_prompt = raw.lines().any(is_prompt_line);

    let mut out: Vec<String> = Vec::new();
    for ln0 in raw.split('\n') {
        let ln0 = ln0.trim_end_matches('\r');
        if has_prompt && !is_prompt_line(ln0) {
            continue; // drop output lines
        }

        let mut ln = if has_prompt {
            strip_prompt(ln0)
        } else {
            ln0.to_string()
        };
        let ltrim = ln.trim_start();

        // PowerShell prompt markers.
        if let Some(rest) = ltrim.strip_prefix("PS> ") {
            ln = rest.to_string();
        } else if let Some(rest) = ltrim.strip_prefix("PS>") {
            // Handle "PS>cmd" / "PS>cmd args"
            ln = rest.trim_start().to_string();
        } else if let Some(rest) = ltrim.strip_prefix("> ") {
            ln = rest.to_string();
        } else if let Some(rest) = ltrim.strip_prefix("$ ") {
            ln = rest.to_string();
        } else {
            // Strip "$" prompt with multiple spaces: "$   git status"
            let lt = ltrim;
            if lt.starts_with('$') && lt[1..].starts_with(char::is_whitespace) {
                ln = lt.trim_start_matches('$').trim_start().to_string();
            }
        }

        // When we don't have explicit prompts, drop obvious tool output lines that models
        // sometimes paste into code fences.
        if !has_prompt && looks_like_output_line_no_prompt(&ln) {
            continue;
        }

        // Some models leak tool-call syntax into code fences (e.g. "assistant to=...").
        // Strip anything after the first known marker to keep the command runnable.
        let lower = ln.to_ascii_lowercase();
        let noise_tokens = [
            "assistant to=",
            "to=multi_tool_use.",
            "to=functions.",
            "to=web.run",
            "recipient_name",
            "parameters:",
        ];
        let mut cut_at: Option<usize> = None;
        for tok in &noise_tokens {
            if let Some(idx) = lower.find(tok) {
                cut_at = Some(cut_at.map_or(idx, |c| c.min(idx)));
            }
        }
        if let Some(idx) = cut_at {
            ln.truncate(idx);
            ln = ln.trim_end().to_string();
        }

        if ln.trim().is_empty() {
            continue;
        }
        out.push(ln);
    }

    let mut s = out.join("\n").trim().to_string();

    // Strip a leading "$" without a following space: "$git status"
    let t = s.trim_start();
    if let Some(rest) = t.strip_prefix('$') {
        if rest.chars().next().is_some_and(|c| !c.is_whitespace()) {
            s = rest.trim_start().to_string();
        }
    }

    // Strip trailing unmatched "}" / "]" (common artifacts).
    // Models sometimes emit "}}]}" or similar when mixing tool syntax into a shell line.
    loop {
        let t = s.trim_end();
        if t.is_empty() {
            s = t.to_string();
            break;
        }

        let mut changed = false;
        if t.ends_with('}') && t.matches('{').count() < t.matches('}').count() {
            s = t[..t.len() - 1].trim_end().to_string();
            changed = true;
        } else if t.ends_with(']') && t.matches('[').count() < t.matches(']').count() {
            s = t[..t.len() - 1].trim_end().to_string();
            changed = true;
        }

        if !changed {
            break;
        }
    }

    s
}

#[cfg(any(target_os = "windows", test))]
fn split_args_simple(s: &str) -> Vec<String> {
    // Minimal tokeniser good enough for short command snippets.
    // Supports single and double quotes. In double quotes, \" and \\ are handled.
    let src = s.trim();
    if src.is_empty() {
        return Vec::new();
    }

    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut q: Option<char> = None;
    let mut chars = src.chars().peekable();

    while let Some(ch) = chars.next() {
        if let Some(qq) = q {
            if ch == qq {
                q = None;
                continue;
            }
            if qq == '"' && ch == '\\' {
                if let Some(n) = chars.next() {
                    cur.push(n);
                    continue;
                }
            }
            cur.push(ch);
            continue;
        }

        if ch == '\'' || ch == '"' {
            q = Some(ch);
            continue;
        }

        if ch.is_whitespace() {
            if !cur.is_empty() {
                out.push(cur.clone());
                cur.clear();
            }
            continue;
        }

        cur.push(ch);
    }

    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

#[cfg(any(target_os = "windows", test))]
fn ps_single_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

#[cfg(target_os = "windows")]
fn looks_bashish_windows_script(s: &str) -> bool {
    let x = s.to_ascii_lowercase();
    // Windows PowerShell 5.x doesn't support `&&`.
    if x.contains("&&") {
        return true;
    }
    // Common bash snippets produced by models.
    if x.contains("mkdir -p") || x.contains("mkdir --parents") {
        return true;
    }
    if x.contains("\ntouch ")
        || x.starts_with("touch ")
        || x.contains("; touch ")
        || x.contains("&& touch ")
    {
        return true;
    }
    if x.contains("rm -rf") || x.contains("rm -r ") || x.contains("rm -r\t") {
        return true;
    }
    if x.contains("\nexport ") || x.starts_with("export ") || x.contains("; export ") {
        return true;
    }
    false
}

#[cfg(target_os = "windows")]
fn bash_to_powershell_script(script: &str) -> String {
    // Port of the UI-side translator: convert common bash commands into a
    // Windows PowerShell script, so models can paste bash snippets and still run.
    let raw = script.replace("\r\n", "\n");
    if raw.trim().is_empty() {
        return String::new();
    }

    let mut out: Vec<String> = Vec::new();
    out.push("$ErrorActionPreference = 'Stop'".to_string());

    for line0 in raw.split('\n') {
        let mut line = line0.trim().to_string();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('#') {
            continue;
        }

        // Common model glitch: trailing brace (e.g. `New-Item ... }`).
        // Do NOT drop a standalone closing brace line (`}`): wrappers may use try/finally blocks.
        if line.len() > 1 && line.ends_with('}') && !line.contains('{') {
            line.pop();
            line = line.trim().to_string();
        }

        // Replace `&&` (unsupported in Windows PowerShell 5.x) with `;`
        // then split into segments (naive, but good enough for typical snippets).
        let line = line.replace("&&", ";");
        for seg0 in line.split(';') {
            let seg = seg0.trim();
            if seg.is_empty() {
                continue;
            }

            let toks = split_args_simple(seg);
            if toks.is_empty() {
                continue;
            }
            let head = toks[0].as_str();

            if head == "mkdir" {
                let mut parents = false;
                let mut paths: Vec<String> = Vec::new();
                for t in toks.iter().skip(1) {
                    if t == "-p" || t == "--parents" {
                        parents = true;
                        continue;
                    }
                    if t.starts_with('-') {
                        continue;
                    }
                    paths.push(t.to_string());
                }
                if parents {
                    for p in paths {
                        out.push(format!(
                            "[System.IO.Directory]::CreateDirectory({}) | Out-Null",
                            ps_single_quote(&p)
                        ));
                    }
                } else {
                    // `mkdir` is an alias in PowerShell; keep it if there's no -p.
                    out.push(seg.to_string());
                }
                continue;
            }

            if head == "touch" {
                let mut paths: Vec<String> = Vec::new();
                for t in toks.iter().skip(1) {
                    if t.starts_with('-') {
                        continue;
                    }
                    paths.push(t.to_string());
                }
                for p in paths {
                    let q = ps_single_quote(&p);
                    out.push(format!(
                        "if (-not (Test-Path -LiteralPath {q})) {{ New-Item -ItemType File -Force -Path {q} | Out-Null }} else {{ (Get-Item -LiteralPath {q}).LastWriteTime = Get-Date }}"
                    ));
                }
                continue;
            }

            if head == "rm" {
                let mut recurse = false;
                let mut paths: Vec<String> = Vec::new();
                for t in toks.iter().skip(1) {
                    if t.starts_with('-') {
                        if t.contains('r') || t.contains('R') {
                            recurse = true;
                        }
                        continue;
                    }
                    paths.push(t.to_string());
                }
                for p in paths {
                    let q = ps_single_quote(&p);
                    out.push(format!(
                        "Remove-Item -Force {}-ErrorAction SilentlyContinue -LiteralPath {}",
                        if recurse { "-Recurse " } else { "" },
                        q
                    ));
                }
                continue;
            }

            if head == "cp" && toks.len() >= 3 {
                out.push(format!(
                    "Copy-Item -Force -LiteralPath {} -Destination {}",
                    ps_single_quote(&toks[1]),
                    ps_single_quote(&toks[2])
                ));
                continue;
            }

            if head == "mv" && toks.len() >= 3 {
                out.push(format!(
                    "Move-Item -Force -LiteralPath {} -Destination {}",
                    ps_single_quote(&toks[1]),
                    ps_single_quote(&toks[2])
                ));
                continue;
            }

            if head == "export" && toks.len() >= 2 {
                let kv = &toks[1];
                if let Some(idx) = kv.find('=') {
                    let k = kv[..idx].to_string();
                    let v = kv[idx + 1..].to_string();
                    if !k.trim().is_empty() {
                        out.push(format!("$env:{k} = {}", ps_single_quote(&v)));
                        continue;
                    }
                }
            }

            out.push(seg.to_string());
        }
    }

    out.join("\n").trim().to_string()
}

fn is_poison_proxy(v: &str) -> bool {
    let s = v.trim().to_ascii_lowercase();
    // Known-bad proxy setting: forces all HTTPS traffic into a dead local proxy.
    // Example from Git: "port 443 via 127.0.0.1 ... Could not connect to server"
    let prefixes = [
        "http://127.0.0.1:9",
        "https://127.0.0.1:9",
        "http://localhost:9",
        "https://localhost:9",
    ];
    prefixes.iter().any(|p| s.starts_with(p))
}

fn scrub_poison_proxy_env(cmd: &mut Command) {
    for k in [
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "ALL_PROXY",
        "GIT_HTTP_PROXY",
        "GIT_HTTPS_PROXY",
        "http_proxy",
        "https_proxy",
        "all_proxy",
    ] {
        if let Ok(v) = std::env::var(k) {
            if !v.trim().is_empty() && is_poison_proxy(&v) {
                cmd.env_remove(k);
            }
        }
    }
}

/// Check whether a command matches the dangerous-command blocklist.
/// Returns `Some(reason)` if blocked, `None` if safe to run.
pub fn check_dangerous_command(cmd: &str) -> Option<&'static str> {
    let s = cmd.trim().to_ascii_lowercase();

    // Git destructive patterns (cross-platform).
    let git_dangerous = [
        (
            "git reset --hard",
            "git reset --hard discards local changes",
        ),
        (
            "git clean -fd",
            "git clean -fd removes untracked files/dirs",
        ),
        ("git clean -xdf", "git clean -xdf removes ignored files too"),
    ];
    for (pat, reason) in &git_dangerous {
        if s.contains(pat) {
            return Some(reason);
        }
    }
    // Block "remove everything from index" variants.
    // Examples:
    // - git rm --cached -r .
    // - git rm -r --cached .
    // - git rm --cached -r ./
    if s.contains("git rm")
        && (s.contains("--cached") || s.contains("--cache"))
        && (s.contains(" -r") || s.contains("--recursive"))
        && (s.ends_with(" .") || s.ends_with(" ./") || s.contains(" . "))
    {
        return Some("git rm --cached -r . would remove the entire repo from the index");
    }

    // Unix destructive patterns
    let unix_dangerous = [
        ("rm -rf /", "rm -rf / would erase the entire filesystem"),
        ("rm -rf /*", "rm -rf /* would erase the entire filesystem"),
        ("rm -rf ~", "rm -rf ~ would erase the home directory"),
        ("> /dev/sda", "writing to raw disk device"),
        ("dd if=", "dd writes raw bytes to block devices"),
        ("mkfs.", "mkfs formats a filesystem partition"),
        (":(){ :|:& };:", "fork bomb"),
        ("shutdown", "shutdown/reboot command"),
        ("halt", "halt command"),
        ("reboot", "reboot command"),
        ("chmod -r 000 /", "removing all permissions from root"),
        ("chown -r root /", "changing ownership of root"),
    ];
    for (pat, reason) in &unix_dangerous {
        if s.contains(pat) {
            return Some(reason);
        }
    }

    // Windows destructive patterns
    // Note: OpenSSH / PowerShell variants can permute arg order; keep these checks flexible.
    if s.contains("remove-item") && s.contains("-recurse") {
        if s.contains("c:\\") || s.contains("c:/") {
            return Some("recursive remove of C: drive");
        }
    }

    let win_dangerous = [
        ("format ", "format command can erase drives"),
        ("del /s /q c:\\", "recursive delete of C: drive"),
        ("del /f /s /q c:\\", "recursive delete of C: drive"),
        ("rd /s /q c:\\", "recursive remove of C: drive"),
        (
            "remove-item -recurse -force c:\\",
            "recursive remove of C: drive",
        ),
        ("remove-item -recurse c:\\", "recursive remove of C: drive"),
        ("stop-computer", "stop-computer shuts down the machine"),
        ("restart-computer", "restart-computer reboots the machine"),
        ("disable-computerrestore", "disabling system restore"),
        ("clear-disk", "clear-disk erases disk contents"),
        ("initialize-disk", "initialize-disk reformats a disk"),
    ];
    for (pat, reason) in &win_dangerous {
        if s.contains(pat) {
            return Some(reason);
        }
    }

    // Repo-destructive git patterns (avoid self-sabotage in agentic runs).
    let git_dangerous = [
        (
            "git reset --hard",
            "git reset --hard discards local changes",
        ),
        (
            "git clean -fdx",
            "git clean -fdx deletes untracked files/directories",
        ),
        (
            "git rm --cached -r .",
            "git rm --cached -r . can remove the entire repo from the index",
        ),
        (
            "git rm -r --cached .",
            "git rm -r --cached . can remove the entire repo from the index",
        ),
    ];
    for (pat, reason) in &git_dangerous {
        if s.contains(pat) {
            return Some(reason);
        }
    }

    None
}

fn is_git_add_all_command(cmd: &str) -> bool {
    let s = cmd.trim().to_ascii_lowercase();
    if s.is_empty() {
        return false;
    }
    let toks: Vec<&str> = s.split_whitespace().collect();
    if toks.is_empty() || !toks.iter().any(|t| *t == "git") {
        return false;
    }
    // Support: `git add .`, `git add -A`, `git -C path add .`, etc.
    let idx_add = toks.iter().position(|t| *t == "add");
    let Some(i) = idx_add else {
        return false;
    };
    let rest = &toks[i + 1..];
    if rest.is_empty() {
        return false;
    }

    // `git add -A` becomes `-a` after lowercasing; treat it as add-all.
    if rest.iter().any(|t| *t == "-a" || *t == "--all") {
        return true;
    }

    // `git add .` / `./` / `.\`
    rest.iter().any(|t| *t == "." || *t == "./" || *t == ".\\")
}

fn resolve_cwd_path(cwd: Option<&str>) -> Option<std::path::PathBuf> {
    let p = cwd.unwrap_or("").trim();
    let base = if p.is_empty() {
        std::env::current_dir().ok()?
    } else {
        let pb = std::path::PathBuf::from(p);
        if pb.is_absolute() {
            pb
        } else {
            std::env::current_dir().ok()?.join(pb)
        }
    };
    Some(base)
}

fn find_git_root(mut start: &Path) -> Option<std::path::PathBuf> {
    // Walk up a few levels; we only need this to catch `git add` invoked from a subdir.
    for _ in 0..12 {
        let git = start.join(".git");
        if git.is_dir() {
            return Some(start.to_path_buf());
        }
        start = start.parent()?;
    }
    None
}

fn find_nested_git_dirs(repo_root: &Path) -> Vec<std::path::PathBuf> {
    // Conservative scan: shallow, with skip lists to avoid huge trees.
    let max_depth: usize = 4;
    let max_hits: usize = 4;
    let mut hits: Vec<std::path::PathBuf> = Vec::new();

    let mut stack: Vec<(std::path::PathBuf, usize)> = vec![(repo_root.to_path_buf(), 0)];
    while let Some((dir, depth)) = stack.pop() {
        if depth > max_depth {
            continue;
        }
        let rd = match std::fs::read_dir(&dir) {
            Ok(r) => r,
            Err(_) => continue,
        };
        for ent in rd.flatten() {
            let ft = match ent.file_type() {
                Ok(t) => t,
                Err(_) => continue,
            };
            if !ft.is_dir() {
                continue;
            }
            let name_os = ent.file_name();
            let name = name_os.to_string_lossy();
            let name_s = name.as_ref();

            // Skip dot dirs and common heavy dirs.
            if name_s.starts_with('.') {
                if name_s != ".git" {
                    continue;
                }
            }
            if matches!(
                name_s,
                "node_modules" | "target" | ".tmp" | ".venv" | "dist" | "build" | "out"
            ) {
                continue;
            }

            let p = ent.path();
            if name_s == ".git" {
                // Skip the root .git itself; everything else is a nested repo.
                if p == repo_root.join(".git") {
                    continue;
                }
                hits.push(p);
                if hits.len() >= max_hits {
                    return hits;
                }
                continue;
            }

            if depth < max_depth {
                stack.push((p, depth + 1));
            }
        }
    }

    hits
}

fn check_nested_git_add_all_preflight(cmd: &str, cwd: Option<&str>) -> Option<String> {
    if !is_git_add_all_command(cmd) {
        return None;
    }
    let base = resolve_cwd_path(cwd)?;
    let Some(repo_root) = find_git_root(&base) else {
        return None;
    };
    let nested = find_nested_git_dirs(&repo_root);
    if nested.is_empty() {
        return None;
    }

    let mut rels: Vec<String> = Vec::new();
    for p in nested.into_iter().take(3) {
        let rel = p
            .strip_prefix(&repo_root)
            .ok()
            .map(|x| x.to_string_lossy().to_string())
            .unwrap_or_else(|| p.to_string_lossy().to_string());
        rels.push(rel);
    }
    Some(format!(
        "git add-all detected nested .git dirs under repo root: {}. Fix: cd into the intended repo, or move the project outside the parent repo (recommended), or use a submodule.",
        rels.join(", ")
    ))
}

/// Run a local shell command and return its combined output.
///
/// On Windows, PowerShell is used. Here-strings (`@'...'@` / `@"..."@`) are
/// handled by writing a temp `.ps1` file and invoking with `-File`, which avoids
/// the column-0 terminator constraint when passing via `-Command`.
pub async fn run_command(command: &str, cwd: Option<&str>) -> Result<ExecResult> {
    let cleaned = sanitize_shellish_command(command);
    if cleaned.trim().is_empty() {
        return Ok(ExecResult {
            stdout: String::new(),
            stderr: String::new(),
            exit_code: 0,
        });
    }

    if let Some(reason) = check_dangerous_command(&cleaned) {
        return Ok(ExecResult {
            stdout: String::new(),
            stderr: format!("[BLOCKED] dangerous command: {reason}"),
            exit_code: -1,
        });
    }

    // Windows: many models paste bash snippets. Translate common bash commands into PowerShell
    // so the tool works reliably in a PowerShell-only runtime.
    #[cfg(target_os = "windows")]
    let cleaned = if looks_bashish_windows_script(&cleaned) {
        bash_to_powershell_script(&cleaned)
    } else {
        cleaned
    };

    if let Some(reason) = check_dangerous_command(&cleaned) {
        return Ok(ExecResult {
            stdout: String::new(),
            stderr: format!("[BLOCKED] dangerous command: {reason}"),
            exit_code: -1,
        });
    }

    if let Some(reason) = check_nested_git_add_all_preflight(&cleaned, cwd) {
        return Ok(ExecResult {
            stdout: String::new(),
            stderr: format!("[BLOCKED] {reason}"),
            exit_code: -1,
        });
    }

    let cmd_str = cleaned.trim();

    let mut cmd = build_command(cmd_str).await?;
    scrub_poison_proxy_env(&mut cmd);
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    if let Some(cwd) = cwd.filter(|s| !s.trim().is_empty()) {
        validate_cwd(cwd).context("invalid cwd")?;
        cmd.current_dir(cwd);
    }

    let output = cmd.output().await.context("failed to spawn command")?;

    Ok(ExecResult {
        stdout: decode_output(&output.stdout),
        stderr: decode_output(&output.stderr),
        exit_code: output.status.code().unwrap_or(-1),
    })
}

/// Build a `Command` appropriate for the OS, handling Windows here-strings.
async fn build_command(cmd_str: &str) -> Result<Command> {
    if cfg!(target_os = "windows") {
        // Detect here-strings or multi-line scripts that need a temp file.
        let needs_tempfile =
            cmd_str.contains("@'") || cmd_str.contains("@\"") || cmd_str.contains('\n');

        if needs_tempfile {
            let mut tmp = tempfile::Builder::new()
                .prefix("spiral_coder_exec_")
                .suffix(".ps1")
                .tempfile()
                .context("failed to create temp ps1 file")?;
            use std::io::Write;
            writeln!(
                tmp,
                "[Console]::OutputEncoding=[System.Text.Encoding]::UTF8"
            )?;
            writeln!(tmp, "[Console]::InputEncoding=[System.Text.Encoding]::UTF8")?;
            writeln!(tmp, "$OutputEncoding=[System.Text.Encoding]::UTF8")?;
            writeln!(tmp)?;
            write!(tmp, "{}", cmd_str)?;
            let path = tmp.into_temp_path();
            let path_str = path.to_string_lossy().into_owned();
            // Keep temp path alive by leaking — the file is cleaned up at process exit.
            // (tokio::process::Command needs it to exist until `output()` returns.)
            let _ = path.keep();
            let mut c = Command::new("powershell");
            c.args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
                &path_str,
            ]);
            return Ok(c);
        }

        let wrapped = format!(
            "[Console]::OutputEncoding=[System.Text.Encoding]::UTF8; \
             [Console]::InputEncoding=[System.Text.Encoding]::UTF8; \
             $OutputEncoding=[System.Text.Encoding]::UTF8; {}",
            cmd_str
        );
        let mut c = Command::new("powershell");
        c.args(["-NoProfile", "-NonInteractive", "-Command", &wrapped]);
        Ok(c)
    } else {
        fn path_has_executable(name: &str) -> bool {
            let Some(path) = std::env::var_os("PATH") else {
                return false;
            };
            for dir in std::env::split_paths(&path) {
                if dir.as_os_str().is_empty() {
                    continue;
                }
                let cand = dir.join(name);
                if std::fs::metadata(&cand).is_ok_and(|m| m.is_file()) {
                    return true;
                }
            }
            false
        }

        // Prefer bash when available: many LLM-produced scripts use bashisms
        // (`[[ ... ]]`, `set -euo pipefail`, etc.) which break under dash/POSIX sh.
        //
        // Avoid sourcing user profile/rc files: they can `cd` outside tool_root or
        // print banners that pollute tool output. The parent process environment
        // already reflects the user's PATH when started from a terminal.
        if path_has_executable("bash") {
            let mut c = Command::new("bash");
            c.args(["--noprofile", "--norc", "-c", cmd_str]);
            Ok(c)
        } else {
            let mut c = Command::new("sh");
            c.args(["-c", cmd_str]);
            Ok(c)
        }
    }
}

pub fn decode_output(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return String::new();
    }
    #[cfg(target_os = "windows")]
    {
        if let Ok(s) = std::str::from_utf8(bytes) {
            return s.to_string();
        }
        const CP_932: u32 = 932;
        const MB_ERR_INVALID_CHARS: u32 = 0x0000_0008;
        unsafe {
            let src = bytes.as_ptr() as *const i8;
            let src_len = if bytes.len() > i32::MAX as usize {
                i32::MAX
            } else {
                bytes.len() as i32
            };
            let needed = MultiByteToWideChar(
                CP_932,
                MB_ERR_INVALID_CHARS,
                src,
                src_len,
                std::ptr::null_mut(),
                0,
            );
            if needed <= 0 {
                let needed2 = MultiByteToWideChar(CP_932, 0, src, src_len, std::ptr::null_mut(), 0);
                if needed2 <= 0 {
                    return String::from_utf8_lossy(bytes).into_owned();
                }
                let mut buf = vec![0u16; needed2 as usize];
                let written =
                    MultiByteToWideChar(CP_932, 0, src, src_len, buf.as_mut_ptr(), needed2);
                if written <= 0 {
                    return String::from_utf8_lossy(bytes).into_owned();
                }
                buf.truncate(written as usize);
                return String::from_utf16_lossy(&buf);
            }
            let mut buf = vec![0u16; needed as usize];
            let written = MultiByteToWideChar(
                CP_932,
                MB_ERR_INVALID_CHARS,
                src,
                src_len,
                buf.as_mut_ptr(),
                needed,
            );
            if written <= 0 {
                return String::from_utf8_lossy(bytes).into_owned();
            }
            buf.truncate(written as usize);
            String::from_utf16_lossy(&buf)
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        String::from_utf8_lossy(bytes).into_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_strips_prompt_markers() {
        let s = sanitize_shellish_command("$ mkdir -p foo\nPS> cd foo\n> touch a.txt");
        assert!(!s.contains("$ mkdir"));
        assert!(s.contains("mkdir -p foo"));
        assert!(s.contains("cd foo"));
        assert!(s.contains("touch a.txt"));
    }

    #[test]
    fn sanitize_strips_leaked_tool_noise() {
        let s = "New-Item -ItemType Directory -Force -Path 'src/'}}]} assistant to=multi_tool_use.parallel 0";
        let cleaned = sanitize_shellish_command(s);
        assert_eq!(cleaned, "New-Item -ItemType Directory -Force -Path 'src/'");
    }

    #[test]
    fn sanitize_drops_transcript_output_lines() {
        let s = "PS> git init MazeGame\nInitialized empty Git repository in C:/x/.git/\nPS> git status\nOn branch main";
        let cleaned = sanitize_shellish_command(s);
        assert!(cleaned.contains("git init MazeGame"));
        assert!(cleaned.contains("git status"));
        assert!(!cleaned.contains("Initialized empty Git repository"));
        assert!(!cleaned.contains("On branch"));
    }

    #[test]
    fn split_args_simple_handles_quotes() {
        let t = split_args_simple("cp 'a b.txt' \"c d.txt\"");
        assert_eq!(t, vec!["cp", "a b.txt", "c d.txt"]);
    }

    #[test]
    fn ps_single_quote_escapes() {
        assert_eq!(ps_single_quote("a'b"), "'a''b'");
    }

    #[test]
    fn validate_cwd_allows_normal_paths() {
        validate_cwd("").unwrap();
        validate_cwd("subdir").unwrap();
        validate_cwd("a/b").unwrap();
    }

    #[test]
    fn validate_cwd_rejects_dotdot() {
        assert!(validate_cwd("../x").is_err());
        assert!(validate_cwd("a/../b").is_err());
    }

    #[test]
    fn validate_cwd_rejects_absolute_with_dotdot() {
        let p = std::env::temp_dir().join("..").join("x");
        let s = p.to_string_lossy().into_owned();
        assert!(validate_cwd(&s).is_err());
    }

    #[test]
    fn nested_git_preflight_blocks_git_add_all() {
        let td = tempfile::tempdir().expect("tempdir");
        let root = td.path();
        std::fs::create_dir_all(root.join(".git")).expect("mk .git");
        std::fs::create_dir_all(root.join("MazeGame").join(".git")).expect("mk nested .git");

        let reason = check_nested_git_add_all_preflight("git add .", Some(&root.to_string_lossy()));
        assert!(reason.is_some());
        assert!(reason.unwrap().to_lowercase().contains("mazegame"));
    }

    #[test]
    fn nested_git_preflight_allows_git_add_all_when_no_nested_repo() {
        let td = tempfile::tempdir().expect("tempdir");
        let root = td.path();
        std::fs::create_dir_all(root.join(".git")).expect("mk .git");

        let reason =
            check_nested_git_add_all_preflight("git add -A", Some(&root.to_string_lossy()));
        assert!(reason.is_none());
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn validate_cwd_rejects_drive_relative_prefix() {
        assert!(validate_cwd("C:foo").is_err());
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn bash_translation_basic() {
        let s = "$ mkdir -p repo && cd repo && touch README.md";
        let cleaned = sanitize_shellish_command(s);
        assert!(looks_bashish_windows_script(&cleaned));
        let ps = bash_to_powershell_script(&cleaned);
        assert!(ps.contains("$ErrorActionPreference = 'Stop'"));
        assert!(ps.contains("[System.IO.Directory]::CreateDirectory('repo')"));
        assert!(ps.contains("cd repo") || ps.contains("Set-Location 'repo'"));
        assert!(ps.contains("New-Item -ItemType File"));
    }
}
