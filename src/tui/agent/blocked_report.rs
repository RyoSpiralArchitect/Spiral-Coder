//! An explicit incomplete terminal report is not successful `done` evidence.
//! It preserves native obligations and validates every claimed completed item.
use super::*;
use serde_json::Value;

#[derive(Debug, serde::Serialize)]
pub(super) struct BlockedReport {
    pub summary: String,
    pub remaining_acceptance: Vec<String>,
    pub next_steps: String,
    pub pending_exact_checks: Vec<String>,
    pub pending_recovery: Vec<String>,
}

pub(super) fn prepare(
    calls: &[ToolCallData],
    plan: Option<&PlanBlock>,
    messages: &[Value],
    test_cmd: Option<&str>,
    observation: &ObservationEvidence,
) -> Option<Result<BlockedReport>> {
    let [call] = calls else { return None };
    if call.name != "done" {
        return None;
    }
    let args: Value = serde_json::from_str(&call.arguments).ok()?;
    let remaining = args["remaining_acceptance"].as_array()?;
    if remaining.is_empty() {
        return None;
    }
    Some((|| {
        anyhow::ensure!(
            remaining
                .iter()
                .all(|v| v.as_str().is_some_and(|s| !s.trim().is_empty())),
            "remaining_acceptance must contain nonempty criterion references"
        );
        let summary = args["summary"].as_str().unwrap_or("").trim();
        let next_steps = args["next_steps"].as_str().unwrap_or("").trim();
        anyhow::ensure!(
            !summary.is_empty(),
            "An incomplete report needs a blocker in summary"
        );
        let completed = parse_string_list_arg(&args["completed_acceptance"]);
        let remaining_acceptance = parse_string_list_arg(&args["remaining_acceptance"]);
        validate_done_acceptance(
            plan,
            &completed,
            &remaining_acceptance,
            &acceptance_report::parse_done_acceptance_evidence(&args["acceptance_evidence"]),
            &acceptance_report::current_commands(messages, test_cmd),
            observation,
        )?;
        let obligations = task_obligations::project(messages, test_cmd);
        Ok(BlockedReport {
            summary: summary.into(),
            remaining_acceptance,
            next_steps: next_steps.into(),
            pending_exact_checks: obligations.pending_exact_checks,
            pending_recovery: obligations.pending_recovery,
        })
    })())
}

impl BlockedReport {
    pub(super) fn render(&self) -> String {
        format!("[BLOCKED — incomplete]\n{}\n\nRemaining acceptance: {}\nPending exact checks: {}\nNative recovery: {}\n\nNext: {}\nNo completion or qualification is claimed.",
            self.summary, self.remaining_acceptance.join("; "),
            self.pending_exact_checks.join("; "), self.pending_recovery.join("; "), if self.next_steps.is_empty() { "Not supplied; Observer diagnosis must identify the missing fact or next action." } else { &self.next_steps })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn truthful_incomplete_report_preserves_unmet_checks_and_cannot_claim_unproved_work() {
        let plan = PlanBlock {
            goal: "repair".into(),
            steps: vec!["fix".into()],
            acceptance_criteria: vec!["tests pass".into()],
            risks: "none".into(),
            assumptions: "none".into(),
        };
        let args = json!({"summary":"The regression still fails", "completed_acceptance":[], "remaining_acceptance":["acceptance 1"], "acceptance_evidence":[], "next_steps":"Inspect the failed assertion in src/a.rs"});
        let call = |args: Value| ToolCallData {
            id: "report".into(),
            name: "done".into(),
            arguments: args.to_string(),
            thought_signature: None,
        };
        let messages = vec![crate::task_origin::user_message(
            "Repair src/a.rs. Run exactly: cargo test -q",
            crate::task_origin::MessageOrigin::User,
        )];
        let report = prepare(
            &[call(args.clone())],
            Some(&plan),
            &messages,
            None,
            &ObservationEvidence::default(),
        )
        .unwrap()
        .unwrap();
        assert!(report.render().starts_with("[BLOCKED"));
        assert!(!report.pending_exact_checks.is_empty());
        let mut bad = args.clone();
        bad["completed_acceptance"] = json!(["acceptance 1"]);
        assert!(prepare(
            &[call(bad)],
            Some(&plan),
            &messages,
            None,
            &ObservationEvidence::default()
        )
        .unwrap()
        .is_err());
        let mut done = args;
        done["remaining_acceptance"] = json!([]);
        assert!(prepare(
            &[call(done)],
            Some(&plan),
            &messages,
            None,
            &ObservationEvidence::default()
        )
        .is_none());
    }
}
