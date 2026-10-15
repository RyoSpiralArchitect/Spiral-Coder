//! Exercise the actual shell wrapper, including output without a final newline.
use super::{exec, is_within_root, strip_pwd_marker, wrap_exec_with_pwd, PWD_MARKER};

#[tokio::test]
async fn newline_free_output_preserves_location_and_exit_status() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("workspace");
    std::fs::create_dir_all(root.join("child")).unwrap();
    let root = root.canonicalize().unwrap();
    for (destination, succeeds) in [("child", true), ("..", true), ("..", false)] {
        let command = if cfg!(target_os = "windows") {
            format!(
                "Set-Location {destination}\n[Console]::Write('verified-current')\n{}",
                if succeeds { "$true" } else { "throw 'failed'" }
            )
        } else {
            format!(
                "cd {destination} && printf verified-current; {}",
                if succeeds { "true" } else { "false" }
            )
        };
        let result = exec::run_command(&wrap_exec_with_pwd(&command), root.to_str())
            .await
            .unwrap();
        let (stdout, cwd) = strip_pwd_marker(&result.stdout);
        assert!(stdout.contains("verified-current"));
        assert!(
            !stdout.contains(PWD_MARKER),
            "internal marker is not user output"
        );
        let expected = root.join(destination).canonicalize().unwrap();
        assert_eq!(cwd.as_deref(), expected.to_str());
        assert_eq!(result.exit_code == 0, succeeds);
        assert_eq!(
            is_within_root(cwd.as_deref().unwrap(), root.to_str().unwrap()),
            destination == "child",
            "root escape must remain observable even without a trailing newline"
        );
    }
}
