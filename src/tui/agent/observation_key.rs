//! Execution arguments used to decide whether an observation is an exact retry.
use super::task_harness::normalize_observed_path;
use serde_json::{json, Value};

pub(super) fn key(name: &str, arguments: &str) -> Option<String> {
    if !super::task_harness::is_observation_tool(name) {
        return None;
    }
    let args: Value = serde_json::from_str(arguments).ok()?;
    let dir = || normalize_observed_path(args["dir"].as_str().unwrap_or_default());
    let pattern = || args["pattern"].as_str().unwrap_or_default();
    let key = match name {
        "read_file" => {
            let request: crate::file_tools::read_range::ReadRequest =
                serde_json::from_value(args).ok()?;
            let range = if request.is_range() {
                Some(request.bounds().ok()?)
            } else {
                None
            };
            json!([name, normalize_observed_path(&request.path), range])
        }
        "search_files" => json!([
            name,
            dir(),
            pattern(),
            args["case_insensitive"].as_bool().unwrap_or(false)
        ]),
        "glob" => json!([name, dir(), pattern()]),
        "list_dir" => json!([
            name,
            dir(),
            crate::file_tools::list_dir_limit(args["max_entries"].as_u64().unwrap_or(0) as usize),
            args["include_hidden"].as_bool().unwrap_or(false)
        ]),
        _ => return None,
    };
    Some(key.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn observation_identity_uses_complete_execution_arguments() {
        let read = |args: Value| key("read_file", &args.to_string()).unwrap();
        let long = format!("src/{}/", "x".repeat(180));
        let ranged = json!({"path":format!("{long}File2"),"start_line":5});
        assert_eq!(
            read(ranged.clone()),
            read(
                json!({"end_line":204,"path":format!("./{long}File2"),"start_line":5,"ignored":true})
            )
        );
        for path in [format!("{long}file2"), format!("{long}File3")] {
            assert_ne!(
                read(ranged.clone()),
                read(json!({"path":path,"start_line":5}))
            );
        }
        assert_ne!(
            read(ranged.clone()),
            read(json!({"path":format!("{long}File2")}))
        );
        let search = |pattern| {
            key(
                "search_files",
                &json!({"pattern":pattern,"dir":"."}).to_string(),
            )
        };
        assert_ne!(search("item 1"), search("Item 2"));
        assert_ne!(search("a  b"), search("a b"));
        assert_eq!(
            key("list_dir", "{}"),
            key(
                "list_dir",
                r#"{"dir":".","include_hidden":false,"max_entries":200}"#
            )
        );
    }
}
