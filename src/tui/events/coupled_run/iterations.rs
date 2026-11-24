//! Explicitly budgeted successive episodes on one live workspace. Individual
//! sessions, ledgers and reports stay in their own output directories.
use super::*;
use crate::tui::agent::current_evidence::Freshness;
use crate::work_budget::{RequestCharge, WorkState};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;
use std::io::Write;
use std::path::Path;

pub(super) fn restart_prompt(work: &WorkState) -> String {
    format!(
        "[New iteration allowance]\nThis is a fresh, predeclared allowance for the SAME unfinished task: {} task requests, {} currently available to Coder, {} reserved for Observer. Prior blocked reports describe the previous iteration, not current budget exhaustion. Use the retained next step and current failure evidence to choose one concrete inspection, repair, or exact verification now. Do not repeat the old blocked report solely because the previous allowance ended. If a current approval barrier, user refusal, or genuine external blocker prevents work, report it honestly; no permission or completion rule is relaxed. The original task and acceptance conditions remain unchanged.",
        work.request_limit, work.coder_remaining(), work.policy.observer_reserved_requests
    )
}

pub(super) fn last_coder_report(app: &App) -> bool {
    let budget = app.work_budget.snapshot();
    if budget.request_limit.saturating_sub(budget.task_requests()) != 1
        || budget.coder_remaining() != 1
    {
        return false;
    }
    let observer_turns = budget
        .requests
        .iter()
        .filter(|r| {
            r.role == "observer" && r.charge == RequestCharge::Task && r.status == "completed"
        })
        .count();
    observer_turns >= budget.policy.observer_reserved_requests.max(1)
        && app.coder_session.as_ref().is_some_and(|s| {
            agent::task_obligations::project(&s.messages, app.project_test_cmd.as_deref())
                .task_requirements
                .declared
        })
}

pub(super) async fn send_last_report(
    app: &mut App,
    tx: &mpsc::Sender<RequestEvent>,
    trace: &crate::trace_writer::TraceWriter,
) -> Result<bool> {
    if !last_coder_report(app) {
        return Ok(false);
    }
    trace.event(
        "reserved_coder_report",
        json!({"remaining_shared":1,"remaining_coder":1}),
    )?;
    send_coder_input(app,tx,"Use your last turn for the native done report. If anything is unfinished, list the outstanding criteria and the first concrete action for a later iteration.".into(),CoderInput::ReportCloseout).await;
    Ok(true)
}

#[derive(Serialize, Deserialize, Default)]
pub(super) struct Progress {
    changed_files: BTreeSet<String>,
    newly_verified: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    newly_reproduced_baselines: BTreeSet<String>,
    user_refusal: bool,
}

pub(super) fn progress(
    evidence: Option<&agent::evidence_packet::ExecutionPacket>,
    old_calls: &BTreeSet<String>,
    old_checks: &BTreeSet<String>,
    required: &[String],
    newly_reproduced_baselines: BTreeSet<String>,
) -> Progress {
    use crate::execution_evidence::FileEditOutcome;
    let mut out = Progress {
        newly_reproduced_baselines,
        ..Progress::default()
    };
    let Some(evidence) = evidence else { return out };
    out.user_refusal = evidence.has_approval_barrier();
    for a in evidence
        .actions
        .iter()
        .filter(|a| !old_calls.contains(&a.call_id) && a.executed == Some(true))
    {
        if a.edit_outcome == Some(FileEditOutcome::Changed) {
            if let Some(path) = a.arguments["path"].as_str() {
                out.changed_files.insert(path.to_owned());
            }
        } else if a.tool == "exec" && a.succeeded == Some(true) && a.freshness == Freshness::Current
        {
            if let Some(command) = a.arguments["command"].as_str() {
                if !old_checks.contains(command) && required.iter().any(|c| c == command) {
                    out.newly_verified.insert(command.to_owned());
                }
            }
        }
    }
    out
}

impl Progress {
    /// A new changed file, newly verified declared command or first reproduced
    /// baseline lets an incomplete round advance instead of ending the run.
    fn advances(&self) -> bool {
        !self.changed_files.is_empty()
            || !self.newly_verified.is_empty()
            || !self.newly_reproduced_baselines.is_empty()
    }
}

