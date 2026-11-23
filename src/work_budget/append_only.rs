//! Preserve the original bytes of a document the human explicitly named as append-only.
use anyhow::{ensure, Result};
use ring::digest::{Context, SHA256};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{ErrorKind, Read};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct Prefix {
    bytes: u64,
    sha256: String,
}

impl Prefix {
    fn read(mut reader: impl Read) -> Result<Self> {
        let mut digest = Context::new(&SHA256);
        let mut bytes = 0;
        let mut buffer = [0_u8; 8192];
        loop {
            let count = reader.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            digest.update(&buffer[..count]);
            bytes += count as u64;
        }
        let sha256 = digest
            .finish()
            .as_ref()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        Ok(Self { bytes, sha256 })
    }

    pub(super) fn check_on_disk(&self, path: &Path, root: &Path) -> Result<()> {
        let target = path.canonicalize()?;
        ensure!(
            target.starts_with(root) && target.is_file(),
            "{} must still resolve to a regular append-only document in its workspace",
            path.display()
        );
        self.check(path, File::open(target)?)
    }

    pub(super) fn check(&self, path: &Path, reader: impl Read) -> Result<()> {
        let candidate = Self::read(reader.take(self.bytes))?;
        ensure!(candidate.bytes == self.bytes && candidate.sha256 == self.sha256,
            "{} is append-only for this task. Keep the original {} bytes at the start. Read the document's final lines and append after a unique end-of-file anchor.",
            path.display(), self.bytes);
        Ok(())
    }
}

fn declared_paths(task: &str) -> Vec<String> {
    let mut paths = Vec::new();
    let mut fence = None;
    for line in task.lines() {
        let text = line.trim();
        let first = text.as_bytes().first().copied();
        let width = text.bytes().take_while(|b| Some(*b) == first).count();
        if let Some((delimiter, length)) = fence {
            if first == Some(delimiter) && width >= length && text[width..].trim().is_empty() {
                fence = None;
            }
            continue;
        }
        if width >= 3 && matches!(first, Some(b'`' | b'~')) {
            fence = Some((first.unwrap(), width));
            continue;
        }
        if line.starts_with("    ") || line.starts_with('\t') {
            continue;
        }
        let line = text
            .strip_prefix("- ")
            .or_else(|| text.strip_prefix("* "))
            .unwrap_or(text);
        let Some((token, rest)) = line.split_once(" is append-only") else {
            continue;
        };
        let suffix = rest.strip_prefix(" for this task").unwrap_or(rest);
        if !(suffix.is_empty() || suffix == "." || suffix.starts_with(". ")) {
            continue;
        }
        let path = if let Some(quoted) = token.strip_prefix('`').and_then(|v| v.strip_suffix('`')) {
            if quoted.contains('`') {
                continue;
            }
            quoted
        } else {
            if token.contains(char::is_whitespace) || token.starts_with(['>', '"', '\'']) {
                continue;
            }
            token
        };
        if let Some(path) = crate::task_scope::relative_path(path, None) {
            paths.push(path);
        }
    }
    paths
}

pub(super) fn capture(task: &str, root: &Path) -> Result<BTreeMap<PathBuf, Prefix>> {
    let mut result = BTreeMap::new();
    for relative in declared_paths(task) {
        let path = root.join(relative);
        let target = match path.canonicalize() {
            Ok(target) => target,
            Err(error) if error.kind() == ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        };
        // Only ordinary files in this workspace can supply an initial prefix.
        if target.starts_with(root) && target.is_file() {
            result.insert(path, Prefix::read(File::open(target)?)?);
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::work_budget::{scope, Context, WorkBudget};
    use std::fs;
    #[test]
    fn directives_select_only_literal_workspace_paths_in_the_human_prose() {
        let task = "guide.md is append-only. Read to the end.\n- `docs/Two words.md` is append-only for this task. More detail.\n```md\nquoted.md is append-only.\n```\n> hypothetical.md is append-only.\n    example.md is append-only.\n- other.md is append-only when convenient.\n../outside is append-only.\n/absolute is append-only.\nIf applicable keep log.md is append-only.\nLeave all other documents editable.";
        assert_eq!(declared_paths(task), ["guide.md", "docs/Two words.md"]);
    }

    #[tokio::test]
    async fn native_edits_and_resumed_completion_preserve_the_original_prefix() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let path = root.join("guide.md");
        let store = root.join("work.json");
        let original = "First\r\nInterior\r\nLast with no final newline";
        fs::write(&path, original).unwrap();
        let task = "guide.md is append-only for this task.";
        let budget = WorkBudget::new(task, 12, Some(store.clone()));
        budget.protect_task_artifacts(&root).unwrap();
        scope(
            Some(Context {
                budget: budget.clone(),
                role: "coder",
            }),
            async {
                let dir = root.to_str();
                let (fail, error) = crate::file_tools::tool_patch_file(
                    "guide.md",
                    "Interior",
                    "New section\r\nInterior",
                    dir,
                );
                assert!(error && fail.contains("end-of-file anchor"), "{fail}");
                let (fail, error) =
                    crate::file_tools::tool_write_file("guide.md", "unrelated", dir);
                assert!(error && fail.contains("append-only"), "{fail}");
                let diff = "--- a/guide.md\n+++ b/guide.md\n@@ -1,1 +1,2 @@\n+Addition\n First\n";
                let (fail, error) = crate::file_tools::tool_apply_diff("guide.md", diff, dir);
                assert!(error && fail.contains("append-only"), "{fail}");
                assert_eq!(fs::read_to_string(&path).unwrap(), original);
                let (ok, error) = crate::file_tools::tool_patch_file(
                    "guide.md",
                    "Last with no final newline",
                    "Last with no final newline\r\nAppendix",
                    dir,
                );
                assert!(!error, "{ok}");
                assert!(budget.preservation_issue().is_none());
            },
        )
        .await;
        let restored = WorkBudget::load(task, store).unwrap();
        let next = WorkBudget::new(task, 12, None);
        next.inherit_protections(&restored.snapshot(), &root)
            .unwrap();
        next.check_write(&path, &format!("{original}\r\nRevised appendix"))
            .unwrap();
        fs::write(&path, "Changed outside native tools").unwrap();
        assert!(next.preservation_issue().unwrap().contains("append-only"));
        let disk_root = root.to_str().unwrap();
        scope(
            Some(Context {
                budget: next.clone(),
                role: "coder",
            }),
            async {
                let (ok, error) = crate::file_tools::tool_write_file(
                    "guide.md",
                    &format!("{original}\nRepaired"),
                    Some(disk_root),
                );
                assert!(!error, "{ok}");
            },
        )
        .await;
        assert!(next.preservation_issue().is_none());
    }
}
