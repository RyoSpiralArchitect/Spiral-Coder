//! Bounded headless driver of the actual TUI Coder/Observer consumers.
//! Roles share one budget; fixtures/oracles stay outside this driver.
use super::*;
use anyhow::{Context, Result};

mod roles;
mod termination;
#[cfg(test)]
mod tests;

#[derive(clap::Args, Debug)]
pub(crate) struct Args {
    #[arg(long)]
    pub tool_root: PathBuf,
    #[arg(long)]
    pub prompt_file: PathBuf,
    #[arg(long)]
    pub out_dir: PathBuf,
    #[arg(long, default_value_t = 18)]
    pub max_requests: usize,
    /// Pure rejected framing/schema responses may spend this separate pool.
    #[arg(long, default_value_t = 0)]
    pub transport_repair_requests: usize,
    /// Task requests unavailable to Coder, distributed across role slices.
    #[arg(long, default_value_t = 0)]
    pub observer_reserved_requests: usize,
    #[arg(long)]
    pub seed_session: Option<PathBuf>,
    #[arg(long)]
    pub without_observer: bool,
    #[command(flatten)]
    pub observer: roles::ObserverArgs,
    /// Upper bound for each Coder slice; leaves Observer + continuation budget.
    #[arg(long)]
    pub coder_chunk_requests: Option<usize>,
    #[arg(long)]
    pub case_spec: Option<PathBuf>,
    /// Re-evaluate existing receipts without a provider call.
    #[arg(long)]
    pub evaluate_only: bool,
}

async fn drain(
    app: &mut App,
    rx: &mut mpsc::Receiver<RequestEvent>,
    pane: PaneId,
    trace: &crate::trace_writer::TraceWriter,
) -> Result<()> {
    loop {
        let event = rx
            .recv()
            .await
            .context("request ended without a terminal event")?;
        if !request_stream::is_current(&event, app, pane) {
            continue;
        }
        let terminal = matches!(
            &event.payload,
            request_stream::Payload::Token(StreamToken::Done | StreamToken::Error(_))
        );
        if let request_stream::Payload::Token(token) = &event.payload {
            match token {
                StreamToken::Telemetry(event) => trace.event(&event.event, event.data.clone())?,
                StreamToken::ToolCall(call) if matches!(pane, PaneId::Coder) => trace.event(
                    "tool_call",
                    json!({"id":call.id,"name":call.name,"arguments":call.arguments}),
                )?,
                StreamToken::Error(error) => {
                    trace.event("error", json!({"message":error,"pane":format!("{pane:?}")}))?
                }
                _ => {}
            }
        }
        let error = match &event.payload {
            request_stream::Payload::Token(StreamToken::Error(error)) => Some(error.clone()),
            _ => None,
        };
        request_stream::dispatch(event, app, pane);
        if let Some(error) = error {
            anyhow::bail!("{pane:?}: {error}");
        }
        if terminal {
            break;
        }
    }
    Ok(())
}

/// The UI and headless driver consume the same pending reconnect and schema
/// repair. Every retry remains explicit and a failed reconnect is still fatal.
async fn drain_observer_assist(
    app: &mut App,
    tx: &mpsc::Sender<RequestEvent>,
    rx: &mut mpsc::Receiver<RequestEvent>,
    trace: &crate::trace_writer::TraceWriter,
) -> Result<()> {
    loop {
        if app.observer.streaming {
            if let Err(error) = drain(app, rx, PaneId::Observer, trace).await {
                if observer_request::reconnect::maybe_start(app, tx).await {
                    continue;
                }
                return Err(error);
            }
        }
        maybe_observer_action_repair(app, tx).await;
        if !app.observer.streaming {
            return Ok(());
        }
    }
}

