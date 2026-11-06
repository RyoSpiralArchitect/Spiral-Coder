use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DoneAcceptanceEvidence {
    pub(crate) criterion: String,
    pub(crate) command: String,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct StablePromptCache {
    pub(crate) resolver_hash: Option<u64>,
    pub(crate) task_harness_hash: Option<u64>,
    pub(crate) meta_harness_hash: Option<u64>,
    pub(crate) evaluator_loop_hash: Option<u64>,
    pub(crate) harness_evolution_promoted_hash: Option<u64>,
    pub(crate) harness_evolution_hash: Option<u64>,
    pub(crate) progress_bridge_hash: Option<u64>,
    pub(crate) session_bridge_hash: Option<u64>,
    pub(crate) task_contract_hash: Option<u64>,
    pub(crate) working_memory_hash: Option<u64>,
    pub(crate) assumption_ledger_hash: Option<u64>,
    pub(crate) reflection_ledger_hash: Option<u64>,
    pub(crate) verification_hash: Option<u64>,
}

pub(crate) fn keyword_tokens(s: &str) -> std::collections::BTreeSet<String> {
    s.split(|ch: char| !ch.is_ascii_alphanumeric())
        .map(|part| part.trim().to_ascii_lowercase())
        .filter(|part| part.len() >= 3)
        .collect()
}

pub(crate) fn token_overlap_score(
    a: &std::collections::BTreeSet<String>,
    b: &std::collections::BTreeSet<String>,
) -> f32 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let overlap = a.intersection(b).count() as f32;
    let denom = a.len().min(b.len()).max(1) as f32;
    (overlap / denom).clamp(0.0, 1.0)
}

pub(crate) fn parse_search_hit_count(content: &str) -> usize {
    let first = content.lines().next().unwrap_or("");
    let Some(idx) = first.find("—") else {
        return 0;
    };
    let tail = first[idx + "—".len()..].trim();
    let digits = tail
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>();
    digits.parse::<usize>().ok().unwrap_or(0)
}

pub(crate) fn parse_search_result_paths(content: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in content.lines().skip(1) {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('[') {
            continue;
        }
        let Some((path, _rest)) = trimmed.split_once(':') else {
            continue;
        };
        let path = compact_one_line(path.trim(), 160);
        if !path.is_empty() {
            remember_recent_unique(&mut out, path.as_str(), 8, 160);
        }
    }
    out
}

pub(crate) fn parse_read_file_result_path(content: &str) -> Option<String> {
    let first = content.lines().next().unwrap_or("").trim();
    let inner = first.strip_prefix('[')?.split(']').next()?.trim();
    let path = compact_one_line(inner, 160);
    if path.is_empty() {
        None
    } else {
        Some(path)
    }
}

