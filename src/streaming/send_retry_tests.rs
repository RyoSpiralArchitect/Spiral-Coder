//! A dropped connection before response headers must remain spent and bounded.
use super::*;
use clap::Parser;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

async fn dropped_send(
    role: &'static str,
    limit: usize,
    respond_on: usize,
) -> (Result<()>, crate::work_budget::WorkState, Vec<Vec<u8>>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let finished = Arc::new(AtomicBool::new(false));
    let done = finished.clone();
    let server = std::thread::spawn(move || {
        let mut bodies = Vec::new();
        while !done.load(Ordering::SeqCst) {
            let (mut socket, _) = match listener.accept() {
                Ok(pair) => pair,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(2));
                    continue;
                }
                Err(e) => panic!("{e}"),
            };
            socket.set_nonblocking(false).unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut request = Vec::new();
            let mut buf = [0; 4096];
            loop {
                let count = socket.read(&mut buf).unwrap();
                assert!(count > 0);
                request.extend_from_slice(&buf[..count]);
                if let Some(end) = request.windows(4).position(|s| s == b"\r\n\r\n") {
                    let len = String::from_utf8_lossy(&request[..end])
                        .lines()
                        .filter_map(|s| s.split_once(':'))
                        .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
                        .unwrap()
                        .1
                        .trim()
                        .parse::<usize>()
                        .unwrap();
                    if request.len() >= end + 4 + len {
                        bodies.push(request[end + 4..end + 4 + len].to_vec());
                        break;
                    }
                }
            }
            if bodies.len() == respond_on {
                let body = "data: {\"choices\":[{\"delta\":{\"content\":\"ready\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n";
                write!(socket,"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).unwrap();
            }
            // Otherwise close only after reading the POST, without HTTP headers.
        }
        bodies
    });
    let cli = crate::Cli::try_parse_from([
        "spiral-coder",
        "--provider",
        "openai",
        "--model",
        "offline",
        "--base-url",
        &url,
        "--api-key",
        "offline",
        "--timeout-seconds",
        "2",
    ])
    .unwrap();
    let cfg = cli.common.to_partial_config().resolve().unwrap();
    let budget = crate::work_budget::WorkBudget::new("task", limit, None);
    let (tx, _rx) = mpsc::channel(128);
    let result = crate::work_budget::scope(
        Some(crate::work_budget::Context {
            budget: budget.clone(),
            role,
        }),
        stream_openai_compat_json(
            &reqwest::Client::new(),
            &cfg,
            &[json!({"role":"user","content":"inspect"})],
            None,
            tx,
        ),
    )
    .await;
    finished.store(true, Ordering::SeqCst);
    (result, budget.snapshot(), server.join().unwrap())
}

#[tokio::test]
async fn both_roles_retry_a_closed_preheader_connection_with_another_task_charge() {
    for role in ["coder", "observer"] {
        let (result, work, bodies) = dropped_send(role, 4, 2).await;
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(bodies.len(), 2);
        assert_eq!(bodies[0], bodies[1]);
        assert_eq!(work.requests.len(), 2);
        assert_eq!(work.requests[0].status, "send_failed");
        assert_eq!(work.requests[1].status, "completed");
        assert!(work
            .requests
            .iter()
            .all(|r| r.role == role && r.charge == crate::work_budget::RequestCharge::Task));
    }
}

#[tokio::test]
async fn failed_send_stops_at_shared_allowance_without_refunding_the_first_wire() {
    let (result, work, bodies) = dropped_send("coder", 1, 2).await;
    assert!(result.is_err());
    assert_eq!(bodies.len(), 1);
    assert_eq!(work.requests.len(), 1);
    assert_eq!(work.requests[0].status, "send_failed");
    assert!(format!("{:#}", result.unwrap_err()).contains("previous failure"));
}

#[tokio::test]
async fn failed_sends_stop_at_the_existing_retry_cap_with_budget_left() {
    let (result, work, bodies) = dropped_send("coder", 8, usize::MAX).await;
    assert!(result.is_err());
    assert_eq!(bodies.len(), 5);
    assert_eq!(work.requests.len(), 5);
    assert_eq!(work.task_requests(), 5);
    assert!(work.requests.iter().all(|r| r.status == "send_failed"));
}

#[tokio::test]
async fn invalid_request_construction_is_not_a_connection_retry() {
    let error = reqwest::Client::new()
        .post("not a URL")
        .send()
        .await
        .unwrap_err();
    assert!(error.is_builder());
    assert!(!retry_policy::send_retryable(&error));
}
