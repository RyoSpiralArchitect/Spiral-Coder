use super::*;

#[derive(Debug, Clone)]
pub(super) struct CriterionEvidenceScore {
    pub(super) idx: usize,
    pub(super) total: f32,
    pub(super) search_specificity: f32,
    pub(super) read_confirm: f32,
    pub(super) repo_prior: f32,
    pub(super) best_path: Option<String>,
    pub(super) suggested_commands: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RepoScaffoldArtifacts {
    repo_root: String,
    logic_path: Option<String>,
    entrypoint_path: Option<String>,
}

fn criterion_prefers_read_confirmation(criterion: &str) -> bool {
    let low = criterion.to_ascii_lowercase();
    [
        "read",
        "verify",
        "confirmed",
        "confirm",
        "context",
        "handler",
        "logic",
        "branch",
    ]
    .iter()
    .any(|term| low.contains(term))
}

fn single_observed_read_target(evidence: &ObservationEvidence) -> Option<String> {
    let observed_paths = evidence
        .searches
        .iter()
        .flat_map(|search| search.paths.iter())
        .map(|path| normalize_path_alias(path))
        .filter(|path| !path.is_empty())
        .collect::<std::collections::BTreeSet<_>>();
    if observed_paths.len() != 1 {
        return None;
    }
    let target_sig = observed_paths.into_iter().next()?;
    evidence
        .reads
        .iter()
        .rev()
        .find(|read| normalize_path_alias(read.path.as_str()) == target_sig)
        .map(|read| read.path.clone())
}

pub(super) fn build_read_only_strong_final_answer(
    root_user_text: &str,
    plan: &PlanBlock,
    evidence: &ObservationEvidence,
    messages: &[serde_json::Value],
    working_mem: &WorkingMemory,
) -> Option<String> {
    let scores = build_read_only_evidence_scores(root_user_text, plan, evidence);
    let strong_read_count = scores
        .iter()
        .filter(|score| score.total >= 0.80 && score.read_confirm >= 0.80)
        .count();
    if strong_read_count < 2 {
        return None;
    }
    build_read_only_iteration_cap_final_answer(
        root_user_text,
        plan,
        evidence,
        messages,
        working_mem,
    )
}

pub(super) fn build_read_only_evidence_scores(
    root_user_text: &str,
    plan: &PlanBlock,
    evidence: &ObservationEvidence,
) -> Vec<CriterionEvidenceScore> {
    let mut path_votes: std::collections::BTreeMap<String, usize> =
        std::collections::BTreeMap::new();
    for search in &evidence.searches {
        for path in &search.paths {
            *path_votes.entry(path.clone()).or_insert(0) += 1;
        }
    }
    for read in &evidence.reads {
        *path_votes.entry(read.path.clone()).or_insert(0) += 2;
    }
    let global_best_path = path_votes
        .into_iter()
        .max_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)))
        .map(|(path, _)| path);

    plan.acceptance_criteria
        .iter()
        .enumerate()
        .map(|(idx, criterion)| {
            let criterion_tokens = keyword_tokens(criterion);

            let best_search = evidence
                .searches
                .iter()
                .map(|search| {
                    let pattern_tokens = keyword_tokens(&search.pattern);
                    let mut relevance = token_overlap_score(&criterion_tokens, &pattern_tokens);
                    let path_relevance = search
                        .paths
                        .iter()
                        .map(|path| path_prior_score(path, root_user_text, &plan.goal, criterion))
                        .fold(0.0f32, f32::max);
                    if relevance == 0.0 && search.hit_count > 0 {
                        relevance = 0.45;
                    }
                    let specificity = search_hit_specificity(search.hit_count)
                        * (0.5 + 0.5 * relevance.max(path_relevance));
                    (specificity.clamp(0.0, 1.0), search)
                })
                .max_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

            let mut suggested_commands = Vec::new();
            let (search_specificity, best_search_command) =
                if let Some((score, search)) = best_search {
                    (score, Some(search.command.clone()))
                } else {
                    (0.0, None)
                };

            let best_path = evidence
                .reads
                .iter()
                .map(|read| read.path.clone())
                .find(|path| {
                    global_best_path
                        .as_ref()
                        .map(|best| best == path)
                        .unwrap_or(false)
                })
                .or_else(|| global_best_path.clone())
                .or_else(|| evidence.reads.first().map(|read| read.path.clone()));

            let (read_confirm, best_read_command) = evidence
                .reads
                .iter()
                .map(|read| {
                    let path_score =
                        path_prior_score(&read.path, root_user_text, &plan.goal, criterion);
                    let mut score = if criterion.to_ascii_lowercase().contains("read")
                        || criterion.to_ascii_lowercase().contains("verify")
                        || criterion.to_ascii_lowercase().contains("context")
                        || criterion.to_ascii_lowercase().contains("handler")
                    {
                        0.75 + 0.25 * path_score
                    } else {
                        0.55 + 0.45 * path_score
                    };
                    if global_best_path.as_deref() == Some(read.path.as_str()) {
                        score = (score + 0.15).clamp(0.0, 1.0);
                    }
                    (score.clamp(0.0, 1.0), read)
                })
                .max_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal))
                .map(|(score, read)| (score, Some(read.command.clone())))
                .unwrap_or((0.0, None));

            let repo_prior = best_path
                .as_deref()
                .map(|path| path_prior_score(path, root_user_text, &plan.goal, criterion))
                .unwrap_or(0.0);

            let prefer_read = best_read_command.is_some()
                && (criterion_prefers_read_confirmation(criterion)
                    || read_confirm + 0.05 >= search_specificity);

            if prefer_read {
                if let Some(command) = best_read_command.as_deref() {
                    remember_recent_unique(&mut suggested_commands, command, 3, 200);
                }
                if let Some(command) = best_search_command.as_deref() {
                    remember_recent_unique(&mut suggested_commands, command, 3, 200);
                }
            } else {
                if let Some(command) = best_search_command.as_deref() {
                    remember_recent_unique(&mut suggested_commands, command, 3, 200);
                }
                if let Some(command) = best_read_command.as_deref() {
                    remember_recent_unique(&mut suggested_commands, command, 3, 200);
                }
            }

            let total = (search_specificity * 0.30 + read_confirm * 0.50 + repo_prior * 0.20)
                .clamp(0.0, 1.0);

            CriterionEvidenceScore {
                idx,
                total,
                search_specificity,
                read_confirm,
                repo_prior,
                best_path,
                suggested_commands,
            }
        })
        .collect()
}

pub(super) fn build_read_only_completion_hint(
    root_user_text: &str,
    plan: &PlanBlock,
    evidence: &ObservationEvidence,
    messages: &[serde_json::Value],
    working_mem: &WorkingMemory,
) -> Option<String> {
    if evidence.reads.is_empty() {
        return None;
    }

    let scores = build_read_only_evidence_scores(root_user_text, plan, evidence);
    let single_target = single_observed_read_target(evidence);
    let medium_threshold = if single_target.is_some() { 0.55 } else { 0.60 };
    let strong: Vec<&CriterionEvidenceScore> =
        scores.iter().filter(|score| score.total >= 0.85).collect();
    let medium_or_better = scores
        .iter()
        .filter(|score| score.total >= medium_threshold)
        .count();
    let completed_scores: Vec<&CriterionEvidenceScore> =
        if !strong.is_empty() && medium_or_better >= 2 {
            strong
        } else if single_target.is_some() && medium_or_better >= 2 {
            scores
                .iter()
                .filter(|score| score.total >= medium_threshold)
                .collect()
        } else if single_target.is_some() {
            scores.iter().collect()
        } else {
            return None;
        };

    if completed_scores.is_empty() {
        return None;
    }

    let known_commands = canonicalize_known_acceptance_commands(
        &collect_known_acceptance_commands(messages, working_mem, None),
        evidence,
    );
    let cite_commands: Vec<String> = completed_scores
        .iter()
        .filter_map(|score| preferred_done_command_for_score(score, &known_commands, evidence))
        .chain(
            completed_scores
                .iter()
                .flat_map(|score| score.suggested_commands.iter().cloned()),
        )
        .chain(known_commands.iter().rev().cloned())
        .fold(Vec::<String>::new(), |mut acc, command| {
            let canonical =
                canonicalize_evidence_command_with_resolution(command.as_str(), evidence);
            let chosen = if canonical.is_empty() {
                compact_one_line(command.as_str(), 200)
            } else {
                canonical
            };
            remember_recent_unique(&mut acc, chosen.as_str(), 4, 200);
            acc
        });

    let completed_lines = completed_scores
        .iter()
        .take(2)
        .map(|score| {
            format!(
                "- acceptance {}: {}",
                score.idx + 1,
                compact_one_line(plan.acceptance_criteria[score.idx].as_str(), 160)
            )
        })
        .collect::<Vec<_>>();

    let mut out = String::from(
        "[Read-Only Completion]\n\
This is a read-only inspection task. Do NOT run exec/build/test/smoke checks.\n\
You already have enough observation evidence to call done directly now.\n",
    );
    if !completed_lines.is_empty() {
        out.push_str("Completed candidates now:\n");
        out.push_str(&completed_lines.join("\n"));
        out.push('\n');
    }
    if !cite_commands.is_empty() {
        out.push_str("Cite successful commands:\n");
        for command in cite_commands.iter().take(3) {
            out.push_str("- ");
            out.push_str(command);
            out.push('\n');
        }
    }
    out.push_str(
        "If your plan includes meta constraints like `no files modified`, keep them in remaining_acceptance instead of blocking done.\n\
Do NOT call another observation tool if the file path and handler context are already confirmed.\n\
Next assistant turn: emit a <think> block with `tool: done`, then call `done` immediately.\n\
If you cite handler confirmation, prefer the successful `read_file(...)` command over another search.\n\
Final answer must include the file path.",
    );
    Some(out)
}

