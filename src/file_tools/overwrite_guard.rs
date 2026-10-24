//! A high-churn source replacement needs explicit deletion context.
//! This is a mechanical editing safeguard, not a semantic preservation oracle.
use anyhow::{bail, Result};
use std::{collections::BTreeSet, path::Path};

pub(super) fn check(path: &Path, replacement: &str) -> Result<()> {
    if !source_path(path) || !path.is_file() {
        return Ok(());
    }
    let original = std::fs::read_to_string(path)?;
    let before = lines(&original);
    let after = lines(replacement);
    let removed = before.difference(&after).count();
    // Ignore whitespace-only formatting, small files and local repairs. Count
    // distinct lines so repeated braces/blanks cannot hide a large replacement.
    if before.len() >= 20 && removed * 2 > before.len() {
        bail!(
            "write_file would remove or replace {removed}/{} distinct nonblank source lines. \
             The existing file is unchanged. Use patch_file or apply_diff with explicit old \
             context for the intended change, preserving unrelated behavior. If a broad \
             replacement is intentional, express its deletions in that patch and verify them.",
            before.len()
        );
    }
    Ok(())
}

fn lines(body: &str) -> BTreeSet<&str> {
    body.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect()
}

fn source_path(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|ext| ext.to_str()),
        Some(
            "py" | "pyi"
                | "rs"
                | "js"
                | "jsx"
                | "mjs"
                | "cjs"
                | "ts"
                | "tsx"
                | "go"
                | "java"
                | "kt"
                | "kts"
                | "swift"
                | "c"
                | "h"
                | "cc"
                | "cpp"
                | "hpp"
                | "cs"
                | "rb"
                | "php"
                | "sh"
                | "bash"
                | "zsh"
                | "lua"
        )
    )
}

#[cfg(test)]
mod tests {
    use crate::file_tools::{tool_patch_file, tool_write_file};

    fn source() -> String {
        (0..30)
            .map(|i| format!("def operation_{i}():\n    return {i}\n\n"))
            .collect()
    }

    #[test]
    fn lossy_replacement_leaves_original_and_temp_file_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("runner.py");
        let original = source();
        std::fs::write(&path, &original).unwrap();
        let (message, failed) =
            tool_write_file("runner.py", "def main():\n    pass\n", dir.path().to_str());
        assert!(failed, "{message}");
        assert!(message.contains("explicit old context"));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
        assert!(!dir.path().join("runner.py.__spiral_coder_tmp").exists());
        // Explicit deletions remain possible; the safeguard grants no proof.
        let (message, failed) = tool_patch_file(
            "runner.py",
            &original,
            "def main():\n    pass\n",
            dir.path().to_str(),
        );
        assert!(!failed, "{message}");
    }

    #[test]
    fn local_repairs_formatting_creation_docs_and_small_files_remain_available() {
        let dir = tempfile::tempdir().unwrap();
        let original = source();
        for (name, before, after) in [
            (
                "runner.py",
                original.clone(),
                original.replace("return 7", "return 42"),
            ),
            (
                "runner.rs",
                original.clone(),
                original.replace("\n", "\r\n").replace("    ", "\t"),
            ),
            (
                "guide.md",
                original.clone(),
                "replacement documentation".into(),
            ),
            ("tiny.py", "print('old')".into(), "print('new')".into()),
        ] {
            std::fs::write(dir.path().join(name), before).unwrap();
            let (message, failed) = tool_write_file(name, &after, dir.path().to_str());
            assert!(!failed, "{name}: {message}");
            assert_eq!(
                std::fs::read_to_string(dir.path().join(name)).unwrap(),
                after
            );
        }
        assert!(!tool_write_file("new.py", "print('new')", dir.path().to_str()).1);
    }
}
