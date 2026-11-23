//! Episode termination is distinct from task completion and qualification.
use super::*;

/// Correlate the final provider error with a typed, current Coder wire receipt.
/// Other consumer or storage errors must still stop the chain.
#[derive(Debug)]
pub(super) struct CoderStreamLoss {
    pub request_id: usize,
    pub message: String,
}

impl std::fmt::Display for CoderStreamLoss {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for CoderStreamLoss {}

pub(super) fn lost_request(
    work: &crate::work_budget::WorkState,
    usage: &serde_json::Value,
) -> Option<usize> {
    let request = work.requests.last()?;
    (request.role == "coder"
        && request.purpose.is_none()
        && request.status == "stream_interrupted"
        && usage["status"] == request.status
        && request.stream_interruption.is_some()
        && usage["stream_interruption"] == json!(request.stream_interruption))
    .then_some(request.id)
}

#[derive(serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Termination {
    Completed,
    Blocked,
    BudgetExhausted,
    ObserverUnavailable,
    ReportIncomplete,
    Interrupted,
}

pub(super) fn project(app: &App, error: Option<&anyhow::Error>) -> Termination {
    if error.is_some() {
        return Termination::Interrupted;
    }
    if app.coder_completed == Some(true) {
        return Termination::Completed;
    }
    if app.coder_yield_reason == Some(crate::tui::coder_yield::CoderYieldReason::Blocked) {
        return Termination::Blocked;
    }
    if app.report_closeout_attempted {
        return Termination::ReportIncomplete;
    }
    if app.work_budget.remaining_for("coder") == 0 {
        return Termination::BudgetExhausted;
    }
    Termination::ObserverUnavailable
}
