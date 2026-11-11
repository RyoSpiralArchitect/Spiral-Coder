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

#[derive(Deserialize)]
struct Receipt {
    completed: bool,
    termination: String,
    work: WorkState,
    iteration_progress: Progress,
}

impl Receipt {
    fn stop(&self) -> Option<&str> {
        if self.completed {
            return Some("completed");
        }
        if self.iteration_progress.user_refusal {
            return Some("user_refusal");
        }
        // Accepted incomplete work can use the next predeclared allowance
        // without spending the last few requests just to exhaust this round.
        let may_advance = self.termination == "blocked"
            || (self.work.coder_remaining() == 0
                && matches!(
                    self.termination.as_str(),
                    "budget_exhausted" | "report_incomplete"
                ));
        if !may_advance {
            return Some(&self.termination);
        }
        if self.iteration_progress.changed_files.is_empty()
            && self.iteration_progress.newly_verified.is_empty()
            && self
                .iteration_progress
                .newly_reproduced_baselines
                .is_empty()
        {
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
        for reason in [
            "budget_exhausted",
            "report_incomplete",
            "observer_unavailable",
            "interrupted",
        ] {
            r.termination = reason.into();
            assert_eq!(r.stop(), Some(reason));
        }
        r.termination = "blocked".into();
        r.completed = true;
        assert_eq!(r.stop(), Some("completed"));
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
        let reason = if outcome.is_err() {
            Some("interrupted")
        } else {
            receipt
                .stop()
                .or_else(|| (number == args.iterations).then_some("iteration_limit"))
        };
        rows.push(json!({"iteration":number,"directory":name,"task_id":receipt.work.task_id,
            "parent_task_id":parent.as_ref().map(|w: &WorkState| &w.task_id),
            "termination":receipt.termination,"completed":receipt.completed,
            "task_requests":receipt.work.task_requests(),"wire_requests":receipt.work.requests.len(),"progress":receipt.iteration_progress}));
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
        args.seed_session = Some(output.join("session.json"));
        parent = Some(receipt.work);
    }
    println!(
        "{}",
        json!({"completed":record["completed"],"iterations":rows.len(),"task_requests":task_used,"wire_requests":wire_used,"manifest":manifest})
    );
    Ok(())
}