pub(super) fn build_read_only_iteration_cap_final_answer(
    root_user_text: &str,
    plan: &PlanBlock,
    evidence: &ObservationEvidence,
    messages: &[serde_json::Value],
    working_mem: &WorkingMemory,
) -> Option<String> {
    if evidence.reads.is_empty() {
        return None;
    }

    let scores = build_read_only_evidence_scores(root_user_text, plan, evidence);
    let single_target = single_observed_read_target(evidence);
    let medium_threshold = if single_target.is_some() { 0.55 } else { 0.60 };
    let completion_threshold = if single_target.is_some() { 0.55 } else { 0.70 };
    let medium_or_better = scores
        .iter()
        .filter(|score| score.total >= medium_threshold)
        .count();
    if medium_or_better < 2 && single_target.is_none() {
        return None;
    }

    let known_commands = canonicalize_known_acceptance_commands(
        &collect_known_acceptance_commands(messages, working_mem, None),
        evidence,
    );
    let mut completed_rows: Vec<(usize, String)> = scores
        .iter()
        .filter(|score| single_target.is_some() || score.total >= completion_threshold)
        .filter_map(|score| {
            let command = preferred_done_command_for_score(score, &known_commands, evidence)?;
            Some((score.idx, command))
        })
        .collect();

    if completed_rows.is_empty() {
        return None;
    }

    completed_rows.sort_by_key(|(idx, _)| *idx);
    completed_rows.dedup_by_key(|(idx, _)| *idx);

    let best_path = completed_rows
        .iter()
        .find_map(|(idx, _)| scores.get(*idx).and_then(|score| score.best_path.clone()))
        .or_else(|| {
            scores
                .iter()
                .max_by(|a, b| {
                    a.total
                        .partial_cmp(&b.total)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .and_then(|score| score.best_path.clone())
        });

    let summary = if let Some(path) = best_path.as_deref() {
        if let Some(slash) = first_slash_literal(root_user_text) {
            format!("Located the `{slash}` slash command handling in `{path}`.")
        } else {
            format!("Located the requested implementation in `{path}`.")
        }
    } else {
        "Completed the requested read-only inspection.".to_string()
    };

    let completed_indices: std::collections::BTreeSet<usize> =
        completed_rows.iter().map(|(idx, _)| *idx).collect();

    let mut final_text = String::from("[DONE]\n");
    final_text.push_str(summary.as_str());
    final_text.push_str("\n\nAcceptance:\n");
    for (idx, command) in &completed_rows {
        final_text.push_str("- done: ");
        final_text.push_str(acceptance_reference_label(plan, *idx).as_str());
        final_text.push_str(" via `");
        final_text.push_str(command.as_str());
        final_text.push_str("`\n");
    }
    for idx in 0..plan.acceptance_criteria.len() {
        if completed_indices.contains(&idx) {
            continue;
        }
        final_text.push_str("- remaining: ");
        final_text.push_str(acceptance_reference_label(plan, idx).as_str());
        final_text.push('\n');
    }
    Some(final_text)
}

pub(super) fn maybe_build_read_only_auto_final_answer(
    root_read_only: bool,
    root_user_text: &str,
    plan: Option<&PlanBlock>,
    evidence: &ObservationEvidence,
    messages: &[serde_json::Value],
    working_mem: &WorkingMemory,
) -> Option<String> {
    if !root_read_only {
        return None;
    }
    let fallback_plan;
    let plan = if let Some(plan) = plan {
        plan
    } else {
        fallback_plan = synthetic_read_only_observation_plan(root_user_text);
        &fallback_plan
    };
    build_read_only_iteration_cap_final_answer(
        root_user_text,
        plan,
        evidence,
        messages,
        working_mem,
    )
}

pub(super) fn canonicalize_known_acceptance_commands(
    known_commands: &[String],
    evidence: &ObservationEvidence,
) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = Vec::new();

    for command in known_commands {
        let raw = command.trim();
        if raw.is_empty() {
            continue;
        }
        let canonical = canonicalize_evidence_command_with_resolution(command.as_str(), evidence);
        let sig = if canonical.is_empty() {
            normalize_memory_entry(raw)
        } else {
            normalize_memory_entry(canonical.as_str())
        };
        if sig.is_empty() {
            continue;
        }
        let shown = raw.to_string();
        if let Some(pos) = seen.iter().position(|existing| existing == &sig) {
            seen.remove(pos);
            out.remove(pos);
        }
        seen.push(sig);
        out.push(shown);
        if out.len() > 16 {
            let drop_n = out.len() - 16;
            out.drain(0..drop_n);
            seen.drain(0..drop_n);
        }
    }

    out
}

fn resolve_known_acceptance_command<'a>(
    command: &str,
    known_commands: &'a [String],
    evidence: &ObservationEvidence,
) -> Option<&'a str> {
    let want = canonicalize_evidence_command_with_resolution(command, evidence);
    if want.is_empty() {
        return None;
    }

    known_commands
        .iter()
        .find(|candidate| {
            let sig = canonicalize_evidence_command_with_resolution(candidate, evidence);
            if sig.is_empty() {
                return false;
            }
            if sig == want || sig.contains(&want) || want.contains(&sig) {
                return true;
            }
            let Some((want_name, want_args)) = parse_named_command_signature(&want) else {
                return false;
            };
            let Some((cand_name, cand_args)) = parse_named_command_signature(&sig) else {
                return false;
            };
            if want_name != cand_name {
                return false;
            }
            want_args
                .iter()
                .all(|(key, want_value)| match cand_args.get(key) {
                    Some(cand_value) if cand_value == want_value => true,
                    Some(cand_value) if want_name == "search_files" && key == "dir" => {
                        want_value.starts_with(&format!("{cand_value}/"))
                            || cand_value.starts_with(&format!("{want_value}/"))
                    }
                    _ => false,
                })
        })
        .map(|s| s.as_str())
}

fn is_read_file_command_for_path(
    command: &str,
    path: &str,
    evidence: &ObservationEvidence,
) -> bool {
    let sig = canonicalize_evidence_command_with_resolution(command, evidence);
    let Some((name, args)) = parse_named_command_signature(sig.as_str()) else {
        return false;
    };
    if name != "read_file" {
        return false;
    }
    let Some(candidate_path) = args.get("path") else {
        return false;
    };
    normalize_path_alias(candidate_path.as_str()) == normalize_path_alias(path)
}

fn preferred_done_command_for_score(
    score: &CriterionEvidenceScore,
    known_commands: &[String],
    evidence: &ObservationEvidence,
) -> Option<String> {
    if score.read_confirm >= 0.70 {
        if let Some(path) = score.best_path.as_deref() {
            if let Some(command) = known_commands
                .iter()
                .rev()
                .find(|candidate| is_read_file_command_for_path(candidate, path, evidence))
            {
                return Some(command.clone());
            }
        }
        if let Some(command) = score.suggested_commands.iter().find_map(|cmd| {
            let resolved =
                resolve_known_acceptance_command(cmd.as_str(), known_commands, evidence)?;
            if is_read_file_command_for_path(
                resolved,
                score.best_path.as_deref().unwrap_or(""),
                evidence,
            ) || canonicalize_evidence_command_with_resolution(resolved, evidence)
                .starts_with("read_file(")
            {
                Some(resolved.to_string())
            } else {
                None
            }
        }) {
            return Some(command);
        }
    }

    score.suggested_commands.iter().find_map(|cmd| {
        resolve_known_acceptance_command(cmd.as_str(), known_commands, evidence)
            .map(|s| s.to_string())
    })
}

fn preferred_known_command_for_evidence<'a>(
    command: &str,
    known_commands: &'a [String],
    evidence: &ObservationEvidence,
) -> Option<&'a str> {
    let want = canonicalize_evidence_command_with_resolution(command, evidence);
    if let Some((name, args)) = parse_named_command_signature(want.as_str()) {
        if name == "read_file" {
            if let Some(path) = args.get("path") {
                if let Some(candidate) = known_commands
                    .iter()
                    .rev()
                    .find(|candidate| is_read_file_command_for_path(candidate, path, evidence))
                {
                    return Some(candidate.as_str());
                }
            }
        }
    }
    resolve_known_acceptance_command(command, known_commands, evidence)
}

