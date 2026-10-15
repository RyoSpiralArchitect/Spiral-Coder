//! Diagnostic-only JSON append recipes derived from the unchanged input.
//! Never repairs a candidate, chooses an item, or writes a file automatically.
use serde::Serialize;
use serde_json::{value::RawValue, Value};

#[derive(Debug, Serialize)]
struct AppendRecipe {
    pointer: String,
    search: String,
    replace_prefix: String,
    replace_suffix: String,
}

fn recipes(content: &str) -> Vec<AppendRecipe> {
    let Ok(value) = serde_json::from_str::<Value>(content) else {
        return vec![];
    };
    let arrays: Vec<(String, &str)> = if value.is_array() {
        vec![(String::new(), content.trim())]
    } else if value.is_object() {
        let Ok(raw) =
            serde_json::from_str::<std::collections::BTreeMap<String, &RawValue>>(content)
        else {
            return vec![];
        };
        raw.into_iter()
            .filter(|(_, value)| value.get().starts_with('['))
            .map(|(key, value)| {
                (
                    format!("/{}", key.replace('~', "~0").replace('/', "~1")),
                    value.get(),
                )
            })
            .collect()
    } else {
        return vec![];
    };
    arrays
        .into_iter()
        .take(4)
        .filter_map(|(pointer, raw)| {
            let end = raw.as_ptr() as usize - content.as_ptr() as usize + raw.len();
            let close = end.checked_sub(1)?;
            let start_array = end - raw.len();
            let before_close = content[start_array + 1..close].trim_end();
            let insertion = start_array + 1 + before_close.len();
            let separator = if before_close.trim().is_empty() {
                ""
            } else {
                ","
            };
            // Grow an exact suffix until unique, bounded so large single-line files
            // do not flood a diagnostic or accidentally offer a truncated anchor.
            let mut start = insertion;
            loop {
                let search = &content[start..end];
                if content.matches(search).count() == 1 {
                    return Some(AppendRecipe {
                        pointer,
                        search: search.into(),
                        replace_prefix: format!("{}{separator}\n", &content[start..insertion]),
                        replace_suffix: content[insertion..end].into(),
                    });
                }
                if start == 0 || end - start >= 512 {
                    return None;
                }
                start -= 1;
                while !content.is_char_boundary(start) {
                    start -= 1;
                }
            }
        })
        .collect()
}

pub(super) fn guidance(content: &str) -> String {
    let recipes = recipes(content);
    if recipes.is_empty() {
        return String::new();
    }
    format!("\n[JSON append guidance — diagnostic only]\nIf the requested change appends an item, choose its array pointer below. Use patch_file with search unchanged and replace = replace_prefix + your complete JSON-encoded new item + replace_suffix. Concatenate decoded strings, not their JSON escape notation. Do not replace an array closing bracket with an object brace. Preserve every existing field. These recipes contain no new task content and grant no execution or verification proof.\n{}\n", serde_json::to_string(&recipes).unwrap())
}

pub(super) fn rejected(candidate: &str, original: &str) -> String {
    let Err(error) = serde_json::from_str::<Value>(candidate) else {
        return String::new();
    };
    let lines: Vec<_> = candidate.lines().collect();
    let index = error.line().saturating_sub(1);
    let context = lines
        .iter()
        .enumerate()
        .skip(index.saturating_sub(1))
        .take(3)
        .map(|(i, line)| format!("{} | {}", i + 1, line.chars().take(180).collect::<String>()))
        .collect::<Vec<_>>()
        .join("\n");
    format!("\nRejected candidate JSON at line {}, column {} (file unchanged; line numbers are display-only):\n{context}{}", error.line(), error.column(), guidance(original))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_append_recipes_preserve_nested_unknown_fields_and_escaped_strings() {
        for original in [
            "{\n  \"cases\": [{\"unknown\": {\"based_on\": [\"a\",\"b\"]}, \"text\": \"🌀]\\\"\"}],\n  \"more\": []\n}\n",
            "[1, 2, {\"same\": []}]", "{\"a/b~c\": [], \"other\": []}", "[   ]",
        ] {
            let baseline: Value = serde_json::from_str(original).unwrap();
            for recipe in recipes(original) {
                let item = serde_json::json!({"new":"猫 🌀 \"quote\""});
                let replacement = format!("{}{}{}", recipe.replace_prefix, item, recipe.replace_suffix);
                let candidate = super::super::edit_preview::patch("sample.json", original, &recipe.search, &replacement).unwrap();
                let result: Value = serde_json::from_str(&candidate).unwrap();
                let mut expected = baseline.clone();
                expected.pointer_mut(&recipe.pointer).unwrap().as_array_mut().unwrap().push(item);
                assert_eq!(result, expected);
            }
        }
    }

    #[test]
    fn malformed_candidate_reports_its_location_without_choosing_new_content() {
        let original = "{\n  \"cases\": [{\"keep\": true}]\n}";
        let diagnostic = rejected("{\n  \"cases\": [{\"keep\": true}}\n}", original);
        assert!(diagnostic.contains("line 2"));
        assert!(diagnostic.contains("file unchanged"));
        assert!(diagnostic.contains("/cases"));
        assert!(!diagnostic.contains("REPLACE_WITH"));
        assert!(guidance("{invalid}").is_empty());
        assert!(guidance("not json").is_empty());
        assert!(guidance(&format!("[\"{}\"]", "猫".repeat(4000))).len() < 2000);
    }

    #[tokio::test]
    async fn rejected_replay_patch_can_use_a_recipe_without_losing_old_cases() {
        use crate::work_budget::{scope, Context, WorkBudget};
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".spiral-coder")).unwrap();
        let path = ".spiral-coder/tui_replay.json";
        let original = "{\n  \"version\": 1,\n  \"cases\": [{\"id\":\"old\",\"unknown\":{\"based_on\":[\"keep\"]}}]\n}\n";
        std::fs::write(dir.path().join(path), original).unwrap();
        let budget = WorkBudget::new(
            "Append .spiral-coder/tui_replay.json; preserve every existing case",
            3,
            None,
        );
        budget.protect_replay(dir.path()).unwrap();
        scope(
            Some(Context {
                budget,
                role: "coder",
            }),
            async {
                let (failure, failed) = crate::file_tools::tool_patch_file(
                    path,
                    "]\n}",
                    "},\n{\"id\":\"new\"}]\n}",
                    dir.path().to_str(),
                );
                assert!(failed, "{failure}");
                assert!(failure.contains("Rejected candidate JSON"));
                assert!(failure.contains("/cases"));
                assert_eq!(
                    std::fs::read_to_string(dir.path().join(path)).unwrap(),
                    original
                );
                let (read, failed) = crate::file_tools::tool_read_file(path, dir.path().to_str());
                assert!(!failed);
                assert!(read.contains("JSON append guidance"));
                let recipe = recipes(original).pop().unwrap();
                let item = serde_json::json!({"id":"new","checks":[]});
                let replacement =
                    format!("{}{}{}", recipe.replace_prefix, item, recipe.replace_suffix);
                let (result, failed) = crate::file_tools::tool_patch_file(
                    path,
                    &recipe.search,
                    &replacement,
                    dir.path().to_str(),
                );
                assert!(!failed, "{result}");
                let mut expected: Value = serde_json::from_str(original).unwrap();
                expected["cases"].as_array_mut().unwrap().push(item);
                assert_eq!(
                    serde_json::from_str::<Value>(
                        &std::fs::read_to_string(dir.path().join(path)).unwrap()
                    )
                    .unwrap(),
                    expected
                );
            },
        )
        .await;
    }
}