pub(crate) fn collect_observation_evidence(messages: &[serde_json::Value]) -> ObservationEvidence {
    #[derive(Debug, Clone)]
    enum PendingObservation {
        Search(ObservationSearchEvidence),
        Read(ObservationReadEvidence),
    }

    let mut pending: std::collections::HashMap<String, PendingObservation> =
        std::collections::HashMap::new();
    let mut evidence = ObservationEvidence::default();

    for msg in messages {
        let role = msg
            .get("role")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        if role == "assistant" {
            let Some(tool_calls) = msg.get("tool_calls").and_then(|v| v.as_array()) else {
                continue;
            };
            for tc in tool_calls {
                let id = tc.get("id").and_then(|v| v.as_str()).unwrap_or("").trim();
                if id.is_empty() {
                    continue;
                }
                let name = tc
                    .get("function")
                    .and_then(|f| f.get("name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim();
                let args = tc
                    .get("function")
                    .and_then(|f| f.get("arguments"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim();
                let parsed = serde_json::from_str::<serde_json::Value>(args).ok();
                match name {
                    "search_files" => {
                        let command =
                            canonicalize_tool_call_command(name, args).unwrap_or_default();
                        let pattern = parsed
                            .as_ref()
                            .and_then(|v| v.get("pattern"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .trim()
                            .to_string();
                        pending.insert(
                            id.to_string(),
                            PendingObservation::Search(ObservationSearchEvidence {
                                command,
                                pattern,
                                hit_count: 0,
                                paths: Vec::new(),
                            }),
                        );
                    }
                    "read_file" => {
                        let command =
                            canonicalize_tool_call_command(name, args).unwrap_or_default();
                        let path = parsed
                            .as_ref()
                            .and_then(|v| v.get("path"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .trim()
                            .to_string();
                        pending.insert(
                            id.to_string(),
                            PendingObservation::Read(ObservationReadEvidence { command, path }),
                        );
                    }
                    _ => {}
                }
            }
            continue;
        }

        if role != "tool" {
            continue;
        }

        let tool_call_id = msg
            .get("tool_call_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        if tool_call_id.is_empty() {
            continue;
        }
        let Some(pending_obs) = pending.remove(tool_call_id) else {
            continue;
        };
        let content = msg.get("content").and_then(|v| v.as_str()).unwrap_or("");
        if !non_exec_tool_succeeded(content) {
            continue;
        }

        match pending_obs {
            PendingObservation::Search(mut search) => {
                search.hit_count = parse_search_hit_count(content);
                search.paths = parse_search_result_paths(content);
                evidence.searches.push(search);
            }
            PendingObservation::Read(mut read) => {
                if read.path.trim().is_empty() {
                    if let Some(parsed_path) = parse_read_file_result_path(content) {
                        read.path = parsed_path;
                    }
                }
                if !read.path.trim().is_empty() {
                    evidence.reads.push(read);
                }
            }
        }
    }

    evidence
}

pub(crate) fn search_hit_specificity(hit_count: usize) -> f32 {
    match hit_count {
        0 => 0.0,
        1 => 1.0,
        2..=3 => 0.85,
        4..=10 => 0.6,
        _ => 0.35,
    }
}

pub(crate) fn path_filename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

pub(crate) fn evidence_path_matches(target: &str, candidate: &str) -> bool {
    let target_sig = normalize_memory_entry(target);
    let candidate_sig = normalize_memory_entry(candidate);
    if target_sig.is_empty() || candidate_sig.is_empty() {
        return false;
    }
    target_sig == candidate_sig
        || target_sig.ends_with(candidate_sig.as_str())
        || candidate_sig.ends_with(target_sig.as_str())
}

pub(crate) fn observation_supports_target_path(
    target: &str,
    evidence: &ObservationEvidence,
) -> bool {
    evidence
        .reads
        .iter()
        .any(|read| evidence_path_matches(target, read.path.as_str()))
        || evidence.searches.iter().any(|search| {
            search
                .paths
                .iter()
                .any(|path| evidence_path_matches(target, path.as_str()))
        })
}

pub(crate) fn mutation_tool_requires_evidence(tc: &ToolCallData) -> bool {
    matches!(tc.name.as_str(), "patch_file" | "apply_diff")
}

pub(crate) fn extract_loose_json_string_field(raw: &str, field: &str) -> Option<String> {
    let needle = format!("\"{field}\"");
    let start = raw.find(needle.as_str())?;
    let after_key = &raw[start + needle.len()..];
    let colon = after_key.find(':')?;
    let mut tail = after_key[colon + 1..].trim_start();
    if !tail.starts_with('"') {
        return None;
    }
    tail = &tail[1..];

    let mut out = String::new();
    let mut chars = tail.chars();
    let mut escaped = false;
    while let Some(ch) = chars.next() {
        if escaped {
            match ch {
                '"' => out.push('"'),
                '\\' => out.push('\\'),
                '/' => out.push('/'),
                'n' => out.push('\n'),
                'r' => out.push('\r'),
                't' => out.push('\t'),
                'b' => out.push('\u{0008}'),
                'f' => out.push('\u{000C}'),
                'u' => {
                    let mut hex = String::new();
                    for _ in 0..4 {
                        let Some(h) = chars.next() else {
                            return Some(out);
                        };
                        hex.push(h);
                    }
                    if let Ok(value) = u16::from_str_radix(hex.as_str(), 16) {
                        if let Some(decoded) = char::from_u32(value as u32) {
                            out.push(decoded);
                        }
                    }
                }
                other => out.push(other),
            }
            escaped = false;
            continue;
        }
        match ch {
            '\\' => escaped = true,
            '"' => return Some(out),
            other => out.push(other),
        }
    }

    Some(out)
}

pub(crate) fn mutation_target_path(tc: &ToolCallData) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(&tc.arguments)
        .ok()
        .and_then(|args| {
            args.get("path")
                .and_then(|v| v.as_str())
                .map(str::to_string)
        })
        .or_else(|| extract_loose_json_string_field(tc.arguments.as_str(), "path"))
        .map(|path| compact_one_line(path.trim(), 180))
        .filter(|path| path != "-")
}

pub(crate) fn validate_evidence_block(
    evidence_block: &EvidenceBlock,
    tc: &ToolCallData,
    observations: &ObservationEvidence,
) -> Result<()> {
    if evidence_block.target_files.is_empty() {
        return Err(anyhow!(
            governor_contract::evidence_missing_target_files_message()
        ));
    }
    if evidence_block.evidence.trim().is_empty() {
        return Err(anyhow!(
            governor_contract::evidence_missing_evidence_message()
        ));
    }
    if evidence_block.open_questions.trim().is_empty() {
        return Err(anyhow!(
            governor_contract::evidence_missing_open_questions_message()
        ));
    }
    if evidence_block.next_probe.trim().is_empty() {
        return Err(anyhow!(
            governor_contract::evidence_missing_next_probe_message()
        ));
    }
    if !mutation_tool_requires_evidence(tc) {
        return Ok(());
    }
    let Some(target_path) = mutation_target_path(tc) else {
        return Err(anyhow!(
            governor_contract::evidence_unresolved_path_message()
        ));
    };
    if !evidence_block
        .target_files
        .iter()
        .any(|path| evidence_path_matches(path.as_str(), target_path.as_str()))
    {
        return Err(anyhow!(
            governor_contract::evidence_target_mismatch_message(target_path.as_str())
        ));
    }
    if !observation_supports_target_path(target_path.as_str(), observations) {
        return Err(anyhow!(
            governor_contract::evidence_missing_observation_message(target_path.as_str())
        ));
    }
    Ok(())
}

pub(crate) fn build_evidence_gate_prompt(
    tc: &ToolCallData,
    observations: &ObservationEvidence,
    ledger: &AssumptionLedger,
) -> String {
    let target_path = mutation_target_path(tc).unwrap_or_else(|| "<path>".to_string());
    let mut out = format!(
        "[Evidence Gate]\n\
You are about to mutate an existing file via {}.\n\
Target path: {target_path}\n\
\n\
Before this mutation, emit exactly:\n\
<evidence>\n\
target_files: 1) <exact target path>\n\
target_symbols: 1) <symbol or area>\n\
evidence: <what previous read/search proved>\n\
open_questions: <what is still uncertain or `none`>\n\
next_probe: <exact next action or edit target>\n\
</evidence>\n\
\n\
Rules:\n\
- `target_files` must include the actual file you are about to change.\n\
- Base the block on real prior read/search evidence from this session.\n\
- If the target is not yet supported by evidence, do NOT mutate; call one diagnostic tool instead.\n\
- After the <evidence> block, call exactly one tool.",
        tc.name
    );

    if !observations.reads.is_empty() {
        out.push_str("\n\n[Observed reads]\n");
        for read in observations.reads.iter().rev().take(3).rev() {
            out.push_str(&format!("- {} -> {}\n", read.command, read.path));
        }
    }
    if !observations.searches.is_empty() {
        out.push_str("[Observed searches]\n");
        for search in observations.searches.iter().rev().take(3).rev() {
            let path_summary = if search.paths.is_empty() {
                "(no paths)".to_string()
            } else {
                search
                    .paths
                    .iter()
                    .take(3)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            out.push_str(&format!(
                "- {} -> hits={} paths={}\n",
                search.command, search.hit_count, path_summary
            ));
        }
    }
    if ledger.has_refuted() {
        out.push_str("[Refuted assumptions]\n");
        for entry in ledger
            .entries
            .iter()
            .filter(|entry| entry.status == AssumptionStatus::Refuted)
            .take(2)
        {
            out.push_str(&format!("- {}\n", entry.text));
        }
    }
    out
}

pub(crate) fn first_slash_literal(text: &str) -> Option<String> {
    text.split_whitespace().find_map(|token| {
        let trimmed = token
            .trim_matches(|ch: char| {
                matches!(ch, '`' | '"' | '\'' | ',' | '.' | ':' | ';' | ')' | '(')
            })
            .trim();
        if !trimmed.starts_with('/') || trimmed.len() < 2 {
            return None;
        }
        let tail = &trimmed[1..];
        if tail
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '/'))
        {
            Some(trimmed.to_string())
        } else {
            None
        }
    })
}

pub(crate) fn parse_leading_ordinal(reference: &str) -> Option<usize> {
    let trimmed = reference.trim_start();
    let digits_len = trimmed.chars().take_while(|ch| ch.is_ascii_digit()).count();
    if digits_len == 0 {
        return None;
    }
    let digits = &trimmed[..digits_len];
    let rest = trimmed[digits_len..].trim_start();
    let Some(number) = digits.parse::<usize>().ok() else {
        return None;
    };
    if rest.is_empty()
        || rest.starts_with(')')
        || rest.starts_with('.')
        || rest.starts_with(':')
        || rest.starts_with('-')
    {
        Some(number)
    } else {
        None
    }
}

pub(crate) fn resolve_acceptance_reference(reference: &str, plan: &PlanBlock) -> Option<usize> {
    let reference_sig = normalize_memory_entry(reference);
    if reference_sig.is_empty() {
        return None;
    }

    if let Some(n) = parse_leading_ordinal(reference) {
        if n <= plan.acceptance_criteria.len() {
            return Some(n - 1);
        }
    }

    if reference_sig.contains("acceptance")
        || reference_sig.contains("criterion")
        || reference_sig.contains("criteria")
    {
        if let Some(n) = parse_first_usize(reference) {
            if n <= plan.acceptance_criteria.len() {
                return Some(n - 1);
            }
        }
    }

    plan.acceptance_criteria
        .iter()
        .position(|criterion| impact_progress_matches_entry(reference, criterion))
}

pub(crate) fn acceptance_reference_label(plan: &PlanBlock, idx: usize) -> String {
    let criterion = plan
        .acceptance_criteria
        .get(idx)
        .map(|s| s.as_str())
        .unwrap_or("-");
    format!("acceptance {}: {}", idx + 1, criterion)
}

pub(crate) fn collect_successful_observation_commands(
    messages: &[serde_json::Value],
) -> Vec<String> {
    let mut pending: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut commands = Vec::new();

    for msg in messages {
        let role = msg
            .get("role")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        if role == "assistant" {
            let Some(tool_calls) = msg.get("tool_calls").and_then(|v| v.as_array()) else {
                continue;
            };
            for tc in tool_calls {
                let id = tc.get("id").and_then(|v| v.as_str()).unwrap_or("").trim();
                if id.is_empty() {
                    continue;
                }
                let name = tc
                    .get("function")
                    .and_then(|f| f.get("name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim();
                if !matches!(name, "read_file" | "list_dir" | "glob" | "search_files") {
                    continue;
                }
                let arguments = tc
                    .get("function")
                    .and_then(|f| f.get("arguments"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim();
                let Some(signature) = canonicalize_tool_call_command(name, arguments) else {
                    continue;
                };
                pending.insert(id.to_string(), signature);
            }
            continue;
        }

        if role != "tool" {
            continue;
        }

        let tool_call_id = msg
            .get("tool_call_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        if tool_call_id.is_empty() {
            continue;
        }
        let Some(signature) = pending.remove(tool_call_id) else {
            continue;
        };
        let content = msg.get("content").and_then(|v| v.as_str()).unwrap_or("");
        if non_exec_tool_succeeded(content) {
            remember_recent_unique(&mut commands, signature.as_str(), 12, 200);
        }
    }

    commands
}

pub(crate) fn collect_successful_mutation_commands(messages: &[serde_json::Value]) -> Vec<String> {
    let mut pending: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut commands = Vec::new();

    for msg in messages {
        let role = msg
            .get("role")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        if role == "assistant" {
            let Some(tool_calls) = msg.get("tool_calls").and_then(|v| v.as_array()) else {
                continue;
            };
            for tc in tool_calls {
                let id = tc.get("id").and_then(|v| v.as_str()).unwrap_or("").trim();
                if id.is_empty() {
                    continue;
                }
                let name = tc
                    .get("function")
                    .and_then(|f| f.get("name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim();
                if !matches!(name, "write_file" | "patch_file" | "apply_diff") {
                    continue;
                }
                let arguments = tc
                    .get("function")
                    .and_then(|f| f.get("arguments"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim();
                let Some(signature) = canonicalize_tool_call_command(name, arguments) else {
                    continue;
                };
                pending.insert(id.to_string(), signature);
            }
            continue;
        }

        if role != "tool" {
            continue;
        }

        let tool_call_id = msg
            .get("tool_call_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        if tool_call_id.is_empty() {
            continue;
        }
        let Some(signature) = pending.remove(tool_call_id) else {
            continue;
        };
        let content = msg.get("content").and_then(|v| v.as_str()).unwrap_or("");
        if non_exec_tool_succeeded(content) {
            remember_recent_unique(&mut commands, signature.as_str(), 12, 200);
        }
    }

    commands
}

pub(crate) fn collect_successful_auto_test_verification_commands(
    messages: &[serde_json::Value],
    test_cmd: Option<&str>,
) -> Vec<String> {
    let Some(test_cmd) = test_cmd.map(str::trim).filter(|cmd| !cmd.is_empty()) else {
        return Vec::new();
    };
    let mut pending_ids: std::collections::BTreeMap<String, String> =
        std::collections::BTreeMap::new();
    let mut commands = Vec::new();

    for msg in messages {
        let role = msg
            .get("role")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        if role == "assistant" {
            let Some(tool_calls) = msg.get("tool_calls").and_then(|v| v.as_array()) else {
                continue;
            };
            for tc in tool_calls {
                let id = tc.get("id").and_then(|v| v.as_str()).unwrap_or("").trim();
                if id.is_empty() {
                    continue;
                }
                let name = tc
                    .get("function")
                    .and_then(|f| f.get("name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim();
                if matches!(name, "write_file" | "patch_file" | "apply_diff") {
                    pending_ids.insert(id.to_string(), name.to_string());
                }
            }
            continue;
        }

        if role != "tool" {
            continue;
        }

        let tool_call_id = msg
            .get("tool_call_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        let Some(name) = pending_ids.remove(tool_call_id) else {
            continue;
        };
        let content = msg.get("content").and_then(|v| v.as_str()).unwrap_or("");
        if crate::execution_evidence::auto_test_succeeded(&name, content) {
            remember_recent_unique(&mut commands, test_cmd, 8, 1024);
        }
    }

    commands
}

pub(crate) fn collect_known_acceptance_commands(
    messages: &[serde_json::Value],
    working_mem: &WorkingMemory,
    test_cmd: Option<&str>,
) -> Vec<String> {
    let mut commands = Vec::new();
    for command in &working_mem.successful_verifications {
        remember_recent_unique(&mut commands, command.as_str(), 16, 1024);
    }
    for command in collect_successful_auto_test_verification_commands(messages, test_cmd) {
        remember_recent_unique(&mut commands, command.as_str(), 16, 1024);
    }
    for command in collect_successful_mutation_commands(messages) {
        remember_recent_unique(&mut commands, command.as_str(), 16, 1024);
    }
    for command in collect_successful_observation_commands(messages) {
        remember_recent_unique(&mut commands, command.as_str(), 16, 1024);
    }
    commands
}

pub(crate) fn build_impact_prompt(
    reason: &str,
    plan: Option<&PlanBlock>,
    working_mem: &WorkingMemory,
) -> String {
    let goal = plan
        .map(|p| compact_one_line(p.goal.as_str(), 100))
        .unwrap_or_else(|| "-".to_string());

    let mut out = format!(
        "[Impact Check Required]\n\
Reason: {reason}\n\
Current goal: {goal}\n\
\n\
Before your next tool call, emit exactly:\n\
<impact>\n\
changed: <one short sentence>\n\
progress: <which plan step or acceptance criterion moved>\n\
remaining_gap: <one short sentence>\n\
</impact>\n\
\n\
Rules:\n\
- Keep the whole block under 60 tokens.\n\
- Mention the actual mutation effect, not intent.\n\
- `progress` must name a real step or acceptance criterion.\n\
- After the <impact> block, call exactly one tool."
    );

    if !working_mem.completed_steps.is_empty() {
        out.push_str("\n\n[Already Completed]\n");
        for step in working_mem.completed_steps.iter().rev().take(2).rev() {
            out.push_str(&format!("- {step}\n"));
        }
    }

    if let Some(plan) = plan {
        if !plan.steps.is_empty() {
            out.push_str("\n[Plan Steps]\n");
            for (idx, step) in plan.steps.iter().enumerate() {
                out.push_str(&format!("- step {}: {}\n", idx + 1, step));
            }
        }
        if !plan.acceptance_criteria.is_empty() {
            out.push_str("[Acceptance Criteria]\n");
            for (idx, criterion) in plan.acceptance_criteria.iter().enumerate() {
                out.push_str(&format!("- acceptance {}: {}\n", idx + 1, criterion));
            }
        }
    }

    out
}