pub(super) fn validate_done_acceptance(
    plan: Option<&PlanBlock>,
    completed_acceptance: &[String],
    remaining_acceptance: &[String],
    acceptance_evidence: &[DoneAcceptanceEvidence],
    known_commands: &[String],
    observation_evidence: &ObservationEvidence,
) -> Result<Vec<(usize, String)>> {
    let Some(plan) = plan else {
        return Err(anyhow!(governor_contract::done_requires_plan_message()));
    };

    if completed_acceptance.is_empty() && remaining_acceptance.is_empty() {
        return Err(anyhow!(governor_contract::done_missing_criteria_message()));
    }

    let mut covered = std::collections::BTreeSet::new();
    let mut completed_indices = std::collections::BTreeSet::new();

    for entry in completed_acceptance {
        let Some(idx) = resolve_acceptance_reference(entry, plan) else {
            return Err(anyhow!(
                governor_contract::done_completed_invalid_reference_message()
            ));
        };
        if !covered.insert(idx) {
            return Err(anyhow!(governor_contract::done_duplicate_criteria_message()));
        }
        completed_indices.insert(idx);
    }

    for entry in remaining_acceptance {
        let Some(idx) = resolve_acceptance_reference(entry, plan) else {
            return Err(anyhow!(
                governor_contract::done_remaining_invalid_reference_message()
            ));
        };
        if !covered.insert(idx) {
            return Err(anyhow!(governor_contract::done_duplicate_criteria_message()));
        }
    }

    if covered.len() != plan.acceptance_criteria.len() {
        return Err(anyhow!(
            governor_contract::done_incomplete_coverage_message()
        ));
    }

    if acceptance_evidence.len() != completed_indices.len() {
        return Err(anyhow!(
            governor_contract::done_evidence_incomplete_message()
        ));
    }

    let mut evidence_rows = Vec::new();
    let mut evidence_indices = std::collections::BTreeSet::new();
    for evidence in acceptance_evidence {
        let Some(idx) = resolve_acceptance_reference(evidence.criterion.as_str(), plan) else {
            return Err(anyhow!(
                governor_contract::done_evidence_invalid_reference_message()
            ));
        };
        if !completed_indices.contains(&idx) {
            return Err(anyhow!(
                governor_contract::done_evidence_only_completed_message()
            ));
        }
        if !evidence_indices.insert(idx) {
            return Err(anyhow!(
                governor_contract::done_evidence_duplicate_criteria_message()
            ));
        }
        let Some(known_command) = preferred_known_command_for_evidence(
            evidence.command.as_str(),
            known_commands,
            observation_evidence,
        ) else {
            return Err(anyhow!(
                governor_contract::done_evidence_unknown_command_message()
            ));
        };
        evidence_rows.push((idx, known_command.to_string()));
    }

    Ok(evidence_rows)
}

fn evidence_score_label(score: f32) -> &'static str {
    if score >= 0.85 {
        "strong"
    } else if score >= 0.60 {
        "medium"
    } else {
        "weak"
    }
}

pub(super) fn build_done_acceptance_recovery_hint(
    error_text: &str,
    known_commands: &[String],
    read_only_scores: &[CriterionEvidenceScore],
) -> String {
    let mut lines = Vec::new();
    let low = error_text.to_ascii_lowercase();

    if low.contains("cover every completed acceptance criterion exactly once") {
        lines.push(
            "Hint: each completed_acceptance item needs exactly one acceptance_evidence row."
                .to_string(),
        );
        lines.push(
            "If you do not have proof yet, move that criterion from completed_acceptance to remaining_acceptance."
                .to_string(),
        );
    }

    if low.contains("known successful verification command") {
        lines.push(
            "Hint: cite only commands that already succeeded in this session; do not invent a new proof command inside done."
                .to_string(),
        );
    }

    if !known_commands.is_empty() {
        lines.push("Known successful commands you can cite now:".to_string());
        for command in known_commands.iter().rev().take(6) {
            lines.push(format!("- {}", compact_one_line(command, 200)));
        }
    }

    if !read_only_scores.is_empty() {
        lines.push(
            "Read-only evidence scores (use these to choose completed vs remaining):".to_string(),
        );
        for score in read_only_scores {
            let mut detail = format!(
                "- acceptance {}: {:.2} {} (search={:.2}, read={:.2}, repo={:.2})",
                score.idx + 1,
                score.total,
                evidence_score_label(score.total),
                score.search_specificity,
                score.read_confirm,
                score.repo_prior
            );
            if let Some(path) = score.best_path.as_deref() {
                detail.push_str(&format!(" path={path}"));
            }
            lines.push(detail);
            if !score.suggested_commands.is_empty() {
                lines.push(format!(
                    "  cite: {}",
                    score
                        .suggested_commands
                        .iter()
                        .take(2)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(" | ")
                ));
            }
        }
        lines.push(
            "Rule: for read-only tasks, criteria with strong scores are good completed candidates; medium scores usually need one more confirming read/search; weak scores should stay remaining."
                .to_string(),
        );
        if read_only_scores.iter().all(|score| score.total < 0.60) {
            lines.push(
                "Hint: you do not have enough read/search evidence yet. Use observation tools first, then call done.".to_string(),
            );
        }
    }

    if lines.is_empty() {
        String::new()
    } else {
        format!("\n{}", lines.join("\n"))
    }
}

fn finalization_step_hint(step: &str) -> bool {
    let low = step.to_ascii_lowercase();
    [
        "done",
        "final",
        "finalize",
        "summary",
        "summarize",
        "report",
        "handoff",
        "wrap up",
        "finish",
    ]
    .iter()
    .any(|term| low.contains(term))
}

fn known_commands_have_required_verification(
    known_commands: &[String],
    required_verification: VerificationLevel,
    test_cmd: Option<&str>,
) -> bool {
    known_commands.iter().any(|command| {
        classify_verify_level(command.as_str(), test_cmd)
            .map(|level| level.satisfies(required_verification))
            .unwrap_or(false)
    })
}

pub(super) fn should_prefer_done_after_verified_action(
    tc: &ToolCallData,
    plan: &PlanBlock,
    known_commands: &[String],
    required_verification: VerificationLevel,
    test_cmd: Option<&str>,
    last_mutation_step: Option<usize>,
    last_verify_ok_step: Option<usize>,
) -> bool {
    if tc.name.as_str() == "done" {
        return false;
    }

    let last_mutation = last_mutation_step.unwrap_or(0);
    let last_verify_ok = last_verify_ok_step.unwrap_or(0);
    if last_mutation == 0 || last_verify_ok < last_mutation {
        return false;
    }

    if !plan.steps.iter().any(|step| finalization_step_hint(step)) {
        return false;
    }

    if !known_commands_have_required_verification(known_commands, required_verification, test_cmd) {
        return false;
    }

    match tc.name.as_str() {
        "read_file" | "search_files" | "list_dir" | "glob" => true,
        "exec" => {
            let command = parse_exec_command_from_args(tc.arguments.as_str()).unwrap_or_default();
            classify_verify_level(command.as_str(), test_cmd)
                .map(|level| level.satisfies(required_verification))
                .unwrap_or_else(|| is_diagnostic_command(command.as_str()))
        }
        _ => false,
    }
}

pub(super) fn build_post_verify_done_completion_hint(
    plan: &PlanBlock,
    known_commands: &[String],
    tc: &ToolCallData,
    required_verification: VerificationLevel,
    test_cmd: Option<&str>,
) -> String {
    let attempted = canonicalize_tool_call_command(tc.name.as_str(), tc.arguments.as_str())
        .unwrap_or_else(|| blocked_tool_call_signature(tc.name.as_str(), tc.arguments.as_str()));
    let verification_commands = known_commands
        .iter()
        .filter(|command| {
            classify_verify_level(command.as_str(), test_cmd)
                .map(|level| level.satisfies(required_verification))
                .unwrap_or(false)
        })
        .take(3)
        .cloned()
        .collect::<Vec<_>>();

    let mut out = format!(
        "[Completion Gate]\n\
Required verification already passed after the latest mutation.\n\
Attempted next action: {attempted}\n\
Do NOT reopen inspection or rerun verification now.\n\
Next assistant turn: emit a <think> block with `tool: done`, then call `done` immediately.\n\
If any acceptance criterion is not fully proven, keep it in `remaining_acceptance` instead of gathering unrelated evidence.\n"
    );
    if !plan.acceptance_criteria.is_empty() {
        out.push_str("Current acceptance criteria:\n");
        for (idx, criterion) in plan.acceptance_criteria.iter().enumerate().take(4) {
            out.push_str(&format!(
                "- acceptance {}: {}\n",
                idx + 1,
                compact_one_line(criterion.as_str(), 160)
            ));
        }
    }
    if !verification_commands.is_empty() {
        out.push_str("Known successful verification commands you can cite now:\n");
        for command in verification_commands {
            out.push_str("- ");
            out.push_str(&compact_one_line(command.as_str(), 200));
            out.push('\n');
        }
    }
    out.push_str(
        "If you still need to explain remaining work, use `next_steps` in `done`; do not read unrelated files now.",
    );
    out
}

