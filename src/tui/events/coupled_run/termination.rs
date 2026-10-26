//! Episode termination is distinct from task completion and qualification.
use super::*;

#[derive(serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Termination {
    Completed,
    Blocked,
    BudgetExhausted,
    ObserverUnavailable,
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
    if app.work_budget.remaining_for("coder") == 0 {
        return Termination::BudgetExhausted;
    }
    Termination::ObserverUnavailable
}
