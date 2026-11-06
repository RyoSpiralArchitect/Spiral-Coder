use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RepoMapHintStrength {
    Strong,
    Medium,
    Weak,
}

pub(crate) fn repo_map_hint_strength(confidence: f64) -> RepoMapHintStrength {
    if confidence >= 0.85 {
        RepoMapHintStrength::Strong
    } else if confidence >= 0.60 {
        RepoMapHintStrength::Medium
    } else {
        RepoMapHintStrength::Weak
    }
}

pub(crate) fn repo_map_action_verb(confidence: f64) -> &'static str {
    match repo_map_hint_strength(confidence) {
        RepoMapHintStrength::Strong => "Prefer",
        RepoMapHintStrength::Medium => "Start with",
        RepoMapHintStrength::Weak => "Consider",
    }
}

pub(crate) fn repo_map_typo_intro(confidence: f64) -> &'static str {
    match repo_map_hint_strength(confidence) {
        RepoMapHintStrength::Strong => "repo_map strong path-typo.",
        RepoMapHintStrength::Medium => "repo_map medium path-typo.",
        RepoMapHintStrength::Weak => "repo_map weak path-typo.",
    }
}

pub(crate) fn repo_map_fallback_intro(confidence: f64) -> &'static str {
    match repo_map_hint_strength(confidence) {
        RepoMapHintStrength::Strong => "repo_map strong candidate.",
        RepoMapHintStrength::Medium => "repo_map medium candidate.",
        RepoMapHintStrength::Weak => "repo_map weak candidate.",
    }
}

pub(crate) fn repo_map_confidence_guidance(confidence: f64) -> &'static str {
    match repo_map_hint_strength(confidence) {
        RepoMapHintStrength::Strong => {
            "Treat the top candidate as the default next step unless your broader plan clearly points elsewhere."
        }
        RepoMapHintStrength::Medium => {
            "Start there, but keep the rest of the fallback list in view before committing."
        }
        RepoMapHintStrength::Weak => {
            "Treat the fallback list as suggestions only and compare multiple candidates before retrying."
        }
    }
}

pub(crate) fn repo_map_reason_suffix(reasons: &[String]) -> String {
    if reasons.is_empty() {
        String::new()
    } else {
        format!(" [{}]", reasons.join(", "))
    }
}

pub(crate) fn repo_map_display_level(confidence: f64) -> &'static str {
    match repo_map_hint_strength(confidence) {
        RepoMapHintStrength::Strong => "strong",
        RepoMapHintStrength::Medium => "medium",
        RepoMapHintStrength::Weak => "weak",
    }
}

pub(crate) fn repo_map_display_next_step(fallback: &crate::repo_map::RepoMapFallback) -> String {
    match (fallback.top_dir.as_deref(), fallback.top_path.as_deref()) {
        (Some(candidate_dir), Some(candidate_path)) => {
            format!(
                "list_dir '{}' -> read_file '{}'",
                candidate_dir, candidate_path
            )
        }
        (Some(candidate_dir), None) => format!("list_dir '{}'", candidate_dir),
        (None, Some(candidate_path)) => format!("read_file '{}'", candidate_path),
        (None, None) => "inspect fallback list".to_string(),
    }
}

pub(crate) fn build_repo_map_display_banner(
    tool_name: &str,
    fallback: &crate::repo_map::RepoMapFallback,
) -> String {
    let confidence = fallback.top_confidence.unwrap_or(0.0);
    let level = repo_map_display_level(confidence);
    let flavor = if fallback.typo_likely {
        "path-typo"
    } else {
        "candidate"
    };
    let next = repo_map_display_next_step(fallback);
    format!("[repo_map:{tool_name}] {level} {flavor} -> {next}\n")
}

pub(crate) fn build_repo_map_search_hint(
    pattern: &str,
    fallback: &crate::repo_map::RepoMapFallback,
) -> String {
    let confidence = fallback.top_confidence.unwrap_or(0.0);
    let action = repo_map_action_verb(confidence);
    let guidance = repo_map_confidence_guidance(confidence);
    match fallback.top_path.as_deref() {
        Some(path) => format!(
            "Literal search found no matches for '{}'. {}\n\
{} read_file '{}' next (top_confidence={:.2}).\n\
{}",
            pattern,
            repo_map_fallback_intro(confidence),
            action,
            path,
            confidence,
            guidance
        ),
        None => format!(
            "Literal search found no matches for '{}'. Use the repo_map fallback candidates from the tool output before retrying.\n\
{}",
            pattern, guidance
        ),
    }
}

