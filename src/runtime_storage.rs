//! Run-owned generated state, separate from the workspace being edited.
//! Explicitly inherited by request tasks; never set process-wide environment.
use anyhow::{ensure, Context, Result};
use std::path::{Component, Path, PathBuf};

#[derive(Clone, Debug)]
pub(crate) struct RuntimeStorage {
    workspace: PathBuf,
    output: PathBuf,
}

impl RuntimeStorage {
    pub(crate) fn create(workspace: &Path, output: &Path) -> Result<Self> {
        let workspace = workspace.canonicalize().context("resolve workspace")?;
        let output = prospective_path(output)?;
        ensure!(
            !output.starts_with(&workspace),
            "coupled-run output must be outside the workspace (including symlinks)"
        );
        ensure!(
            !output.exists(),
            "output exists; preserve earlier receipts and choose a fresh directory"
        );
        std::fs::create_dir_all(output.parent().context("output needs a parent")?)?;
        // Atomic creation refuses an already-owned run, including concurrent starts.
        std::fs::create_dir(&output)?;
        let output = output.canonicalize()?;
        ensure!(
            !output.starts_with(&workspace),
            "output resolved inside workspace"
        );
        Ok(Self { workspace, output })
    }

    pub(crate) fn output(&self) -> &Path {
        &self.output
    }

    fn owns(&self, root: &Path) -> bool {
        root == self.workspace || root.canonicalize().ok().as_ref() == Some(&self.workspace)
    }
}

// Resolve existing symlink ancestors before creating any directory. Missing
// normal components can be appended safely; ambiguous missing/../ paths fail.
fn prospective_path(path: &Path) -> Result<PathBuf> {
    let path = std::path::absolute(path)?;
    let mut ancestor = path.as_path();
    let mut suffix = Vec::new();
    while !ancestor.exists() {
        ensure!(
            matches!(
                ancestor.components().next_back(),
                Some(Component::Normal(_))
            ),
            "output has an unresolved parent component"
        );
        suffix.push(
            ancestor
                .file_name()
                .context("invalid output path")?
                .to_owned(),
        );
        ancestor = ancestor.parent().context("output has no existing parent")?;
    }
    let mut resolved = ancestor.canonicalize()?;
    for name in suffix.into_iter().rev() {
        resolved.push(name);
    }
    Ok(resolved)
}

tokio::task_local! { static CURRENT: RuntimeStorage; }

pub(crate) fn current() -> Option<RuntimeStorage> {
    CURRENT.try_with(Clone::clone).ok()
}

pub(crate) async fn scope<F: std::future::Future>(
    storage: Option<RuntimeStorage>,
    future: F,
) -> F::Output {
    match storage {
        Some(storage) => CURRENT.scope(storage, future).await,
        None => future.await,
    }
}

/// Only generated state uses this resolver. Project instructions, replay specs
/// and promoted policy overlays remain workspace inputs.
pub(crate) fn state_path(root: &str, name: &str) -> PathBuf {
    if let Some(storage) = current().filter(|s| s.owns(Path::new(root))) {
        storage.output.join("state").join(name)
    } else {
        Path::new(root).join(".spiral-coder").join(name)
    }
}

pub(crate) fn configure_child(command: &mut tokio::process::Command) {
    if let Some(storage) = current() {
        command.env("PYTHONPYCACHEPREFIX", storage.output.join("cache/python"));
    }
}

#[cfg(test)]
mod tests;
