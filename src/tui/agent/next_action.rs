//! A derived wire contract, not completion evidence or new persistent state.
use super::requirement_proof::Status;
use crate::streaming::ToolCallData;
use serde::Serialize;
use serde_json::{json, Value};

#[derive(Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum Phase {
    Work,
    Report,
}

#[derive(Debug, Serialize)]
pub(super) struct Contract {
    phase: Phase,
    shared_requests_remaining: Option<usize>,
    max_native_tool_calls: usize,
    pending_exact_checks: Vec<String>,
}

impl Contract {
    pub(super) fn project(
        remaining: Option<usize>,
        pending: &[String],
        requirements: &Status,
    ) -> Self {
        let proof_ready = requirements.declared
            && requirements.pending.is_empty()
            && requirements.pending_baselines.is_empty()
            && pending.is_empty();
        // Undeclared interactive tasks may intentionally use one request for a
        // repair/resume step. Only explicit task contracts opt into reservation.
        let report =
            requirements.declared && remaining.is_some_and(|n| n == 1 || (n <= 3 && proof_ready));
        Self {
            phase: if report { Phase::Report } else { Phase::Work },
            shared_requests_remaining: remaining,
            max_native_tool_calls: 1,
            pending_exact_checks: pending.to_vec(),
        }
    }

    pub(super) fn tools(&self, all: &Value) -> Value {
        if self.phase == Phase::Work {
            return all.clone();
        }
        json!(all
            .as_array()
            .expect("tool definitions are an array")
            .iter()
            .filter(|tool| tool["function"]["name"] == "done")
            .cloned()
            .collect::<Vec<_>>())
    }

    pub(super) fn prompt(&self) -> String {
        let action = if self.phase == Phase::Report {
            "Only done is available now. Author the requested report from retained native evidence. If anything remains unverified, list it in remaining_acceptance and describe the exact next action. This restriction does not certify completion or bypass any gate. Do not claim unexecuted checks passed."
        } else {
            "Choose exactly one native function call. Keep required plan/think/reflect/impact blocks in assistant text alongside that call; text or XML is not tool execution. Leave one shared request for your authored done report after the final exact check."
        };
        format!(
            "[Next action contract]\n{}\n{action}",
            serde_json::to_string(self).expect("contract serializes")
        )
    }

    pub(super) fn rejection(&self, calls: &[ToolCallData]) -> Option<String> {
        if self.phase == Phase::Report
            && (calls.is_empty() || calls.iter().any(|c| c.name != "done"))
        {
            Some("[Closeout action] Only a native done call is available for this request. No proposed tool executed. Report verified items and remaining work truthfully; the existing proof and acceptance gates still apply.".into())
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_the_shared_cap_or_declared_current_proof_selects_report() {
        let all = json!([{"function":{"name":"exec"}}, {"function":{"name":"done"}}]);
        let mut status = Status::default();
        status.declared = true;
        for (remaining, checks, expected) in [
            (None, vec![], Phase::Work),
            (Some(4), vec![], Phase::Work),
            (Some(3), vec![], Phase::Report),
            (Some(3), vec!["test".into()], Phase::Work),
            (Some(1), vec!["test".into()], Phase::Report),
        ] {
            let contract = Contract::project(remaining, &checks, &status);
            assert_eq!(contract.phase, expected);
            assert_eq!(
                contract.tools(&all).as_array().unwrap().len(),
                if expected == Phase::Report { 1 } else { 2 }
            );
        }
        for missing in ["artifact", "baseline"] {
            let mut status = Status::default();
            status.declared = true;
            if missing == "artifact" {
                status.pending.push(missing.into());
            } else {
                status.pending_baselines.push(missing.into());
            }
            assert_eq!(Contract::project(Some(2), &[], &status).phase, Phase::Work);
        }
        assert_eq!(
            Contract::project(Some(2), &[], &Status::default()).phase,
            Phase::Work
        );
        assert_eq!(
            Contract::project(Some(1), &[], &Status::default()).phase,
            Phase::Work
        );
    }
}
