//! Bound identical observations without treating reading as repair evidence.
use crate::streaming::ToolCallData;
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Default)]
struct Observation {
    output: String,
    count: usize,
}

fn key(name: &str, args: &Value) -> Option<String> {
    (matches!(name, "read_file" | "search_files") && args.is_object())
        .then(|| format!("{name}:{}", args))
}

/// Cache annotations do not constitute new information. Pruned results cannot
/// establish that the same contents were seen twice.
fn native_observation<'a>(name: &str, args: &Value, text: &'a str) -> Option<&'a str> {
    if name == "search_files" {
        let header = text.lines().next()?;
        return ((header.starts_with("[search_files: '")
            || header.starts_with("[search_files] No matches for '"))
            && !header.contains(" [pruned "))
        .then_some(text);
    }
    let path = args["path"].as_str()?;
    let (header, body) = text.split_once('\n')?;
    let rest = header.strip_prefix(&format!("[{path}] ("))?;
    let (lines, rest) = rest.split_once(" lines, ")?;
    let (bytes, suffix) = rest.split_once(" bytes)")?;
    lines.parse::<usize>().ok()?;
    bytes.parse::<usize>().ok()?;
    if suffix.is_empty()
        || (crate::file_tools::read_range::matches_range_header(&args.to_string(), header)
            && !suffix.contains(" [pruned "))
    {
        return Some(text);
    }
    let marker = " [⚡ cached — unchanged since last read]";
    if suffix == marker {
        let original_header = header.strip_suffix(marker)?;
        return body
            .strip_prefix(original_header)
            .is_some_and(|rest| rest.starts_with('\n'))
            .then_some(body);
    }
    None
}

fn history(messages: &[Value]) -> BTreeMap<String, Observation> {
    let mut pending = BTreeMap::<&str, (&str, Value)>::new();
    let mut observations = BTreeMap::<String, Observation>::new();
    for message in crate::task_origin::current_task_messages(messages) {
        if message["role"] == "assistant" {
            for call in message["tool_calls"].as_array().into_iter().flatten() {
                if let (Some(id), Some(name), Some(args)) = (
                    call["id"].as_str().filter(|id| !id.is_empty()),
                    call["function"]["name"].as_str(),
                    call["function"]["arguments"]
                        .as_str()
                        .and_then(|s| serde_json::from_str::<Value>(s).ok()),
                ) {
                    pending.insert(id, (name, args));
                }
            }
        }
        if message["role"] != "tool" {
            continue;
        }
        let Some((name, args)) = message["tool_call_id"]
            .as_str()
            .and_then(|id| pending.remove(id))
        else {
            continue;
        };
        let text = message["content"].as_str().unwrap_or("");
        if matches!(name, "exec" | "patch_file" | "write_file" | "apply_diff")
            && crate::execution_evidence::file_edit_outcome(name, text)
                != Some(crate::execution_evidence::FileEditOutcome::Unchanged)
            && !text.trim_start().starts_with("Awaiting approval")
            && crate::execution_evidence::exec_may_have_run(text)
        {
            // Even a failed attempt can introduce new diagnostic information
            // or partial changes. A rejection cannot reset observation pressure.
            observations.clear();
            continue;
        }
        let Some(key) = key(name, &args) else {
            continue;
        };
        let Some(output) = native_observation(name, &args, text) else {
            // Refusals do not undo known reads. Actual errors/unknown results
            // invalidate comparison and allow a fresh diagnostic observation.
            if crate::execution_evidence::exec_may_have_run(text)
                && !text.trim_start().starts_with("Awaiting approval")
            {
                observations.remove(&key);
            }
            continue;
        };
        let previous = observations.entry(key).or_default();
        previous.count = if previous.output == output {
            previous.count + 1
        } else {
            1
        };
        previous.output = output.to_owned();
    }
    observations
}

