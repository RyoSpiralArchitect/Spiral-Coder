/// Run the project's test command after a file edit. Returns a formatted result string.
/// Capped at 120 seconds; preserve bounded diagnostics before shortening output.
pub(super) async fn run(cmd: &str, cwd: &str) -> String {
    let mut command = tokio::process::Command::new(if cfg!(target_os = "windows") {
        "powershell"
    } else {
        "sh"
    });
    crate::runtime_storage::configure_child(&mut command);
    let fut = command
        .args(if cfg!(target_os = "windows") {
            vec!["-Command", cmd]
        } else {
            vec!["-c", cmd]
        })
        .current_dir(cwd)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output();

    match tokio::time::timeout(std::time::Duration::from_secs(120), fut).await {
        Ok(Ok(out)) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let stderr = String::from_utf8_lossy(&out.stderr);
            let exit = out.status.code().unwrap_or(-1);
            super::failure_diagnostics::automatic_test_output(&stdout, &stderr, exit, cwd)
        }
        Ok(Err(e)) => super::failure_diagnostics::automatic_test_output(
            "",
            &format!("error running test: {e}"),
            -1,
            cwd,
        ),
        Err(_) => super::failure_diagnostics::automatic_test_output(
            "",
            "test timed out after 120s",
            -1,
            cwd,
        ),
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    #[tokio::test]
    async fn edit_triggered_tests_use_the_run_cache_and_keep_native_status() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("workspace");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("sample.py"), "value = 42\n").unwrap();
        let storage =
            crate::runtime_storage::RuntimeStorage::create(&root, &dir.path().join("run")).unwrap();
        let output = storage.output().to_owned();
        crate::runtime_storage::scope(Some(storage), async {
            let result = run(
                "python3 -S -c 'import sample; assert sample.value == 42'",
                root.to_str().unwrap(),
            )
            .await;
            assert!(result.contains("[auto-test] ✓ PASSED (exit 0)"), "{result}");
            let failed = run(
                "python3 -S -c 'import sample; assert sample.value == 0'",
                root.to_str().unwrap(),
            )
            .await;
            assert!(failed.contains("[auto-test] ✗ FAILED (exit 1)"), "{failed}");
        })
        .await;
        assert!(!root.join("__pycache__").exists());
        assert!(output.join("cache/python").is_dir());
    }
}
