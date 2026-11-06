use super::*;

pub(super) fn is_git_repo_root(dir: &str) -> bool {
    let p = Path::new(dir);
    let dot_git = p.join(".git");
    dot_git.is_dir() || dot_git.is_file()
}

pub(super) fn gitmodules_lists_path(repo_root: &str, rel_path: &str) -> bool {
    let p = Path::new(repo_root).join(".gitmodules");
    let Ok(text) = std::fs::read_to_string(&p) else {
        return false;
    };
    let needle = format!("path = {rel_path}");
    text.lines().any(|l| l.trim() == needle)
}

pub(super) fn nested_git_dirs_shallow(repo_root: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let root = Path::new(repo_root);
    let Ok(rd) = std::fs::read_dir(root) else {
        return out;
    };
    for ent in rd.flatten() {
        let path = ent.path();
        if !path.is_dir() {
            continue;
        }
        let dot_git = path.join(".git");
        if !(dot_git.is_dir() || dot_git.is_file()) {
            continue;
        }
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        // Ignore the repo root itself (we only scan immediate children anyway).
        if name == ".git" {
            continue;
        }
        out.push(name.to_string());
    }
    out.sort();
    out.dedup();
    out
}

pub(super) fn should_block_git_landmines(
    command: &str,
    tool_root_abs: Option<&str>,
) -> Option<String> {
    let root = tool_root_abs?;
    if !is_git_repo_root(root) {
        return None;
    }

    let cmd_low = command.to_ascii_lowercase();

    // 1) Never create a new git repo inside an existing repo; this causes embedded repos and breaks `git add`.
    if cmd_low.contains("git init") {
        return Some(format!(
            "Refusing to run `git init` inside an existing git repo (tool_root has .git): {root}\n\
This is a common agent failure mode that creates embedded repos and breaks `git add`.\n\
Fix: set tool_root to a fresh directory outside this repo (e.g. `.tmp/newrepo`), or remove `git init` and just create files."
        ));
    }

    // 2) Block `git add` when we detect nested git directories under tool_root (unless they are declared submodules).
    if cmd_low.contains("git add") {
        let nested = nested_git_dirs_shallow(root);
        let mut offenders: Vec<String> = Vec::new();
        for d in nested {
            if !gitmodules_lists_path(root, &d) {
                offenders.push(d);
            }
        }
        if !offenders.is_empty() {
            let list = offenders.join(", ");
            return Some(format!(
                "Nested git repo(s) detected under tool_root: {list}\n\
Running `git add` will trigger embedded-repo errors or index failures.\n\
Fix: move those directories outside tool_root, add them to `.gitignore`, or add them properly as submodules (`git submodule add <url> <path>`)."
            ));
        }
    }

    None
}

/// Create a git checkpoint commit in `root` (if it is a git repo).
/// Returns the HEAD hash after the commit, or None if not a git repo / git unavailable.
pub(super) async fn git_create_checkpoint(root: &str) -> Option<String> {
    // Only proceed if `root` itself is the git top-level. `git -C subdir`
    // happily walks up to a parent repo; for copied eval sandboxes under
    // `.tmp/`, that would create checkpoint commits in the real workspace.
    let git_top = run_git_cmd(root, &["rev-parse", "--show-toplevel"]).await?;
    if !git_checkpoint_root_matches_tool_root(root, git_top.as_str()) {
        return None;
    }

    // Only proceed if this is a git repo.
    let head = run_git_cmd(root, &["rev-parse", "HEAD"]).await;
    if head.is_none() {
        return None;
    }

    // Stage all current changes (untracked included).
    let _ = run_git_cmd(root, &["add", "-A"]).await;

    // Commit with --allow-empty so we always get a clean ref even if nothing changed.
    let epoch = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let msg = format!("spiral-coder: pre-session checkpoint {epoch}");
    let _ = run_git_cmd(root, &["commit", "--allow-empty", "-m", &msg]).await;

    // Return the new HEAD hash.
    run_git_cmd(root, &["rev-parse", "HEAD"]).await
}

pub(super) fn git_checkpoint_root_matches_tool_root(root: &str, git_top_level: &str) -> bool {
    let root_path = match std::fs::canonicalize(root) {
        Ok(path) => path,
        Err(_) => return false,
    };
    let top_path = match std::fs::canonicalize(git_top_level) {
        Ok(path) => path,
        Err(_) => return false,
    };
    normalize_checkpoint_path(&root_path) == normalize_checkpoint_path(&top_path)
}

pub(super) fn normalize_checkpoint_path(path: &Path) -> String {
    let mut out = path.to_string_lossy().replace('\\', "/");
    while out.ends_with('/') && out.len() > 1 {
        out.pop();
    }
    if cfg!(target_os = "windows") {
        out = out.to_ascii_lowercase();
    }
    out
}

/// Run `git -C root <args>` with a 5-second timeout. Returns trimmed stdout or None.
pub(super) async fn run_git_cmd(root: &str, args: &[&str]) -> Option<String> {
    let fut = tokio::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output();
    let out = tokio::time::timeout(std::time::Duration::from_secs(5), fut)
        .await
        .ok()?
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}