pub(crate) fn build_repo_map_read_hint(
    path: &str,
    fallback: &crate::repo_map::RepoMapFallback,
) -> String {
    let confidence = fallback.top_confidence.unwrap_or(0.0);
    let action = repo_map_action_verb(confidence);
    let reason_suffix = repo_map_reason_suffix(&fallback.top_path_reasons);
    let guidance = repo_map_confidence_guidance(confidence);
    match fallback.top_path.as_deref() {
        Some(candidate) if fallback.typo_likely => format!(
            "read_file failed for '{}'. {}\n\
{} read_file '{}' next (top_confidence={:.2}){}.\n\
{}",
            path,
            repo_map_typo_intro(confidence),
            action,
            candidate,
            confidence,
            reason_suffix,
            guidance
        ),
        Some(candidate) => format!(
            "read_file failed for '{}'. {}\n\
{} read_file '{}' next (top_confidence={:.2}).\n\
{}",
            path,
            repo_map_fallback_intro(confidence),
            action,
            candidate,
            confidence,
            guidance
        ),
        None => format!(
            "read_file failed for '{}'. Use the repo_map fallback candidates from the tool output before retrying.\n\
{}",
            path, guidance
        ),
    }
}

pub(crate) fn build_repo_map_list_dir_hint(
    dir: &str,
    fallback: &crate::repo_map::RepoMapFallback,
) -> String {
    let confidence = fallback.top_confidence.unwrap_or(0.0);
    let action = repo_map_action_verb(confidence);
    let reason_suffix = repo_map_reason_suffix(&fallback.top_path_reasons);
    let guidance = repo_map_confidence_guidance(confidence);
    match (fallback.top_dir.as_deref(), fallback.top_path.as_deref(), fallback.typo_likely) {
        (Some(candidate_dir), Some(candidate_path), true) => format!(
            "list_dir failed for '{}'. {}\n\
{} list_dir '{}' next; if you were actually targeting a file, read_file '{}' after that (top_confidence={:.2}){}.\n\
{}",
            dir,
            repo_map_typo_intro(confidence),
            action,
            candidate_dir,
            candidate_path,
            confidence,
            reason_suffix,
            guidance
        ),
        (Some(candidate_dir), None, true) => format!(
            "list_dir failed for '{}'. {}\n\
{} list_dir '{}' next (top_confidence={:.2}){}.\n\
{}",
            dir,
            repo_map_typo_intro(confidence),
            action,
            candidate_dir,
            confidence,
            reason_suffix,
            guidance
        ),
        (None, Some(candidate_path), true) => format!(
            "list_dir failed for '{}'. {}\n\
{} read_file '{}' next (top_confidence={:.2}){}.\n\
{}",
            dir,
            repo_map_typo_intro(confidence),
            action,
            candidate_path,
            confidence,
            reason_suffix,
            guidance
        ),
        (Some(candidate_dir), Some(candidate_path), false) => format!(
            "list_dir failed for '{}'. {}\n\
{} list_dir '{}' first; if that still misses your target, read_file '{}' next (top_confidence={:.2}).\n\
{}",
            dir,
            repo_map_fallback_intro(confidence),
            action,
            candidate_dir,
            candidate_path,
            confidence,
            guidance
        ),
        (Some(candidate_dir), None, false) => format!(
            "list_dir failed for '{}'. {}\n\
{} list_dir '{}' first (top_confidence={:.2}).\n\
{}",
            dir,
            repo_map_fallback_intro(confidence),
            action,
            candidate_dir,
            confidence,
            guidance
        ),
        (None, Some(candidate_path), false) => format!(
            "list_dir failed for '{}'. {}\n\
{} read_file '{}' first (top_confidence={:.2}).\n\
{}",
            dir,
            repo_map_fallback_intro(confidence),
            action,
            candidate_path,
            confidence,
            guidance
        ),
        (None, None, _) => format!(
            "list_dir failed for '{}'. Use the repo_map fallback candidates from the tool output before retrying.\n\
{}",
            dir, guidance
        ),
    }
}

