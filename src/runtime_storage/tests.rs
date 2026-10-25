use super::*;

#[tokio::test]
async fn parallel_scopes_keep_generated_state_owned_and_restore_default() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("workspace");
    let other = dir.path().join("other");
    std::fs::create_dir(&root).unwrap();
    std::fs::create_dir(&other).unwrap();
    let root = root.canonicalize().unwrap();
    let a = RuntimeStorage::create(&root, &dir.path().join("a")).unwrap();
    let b = RuntimeStorage::create(&root, &dir.path().join("b")).unwrap();
    let run = |s: RuntimeStorage| {
        let root = root.clone();
        let other = other.clone();
        tokio::spawn(scope(Some(s.clone()), async move {
            tokio::task::yield_now().await;
            assert_eq!(
                crate::progress_state::path_for_root(root.to_str().unwrap()),
                s.output.join("state/progress.json")
            );
            assert_eq!(
                crate::reflection_ledger::path_for_root(root.to_str().unwrap()),
                s.output.join("state/reflection_ledger.json")
            );
            assert_eq!(
                state_path(other.to_str().unwrap(), "progress.json"),
                other.join(".spiral-coder/progress.json")
            );
        }))
    };
    let (a, b) = tokio::join!(run(a), run(b));
    a.unwrap();
    b.unwrap();
    assert_eq!(
        state_path(root.to_str().unwrap(), "progress.json"),
        root.join(".spiral-coder/progress.json")
    );
    assert!(!root.join(".spiral-coder").exists());
}

#[test]
fn refuses_workspace_outputs_and_reuse_before_creating_state() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("workspace");
    std::fs::create_dir(&root).unwrap();
    assert!(RuntimeStorage::create(&root, &root.join("nested/run")).is_err());
    assert!(!root.join("nested").exists());
    let out = dir.path().join("receipts/run");
    RuntimeStorage::create(&root, &out).unwrap();
    std::fs::write(out.join("receipt"), "keep").unwrap();
    assert!(RuntimeStorage::create(&root, &out).is_err());
    assert_eq!(
        std::fs::read_to_string(out.join("receipt")).unwrap(),
        "keep"
    );
}

#[cfg(unix)]
#[test]
fn symlinked_output_parent_cannot_put_state_back_in_workspace() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("workspace");
    std::fs::create_dir(&root).unwrap();
    let alias = dir.path().join("alias");
    std::os::unix::fs::symlink(&root, &alias).unwrap();
    assert!(RuntimeStorage::create(&root, &alias.join("nested/run")).is_err());
    assert!(!root.join("nested").exists());
}

#[cfg(unix)]
#[tokio::test]
async fn exact_python_command_keeps_bytecode_outside_the_workspace() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("workspace");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("sample.py"), "value = 42\n").unwrap();
    let storage = RuntimeStorage::create(&root, &dir.path().join("run")).unwrap();
    scope(Some(storage.clone()), async {
        let result = crate::exec::run_command(
            "python3 -S -c 'import sample; print(sample.value)'",
            Some(root.to_str().unwrap()),
        )
        .await
        .unwrap();
        assert_eq!(result.exit_code, 0, "{}", result.stderr);
        assert_eq!(result.stdout.trim(), "42");
    })
    .await;
    assert!(!root.join("__pycache__").exists());
    assert!(storage.output.join("cache/python").is_dir());
}
