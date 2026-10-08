//! Fields in the Coder's structured protocol blocks.

use super::compact_one_line;
use crate::governor_contract;

fn parse_nested_tag_fields(body: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut cursor = 0usize;

    while let Some(rel_open) = body[cursor..].find('<') {
        let open_start = cursor + rel_open;
        let name_start = open_start + 1;
        let Some(rel_name_end) = body[name_start..].find('>') else {
            break;
        };
        let name_end = name_start + rel_name_end;
        let raw_name = body[name_start..name_end].trim();
        if raw_name.is_empty()
            || raw_name.starts_with('/')
            || raw_name.contains(char::is_whitespace)
            || raw_name.contains('=')
        {
            cursor = name_end + 1;
            continue;
        }

        let close = format!("</{raw_name}>");
        let value_start = name_end + 1;
        let Some(rel_close_start) = body[value_start..].find(&close) else {
            cursor = value_start;
            continue;
        };
        let value_end = value_start + rel_close_start;
        let value = body[value_start..value_end].trim();
        if !value.is_empty() {
            out.push((raw_name.to_ascii_lowercase(), value.to_string()));
        }
        cursor = value_end + close.len();
    }

    out
}

fn colon_field<'a>(line: &'a str, tag: &str) -> Option<(usize, String, &'a str)> {
    let trimmed = line.trim_start();
    let (key, value) = trimmed.split_once(':')?;
    let key = key.trim().to_ascii_lowercase();
    let recognized = if tag == "realize" {
        key == "reason"
    } else {
        governor_contract::block_field(tag, &key).is_some()
    };
    recognized.then(|| (line.len() - trimmed.len(), key, value.trim()))
}

pub(super) fn parse_tag_fields(body: &str, tag: &str) -> Vec<(String, String)> {
    let nested = parse_nested_tag_fields(body);
    if !nested.is_empty() {
        return nested;
    }

    let bracketed = parse_bracket_quoted_fields(body);
    if !bracketed.is_empty() {
        return bracketed;
    }

    // Only known fields at the least-indented field level delimit this block.
    // A nested fixture's `goal:` or `tool:` belongs to the containing value.
    let top_indent = body
        .lines()
        .filter_map(|line| colon_field(line, tag).map(|(indent, _, _)| indent))
        .min();
    let mut out: Vec<(String, String)> = Vec::new();
    let mut current_key: Option<String> = None;
    let mut current_value = String::new();

    for raw_line in body.lines() {
        let line = raw_line.trim();
        if let Some((_, key, value)) =
            colon_field(raw_line, tag).filter(|(indent, _, _)| Some(*indent) == top_indent)
        {
            if let Some(key) = current_key.take() {
                out.push((key, current_value.trim().to_string()));
            }
            current_key = Some(key);
            current_value = value.to_string();
            continue;
        }

        if current_key.is_some() && !line.is_empty() {
            if !current_value.is_empty() {
                current_value.push(' ');
            }
            current_value.push_str(line);
        }
    }

    if let Some(key) = current_key {
        out.push((key, current_value.trim().to_string()));
    }

    out
}

fn canonical_loose_tag_key(raw_key: &str) -> Option<String> {
    const FIELDS: &[&str] = &[
        "next_minimal_action",
        "wrong_assumption",
        "strategy_change",
        "acceptance",
        "assumptions",
        "last_outcome",
        "remaining_gap",
        "goal_delta",
        "progress",
        "changed",
        "verify",
        "reason",
        "steps",
        "risks",
        "doubt",
        "goal",
        "step",
        "tool",
        "risk",
        "next",
    ];

    let key = raw_key.trim().to_ascii_lowercase();
    if key.is_empty() {
        return None;
    }
    for field in FIELDS {
        if key == *field || key.ends_with(field) {
            return Some((*field).to_string());
        }
    }
    None
}