fn read_key(args: &Value) -> Option<(String, Option<u64>, Option<u64>)> {
    Some((
        args["path"].as_str()?.trim_start_matches("./").to_owned(),
        args["start_line"].as_u64(),
        args["end_line"].as_u64(),
    ))
}

/// How many consecutive identical native results the same read arguments
/// returned in this task, if reading the file now would return them again.
/// Edits of other files and executed commands do not make such a read new:
/// the file is read from disk and compared byte for byte with the latest
/// result, so a change made by any command does. R read its unchanged
/// protected path check eight times across two iterations while the failure
/// stayed the same.
fn unchanged_read_repeats(messages: &[Value], args: &Value, tool_root: Option<&str>) -> usize {
    let Some(wanted) = read_key(args) else {
        return 0;
    };
    let mut latest: Option<String> = None;
    let mut count = 0;
    for (call, message) in super::recovery_focus::correlated_results(messages) {
        if call.name != "read_file" {
            continue;
        }
        let Ok(call_args) = serde_json::from_str::<Value>(&call.arguments) else {
            continue;
        };
        if read_key(&call_args).as_ref() != Some(&wanted) {
            continue;
        }
        let text = message["content"].as_str().unwrap_or("");
        match native_observation("read_file", &call_args, text) {
            Some(output) if latest.as_deref() == Some(output) => count += 1,
            Some(output) => {
                latest = Some(output.to_owned());
                count = 1;
            }
            // Actual errors invalidate the comparison; refusals do not.
            None if crate::execution_evidence::exec_may_have_run(text)
                && !text.trim_start().starts_with("Awaiting approval") =>
            {
                latest = None;
                count = 0;
            }
            None => {}
        }
    }
    let Some(latest) = latest else {
        return 0;
    };
    let (current, failed) = if args.get("start_line").is_some() || args.get("end_line").is_some() {
        crate::file_tools::read_range::execute_args(args, tool_root)
    } else {
        crate::file_tools::tool_read_file(args["path"].as_str().unwrap_or(""), tool_root)
    };
    if failed || current != latest {
        return 0;
    }
    count
}

/// A read that would return, for the third time or more, the same bytes the
/// same arguments already returned while the file stayed unchanged. The
/// diagnosis of a failed edit or check keeps its own read requirement, so a
/// pending target read or required diagnostic read never counts. Both the
/// Coder's call and an Observer suggestion are checked with this.
pub(crate) fn unchanged_reread(
    messages: &[Value],
    tool: &str,
    args: &Value,
    tool_root: Option<&str>,
    test_cmd: Option<&str>,
) -> Option<(String, usize)> {
    if tool != "read_file" {
        return None;
    }
    let path = args["path"].as_str()?.trim_start_matches("./").to_owned();
    let repeats = unchanged_read_repeats(messages, args, tool_root);
    if repeats < 2 {
        return None;
    }
    let context =
        super::exec_verification::ExecVerificationContext::from_messages(test_cmd, messages);
    if super::edit_failure::EditFailureMemory::from_messages(messages, &context)
        .needs_target_read(tool, &args.to_string())
    {
        return None;
    }
    if super::task_obligations::project_advisory(messages, test_cmd, tool_root)
        .required_diagnostic_read
        .is_some_and(|read| read.path.trim_start_matches("./") == path)
    {
        return None;
    }
    Some((path, repeats))
}