fn preferred_action_verification_command(
    known_commands: &[String],
    required_verification: VerificationLevel,
    test_cmd: Option<&str>,
) -> Option<String> {
    if let Some(configured) = test_cmd
        .map(str::trim)
        .filter(|command| !command.is_empty())
        .filter(|command| {
            configured_test_cmd_verification_level(Some(command))
                .map(|level| level.satisfies(required_verification))
                .unwrap_or(false)
        })
    {
        let configured_sig = command_sig_full(configured);
        if !configured_sig.is_empty()
            && known_commands.iter().any(|candidate| {
                let candidate_sig = command_sig_full(candidate.as_str());
                !candidate_sig.is_empty()
                    && (candidate_sig == configured_sig
                        || candidate_sig.contains(&configured_sig)
                        || configured_sig.contains(&candidate_sig))
            })
        {
            return Some(configured.to_string());
        }
    }

    known_commands.iter().rev().find_map(|command| {
        classify_verify_level(command.as_str(), test_cmd)
            .filter(|level| level.satisfies(required_verification))
            .map(|_| command.clone())
    })
}

fn preferred_action_observation_command(
    known_commands: &[String],
    observation_evidence: &ObservationEvidence,
) -> Option<String> {
    for prefix in ["read_file(", "search_files(", "glob(", "list_dir("] {
        if let Some(command) = known_commands.iter().rev().find(|command| {
            canonicalize_evidence_command_with_resolution(command.as_str(), observation_evidence)
                .starts_with(prefix)
        }) {
            return Some(command.clone());
        }
    }
    None
}

fn preferred_action_artifact_command(
    known_commands: &[String],
    observation_evidence: &ObservationEvidence,
) -> Option<String> {
    for prefix in ["write_file(", "patch_file(", "apply_diff("] {
        if let Some(command) = known_commands.iter().rev().find(|command| {
            canonicalize_evidence_command_with_resolution(command.as_str(), observation_evidence)
                .starts_with(prefix)
        }) {
            return Some(command.clone());
        }
    }
    None
}

fn artifact_command_path(
    command: &str,
    observation_evidence: &ObservationEvidence,
) -> Option<String> {
    let canonical = canonicalize_evidence_command_with_resolution(command, observation_evidence);
    let (name, args) = parse_named_command_signature(canonical.as_str())?;
    if !matches!(name.as_str(), "write_file" | "patch_file" | "apply_diff") {
        return None;
    }
    args.get("path").cloned()
}

fn criterion_prefers_artifact_proof(criterion: &str) -> bool {
    let low = criterion.to_ascii_lowercase();
    low.contains('/')
        || [
            ".rs",
            ".py",
            ".md",
            ".toml",
            ".json",
            ".yaml",
            ".yml",
            ".gitignore",
            "entrypoint",
            "readme",
            "gameplay",
            "logic",
        ]
        .iter()
        .any(|term| low.contains(term))
}

fn score_artifact_command_for_criterion(criterion: &str, path: &str) -> usize {
    let criterion_low = criterion.to_ascii_lowercase();
    let path_low = path.to_ascii_lowercase();
    let basename = path_low.rsplit('/').next().unwrap_or(path_low.as_str());

    let mut score = 0usize;
    if criterion_low.contains(path_low.as_str()) {
        score += 8;
    }
    if criterion_low.contains(basename) {
        score += 6;
    }
    for token in basename
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|token| token.len() >= 3)
    {
        if criterion_low.contains(token) {
            score += 1;
        }
    }
    score
}

fn preferred_action_artifact_command_for_criterion(
    criterion: &str,
    known_commands: &[String],
    observation_evidence: &ObservationEvidence,
) -> Option<String> {
    known_commands
        .iter()
        .rev()
        .filter_map(|command| {
            let path = artifact_command_path(command.as_str(), observation_evidence)?;
            Some((
                score_artifact_command_for_criterion(criterion, path.as_str()),
                command.clone(),
            ))
        })
        .filter(|(score, _)| *score > 0)
        .max_by_key(|(score, _)| *score)
        .map(|(_, command)| command)
        .or_else(|| preferred_action_artifact_command(known_commands, observation_evidence))
}

fn criterion_prefers_verification_proof(criterion: &str) -> bool {
    let low = criterion.to_ascii_lowercase();
    [
        "implement",
        "implemented",
        "change",
        "fix",
        "verify",
        "verification",
        "behavioral",
        "build",
        "lint",
        "test",
        "pass",
        "passes",
        "final result",
        "final answer",
        "cite",
        "command",
        "summary",
        "report",
    ]
    .iter()
    .any(|term| low.contains(term))
}

fn criterion_prefers_observation_proof(criterion: &str) -> bool {
    let low = criterion.to_ascii_lowercase();
    [
        "file", "path", "read", "context", "handler", "symbol", "location", "line",
    ]
    .iter()
    .any(|term| low.contains(term))
}

fn successful_mutation_paths(messages: &[serde_json::Value]) -> Vec<(String, String)> {
    let mut pending_by_id: std::collections::BTreeMap<String, (String, String)> =
        std::collections::BTreeMap::new();
    let mut successes = Vec::new();

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
                    if id.is_empty() || !matches!(name, "write_file" | "patch_file" | "apply_diff")
                    {
                        continue;
                    }
                    let Some(arguments) = tc
                        .get("function")
                        .and_then(|v| v.get("arguments"))
                        .and_then(|v| v.as_str())
                    else {
                        continue;
                    };
                    let path = serde_json::from_str::<serde_json::Value>(arguments)
                        .ok()
                        .and_then(|value| {
                            value
                                .get("path")
                                .and_then(|v| v.as_str())
                                .map(str::trim)
                                .filter(|path| !path.is_empty())
                                .map(ToString::to_string)
                        });
                    let Some(path) = path else {
                        continue;
                    };
                    pending_by_id.insert(id.to_string(), (name.to_string(), path));
                }
            }
            "tool" => {
                let id = msg
                    .get("tool_call_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim();
                if id.is_empty() {
                    continue;
                }
                let content = msg.get("content").and_then(|v| v.as_str()).unwrap_or("");
                if !looks_like_successful_mutation_result(content) {
                    continue;
                }
                if let Some(entry) = pending_by_id.get(id) {
                    successes.push(entry.clone());
                }
            }
            _ => {}
        }
    }

    successes
}

fn latest_successful_mutation_path(messages: &[serde_json::Value]) -> Option<(String, String)> {
    successful_mutation_paths(messages).into_iter().last()
}

fn unique_successful_mutation_paths(messages: &[serde_json::Value]) -> Vec<String> {
    let mut out = Vec::new();
    for (_, path) in successful_mutation_paths(messages) {
        if !out.contains(&path) {
            out.push(path);
        }
    }
    out
}

fn inferred_followup_summary_paths(messages: &[serde_json::Value]) -> Vec<String> {
    let paths = unique_successful_mutation_paths(messages);
    let mut chosen = Vec::new();

    if let Some(code_path) = paths
        .iter()
        .rev()
        .find(|path| path.starts_with("src/") && path.ends_with(".rs"))
        .cloned()
    {
        chosen.push(code_path);
    }
    if let Some(docs_path) = paths
        .iter()
        .rev()
        .find(|path| path.starts_with("docs/") && path.ends_with(".md"))
        .cloned()
    {
        if !chosen.contains(&docs_path) {
            chosen.push(docs_path);
        }
    }
    if let Some(replay_path) = paths
        .iter()
        .rev()
        .find(|path| path.ends_with("tui_replay.json"))
        .cloned()
    {
        if !chosen.contains(&replay_path) {
            chosen.push(replay_path);
        }
    }

    chosen
}

fn format_path_list(paths: &[String]) -> String {
    let quoted = paths
        .iter()
        .map(|path| format!("`{}`", compact_one_line(path.as_str(), 160)))
        .collect::<Vec<_>>();
    match quoted.as_slice() {
        [] => String::new(),
        [only] => only.clone(),
        [first, second] => format!("{first} and {second}"),
        _ => {
            let mut out = quoted[..quoted.len() - 1].join(", ");
            out.push_str(", and ");
            out.push_str(&quoted[quoted.len() - 1]);
            out
        }
    }
}

fn repo_root_from_success_path(path: &str) -> Option<String> {
    let trimmed = path.trim().trim_end_matches('/');
    for suffix in [
        "/src/lib.rs",
        "/src/main.rs",
        "/README.md",
        "/.gitignore",
        "/Cargo.toml",
        "/game.py",
        "/main.py",
        "/test_game.py",
    ] {
        if let Some(root) = trimmed.strip_suffix(suffix) {
            let root = root.trim_end_matches('/');
            if !root.is_empty() {
                return Some(root.to_string());
            }
        }
    }
    None
}

