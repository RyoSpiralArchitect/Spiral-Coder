use super::*;
use crate::tui::agent;
use serde_json::json;

const CHECK: &str = "python3 -S acceptance.py";

#[test]
fn initial_baseline_exception_excludes_attempted_checks_mutations_and_other_commands() {
    let call = crate::streaming::ToolCallData {
        id: "next".into(),
        name: "exec".into(),
        arguments: json!({"command":CHECK}).to_string(),
        thought_signature: None,
    };
    let blocked = history("GOVERNOR BLOCKED: missing plan");
    assert!(is_unattempted_baseline(&call, &blocked, None));
    for output in ["FAILED (exit_code: 1)", "OK (exit_code: 0)"] {
        assert!(!is_unattempted_baseline(&call, &history(output), None));
    }
    let mut edited = blocked.clone();
    edited.extend([json!({"role":"assistant","tool_calls":[{"id":"edit","function":{"name":"patch_file","arguments":"{\"path\":\"src/runner.py\"}"}}]}),json!({"role":"tool","tool_call_id":"edit","content":"OK: patched 'src/runner.py'"})]);
    assert!(!is_unattempted_baseline(&call, &edited, None));
    let mut other = call.clone();
    other.arguments = json!({"command":"python3 other.py"}).to_string();
    assert!(!is_unattempted_baseline(&other, &blocked, None));
}

fn history(output: &str) -> Vec<Value> {
    let root = format!(
        "Fix the failing check.\n<task_requirements>\n{}\n</task_requirements>",
        json!({
        "version":1,"requirements":[{"id":"behavior","description":"Implement the missing behavior",
        "artifacts":[{"path":"src/runner.py","kind":"source"}],
        "checks":[{"command":CHECK,"baseline_failure":true}]}]})
    );
    vec![
        json!({"role":"user","content":root}),
        json!({"role":"assistant","tool_calls":[{"id":"baseline","function":{"name":"exec","arguments":json!({"command":CHECK}).to_string()}}]}),
        json!({"role":"tool","tool_call_id":"baseline","content":output}),
    ]
}

#[test]
fn rendered_baseline_failures_unlock_edits_for_both_roles_and_survive_resume() {
    let child = std::process::Command::new("python3")
        .args(["-S", "-c", "assert False, 'missing behavior'"])
        .output()
        .unwrap();
    assert!(!child.status.success());
    let output = agent::build_failed_tool_output(
        &String::from_utf8_lossy(&child.stdout),
        &String::from_utf8_lossy(&child.stderr),
        child.status.code().unwrap(),
    );
    assert!(output.starts_with("⚠ LOGIC ERROR"));
    let long_gui = format!(
        "⚠ LOGIC/TEST ERROR: the program ran but behavior is wrong.\n\nERROR_DIGEST:\n{}\nFAILED (exit_code: 1).\nduration_ms: 20\ncwd: /workspace\nstderr: AssertionError\nstdout: (empty)",
        (0..8).map(|i| format!("- Error {i}: {}\n", "x".repeat(310))).collect::<String>()
    );
    assert!(long_gui.find("FAILED (exit_code:").unwrap() > 2400);
    for output in [
        agent::inject_cwd(&output, "cwd: /workspace", None),
        agent::inject_cwd(
            &agent::build_failed_tool_output("", "Could not resolve host: index.crates.io", 6),
            "cwd: /workspace",
            None,
        ),
        agent::inject_cwd(
            &agent::build_failed_tool_output("", "Failed to connect to github.com port 443 via 127.0.0.1: Could not connect to server", 7),
            "cwd: /workspace", Some("NOTE: cwd_after was outside tool_root; ignored: /outside"),
        ),
        agent::inject_cwd(
            &agent::build_failed_tool_output("", "failed to remove file spiral-coder.exe: access is denied (os error 5)", 1),
            "cwd: /workspace", None,
        ),
        long_gui,
    ] {
        let mut messages = history(&output);
        let view = agent::task_obligations::project(&messages, None);
        assert!(
            view.task_requirements.pending_baselines.is_empty(),
            "{output}"
        );
        assert_eq!(view.pending_exact_checks, [CHECK]);
        assert!(
            issue(&messages, None).is_some(),
            "a red baseline alone cannot finish"
        );
        let edit = crate::streaming::ToolCallData {
            id: "edit".into(),
            name: "patch_file".into(),
            thought_signature: None,
            arguments: json!({"path":"src/runner.py","search":"old","replace":"new"}).to_string(),
        };
        assert!(mutation_issue(&edit, &messages, None).is_none());
        messages = serde_json::from_str(&serde_json::to_string(&messages).unwrap()).unwrap();
        for index in 0..60 {
            let id = format!("read-{index}");
            messages.extend([
                json!({"role":"assistant","tool_calls":[{"id":id,"function":{"name":"read_file","arguments":"{\"path\":\"src/runner.py\"}"}}]}),
                json!({"role":"tool","tool_call_id":id,"content":"[src/runner.py] (2 lines, 10 bytes)\nold\ncode"}),
            ]);
        }
        agent::prune_old_tool_results(&mut messages);
        agent::message_window::prune_message_window(&mut messages);
        let browser = crate::observer_action::evaluate(crate::observer_action::Request {
            packet: json!({}),
            messages,
            test_command: None,
            lang: "en".into(),
            reason: "baseline_failed".into(),
            response: None,
            final_summary: None,
            repair_native: true,
        });
        assert_eq!(
            browser.packet["task_obligations"]["task_requirements"],
            serde_json::to_value(view.task_requirements).unwrap()
        );
    }
}

#[test]
fn a_failed_marker_in_output_or_a_refusal_cannot_unlock_baseline_edits() {
    for output in [
        "OK (exit_code: 0)\nstdout:\nFAILED (exit_code: 1)",
        "GOVERNOR BLOCKED\nFAILED (exit_code: 1)",
        "REJECTED BY USER\nFAILED (exit_code: 1)",
        "Awaiting approval\nFAILED (exit_code: 1)",
        "error: failed to start command\nFAILED (exit_code: 1)",
        "⚠ LOGIC ERROR: advisory only\nstdout:\nFAILED (exit_code: 1)",
        "FAILED (exit_code: 1) invented suffix",
    ] {
        assert_eq!(
            project(&history(output), None).pending_baselines,
            [CHECK],
            "{output}"
        );
    }
    let mut messages = history("FAILED (exit_code: 1)");
    messages[2]["tool_call_id"] = json!("uncorrelated");
    assert_eq!(project(&messages, None).pending_baselines, [CHECK]);
    // A later successful attempt before editing supersedes the failed baseline.
    messages = history("FAILED (exit_code: 1)");
    messages.extend(history("OK (exit_code: 0)").into_iter().skip(1));
    assert_eq!(project(&messages, None).pending_baselines, [CHECK]);
}