/// Failing case labels from the latest executed command, when it failed.
fn latest_failure(messages: &[Value]) -> String {
    let Some((command, text)) = super::recovery_focus::correlated_results(messages)
        .filter(|(call, _)| call.name == "exec")
        .last()
        .map(|(call, message)| {
            (
                serde_json::from_str::<Value>(&call.arguments)
                    .ok()
                    .and_then(|args| args["command"].as_str().map(str::to_owned))
                    .unwrap_or_default(),
                message["content"].as_str().unwrap_or("").to_owned(),
            )
        })
    else {
        return String::new();
    };
    if !text.contains("FAILED (exit_code: ") {
        return String::new();
    }
    let mut cases: Vec<String> = Vec::new();
    for line in text.lines().map(str::trim) {
        let case = if let Some(rest) = line
            .strip_prefix("FAIL: ")
            .or_else(|| line.strip_prefix("ERROR: "))
        {
            let mut parts = rest.split(" (");
            let name = parts.next().unwrap_or("").trim();
            let params: Vec<&str> = parts
                .filter(|part| part.contains('='))
                .map(|part| part.trim_end_matches(')'))
                .collect();
            if params.is_empty() {
                name.to_owned()
            } else {
                format!("{name} ({})", params.join(", "))
            }
        } else if let Some(name) = line
            .strip_prefix("test ")
            .and_then(|rest| rest.strip_suffix(" ... FAILED"))
        {
            name.to_owned()
        } else {
            continue;
        };
        if !case.is_empty() && !cases.contains(&case) {
            cases.push(case);
        }
    }
    if cases.is_empty() {
        return String::new();
    }
    let shown = cases.iter().take(6).cloned().collect::<Vec<_>>().join("; ");
    format!(
        "\nThe latest command, `{command}`, still fails in {} case(s): {shown}{}.",
        cases.len(),
        if cases.len() > 6 { "; …" } else { "" }
    )
}

/// The Coder's third identical read of an unchanged file is refused (see
/// `unchanged_reread`), in any recovery stage: a required diagnostic read is
/// never refused, and other reads remain available for a diagnosis.
pub(super) fn unchanged_read_block(
    messages: &[Value],
    call: &ToolCallData,
    tool_root: Option<&str>,
    test_cmd: Option<&str>,
) -> Option<String> {
    let args = serde_json::from_str::<Value>(&call.arguments).ok()?;
    let (path, repeats) = unchanged_reread(messages, &call.name, &args, tool_root, test_cmd)?;
    Some(format!(
        "[Progress Gate]\n{path} has not changed since these read_file arguments last returned it: the same native result came back {repeats} times in this task, across later edits and commands. Reading it again adds no evidence.{}\nWork from the content already read: compare it with the latest failure and change the code that produces that result, or read a different range or file for a fact not seen yet. No edit is forced by this gate.",
        latest_failure(messages)
    ))
}

pub(super) fn previously_observed(messages: &[Value], call: &ToolCallData) -> bool {
    let args = serde_json::from_str::<Value>(&call.arguments).unwrap_or_default();
    key(&call.name, &args).is_some_and(|key| history(messages).contains_key(&key))
}

