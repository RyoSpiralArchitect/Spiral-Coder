use serde_json::Value;
use std::collections::BTreeSet;

/// Explicit final-answer instructions need an authored handoff. The runtime
/// must not replace them with its generic verified-action summary.
pub(super) fn requires_authored_final_answer(root_user_text: &str) -> bool {
    root_user_text
        .to_ascii_lowercase()
        .contains("final answer must include")
}

pub(super) fn authored_final_answer_hint(root_user_text: &str) -> Option<String> {
    let instructions = root_user_text
        .lines()
        .filter_map(|line| {
            let start = line
                .to_ascii_lowercase()
                .find("final answer must include")?;
            Some(line[start..].trim())
        })
        .collect::<Vec<_>>();
    if instructions.is_empty() {
        return None;
    }
    Some(format!(
        "[Final handoff] Call `done` with a nonempty `summary` answering the original final-answer instruction. Use the verified evidence and state any remaining limitations; do not repeat tools solely to obtain a generic runtime summary.\nOriginal final-answer instruction:\n{}",
        instructions.join("\n")
    ))
}

pub(super) fn validate_authored_done_summary(
    root_user_text: &str,
    summary: &str,
) -> Result<(), String> {
    if summary.trim().is_empty() {
        if let Some(hint) = authored_final_answer_hint(root_user_text) {
            return Err(hint);
        }
    }
    Ok(())
}

pub(super) fn enrich_text_final_handoff(
    content: &str,
    root_user_text: &str,
    messages: &[Value],
    test_cmd: Option<&str>,
) -> Option<String> {
    if !content.trim_start().starts_with("[DONE]") {
        return None;
    }
    if !root_user_text
        .to_ascii_lowercase()
        .contains("final answer must include")
    {
        return None;
    }

    let mutated = successful_mutation_paths(messages);
    let mut missing_paths = path_literals_in_text(root_user_text)
        .into_iter()
        .filter(|path| mutated.contains(path))
        .filter(|path| !content.contains(path))
        .collect::<Vec<_>>();
    missing_paths.sort();
    missing_paths.dedup();

    let mut enriched = content.to_string();
    if !missing_paths.is_empty() {
        enriched.push_str("\n\nArtifacts:\n");
        for path in missing_paths {
            enriched.push_str("- `");
            enriched.push_str(path.as_str());
            enriched.push_str("`\n");
        }
    }

    if let Some(cmd) = test_cmd
        .map(str::trim)
        .filter(|cmd| !cmd.is_empty())
        .filter(|_cmd| {
            root_user_text
                .to_ascii_lowercase()
                .contains("verification command")
        })
        .filter(|cmd| !content.contains(cmd))
        .filter(|_cmd| successful_auto_test_seen(messages))
    {
        enriched.push_str("\nVerification:\n- `");
        enriched.push_str(cmd);
        enriched.push_str("`\n");
    }

    let mut missing_required_literals = required_non_command_final_literals(root_user_text)
        .into_iter()
        .filter(|literal| !content.contains(literal))
        .collect::<Vec<_>>();
    missing_required_literals.sort();
    missing_required_literals.dedup();
    if !missing_required_literals.is_empty() {
        enriched.push_str("\nRequired final answer items:\n");
        for literal in missing_required_literals {
            enriched.push_str("- `");
            enriched.push_str(literal.as_str());
            enriched.push_str("`\n");
        }
    }

    (enriched != content).then_some(enriched)
}

