use super::*;
use crate::tui::agent::{evidence_packet, task_obligations};
use serde_json::json;

fn exchange(messages: &mut Vec<Value>, id: &str, tool: &str, args: Value, result: &str) {
    messages.extend([
        json!({"role":"assistant","content":"<think>goal: repair the source</think>",
            "tool_calls":[{"id":id,"type":"function","function":{"name":tool,"arguments":args.to_string()}}]}),
        json!({"role":"tool","tool_call_id":id,"content":result}),
    ]);
}

fn add_edits(messages: &mut Vec<Value>, from: usize, count: usize) {
    for i in from..from + count {
        exchange(
            messages,
            &format!("edit-{i}"),
            "write_file",
            json!({"path":"src/a.rs","content":format!("// iteration {i}\n{}", "fn sample() {}\n".repeat(400))}),
            "OK: wrote 'src/a.rs'\n[auto-test] ✗ FAILED (exit 1)\nerror: unfinished source",
        );
    }
}

fn decode(value: &Value) -> Value {
    fn expand(value: &Value, texts: &Value) -> Value {
        match value {
            Value::Object(fields) if fields.len() == 1 && fields.contains_key("$context_text") => {
                texts[fields["$context_text"].as_str().unwrap()].clone()
            }
            Value::Object(fields) => Value::Object(
                fields
                    .iter()
                    .map(|(k, v)| (k.clone(), expand(v, texts)))
                    .collect(),
            ),
            Value::Array(values) => {
                json!(values.iter().map(|v| expand(v, texts)).collect::<Vec<_>>())
            }
            _ => value.clone(),
        }
    }
    if value["context_encoding"] == "shared_text_v1" {
        expand(&value["value"], &value["texts"])
    } else {
        value.clone()
    }
}

#[test]
fn repeated_context_is_lossless_for_commands_unicode_and_literals() {
    let command = format!(
        "python3 - <<'PY'\nprint({:?})\nPY",
        "🐈‍⬛ exact \"literal\"\n".repeat(200)
    );
    let native = json!({"pending_exact_checks":[command],"required_exact_checks":[command],
        "final_answer":{"required_literals":[command]},"citable_checks":[],"check_facts":[{"command":command,"passed":0,"failed":null}]});
    let encoded = text_pool::encode(&native);
    assert_eq!(encoded["context_encoding"], "shared_text_v1");
    assert_eq!(decode(&encoded), native);
    assert!(encoded.to_string().len() < native.to_string().len() / 2);
    assert_eq!(text_pool::encode(&native), encoded);
    let mut reserved = native.clone();
    reserved["user_data"] = json!({"$context_text":"literal source value"});
    assert_eq!(text_pool::encode(&reserved), reserved);
    let short = json!({"task":"small","checks":["cargo test","cargo test"]});
    assert_eq!(text_pool::encode(&short), short);
}

#[test]
fn long_coder_task_references_the_human_message_and_keeps_pending_requirements() {
    let command = format!("python3 -S check_{}", "x".repeat(600));
    let task = format!(
        "Repair src/a.rs.\n{}\n<task_requirements>\n{}\n</task_requirements>",
        "Source and documentation must agree. 猫\n".repeat(55),
        json!({"version":1,"requirements":[{"id":"repair","description":"Repair the source",
            "artifacts":[{"path":"src/a.rs","kind":"source"}],"checks":[{"command":command}]}]})
    );
    let native = vec![
        crate::task_origin::user_message(&task, crate::task_origin::MessageOrigin::User),
        crate::task_origin::user_message("Continue.", crate::task_origin::MessageOrigin::Runtime),
    ];
    let obligations = task_obligations::project(&native, None);
    let full = json!(&obligations);
    let compact = render_coder_obligations(&obligations, &task);
    let view = decode(&serde_json::from_str(&compact).unwrap());
    let original = &view["work_progress"]["original_human_task"];
    assert_eq!(original["source"], "conversation_user_message");
    assert!(task.starts_with(original["starts_with"].as_str().unwrap()));
    assert_eq!(original["total_chars"], task.chars().count());
    assert_eq!(coder(&native, None).0, native);
    assert!(view["task_requirements"].get("pending").is_none());
    assert_eq!(
        view["task_requirements"]["requirements"],
        full["task_requirements"]["requirements"],
    );
    assert_eq!(view["pending_exact_checks"], full["pending_exact_checks"]);
    assert_eq!(json!(&obligations), full);

    let mut distinct = full.clone();
    distinct["task_requirements"]["pending"]
        .as_array_mut()
        .unwrap()
        .push(json!("an ungrouped issue"));
    let rendered = decode(&serde_json::from_str(&render_coder_obligations(&distinct, "")).unwrap());
    assert_eq!(rendered, distinct);
}

