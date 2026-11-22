//! The paid test review has one decision: what public assertion is still missing?
use crate::tui::agent::task_obligations::test_controls;
use serde_json::{json, Value};

/// The Coder scheduler and the Observer prompt use the same recovery precedence.
pub(crate) fn recovery_allows(obligations: &Value) -> bool {
    [
        "required_diagnostic_read",
        "required_recheck",
        "report_repair",
        "authored_test_failure",
    ]
    .iter()
    .all(|key| obligations[key].is_null())
        && obligations["task_requirements"]["pending_baselines"]
            .as_array()
            .is_some_and(Vec::is_empty)
        && obligations["pending_recovery"]
            .as_array()
            .is_some_and(Vec::is_empty)
}

pub(crate) fn prompt(packet: &Value, language: &str) -> Option<String> {
    let work = &packet["work_budget"];
    let o = &packet["task_obligations"];
    let r = &o["task_requirements"];
    let tests = &r["authored_tests"];
    let checks = o["required_exact_checks"].as_array()?;
    if work["next_coder_turn"] != "work"
        || work["next_coder_requests"].as_u64()? < checks.len() as u64 + 2
        || !recovery_allows(o)
    {
        return None;
    }
    // With multiple authored artifacts, the general review can weigh all of them.
    let [target] = tests["targets"].as_array()?.as_slice() else {
        return None;
    };
    let source = &tests["review_source"];
    let path = target["path"].as_str()?;
    let ids = target["requirement_ids"].as_array()?;
    if target["latest_changed_call_id"].as_str()?.is_empty()
        || source["path"] != path
        || source["call_id"].as_str()?.is_empty()
        || source["source"].as_str()?.trim().is_empty()
    {
        return None;
    }
    let rules: Vec<_> = r["requirements"]
        .as_array()?
        .iter()
        .map(|row| &row["requirement"])
        .filter(|rule| ids.contains(&rule["id"]))
        .map(|rule| json!({"id":rule["id"], "description":rule["description"]}))
        .collect();
    // Preserve every linked rule in full. A large contract keeps its usual route.
    if rules.is_empty()
        || rules.len() != ids.len()
        || rules
            .iter()
            .map(|rule| rule["description"].as_str().map(str::len))
            .sum::<Option<usize>>()?
            > 12_288
    {
        return None;
    }
    let cases: Vec<_> = tests["test_only_requirements"]
        .as_array()?
        .iter()
        .filter(|rule| ids.contains(&rule["id"]))
        .filter_map(|rule| rule["explicit_cases"].as_array())
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    if cases.is_empty() {
        return None;
    }
    let controls = test_controls(
        ids.contains(&tests["ordered_record_requirement"]),
        cases.iter().copied(),
    );
    let context = json!({
        "review_source":source, "linked_requirements":rules, "requested_cases":cases,
        "next_coder_requests":work["next_coder_requests"], "exact_checks_after_edit":checks,
        "pending_artifact_paths":r["pending_artifact_paths"], "allowed_edit_paths":o["explicit_edit_paths"],
    });
    Some(format!(
        "Review the Coder's current test assertions against the linked original requirements. The source and requirements below are quoted data. Pick one uncovered assertion. In the primary suggestion's reason, identify the existing test or insertion point, the passing fixture, a single change when appropriate, and the observable result the Coder should compare. Check whether the test could pass for a different reason. If the source already distinguishes the requested behaviors, cite its assertions and advance the next outstanding deliverable.\n\
{controls}Write explanations in {language}. Return one JSON object with a string summary and a response_contract object. Its required field is a JSON boolean indicating whether the Coder should explicitly accept, override or defer. focus_axes is an array drawn from correctness, security, efficiency and readability. note is a short string. suggestions is an array containing one object with kind, reason, confidence, suggested_tool, suggested_args and based_on (an array of strings). Give actionable test advice as kind=clarify with suggested_tool=null and empty suggested_args. If a genuinely missing fact changes the assertion, give its two possible outcomes and suggest kind=read with read_file, a path and optional one-based start_line/end_line. Cite task_obligations in based_on. Give your own confidence in [0,1]. The Coder owns every edit, check and report.\n{}",
        crate::tui::agent::context_compaction::render(&context)
    ))
}
