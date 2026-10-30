use std::{fs, path::Path, process::Command};

const REPLAY_ROOT: &str = "SPIRAL_CODER_REPLAY_OUTPUT_ROOT";
const SPEC: &str = include_str!(
    "../benchmarks/coding-v1/cases/replay-json/reference/.spiral-coder/tui_replay.json"
);

fn workspace(parent: &Path) -> std::path::PathBuf {
    let root = parent.join("workspace");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("spec.json"), SPEC).unwrap();
    root
}

fn replay(workspace: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_spiral-coder"));
    cmd.current_dir(workspace).env_remove(REPLAY_ROOT).args([
        "--provider",
        "openai",
        "tui-replay",
        "--spec",
        "spec.json",
    ]);
    cmd
}

fn assert_report(dir: &Path) {
    let report: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.join("report.json")).unwrap()).unwrap();
    assert_eq!(report["summary"]["total"], 5);
    assert_eq!(report["summary"]["passed"], 5);
    for case in report["cases"].as_array().unwrap() {
        let path = Path::new(case["artifacts"]["case_report_path"].as_str().unwrap());
        assert!(
            path.is_file()
                && path
                    .canonicalize()
                    .unwrap()
                    .starts_with(dir.canonicalize().unwrap())
        );
    }
}

#[test]
fn concurrent_scoped_replays_keep_both_reports_outside_the_workspace() {
    let dir = tempfile::tempdir().unwrap();
    let root = workspace(dir.path());
    let receipts = dir.path().join("run/replays");
    let mut children = (0..2)
        .map(|_| {
            replay(&root)
                .env(REPLAY_ROOT, &receipts)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .unwrap()
        })
        .collect::<Vec<_>>();
    for child in children.drain(..) {
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    assert!(
        !root.join(".tmp").exists(),
        "verification wrote into the candidate"
    );
    assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
    assert_eq!(fs::read_to_string(root.join("spec.json")).unwrap(), SPEC);
    let reports = fs::read_dir(receipts)
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(reports.len(), 2);
    for report in reports {
        assert_report(&report.path());
    }
}

#[test]
fn explicit_replay_output_and_standalone_default_remain_usable() {
    let dir = tempfile::tempdir().unwrap();
    let root = workspace(dir.path());
    let receipts = dir.path().join("unused");
    let explicit = dir.path().join("explicit");
    let result = replay(&root)
        .env(REPLAY_ROOT, &receipts)
        .arg("--out-dir")
        .arg(&explicit)
        .output()
        .unwrap();
    assert!(result.status.success());
    assert_report(&explicit);
    assert!(!receipts.exists() && !root.join(".tmp").exists());
    assert!(replay(&root).output().unwrap().status.success());
    let ordinary = fs::read_dir(root.join(".tmp"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    assert_report(&ordinary);
}

#[test]
fn invalid_inherited_roots_fail_before_writing_into_the_candidate() {
    let dir = tempfile::tempdir().unwrap();
    let root = workspace(dir.path());
    let mut invalid = vec![root.join("nested/replays"), "relative".into()];
    #[cfg(unix)]
    {
        let alias = dir.path().join("alias");
        std::os::unix::fs::symlink(&root, &alias).unwrap();
        invalid.push(alias.join("nested/replays"));
    }
    for value in invalid {
        let output = replay(&root).env(REPLAY_ROOT, value).output().unwrap();
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("child replay output"));
    }
    assert_eq!(fs::read_dir(root).unwrap().count(), 1);
}