fn preferred_repo_logic_path(repo_root: &str, paths: &[String]) -> Option<String> {
    let exact_candidates = [
        format!("{repo_root}/src/lib.rs"),
        format!("{repo_root}/game.py"),
        format!("{repo_root}/lib.py"),
        format!("{repo_root}/src/main.rs"),
        format!("{repo_root}/main.py"),
    ];
    for candidate in &exact_candidates {
        if paths.iter().any(|path| path == candidate) {
            return Some(candidate.clone());
        }
    }

    paths.iter().find_map(|path| {
        let low = path.to_ascii_lowercase();
        if !path.starts_with(&format!("{repo_root}/")) {
            return None;
        }
        if low.ends_with("/readme.md")
            || low.ends_with("/.gitignore")
            || low.ends_with("/cargo.toml")
            || low.ends_with("/test_game.py")
            || low.contains("/tests/")
            || low.contains("/test_")
        {
            return None;
        }
        if [".rs", ".py", ".js", ".ts"]
            .iter()
            .any(|ext| low.ends_with(ext))
        {
            return Some(path.clone());
        }
        None
    })
}

fn preferred_repo_entrypoint_path(repo_root: &str, paths: &[String]) -> Option<String> {
    let exact_candidates = [
        format!("{repo_root}/src/main.rs"),
        format!("{repo_root}/main.py"),
        format!("{repo_root}/main.js"),
        format!("{repo_root}/main.ts"),
    ];
    for candidate in &exact_candidates {
        if paths.iter().any(|path| path == candidate) {
            return Some(candidate.clone());
        }
    }
    None
}

fn inferred_repo_scaffold_artifacts(
    messages: &[serde_json::Value],
) -> Option<RepoScaffoldArtifacts> {
    let successes = successful_mutation_paths(messages);
    let mut groups: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();
    for (_, path) in &successes {
        let Some(repo_root) = repo_root_from_success_path(path.as_str()) else {
            continue;
        };
        groups.entry(repo_root).or_default().push(path.clone());
    }

    let (repo_root, paths) = groups.into_iter().max_by_key(|(repo_root, paths)| {
        let mut score = paths.len();
        if paths.iter().any(|path| path.ends_with("/README.md")) {
            score += 3;
        }
        if paths
            .iter()
            .any(|path| path.ends_with("/src/lib.rs") || path.ends_with("/game.py"))
        {
            score += 4;
        }
        if paths
            .iter()
            .any(|path| path.ends_with("/src/main.rs") || path.ends_with("/main.py"))
        {
            score += 2;
        }
        score + repo_root.len() / 1000
    })?;

    let logic_path = preferred_repo_logic_path(repo_root.as_str(), paths.as_slice());
    let entrypoint_path = preferred_repo_entrypoint_path(repo_root.as_str(), paths.as_slice());

    Some(RepoScaffoldArtifacts {
        repo_root,
        logic_path,
        entrypoint_path,
    })
}

fn looks_like_successful_mutation_result(content: &str) -> bool {
    let trimmed = content.trim_start();
    trimmed.starts_with("OK: wrote '")
        || trimmed.starts_with("OK: patched '")
        || trimmed.starts_with("OK: applied ")
}

pub(super) fn latest_assistant_message_is_done(messages: &[serde_json::Value]) -> bool {
    messages
        .iter()
        .rev()
        .find(|msg| msg.get("role").and_then(|v| v.as_str()) == Some("assistant"))
        .and_then(|msg| msg.get("content").and_then(|v| v.as_str()))
        .map(|content| content.trim_start().starts_with("[DONE]"))
        .unwrap_or(false)
}

pub(super) fn synthesize_action_done_summary(
    plan: Option<&PlanBlock>,
    messages: &[serde_json::Value],
) -> Option<String> {
    if let Some(artifacts) = inferred_repo_scaffold_artifacts(messages) {
        let repo_root = compact_one_line(artifacts.repo_root.as_str(), 160);
        let logic_path = artifacts
            .logic_path
            .as_deref()
            .map(|path| compact_one_line(path, 160));
        let entrypoint_path = artifacts
            .entrypoint_path
            .as_deref()
            .map(|path| compact_one_line(path, 160));
        return Some(match (logic_path, entrypoint_path) {
            (Some(logic), Some(entrypoint)) if logic != entrypoint => format!(
                "Created repository at `{repo_root}/` with main logic in `{logic}` and runnable entrypoint in `{entrypoint}`."
            ),
            (Some(logic), _) => {
                format!("Created repository at `{repo_root}/` with main logic in `{logic}`.")
            }
            (None, Some(entrypoint)) => format!(
                "Created repository at `{repo_root}/` with runnable entrypoint in `{entrypoint}`."
            ),
            (None, None) => format!("Created repository at `{repo_root}/` and verified its starter artifacts."),
        });
    }

    let followup_paths = inferred_followup_summary_paths(messages);
    if followup_paths.len() >= 2 {
        return Some(format!(
            "Updated {} and verified the requested change.",
            format_path_list(&followup_paths)
        ));
    }

    if let Some((tool, path)) = latest_successful_mutation_path(messages) {
        let path = compact_one_line(path.as_str(), 160);
        return Some(match tool.as_str() {
            "write_file" => format!("Created `{path}` and verified it."),
            "patch_file" | "apply_diff" => {
                format!("Updated `{path}` and verified the requested change.")
            }
            _ => format!("Completed the requested change in `{path}` and verified it."),
        });
    }

    plan.map(|plan| {
        let goal = compact_one_line(plan.goal.as_str(), 180);
        if goal == "-" {
            "Completed the requested task and verified it.".to_string()
        } else {
            format!("Completed and verified: {goal}")
        }
    })
}

pub(super) fn maybe_build_verified_action_auto_final_answer(
    plan: Option<&PlanBlock>,
    messages: &[serde_json::Value],
    known_commands: &[String],
    observation_evidence: &ObservationEvidence,
    required_verification: VerificationLevel,
    test_cmd: Option<&str>,
    last_mutation_step: Option<usize>,
    last_verify_ok_step: Option<usize>,
) -> Option<String> {
    let plan = plan?;
    let (completed_acceptance, remaining_acceptance, acceptance_evidence) =
        rescue_invalid_done_payload_for_verified_action(
            Some(plan),
            known_commands,
            observation_evidence,
            required_verification,
            test_cmd,
            last_mutation_step,
            last_verify_ok_step,
        )?;
    let evidence_rows = validate_done_acceptance(
        Some(plan),
        &completed_acceptance,
        &remaining_acceptance,
        &acceptance_evidence,
        known_commands,
        observation_evidence,
    )
    .ok()?;
    let evidence_by_idx: std::collections::HashMap<usize, String> =
        evidence_rows.into_iter().collect();
    let summary = synthesize_action_done_summary(Some(plan), messages).unwrap_or_default();

    let mut final_text = String::from("[DONE]\n");
    if !summary.is_empty() {
        final_text.push_str(summary.as_str());
    }
    if let Some(artifacts) = inferred_repo_scaffold_artifacts(messages) {
        final_text.push_str("\n\nArtifacts:\n");
        final_text.push_str("- repo: `");
        final_text.push_str(compact_one_line(artifacts.repo_root.as_str(), 160).as_str());
        final_text.push_str("/`\n");
        if let Some(logic_path) = artifacts.logic_path.as_deref() {
            final_text.push_str("- main logic: `");
            final_text.push_str(compact_one_line(logic_path, 160).as_str());
            final_text.push_str("`\n");
        }
        if let Some(entrypoint_path) = artifacts.entrypoint_path.as_deref() {
            final_text.push_str("- entrypoint: `");
            final_text.push_str(compact_one_line(entrypoint_path, 160).as_str());
            final_text.push_str("`\n");
        }
    }
    final_text.push_str("\n\nAcceptance:\n");
    for item in &completed_acceptance {
        let idx = resolve_acceptance_reference(item, plan)?;
        final_text.push_str("- done: ");
        final_text.push_str(acceptance_reference_label(plan, idx).as_str());
        if let Some(command) = evidence_by_idx.get(&idx) {
            final_text.push_str(" via `");
            final_text.push_str(command.as_str());
            final_text.push('`');
        }
        final_text.push('\n');
    }
    for item in &remaining_acceptance {
        let idx = resolve_acceptance_reference(item, plan)?;
        final_text.push_str("- remaining: ");
        final_text.push_str(acceptance_reference_label(plan, idx).as_str());
        final_text.push('\n');
    }
    Some(final_text)
}

