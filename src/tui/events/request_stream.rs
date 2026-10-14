//! Every UI request owns a generation; cancellation never reuses its mailbox.
use super::*;
use crate::agent_session::AgentSession;

#[derive(Debug)]
pub enum Payload {
    Token(StreamToken),
    Session(AgentSession),
}

#[derive(Debug)]
pub struct RequestEvent {
    pub generation: u64,
    pub payload: Payload,
}

pub(super) fn spawn<F, Fut>(
    out: &mpsc::Sender<RequestEvent>,
    generation: &mut u64,
    context: Option<crate::work_budget::Context>,
    work: F,
) -> tokio::task::JoinHandle<()>
where
    F: FnOnce(mpsc::Sender<StreamToken>) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = Option<AgentSession>> + Send + 'static,
{
    *generation = generation.wrapping_add(1);
    let generation = *generation;
    let out = out.clone();
    tokio::spawn(async move {
        let (tx, mut rx) = mpsc::channel(256);
        let forward = async {
            let mut error = None;
            while let Some(token) = rx.recv().await {
                // Completion is published only AFTER the native snapshot. The
                // next UI tick must not resume from the previous run's state.
                if let StreamToken::Error(message) = token {
                    error = Some(message);
                    continue;
                }
                if !matches!(token, StreamToken::Done) {
                    let _ = out
                        .send(RequestEvent {
                            generation,
                            payload: Payload::Token(token),
                        })
                        .await;
                }
            }
            error
        };
        let (session, error) = tokio::join!(crate::work_budget::scope(context, work(tx)), forward);
        if let Some(session) = session {
            let _ = out
                .send(RequestEvent {
                    generation,
                    payload: Payload::Session(session),
                })
                .await;
        }
        let _ = out
            .send(RequestEvent {
                generation,
                payload: Payload::Token(error.map(StreamToken::Error).unwrap_or(StreamToken::Done)),
            })
            .await;
    })
}

pub(super) fn dispatch(event: RequestEvent, app: &mut App, pane: PaneId) {
    let current = match pane {
        PaneId::Coder => app.coder_generation,
        PaneId::Observer => app.observer_generation,
        PaneId::Chat => app.chat_generation,
    };
    if event.generation != current {
        return;
    }
    match (pane, event.payload) {
        (PaneId::Coder, Payload::Session(session)) => app.coder_session = Some(session),
        (PaneId::Coder, Payload::Token(token)) => handle_coder_token(token, app),
        (PaneId::Observer, Payload::Token(token)) => handle_observer_token(token, app),
        (PaneId::Chat, Payload::Token(token)) => handle_chat_token(token, app),
        _ => {}
    }
}
