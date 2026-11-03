use std::fs;
use std::path::Path;
use std::process::Command;

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_spiral-coder")
}

fn snapshot(root: &Path) -> Vec<(String, Vec<u8>)> {
    fn visit(root: &Path, dir: &Path, out: &mut Vec<(String, Vec<u8>)>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let rel = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .into_owned();
            if path.is_dir() {
                visit(root, &path, out);
            } else {
                out.push((rel, fs::read(path).unwrap()));
            }
        }
    }
    let mut out = Vec::new();
    visit(root, root, &mut out);
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

#[test]
fn no_match_listing_matches_replay_and_is_read_only() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::create_dir(root.join(".spiral-coder")).unwrap();
    fs::copy(
        ".spiral-coder/tui_replay.json",
        root.join(".spiral-coder/tui_replay.json"),
    )
    .unwrap();
    let output_dir = tempfile::tempdir().unwrap();
    let filter = "definitely-no-replay-case-matches";
    let replay = Command::new(binary())
        .args([
            "tui-replay",
            "--tool-root",
            root.to_str().unwrap(),
            "--spec",
            ".spiral-coder/tui_replay.json",
            "--filter",
            filter,
            "--out-dir",
            output_dir.path().to_str().unwrap(),
        ])
        .output()
        .unwrap();
    let before = snapshot(root);
    let listing = Command::new(binary())
        .args([
            "tui-replay-list",
            "--tool-root",
            root.to_str().unwrap(),
            "--spec",
            ".spiral-coder/tui_replay.json",
            "--filter",
            filter,
        ])
        .output()
        .unwrap();
    assert!(!replay.status.success());
    assert!(!listing.status.success());
    assert_eq!(listing.stderr, replay.stderr);
    assert_eq!(snapshot(root), before);
}