pub(super) fn block(messages: &[Value], call: &ToolCallData) -> Option<String> {
    let args = serde_json::from_str::<Value>(&call.arguments).ok()?;
    let key = key(&call.name, &args)?;
    let history = history(messages);
    let observation = history.get(&key)?;
    if observation.count < 2 {
        return None;
    }
    Some(format!(
        "[Progress Gate]\nThe identical {} arguments returned the same native output {} times without an intervening edit or executed command. This repeated observation supplies no new evidence.\nName the specific missing fact and choose an unread range, a different targeted query, or a behavioral check that tests the repair hypothesis. If diagnosis is sufficient, make the smallest justified edit and verify it. If blocked, report the unresolved requirement honestly. Do not repeat this exact observation or claim that reading repairs the task. No edit is forced by this gate.",
        call.name, observation.count
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn call(args: Value) -> ToolCallData {
        ToolCallData {
            id: "next".into(),
            name: "read_file".into(),
            arguments: args.to_string(),
            thought_signature: None,
        }
    }
    fn tool(messages: &mut Vec<Value>, name: &str, args: Value, result: &str) {
        let id = format!("call{}", messages.len());
        messages.push(json!({"role":"assistant","tool_calls":[{"id":id,"function":{"name":name,"arguments":args.to_string()}}]}));
        messages.push(json!({"role":"tool","tool_call_id":id,"content":result}));
    }
    fn repeated() -> Vec<Value> {
        let mut messages = vec![crate::task_origin::user_message(
            "Repair src/a.rs",
            crate::task_origin::MessageOrigin::User,
        )];
        for _ in 0..2 {
            tool(
                &mut messages,
                "read_file",
                json!({"path":"src/a.rs","start_line":1,"end_line":20}),
                "[src/a.rs] (30 lines, 100 bytes) [range: lines 1-20; partial file]\nsame body",
            );
        }
        messages
    }

    #[test]
    fn unchanged_edits_preserve_observation_pressure_review_regression() {
        for name in ["write_file", "patch_file", "apply_diff"] {
            let mut messages = repeated();
            tool(
                &mut messages,
                name,
                json!({"path":"src/a.rs"}),
                "OK: unchanged 'src/a.rs'",
            );
            assert!(block(
                &messages,
                &call(json!({"path":"src/a.rs","start_line":1,"end_line":20}))
            )
            .is_some());
        }
    }

    #[test]
    fn identical_successful_ranges_block_but_new_range_and_arguments_do_not() {
        let messages = repeated();
        assert!(block(
            &messages,
            &call(json!({"end_line":20,"path":"src/a.rs","start_line":1}))
        )
        .is_some());
        for args in [
            json!({"path":"src/a.rs","start_line":21,"end_line":30}),
            json!({"path":"src/b.rs","start_line":1,"end_line":20}),
            json!({"path":"src/a.rs"}),
        ] {
            assert!(block(&messages, &call(args)).is_none());
        }
    }

    #[test]
    fn cache_wrapping_does_not_make_an_identical_full_read_new() {
        let mut messages = vec![];
        let args = json!({"path":"a"});
        tool(
            &mut messages,
            "read_file",
            args.clone(),
            "[a] (1 lines, 1 bytes)\nx",
        );
        tool(&mut messages, "read_file", args.clone(), "[a] (1 lines, 1 bytes) [⚡ cached — unchanged since last read]\n[a] (1 lines, 1 bytes)\nx");
        assert!(block(&messages, &call(args)).is_some());
    }

    #[test]
    fn changed_result_pruning_and_new_task_invalidate_old_pressure() {
        let args = json!({"path":"src/a.rs","start_line":1,"end_line":20});
        for result in [
            "[src/a.rs] (30 lines, 100 bytes) [range: lines 1-20; partial file]\nnew body",
            "[src/a.rs] (30 lines, 100 bytes) [range: lines 1-20; partial file] [pruned 21L]",
            "ERROR reading src/a.rs",
        ] {
            let mut messages = repeated();
            tool(&mut messages, "read_file", args.clone(), result);
            assert!(block(&messages, &call(args.clone())).is_none());
        }
        let mut messages = repeated();
        messages.push(crate::task_origin::user_message(
            "New task",
            crate::task_origin::MessageOrigin::User,
        ));
        assert!(block(&messages, &call(args)).is_none());
    }

    #[test]
    fn attempts_reset_but_refusals_and_runtime_handoffs_do_not() {
        let next = call(json!({"path":"src/a.rs","start_line":1,"end_line":20}));
        for (name, result, resets) in [
            ("exec", "OK (exit_code: 0)", true),
            ("exec", "FAILED (exit_code: 1)", true),
            ("patch_file", "ERROR: search not found", true),
            ("patch_file", "OK: patched 'a'", true),
            ("exec", "GOVERNOR BLOCKED\nOK (exit_code: 0)", false),
            ("patch_file", "REJECTED BY USER", false),
        ] {
            let mut messages = repeated();
            tool(
                &mut messages,
                name,
                json!({"command":"check","path":"a"}),
                result,
            );
            assert_eq!(
                block(&messages, &next).is_none(),
                resets,
                "{name}: {result}"
            );
        }
        let mut messages = repeated();
        messages.push(crate::task_origin::user_message(
            "Continue.",
            crate::task_origin::MessageOrigin::Runtime,
        ));
        assert!(block(&messages, &next).is_some());
    }

    const PATHS: &str = "python3 -S -m unittest -v scripts.status_paths";

    /// Sealed shape of R iterations 5–6: the protected path check failed with
    /// the same six dangling-symlink cases, raised at its helper's line 25.
    fn paths_failure(root: &std::path::Path) -> String {
        let mut body = String::new();
        for case in [
            "running/iteration-02",
            "running/iteration-02/state",
            "running/iteration-02/state/tui-work.json",
            "interrupted/iteration-02",
            "interrupted/iteration-02/state",
            "interrupted/iteration-02/state/tui-work.json",
        ] {
            body.push_str(&format!("FAIL: test_symlink_components_even_dangling_are_rejected (scripts.status_paths.StatusPaths.test_symlink_components_even_dangling_are_rejected) (case='{case}/dangling=True')\nTraceback (most recent call last):\n  File \"{}/scripts/status_paths.py\", line 25, in invoke\nAssertionError: True != False : {case}/dangling=True: \n", root.display()));
        }
        body.push_str("Ran 3 tests in 1.364s\n\nFAILED (failures=6)\n");
        crate::tui::agent::inject_cwd(
            &format!("FAILED (exit_code: 1)\n{body}"),
            &format!("cwd: {}", root.display()),
            None,
        )
    }

    #[test]
    fn unchanged_rereads_stop_for_both_roles_but_required_reads_never_do() {
        // R's failure shape: the protected path check fails with the same six
        // dangling-symlink cases, located in its own helper. Each failure
        // requires reading that location. A second, unchanged file (the
        // fixture helper) is re-read alongside it, which nothing requires.
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("scripts")).unwrap();
        let mut source = String::from("import unittest\n");
        for line in 2..=40 {
            source.push_str(&format!("# protected path matrix line {line}\n"));
        }
        std::fs::write(dir.path().join("scripts/status_paths.py"), &source).unwrap();
        std::fs::write(
            dir.path().join("scripts/status_fixture.py"),
            "def make(root, state):\n    pass\n",
        )
        .unwrap();
        let root = dir.path().to_str();
        let task = format!(
            "Implement coupled-status.\n<task_requirements>\n{}\n</task_requirements>",
            json!({"version":1,"requirements":[{"id":"paths","description":"Reject symlinks below the run directory.",
                "artifacts":[{"path":"src/coupled_status.rs","kind":"source"}],"checks":[{"command":PATHS}]}]})
        );
        let mut messages = vec![crate::task_origin::user_message(
            &task,
            crate::task_origin::MessageOrigin::User,
        )];
        // Native results carry the live recovery snapshot, as in the agent loop.
        let context =
            super::super::exec_verification::ExecVerificationContext::from_root(Some(PATHS), &task);
        let mut focus = super::super::recovery_focus::RecoveryFocus::default();
        let mut native = |messages: &mut Vec<Value>, name: &str, args: Value, result: &str| {
            let tc = ToolCallData {
                id: format!("call{}", messages.len()),
                name: name.into(),
                arguments: args.to_string(),
                thought_signature: None,
            };
            focus.record_result(&tc, result, root, Some(PATHS), &context);
            messages.push(json!({"role":"assistant","tool_calls":[{"id":tc.id,"function":{"name":name,"arguments":tc.arguments}}]}));
            messages.push(focus.tool_message(&tc, result.to_owned()));
        };
        let check = json!({"path":"scripts/status_paths.py"});
        let fixture = json!({"path":"scripts/status_fixture.py"});
        let read = |path: &str| crate::file_tools::tool_read_file(path, root).0;
        let failure = paths_failure(dir.path());
        let suggest = |args: &Value| {
            json!({"suggestions":[{"kind":"read","reason":"re-read it",
                "confidence":0.9,"suggested_tool":"read_file","suggested_args":args}]})
            .to_string()
        };
        let observer = |messages: &[Value], args: &Value| {
            crate::observer_action::validation_issue_with_capacity(
                &suggest(args),
                messages,
                None,
                root,
                Some(12),
            )
        };

        for round in 0..2 {
            native(&mut messages, "exec", json!({"command":PATHS}), &failure);
            native(
                &mut messages,
                "read_file",
                check.clone(),
                &read("scripts/status_paths.py"),
            );
            assert_eq!(
                unchanged_read_block(&messages, &call(fixture.clone()), root, None),
                None,
                "round {round}"
            );
            assert_eq!(observer(&messages, &fixture), None, "round {round}");
            native(
                &mut messages,
                "read_file",
                fixture.clone(),
                &read("scripts/status_fixture.py"),
            );
            native(
                &mut messages,
                "patch_file",
                json!({"path":"src/coupled_status.rs"}),
                "OK: patched 'src/coupled_status.rs' (+0 lines, 287 total)",
            );
        }
        native(&mut messages, "exec", json!({"command":PATHS}), &failure);
        // The read each failure requires is never refused, however unchanged.
        assert_eq!(
            super::super::task_obligations::project(&messages, None)
                .required_diagnostic_read
                .map(|read| read.path)
                .as_deref(),
            Some("scripts/status_paths.py")
        );
        assert_eq!(
            unchanged_read_block(&messages, &call(check.clone()), root, None),
            None
        );
        assert_eq!(observer(&messages, &check), None);
        native(
            &mut messages,
            "read_file",
            check.clone(),
            &read("scripts/status_paths.py"),
        );

        let block = unchanged_read_block(&messages, &call(fixture.clone()), root, None)
            .expect("the third identical read of the unchanged fixture helper");
        assert!(block.starts_with("[Progress Gate]\nscripts/status_fixture.py has not changed since these read_file arguments last returned it: the same native result came back 2 times"), "{block}");
        assert!(block.contains(
            "The latest command, `python3 -S -m unittest -v scripts.status_paths`, still fails in 6 case(s): test_symlink_components_even_dangling_are_rejected (case='running/iteration-02/dangling=True'); test_symlink_components_even_dangling_are_rejected (case='running/iteration-02/state/dangling=True');"
        ));
        assert!(observer(&messages, &fixture).unwrap().contains(
            "This read_file of scripts/status_fixture.py would return the same native result the Coder already received 2 times in this task"
        ));

        // Another range and a changed file remain new evidence.
        let range = json!({"path":"scripts/status_fixture.py","start_line":1,"end_line":1});
        assert_eq!(
            unchanged_read_block(&messages, &call(range), root, None),
            None
        );
        std::fs::write(
            dir.path().join("scripts/status_fixture.py"),
            "def make(root, state):\n    return 1\n",
        )
        .unwrap();
        assert_eq!(
            unchanged_read_block(&messages, &call(fixture.clone()), root, None),
            None
        );
        assert_eq!(observer(&messages, &fixture), None);
    }

    #[test]
    fn only_correlated_native_results_count_including_searches() {
        let mut messages = vec![
            json!({"role":"assistant","content":"[search_files: 'x' — 0 match(es)]"}),
            json!({"role":"tool","tool_call_id":"missing","content":"[search_files: 'x' — 0 match(es)]"}),
        ];
        let next = ToolCallData {
            name: "search_files".into(),
            arguments: json!({"pattern":"x","dir":"src"}).to_string(),
            ..call(json!({}))
        };
        assert!(block(&messages, &next).is_none());
        for _ in 0..2 {
            tool(
                &mut messages,
                "search_files",
                json!({"pattern":"x","dir":"src"}),
                "[search_files: 'x' — 0 match(es)]\n",
            );
        }
        assert!(block(&messages, &next).is_some());
    }
}