/// The calls and current successful checks recorded before a round starts.
/// Only later calls can count as the round's progress.
pub(super) fn round_baseline(
    messages: &[Value],
    test_command: Option<&str>,
) -> (BTreeSet<String>, BTreeSet<String>) {
    let mut calls = BTreeSet::new();
    let mut checks = BTreeSet::new();
    for action in agent::evidence_packet::project(messages, test_command).actions {
        if action.tool == "exec"
            && action.succeeded == Some(true)
            && action.freshness == Freshness::Current
        {
            if let Some(command) = action.arguments["command"].as_str() {
                checks.insert(command.to_owned());
            }
        }
        calls.insert(action.call_id);
    }
    (calls, checks)
}

/// The round's progress so far, against the calls and current checks recorded
/// before it started.
pub(super) fn round_progress(
    messages: &[Value],
    obligations: &agent::task_obligations::TaskObligations,
    evidence: Option<&agent::evidence_packet::ExecutionPacket>,
    root: &str,
    old_calls: &BTreeSet<String>,
    old_checks: &BTreeSet<String>,
) -> Progress {
    let baselines = evidence
        .map(|packet| {
            obligations
                .task_requirements
                .new_test_baselines(messages, packet, root, old_calls)
        })
        .unwrap_or_default();
    progress(
        evidence,
        old_calls,
        old_checks,
        &obligations.required_exact_checks,
        baselines,
    )
}

/// An accepted blocked report in a round without progress stops the chain.
/// The driver returns the first such report as work when the report names its
/// next step, no approval barrier is recorded, and the Coder can still afford
/// one edit, every required check after it and a report. The returned turn
/// must also be a work turn: with complete proof, three or fewer requests make
/// the next Coder turn report-only. A second blocked report in the same round
/// ends it as before.
pub(super) fn blocked_report_return(
    app: &App,
    root: &str,
    old_calls: &BTreeSet<String>,
    old_checks: &BTreeSet<String>,
) -> Option<String> {
    let messages = &app.coder_session.as_ref()?.messages;
    let test_command = app.project_test_cmd.as_deref();
    let obligations = agent::task_obligations::project(messages, test_command);
    let next_steps = obligations.previous_blocked_report.as_ref()?["next_steps"]
        .as_str()
        .map(str::trim)
        .filter(|steps| !steps.is_empty())?;
    let remaining = app.work_budget.remaining_for("coder");
    if remaining < obligations.required_exact_checks.len() + 2
        || obligations.coder_turn(remaining) != agent::task_obligations::CoderTurn::Work
    {
        return None;
    }
    let evidence = agent::evidence_packet::project(messages, test_command);
    let progress = round_progress(
        messages,
        &obligations,
        Some(&evidence),
        root,
        old_calls,
        old_checks,
    );
    (!progress.user_refusal && !progress.advances()).then(|| format!(
        "[Blocked report returned]\nYour blocked report is recorded, but this round has no changed file or newly passed required check yet. A round that ends this way stops the whole run, and {remaining} Coder requests remain in this one. Take the first concrete step of your own next steps now:\n{next_steps}\nIf an approval barrier, a user refusal or a genuine external blocker prevents every edit and check, report again; that report ends the round. The task, its checks and its approvals are unchanged."
    ))
}

#[derive(Deserialize)]
struct Receipt {
    completed: bool,
    termination: String,
    work: WorkState,
    iteration_progress: Progress,
}

impl Receipt {
    fn has_progress(&self) -> bool {
        self.iteration_progress.advances()
    }

    fn interrupted_boundary(&self, error: &anyhow::Error) -> Option<usize> {
        let loss = error.downcast_ref::<termination::CoderStreamLoss>()?;
        let last = self.work.requests.last()?;
        (self.termination == "interrupted"
            && !self.completed
            && !self.iteration_progress.user_refusal
            && self.has_progress()
            && self.work.task_requests() == self.work.request_limit
            && last.id == loss.request_id
            && last.charge == RequestCharge::Task
            && last.role == "coder"
            && last.purpose.is_none()
            && last.status == "stream_interrupted"
            && last.stream_interruption.is_some())
        .then_some(last.id)
    }