#[test]
fn observer_prompt_keeps_report_facts_with_one_copy_of_the_bound_guidance() {
    let report = json!({"clauses":["Include paths and outcomes"],
        "required_literals":["src/a.rs"],
        "outcome_rows":[format!("Before: FAILED — {}", "exact check ".repeat(100))],
        "check_facts":[{"command":"cargo test","passed":null}]});
    let full = super::super::report_surface::guidance(&report);
    let packet = json!({"task_obligations":{"final_answer":report},
    "tool_argument_schemas":[{"name":"done","parameters":{"properties":{
        "summary":{"type":"string","description":full},"completed_acceptance":{"type":"array"}
    }}}]});
    let view = super::super::current_evidence::observer_prompt_packet(&packet);
    assert_eq!(view["task_obligations"], packet["task_obligations"]);
    let pointer = "/tool_argument_schemas/0/parameters/properties/summary/description";
    assert!(view
        .pointer(pointer)
        .unwrap()
        .as_str()
        .unwrap()
        .contains("task_obligations.final_answer"));
    assert_eq!(packet.pointer(pointer), Some(&json!(full)));
    let mut custom = packet;
    *custom.pointer_mut(pointer).unwrap() = json!(format!("{full}\nUse custom guidance."));
    assert_eq!(
        super::super::current_evidence::observer_prompt_packet(&custom),
        custom
    );
}

#[test]
fn long_iterations_compress_wire_history_without_changing_native_proof_or_resume() {
    let root = "Repair src/a.rs and document it.\n<observer_benchmark_plan>\nrequired_checks:\n- cargo test\n- cargo fmt --check\n</observer_benchmark_plan>";
    let mut native = vec![
        json!({"role":"system","content":"Keep the original contract"}),
        json!({"role":"user","content":root}),
    ];
    exchange(
        &mut native,
        "old-test",
        "exec",
        json!({"command":"cargo test"}),
        "OK (exit_code: 0)\nstdout:\ntest result: ok. 2 passed; 0 failed",
    );
    add_edits(&mut native, 0, 30);
    for iteration in 0..3 {
        native.push(crate::task_origin::user_message(
            "Resume the unfinished task; preserve the original requirements.",
            crate::task_origin::MessageOrigin::Runtime,
        ));
        add_edits(&mut native, 30 + iteration * 10, 10);
        exchange(
            &mut native,
            &format!("fresh-read-{iteration}"),
            "read_file",
            json!({"path":"src/a.rs"}),
            "[src/a.rs] (1 lines, 16 bytes)\ncurrent source",
        );
        let before = native.clone();
        let obligations = json!(task_obligations::project(&native, None));
        let proof = json!(evidence_packet::project(&native, None));
        let (projected, metrics) = coder(&native, None);
        assert!(metrics.compacted_exchanges >= 20, "{metrics:?}");
        assert!(
            metrics.after_bytes < metrics.before_bytes / 2,
            "{metrics:?}"
        );
        assert_eq!(native, before);
        assert_eq!(&projected[..2], &native[..2]);
        assert_eq!(
            &projected[projected.len() - 2..],
            &native[native.len() - 2..]
        );
        let saved = crate::agent_session::AgentSession::new(None, None, None, None, native.clone());
        let restored: crate::agent_session::AgentSession =
            serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap();
        assert_eq!(restored.messages, native);
        assert_eq!(
            json!(task_obligations::project(&restored.messages, None)),
            obligations
        );
        assert_eq!(
            json!(evidence_packet::project(&restored.messages, None)),
            proof
        );
        assert_eq!(
            obligations["pending_exact_checks"],
            json!(["cargo test", "cargo fmt --check"])
        );
        // The wire projection itself must remain structurally resumable, but
        // is never persisted or fed into a native proof evaluator.
        let mut wire = crate::agent_session::AgentSession::new(None, None, None, None, projected);
        assert_eq!(wire.repair_for_resume(), None);
    }
}

