use super::super::{ExecKind, FailureMemory};
use super::*;
use serde_json::json;

fn attempt(
    assumption: &str,
    command: &str,
    context: &ExecVerificationContext<'_>,
) -> Option<String> {
    let mut ledger = AssumptionLedger::default();
    ledger.mark_refuted(
        assumption,
        Some("the prior attempt failed; gather new evidence"),
    );
    let think = ThinkBlock {
        goal: assumption.into(),
        step: 1,
        tool: "exec".into(),
        risk: "incorrect assumption".into(),
        doubt: "test it against current evidence".into(),
        next: command.into(),
        verify: "inspect the actual result".into(),
    };
    let tc = ToolCallData {
        thought_signature: None,
        id: "next".into(),
        name: "exec".into(),
        arguments: json!({"command":command}).to_string(),
    };
    refuted_assumption_conflict(&ledger, &think, &tc, context)
}

#[test]
fn diagnostic_and_verification_exec_can_test_arbitrary_refuted_prose() {
    let configured =
        "cargo test -q runtime::followup_rules::tests:: 2>&1 && bash scripts/review-smoke.sh";
    let context = ExecVerificationContext::from_root(Some(configured), "");
    for assumption in [
        "Assumed `src/runtime/session_state.rs` exists in the project.",
        "The target implementation lives in the runtime module.",
        "cargo test works unchanged",
    ] {
        for command in [
            "ls -R src/runtime/",
            "git status",
            "cargo test --lib",
            configured,
        ] {
            assert!(matches!(
                context.classify(command).kind,
                ExecKind::Diagnostic | ExecKind::Verify
            ));
            assert!(
                attempt(assumption, command, &context).is_none(),
                "{command}: {assumption}"
            );
        }
    }
}

#[test]
fn only_exact_configured_or_approved_custom_checks_bypass_assumption_prose() {
    let assumption = "Assumed `src/runtime/session_state.rs` exists in the project.";
    let configured =
        "cargo test -q runtime::followup_rules::tests:: 2>&1 && bash scripts/review-smoke.sh";
    let unconfigured = ExecVerificationContext::from_root(None, "");
    assert_eq!(unconfigured.classify(configured).kind, ExecKind::Action);
    assert!(attempt(assumption, configured, &unconfigured).is_some());

    let approved = "check-runtime 'A  B' && printf verified > receipt";
    let root = format!(
        "<observer_benchmark_plan>\nrequired_checks:\n- {approved}\n</observer_benchmark_plan>"
    );
    let context = ExecVerificationContext::from_root(None, &root);
    assert_eq!(context.classify(approved).kind, ExecKind::Verify);
    assert!(attempt(assumption, approved, &context).is_none());
    for changed in [
        approved.replace("A  B", "A B"),
        format!("{approved} && rm source.rs"),
    ] {
        assert_eq!(context.classify(&changed).kind, ExecKind::Action);
        assert!(attempt(assumption, &changed, &context).is_some());
    }
}

#[test]
fn mutation_dependencies_and_failure_repetition_remain_guarded() {
    let context = ExecVerificationContext::from_root(None, "");
    for command in [
        "cargo fix --allow-dirty",
        "printf 'cargo test' > receipt",
        "git status && rm source.rs",
    ] {
        assert_eq!(context.classify(command).kind, ExecKind::Action);
        assert!(attempt("The source mutation will work unchanged", command, &context).is_some());
    }
    let mut memory = FailureMemory::default();
    for _ in 0..2 {
        memory.on_tool_result("cargo test", "", "test failed", 1);
    }
    assert!(
        memory.repeated_failure_or_stall(),
        "verification remains subject to the independent repetition guard"
    );
}