    fn stop(&self) -> Option<&str> {
        if self.completed {
            return Some("completed");
        }
        if self.iteration_progress.user_refusal {
            return Some("user_refusal");
        }
        // Accepted incomplete work can use the next predeclared allowance
        // without spending the last few requests just to exhaust this round.
        // A report-only closeout is the iteration's final Coder turn: once its
        // report is rejected nothing more runs in this iteration, although the
        // rejected attempt is charged as transport and the ledger can still
        // show Coder allowance (S29: the reserved last turn left one request).
        let may_advance = matches!(self.termination.as_str(), "blocked" | "report_incomplete")
            || (self.work.coder_remaining() == 0 && self.termination == "budget_exhausted");
        if !may_advance {
            return Some(&self.termination);
        }
        if !self.has_progress() {
            return Some("no_new_edit_or_verification");
        }
        None
    }
}

#[cfg(test)]
mod continuation_tests {
    use super::*;

    fn receipt() -> Receipt {
        Receipt {
            completed: false,
            termination: "blocked".into(),
            work: crate::work_budget::WorkBudget::new("task", 32, None).snapshot(),
            iteration_progress: Progress {
                changed_files: BTreeSet::from(["source.rs".into()]),
                ..Progress::default()
            },
        }
    }

    #[test]
    fn early_blocked_continuation_needs_progress_and_no_approval_barrier() {
        let mut r = receipt();
        assert_ne!(r.work.coder_remaining(), 0);
        assert_eq!(r.stop(), None);
        r.iteration_progress.user_refusal = true;
        assert_eq!(r.stop(), Some("user_refusal"));
        r.iteration_progress = Progress::default();
        assert_eq!(r.stop(), Some("no_new_edit_or_verification"));
        r.iteration_progress.newly_verified.insert("test".into());
        assert_eq!(r.stop(), None);
        r.iteration_progress = Progress::default();
        r.iteration_progress
            .newly_reproduced_baselines
            .insert("test".into());
        assert_eq!(r.stop(), None);
    }

    #[test]
    fn early_blocked_exception_does_not_resume_other_terminations_or_completion() {
        let mut r = receipt();
        for reason in ["budget_exhausted", "observer_unavailable", "interrupted"] {
            r.termination = reason.into();
            assert_eq!(r.stop(), Some(reason));
        }
        r.termination = "blocked".into();
        r.completed = true;
        assert_eq!(r.stop(), Some("completed"));
    }

    #[test]
    fn advancing_after_stream_loss_requires_the_matching_ordinary_last_request_and_progress() {
        use crate::work_budget::{RequestPurpose, StreamInterruption, WorkBudget};
        let budget = WorkBudget::new("task", 1, None);
        budget
            .begin("coder", &Value::Null)
            .unwrap()
            .interrupted(StreamInterruption::ReadFailed);
        let mut r = receipt();
        r.work = budget.snapshot();
        r.termination = "interrupted".into();
        let error = |id| {
            anyhow::Error::new(termination::CoderStreamLoss {
                request_id: id,
                message: "wire closed".into(),
            })
        };
        assert_eq!(r.interrupted_boundary(&error(0)), Some(0));
        assert_eq!(r.interrupted_boundary(&error(1)), None);
        assert_eq!(
            r.interrupted_boundary(&anyhow::anyhow!("wire closed")),
            None
        );
        r.work.request_limit = 2;
        assert_eq!(r.interrupted_boundary(&error(0)), None);
        r.work.request_limit = 1;
        for purpose in [
            RequestPurpose::CoderReconnect,
            RequestPurpose::ReportCloseout,
        ] {
            r.work.requests[0].purpose = Some(purpose);
            assert_eq!(r.interrupted_boundary(&error(0)), None);
        }
        r.work.requests[0].purpose = None;
        r.iteration_progress.user_refusal = true;
        assert_eq!(r.interrupted_boundary(&error(0)), None);
        r.iteration_progress = Progress::default();
        assert_eq!(r.interrupted_boundary(&error(0)), None);
        r.iteration_progress.newly_verified.insert("test".into());
        let usage = json!({"status":"stream_interrupted","stream_interruption":"read_failed"});
        assert_eq!(termination::lost_request(&r.work, &usage), Some(0));
        assert_eq!(
            termination::lost_request(&r.work, &json!({"status":"stream_interrupted"})),
            None
        );
        r.work.requests[0].role = "observer".into();
        assert_eq!(termination::lost_request(&r.work, &usage), None);
        r.work.requests[0].role = "coder".into();
        r.work.requests[0].status = "http_429".into();
        assert_eq!(termination::lost_request(&r.work, &usage), None);
        assert_eq!(r.interrupted_boundary(&error(0)), None);
    }