fn successful_mutation_paths(messages: &[Value]) -> BTreeSet<String> {
    let mut pending = std::collections::BTreeMap::new();
    let mut out = BTreeSet::new();

    for msg in messages {
        match msg.get("role").and_then(|v| v.as_str()).unwrap_or("") {
            "assistant" => {
                let Some(tool_calls) = msg.get("tool_calls").and_then(|v| v.as_array()) else {
                    continue;
                };
                for tc in tool_calls {
                    let id = tc.get("id").and_then(|v| v.as_str()).unwrap_or("").trim();
                    let name = tc
                        .get("function")
                        .and_then(|v| v.get("name"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .trim();
                    if id.is_empty() || !matches!(name, "patch_file" | "write_file") {
                        continue;
                    }
                    let Some(path) = tc
                        .get("function")
                        .and_then(|v| v.get("arguments"))
                        .and_then(|v| v.as_str())
                        .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
                        .and_then(|value| {
                            value
                                .get("path")
                                .and_then(|v| v.as_str())
                                .map(str::to_string)
                        })
                    else {
                        continue;
                    };
                    pending.insert(id.to_string(), path);
                }
            }
            "tool" => {
                let tool_call_id = msg
                    .get("tool_call_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim();
                let Some(path) = pending.remove(tool_call_id) else {
                    continue;
                };
                let content = msg.get("content").and_then(|v| v.as_str()).unwrap_or("");
                if content.starts_with("OK:") {
                    out.insert(path);
                }
            }
            _ => {}
        }
    }

    out
}

fn successful_auto_test_seen(messages: &[Value]) -> bool {
    messages.iter().any(|msg| {
        msg.get("role").and_then(|v| v.as_str()) == Some("tool")
            && msg
                .get("content")
                .and_then(|v| v.as_str())
                .is_some_and(|content| content.contains("[auto-test] ✓ PASSED"))
    })
}

fn path_literals_in_text(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for token in text.split_whitespace() {
        let trimmed = token
            .trim_matches(|c: char| {
                (c.is_ascii_punctuation() && !matches!(c, '.' | '/' | '\\' | '_' | '-'))
                    || matches!(
                        c,
                        '「' | '」'
                            | '『'
                            | '』'
                            | '（'
                            | '）'
                            | '('
                            | ')'
                            | '['
                            | ']'
                            | '{'
                            | '}'
                            | '`'
                            | '"'
                            | '\''
                    )
            })
            .trim_end_matches(|c: char| matches!(c, '.' | ',' | ';' | ':' | '!' | '?'))
            .trim_matches(|c: char| {
                matches!(
                    c,
                    '「' | '」'
                        | '『'
                        | '』'
                        | '（'
                        | '）'
                        | '('
                        | ')'
                        | '['
                        | ']'
                        | '{'
                        | '}'
                        | '`'
                        | '"'
                        | '\''
                )
            });
        let has_path_sep = trimmed.contains('/') || trimmed.contains('\\');
        let has_extension = trimmed
            .split('/')
            .next_back()
            .is_some_and(|segment| segment.contains('.'));
        if (has_path_sep || has_extension) && !trimmed.is_empty() {
            let literal = trimmed.replace('\\', "/");
            if !out.contains(&literal) {
                out.push(literal);
            }
        }
    }
    out
}

fn required_non_command_final_literals(root_user_text: &str) -> Vec<String> {
    if !root_user_text
        .to_ascii_lowercase()
        .contains("final answer must include")
    {
        return Vec::new();
    }
    backtick_literals(root_user_text)
        .into_iter()
        .filter(|literal| !looks_like_path_literal(literal))
        .filter(|literal| !looks_like_command_literal(literal))
        .collect()
}

fn backtick_literals(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some((_, after_open)) = rest.split_once('`') {
        let Some((literal, after_close)) = after_open.split_once('`') else {
            break;
        };
        let literal = literal.trim();
        if !literal.is_empty() && !out.iter().any(|existing| existing == literal) {
            out.push(literal.to_string());
        }
        rest = after_close;
    }
    out
}

fn looks_like_path_literal(literal: &str) -> bool {
    let normalized = literal.replace('\\', "/");
    normalized.contains('/')
        || normalized
            .split('/')
            .next_back()
            .is_some_and(|segment| segment.contains('.'))
}

fn looks_like_command_literal(literal: &str) -> bool {
    let low = literal.trim().to_ascii_lowercase();
    low.contains("&&")
        || low.contains(" 2>&1")
        || [
            "cargo ", "bash ", "python", "pytest", "npm ", "pnpm ", "yarn ", "bun ", "go ",
            "deno ", "test ", "cd ",
        ]
        .iter()
        .any(|prefix| low.starts_with(prefix))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn explicit_unquoted_final_instruction_requires_authored_handoff() {
        let instruction =
            "Final answer must include verification_receipt.txt and fresh exec proof.";
        let root = format!("Finish the verified task. {instruction}");
        assert!(requires_authored_final_answer(&root));
        let hint = authored_final_answer_hint(&root).expect("original instruction");
        assert!(hint.contains(instruction));
        assert!(hint.contains("nonempty `summary`"));
        assert!(!hint.contains("[DONE]"));
        assert_eq!(validate_authored_done_summary(&root, "   "), Err(hint));
        assert!(validate_authored_done_summary(
            &root,
            "Refreshed verification_receipt.txt after rerunning both checks; fresh exec proof is recorded."
        ).is_ok());
    }

    #[test]
    fn ordinary_tasks_keep_legacy_summary_fallback() {
        let root = "Finish the verified task.";
        assert!(!requires_authored_final_answer(root));
        assert!(authored_final_answer_hint(root).is_none());
        assert!(validate_authored_done_summary(root, "").is_ok());
    }

    #[test]
    fn enriches_done_with_missing_successful_mutation_path() {
        let messages = vec![
            json!({
                "role":"assistant",
                "tool_calls":[{
                    "id":"call_patch_src",
                    "type":"function",
                    "function":{"name":"patch_file","arguments":"{\"path\":\"src/tui/agent/followup_requirements.rs\",\"search\":\"old\",\"replace\":\"new\"}"}
                }]
            }),
            json!({
                "role":"tool",
                "tool_call_id":"call_patch_src",
                "content":"OK: patched 'src/tui/agent/followup_requirements.rs' (+1 lines, 25 total)"
            }),
            json!({
                "role":"assistant",
                "tool_calls":[{
                    "id":"call_patch_eval",
                    "type":"function",
                    "function":{"name":"patch_file","arguments":"{\"path\":\".spiral-coder/runtime_eval.json\",\"search\":\"old\",\"replace\":\"new\"}"}
                }]
            }),
            json!({
                "role":"tool",
                "tool_call_id":"call_patch_eval",
                "content":"OK: patched '.spiral-coder/runtime_eval.json' (+1 lines, 14 total)\n[auto-test] ✓ PASSED (exit 0)"
            }),
        ];
        let root = "Final answer must include `src/tui/agent/followup_requirements.rs`, `.spiral-coder/runtime_eval.json`, and the verification command.";
        let content = "[DONE]\nUpdated `src/tui/agent/followup_requirements.rs`.";

        let enriched =
            enrich_text_final_handoff(content, root, &messages, Some("cargo test -q demo 2>&1"))
                .expect("enriched final");

        assert!(enriched.contains(".spiral-coder/runtime_eval.json"));
        assert!(enriched.contains("cargo test -q demo 2>&1"));
    }

    #[test]
    fn ignores_non_done_text() {
        let enriched = enrich_text_final_handoff(
            "still working",
            "Final answer must include `src/lib.rs`.",
            &[],
            None,
        );
        assert!(enriched.is_none());
    }

    #[test]
    fn enriches_done_with_required_status_label_literal() {
        let content = "[DONE]\nUpdated `src/tui/agent/merge_approval.rs`.";
        let root = "Final answer must include the changed Rust file path, the verification command, and the status label `PR-ready merge approved`.";

        let enriched = enrich_text_final_handoff(content, root, &[], None).expect("enriched");

        assert!(enriched.contains("PR-ready merge approved"));
        assert!(enriched.contains("Required final answer items"));
    }

    #[test]
    fn does_not_add_command_literals_without_verified_command_evidence() {
        let content = "[DONE]\nUpdated `src/tui/agent/merge_approval.rs`.";
        let root = "Final answer must include `cargo test -q demo::tests:: 2>&1 && bash scripts/smoke.sh` and `PR-ready merge approved`.";

        let enriched = enrich_text_final_handoff(content, root, &[], None).expect("enriched");

        assert!(enriched.contains("PR-ready merge approved"));
        assert!(!enriched.contains("cargo test -q demo::tests::"));
    }
}
