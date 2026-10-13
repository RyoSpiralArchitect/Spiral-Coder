use super::*;

pub(super) fn build_tui_meta_failure_packet_for_selector(
    app: &App,
    selector: &str,
) -> Result<serde_json::Value, String> {
    let target_idx = resolve_tui_meta_target(&app.coder.messages, selector)?;
    let Some(target_msg) = app.coder.messages.get(target_idx) else {
        return Err(format!(
            "meta-diagnose: no coder message found for `coder-{target_idx}`"
        ));
    };
    let start = target_idx.saturating_sub(8);
    let base = &app.coder.messages[start..=target_idx];
    let recent_users: Vec<String> = base
        .iter()
        .filter(|m| matches!(m.role, Role::User) && m.complete)
        .rev()
        .take(3)
        .map(|m| meta_digest_text(&m.content, 220, 4))
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let recent_assistant: Vec<String> = base
        .iter()
        .enumerate()
        .filter(|(i, m)| {
            *i + start != target_idx && matches!(m.role, Role::Assistant) && m.complete
        })
        .rev()
        .take(3)
        .map(|(_, m)| meta_digest_text(&m.content, 220, 4))
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let recent_tool_results: Vec<serde_json::Value> = base
        .iter()
        .filter(|m| matches!(m.role, Role::Tool) && m.complete)
        .rev()
        .take(4)
        .map(|m| {
            json!({
                "digest": meta_digest_text(&m.content, 260, 6),
                "first_line": meta_first_line(&m.content, 220),
                "diagnostics": diagnostic_lines(&m.content),
            })
        })
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let intent_anchor = app
        .coder_intent_anchor
        .as_ref()
        .map(|anchor| {
            json!({
                "revision": anchor.revision,
                "goal": anchor.goal,
                "target": anchor.target,
                "constraints": anchor.constraints,
                "success_criteria": anchor.success_criteria,
                "optimization_hints": anchor.optimization_hints,
                "ambiguity": anchor.ambiguity,
                "confidence": anchor.confidence,
                "requires_human_confirmation": anchor.requires_human_confirmation,
                "baseline": intent::anchor_baseline(anchor),
            })
        })
        .unwrap_or(serde_json::Value::Null);
    let actual_outcome = meta_digest_text(&target_msg.content, 320, 8);
    let system_digest = meta_digest_text(
        &format!(
            "coder_mode={:?}\nobserver_mode={:?}\ncoder_provider={}\nobserver_provider={}",
            app.coder_cfg.mode,
            app.observer_cfg.mode,
            app.coder_cfg.provider,
            app.observer_cfg.provider
        ),
        320,
        8,
    );
    let task_summary = app
        .coder_intent_anchor
        .as_ref()
        .map(|anchor| anchor.goal.as_str())
        .filter(|goal| !goal.trim().is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| recent_users.last().cloned().unwrap_or_default());
    let available_tools = suggestion::tool_schemas();
    Ok(json!({
        "thread_id": app.tool_root.clone().unwrap_or_else(|| "tui-session".to_string()),
        "target_message_id": format!("coder-{}", target_idx),
        "task_summary": task_summary,
        "expected_outcome": app.coder_intent_anchor.as_ref().map(|anchor| anchor.success_criteria.join("; ")).filter(|text| !text.is_empty()).unwrap_or_else(|| task_summary.clone()),
        "actual_outcome": actual_outcome,
        "failure_kind": meta_failure_kind(&target_msg.content),
        "coder_mode": format!("{:?}", app.coder_cfg.mode),
        "coder_provider": app.coder_cfg.provider.to_string(),
        "coder_model": app.coder_cfg.model.clone(),
        "observer_model": app.observer_cfg.model.clone(),
        "tool_root": app.tool_root.clone(),
        "cur_cwd": app.tool_root.clone(),
        "checkpoint": app.last_git_checkpoint.clone(),
        "system_prompt_digest": system_digest,
        "project_context_digest": app.project_stack_label.clone(),
        "intent_anchor": intent_anchor,
        "agents_md_digest": serde_json::Value::Null,
        "available_tools": available_tools.as_array().into_iter().flatten().map(|schema| schema["name"].clone()).collect::<Vec<_>>(),
        "tool_argument_schemas": available_tools,
        "recovery_stage": app.coder_governor.as_ref().and_then(|state| state.recovery_stage.as_deref()),
        "recent_user_messages": recent_users,
        "recent_assistant_messages": recent_assistant,
        "recent_tool_calls": Vec::<serde_json::Value>::new(),
        "recent_tool_results": recent_tool_results,
        "last_error_digest": meta_first_line(&target_msg.content, 240),
        "loop_signals": {
            "same_command_repeats": 0,
            "same_error_repeats": 0,
            "same_output_repeats": 0,
            "ui_loop_depth": 0,
        },
        "approval_signals": Vec::<String>::new(),
        "packet_notes": vec![
            "tui meta-diagnose packet built from visible coder history".to_string(),
            "tui path does not preserve structured tool-call snapshots".to_string(),
            "intent_anchor and recent tool results are included when available".to_string(),
        ],
    }))
}