    #[test]
    fn a_rejected_report_only_closeout_advances_only_with_progress() {
        // Sealed S29 iteration 1: 31 of 32 task requests (8 Observer), then the
        // reserved last-turn report was rejected. That attempt is charged as
        // transport, so the ledger still shows one Coder request, and the chain
        // stopped with five predeclared iterations unused.
        use crate::work_budget::{BudgetPolicy, TransportRejection, WorkBudget};
        let budget = WorkBudget::new("task", 32, None);
        budget
            .configure(BudgetPolicy {
                observer_reserved_requests: 8,
                transport_repair_limit: 4,
            })
            .unwrap();
        for role in std::iter::repeat_n("coder", 23).chain(std::iter::repeat_n("observer", 8)) {
            budget
                .begin(role, &Value::Null)
                .unwrap()
                .status("completed");
        }
        let closeout = budget.with_report_closeout();
        closeout
            .begin("coder", &Value::Null)
            .unwrap()
            .status("completed");
        closeout
            .reject_transport("coder", 31, TransportRejection::GovernorBlock)
            .unwrap();
        let mut r = receipt();
        r.termination = "report_incomplete".into();
        r.work = budget.snapshot();
        assert_eq!(r.work.coder_remaining(), 1);
        assert_eq!(r.stop(), None);
        r.iteration_progress = Progress::default();
        assert_eq!(r.stop(), Some("no_new_edit_or_verification"));
        r.iteration_progress.user_refusal = true;
        assert_eq!(r.stop(), Some("user_refusal"));
    }
}

#[cfg(test)]
mod blocked_report_return_tests {
    use super::*;

    const CHECK: &str = "python3 -B -S check.py";

    fn call(id: &str, tool: &str, args: Value, result: &str) -> [Value; 2] {
        [
            json!({"role":"assistant","tool_calls":[{"id":id,"type":"function","function":{"name":tool,"arguments":args.to_string()}}]}),
            json!({"role":"tool","tool_call_id":id,"content":result}),
        ]
    }

    /// Trial U, round 5: earlier rounds edited every artifact and passed the
    /// exact check. This round only read and then reported blocked, although
    /// its own next step was a test edit. With `stale`, an earlier round also
    /// edited the source after its last check, so the check is pending again.
    fn round(
        stale: bool,
        calls: Vec<[Value; 2]>,
        next_steps: &str,
        coder_left: usize,
    ) -> (App, tempfile::TempDir) {
        let (mut app, dir) = crate::tui::events::tests::interaction_app();
        let mut messages = crate::tui::events::tests::closeout_messages();
        let task = crate::task_origin::root_user_text(&messages).to_owned();
        if stale {
            messages.extend(call(
                "late-edit",
                "patch_file",
                json!({"path":"source.py"}),
                "OK: patched 'source.py'",
            ));
        }
        messages.extend(calls.into_iter().flatten());
        let report = json!({"summary":"The authored tests are still missing.","completed_acceptance":[],"remaining_acceptance":["acceptance 1"],"acceptance_evidence":[],"next_steps":next_steps});
        messages.extend([
            json!({"role":"assistant","tool_calls":[{"id":"report","type":"function","function":{"name":"done","arguments":report.to_string()}}]}),
            json!({"role":"tool","tool_call_id":"report","content":"OK: blocked","blocked_report":{"remaining_acceptance":["acceptance 1"],"next_steps":next_steps}}),
        ]);
        app.project_test_cmd = Some(CHECK.into());
        app.coder_session = Some(crate::agent_session::AgentSession::new(
            None, None, None, None, messages,
        ));
        app.work_budget = crate::work_budget::WorkBudget::new(&task, coder_left, None);
        (app, dir)
    }