pub(super) fn rescue_invalid_done_payload_for_verified_action(
    plan: Option<&PlanBlock>,
    known_commands: &[String],
    observation_evidence: &ObservationEvidence,
    required_verification: VerificationLevel,
    test_cmd: Option<&str>,
    last_mutation_step: Option<usize>,
    last_verify_ok_step: Option<usize>,
) -> Option<(Vec<String>, Vec<String>, Vec<DoneAcceptanceEvidence>)> {
    let plan = plan?;
    let last_mutation = last_mutation_step.unwrap_or(0);
    let last_verify_ok = last_verify_ok_step.unwrap_or(0);
    if last_mutation == 0 || last_verify_ok < last_mutation {
        return None;
    }

    let verify_command =
        preferred_action_verification_command(known_commands, required_verification, test_cmd);
    let observation_command =
        preferred_action_observation_command(known_commands, observation_evidence);

    let mut completed_acceptance = Vec::new();
    let mut remaining_acceptance = Vec::new();
    let mut acceptance_evidence = Vec::new();

    for (idx, criterion) in plan.acceptance_criteria.iter().enumerate() {
        let label = acceptance_reference_label(plan, idx);
        let artifact_command = preferred_action_artifact_command_for_criterion(
            criterion,
            known_commands,
            observation_evidence,
        );
        let command = if criterion_prefers_verification_proof(criterion) {
            verify_command
                .clone()
                .or_else(|| artifact_command.clone())
                .or_else(|| observation_command.clone())
        } else if criterion_prefers_observation_proof(criterion) {
            observation_command
                .clone()
                .or_else(|| artifact_command.clone())
                .or_else(|| verify_command.clone())
        } else if criterion_prefers_artifact_proof(criterion) {
            artifact_command
                .clone()
                .or_else(|| observation_command.clone())
                .or_else(|| verify_command.clone())
        } else {
            None
        };

        if let Some(command) = command {
            completed_acceptance.push(label.clone());
            acceptance_evidence.push(DoneAcceptanceEvidence {
                criterion: label,
                command,
            });
        } else {
            remaining_acceptance.push(label);
        }
    }

    if completed_acceptance.is_empty() {
        return None;
    }

    Some((
        completed_acceptance,
        remaining_acceptance,
        acceptance_evidence,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn iteration_cap_final_answer_prefers_read_file_for_repo_map_fallback_task() {
        let plan = synthetic_read_only_observation_plan(
            "Find where coder-side repo-map read_file fallback is wired into the TUI agent flow. Do not edit anything.",
        );
        let messages = vec![
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_search",
                    "type": "function",
                    "function": {
                        "name": "search_files",
                        "arguments": "{\"pattern\":\"lazy_read_fallback\",\"dir\":\"src/tui\"}"
                    }
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_search",
                "content": "[search_files: 'lazy_read_fallback' — 1 match(es)]\nsrc/tui/agent.rs:12577: crate::repo_map::lazy_read_fallback(root, &path)"
            }),
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_read",
                    "type": "function",
                    "function": {
                        "name": "read_file",
                        "arguments": "{\"path\":\"tui/agent.rs\"}"
                    }
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_read",
                "content": "[src/tui/agent.rs] (16610 lines, 712345 bytes)\nif let Some(fallback) = crate::repo_map::lazy_read_fallback(root, &path) {"
            }),
        ];

        let mut evidence = collect_observation_evidence(&messages);
        evidence.remember_resolution("tui/agent.rs", "src/tui/agent.rs", "repo_map:read_file");
        let final_text = build_read_only_iteration_cap_final_answer(
            "Find where coder-side repo-map read_file fallback is wired into the TUI agent flow. Do not edit anything.",
            &plan,
            &evidence,
            &messages,
            &WorkingMemory::default(),
        )
        .expect("final answer");

        assert!(final_text.contains("src/tui/agent.rs") || final_text.contains("tui/agent.rs"));
        assert!(
            final_text.contains("via `read_file(path=src/tui/agent.rs)`")
                || final_text.contains("via `read_file(path=tui/agent.rs)`")
        );
        assert!(!final_text.contains("via `search_files("));
    }

    #[test]
    fn validate_done_acceptance_prefers_read_file_evidence_when_present() {
        let plan = synthetic_read_only_observation_plan(
            "Find where coder-side repo-map read_file fallback is wired into the TUI agent flow. Do not edit anything.",
        );
        let completed_acceptance = vec![
            "1) File path identifying coder-side repo-map read_file fallback wiring".to_string(),
            "2) Confirmed handling context where read_file miss fallback is invoked".to_string(),
        ];
        let acceptance_evidence = vec![
            DoneAcceptanceEvidence {
                criterion: completed_acceptance[0].clone(),
                command: "search_files(dir=src/tui, pattern=lazy_read_fallback)".to_string(),
            },
            DoneAcceptanceEvidence {
                criterion: completed_acceptance[1].clone(),
                command: "read_file(path=src/tui/agent.rs)".to_string(),
            },
        ];
        let known_commands = vec![
            "search_files(dir=src/tui, pattern=lazy_read_fallback)".to_string(),
            "read_file(path=src/tui/agent.rs)".to_string(),
        ];
        let mut evidence = ObservationEvidence::default();
        evidence.remember_read("read_file(path=tui/agent.rs)", "src/tui/agent.rs");
        evidence.remember_resolution("tui/agent.rs", "src/tui/agent.rs", "repo_map:read_file");

        let rows = validate_done_acceptance(
            Some(&plan),
            &completed_acceptance,
            &[],
            &acceptance_evidence,
            &known_commands,
            &evidence,
        )
        .expect("done evidence rows");

        assert_eq!(
            rows,
            vec![
                (
                    0,
                    "search_files(dir=src/tui, pattern=lazy_read_fallback)".to_string(),
                ),
                (1, "read_file(path=src/tui/agent.rs)".to_string()),
            ]
        );
    }

    #[test]
    fn completion_hint_accepts_single_observed_generic_target_after_read() {
        let plan = synthetic_read_only_observation_plan(
            "Locate where project-local profile aliases are loaded for the greet command. Do not edit anything.",
        );
        let messages = vec![
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_search",
                    "type": "function",
                    "function": {
                        "name": "search_files",
                        "arguments": "{\"pattern\":\"aliases\",\"dir\":\"src\"}"
                    }
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_search",
                "content": "[search_files: 'aliases' — 7 match(es)]\nsrc/config.rs:7:     pub aliases: BTreeMap<String, String>,"
            }),
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_read",
                    "type": "function",
                    "function": {
                        "name": "read_file",
                        "arguments": "{\"path\":\"src/config.rs\"}"
                    }
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_read",
                "content": "[src/config.rs] (39 lines, 1136 bytes)\npub fn resolve_profile_alias(root: &Path, requested: Option<&str>, config: &AppConfig) -> String {"
            }),
        ];
        let evidence = collect_observation_evidence(&messages);

        let hint = build_read_only_completion_hint(
            "Locate where project-local profile aliases are loaded for the greet command. Do not edit anything.",
            &plan,
            &evidence,
            &messages,
            &WorkingMemory::default(),
        )
        .expect("completion hint");

        assert!(hint.contains("call done directly now"));
        assert!(hint.contains("src/config.rs"));
    }

    #[test]
    fn iteration_cap_final_answer_supports_single_observed_generic_target() {
        let plan = synthetic_read_only_observation_plan(
            "Locate where project-local profile aliases are loaded for the greet command. Do not edit anything.",
        );
        let messages = vec![
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_search",
                    "type": "function",
                    "function": {
                        "name": "search_files",
                        "arguments": "{\"pattern\":\"aliases\",\"dir\":\"src\"}"
                    }
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_search",
                "content": "[search_files: 'aliases' — 7 match(es)]\nsrc/config.rs:7:     pub aliases: BTreeMap<String, String>,"
            }),
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_read",
                    "type": "function",
                    "function": {
                        "name": "read_file",
                        "arguments": "{\"path\":\"src/config.rs\"}"
                    }
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_read",
                "content": "[src/config.rs] (39 lines, 1136 bytes)\npub fn resolve_profile_alias(root: &Path, requested: Option<&str>, config: &AppConfig) -> String {"
            }),
        ];
        let evidence = collect_observation_evidence(&messages);

        let final_text = build_read_only_iteration_cap_final_answer(
            "Locate where project-local profile aliases are loaded for the greet command. Do not edit anything.",
            &plan,
            &evidence,
            &messages,
            &WorkingMemory::default(),
        )
        .expect("final answer");

        assert!(final_text.contains("src/config.rs"));
        assert!(final_text.contains("read_file(path=src/config.rs)"));
    }

    #[test]
    fn should_prefer_done_after_verified_action_blocks_post_verify_reads() {
        let plan = PlanBlock {
            goal: "Fix the failing test with the smallest code change.".to_string(),
            steps: vec![
                "inspect the failing code path".to_string(),
                "patch the smallest confirmed bug".to_string(),
                "run cargo test 2>&1".to_string(),
                "call done with verified results".to_string(),
            ],
            acceptance_criteria: vec![
                "the requested change is implemented".to_string(),
                "cargo test 2>&1 passes".to_string(),
            ],
            risks: "wrong file".to_string(),
            assumptions: "cargo test is relevant".to_string(),
        };
        let tc = ToolCallData {
            id: "call_read".to_string(),
            name: "read_file".to_string(),
            arguments: serde_json::json!({"path":"Cargo.toml"}).to_string(),
        };
        let known_commands = vec!["cargo test 2>&1".to_string()];

        assert!(should_prefer_done_after_verified_action(
            &tc,
            &plan,
            &known_commands,
            VerificationLevel::Behavioral,
            Some("cargo test 2>&1"),
            Some(4),
            Some(5),
        ));
    }

    #[test]
    fn should_prefer_done_after_verified_action_accepts_verified_patch_auto_test() {
        let plan = PlanBlock {
            goal: "Fix the failing test with the smallest code change.".to_string(),
            steps: vec![
                "inspect the failing code path".to_string(),
                "patch the smallest confirmed bug".to_string(),
                "run cargo test 2>&1".to_string(),
                "call done with verified results".to_string(),
            ],
            acceptance_criteria: vec![
                "the requested change is implemented".to_string(),
                "cargo test 2>&1 passes".to_string(),
            ],
            risks: "wrong file".to_string(),
            assumptions: "cargo test is relevant".to_string(),
        };
        let tc = ToolCallData {
            id: "call_exec".to_string(),
            name: "exec".to_string(),
            arguments: serde_json::json!({"command":"cargo test 2>&1"}).to_string(),
        };
        let known_commands = vec!["cargo test 2>&1".to_string()];

        assert!(should_prefer_done_after_verified_action(
            &tc,
            &plan,
            &known_commands,
            VerificationLevel::Behavioral,
            Some("cargo test 2>&1"),
            Some(4),
            Some(4),
        ));
    }

    #[test]
    fn should_prefer_done_after_verified_action_requires_known_verification_command() {
        let plan = PlanBlock {
            goal: "Fix the failing test with the smallest code change.".to_string(),
            steps: vec![
                "inspect the failing code path".to_string(),
                "patch the smallest confirmed bug".to_string(),
                "run cargo test 2>&1".to_string(),
                "call done with verified results".to_string(),
            ],
            acceptance_criteria: vec![
                "the requested change is implemented".to_string(),
                "cargo test 2>&1 passes".to_string(),
            ],
            risks: "wrong file".to_string(),
            assumptions: "cargo test is relevant".to_string(),
        };
        let tc = ToolCallData {
            id: "call_read".to_string(),
            name: "read_file".to_string(),
            arguments: serde_json::json!({"path":"Cargo.toml"}).to_string(),
        };

        assert!(!should_prefer_done_after_verified_action(
            &tc,
            &plan,
            &[],
            VerificationLevel::Behavioral,
            Some("cargo test 2>&1"),
            Some(4),
            Some(4),
        ));
    }

    #[test]
    fn post_verify_done_completion_hint_mentions_done_and_verification() {
        let plan = PlanBlock {
            goal: "Fix the failing test with the smallest code change.".to_string(),
            steps: vec![
                "inspect the failing code path".to_string(),
                "patch the smallest confirmed bug".to_string(),
                "run cargo test 2>&1".to_string(),
                "call done with verified results".to_string(),
            ],
            acceptance_criteria: vec![
                "the requested change is implemented".to_string(),
                "cargo test 2>&1 passes".to_string(),
            ],
            risks: "wrong file".to_string(),
            assumptions: "cargo test is relevant".to_string(),
        };
        let tc = ToolCallData {
            id: "call_read".to_string(),
            name: "read_file".to_string(),
            arguments: serde_json::json!({"path":"Cargo.toml"}).to_string(),
        };
        let hint = build_post_verify_done_completion_hint(
            &plan,
            &["cargo test 2>&1".to_string()],
            &tc,
            VerificationLevel::Behavioral,
            Some("cargo test 2>&1"),
        );

        assert!(hint.contains("[Completion Gate]"));
        assert!(hint.contains("tool: done"));
        assert!(hint.contains("cargo test 2>&1"));
    }

    #[test]
    fn rescue_invalid_done_payload_for_verified_action_repairs_smoke_acceptance() {
        let plan = PlanBlock {
            goal: "Fix the failing test with the smallest code change.".to_string(),
            steps: vec![
                "inspect the failing code path".to_string(),
                "patch the smallest confirmed bug".to_string(),
                "run cargo test 2>&1".to_string(),
                "call done with verified results".to_string(),
            ],
            acceptance_criteria: vec![
                "the requested change is implemented and confirmed by a passing behavioral verification command".to_string(),
                "the final result cites the exact passing behavioral verification command".to_string(),
            ],
            risks: "wrong file".to_string(),
            assumptions: "cargo test is relevant".to_string(),
        };
        let observation_evidence = ObservationEvidence {
            reads: vec![ObservationReadEvidence {
                command: "read_file(path=src/lib.rs)".to_string(),
                path: "src/lib.rs".to_string(),
            }],
            searches: Vec::new(),
            resolutions: Vec::new(),
        };

        let (completed, remaining, evidence) = rescue_invalid_done_payload_for_verified_action(
            Some(&plan),
            &[
                "read_file(path=src/lib.rs)".to_string(),
                "cargo test 2>&1".to_string(),
            ],
            &observation_evidence,
            VerificationLevel::Behavioral,
            Some("cargo test 2>&1"),
            Some(3),
            Some(3),
        )
        .expect("rescued done payload");

        assert_eq!(
            completed,
            vec![
                acceptance_reference_label(&plan, 0),
                acceptance_reference_label(&plan, 1)
            ]
        );
        assert!(remaining.is_empty());
        assert_eq!(evidence.len(), 2);
        assert!(evidence.iter().all(|row| row.command == "cargo test 2>&1"));
    }

    #[test]
    fn synthesize_action_done_summary_prefers_created_file_path() {
        let messages = vec![
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_write",
                    "type": "function",
                    "function": {
                        "name":"write_file",
                        "arguments": "{\"path\":\"notes/todo.txt\",\"content\":\"ship it\\n\"}"
                    }
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_write",
                "content": "OK: wrote 'notes/todo.txt' (1 lines, 8 bytes)\n[auto-test] ✓ PASSED (exit 0)"
            }),
        ];

        let summary = synthesize_action_done_summary(None, &messages).expect("summary");
        assert!(summary.contains("notes/todo.txt"));
        assert!(summary.contains("Created"));
    }

    #[test]
    fn synthesize_action_done_summary_mentions_code_docs_and_replay_followups() {
        let messages = vec![
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_rules",
                    "type": "function",
                    "function": {
                        "name":"patch_file",
                        "arguments":"{\"path\":\"src/observer/repo_rules.rs\",\"search\":\"old\",\"replace\":\"new\"}"
                    }
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_rules",
                "content": "OK: patched 'src/observer/repo_rules.rs' (+1 lines, 10 total)"
            }),
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_docs",
                    "type": "function",
                    "function": {
                        "name":"patch_file",
                        "arguments":"{\"path\":\"docs/runtime-architecture.md\",\"search\":\"old\",\"replace\":\"new\"}"
                    }
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_docs",
                "content": "OK: patched 'docs/runtime-architecture.md' (+1 lines, 12 total)"
            }),
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_replay",
                    "type": "function",
                    "function": {
                        "name":"patch_file",
                        "arguments":"{\"path\":\".spiral-coder/tui_replay.json\",\"search\":\"old\",\"replace\":\"new\"}"
                    }
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_replay",
                "content": "OK: patched '.spiral-coder/tui_replay.json' (+1 lines, 10 total)"
            }),
        ];

        let summary = synthesize_action_done_summary(None, &messages).expect("summary");
        assert!(summary.contains("src/observer/repo_rules.rs"));
        assert!(summary.contains("docs/runtime-architecture.md"));
        assert!(summary.contains(".spiral-coder/tui_replay.json"));
    }

    #[test]
    fn synthesize_action_done_summary_prefers_repo_logic_over_late_gitignore() {
        let messages = vec![
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_lib",
                    "type": "function",
                    "function": {
                        "name":"write_file",
                        "arguments":"{\"path\":\"maze_game/src/lib.rs\",\"content\":\"pub fn demo() {}\\n\"}"
                    }
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_lib",
                "content": "OK: wrote 'maze_game/src/lib.rs' (1 lines, 17 bytes)"
            }),
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_main",
                    "type": "function",
                    "function": {
                        "name":"write_file",
                        "arguments":"{\"path\":\"maze_game/src/main.rs\",\"content\":\"fn main() {}\\n\"}"
                    }
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_main",
                "content": "OK: wrote 'maze_game/src/main.rs' (1 lines, 13 bytes)"
            }),
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_gitignore",
                    "type": "function",
                    "function": {
                        "name":"write_file",
                        "arguments":"{\"path\":\"maze_game/.gitignore\",\"content\":\"target/\\n\"}"
                    }
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_gitignore",
                "content": "OK: wrote 'maze_game/.gitignore' (1 lines, 8 bytes)"
            }),
        ];

        let summary = synthesize_action_done_summary(None, &messages).expect("summary");
        assert!(summary.contains("maze_game/"));
        assert!(summary.contains("maze_game/src/lib.rs"));
        assert!(summary.contains("maze_game/src/main.rs"));
        assert!(!summary.contains(".gitignore"));
    }

    #[test]
    fn synthesize_action_done_summary_mentions_repo_root_and_python_logic() {
        let messages = vec![
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_readme",
                    "type": "function",
                    "function": {
                        "name":"write_file",
                        "arguments":"{\"path\":\"maze_game_pygame/README.md\",\"content\":\"# maze\\n\"}"
                    }
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_readme",
                "content": "OK: wrote 'maze_game_pygame/README.md' (1 lines, 7 bytes)"
            }),
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_game",
                    "type": "function",
                    "function": {
                        "name":"write_file",
                        "arguments":"{\"path\":\"maze_game_pygame/game.py\",\"content\":\"class MazeGame: ...\\n\"}"
                    }
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_game",
                "content": "OK: wrote 'maze_game_pygame/game.py' (1 lines, 20 bytes)"
            }),
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_main",
                    "type": "function",
                    "function": {
                        "name":"write_file",
                        "arguments":"{\"path\":\"maze_game_pygame/main.py\",\"content\":\"def main(): ...\\n\"}"
                    }
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_main",
                "content": "OK: wrote 'maze_game_pygame/main.py' (1 lines, 16 bytes)"
            }),
        ];

        let summary = synthesize_action_done_summary(None, &messages).expect("summary");
        assert!(summary.contains("maze_game_pygame/"));
        assert!(summary.contains("maze_game_pygame/game.py"));
        assert!(summary.contains("maze_game_pygame/main.py"));
    }

    #[test]
    fn verified_action_auto_final_answer_uses_repo_artifact_evidence() {
        let plan = PlanBlock {
            goal: "Create a new git repo in demo_repo with starter files.".to_string(),
            steps: vec![
                "create demo_repo".to_string(),
                "verify repo artifacts".to_string(),
                "call done".to_string(),
            ],
            acceptance_criteria: vec![
                "the requested artifact is created and confirmed by a passing build/check/lint verification command".to_string(),
                "the final result cites the exact passing build/check/lint verification command".to_string(),
            ],
            risks: "wrong path".to_string(),
            assumptions: "the configured verification command is relevant".to_string(),
        };
        let messages = vec![
            json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_write",
                    "type": "function",
                    "function": {
                        "name":"write_file",
                        "arguments":"{\"path\":\"demo_repo/.gitignore\",\"content\":\"target/\\n\"}"
                    }
                }]
            }),
            json!({
                "role": "tool",
                "tool_call_id": "call_write",
                "content": "OK: wrote 'demo_repo/.gitignore' (1 lines, 8 bytes)\n[auto-test] ✓ PASSED (exit 0)"
            }),
        ];

        let final_text = maybe_build_verified_action_auto_final_answer(
            Some(&plan),
            &messages,
            &["test -d demo_repo/.git && test -f demo_repo/README.md && test -f demo_repo/.gitignore".to_string()],
            &ObservationEvidence::default(),
            VerificationLevel::Build,
            Some("test -d demo_repo/.git && test -f demo_repo/README.md && test -f demo_repo/.gitignore"),
            Some(6),
            Some(6),
        )
        .expect("auto final");

        assert!(final_text.contains("demo_repo/.gitignore"));
        assert!(final_text.contains("Acceptance:"));
        assert!(final_text.contains("test -d demo_repo/.git"));
        assert!(final_text.contains("README.md"));
        assert!(final_text.contains("Artifacts:"));
        assert!(final_text.contains("repo: `demo_repo/`"));
    }

    #[test]
    fn verified_action_auto_final_answer_accepts_successful_write_as_artifact_proof() {
        let plan = PlanBlock {
            goal: "Create a pygame maze repo and verify it.".to_string(),
            steps: vec![
                "initialize the repo".to_string(),
                "write the pygame files".to_string(),
                "run the unittest smoke".to_string(),
                "call done".to_string(),
            ],
            acceptance_criteria: vec![
                "maze_game_pygame/README.md explains the controls".to_string(),
                "maze_game_pygame/game.py contains the maze gameplay logic".to_string(),
                "maze_game_pygame/main.py launches the game loop".to_string(),
                "maze_game_pygame/test_game.py passes under SDL_VIDEODRIVER=dummy python3 -m unittest -q 2>&1".to_string(),
            ],
            risks: "wrong repo layout".to_string(),
            assumptions: "pygame is available".to_string(),
        };
        let verify_command = "SDL_VIDEODRIVER=dummy python3 -m unittest -q 2>&1";
        let final_text = maybe_build_verified_action_auto_final_answer(
            Some(&plan),
            &[json!({
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_main",
                    "type": "function",
                    "function": {
                        "name":"write_file",
                        "arguments":"{\"path\":\"maze_game_pygame/main.py\",\"content\":\"print('maze')\\n\"}"
                    }
                }]
            }), json!({
                "role": "tool",
                "tool_call_id": "call_main",
                "content": "OK: wrote 'maze_game_pygame/main.py' (1 lines, 13 bytes)\n[auto-test] ✓ PASSED (exit 0)"
            })],
            &[
                verify_command.to_string(),
                "write_file(path=maze_game_pygame/main.py)".to_string(),
            ],
            &ObservationEvidence::default(),
            VerificationLevel::Behavioral,
            Some(verify_command),
            Some(7),
            Some(7),
        )
        .expect("auto final");

        assert!(final_text.contains("maze_game_pygame/main.py"));
        assert!(final_text.contains("SDL_VIDEODRIVER=dummy python3 -m unittest -q 2>&1"));
    }

    #[test]
    fn rescue_invalid_done_payload_for_verified_action_uses_file_specific_artifact_proof() {
        let plan = PlanBlock {
            goal: "Create a pygame maze repo and verify it.".to_string(),
            steps: vec![
                "initialize the repo".to_string(),
                "write the pygame files".to_string(),
                "run the unittest smoke".to_string(),
                "call done".to_string(),
            ],
            acceptance_criteria: vec![
                "maze_game_pygame/game.py contains the maze gameplay logic".to_string(),
                "maze_game_pygame/main.py remains runnable".to_string(),
                "maze_game_pygame/test_game.py passes under SDL_VIDEODRIVER=dummy python3 -m unittest -q 2>&1".to_string(),
            ],
            risks: "wrong repo layout".to_string(),
            assumptions: "pygame is available".to_string(),
        };
        let verify_command = "SDL_VIDEODRIVER=dummy python3 -m unittest -q 2>&1";

        let (_, _, evidence) = rescue_invalid_done_payload_for_verified_action(
            Some(&plan),
            &[
                "write_file(path=maze_game_pygame/game.py)".to_string(),
                "write_file(path=maze_game_pygame/main.py)".to_string(),
                "write_file(path=maze_game_pygame/test_game.py)".to_string(),
                verify_command.to_string(),
            ],
            &ObservationEvidence::default(),
            VerificationLevel::Behavioral,
            Some(verify_command),
            Some(7),
            Some(7),
        )
        .expect("rescued done payload");

        assert_eq!(evidence.len(), 3);
        assert_eq!(
            evidence[0].command,
            "write_file(path=maze_game_pygame/game.py)"
        );
        assert_eq!(
            evidence[1].command,
            "write_file(path=maze_game_pygame/main.py)"
        );
        assert_eq!(evidence[2].command, verify_command);
    }

    #[test]
    fn canonicalize_known_acceptance_commands_preserves_long_exec_verification_command() {
        let verify_command = "test -d maze_game_pygame/.git && test -f maze_game_pygame/README.md && test -f maze_game_pygame/game.py && test -f maze_game_pygame/main.py && test -f maze_game_pygame/test_game.py && cd maze_game_pygame && SDL_VIDEODRIVER=dummy python3 -m unittest -q 2>&1";

        let commands = canonicalize_known_acceptance_commands(
            &[verify_command.to_string()],
            &ObservationEvidence::default(),
        );

        assert_eq!(commands, vec![verify_command.to_string()]);
    }

    #[test]
    fn preferred_action_verification_command_prefers_configured_case_preserving_test_cmd() {
        let configured = "test -d maze_game_pygame/.git && test -f maze_game_pygame/README.md && test -f maze_game_pygame/game.py && test -f maze_game_pygame/main.py && test -f maze_game_pygame/test_game.py && cd maze_game_pygame && SDL_VIDEODRIVER=dummy python3 -m unittest -q 2>&1";
        let lowercased = "test -d maze_game_pygame/.git && test -f maze_game_pygame/readme.md && test -f maze_game_pygame/game.py && test -f maze_game_pygame/main.py && test -f maze_game_pygame/test_game.py && cd maze_game_pygame && sdl_videodriver=dummy python3 -m unittest -q 2>&1";

        let chosen = preferred_action_verification_command(
            &[lowercased.to_string()],
            VerificationLevel::Behavioral,
            Some(configured),
        )
        .expect("verification command");

        assert_eq!(chosen, configured);
    }

    #[test]
    fn latest_assistant_message_is_done_ignores_prior_done_when_newer_message_exists() {
        let messages = vec![
            json!({
                "role": "assistant",
                "content": "[DONE]\nCreated `demo_repo/.gitignore` and verified it."
            }),
            json!({
                "role": "assistant",
                "content": "Still working through one more verification step."
            }),
        ];

        assert!(!latest_assistant_message_is_done(&messages));

        let mut completed = messages;
        completed.push(json!({
            "role": "assistant",
            "content": "[DONE]\nCreated `demo_repo/README.md` and verified it."
        }));

        assert!(latest_assistant_message_is_done(&completed));
    }
}