fn parse_bracket_quoted_fields(body: &str) -> Vec<(String, String)> {
    let normalized = body.replace("\r\n", "\n").replace('\n', " ");
    let chars: Vec<char> = normalized.chars().collect();
    let mut out = Vec::new();
    let mut i = 0usize;

    while i < chars.len() {
        while i < chars.len() && !(chars[i].is_ascii_alphabetic() || chars[i] == '_') {
            i += 1;
        }
        if i >= chars.len() {
            break;
        }
        let start = i;
        while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
            i += 1;
        }
        let raw_key: String = chars[start..i].iter().collect();
        let Some(key) = canonical_loose_tag_key(&raw_key) else {
            continue;
        };

        while i < chars.len() && chars[i].is_ascii_whitespace() {
            i += 1;
        }
        if i + 1 >= chars.len() || chars[i] != '[' || chars[i + 1] != '"' {
            continue;
        }
        i += 2;

        let mut value = String::new();
        let mut escaped = false;
        while i < chars.len() {
            let ch = chars[i];
            if escaped {
                value.push(ch);
                escaped = false;
                i += 1;
                continue;
            }
            if ch == '\\' {
                escaped = true;
                i += 1;
                continue;
            }
            if ch == '"' && i + 1 < chars.len() && chars[i + 1] == ']' {
                i += 2;
                break;
            }
            value.push(ch);
            i += 1;
        }

        let value = compact_one_line(value.trim(), 300);
        if !value.is_empty() {
            out.push((key, value));
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::super::{
        parse_plan_block, parse_realize_block, parse_reflection_block, parse_think_block,
        validate_plan,
    };

    #[test]
    fn spaced_acceptance_heading_restores_the_live_resume_plan_without_waiving_validation() {
        let text = r#"<plan>
Goal: Fix the failing greet test in the Rust project with the smallest possible code change.

Steps:
1. Inspect the project structure to understand the codebase
2. Read the failing test to identify what is broken
3. Read the source code to understand the implementation
4. Identify the minimal change needed to fix the test
5. Apply the fix
6. Run `cargo test` to verify the fix
7. Document the changed source path

Assumptions:
- The test failure is related to a "greet" test as mentioned in the prompt
- The issue is likely in the source code that the test exercises
- The smallest change means either a bug fix or adjustment to match expected behavior

Risks:
- Making too large a change that affects other functionality
- Missing the actual root cause and applying a band-aid fix

Acceptance Criteria:
1. The failing test passes after the change (verified by running `cargo test`)
2. No other tests are broken by the change (verified by running `cargo test`)
3. The change is minimal and focused on the specific issue
</plan>"#;
        let plan = parse_plan_block(text).unwrap();
        let canonical =
            parse_plan_block(&text.replace("Acceptance Criteria:", "acceptance:")).unwrap();
        assert_eq!(plan.acceptance_criteria, canonical.acceptance_criteria);
        assert_eq!(plan.acceptance_criteria.len(), 3);
        assert_eq!(plan.steps, canonical.steps);
        assert_eq!(plan.steps.len(), 7);
        assert_eq!(plan.risks, canonical.risks);
        assert!(!plan.risks.contains("Acceptance Criteria"));
        validate_plan(&plan).unwrap();

        for heading in ["Completion wishes:", "  Acceptance Criteria:"] {
            let invalid = parse_plan_block(&text.replace("Acceptance Criteria:", heading)).unwrap();
            assert!(invalid.acceptance_criteria.is_empty());
            assert!(validate_plan(&invalid).is_err());
        }
        let empty = format!(
            "{}</plan>",
            text.split("Acceptance Criteria:").next().unwrap()
        );
        assert!(validate_plan(&parse_plan_block(&empty).unwrap()).is_err());
    }

    #[test]
    fn plan_keeps_four_steps_with_nested_colon_details_and_recognized_labels() {
        let text = r#"<plan>
    goal: add the replay case
    steps:
      1) Read the current replay fixture.
      2) Add the case with:
        - id: review-panel
          observer_response: recorded response
          goal: this nested fixture value is not the plan goal
      3) Run the replay command: spiral-coder tui-replay.
      4) Report the verified artifact.
    acceptance_criteria: 1) replay passes 2) artifact is reported
    risks: wrong JSON shape
    assumptions: fixture is available
</plan>"#;
        let plan = parse_plan_block(text).unwrap();
        assert_eq!(plan.goal, "add the replay case");
        assert_eq!(plan.steps.len(), 4, "{:?}", plan.steps);
        assert!(plan.steps[1].contains("- id: review-panel"));
        assert!(plan.steps[1].contains("observer_response: recorded response"));
        assert!(plan.steps[1].contains("goal: this nested fixture value"));
        assert_eq!(
            plan.steps[2],
            "Run the replay command: spiral-coder tui-replay."
        );
        assert_eq!(plan.steps[3], "Report the verified artifact.");
        assert_eq!(plan.acceptance_criteria.len(), 2);
        validate_plan(&plan).unwrap();
    }

    #[test]
    fn think_and_reflection_keep_colon_continuations_under_the_current_field() {
        let think = parse_think_block(
            r#"<think>
goal: inspect output
step: 2
tool: exec
risk: stale result
doubt: output might be partial
next: inspect status:
  tool: fixture metadata only
  endpoint: https://example.invalid/status
verify: status is recorded
</think>"#,
        )
        .unwrap();
        assert_eq!(think.tool, "exec");
        assert!(think.next.contains("tool: fixture metadata only"));
        assert!(think.next.contains("https://example.invalid/status"));
        assert_eq!(think.verify, "status is recorded");

        let reflection = parse_reflection_block(
            r#"<reflect>
last_outcome: failure
goal_delta: same
wrong_assumption: the response shape was valid:
  observed: invalid request
strategy_change: adjust
next_minimal_action: read the expected schema
</reflect>"#,
        )
        .unwrap();
        assert!(reflection
            .wrong_assumption
            .contains("observed: invalid request"));
        assert_eq!(reflection.next_minimal_action, "read the expected schema");
    }

    #[test]
    fn xml_and_bracket_fields_keep_their_existing_forms() {
        for text in [
            r#"<plan><goal>Inspect output: verify it</goal><steps>1) Read output 2) Run check</steps><acceptance>1) check passes</acceptance><risks>wrong output</risks><assumptions>local tool</assumptions></plan>"#,
            r#"<plan>goal["Inspect output: verify it"] steps["1) Read output 2) Run check"] acceptance["1) check passes"] risks["wrong output"] assumptions["local tool"]</plan>"#,
        ] {
            let plan = parse_plan_block(text).unwrap();
            assert_eq!(plan.goal, "Inspect output: verify it");
            assert_eq!(plan.steps, vec!["Read output", "Run check"]);
            assert_eq!(plan.acceptance_criteria, vec!["check passes"]);
            validate_plan(&plan).unwrap();
        }
        assert_eq!(
            parse_realize_block("<realize>reason: publish current plan: before acting</realize>"),
            Some("publish current plan: before acting".to_string())
        );
    }
}