    fn returned(app: &App, dir: &tempfile::TempDir, old_checks: &[&str]) -> Option<String> {
        let old_calls = ["source-edit", "docs-edit", "current-check", "late-edit"]
            .map(String::from)
            .into();
        let old_checks = old_checks.iter().map(|c| c.to_string()).collect();
        blocked_report_return(app, dir.path().to_str().unwrap(), &old_calls, &old_checks)
    }

    #[test]
    fn a_blocked_report_that_would_stop_the_run_returns_its_own_next_step() {
        let read = || {
            call(
                "read",
                "read_file",
                json!({"path":"source.py"}),
                "[source.py] (1 lines, 9 bytes)\nfixed()\n",
            )
        };
        let next = "Add a test for the empty input in tests/test_source.py.";
        let (app, dir) = round(false, vec![read()], next, 4);
        let text = returned(&app, &dir, &[CHECK]).expect("the run would stop");
        assert!(text.starts_with("[Blocked report returned]\n"), "{text}");
        assert!(
            text.contains(next) && text.contains("4 Coder requests remain"),
            "{text}"
        );
        // Complete proof turns three remaining requests into a report-only
        // turn, which could not act on the returned step.
        let (app, dir) = round(false, vec![read()], next, 3);
        assert_eq!(returned(&app, &dir, &[CHECK]), None, "a report-only turn");
        // With the check pending again, three requests are a work turn: one
        // edit, the required check and the report. Two are not enough.
        let (app, dir) = round(true, vec![read()], next, 3);
        assert!(returned(&app, &dir, &[]).is_some(), "a pending check");
        let (app, dir) = round(true, vec![read()], next, 2);
        assert_eq!(
            returned(&app, &dir, &[]),
            None,
            "below one edit, the check and a report"
        );
        let (app, dir) = round(false, vec![read()], "  ", 4);
        assert_eq!(
            returned(&app, &dir, &[CHECK]),
            None,
            "no next step to return"
        );
        // Progress lets the round advance, so its report is not returned.
        let edit = call(
            "round-edit",
            "patch_file",
            json!({"path":"source.py"}),
            "OK: patched 'source.py'",
        );
        let (app, dir) = round(false, vec![read(), edit], next, 4);
        assert_eq!(returned(&app, &dir, &[CHECK]), None, "a changed file");
        let check = call(
            "round-check",
            "exec",
            json!({"command":CHECK}),
            "OK (exit_code: 0)",
        );
        let (app, dir) = round(false, vec![read(), check.clone()], next, 4);
        assert_eq!(returned(&app, &dir, &[]), None, "a newly verified check");
        let (app, dir) = round(false, vec![read(), check], next, 4);
        assert!(
            returned(&app, &dir, &[CHECK]).is_some(),
            "a recheck is not progress"
        );
        let denied = call(
            "denied",
            "exec",
            json!({"command":"verify"}),
            "REJECTED BY USER",
        );
        let (app, dir) = round(false, vec![read(), denied], next, 4);
        assert_eq!(returned(&app, &dir, &[CHECK]), None, "an approval barrier");
    }
}

fn save(path: &Path, value: &Value) -> Result<()> {
    let mut file = tempfile::NamedTempFile::new_in(path.parent().context("manifest parent")?)?;
    serde_json::to_writer_pretty(&mut file, value)?;
    file.flush()?;
    file.persist(path)?;
    Ok(())
}