pub(super) fn build_tui_next_action_prompt(
    packet: &serde_json::Value,
    lang: &str,
    reason_hint: &str,
) -> String {
    let lang_name = match lang {
        "fr" => "French",
        "en" => "English",
        _ => "Japanese",
    };
    let reason = reason_hint.trim();
    let schema = json!({
        "summary": format!("{lang_name} summary of the blocker"),
        "primary_blocker": "missing_concrete_next_step",
        "scores": {
            "correctness": 0.82,
            "security": 0.61,
            "efficiency": 0.77,
            "readability": 0.80
        },
        "response_contract": {
            "required": true,
            "focus_axes": ["security"],
            "note": format!("{lang_name} note about when to accept, override, or defer")
        },
        "suggestions": [{
            "kind": "read",
            "reason": format!("{lang_name} reason"),
            "confidence": 0.84,
            "suggested_tool": "read_file",
            "suggested_args": { "path": "<exact path from packet>" },
            "based_on": ["intent_anchor", "recent_tool_results"]
        }],
        "quickest_check": "<same concrete action as the primary suggestion>",
        "why_this_first": format!("{lang_name} why this is the smallest next step"),
        "fallback": format!("{lang_name} fallback if the first action fails")
    });
    format!(
        "This is intervention mode, not critique.\n\
Tool calls, code changes, and diff application are forbidden.\n\
Your task is to help the Coder take the next concrete step only.\n\
Write all free-text explanations in {lang_name}. Keep enum values, tool names, and file paths in English.\n\
Return JSON only. No markdown. No backticks. No prose outside JSON.\n\n\
Output schema:\n\
{}\n\n\
Rules:\n\
- Prefer small, local, reversible actions.\n\
- Mention exact files, commands, or symbols when possible.\n\
- Respect intent_anchor.goal/target when present; do not widen scope.\n\
- If recent_tool_results already contain useful evidence, continue from that evidence instead of restarting broad search.\n\
- Use tool_argument_schemas as the tool catalog. Shell programs such as cargo are commands for exec, not tool names.\n\
- If the only gap is a required verification, suggest kind=verify with exec and the exact required command. Do not substitute another read or append commands.\n\
- Use kind=edit only when the packet supports the exact edit arguments; otherwise read the target first.\n\
- An attempted edit or command is not a successful result. Failed automatic tests preserve the edit but do not verify it.\n\
- Read retained diagnostics as well as short digests. A source path mentioned as data does not establish that the file exists.\n\
- If the blocker is repo code, say so directly.\n\
- If the blocker is instruction/harness/tooling, say so directly.\n\
- Do not broaden into a full review.\n\
- If evidence is weak, make quickest_check purely diagnostic.\n\
- scores must be normalized 0..1 floats for: correctness, security, efficiency, readability\n\
- response_contract.required should be true when the next coder turn must explicitly accept, override, or defer your primary suggestion\n\
- response_contract.focus_axes should name the dimensions that most need attention (correctness|security|efficiency|readability)\n\
- response_contract.note should briefly explain when override/defer is acceptable\n\
- suggestions.kind must be exactly one of: search | read | edit | verify | done | clarify | abandon_path\n\
- Match kind to tool: search=search_files/glob/list_dir; read=read_file; edit=patch_file/apply_diff/write_file; verify=exec; done=done. clarify/abandon_path have no tool.\n\
- suggestions.suggested_args must be an object (or empty object)\n\
- quickest_check is an optional display string, not an object. Put tool arguments only in suggestions.suggested_args.\n\
- suggestions.based_on should cite coarse facts only: intent_anchor, recent_tool_results, recovery_stage, failure_kind\n\
- Keep suggestions advisory-only; do not claim you already executed anything.\n\n\
reason_hint: {reason}\n\
stuck packet:\n\
<packet>\n\
{}\n\
</packet>",
        serde_json::to_string_pretty(&schema).unwrap_or_else(|_| "{}".to_string()),
        serde_json::to_string_pretty(packet).unwrap_or_else(|_| "{}".to_string())
    )
}

fn diagnostic_lines(content: &str) -> Vec<String> {
    // Preserve actionable failure lines hidden behind diff/output preambles.
    // These are quoted observations, not an execution-success signal.
    content
        .lines()
        .filter(|line| {
            let line = line.trim_start();
            line.starts_with("Error:")
                || line.starts_with("error:")
                || line.starts_with("FAILED (")
                || line.starts_with("[auto-test]")
                || line.starts_with("Required now:")
                || line.starts_with("fatal:")
        })
        .take(4)
        .map(|line| line.chars().take(360).collect())
        .collect()
}
