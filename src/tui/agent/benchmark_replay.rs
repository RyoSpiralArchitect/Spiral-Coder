//! Build a deterministic Observer-to-Coder handoff case for a benchmark target.
use crate::tui_replay::{TuiReplayCase, TuiReplayCheck, TuiReplayMessage, TuiReplayMessageRole};
use serde_json::json;

pub(super) fn case(id: String, path: &str) -> TuiReplayCase {
    let prompt = format!("Find the role of `{path}`. Read the target before widening the search.");
    TuiReplayCase {
        id,
        prompt: prompt.clone(),
        tool_root: None,
        lang: Some("en".to_string()),
        selector: None,
        reason_hint: None,
        tags: vec!["benchmark-plan".to_string(), "observer-handoff".to_string()],
        coder_messages: vec![
            TuiReplayMessage { role: TuiReplayMessageRole::User, content: prompt },
            TuiReplayMessage {
                role: TuiReplayMessageRole::Assistant,
                content: "[GOVERNOR BLOCK]\nMissing <think>\nNeed one concrete next step before broadening search.".to_string(),
            },
        ],
        observer_response: json!({
            "summary": "Read the requested target before broadening the search.",
            "primary_blocker": "missing_concrete_next_step",
            "suggestions": [{
                "kind": "read",
                "reason": "The approved benchmark names this target as the next focused read.",
                "confidence": 0.9,
                "suggested_tool": "read_file",
                "suggested_args": { "path": path },
                "based_on": ["intent_anchor", "failure_kind"]
            }],
            "quickest_check": format!("read_file(path={path})"),
            "why_this_first": "Keep the continuation attached to the approved target."
        }),
        checks: vec![
            TuiReplayCheck::SuggestionParsed,
            TuiReplayCheck::HintQueued,
            TuiReplayCheck::ObserverContains { value: path.to_string() },
            TuiReplayCheck::CoderSystemContains { value: format!("read_file(path={path})") },
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synthesize(body: &str) -> String {
        super::super::synthesize_benchmark_plan_json_body(
            ".spiral-coder/tui_replay.json",
            body,
            "src/tui/review_panel.rs",
            "<observer_benchmark_plan>\nlane: tui_replay\ncase_id_hint: review-panel-replay-sensitive\n</observer_benchmark_plan>",
        ).expect("valid replay spec patch")
    }

    fn replay(body: &str) -> crate::tui_replay::TuiReplayReport {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tui_replay.json");
        std::fs::write(&path, body).unwrap();
        crate::tui_replay::replay_spec_for_test(&path, dir.path(), &dir.path().join("reports"))
            .expect("actual replay parser and runner")
    }

    #[test]
    fn synthesized_case_loads_and_runs_in_actual_tui_replay() {
        let body = synthesize(r#"{"version":1,"cases":[]}"#);
        let report = replay(&body);
        assert_eq!(report.summary.total, 1);
        assert_eq!(report.summary.passed, 1, "{:#?}", report.cases);
        assert_eq!(report.cases[0].id, "review-panel-replay-sensitive");
        assert!(report.cases[0].metrics.suggestion_parsed);
        assert!(report.cases[0].metrics.hint_queued);
        assert_eq!(report.cases[0].checks.len(), 4);
    }

    #[test]
    fn real_smoke_fixture_replays_before_and_after_synthesis() {
        let fixture = include_str!("../../../tests/fixtures/runtime-benchmark-plan-tui-replay/.spiral-coder/tui_replay.json");
        let baseline = replay(fixture);
        assert_eq!(baseline.summary.passed, 1, "{:#?}", baseline.cases);
        let report = replay(&synthesize(fixture));
        assert_eq!(report.summary.total, 2);
        assert_eq!(report.summary.passed, 2, "{:#?}", report.cases);
    }

    #[test]
    fn replay_checks_fail_when_observer_handoff_is_missing() {
        let mut spec: serde_json::Value =
            serde_json::from_str(&synthesize(r#"{"version":1,"cases":[]}"#)).unwrap();
        spec["cases"][0]["observer_response"] =
            json!({"summary": "No concrete next step", "suggestions": []});
        let report = replay(&spec.to_string());
        assert_eq!(report.summary.failed, 1);
        assert!(report.cases[0].checks.iter().any(|check| !check.ok));
    }
}
