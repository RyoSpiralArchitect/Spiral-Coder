//! Bounded headless driver of the actual TUI Coder/Observer consumers.
//! Uses the same model in both roles; fixtures/oracles stay outside this driver.
use super::*;
use anyhow::{Context, Result};

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
    #[arg(long)]
    pub seed_session: Option<PathBuf>,
    #[arg(long)]
    pub without_observer: bool,
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
        request_stream::dispatch(event, app, pane);
        if terminal {
            break;
        }
    }
    Ok(())
}

pub(crate) async fn run(args: Args, common: crate::CommonArgs) -> Result<()> {
    if args.evaluate_only {
        let report = evaluate_receipts(&args, 0)?;
        println!("{}", serde_json::to_string(&report)?);
        return Ok(());
    }
    anyhow::ensure!(
        !args.out_dir.exists(),
        "output exists; preserve earlier receipts and choose a fresh directory"
    );
    anyhow::ensure!(
        (1..=64).contains(&args.max_requests),
        "max-requests must be 1..64"
    );
    std::fs::create_dir_all(&args.out_dir)?;
    let root = args
        .tool_root
        .canonicalize()?
        .to_string_lossy()
        .into_owned();
    let prompt = std::fs::read_to_string(&args.prompt_file)?;
    let cfg = common.to_partial_config().resolve()?;
    let mut obs_cfg = cfg.clone();
    obs_cfg.mode = crate::modes::Mode::Observer;
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
    if args.without_observer {
        app.coder_chunk_reserve = 0;
    }
    let trace = crate::trace_writer::TraceWriter::new(args.out_dir.join("trace.jsonl"))?;
    trace.event("agent_config", json!({"provider":cfg.provider.to_string(),"model":cfg.model,"coupled":true,"max_requests":args.max_requests}))?;
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
            Some(PathBuf::from(&root).join(".spiral-coder/tui-work.json")),
        );
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
    while app.coder.streaming {
        drain(&mut app, &mut coder_rx, PaneId::Coder, &trace).await?;
        if app.coder_completed == Some(true)
            || args.without_observer
            || app.work_budget.remaining() < 2
        {
            break;
        }
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
            "incomplete_task",
        )
        .await;
        if app.observer.streaming {
            drain(&mut app, &mut observer_rx, PaneId::Observer, &trace).await?;
        }
        maybe_observer_action_repair(&mut app, &observer_tx).await;
        if app.observer.streaming {
            drain(&mut app, &mut observer_rx, PaneId::Observer, &trace).await?;
        }
        if app.pending_observer_hint.is_none() || app.work_budget.remaining() == 0 {
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
    trace.event("agent_end", json!({"ok":true}))?;
    if let Some(session) = &app.coder_session {
        crate::agent_session::AgentSession::save_atomic(
            &args.out_dir.join("session.json"),
            session,
        )?;
    }
    let report = json!({
        "version":1,"completed":app.coder_completed == Some(true), "wall_elapsed_ms":started.elapsed().as_millis(),
        "work":app.work_budget.snapshot(),
        "execution":app.coder_session.as_ref().map(|s| agent::evidence_packet::project(&s.messages, app.project_test_cmd.as_deref())),
        "coder_display":app.coder.messages.iter().map(|m| json!({"role":format!("{:?}",m.role),"content":m.content})).collect::<Vec<_>>(),
        "observer_display":app.observer.messages.iter().map(|m| json!({"role":format!("{:?}",m.role),"content":m.content})).collect::<Vec<_>>(),
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
    Ok(())
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
