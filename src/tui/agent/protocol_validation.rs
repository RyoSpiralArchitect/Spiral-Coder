//! Report independent protocol defects together; never repair model claims or calls.
use super::{
    compact_one_line, parse_exec_command_from_args, plan_field_max_items, plan_field_min_items,
    think_next_matches_exec_command, PlanBlock, ThinkBlock, ToolCallData,
};
use crate::governor_contract as contract;
use anyhow::{anyhow, Result};

pub(super) fn validate_plan(plan: &PlanBlock) -> Result<()> {
    let mut errors = Vec::new();
    if plan.goal.trim().is_empty() {
        errors.push(contract::plan_missing_goal_message());
    }
    let min_steps = plan_field_min_items("steps").unwrap_or(2);
    let max_steps = plan_field_max_items("steps").unwrap_or(7);
    let min_acceptance = plan_field_min_items("acceptance").unwrap_or(1);
    let max_acceptance = plan_field_max_items("acceptance").unwrap_or(4);
    if plan.steps.is_empty() {
        errors.push(contract::plan_missing_steps_message());
    } else if plan.steps.len() < min_steps {
        errors.push(contract::plan_min_steps_message(min_steps));
    } else if plan.steps.len() > max_steps {
        errors.push(contract::plan_max_steps_message(max_steps));
    }
    if plan.acceptance_criteria.is_empty() {
        errors.push(contract::plan_missing_acceptance_message());
    } else if plan.acceptance_criteria.len() < min_acceptance {
        errors.push(contract::plan_min_acceptance_message(min_acceptance));
    } else if plan.acceptance_criteria.len() > max_acceptance {
        errors.push(contract::plan_max_acceptance_message(max_acceptance));
    }
    if plan.risks.trim().is_empty() {
        errors.push(contract::plan_missing_risks_message());
    }
    if plan.assumptions.trim().is_empty() {
        errors.push(contract::plan_missing_assumptions_message());
    }
    if plan.steps.iter().any(|step| step.trim().is_empty()) {
        errors.push(contract::plan_empty_step_message());
    }
    if plan
        .acceptance_criteria
        .iter()
        .any(|criterion| criterion.trim().is_empty())
    {
        errors.push(contract::plan_empty_acceptance_message());
    }
    if errors.is_empty() {
        return Ok(());
    }
    Err(anyhow!(
        "{}\nRepair all listed fields together. Use one field per line: goal, steps ({min_steps}..{max_steps} numbered items), acceptance ({min_acceptance}..{max_acceptance} items), risks, assumptions. Keep the user's task and acceptance obligations.",
        errors.join("; ")
    ))
}

pub(super) fn validate_think(
    think: &ThinkBlock,
    plan: &PlanBlock,
    tc: &ToolCallData,
) -> Result<()> {
    let mut errors = Vec::new();
    if think.goal.trim().is_empty() {
        errors.push(contract::think_missing_goal_message());
    }
    if think.step == 0 {
        errors.push(contract::think_invalid_step_message());
    } else if think.step > plan.steps.len() {
        errors.push(contract::think_step_out_of_range_message(
            think.step,
            plan.steps.len(),
        ));
    }
    if think.tool.trim().is_empty() {
        errors.push(contract::think_invalid_tool_message());
    } else if think.tool != tc.name {
        errors.push(contract::think_tool_mismatch_message(
            &compact_one_line(&think.tool, 80),
            &compact_one_line(&tc.name, 80),
        ));
    }
    if think.risk.trim().is_empty() {
        errors.push(contract::think_missing_risk_message());
    }
    if think.doubt.trim().is_empty() {
        errors.push(contract::think_missing_doubt_message());
    }
    if think.next.trim().is_empty() {
        errors.push(contract::think_missing_next_message());
    } else if tc.name == "exec" {
        let command = parse_exec_command_from_args(&tc.arguments)
            .unwrap_or_else(|| tc.arguments.trim().to_string());
        if !think_next_matches_exec_command(&think.next, &command) {
            errors.push(contract::think_exec_prefix_mismatch_message());
        }
    }
    if think.verify.trim().is_empty() {
        errors.push(contract::think_missing_verify_message());
    }
    if errors.is_empty() {
        return Ok(());
    }
    // These are field requirements, not synthesized reasoning or evidence.
    Err(anyhow!(
        "{}\nRepair all listed fields together. Use one field per line inside <think>:\n\
goal: the immediate action's purpose\n\
step: an integer from 1 to {} in the current plan\n\
tool: {}\n\
risk: a short risk assessment\n\
doubt: what remains uncertain\n\
next: the intended action (for exec, include its command prefix)\n\
verify: how to check this action's result\n\
Then issue the actual tool call. Keep the current plan unless the task changed.",
        errors.join("; "),
        plan.steps.len(),
        compact_one_line(&tc.name, 80)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan() -> PlanBlock {
        PlanBlock {
            goal: "repair".into(),
            steps: vec!["inspect".into(), "repair and test".into()],
            acceptance_criteria: vec!["tests pass".into()],
            risks: "regression".into(),
            assumptions: "source exists".into(),
        }
    }

    #[test]
    fn complete_fields_do_not_waive_tool_step_or_exec_alignment() {
        let call = ToolCallData {
            id: "call".into(),
            name: "exec".into(),
            arguments: r#"{"command":"cargo test"}"#.into(),
            thought_signature: None,
        };
        let mut think = ThinkBlock {
            goal: "test".into(),
            step: 9,
            tool: "read_file".into(),
            risk: "test failure".into(),
            doubt: "repair unverified".into(),
            next: "pwd".into(),
            verify: "exit status".into(),
        };
        let error = validate_think(&think, &plan(), &call)
            .unwrap_err()
            .to_string();
        for defect in [
            "outside the current plan",
            "does not match actual tool",
            "command prefix match",
        ] {
            assert!(error.contains(defect), "{error}");
        }
        think.step = 2;
        think.tool = "exec".into();
        think.next = "cargo test".into();
        assert!(validate_think(&think, &plan(), &call).is_ok());
    }

    #[test]
    fn aggregate_plan_validation_retains_size_and_empty_item_limits() {
        let mut plan = plan();
        plan.steps = vec![String::new(); 8];
        plan.acceptance_criteria = vec![String::new(); 5];
        let error = validate_plan(&plan).unwrap_err().to_string();
        for defect in [
            "too many steps",
            "too many acceptance",
            "empty step",
            "empty acceptance",
        ] {
            assert!(error.contains(defect), "{error}");
        }
        plan.steps = vec!["inspect".into()];
        assert!(validate_plan(&plan)
            .unwrap_err()
            .to_string()
            .contains("at least 2"));
    }
}
