use super::*;

#[tokio::test]
async fn conflicting_answer_uses_one_repair_and_its_identical_summary_reaches_the_coder() {
    let coder = MockServer::start();
    let observer = MockServer::start();
    let sse = |text: &str| {
        format!(
            "data: {}\n\ndata: [DONE]\n\n",
            json!({"choices":[{"delta":{"content":text},"finish_reason":"stop"}]})
        )
    };
    const MARKER: &str = "Reinspect the fixture at the repaired-summary boundary.";
    coder.mock(|when, then| {
        when.method(POST).path("/chat/completions").matches(|r| {
            !String::from_utf8_lossy(r.body.as_deref().unwrap_or_default()).contains(MARKER)
        });
        then.status(200)
            .header("content-type", "text/event-stream")
            .body(sse("The task is unfinished."));
    });
    let received = coder.mock(|when, then| {
        when.method(POST)
            .path("/chat/completions")
            .body_contains(MARKER)
            .body_contains("observer_advisory");
        // End the fixture at delivery; no tool result or completion can be inferred.
        then.status(400).body("fixture stops at the Coder handoff");
    });
    let conflict = r#"{"summary":"one","summary":"two"}"#;
    let primary = json!({"kind":"read","confidence":0.95,"reason":MARKER,
        "suggested_tool":"read_file","suggested_args":{"path":"engine.rs"}});
    let identical =
        format!(r#"{{"summary":"inspect","suggestions":[{primary}],"summary":"inspect"}}"#);
    let first = observer.mock(|when, then| {
        when.method(POST).path("/chat/completions").matches(|r| {
            !String::from_utf8_lossy(r.body.as_deref().unwrap_or_default())
                .contains("Observer action repair")
        });
        then.status(200)
            .header("content-type", "text/event-stream")
            .body(sse(conflict));
    });
    let second = observer.mock(|when, then| {
        when.method(POST)
            .path("/chat/completions")
            .body_contains("Observer action repair")
            .body_contains("duplicate JSON field: summary");
        then.status(200)
            .header("content-type", "text/event-stream")
            .body(sse(&identical));
    });
    let report = budget_rhythm::episode(&coder, &observer, 8, 2, 4).await;
    first.assert_hits(1);
    second.assert_hits(1);
    received.assert_hits(1);
    let charges: Vec<_> = report["work"]["requests"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["role"] == "observer")
        .map(|r| r["charge"].as_str().unwrap())
        .collect();
    assert_eq!(charges, ["transport", "task"]);
    let display = report["observer_display"].as_array().unwrap();
    assert!(display.iter().any(|m| m["content"] == conflict));
    assert!(display.iter().any(|m| m["raw_content"] == identical));
    assert!(report["execution"]["actions"]
        .as_array()
        .unwrap()
        .is_empty());
    assert_eq!(report["completed"], false);
}
