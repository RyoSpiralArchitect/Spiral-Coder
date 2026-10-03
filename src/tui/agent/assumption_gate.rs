use super::{
    governor_contract, keyword_tokens, mutation_target_path, normalize_memory_entry,
    parse_exec_command_from_args, token_overlap_score, AssumptionLedger, AssumptionStatus,
    ThinkBlock, ToolCallData,
};

pub(super) fn refuted_assumption_conflict(
    ledger: &AssumptionLedger,
    think: &ThinkBlock,
    tc: &ToolCallData,
) -> Option<String> {
    // Observations gather the new evidence requested by this gate. Mentioning a
    // refuted assumption while probing it does not mean relying on it.
    if matches!(
        tc.name.as_str(),
        "read_file" | "list_dir" | "search_files" | "glob"
    ) {
        return None;
    }
    let mut probe = format!("{} {}", think.goal, think.next);
    if tc.name == "exec" {
        if let Some(command) = parse_exec_command_from_args(tc.arguments.as_str()) {
            probe.push(' ');
            probe.push_str(command.as_str());
        }
    } else if let Some(path) = mutation_target_path(tc) {
        probe.push(' ');
        probe.push_str(path.as_str());
    }

    let probe_sig = normalize_memory_entry(probe.as_str());
    let probe_tokens = keyword_tokens(probe.as_str());
    if probe_sig.is_empty() && probe_tokens.is_empty() {
        return None;
    }

    for entry in ledger
        .entries
        .iter()
        .filter(|entry| entry.status == AssumptionStatus::Refuted)
    {
        if let Some(path) = refuted_file_existence(&entry.text) {
            // Only an existing-file edit depends on this exact file existing.
            // A literal in another file, a creation, or an exec scratchpad does
            // not establish that dependency. Never infer it from token overlap.
            if matches!(tc.name.as_str(), "patch_file" | "apply_diff")
                && mutation_target_path(tc).is_some_and(|target| same_path(&target, path))
            {
                return Some(render_conflict(entry));
            }
            continue;
        }
        let assumption_sig = normalize_memory_entry(entry.text.as_str());
        let assumption_tokens = keyword_tokens(entry.text.as_str());
        let overlap = token_overlap_score(&assumption_tokens, &probe_tokens);
        let exec_retry = tc.name == "exec" && overlap >= 0.50;
        if (!assumption_sig.is_empty()
            && (probe_sig.contains(assumption_sig.as_str())
                || assumption_sig.contains(probe_sig.as_str())))
            || overlap >= 0.75
            || exec_retry
        {
            return Some(render_conflict(entry));
        }
    }

    None
}

fn render_conflict(entry: &super::AssumptionEntry) -> String {
    let evidence = entry
        .evidence
        .as_deref()
        .map(|text| format!(" ({text})"))
        .unwrap_or_default();
    governor_contract::assumption_refuted_reuse_message(&entry.text, &evidence)
}

fn refuted_file_existence(text: &str) -> Option<&str> {
    const PREFIX: &str = "file exists at ";
    let text = text.trim();
    if !text.get(..PREFIX.len())?.eq_ignore_ascii_case(PREFIX) {
        return None;
    }
    let path = text[PREFIX.len()..].trim().trim_matches(['`', '\'', '"']);
    (!path.is_empty()).then_some(path)
}

fn same_path(left: &str, right: &str) -> bool {
    use std::path::{Component, Path};
    Path::new(left)
        .components()
        .filter(|part| *part != Component::CurDir)
        .eq(Path::new(right)
            .components()
            .filter(|part| *part != Component::CurDir))
}

pub(super) fn needs_fresh_file_read(ledger: &AssumptionLedger, path: &str) -> bool {
    ledger.entries.iter().any(|entry| {
        entry.status == AssumptionStatus::Refuted
            && refuted_file_existence(&entry.text).is_some_and(|missing| same_path(path, missing))
    })
}

/// Feed only a newly completed read/write result, never cached working-memory facts.
pub(super) fn confirm_file_existence_after_result(
    ledger: &mut AssumptionLedger,
    name: &str,
    path: &str,
    result: &str,
) {
    let header = result.lines().next().unwrap_or("");
    let observed = match name {
        "read_file" if !header.contains("[⚡ cached — unchanged since last read]") => header
            .strip_prefix('[')
            .and_then(|rest| rest.rsplit_once("] ("))
            .map(|(path, _)| path),
        "write_file" if crate::execution_evidence::file_edit_succeeded(name, result) => header
            .strip_prefix("OK: wrote '")
            .and_then(|rest| rest.rsplit_once("' ("))
            .map(|(path, _)| path),
        _ => None,
    };
    if !observed.is_some_and(|observed| same_path(observed, path)) {
        return;
    }
    for entry in &mut ledger.entries {
        if entry.status == AssumptionStatus::Refuted
            && refuted_file_existence(&entry.text).is_some_and(|missing| same_path(path, missing))
        {
            entry.status = AssumptionStatus::Confirmed;
            entry.evidence = Some(format!("{name}({path}) succeeded after refutation"));
        }
    }
}

