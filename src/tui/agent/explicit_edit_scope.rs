//! Human-authored file allowlist for choosing native edit targets.
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Serialize)]
#[serde(transparent)]
pub(crate) struct Scope(Vec<String>);

impl Scope {
    pub(super) fn parse(root: &str) -> Option<Self> {
        if !root.contains("Allowed paths only:") {
            return None;
        }
        let mut scope = None;
        let mut fence: Option<(u8, usize)> = None;
        let mut lines = root.lines().peekable();
        while let Some(line) = lines.next() {
            let text = line.trim_start();
            let marker = text
                .as_bytes()
                .first()
                .copied()
                .filter(|c| matches!(c, b'`' | b'~'));
            let width = text.bytes().take_while(|c| Some(*c) == marker).count();
            if let Some((kind, n)) = fence {
                if marker == Some(kind) && width >= n && text[width..].trim().is_empty() {
                    fence = None;
                }
                continue;
            }
            if width >= 3 {
                fence = marker.map(|c| (c, width));
                continue;
            }
            if line.trim_end() != "Allowed paths only:" {
                continue;
            }
            if scope.is_some() {
                return None;
            }
            let mut paths = Vec::new();
            loop {
                while lines.next_if(|s| s.trim().is_empty()).is_some() {}
                let Some(line) = lines.next_if(|s| s.starts_with("- ")) else {
                    break;
                };
                let token = line[2..].trim();
                let path = token
                    .strip_prefix('`')
                    .and_then(|s| s.strip_suffix('`'))
                    .unwrap_or(token);
                if paths.len() >= 64
                    || path.len() > 240
                    || !path
                        .chars()
                        .all(|c| c.is_alphanumeric() || matches!(c, '.' | '/' | '_' | '-'))
                    || path.split('/').any(|p| matches!(p, "" | "." | ".."))
                {
                    return None;
                }
                if !paths.iter().any(|p| p == path) {
                    paths.push(path.to_owned())
                }
            }
            // An alternative bullet style or continuation makes a partial list unreliable.
            if paths.is_empty()
                || lines.peek().is_some_and(|l| {
                    !l.trim().is_empty()
                        && (l.starts_with(char::is_whitespace)
                            || l.starts_with(['*', '+', '-'])
                            || l.split_once(". ")
                                .is_some_and(|(n, _)| n.parse::<usize>().is_ok()))
                })
            {
                return None;
            }
            scope = Some(Self(paths));
        }
        scope
    }

    pub(super) fn allows(&self, path: &str, workspace: Option<&str>) -> bool {
        crate::task_scope::relative_path(path.trim(), workspace)
            .is_some_and(|path| self.0.contains(&path))
    }

    pub(crate) fn issue(
        &self,
        tool: &str,
        args: &Value,
        workspace: Option<&str>,
    ) -> Option<String> {
        if !matches!(tool, "patch_file" | "write_file" | "apply_diff") {
            return None;
        }
        self.path_issue(args["path"].as_str()?, workspace)
    }

    fn path_issue(&self, path: &str, workspace: Option<&str>) -> Option<String> {
        (!self.allows(path, workspace)).then(|| format!("[Edit Scope] The original task limits file edits to {}. Use a listed workspace-relative target. Proposed path: {}.",
            serde_json::json!(self.0), serde_json::json!(path)))
    }
}

pub(super) fn implied_write_issue(
    task: &str,
    files: &[super::ImpliedWriteFile],
    workspace: Option<&str>,
) -> Option<String> {
    let scope = Scope::parse(task)?;
    files
        .iter()
        .find_map(|file| scope.path_issue(&file.path, workspace))
}

/// A literal identifier lookup in a listed file or its immediate directory.
pub(super) fn symbol_lookup(task: &str, call: &crate::streaming::ToolCallData) -> bool {
    if call.name != "search_files" {
        return false;
    }
    let Some(scope) = Scope::parse(task) else {
        return false;
    };
    let Ok(args) = serde_json::from_str::<Value>(&call.arguments) else {
        return false;
    };
    let Some(pattern) = args["pattern"].as_str() else {
        return false;
    };
    let Some(dir) = args["dir"]
        .as_str()
        .and_then(|s| crate::task_scope::relative_path(s, None))
    else {
        return false;
    };
    (2..=96).contains(&pattern.len())
        && pattern.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
        && pattern
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
        && scope.0.iter().any(|path| {
            path == &dir
                || path
                    .rsplit_once('/')
                    .is_some_and(|(parent, _)| parent == dir)
        })
}

