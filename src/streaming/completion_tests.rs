use super::*;
use clap::Parser;
use httpmock::prelude::*;

#[tokio::test]
async fn clean_eof_without_terminal_frame_is_not_completed_or_transport_credit() {
    for ending in [
        "",
        ",\"finish_reason\":\"stop\"",
        ",\"finish_reason\":\"length\"",
    ] {
        let server = MockServer::start();
        let wire = server.mock(|when, then| {
            when.method(POST).path("/chat/completions");
            then.status(200)
                .header("content-type", "text/event-stream")
                .body(format!(
                "data: {{\"choices\":[{{\"delta\":{{\"content\":\"{{unfinished\"}}{ending}}}]}}\n\n"
            ));
        });
        let cli = crate::Cli::try_parse_from([
            "spiral-coder",
            "--provider",
            "openai",
            "--model",
            "offline",
            "--base-url",
            &server.base_url(),
            "--api-key",
            "offline",
        ])
        .unwrap();
        let cfg = cli.common.to_partial_config().resolve().unwrap();
        let budget = crate::work_budget::WorkBudget::new("task", 4, None);
        budget
            .configure(crate::work_budget::BudgetPolicy {
                transport_repair_limit: 2,
                observer_reserved_requests: 1,
            })
            .unwrap();
        let (tx, mut rx) = mpsc::channel(32);
        let result = crate::work_budget::scope(
            Some(crate::work_budget::Context {
                budget: budget.clone(),
                role: "observer",
            }),
            stream_openai_compat_json(
                &reqwest::Client::new(),
                &cfg,
                &[json!({"role":"user","content":"observe"})],
                None,
                tx,
            ),
        )
        .await;
        wire.assert_hits(1);
        assert_eq!(result.is_err(), ending.is_empty());
        assert_eq!(
            budget.snapshot().requests[0].status,
            if ending.is_empty() {
                "stream_interrupted"
            } else {
                "completed"
            }
        );
        assert_eq!(
            budget
                .reject_transport(
                    "observer",
                    0,
                    crate::work_budget::TransportRejection::ObserverEnvelope
                )
                .unwrap(),
            !ending.is_empty()
        );
        let mut done = false;
        while let Ok(token) = rx.try_recv() {
            done |= matches!(token, StreamToken::Done);
        }
        assert_eq!(done, !ending.is_empty());
    }
}
