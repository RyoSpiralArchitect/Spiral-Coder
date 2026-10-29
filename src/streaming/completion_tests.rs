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
        let receipt = serde_json::to_value(&budget.snapshot().requests[0]).unwrap();
        assert_eq!(
            receipt["stream_interruption"],
            if reason.is_none() {
                json!("missing_terminal_frame")
            } else {
                serde_json::Value::Null
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
            transport_eligible
        );
        let mut done = false;
        while let Ok(token) = rx.try_recv() {
            done |= matches!(token, StreamToken::Done);
        }
        assert_eq!(done, reason.is_some());
    }
}

fn config_for_stream(base_url: &str, provider: &str) -> RunConfig {
    crate::Cli::try_parse_from([
        "spiral-coder",
        "--provider",
        provider,
        "--model",
        "offline",
        "--base-url",
        base_url,
        "--api-key",
        "offline",
        "--timeout-seconds",
        "2",
    ])
    .unwrap()
    .common
    .to_partial_config()
    .resolve()
    .unwrap()
}

#[tokio::test]
async fn broken_http_body_records_read_failure_without_completion_or_refund() {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut buf = [0; 4096];
        socket.read(&mut buf).unwrap();
        socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: 10000\r\nConnection: close\r\n\r\ndata: {\"choices\":[]}\n\n").unwrap();
    });
    let cfg = config_for_stream(&base_url, "openai");
    let budget = crate::work_budget::WorkBudget::new("task", 4, None);
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
    server.join().unwrap();
    assert!(result.is_err());
    let work = budget.snapshot();
    assert_eq!(work.requests.len(), 1);
    assert_eq!(
        work.requests[0].stream_interruption,
        Some(crate::work_budget::StreamInterruption::ReadFailed)
    );
    assert_eq!(work.requests[0].status, "stream_interrupted");
    assert_eq!(
        work.requests[0].charge,
        crate::work_budget::RequestCharge::Task
    );
    while let Ok(token) = rx.try_recv() {
        assert!(!matches!(token, StreamToken::Done));
    }
}

#[tokio::test]
async fn anthropic_interruption_causes_distinguish_eof_from_provider_errors() {
    for (status, body, expected) in [
        (200, "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"delta\":{\"text\":\"partial\"}}\n\n", Some(crate::work_budget::StreamInterruption::MissingTerminalFrame)),
        (200, "event: error\ndata: {\"type\":\"error\",\"error\":\"stream ended before message_stop\"}\n\n",None),
        (401, "unauthorized", None),
        (200, "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n", None),
    ] {
        let server = MockServer::start();
        let wire = server.mock(|when,then| {
            when.method(POST).path("/messages");
            then.status(status).header("content-type","text/event-stream").body(body);
        });
        let cfg = config_for_stream(&server.base_url(),"anthropic");
        let budget = crate::work_budget::WorkBudget::new("task",4,None);
        let (tx,_rx) = mpsc::channel(32);
        let result = crate::work_budget::scope(Some(crate::work_budget::Context {budget:budget.clone(),role:"observer"}),stream_anthropic(&reqwest::Client::new(),&cfg,&[ChatMessage {role:"user".into(),content:"observe".into()}],tx)).await;
        wire.assert_hits(1);
        assert_eq!(result.is_ok(),body.starts_with("event: message_stop"));
        assert_eq!(budget.snapshot().requests[0].stream_interruption,expected);
        if status == 401 { assert_eq!(budget.snapshot().requests[0].status,"http_401"); }
    }
}
