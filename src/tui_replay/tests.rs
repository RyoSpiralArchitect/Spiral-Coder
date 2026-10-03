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

#[test]
fn invalid_check_diagnostics_include_valid_shapes_without_accepting_the_case() {
    let source = load_spec(&repository_spec_path()).unwrap();
    for invalid in [
        serde_json::json!({"kind":"replay_sensitive"}),
        serde_json::json!({"kind":"target_message_contains"}),
    ] {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("invalid.json");
        let mut case = serde_json::to_value(&source.cases[0]).unwrap();
        case["checks"] = serde_json::json!([invalid]);
        std::fs::write(
            &path,
            serde_json::json!({"version":1,"cases":[case]}).to_string(),
        )
        .unwrap();
        let error = load_spec(&path).unwrap_err().to_string();
        assert!(error.contains("unknown variant") || error.contains("missing field `value`"));
        let examples = error
            .lines()
            .find_map(|line| line.strip_prefix("Error: tui-replay valid top-level checks shapes: "))
            .unwrap();
        let checks: Vec<TuiReplayCheck> = serde_json::from_str(examples).unwrap();
        assert_eq!(checks.len(), 6);
        assert!(
            matches!(&checks[5], TuiReplayCheck::TargetMessageContains { value } if value == "coder-0")
        );
        assert!(error.contains("not source-file content"));
        assert!(error.contains("do not remove failing assertions"));
    }
}

#[test]
fn failed_replay_reports_actual_checks_and_bounds_model_supplied_labels() {
    let mut spec = load_spec(&repository_spec_path()).unwrap();
    spec.cases.truncate(1);
    spec.cases[0].observer_response = serde_json::json!({
        "summary":"Inspect the requested target", "quickest_check":"read_file(path=src/target.rs)"
    });
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("case.json");
    std::fs::write(&path, serde_json::to_string(&spec).unwrap()).unwrap();
    let mut report =
        replay_spec_for_test(&path, root.path(), &root.path().join("reports")).unwrap();
    assert_eq!(report.summary.failed, 1);
    let diagnostic = diagnostics::failed_report(&report);
    assert!(diagnostic.contains("\"check\":\"hint_queued\""));
    assert!(diagnostic.contains("hint_queued=false"));
    assert!(diagnostic.contains("quickest_check alone does not queue a hint"));
    assert!(diagnostic.contains(&report.out_dir.join("report.json").display().to_string()));

    report.cases[0].id = "case\n".repeat(100);
    report.cases[0].checks = (0..100)
        .map(|_| TuiReplayCheckResult {
            label: "失敗\n".repeat(500),
            ok: false,
            detail: "detail\n".repeat(500),
        })
        .collect();
    let diagnostic = diagnostics::failed_report(&report);
    let records = diagnostic
        .lines()
        .filter_map(|line| line.strip_prefix("Error: tui-replay {"));
    assert_eq!(records.count(), 8);
    assert!(diagnostic.contains("92 additional failed checks"));
    assert!(diagnostic.chars().count() < 7000);
}

#[test]
fn target_inference_requires_failure_like_assistant_and_selector_cannot_use_user() {
    let mut spec = load_spec(&repository_spec_path()).unwrap();
    spec.cases.truncate(1);
    let case = &mut spec.cases[0];
    case.coder_messages.truncate(1);
    case.checks = vec![TuiReplayCheck::TargetMessageContains {
        value: "coder-1".to_string(),
    }];
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("replay.json");
    let replay = |spec: &TuiReplaySpec| {
        std::fs::write(&path, serde_json::to_string(spec).unwrap()).unwrap();
        replay_spec_for_test(&path, root.path(), &root.path().join("reports"))
    };
    let error = replay(&spec).unwrap_err().to_string();
    assert!(error.contains(r#""roles":{"assistant":0,"tool":0,"user":1}"#));
    assert!(error.contains("completed failure-like assistant"));
    assert!(error.contains("target_message_contains.value cannot create or select a target"));
    spec.cases[0].coder_messages.push(TuiReplayMessage {
        role: TuiReplayMessageRole::Assistant,
        content: "[GOVERNOR BLOCK]\nMissing <think>".to_string(),
    });
    assert_eq!(replay(&spec).unwrap().summary.passed, 1);
    spec.cases[0].coder_messages.push(TuiReplayMessage {
        role: TuiReplayMessageRole::Assistant,
        content: "Continuing normally".to_string(),
    });
    assert!(replay(&spec)
        .unwrap_err()
        .to_string()
        .contains("last nonempty assistant must be failure-like"));
    spec.cases[0].selector = Some("msg:coder-1".to_string());
    assert_eq!(replay(&spec).unwrap().summary.passed, 1);
    spec.cases[0].selector = Some("msg:coder-0".to_string());
    assert!(replay(&spec)
        .unwrap_err()
        .to_string()
        .contains("not a completed coder assistant message"));
}
