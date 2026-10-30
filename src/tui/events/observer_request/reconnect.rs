//! One explicit stream reconnect per next-action assist, charged to task work.
//! The interrupted text is retained for inspection, never reused as advice or
//! incorporated into the fresh request. Native generation and context guard it.
use super::*;

#[derive(Debug, Default)]
pub(super) struct State {
    active_prompt: String,
    used: bool,
    failed: bool,
    pending_request: Option<usize>,
}

impl State {
    pub(super) fn started(&mut self, prompt: &str) {
        self.active_prompt = prompt.to_owned();
        self.failed = false;
    }
}

fn has_continuation_allowance(app: &App) -> bool {
    app.work_budget.remaining_for("observer") >= 2 && app.work_budget.remaining_for("coder") > 0
}

/// True means this next-action failure is retained by its request owner.
/// Classification comes only from the correlated native receipt, not error text.
pub(in super::super) fn on_error(app: &mut App) -> bool {
    if !app.observer_next_action_mode || !request_is_current(app) || app.coder.streaming {
        return false;
    }
    let available = has_continuation_allowance(app);
    let work = app.work_budget.snapshot();
    let Some(request) = app.observer_assist_request.as_mut() else {
        return false;
    };
    app.pending_observer_hint = None;
    app.pending_observer_contract = None;
    app.last_observer_suggestion = None;
    request.pending_report_closeout = false;
    request.pending_repair = None;
    request.reconnect.failed = true;
    let interrupted = work.requests.last().filter(|r| {
        r.id >= request.request_cursor
            && r.role == "observer"
            && r.status == "stream_interrupted"
            && r.stream_interruption.is_some()
    });
    if let Some(receipt) =
        interrupted.filter(|_| available && !request.terminal_diagnosis && !request.reconnect.used)
    {
        request.reconnect.used = true;
        request.reconnect.pending_request = Some(receipt.id);
        app.observer.push_tool(format!("(next-action) interrupted stream retained as request {}. Queued one fresh Observer request (1/1) from the same native context; partial output is not forwarded. The interruption remains spent; recovery reserves another task request.", receipt.id));
    }
    true
}

pub(in super::super) fn blocks_automatic(app: &App) -> bool {
    request_is_current(app)
        && app
            .observer_assist_request
            .as_ref()
            .is_some_and(|r| r.reconnect.failed)
}

pub(in super::super) async fn maybe_start(app: &mut App, tx: &mpsc::Sender<RequestEvent>) -> bool {
    if app.coder.streaming || app.observer.streaming {
        return false;
    }
    if !request_is_current(app) {
        discard_pending_assist(app);
        return false;
    }
    let Some((parent, original)) = app.observer_assist_request.as_mut().and_then(|request| {
        request
            .reconnect
            .pending_request
            .take()
            .map(|parent| (parent, request.reconnect.active_prompt.clone()))
    }) else {
        return false;
    };
    if !has_continuation_allowance(app) {
        app.observer.push_tool("(next-action) stream recovery not started: insufficient allowance for Observer and Coder continuation; interruption retained.".into());
        return false;
    }
    if let Some(problem) = validate_pane_ready(app, PaneId::Observer) {
        app.observer.push_tool(problem);
        return false;
    }
    let budget = app.work_budget.with_observer_reconnect(parent);
    let work = app.work_budget.snapshot();
    let allowance = json!({
        "task_request_limit": work.request_limit,
        "task_requests_spent": work.task_requests(),
        "remaining_task_requests_before_this_request": app.work_budget.remaining_for("observer"),
        "remaining_coder_requests_before_this_request": app.work_budget.remaining_for("coder"),
        "reconnect_wire_limit": 1,
    });
    let prompt = format!("{original}\n\n[Observer stream recovery: attempt 1 of 1]\nThe previous delivery ended before a complete terminal response. Its partial text is not an action or evidence and is not included here. Produce a fresh complete JSON envelope from the unchanged native packet and existing validation feedback above. Preserve uncertainty; use no-tool clarify with empty args if no safe action follows. Do not assume that any proposed action executed.\n[Current request allowance]\nThis accounting supersedes earlier budget values in the packet; native evidence and obligations are unchanged. This request consumes one of the remaining task requests.\n{allowance}");
    start_request(
        app,
        tx,
        prompt,
        format!("[NEXT-ACTION RECONNECT] attempt=1/1 source_request={parent}"),
        budget,
    )
    .await;
    true
}
