//! Independent oracle for an append-only JSON case collection. The baseline
//! comes from the evaluator spec outside the model's writable workspace.
use anyhow::{bail, Context, Result};
use serde_json::Value;
use std::collections::BTreeMap;

pub(crate) fn check(baseline: &Value, candidate: &Value) -> Result<()> {
    fn cases(value: &Value) -> Result<BTreeMap<String, &Value>> {
        let cases = value["cases"]
            .as_array()
            .context("cases must be an array")?;
        let mut out = BTreeMap::new();
        for case in cases {
            let id = case["id"]
                .as_str()
                .filter(|s| !s.is_empty())
                .context("case id must be nonempty")?;
            if out.insert(id.to_owned(), case).is_some() {
                bail!("duplicate case id: {id}");
            }
        }
        Ok(out)
    }
    let original = cases(baseline)?;
    let edited = cases(candidate)?;
    for (id, value) in original {
        match edited.get(&id) {
            Some(new) if *new == value => {},
            Some(_) => bail!("existing case changed: {id}; preserve every field and nested based_on evidence. Original case (data): {}", serde_json::to_string(value)?.chars().take(1800).collect::<String>()),
            None => bail!("existing case removed: {id}"),
        }
    }
    for (key, value) in baseline.as_object().context("baseline must be an object")? {
        if key != "cases" && candidate.get(key) != Some(value) {
            bail!("top-level field changed: {key}");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn independent_oracle_rejects_weakened_assertions_and_lost_nested_evidence() {
        let baseline = json!({"version":1,"cases":[{"id":"case-A","checks":[{"kind":"packet_contains","value":"coder-1"}],"observer_response":{"suggestions":[{}, {"based_on":["failure_kind"]}]},"extension":{"keep":true}}]});
        let mut candidate = baseline.clone();
        candidate["cases"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"case-B"}));
        check(&baseline, &candidate).unwrap();
        candidate["cases"][0]["observer_response"]["suggestions"][1]["based_on"] = json!([]);
        assert!(check(&baseline, &candidate).is_err());
        candidate = baseline.clone();
        candidate["cases"][0]["checks"][0]["value"] = json!("case-A");
        assert!(check(&baseline, &candidate).is_err());
        candidate = baseline.clone();
        candidate["cases"]
            .as_array_mut()
            .unwrap()
            .push(baseline["cases"][0].clone());
        assert!(check(&baseline, &candidate).is_err());
    }
}
