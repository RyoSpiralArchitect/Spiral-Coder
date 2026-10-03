use super::*;
use crate::runtime_eval::{evaluate_case, RuntimeEvalArtifacts, RuntimeEvalCase};

fn exchange(id: &str, name: &str, content: &str) -> [serde_json::Value; 2] {
    [
        json!({"role":"assistant","tool_calls":[{"id":id,"function":{"name":name,"arguments":"{\"path\":\"src/lib.rs\"}"}}]}),
        json!({"role":"tool","tool_call_id":id,"content":content}),
    ]
}

#[test]
fn old_patch_auto_test_evidence_survives_digest_pruning_and_evaluation() {
    let case: RuntimeEvalCase = serde_json::from_value(json!({
        "id":"pruned-auto-test", "prompt":"Fix the file and run its configured test",
        "checks":[{"kind":"auto_test_passed","command":"cargo test"}]
    }))
    .unwrap();
    for (status, expected) in [
        ("[auto-test] ✓ PASSED (exit 0)", true),
        ("[auto-test] ✗ FAILED (exit 1)", false),
        ("[auto-test] ✓ PASSED (exit 0) ", false),
    ] {
        let root = tempfile::tempdir().unwrap();
        let artifacts = RuntimeEvalArtifacts {
            case_dir: root.path().to_path_buf(),
            trace_path: root.path().join("trace.jsonl"),
            session_path: root.path().join("session.json"),
            json_path: root.path().join("final.json"),
            graph_path: root.path().join("graph.json"),
        };
        std::fs::write(&artifacts.trace_path, "{\"event\":\"verification_config\",\"data\":{\"test_command\":\"cargo test\"}}\n{\"event\":\"agent_outcome\",\"data\":{\"completed\":true}}\n{\"event\":\"agent_end\",\"data\":{\"ok\":true}}\n").unwrap();
        let result = format!(
            "OK: patched 'src/lib.rs'\n{}{status}\nstdout:\n[auto-test] ✓ PASSED (exit 0)\n{}",
            "[hash] diagnostic detail\n".repeat(30),
            "test output\n".repeat(100)
        );
        let compacted = compact_success_tool_result_for_history("patch_file", &result);
        assert_eq!(
            compacted
                .lines()
                .find(|line| line.starts_with("[auto-test]")),
            Some(status)
        );
        let mut messages =
            vec![json!({"role":"user","content":"Fix the file and run its configured test"})];
        messages.extend(exchange(
            "read",
            "read_file",
            "[src/lib.rs] (2 lines, 16 bytes)\nold code\n",
        ));
        messages.extend(exchange("patch", "patch_file", &compacted));
        for index in 0..KEEP_RECENT_TOOL_TURNS + 8 {
            messages.extend(exchange(
                &format!("read-{index}"),
                "read_file",
                "[src/lib.rs] (2 lines, 16 bytes)\nnew code\n",
            ));
        }
        prune_old_tool_results(&mut messages);
        prune_message_window(&mut messages);
        let pruned = messages
            .iter()
            .find(|msg| msg["tool_call_id"] == "patch")
            .unwrap()["content"]
            .as_str()
            .unwrap();
        assert!(pruned.contains("[pruned "));
        assert_eq!(
            pruned
                .lines()
                .filter(|line| line.starts_with("[auto-test]"))
                .collect::<Vec<_>>(),
            vec![status]
        );
        std::fs::write(
            &artifacts.json_path,
            json!({"messages":messages}).to_string(),
        )
        .unwrap();
        let report =
            evaluate_case(&case, root.path().to_str().unwrap(), artifacts, 0, None).unwrap();
        assert_eq!(report.ok, expected, "{status:?}");
        assert_eq!(
            report.metrics.fresh_auto_test_pass_count,
            usize::from(expected)
        );
        let (_, mutation, _, behavioral, _) =
            restore_done_gate_from_messages(&messages, Some("cargo test"));
        assert_eq!(
            behavioral.is_some(),
            expected,
            "resume must agree with the evaluator"
        );
        if expected {
            assert_eq!(behavioral, mutation);
        }
    }
}
