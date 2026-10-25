use super::*;

#[tokio::test]
async fn candidate_commands_do_not_inherit_provider_credentials() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("workspace");
    std::fs::create_dir(&root).unwrap();
    let storage = RuntimeStorage::create(&root, &dir.path().join("run"))
        .unwrap()
        .with_private_env_names(["SELF_DOGFOOD_OBSERVER_KEY".into()]);
    scope(Some(storage), async {
        let mut command = tokio::process::Command::new("unused");
        for key in [
            "OPENAI_API_KEY",
            "GEMINI_API_KEY",
            "MISTRAL_API_KEY",
            "SELF_DOGFOOD_OBSERVER_KEY",
        ] {
            command.env(key, "test-only-placeholder");
        }
        command.env("CARGO_TARGET_DIR", "retained-build-cache");
        configure_child(&mut command);
        let env: std::collections::BTreeMap<_, _> = command.as_std().get_envs().collect();
        for key in [
            "OPENAI_API_KEY",
            "GEMINI_API_KEY",
            "MISTRAL_API_KEY",
            "SELF_DOGFOOD_OBSERVER_KEY",
        ] {
            assert_eq!(env.get(std::ffi::OsStr::new(key)), Some(&None));
        }
        assert_eq!(
            env.get(std::ffi::OsStr::new("CARGO_TARGET_DIR")),
            Some(&Some(std::ffi::OsStr::new("retained-build-cache")))
        );
    })
    .await;
    let mut ordinary = tokio::process::Command::new("unused");
    ordinary.env("OPENAI_API_KEY", "test-only-placeholder");
    configure_child(&mut ordinary);
    assert!(ordinary
        .as_std()
        .get_envs()
        .any(|(name, value)| name == "OPENAI_API_KEY" && value.is_some()));
}

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