pub(super) async fn run(mut args: Args, roles: roles::ResolvedRoles) -> Result<()> {
    let directory = args.out_dir.clone();
    let manifest = directory.join("iterations.json");
    let mut parent = None;
    let mut rows = Vec::new();
    let mut task_used = 0;
    let mut wire_used = 0;
    let mut record = json!({"version":1,"max_iterations":args.iterations,"max_task_requests_per_iteration":args.max_requests,
        "max_task_requests":args.iterations as usize * args.max_requests,
        "max_wire_requests":args.iterations as usize * (args.max_requests + args.transport_repair_requests),
        "completed":false,"stop_reason":null,"iterations":[],"task_requests":0,"wire_requests":0});
    // The task an operator authorizes at launch stays fixed across every round.
    let original = std::fs::read(&args.prompt_file)?;
    args.prompt_file = directory.join("task.txt");
    std::fs::write(&args.prompt_file, &original)?;
    for number in 1..=args.iterations {
        if std::fs::read(&args.prompt_file)? != original {
            record["stop_reason"] = json!("original_prompt_changed");
            save(&manifest, &record)?;
            anyhow::bail!("original prompt changed between iterations");
        }
        let name = format!("iteration-{number:02}");
        let output = directory.join(&name);
        let storage = crate::runtime_storage::RuntimeStorage::create(&args.tool_root, &output)?
            .with_private_env_names(args.observer.api_key_env.clone());
        args.out_dir = output.clone();
        record["active_iteration"] = json!(name);
        save(&manifest, &record)?;
        let outcome = crate::runtime_storage::scope(
            Some(storage),
            run_iteration(args.clone(), roles.clone(), parent.as_ref()),
        )
        .await;
        // If startup failed, preserve any already-spent durable wire receipts.
        let receipt = match std::fs::read(output.join("report.json")) {
            Ok(data) => serde_json::from_slice::<Receipt>(&data)?,
            Err(error) => {
                // Finished-round totals do not claim zero usage for an incomplete ledger.
                let durable = std::fs::read(output.join("state/tui-work.json"))
                    .ok()
                    .and_then(|data| serde_json::from_slice::<WorkState>(&data).ok());
                record["interrupted_iteration"] = json!({"directory":name,
                    "task_requests":durable.as_ref().map(WorkState::task_requests),
                    "wire_requests":durable.as_ref().map(|w| w.requests.len())});
                record["task_requests"] =
                    json!(durable.as_ref().map(|w| task_used + w.task_requests()));
                record["wire_requests"] =
                    json!(durable.as_ref().map(|w| wire_used + w.requests.len()));
                record["active_iteration"] = Value::Null;
                record["stop_reason"] = json!("interrupted");
                save(&manifest, &record)?;
                outcome?;
                return Err(error.into());
            }
        };
        task_used += receipt.work.task_requests();
        wire_used += receipt.work.requests.len();
        let session = output.join("session.json");
        let boundary = outcome
            .as_ref()
            .err()
            .filter(|_| number < args.iterations && session.is_file())
            .and_then(|error| receipt.interrupted_boundary(error));
        let reason = if boundary.is_some() {
            None
        } else if outcome.is_err() {
            Some("interrupted")
        } else {
            receipt
                .stop()
                .or_else(|| (number == args.iterations).then_some("iteration_limit"))
        };
        let mut row = json!({"iteration":number,"directory":name,"task_id":receipt.work.task_id,
            "parent_task_id":parent.as_ref().map(|w: &WorkState| &w.task_id),
            "termination":receipt.termination,"completed":receipt.completed,
            "task_requests":receipt.work.task_requests(),"wire_requests":receipt.work.requests.len(),"progress":receipt.iteration_progress});
        if let Some(id) = boundary {
            row["continued_after_coder_stream_request"] = json!(id);
        }
        rows.push(row);
        record["iterations"] = json!(rows);
        record["completed"] = json!(receipt.completed);
        record["stop_reason"] = json!(reason);
        record["active_iteration"] = Value::Null;
        record["task_requests"] = json!(task_used);
        record["wire_requests"] = json!(wire_used);
        save(&manifest, &record)?;
        if reason.is_some() {
            outcome?;
            break;
        }
        args.follow_up_session = None;
        args.seed_session = Some(session);
        parent = Some(receipt.work);
    }
    println!(
        "{}",
        json!({"completed":record["completed"],"iterations":rows.len(),"task_requests":task_used,"wire_requests":wire_used,"manifest":manifest})
    );
    Ok(())
}
