use super::*;

fn repository_spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(".spiral-coder/tui_replay.json")
}

#[test]
fn empty_effective_replay_checks_are_rejected_during_load_and_execution() {
    use clap::Parser;
    let source = load_spec(&repository_spec_path()).unwrap();
    let mut raw = serde_json::to_value(&source.cases[0]).unwrap();
    raw["replay"] = serde_json::json!({"checks":raw["checks"].clone()});
    for omitted in [false, true] {
        raw["checks"] = serde_json::json!([]);
        if omitted {
            raw.as_object_mut().unwrap().remove("checks");
        }
        let case: TuiReplayCase = serde_json::from_value(raw.clone()).unwrap();
        let root = tempfile::tempdir().unwrap();
        let spec_path = root.path().join("invalid.json");
        std::fs::write(
            &spec_path,
            serde_json::json!({"version":1,"cases":[raw.clone()]}).to_string(),
        )
        .unwrap();
        let load_error = load_spec(&spec_path).unwrap_err().to_string();
        assert!(load_error.contains(&case.id));
        assert!(load_error.contains("top-level checks"));
        let common =
            crate::CommonArgs::try_parse_from(["spiral-coder", "--provider", "openai"]).unwrap();
        let output = root.path().join("reports");
        let error = run_case(
            0,
            &common,
            &TuiReplayDefaults::default(),
            &case,
            root.path(),
            &output,
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("top-level checks"));
        assert!(
            !output.exists(),
            "invalid cases are rejected before artifacts are produced"
        );
    }
}

#[test]
fn existing_repository_replay_cases_remain_executable() {
    let spec_path = repository_spec_path();
    let spec = load_spec(&spec_path).unwrap();
    assert!(!spec.cases.is_empty());
    let output = tempfile::tempdir().unwrap();
    let report = replay_spec_for_test(
        &spec_path,
        Path::new(env!("CARGO_MANIFEST_DIR")),
        output.path(),
    )
    .unwrap();
    assert_eq!(report.summary.total, spec.cases.len());
    assert_eq!(report.summary.failed, 0);
    assert_eq!(report.summary.passed, spec.cases.len());
}