#[test]
fn new_human_task_compresses_earlier_reads_without_importing_old_proof() {
    let mut messages = vec![json!({"role":"user","content":"Inspect the previous version."})];
    exchange(
        &mut messages,
        "old-check",
        "exec",
        json!({"command":"cargo test"}),
        "OK (exit_code: 0)\ncwd: /work",
    );
    exchange(
        &mut messages,
        "signed",
        "read_file",
        json!({"path":"signed.rs"}),
        "[signed.rs] (1 line)\nsource",
    );
    messages[3]["tool_calls"][0]["extra_content"] =
        json!({"google":{"thought_signature":"opaque"}});
    exchange(
        &mut messages,
        "refused",
        "write_file",
        json!({"path":"keep.rs"}),
        "REJECTED BY USER: keep this file",
    );
    let protected = messages[3..7].to_vec();
    let output = format!(
        "[old.rs] (100 lines)\n{}",
        "earlier source line with details ".repeat(256)
    );
    for i in 0..28 {
        exchange(
            &mut messages,
            &format!("old-read-{i}"),
            "read_file",
            json!({"path":"old.rs"}),
            &output,
        );
    }
    let request = crate::task_origin::user_message(
        "Verify the new version.\n<observer_benchmark_plan>\nrequired_checks:\n- cargo test\n</observer_benchmark_plan>",
        crate::task_origin::MessageOrigin::User,
    );
    messages.push(request.clone());
    let evidence = json!(evidence_packet::project(&messages, None));
    assert_eq!(evidence["pending_exact_checks"], json!(["cargo test"]));
    let (wire, metrics) = coder(&messages, None);
    assert!(metrics.compacted_exchanges >= 20, "{metrics:?}");
    assert!(
        metrics.after_bytes < metrics.before_bytes / 2,
        "{metrics:?}"
    );
    assert_eq!(wire.last(), Some(&request));
    assert!(protected.iter().all(|m| wire.contains(m)));
    assert!(wire.iter().any(|m| {
        m["content"]
            .as_str()
            .is_some_and(|s| s.contains("earlier user tasks"))
    }));
    assert!(!wire.iter().any(|m| m["tool_call_id"] == "old-read-0"));
    assert_eq!(json!(evidence_packet::project(&messages, None)), evidence);
    let mut resumed = crate::agent_session::AgentSession::new(None, None, None, None, wire);
    assert_eq!(resumed.repair_for_resume(), None);
}

