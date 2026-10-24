//! Read an authored JSON object inside <plan> without inventing missing fields
//! or collapsing criteria. Existing plan validation and task alignment still run.
use super::PlanBlock;
use serde::Deserialize;

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Fields {
    goal: String,
    steps: Vec<String>,
    #[serde(alias = "acceptance_criteria")]
    acceptance: Vec<String>,
    risks: Text,
    assumptions: Text,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Text {
    One(String),
    Many(Vec<String>),
}

impl Default for Text {
    fn default() -> Self {
        Self::One(String::new())
    }
}
impl Text {
    fn into_string(self) -> String {
        match self {
            Self::One(text) => text,
            Self::Many(items) => items.join("\n"),
        }
    }
}

pub(super) fn parse(body: &str) -> Option<PlanBlock> {
    if !body.trim_start().starts_with('{') {
        return None;
    }
    // Malformed/ambiguous JSON stays an invalid empty plan; do not search a
    // nested string for a second set of executable protocol fields.
    let fields: Fields = serde_json::from_str(body).unwrap_or_default();
    Some(PlanBlock {
        goal: fields.goal,
        steps: fields.steps,
        acceptance_criteria: fields.acceptance,
        risks: fields.risks.into_string(),
        assumptions: fields.assumptions.into_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn authored() -> serde_json::Value {
        json!({"goal":"Repair selection in runner.py", "steps":["Inspect runner.py", "Repair and verify runner.py"],
            "acceptance":["Selection passes the original check"], "risks":["default behavior regression"], "assumptions":["supplied tests describe behavior"]})
    }

    #[test]
    fn json_keeps_authored_fields_through_parse_and_resume() {
        let value = authored();
        let text = format!("<plan>\n{value}\n</plan>");
        let plan = super::super::parse_plan_block(&text).unwrap();
        super::super::validate_plan(&plan).unwrap();
        assert_eq!(json!(plan.steps), value["steps"]);
        assert_eq!(json!(plan.acceptance_criteria), value["acceptance"]);
        assert_eq!(plan.risks, value["risks"][0].as_str().unwrap());
        let messages = vec![
            json!({"role":"user","content":"Repair selection in runner.py"}),
            json!({"role":"assistant","content":text}),
        ];
        let resumed: Vec<serde_json::Value> =
            serde_json::from_str(&serde_json::to_string(&messages).unwrap()).unwrap();
        assert_eq!(
            json!(super::super::plan_resume::latest_valid(&resumed, |p| {
                super::super::validate_plan(p).is_ok()
            })),
            json!(plan)
        );
    }

    #[test]
    fn invalid_or_oversized_plans_remain_invalid_without_dropping_criteria() {
        let mut value = authored();
        value["acceptance"] = json!(["one", "two", "three", "four", "five"]);
        let plan = parse(&value.to_string()).unwrap();
        assert_eq!(plan.acceptance_criteria.len(), 5);
        assert!(super::super::validate_plan(&plan).is_err());
        for body in [
            "{}",
            "{\"goal\":\"one\",\"goal\":\"two\"}",
            "{\"goal\":false}",
            "{\"goal\":\"<goal>nested</goal>\",\"unknown\":1}",
            "{\"goal\":\"unfinished",
        ] {
            assert!(
                super::super::validate_plan(&parse(body).unwrap()).is_err(),
                "{body}"
            );
        }
        assert!(parse("goal: Repair selection").is_none());
    }
}
