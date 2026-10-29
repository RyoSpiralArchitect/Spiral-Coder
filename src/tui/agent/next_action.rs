//! A derived wire contract, not completion evidence or new persistent state.
use super::requirement_proof::Status;
use crate::streaming::ToolCallData;
use serde::Serialize;
use serde_json::{json, Value};

#[derive(Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum Phase {
    Work,
    Diagnose,
    Report,
}

#[derive(Debug, Serialize)]
pub(super) struct Contract {
    phase: Phase,
    coder_requests_remaining: Option<usize>,
    max_native_tool_calls: usize,
    pending_exact_checks: Vec<String>,
    pending_artifact_paths: Vec<String>,
    required_diagnostic_read: Option<super::recovery_focus::DiagnosticRead>,
}

impl Contract {
    pub(super) fn with_report_closeout(mut self, closeout: bool) -> Self {
        if closeout {
            self.phase = Phase::Report;
        }
        self
    }

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
            coder_requests_remaining: remaining,
            max_native_tool_calls: 1,
            pending_exact_checks: pending.to_vec(),
            pending_artifact_paths: requirements.pending_artifact_paths.clone(),
            required_diagnostic_read: None,
        }
    }

    pub(super) fn with_diagnostic_read(
        mut self,
        read: Option<super::recovery_focus::DiagnosticRead>,
    ) -> Self {
        if self.phase == Phase::Work && read.is_some() {
            self.phase = Phase::Diagnose;
        }
        self.required_diagnostic_read = read;
        self
    }

    pub(super) fn tools(&self, all: &Value) -> Value {
        if self.phase == Phase::Work {
            return all.clone();
        }
        let mut tools = json!(all
            .as_array()
            .expect("tool definitions are an array")
            .iter()
            .filter(|tool| tool["function"]["name"] == "done"
                || (self.phase == Phase::Diagnose && tool["function"]["name"] == "read_file"))
            .cloned()
            .collect::<Vec<_>>());
        if self.phase == Phase::Diagnose {
            let read = self
                .required_diagnostic_read
                .as_ref()
                .expect("diagnostic read");
            for tool in tools.as_array_mut().expect("tool array") {
                if tool["function"]["name"] == "read_file" {
                    tool["function"]["parameters"]["properties"]["path"]["enum"] =
                        json!([read.path]);
                }
            }
        }
        tools
    }

    pub(super) fn prompt(&self) -> String {
        let action = if self.phase == Phase::Report {
            "Only done is available now. Author the requested report from retained native evidence. If anything remains unverified, list it in remaining_acceptance, explain the blocker in summary, and supply the concrete next action in next_steps. This produces a BLOCKED terminal report and one Observer diagnosis, not a successful done. This restriction does not certify completion or bypass any gate. Do not claim unexecuted checks passed."
        } else if self.phase == Phase::Diagnose {
            "The current failed check requires a fresh read_file of required_diagnostic_read.path before any repair. Only that read and done are offered now. A read before this failure, another path, and Observer advice cannot discharge it. Read the exact path; it locates the failing assertion and is not permission to edit a supplied test. A successful native read reopens the work tools, while the failed check and pending artifacts remain unresolved. If the read fails, ordinary diagnosis tools reopen. If blocked, author done with remaining_acceptance, a blocker summary and concrete next_steps. No check is certified by reading."
        } else {
            "Choose exactly one native function call. Keep required plan/think/reflect/impact blocks in assistant text alongside that call; text or XML is not tool execution. pending_artifact_paths are required artifacts without a successful native edit, not a claim that all other behavior is correct. After diagnosis, address the remaining task artifacts before the final exact checks; preserve the user's edit restrictions. After the final mutation, execute each pending exact verification command before a successful done; automatic checks are not a substitute. Leave one Coder request for your authored done report. If blocked, use done with nonempty remaining_acceptance, a blocker summary, and concrete next_steps; this terminates incomplete without clearing obligations."
        };
        format!(
            "[Next action contract]\n{}\n{action}",
            serde_json::to_string(self).expect("contract serializes")
        )
    }

    pub(super) fn rejection(&self, calls: &[ToolCallData]) -> Option<String> {
        if self.phase == Phase::Diagnose {
            let read = self
                .required_diagnostic_read
                .as_ref()
                .expect("diagnostic read");
            let permitted = calls.len() == 1
                && (calls[0].name == "done"
                    || (calls[0].name == "read_file"
                        && serde_json::from_str::<Value>(&calls[0].arguments)
                            .ok()
                            .is_some_and(|args| args["path"] == read.path)));
            if !permitted {
                return Some(format!("[Diagnostic action] Read the current failure with read_file path={}. No proposed tool executed. Prior reads and Observer advice cannot replace this fresh observation. The failed check and required artifacts stay pending. If blocked, use done to report remaining work truthfully.", json!(read.path)));
            }
        }
        if self.phase == Phase::Report
            && (calls.is_empty() || calls.iter().any(|c| c.name != "done"))
        {
            Some("[Closeout action] Only a native done call is available for this request. No proposed tool executed. Report verified items and remaining work truthfully; the existing proof and acceptance gates still apply.".into())
        } else {
            None
        }
    }

    pub(super) fn rejection_gate(&self) -> &'static str {
        if self.phase == Phase::Diagnose {
            "diagnostic_action"
        } else {
            "closeout_action"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnosis_never_consumes_the_reserved_final_report_or_changes_proof() {
        let mut status = Status::default();
        status.declared = true;
        let read = || {
            Some(super::super::recovery_focus::DiagnosticRead {
                path: "tests/check.py".into(),
                failed_check: "python3 tests/check.py".into(),
            })
        };
        let pending = vec!["python3 tests/check.py".into()];
        let final_request =
            Contract::project(Some(1), &pending, &status).with_diagnostic_read(read());
        assert_eq!(final_request.phase, Phase::Report);
        let tools = json!([{"function":{"name":"read_file","parameters":{"properties":{"path":{"type":"string"}}}}}, {"function":{"name":"done"}}, {"function":{"name":"exec"}}]);
        assert_eq!(
            final_request.tools(&tools),
            json!([{"function":{"name":"done"}}])
        );
        let diagnostic = Contract::project(Some(3), &pending, &status).with_diagnostic_read(read());
        let call = |tool: &str, args: Value| ToolCallData {
            id: "request".into(),
            name: tool.into(),
            arguments: args.to_string(),
            thought_signature: None,
        };
        assert!(diagnostic
            .rejection(&[call("read_file", json!({"path":"docs/usage.md"}))])
            .is_some());
        assert!(diagnostic
            .rejection(&[call("exec", json!({"command":"python3 tests/check.py"}))])
            .is_some());
        assert!(diagnostic
            .rejection(&[call("read_file", json!({"path":"tests/check.py"}))])
            .is_none());
        assert!(diagnostic
            .rejection(&[call("done", json!({"remaining_acceptance":["check"]}))])
            .is_none());
        assert_eq!(diagnostic.pending_exact_checks, pending);
        assert_eq!(status.pending_baselines, Vec::<String>::new());
    }
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