pub(crate) async fn run(mut args: Args, common: crate::CommonArgs) -> Result<()> {
    if args.evaluate_only {
        let report = evaluate_receipts(&args, 0)?;
        println!("{}", serde_json::to_string(&report)?);
        return Ok(());
    }
    anyhow::ensure!(
        (1..=64).contains(&args.max_requests),
        "max-requests must be 1..64"
    );
    anyhow::ensure!(
        args.transport_repair_requests <= 8,
        "transport-repair-requests must be 0..8"
    );
    anyhow::ensure!(
        args.observer_reserved_requests < args.max_requests,
        "Observer reservation must leave a Coder request"
    );
    anyhow::ensure!(
        !args.without_observer || args.observer_reserved_requests == 0,
        "Observer reservation conflicts with --without-observer"
    );
    if let Some(limit) = args.coder_chunk_requests {
        anyhow::ensure!(
            !args.without_observer,
            "--coder-chunk-requests conflicts with --without-observer"
        );
        anyhow::ensure!(
            limit >= 4 && limit <= args.max_requests.saturating_sub(2),
            "coder-chunk-requests must be at least 4 and leave at least 2 shared requests"
        );
    }
    let roles = roles::resolve(
        common.to_partial_config().resolve()?,
        &args.observer,
        args.without_observer,
    )?;
    let storage = crate::runtime_storage::RuntimeStorage::create(&args.tool_root, &args.out_dir)?
        .with_private_env_names(args.observer.api_key_env.clone());
    args.out_dir = storage.output().to_owned();
    crate::runtime_storage::scope(Some(storage), run_owned(args, roles)).await
}

