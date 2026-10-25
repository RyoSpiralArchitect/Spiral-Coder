use super::*;
use crate::tui::agent::{
    build_failed_tool_output, build_ok_tool_output, compact_success_tool_result_for_history,
    exec_proof::ExecProofResult, parse_exec_tool_output_sections, restore_done_gate_from_messages,
    FailureMemory,
};
use serde_json::json;

#[test]
fn successful_test_names_are_not_error_records() {
    let output = "running 3 tests\n\
test agent::initial_baseline_exception_preserves_proof ... ok\n\
test python::traceback_is_retained ... ok\n\
test r#type::commandnotfoundexception ... ok\n\
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n";
    assert_eq!(suspicious_success_reason(output, ""), None);
    assert_eq!(suspicious_success_reason("", output), None);
    assert_eq!(
        suspicious_success_reason(
            "test suite::exception_case ... \u{1b}[32mok\u{1b}[0m\r\n",
            ""
        ),
        None
    );
}

#[test]
fn passing_summary_does_not_hide_actual_errors_in_either_stream() {
    let passed = "test suite::exception_case ... ok\n\
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n";
    for error in [
        "ParserError: Missing expression after unary operator",
        "CommandNotFoundException: command is not recognized",
        "Traceback (most recent call last):",
        "fatal: unable to index file",
        "error: failed to compile dependency",
        "permission denied",
    ] {
        for (stdout, stderr) in [
            (format!("{passed}{error}\n"), String::new()),
            (passed.to_string(), error.to_string()),
            (error.to_string(), passed.to_string()),
        ] {
            let reason = suspicious_success_reason(&stdout, &stderr).unwrap();
            assert!(reason.contains(error), "{reason}");
            assert!(!reason.contains("suite::exception_case"));
        }
    }
}

#[test]
fn arbitrary_or_incomplete_test_like_text_is_not_exempt() {
    for line in [
        "test exception handler ... ok",
        "test ./exception.rs ... ok",
        "test ::exception ... ok",
        "test suite::::exception ... ok",
        "test suite::1exception ... ok",
        "test suite::exception ... ignored",
        "test suite::exception ... FAILED",
        "test suite::exception ...",
        "test suite::exception",
        "test suite::exception ... okay",
        "test suite::exception ... ok error: real failure",
        "test suite::exception ... \u{1b}[2Jok",
    ] {
        assert!(suspicious_success_reason(line, "").is_some(), "{line}");
    }
}

#[test]
fn no_summary_or_complete_suite_is_required_to_classify_a_retained_pass_row() {
    // Native history is intentionally shorter than the original full stdout.
    assert_eq!(
        suspicious_success_reason("test suite::exception_case ... ok", ""),
        None
    );
    assert!(
        suspicious_success_reason("test result: ok. 1 passed\nerror: later command", "").is_some()
    );
}

#[test]
fn diagnostic_is_the_actual_matching_line() {
    let stdout = "test suite::failed_assertion ... ok\n\
test suite::exception_case ... ok\nactual runtime exception: bad input";
    assert_eq!(
        suspicious_success_reason(stdout, ""),
        Some("exit_code was 0, but output contained error markers (e.g. `actual runtime exception: bad input`)".to_string())
    );
}

fn success_messages(content: &str) -> Vec<serde_json::Value> {
    vec![
        json!({"role":"assistant","tool_calls":[{"id":"verify","function":{"name":"exec","arguments":"{\"command\":\"cargo test\"}"}}]}),
        json!({"role":"tool","tool_call_id":"verify","content":content}),
    ]
}

#[test]
fn long_passing_output_keeps_success_after_preview_history_and_resume() {
    // Both the stdout boundary and history's per-line boundary fall inside a
    // real passing row whose identifier contains an old error substring.
    let output = format!(
        "running 3 tests\ntest suite::exception_{} ... ok\ntest suite::traceback_{} ... ok\n\
test suite::final_case ... ok\ntest result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n",
        "x".repeat(1100), "y".repeat(1800),
    );
    assert_eq!(suspicious_success_reason(&output, ""), None);
    let preview = build_ok_tool_output(&output);
    assert!(preview.contains("partial passing test row omitted"));
    let history = compact_success_tool_result_for_history("exec", &preview);
    assert!(history.contains("long name omitted from history"));
    for content in [&preview, &history] {
        let (_, stdout, stderr) = parse_exec_tool_output_sections(content);
        assert_eq!(
            suspicious_success_reason(&stdout, &stderr),
            None,
            "{content}"
        );
        assert!(ExecProofResult::from_history(content).succeeded);
        let messages = success_messages(content);
        let (_, _, _, behavioral, _) =
            restore_done_gate_from_messages(&messages, Some("cargo test"));
        assert_eq!(behavioral, Some(1));
        assert_eq!(
            FailureMemory::from_recent_messages(&messages, Some("cargo test")).consecutive_failures,
            0
        );
    }
}

#[test]
fn partial_error_line_and_failed_history_remain_failures() {
    let output = format!("runtime exception: {}", "x".repeat(2000));
    let preview = stdout_preview(&output, 1500);
    assert!(suspicious_success_reason(&preview, "").is_some());
    let failed = build_failed_tool_output(
        "test suite::exception_case ... ok",
        "actual runtime exception: bad input",
        1,
    );
    assert!(!ExecProofResult::from_history(&failed).succeeded);
    assert_eq!(
        FailureMemory::from_recent_messages(&success_messages(&failed), Some("cargo test"))
            .consecutive_failures,
        1
    );
}
