//! Per-request reporting vocabulary. Choices guide authored reports; they do
//! not assign evidence to criteria, alter returned calls, or grant completion.
use super::{acceptance_report, PlanBlock};
use serde_json::{json, Value};

const MAX_ENUM_BYTES: usize = 16_384;

pub(super) fn bind(tools: &mut Value, plan: Option<&PlanBlock>, commands: &[String]) {
    let references: Vec<String> = acceptance_report::criteria(plan)
        .iter()
        .filter_map(|c| c["reference"].as_str().map(str::to_owned))
        .collect();
    if references.is_empty() {
        return;
    }
    let Some(done) = tools.as_array_mut().and_then(|tools| {
        tools
            .iter_mut()
            .find(|tool| tool["function"]["name"] == "done")
    }) else {
        return;
    };
    let properties = &mut done["function"]["parameters"]["properties"];
    for field in ["completed_acceptance", "remaining_acceptance"] {
        set_enum(&mut properties[field]["items"], &references);
    }
    let items = &mut properties["acceptance_evidence"]["items"];
    if items["type"] == "object" {
        set_enum(&mut items["properties"]["criterion"], &references);
        set_enum(&mut items["properties"]["command"], commands);
    } else if items["type"] == "string" {
        // Google compatibility needs flat rows. Bound the Cartesian product
        // before allocation; never shorten a command to make it fit.
        let bytes = references.iter().try_fold(0usize, |sum, reference| {
            commands.iter().try_fold(sum, |sum, command| {
                sum.checked_add(reference.len() + command.len() + 8)
                    .filter(|size| *size <= MAX_ENUM_BYTES)
            })
        });
        if bytes.is_some() {
            let rows: Vec<String> = references
                .iter()
                .flat_map(|reference| {
                    commands
                        .iter()
                        .map(move |command| format!("{reference} | {command}"))
                })
                .collect();
            set_enum(items, &rows);
        }
    }
    properties["acceptance_evidence"]["description"] = json!(
        "Choose exactly one evidence row per completed criterion. Choices are current receipts, not proof that every receipt supports every criterion. Decide that mapping yourself. Keep unverified criteria in remaining_acceptance; never rerun checks just to reformat this report."
    );
}

fn set_enum(schema: &mut Value, choices: &[String]) {
    let values = json!(choices);
    if !choices.is_empty() && values.to_string().len() <= MAX_ENUM_BYTES {
        schema["enum"] = values;
    }
}

pub(super) fn bind_summary(tools: &mut Value, report: &Value) {
    if report["clauses"].as_array().is_none_or(Vec::is_empty) {
        return;
    }
    let Some(done) = tools
        .as_array_mut()
        .and_then(|tools| tools.iter_mut().find(|t| t["function"]["name"] == "done"))
    else {
        return;
    };
    let guidance = super::report_surface::guidance(report);
    if guidance.len() <= MAX_ENUM_BYTES {
        done["function"]["parameters"]["properties"]["summary"]["description"] = json!(guidance);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan() -> PlanBlock {
        PlanBlock {
            goal: "Repair".into(),
            steps: vec!["Repair".into(), "Verify".into()],
            acceptance_criteria: vec!["Behavior is correct".into(), "Docs explain behavior".into()],
            risks: "regressions".into(),
            assumptions: "fixture exists".into(),
        }
    }

    #[test]
    fn both_wire_shapes_keep_authored_decisions_and_exact_commands() {
        let command = "check --case 'A' | tee receipt.txt".to_string();
        for base in [
            "https://api.example",
            "https://generativelanguage.googleapis.com/v1beta/openai",
        ] {
            let mut tools = json!([super::super::done_tool_def(base)]);
            bind(&mut tools, Some(&plan()), &[command.clone()]);
            let props = &tools[0]["function"]["parameters"]["properties"];
            assert_eq!(
                props["completed_acceptance"]["items"]["enum"],
                json!(["acceptance 1", "acceptance 2"])
            );
            assert_eq!(
                props["remaining_acceptance"]["items"]["enum"],
                props["completed_acceptance"]["items"]["enum"]
            );
            let items = &props["acceptance_evidence"]["items"];
            let rows = if items["type"] == "string" {
                assert_eq!(
                    items["enum"],
                    json!([
                        format!("acceptance 1 | {command}"),
                        format!("acceptance 2 | {command}")
                    ])
                );
                json!([items["enum"][1]])
            } else {
                assert_eq!(items["properties"]["command"]["enum"], json!([command]));
                json!([{"criterion":"acceptance 2","command":command}])
            };
            let parsed = acceptance_report::parse_done_acceptance_evidence(&rows);
            assert_eq!(parsed[0].criterion, "acceptance 2");
            assert_eq!(parsed[0].command, command);
            assert!(props["summary"].get("default").is_none());
        }
    }

    #[test]
    fn missing_plan_empty_receipts_and_large_choices_keep_valid_generic_schema() {
        let original = json!([super::super::done_tool_def(
            "https://generativelanguage.googleapis.com"
        )]);
        let mut tools = original.clone();
        bind(&mut tools, None, &["check".into()]);
        assert_eq!(tools, original);
        for commands in [vec![], vec!["x".repeat(MAX_ENUM_BYTES)]] {
            let mut tools = original.clone();
            bind(&mut tools, Some(&plan()), &commands);
            assert!(
                tools[0]["function"]["parameters"]["properties"]["acceptance_evidence"]["items"]
                    .get("enum")
                    .is_none()
            );
        }
    }

    #[test]
    fn summary_guidance_tracks_native_outcomes_without_filling_a_report() {
        let root = "Repair a.py. Final answer must include the actual before/after check outcomes.";
        let check = "python3 acceptance.py";
        let mut messages = vec![json!({"role":"user","content":root})];
        let add = |m: &mut Vec<Value>, id: &str, tool: &str, args: Value, result: &str| {
            m.extend([json!({"role":"assistant","tool_calls":[{"id":id,"function":{"name":tool,"arguments":args.to_string()}}]}),json!({"role":"tool","tool_call_id":id,"content":result})]);
        };
        for state in 0..3 {
            if state == 1 {
                add(
                    &mut messages,
                    "before",
                    "exec",
                    json!({"command":check}),
                    "FAILED (exit_code: 1)",
                );
                add(
                    &mut messages,
                    "edit",
                    "patch_file",
                    json!({"path":"a.py"}),
                    "OK: patched 'a.py'",
                );
                add(
                    &mut messages,
                    "after",
                    "exec",
                    json!({"command":check}),
                    "OK (exit_code: 0)",
                );
            } else if state == 2 {
                add(
                    &mut messages,
                    "edit2",
                    "patch_file",
                    json!({"path":"a.py"}),
                    "OK: patched 'a.py'",
                );
            }
            let report = super::super::final_handoff::report_context(root, &messages, Some(check));
            let mut tools = json!([super::super::done_tool_def("https://api.example")]);
            bind_summary(&mut tools, &report);
            let schema = &tools[0]["function"]["parameters"]["properties"]["summary"];
            let guidance = schema["description"].as_str().unwrap();
            assert!(guidance.contains(check));
            assert_eq!(guidance.contains("After: PASSED"), state == 1);
            assert_eq!(guidance.contains("Before: NOT RECORDED"), state == 0);
            assert!(schema.get("default").is_none() && schema.get("const").is_none());
            if state == 1 {
                assert!(super::super::validate_authored_done_summary(
                    root,
                    "Fixed a.py",
                    "Fixed a.py",
                    &messages,
                    Some(check)
                )
                .is_err());
            }
        }
    }
}