#[test]
fn signed_malformed_denied_and_current_exchanges_survive_pressure_verbatim() {
    let mut messages = vec![json!({"role":"user","content":"Fix src/a.rs"})];
    exchange(
        &mut messages,
        "signed",
        "write_file",
        json!({"path":"src/a.rs","content":"old signed source"}),
        "OK: wrote 'src/a.rs'",
    );
    messages[1]["tool_calls"][0]["extra_content"] =
        json!({"google":{"thought_signature":"opaque"}});
    exchange(
        &mut messages,
        "denied",
        "exec",
        json!({"command":"delete files"}),
        "REJECTED BY USER: keep the originals",
    );
    exchange(
        &mut messages,
        "duplicate",
        "exec",
        json!({"command":"false"}),
        "FAILED (exit_code: 1)",
    );
    exchange(
        &mut messages,
        "duplicate",
        "exec",
        json!({"command":"true"}),
        "OK (exit_code: 0)",
    );
    let protected = messages.clone();
    add_edits(&mut messages, 0, 40);
    exchange(
        &mut messages,
        "current-check",
        "exec",
        json!({"command":"cargo test"}),
        "FAILED (exit_code: 1)\ncwd: /work\nerror: current failure",
    );
    exchange(
        &mut messages,
        "current-read",
        "read_file",
        json!({"path":"src/a.rs"}),
        "[src/a.rs] (1 lines, 6 bytes)\nsource",
    );
    for _ in 0..20 {
        messages.push(json!({"role":"assistant","content":"<think>goal: repair</think>"}));
    }
    messages.push(json!({"role":"assistant","tool_calls":[{"id":"incomplete","function":{"name":"exec","arguments":"{}"}}]}));
    let (projected, metrics) = coder(&messages, None);
    assert!(metrics.compacted_exchanges > 0);
    for message in protected {
        assert!(projected.contains(&message));
    }
    for message in messages.iter().filter(|m| {
        m["tool_call_id"] == "current-check"
            || m["tool_call_id"] == "current-read"
            || m["tool_calls"][0]["id"] == "incomplete"
    }) {
        assert!(projected.contains(message));
    }
    let google = crate::streaming::provider_metadata::prepare_messages(
        &projected,
        "https://generativelanguage.googleapis.com/v1beta/openai",
    );
    assert_eq!(google[1], messages[1]);
    let oversized = vec![json!({"role":"user","content":"mandatory ".repeat(20000)})];
    let (unchanged, metrics) = coder(&oversized, None);
    assert_eq!(unchanged, oversized);
    assert!(metrics.protected_overflow);
}

#[test]
fn observer_modes_and_coder_share_reversible_context_without_changing_validation() {
    let command = format!("cargo test -- {}", "an_exact_long_test_name_".repeat(100));
    let root = format!("Repair the catalog.\n<observer_benchmark_plan>\nrequired_checks:\n- {command}\n</observer_benchmark_plan>");
    let messages = vec![json!({"role":"user","content":root})];
    let obligations = json!(task_obligations::project(&messages, None));
    assert_eq!(
        decode(&serde_json::from_str(&render(&obligations)).unwrap()),
        obligations
    );
    let mut packet = json!({"task_obligations":obligations,"execution_evidence":evidence_packet::project(&messages, None)});
    for mode in ["general", "diagnostic", "handoff"] {
        packet["work_budget"] = json!({"remaining":8,"coder_remaining":6,"next_coder_requests":5,"next_coder_turn":"work"});
        packet["task_obligations"]["required_diagnostic_read"] = Value::Null;
        if mode == "diagnostic" {
            packet["task_obligations"]["required_diagnostic_read"] =
                json!({"path":"src/a.rs","failed_check":command});
        }
        if mode == "handoff" {
            packet["work_budget"]["next_coder_turn"] = json!("exhausted");
        }
        let before = packet.clone();
        let prompt = crate::tui::events::observer_assist::build_tui_next_action_prompt(
            &packet,
            "en",
            "verification_failure",
        );
        assert!(prompt.contains("catalog"));
        assert_eq!(packet, before);
        if mode != "handoff" {
            assert!(prompt.contains(&command));
        }
    }
    assert!(evidence_packet::completion_issue(&messages, "done", None)
        .unwrap()
        .contains(&command));
    // A dictionary tag is never accepted as an executable/evidence command.
    assert!(super::super::acceptance_report::resolve_evidence(
        &json!([{"criterion":"acceptance 1","command":"@t1"}]),
        &super::super::acceptance_report::CurrentEvidence::from_native(&messages, None, None),
    )
    .is_err());
}
