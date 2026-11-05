use serde_json::Value;
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

const GUIDE: &str = include_str!("../docs/evaluation.md");
const STOCK_SPEC: &[u8] = include_bytes!("../.spiral-coder/tui_replay.json");

fn list(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_spiral-coder"))
        .current_dir(root)
        .env_remove("OPENAI_API_KEY")
        .env_remove("MISTRAL_API_KEY")
        .env_remove("GEMINI_API_KEY")
        .env_remove("GOOGLE_API_KEY")
        .env("HOME", root)
        .arg("tui-replay-list")
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn guide_examples_match_the_stock_catalog_and_leave_the_project_untouched() {
    let root = tempfile::tempdir().unwrap();
    let spec_dir = root.path().join(".spiral-coder");
    fs::create_dir(&spec_dir).unwrap();
    let spec = spec_dir.join("tui_replay.json");
    fs::write(&spec, STOCK_SPEC).unwrap();
    let actual = list(root.path(), &["--filter", "prefs", "--max-cases", "2"]);
    assert!(
        actual.status.success(),
        "{}",
        String::from_utf8_lossy(&actual.stderr)
    );
    let guide = GUIDE.replace("\r\n", "\n");
    let (_, example) = guide
        .split_once("$ spiral tui-replay-list --filter prefs --max-cases 2\n")
        .unwrap();
    let (json, _) = example.split_once("\n```").unwrap();
    let shown: Value = serde_json::from_str(json).unwrap();
    assert_eq!(
        shown,
        serde_json::from_slice::<Value>(&actual.stdout).unwrap()
    );
    let empty = list(root.path(), &["--filter", "__spiral_no_match__"]);
    assert!(!empty.status.success() && empty.stdout.is_empty() && !empty.stderr.is_empty());
    let code = empty.status.code().unwrap();
    assert!(guide.contains(&format!(
        "$ spiral tui-replay-list --filter __spiral_no_match__\n$ echo $?\n{code}\n```"
    )));
    assert_eq!(fs::read(&spec).unwrap(), STOCK_SPEC);
    assert_eq!(fs::read_dir(&spec_dir).unwrap().count(), 1);
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
}