async fn run_owned(args: Args, roles: roles::ResolvedRoles) -> Result<()> {
    let root = args
        .tool_root
        .canonicalize()?
        .to_string_lossy()
        .into_owned();
    let prompt = std::fs::read_to_string(&args.prompt_file)?;
    let cfg = roles.coder.clone();
    let obs_cfg = roles.observer.clone().unwrap_or_else(|| {
        let mut observer = cfg.clone();
        observer.mode = crate::modes::Mode::Observer;
        observer
    });
    let mut app = App::new(
        cfg.clone(),
        obs_cfg,
        cfg.clone(),
        Some(root.clone()),
        Some(root.clone()),
        false,
        "en".into(),
        Some(args.max_requests),
    );
    app.coder_realize_preset = agent::RealizePreset::Off;
    app.coder_chunk_requests = args.coder_chunk_requests;
    app.work_policy = crate::work_budget::BudgetPolicy {
        transport_repair_limit: args.transport_repair_requests,
        observer_reserved_requests: args.observer_reserved_requests,
    };
    app.observer_assist_max_tokens = roles.observer_max_tokens;
    if args.without_observer {
        app.coder_chunk_reserve = 0;
    }
    let trace = crate::trace_writer::TraceWriter::new(args.out_dir.join("trace.jsonl"))?;
    trace.event("agent_config", json!({"provider":cfg.provider.to_string(),"model":cfg.model,"coupled":!args.without_observer,"max_requests":args.max_requests,"roles":roles.receipts(),"same_model_both_roles":roles.same_model_both_roles(),"coder_chunk_requests":args.coder_chunk_requests,"budget_policy":app.work_policy}))?;
    let started = std::time::Instant::now();
    let (coder_tx, mut coder_rx) = mpsc::channel(256);
    let (observer_tx, mut observer_rx) = mpsc::channel(256);
    if let Some(seed) = &args.seed_session {
        let mut session = crate::agent_session::AgentSession::load(&seed)?;
        anyhow::ensure!(
            crate::task_origin::root_user_text(&session.messages) == prompt.trim_end(),
            "seed task must match prompt exactly"
        );
        session.tool_root = Some(root.clone());
        session.cur_cwd = Some(root.clone());
        session.repair_for_resume();
        app.coder_session = Some(session);
        app.work_budget = crate::work_budget::WorkBudget::new(
            prompt.trim_end(),
            args.max_requests,
            Some(crate::runtime_storage::state_path(&root, "tui-work.json")),
        );
        app.work_budget.configure(app.work_policy)?;
        send_coder_input(
            &mut app,
            &coder_tx,
            "Continue.".into(),
            CoderInput::RuntimeContinuation,
        )
        .await;
    } else {
        send_coder_with_text(&mut app, &coder_tx, prompt).await;
    }
    trace.event(
        "verification_config",
        json!({"test_command":app.project_test_cmd}),
    )?;
    // Provider/consumer failures still need the budget, session and failure receipt.
    let outcome: Result<()> = async {
        while app.coder.streaming {
            drain(&mut app, &mut coder_rx, PaneId::Coder, &trace).await?;
            if app.coder_completed == Some(true)
                || app.report_closeout_attempted
                || args.without_observer
                || app.work_budget.remaining_for("observer") == 0
            {
                break;
            }
            let blocked = app.coder_yield_reason
                == Some(super::super::coder_yield::CoderYieldReason::Blocked);
            let idx = app
                .coder
                .messages
                .iter()
                .rposition(|m| m.role == Role::Assistant && m.complete)
                .context("no completed Coder response")?;
            send_next_action_assist(
                &mut app,
                &observer_tx,
                &format!("msg:coder-{idx}"),
                if blocked {
                    "blocked_terminal_diagnosis"
                } else {
                    "incomplete_task"
                },
            )
            .await;
            drain_observer_assist(&mut app, &observer_tx, &mut observer_rx, &trace).await?;
            if !blocked && observer_request::maybe_report_closeout(&mut app, &coder_tx).await {
                continue;
            }
            if blocked
                || app.pending_observer_hint.is_none()
                || app.work_budget.remaining_for("coder") == 0
            {
                break;
            }
            send_coder_input(
                &mut app,
                &coder_tx,
                "Continue.".into(),
                CoderInput::RuntimeContinuation,
            )
            .await;
        }
        Ok(())
    }
    .await;
    trace.event("agent_end", json!({"ok":outcome.is_ok()}))?;
    if let Some(session) = &app.coder_session {
        crate::agent_session::AgentSession::save_atomic(
            &args.out_dir.join("session.json"),
            session,
        )?;
    }
    let report = json!({
        "version":1,"completed":outcome.is_ok() && app.coder_completed == Some(true),
        "termination":termination::project(&app, outcome.as_ref().err()),
        "roles":roles.receipts(),"same_model_both_roles":roles.same_model_both_roles(),
        "coder_chunk_requests":args.coder_chunk_requests,
        "report_closeout_attempted":app.report_closeout_attempted,
        "budget_policy":app.work_policy,
        "role_requests":{
            "coder":app.work_budget.snapshot().requests.iter().filter(|r| r.role == "coder").count(),
            "observer":app.work_budget.snapshot().requests.iter().filter(|r| r.role == "observer").count(),
        },
        "runtime_storage":{"state":"state","python_bytecode":"cache/python","child_tui_replays":"replays","relative_to":"out_dir"},
        "error":outcome.as_ref().err().map(ToString::to_string), "wall_elapsed_ms":started.elapsed().as_millis(),
        "work":app.work_budget.snapshot(),
        "execution":app.coder_session.as_ref().map(|s| agent::evidence_packet::project(&s.messages, app.project_test_cmd.as_deref())),
        "coder_display":app.coder.messages.iter().map(|m| json!({"role":format!("{:?}",m.role),"content":m.content})).collect::<Vec<_>>(),
        "observer_display":app.observer.messages.iter().map(|m| json!({"role":format!("{:?}",m.role),"content":m.content,"raw_content":m.raw_content})).collect::<Vec<_>>(),
    });
    std::fs::write(
        args.out_dir.join("report.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    if args.case_spec.is_some() {
        let checks = evaluate_receipts(&args, started.elapsed().as_millis())?;
        std::fs::write(
            args.out_dir.join("checks.json"),
            serde_json::to_vec_pretty(&checks)?,
        )?;
    }
    println!(
        "{}",
        json!({"completed":report["completed"],"requests":app.work_budget.snapshot().requests.len(),"wall_elapsed_ms":report["wall_elapsed_ms"],"report":args.out_dir.join("report.json")})
    );
    outcome
}

fn evaluate_receipts(
    args: &Args,
    duration_ms: u128,
) -> Result<crate::runtime_eval::RuntimeEvalCaseReport> {
    let path = args
        .case_spec
        .as_ref()
        .context("--case-spec required for evaluation")?;
    let case = serde_json::from_slice(&std::fs::read(path)?)?;
    crate::runtime_eval::evaluate_case(
        &case,
        &args.tool_root.canonicalize()?.to_string_lossy(),
        crate::runtime_eval::RuntimeEvalArtifacts {
            case_dir: args.out_dir.clone(),
            trace_path: args.out_dir.join("trace.jsonl"),
            session_path: args.out_dir.join("session.json"),
            json_path: args.out_dir.join("session.json"),
            graph_path: args.out_dir.join("graph.json"),
        },
        duration_ms,
        None,
    )
}