pub(super) fn native_issue(
    task: &str,
    call: &crate::streaming::ToolCallData,
    workspace: Option<&str>,
) -> Option<String> {
    if !matches!(
        call.name.as_str(),
        "patch_file" | "write_file" | "apply_diff"
    ) {
        return None;
    }
    Scope::parse(task)?.issue(
        &call.name,
        &serde_json::from_str(&call.arguments).ok()?,
        workspace,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::agent::{meta_harness::MetaHarness, task_harness::TaskHarness};
    use serde_json::json;

    const TASK: &str = "Fix the existing command in src/main.rs.\n\nAllowed paths only:\n- src/main.rs\n- src/tui_replay/catalog.rs\nKeep the protected tests unchanged.";

    #[test]
    fn a_mixed_text_only_file_batch_is_checked_before_writing() {
        let allowed = "File: src/tui_replay/catalog.rs\n```rust\nfn ok() {}\n```";
        let invalid = format!("{allowed}\nFile: scratch.rs\n```rust\nfn stray() {{}}\n```");
        let (_, files) = super::super::implied_actions::extract(&invalid);
        assert_eq!(files.len(), 2);
        let issue = implied_write_issue(TASK, &files, None).unwrap();
        assert!(issue.contains("[Edit Scope]") && issue.contains("scratch.rs"));
        assert!(implied_write_issue(TASK, &files[..1], None).is_none());
        assert!(implied_write_issue("Create files.", &files, None).is_none());
    }

    #[test]
    fn explicit_scope_filters_native_writes_and_observer_advice_but_permits_reads_and_new_listed_files(
    ) {
        let scope = Scope::parse(TASK).unwrap();
        let root = std::env::temp_dir();
        let direct = root.join("src/main.rs");
        for tool in ["apply_diff", "patch_file", "write_file"] {
            assert!(scope
                .issue(tool, &json!({"path":"src/task_scope.rs"}), root.to_str())
                .is_some());
            assert!(scope
                .issue(tool, &json!({"path":direct}), root.to_str())
                .is_none());
            assert!(scope
                .issue(
                    tool,
                    &json!({"path":"./src/tui_replay/catalog.rs"}),
                    root.to_str()
                )
                .is_none());
            assert!(scope
                .issue(tool, &json!({"path":"../src/main.rs"}), root.to_str())
                .is_some());
        }
        assert!(scope
            .issue("read_file", &json!({"path":"src/task_scope.rs"}), None)
            .is_none());
        let advice=json!({"suggestions":[{"kind":"edit","confidence":0.95,"reason":"edit the module","suggested_tool":"patch_file",
            "suggested_args":{"path":"src/task_scope.rs","search":"old","replace":"new"}}]}).to_string();
        let messages = vec![json!({"role":"user","content":TASK})];
        assert!(
            crate::observer_action::validation_issue(&advice, &messages, None)
                .unwrap()
                .contains("[Edit Scope]")
        );
        let mut changed = messages;
        changed.push(json!({"role":"user","content":"Fix src/task_scope.rs instead."}));
        assert!(crate::observer_action::validation_issue(&advice, &changed, None).is_none());
    }

    #[test]
    fn examples_partial_lists_and_required_artifacts_do_not_define_an_edit_allowlist() {
        for text in [
            "Edit the sources.\n````md\n```\nAllowed paths only:\n- src/a.rs\n````",
            "> Allowed paths only:\n> - src/a.rs",
            "Required files:\n- src/a.rs",
            "Allowed paths only:\n- src/a.rs\n- src/*.rs",
            "Allowed paths only:\n- src/a.rs\n  - src/b.rs",
            "Allowed paths only:\n- src/a.rs\n* src/b.rs",
            "Allowed paths only:\n- ../bad",
            "Allowed paths only:\n- a.rs\n\nAllowed paths only:\n- b.rs",
        ] {
            assert!(Scope::parse(text).is_none(), "{text}");
        }
    }

    #[test]
    fn blank_lines_between_bullets_preserve_the_entire_explicit_scope() {
        let scope =
            Scope::parse("Allowed paths only:\n\n- a.rs\n\n- `b.rs`\n\nThen verify.").unwrap();
        assert!(scope.allows("a.rs", None));
        assert!(scope.allows("b.rs", None));
        assert!(!scope.allows("c.rs", None));
    }

    #[test]
    fn repeated_reads_of_main_do_not_force_a_mutation_in_an_unlisted_module() {
        let mut messages = vec![json!({"role":"user","content":TASK})];
        for i in 0..3 {
            let id = format!("read-{i}");
            messages.extend([json!({"role":"assistant","tool_calls":[{"id":id,"function":{"name":"read_file","arguments":"{\"path\":\"src/main.rs\"}"}}]}),
                json!({"role":"tool","tool_call_id":id,"content":"[src/main.rs] (2 lines, 29 bytes)\nmod task_scope;\nfn main() {}\n"})]);
        }
        let meta = |m: &[Value]| {
            let harness = TaskHarness::infer(crate::task_origin::root_user_text(m), false);
            MetaHarness::analyze(harness, m, None, None, None).telemetry_payload()
        };
        let mut unrestricted = messages.clone();
        unrestricted[0]["content"] = json!("Fix the existing command in src/main.rs.");
        assert_eq!(
            meta(&unrestricted).unwrap()["next_target"],
            "src/task_scope.rs"
        );
        let target = meta(&messages).and_then(|p| p["next_target"].as_str().map(String::from));
        assert!(
            target
                .as_deref()
                .is_none_or(|p| p.split("::").next() == Some("src/main.rs")),
            "{target:?}"
        );
        messages.push(json!({"role":"user","content":TASK}));
        assert!(
            meta(&messages).is_none(),
            "previous-task reads must not force a fresh edit"
        );
    }
}
