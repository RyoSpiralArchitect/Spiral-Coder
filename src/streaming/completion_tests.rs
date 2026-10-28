use super::*;
use clap::Parser;
use httpmock::prelude::*;

#[tokio::test]
async fn clean_eof_without_terminal_frame_is_not_completed_or_transport_credit() {
    for (reason, content, expected_status, transport_eligible) in [
        (None, "{unfinished", "stream_interrupted", false),
        (Some("stop"), "{unfinished", "completed", true),
        (Some("length"), "{unfinished", "output_limited", false),
        (Some("length"), "", "output_limited", false),
        (Some("content_filter"), "", "output_filtered", false),
        (
            Some("function_call_filter: MALFORMED_FUNCTION_CALL"),
            "",
            "output_filtered",
            false,
        ),
        (
            Some("unexpected_stop"),
            "{unfinished",
            "provider_stopped",
            false,
        ),
        (Some("stop"), "", "empty_response", false),
    ] {
        let server = MockServer::start();
        let wire = server.mock(|when, then| {
            when.method(POST).path("/chat/completions");
            then.status(200)
                .header("content-type", "text/event-stream")
                .body(format!(
                    "data: {}\n\n",
                    json!({"choices":[{
                        "delta":{"content":content}, "finish_reason":reason
                    }]})
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
        assert_eq!(result.is_err(), reason.is_none());
        assert_eq!(budget.snapshot().requests[0].status, expected_status);
        assert_eq!(
            budget
                .reject_transport(
                    "observer",
                    0,
                    crate::work_budget::TransportRejection::ObserverEnvelope
                )
                .unwrap(),
            transport_eligible
        );
        let mut done = false;
        while let Ok(token) = rx.try_recv() {
            done |= matches!(token, StreamToken::Done);
        }
        assert_eq!(done, reason.is_some());
    }
}
