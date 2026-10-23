use super::*;
use clap::Parser;
use httpmock::prelude::*;

#[tokio::test]
async fn failed_first_stream_retains_incomplete_report_and_spent_request() {
    let server = MockServer::start();
    let request = server.mock(|when, then| {
        when.method(POST).path("/chat/completions");
        let data = json!({"choices":[{"delta":{"tool_calls":[
            {"index":0,"id":"first","function":{"name":"read_file","arguments":"{}"}},
            {"index":0,"id":"conflict","function":{"name":"read_file","arguments":"{}"}}
        ]}}]});
        then.status(200)
            .header("content-type", "text/event-stream")
            .body(format!("data: {data}\n\ndata: [DONE]\n\n"));
    });
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("workspace");
    std::fs::create_dir(&root).unwrap();
    let prompt = directory.path().join("task.txt");
    std::fs::write(&prompt, "Read note.txt and report its content.").unwrap();
    let output = directory.path().join("run");
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
        "--timeout-seconds",
        "2",
    ])
    .unwrap();
    let result = run(
        Args {
            tool_root: root,
            prompt_file: prompt,
            out_dir: output.clone(),
            max_requests: 4,
            seed_session: None,
            without_observer: false,
            case_spec: None,
            evaluate_only: false,
        },
        cli.common,
    )
    .await;
    assert!(result.is_err());
    request.assert_hits(1);
    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(output.join("report.json")).unwrap()).unwrap();
    assert_eq!(report["completed"], false);
    assert!(report["error"].is_string());
    assert_eq!(report["work"]["requests"].as_array().unwrap().len(), 1);
    assert!(report["execution"]["actions"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(output.join("session.json").is_file());
    let trace = std::fs::read_to_string(output.join("trace.jsonl")).unwrap();
    assert!(trace.contains("streamed tool-call id changed"));
    assert!(trace.contains(r#""ok":false"#));
}