/// Session reconstruction applies the same event-local update in transcript order.
#[derive(Default)]
pub(super) struct FileExistenceReplay {
    pending: std::collections::HashMap<String, (String, String)>,
}

impl FileExistenceReplay {
    pub(super) fn observe(&mut self, ledger: &mut AssumptionLedger, message: &serde_json::Value) {
        match message["role"].as_str() {
            Some("assistant") => {
                for call in message["tool_calls"].as_array().into_iter().flatten() {
                    let Some(id) = call["id"].as_str().filter(|id| !id.is_empty()) else {
                        continue;
                    };
                    let Some(name @ ("read_file" | "write_file")) =
                        call["function"]["name"].as_str()
                    else {
                        continue;
                    };
                    let Some(args) = call["function"]["arguments"]
                        .as_str()
                        .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok())
                    else {
                        continue;
                    };
                    if let Some(path) = args["path"].as_str() {
                        self.pending
                            .insert(id.to_string(), (name.to_string(), path.to_string()));
                    }
                }
            }
            Some("tool") => {
                if let Some((name, path)) = message["tool_call_id"]
                    .as_str()
                    .and_then(|id| self.pending.remove(id))
                {
                    confirm_file_existence_after_result(
                        ledger,
                        &name,
                        &path,
                        message["content"].as_str().unwrap_or(""),
                    );
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
#[path = "assumption_evidence_tests.rs"]
mod evidence_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ledger() -> AssumptionLedger {
        let mut ledger = AssumptionLedger::default();
        ledger.mark_refuted(
            "file exists at src/runtime/missing.rs",
            Some("read returned not found"),
        );
        ledger
    }

    fn think(tool: &str) -> ThinkBlock {
        ThinkBlock {
            goal: "add src/runtime/missing.rs to the registry".into(),
            step: 1,
            tool: tool.into(),
            risk: "wrong registry entry".into(),
            doubt: "inspect the exact target".into(),
            next: format!("{tool} src/runtime/registry.rs to add src/runtime/missing.rs"),
            verify: "run the configured checks".into(),
        }
    }

    fn call(name: &str, args: serde_json::Value) -> ToolCallData {
        ToolCallData {
            id: "next".into(),
            name: name.into(),
            arguments: args.to_string(),
        }
    }

    #[test]
    fn refuted_path_literal_does_not_block_another_existing_file_patch() {
        for name in ["patch_file", "apply_diff"] {
            let tc = call(
                name,
                json!({"path":"src/runtime/registry.rs",
                "search":"const PATHS: &[&str] = &[];",
                "replace":"const PATHS: &[&str] = &[\"src/runtime/missing.rs\"];",
                "diff":"@@ -1 +1 @@\n-old\n+src/runtime/missing.rs"}),
            );
            assert!(refuted_assumption_conflict(&ledger(), &think(name), &tc).is_none());
        }
    }

    #[test]
    fn refuted_existence_does_not_block_creation_or_new_diagnosis() {
        for (name, args) in [
            (
                "write_file",
                json!({"path":"src/runtime/missing.rs","content":"created"}),
            ),
            ("read_file", json!({"path":"src/runtime/missing.rs"})),
            ("list_dir", json!({"dir":"src/runtime"})),
            (
                "search_files",
                json!({"pattern":"missing","dir":"src/runtime"}),
            ),
            ("glob", json!({"pattern":"src/runtime/*.rs"})),
            (
                "exec",
                json!({"command":"cargo test -q runtime::registry 2>&1"}),
            ),
        ] {
            assert!(
                refuted_assumption_conflict(&ledger(), &think(name), &call(name, args)).is_none(),
                "{name}"
            );
        }
    }

    #[test]
    fn refuted_existing_target_still_blocks_patch_but_not_a_similar_path() {
        for name in ["patch_file", "apply_diff"] {
            let tc = call(name, json!({"path":"src/runtime/missing.rs"}));
            assert!(refuted_assumption_conflict(&ledger(), &think(name), &tc).is_some());
            let other = call(name, json!({"path":"src/runtime/missing.rs.backup"}));
            assert!(refuted_assumption_conflict(&ledger(), &think(name), &other).is_none());
        }
    }
}