pub(crate) fn build_repo_map_glob_hint(
    pattern: &str,
    fallback: &crate::repo_map::RepoMapFallback,
) -> String {
    let confidence = fallback.top_confidence.unwrap_or(0.0);
    let action = repo_map_action_verb(confidence);
    let reason_suffix = repo_map_reason_suffix(&fallback.top_path_reasons);
    let guidance = repo_map_confidence_guidance(confidence);
    match (fallback.top_dir.as_deref(), fallback.top_path.as_deref(), fallback.typo_likely) {
        (Some(candidate_dir), Some(candidate_path), true) => format!(
            "Literal glob found no matches for '{}'. {}\n\
{} inspect '{}' first; if needed, read_file '{}' after that (top_confidence={:.2}){}.\n\
{}",
            pattern,
            repo_map_typo_intro(confidence),
            action,
            candidate_dir,
            candidate_path,
            confidence,
            reason_suffix,
            guidance
        ),
        (Some(candidate_dir), None, true) => format!(
            "Literal glob found no matches for '{}'. {}\n\
{} inspect '{}' first (top_confidence={:.2}){}.\n\
{}",
            pattern,
            repo_map_typo_intro(confidence),
            action,
            candidate_dir,
            confidence,
            reason_suffix,
            guidance
        ),
        (None, Some(candidate_path), true) => format!(
            "Literal glob found no matches for '{}'. {}\n\
{} read_file '{}' next (top_confidence={:.2}){}.\n\
{}",
            pattern,
            repo_map_typo_intro(confidence),
            action,
            candidate_path,
            confidence,
            reason_suffix,
            guidance
        ),
        (Some(candidate_dir), Some(candidate_path), false) => format!(
            "Literal glob found no matches for '{}'. {}\n\
{} list_dir '{}' first; if needed, read_file '{}' next (top_confidence={:.2}).\n\
{}",
            pattern,
            repo_map_fallback_intro(confidence),
            action,
            candidate_dir,
            candidate_path,
            confidence,
            guidance
        ),
        (Some(candidate_dir), None, false) => format!(
            "Literal glob found no matches for '{}'. {}\n\
{} list_dir '{}' first (top_confidence={:.2}).\n\
{}",
            pattern,
            repo_map_fallback_intro(confidence),
            action,
            candidate_dir,
            confidence,
            guidance
        ),
        (None, Some(candidate_path), false) => format!(
            "Literal glob found no matches for '{}'. {}\n\
{} read_file '{}' first (top_confidence={:.2}).\n\
{}",
            pattern,
            repo_map_fallback_intro(confidence),
            action,
            candidate_path,
            confidence,
            guidance
        ),
        (None, None, _) => format!(
            "Literal glob found no matches for '{}'. Use the repo_map fallback candidates from the tool output before retrying.\n\
{}",
            pattern, guidance
        ),
    }
}

pub(crate) fn rewrite_tool_call_to_search_files(
    tc: &ToolCallData,
    pattern: &str,
    dir: &str,
) -> Option<(ToolCallData, String, String)> {
    let rewritten = ToolCallData {
        thought_signature: None,
        id: tc.id.clone(),
        name: "search_files".to_string(),
        arguments: json!({
            "pattern": pattern,
            "dir": dir,
        })
        .to_string(),
    };
    let original = canonicalize_tool_call_command(tc.name.as_str(), tc.arguments.as_str())
        .unwrap_or_else(|| blocked_tool_call_signature(tc.name.as_str(), tc.arguments.as_str()));
    let coerced =
        canonicalize_tool_call_command(rewritten.name.as_str(), rewritten.arguments.as_str())
            .unwrap_or_else(|| {
                format!(
                    "search_files(pattern={}, dir={})",
                    compact_one_line(pattern, 120),
                    compact_one_line(dir, 160)
                )
            });
    if original == coerced {
        None
    } else {
        Some((rewritten, original, coerced))
    }
}
