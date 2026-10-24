//! Small JSON edits prepared from the full file, shared with approval previews.
//! Models supply only the new value, never the unchanged document.
use anyhow::{bail, ensure, Context, Result};
use serde_json::Value;

/// Materialize a small structured request as an exact compare-and-replace edit.
/// Approval and execution keep the same original bytes, so concurrent changes fail.
pub(crate) fn text_edit(path: &str, args: &Value, base: Option<&str>) -> Result<(String, String)> {
    if ["json_pointer", "json_value", "json_expected"]
        .iter()
        .any(|k| args.get(k).is_some())
    {
        let original = std::fs::read_to_string(super::resolve_safe_path(path, base)?)?;
        let candidate = prepare(path, &original, args)?;
        Ok((original, candidate))
    } else {
        Ok((
            args["search"].as_str().context("missing search")?.into(),
            args["replace"].as_str().context("missing replace")?.into(),
        ))
    }
}

pub(crate) fn prepare(path: &str, original: &str, args: &Value) -> Result<String> {
    let structured = ["json_pointer", "json_value", "json_expected"]
        .iter()
        .any(|key| args.get(key).is_some());
    if !structured {
        return super::edit_preview::patch(
            path,
            original,
            args["search"].as_str().context("missing search")?,
            args["replace"].as_str().context("missing replace")?,
        );
    }
    ensure!(
        args.get("search").is_none() && args.get("replace").is_none(),
        "choose text search/replace OR JSON fields, never both"
    );
    let pointer = args["json_pointer"]
        .as_str()
        .context("missing json_pointer")?;
    ensure!(
        pointer.starts_with('/'),
        "json_pointer must name a member, not the entire document"
    );
    let value: Value = serde_json::from_str(
        args["json_value"]
            .as_str()
            .context("json_value must be a JSON-encoded string")?,
    )
    .context("invalid json_value; file unchanged")?;
    let mut document: Value =
        serde_json::from_str(original).context("existing file is not valid JSON")?;
    if let Some(parent) = pointer.strip_suffix("/-") {
        ensure!(
            args.get("json_expected").is_none(),
            "append does not accept json_expected"
        );
        let array = document
            .pointer_mut(parent)
            .and_then(Value::as_array_mut)
            .context("append pointer must address an existing array followed by /-")?;
        // Repeating a case append after a lost response must not duplicate its ID.
        if let Some(id) = value.get("id") {
            ensure!(
                !array.iter().any(|item| item.get("id") == Some(id)),
                "array already contains this id; read before retrying"
            );
        }
        array.push(value);
    } else {
        let expected: Value = serde_json::from_str(
            args["json_expected"]
                .as_str()
                .context("replace requires json_expected containing the current JSON value")?,
        )
        .context("invalid json_expected")?;
        let current = document
            .pointer_mut(pointer)
            .context("json_pointer does not exist")?;
        if current != &expected {
            bail!("json_expected does not match the current value; file unchanged; read before retrying");
        }
        *current = value;
    }
    Ok(format!("{}\n", serde_json::to_string_pretty(&document)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn appends_beyond_read_limit_without_reprinting_or_losing_old_data() {
        let original = json!({"cases":[{"id":"old","unknown":{"based_on":["keep"]},"large":"x".repeat(28000)}],"extension":{"retain":true}});
        let args = json!({"json_pointer":"/cases/-","json_value":"{\"id\":\"new\",\"checks\":[{\"kind\":\"hint_queued\"}]}"});
        let text = prepare("spec.json", &original.to_string(), &args).unwrap();
        let result: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(result["cases"][0], original["cases"][0]);
        assert_eq!(result["extension"], original["extension"]);
        assert_eq!(result["cases"][1]["id"], "new");
        assert!(prepare("spec.json", &text, &args).is_err());
    }

    #[test]
    fn compare_and_replace_handles_escaped_pointers_and_rejects_stale_or_ambiguous_edits() {
        let original = r#"{"a/b~c":{"flag":false},"keep":[1,2]}"#;
        let args =
            json!({"json_pointer":"/a~1b~0c/flag","json_value":"true","json_expected":"false"});
        let changed = prepare("spec.json", original, &args).unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&changed).unwrap(),
            json!({"a/b~c":{"flag":true},"keep":[1,2]})
        );
        assert!(prepare("spec.json", &changed, &args).is_err());
        for bad in [
            json!({"json_pointer":"/keep/-","json_value":"{"}),
            json!({"json_pointer":"/missing/-","json_value":"1"}),
            json!({"json_pointer":"","json_value":"{}","json_expected":original}),
            json!({"json_pointer":"/keep/-","json_value":"1","search":"x","replace":"y"}),
            json!({"json_pointer":"/keep/0","json_value":"3"}),
        ] {
            assert!(prepare("spec.json", original, &bad).is_err(), "{bad}");
        }
    }

    #[tokio::test]
    async fn preservation_still_blocks_changes_to_existing_replay_cases() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".spiral-coder")).unwrap();
        let path = ".spiral-coder/tui_replay.json";
        let original = r#"{"cases":[{"id":"old","checks":[{"kind":"hint_queued"}]}]}"#;
        std::fs::write(dir.path().join(path), original).unwrap();
        let budget = crate::work_budget::WorkBudget::new(
            "Append .spiral-coder/tui_replay.json; preserve every existing case",
            3,
            None,
        );
        budget.protect_replay(dir.path()).unwrap();
        crate::work_budget::scope(Some(crate::work_budget::Context { budget, role: "coder" }), async {
            let args = json!({"json_pointer":"/cases/0/checks","json_value":"[]","json_expected":"[{\"kind\":\"hint_queued\"}]"});
            let (output, failed) = super::super::tool_patch_file_args(path, &args, dir.path().to_str());
            assert!(failed, "{output}");
            assert_eq!(std::fs::read_to_string(dir.path().join(path)).unwrap(), original);
            let args = json!({"json_pointer":"/cases/-","json_value":"{\"id\":\"new\"}"});
            let (output, failed) = super::super::tool_patch_file_args(path, &args, dir.path().to_str());
            assert!(!failed, "{output}");
            assert!(output.starts_with("OK: patched '"));
        }).await;
    }
}
