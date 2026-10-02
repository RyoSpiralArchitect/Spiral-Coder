use anyhow::{anyhow, Result};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Deserialize)]
struct RepoMapQueryHit {
    rank: usize,
    score: f64,
    confidence: f64,
    confidence_label: String,
    margin_to_next: f64,
    path: String,
    lang: String,
    symbols: Vec<RepoMapSymbol>,
    #[serde(default)]
    explain: Option<RepoMapExplain>,
}

#[derive(Debug, Clone, Deserialize)]
struct RepoMapSymbol {
    qualname: String,
}

#[derive(Debug, Clone, Deserialize)]
struct RepoMapExplain {
    #[serde(default)]
    feature_scores: BTreeMap<String, f64>,
    #[serde(default)]
    covered_terms: Vec<String>,
    #[serde(default)]
    missed_terms: Vec<String>,
    #[serde(default)]
    symbol_hits: Vec<String>,
    #[serde(default)]
    lexeme_hits: Vec<String>,
    #[serde(default)]
    path_hits: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct RepoMapFallback {
    pub content: String,
    pub top_path: Option<String>,
    pub top_confidence: Option<f64>,
    pub top_dir: Option<String>,
    pub top_path_reasons: Vec<String>,
    pub typo_likely: bool,
}

fn normalize_pathish_query(query: &str) -> String {
    query
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
        .filter(|part| !part.chars().all(|ch| ch.is_ascii_digit()))
        .collect::<Vec<_>>()
        .join(" ")
}

fn pathish_tokens(query: &str) -> Vec<String> {
    normalize_pathish_query(query)
        .split_whitespace()
        .map(|part| part.to_ascii_lowercase())
        .collect()
}

fn normalize_slash_path(path: &str) -> String {
    path.replace('\\', "/")
        .trim_matches('/')
        .to_ascii_lowercase()
}

fn path_components_lower(path: &str) -> Vec<String> {
    normalize_slash_path(path)
        .split('/')
        .filter(|part| !part.is_empty())
        .map(|part| part.to_string())
        .collect()
}

fn basename_lower(path: &str) -> Option<String> {
    Path::new(path)
        .file_name()
        .map(|value| value.to_string_lossy().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
}

fn stem_lower(path: &str) -> Option<String> {
    Path::new(path)
        .file_stem()
        .map(|value| value.to_string_lossy().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
}

fn parent_dir_from_path(path: &str) -> Option<String> {
    let parent = Path::new(path).parent()?;
    let value = parent.to_string_lossy().replace('\\', "/");
    let trimmed = value.trim_matches('/').trim();
    if trimmed.is_empty() || trimmed == "." {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn shared_suffix_components(left: &[String], right: &[String]) -> usize {
    let mut count = 0;
    let mut lhs = left.iter().rev();
    let mut rhs = right.iter().rev();
    loop {
        match (lhs.next(), rhs.next()) {
            (Some(a), Some(b)) if a == b => count += 1,
            _ => break,
        }
    }
    count
}

fn common_prefix_chars(left: &str, right: &str) -> usize {
    left.chars()
        .zip(right.chars())
        .take_while(|(a, b)| a == b)
        .count()
}

fn levenshtein_distance_limited(left: &str, right: &str, max_distance: usize) -> Option<usize> {
    if left == right {
        return Some(0);
    }
    if left.is_empty() {
        return (right.chars().count() <= max_distance).then_some(right.chars().count());
    }
    if right.is_empty() {
        return (left.chars().count() <= max_distance).then_some(left.chars().count());
    }

    let left_chars: Vec<char> = left.chars().collect();
    let right_chars: Vec<char> = right.chars().collect();
    if left_chars.len().abs_diff(right_chars.len()) > max_distance {
        return None;
    }

    let mut prev: Vec<usize> = (0..=right_chars.len()).collect();
    let mut curr: Vec<usize> = vec![0; right_chars.len() + 1];

    for (i, left_ch) in left_chars.iter().enumerate() {
        curr[0] = i + 1;
        let mut row_min = curr[0];
        for (j, right_ch) in right_chars.iter().enumerate() {
            let cost = usize::from(left_ch != right_ch);
            curr[j + 1] = (prev[j + 1] + 1).min(curr[j] + 1).min(prev[j] + cost);
            row_min = row_min.min(curr[j + 1]);
        }
        if row_min > max_distance {
            return None;
        }
        std::mem::swap(&mut prev, &mut curr);
    }

    (prev[right_chars.len()] <= max_distance).then_some(prev[right_chars.len()])
}

fn candidate_dirs(hits: &[RepoMapQueryHit], limit: usize) -> Vec<String> {
    let mut dirs = Vec::new();
    for hit in hits {
        if let Some(dir) = parent_dir_from_path(&hit.path) {
            if !dirs.iter().any(|existing| existing == &dir) {
                dirs.push(dir);
                if dirs.len() >= limit {
                    break;
                }
            }
        }
    }
    dirs
}

fn path_typo_likely(reasons: &[String]) -> bool {
    let has_edit = reasons
        .iter()
        .any(|reason| reason.starts_with("basename_edit=") || reason.starts_with("stem_edit="));
    let has_parent_context = reasons
        .iter()
        .any(|reason| reason.starts_with("shared_parent="));
    let has_name_affinity = reasons.iter().any(|reason| {
        reason.starts_with("basename_prefix=")
            || reason == "basename_contains"
            || reason.starts_with("shared_suffix=")
    });
    has_edit || (has_parent_context && has_name_affinity)
}

#[derive(Debug, Clone)]
struct RepoMapScoredHit {
    hit: RepoMapQueryHit,
    total_score: f64,
    path_bonus: f64,
    path_reasons: Vec<String>,
}

fn path_similarity_bonus(requested: &str, candidate: &str) -> (f64, Vec<String>) {
    let requested_components = path_components_lower(requested);
    let candidate_components = path_components_lower(candidate);
    let requested_terms = pathish_tokens(requested);
    let candidate_terms = pathish_tokens(candidate);
    let requested_basename = basename_lower(requested);
    let candidate_basename = basename_lower(candidate);
    let requested_stem = stem_lower(requested);
    let candidate_stem = stem_lower(candidate);

    let mut bonus = 0.0;
    let mut reasons = Vec::new();

    let suffix = shared_suffix_components(&requested_components, &candidate_components);
    if suffix > 0 {
        bonus += suffix as f64 * 18.0;
        reasons.push(format!("shared_suffix={suffix}"));
    }

    if requested_components.len() > 1 && candidate_components.len() > 1 {
        let requested_parent = &requested_components[..requested_components.len() - 1];
        let candidate_parent = &candidate_components[..candidate_components.len() - 1];
        let shared_parent = shared_suffix_components(requested_parent, candidate_parent);
        if shared_parent > 0 {
            bonus += shared_parent as f64 * 10.0;
            reasons.push(format!("shared_parent={shared_parent}"));
        }
    }

    let overlap = requested_terms
        .iter()
        .filter(|term| candidate_terms.iter().any(|candidate| candidate == *term))
        .count();
    if overlap > 0 {
        bonus += overlap as f64 * 6.0;
        reasons.push(format!("term_overlap={overlap}"));
    }

    if let (Some(req_base), Some(cand_base)) =
        (requested_basename.as_deref(), candidate_basename.as_deref())
    {
        if req_base == cand_base {
            bonus += 40.0;
            reasons.push("basename_exact".to_string());
        } else {
            if req_base.contains(cand_base) || cand_base.contains(req_base) {
                bonus += 12.0;
                reasons.push("basename_contains".to_string());
            }
            let prefix = common_prefix_chars(req_base, cand_base);
            if prefix >= 3 {
                bonus += (prefix.min(10) as f64) * 1.5;
                reasons.push(format!("basename_prefix={prefix}"));
            }
            if let Some(distance) = levenshtein_distance_limited(req_base, cand_base, 3) {
                if distance > 0 {
                    bonus += match distance {
                        1 => 18.0,
                        2 => 10.0,
                        3 => 4.0,
                        _ => 0.0,
                    };
                    reasons.push(format!("basename_edit={distance}"));
                }
            }
        }
    }

    if let (Some(req_stem), Some(cand_stem)) =
        (requested_stem.as_deref(), candidate_stem.as_deref())
    {
        if req_stem == cand_stem {
            bonus += 28.0;
            reasons.push("stem_exact".to_string());
        } else if let Some(distance) = levenshtein_distance_limited(req_stem, cand_stem, 2) {
            if distance > 0 {
                bonus += match distance {
                    1 => 12.0,
                    2 => 6.0,
                    _ => 0.0,
                };
                reasons.push(format!("stem_edit={distance}"));
            }
        }
    }

    (bonus, reasons)
}

fn query_variants_for_path(pathish: &str) -> Vec<String> {
    let mut variants = Vec::new();
    let raw = pathish.trim();
    let normalized = normalize_pathish_query(pathish);
    let components = path_components_lower(pathish);

    for candidate in [
        Some(raw.to_string()),
        (!normalized.is_empty()).then_some(normalized.clone()),
        basename_lower(pathish),
        stem_lower(pathish),
        (components.len() >= 2).then_some(components[components.len() - 2..].join(" ")),
    ]
    .into_iter()
    .flatten()
    {
        let trimmed = candidate.trim().to_string();
        if trimmed.chars().count() >= 2 && !variants.iter().any(|existing| existing == &trimmed) {
            variants.push(trimmed);
        }
    }

    variants
}

fn merge_repo_map_hits(
    root: &str,
    queries: &[String],
    top_k: usize,
) -> Result<Vec<RepoMapQueryHit>> {
    let mut merged: BTreeMap<String, RepoMapQueryHit> = BTreeMap::new();
    for query in queries {
        let hits = query_repo_map(root, query, top_k)?;
        for hit in hits {
            match merged.get(&hit.path) {
                Some(existing)
                    if existing.confidence > hit.confidence
                        || (existing.confidence == hit.confidence
                            && existing.score >= hit.score) => {}
                _ => {
                    merged.insert(hit.path.clone(), hit);
                }
            }
        }
    }
    Ok(merged.into_values().collect())
}

fn rank_path_hits(requested: &str, hits: Vec<RepoMapQueryHit>) -> Vec<RepoMapScoredHit> {
    let mut ranked = hits
        .into_iter()
        .map(|hit| {
            let (path_bonus, path_reasons) = path_similarity_bonus(requested, &hit.path);
            let total_score = hit.score + (hit.confidence * 20.0) + path_bonus;
            RepoMapScoredHit {
                hit,
                total_score,
                path_bonus,
                path_reasons,
            }
        })
        .collect::<Vec<_>>();

    ranked.sort_by(|left, right| {
        right
            .total_score
            .partial_cmp(&left.total_score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                right
                    .hit
                    .confidence
                    .partial_cmp(&left.hit.confidence)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    });
    ranked
}

fn lazy_query_fallback(
    root: &str,
    display_query: &str,
    lookup_query: &str,
    min_confidence: f64,
    include_dirs: bool,
    show_preview: bool,
) -> Option<RepoMapFallback> {
    if !repo_map_ready(root) {
        return None;
    }
    if lookup_query.trim().chars().count() < 2 {
        return None;
    }

    let hits = query_repo_map(root, lookup_query, 3).ok()?;
    if hits.is_empty() {
        return None;
    }
    if hits.first().map(|h| h.confidence).unwrap_or(0.0) < min_confidence {
        return None;
    }

    let mut out = String::new();
    if display_query.trim() == lookup_query.trim() {
        out.push_str(&format!(
            "[repo_map fallback: '{}' — {} candidate(s)]\n",
            display_query,
            hits.len()
        ));
    } else {
        out.push_str(&format!(
            "[repo_map fallback: '{}' -> '{}' — {} candidate(s)]\n",
            display_query,
            lookup_query,
            hits.len()
        ));
    }

    for hit in hits.iter().take(3) {
        out.push_str(&format!(
            "{}. {} [{}] score={:.1} conf={:.2} ({}) margin={:.1}\n",
            hit.rank,
            hit.path,
            hit.lang,
            hit.score,
            hit.confidence,
            hit.confidence_label,
            hit.margin_to_next
        ));
        if let Some(ref explain) = hit.explain {
            let features = summarize_feature_scores(&explain.feature_scores, 3);
            if !features.is_empty() {
                out.push_str(&format!("   explain: {features}\n"));
            }
            if !explain.covered_terms.is_empty() {
                out.push_str(&format!(
                    "   covered: {}\n",
                    explain
                        .covered_terms
                        .iter()
                        .take(6)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            if !explain.path_hits.is_empty() {
                out.push_str(&format!(
                    "   path_hits: {}\n",
                    explain
                        .path_hits
                        .iter()
                        .take(6)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            if !explain.lexeme_hits.is_empty() {
                out.push_str(&format!(
                    "   lexeme_hits: {}\n",
                    explain
                        .lexeme_hits
                        .iter()
                        .take(6)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            if !explain.symbol_hits.is_empty() {
                out.push_str(&format!(
                    "   symbol_hits: {}\n",
                    explain
                        .symbol_hits
                        .iter()
                        .take(4)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            if !explain.missed_terms.is_empty() {
                out.push_str(&format!(
                    "   missed: {}\n",
                    explain
                        .missed_terms
                        .iter()
                        .take(6)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
        }
    }

    let dirs = if include_dirs {
        candidate_dirs(&hits, 3)
    } else {
        Vec::new()
    };
    if !dirs.is_empty() {
        out.push_str("\n[candidate dirs]\n");
        for (idx, dir) in dirs.iter().enumerate() {
            out.push_str(&format!("{}. {}\n", idx + 1, dir));
        }
    }

    if show_preview {
        if let Some(top) = hits.first() {
            if top.confidence >= 0.75 {
                if let Some(symbol) = top.symbols.first() {
                    if let Ok(preview) = show_repo_map_symbol(root, &top.path, &symbol.qualname) {
                        out.push_str("\n[repo_map preview]\n");
                        out.push_str(&truncate_chars(&preview, 1200));
                        out.push('\n');
                    }
                }
            }
        }
    }

    Some(RepoMapFallback {
        content: out.trim_end().to_string(),
        top_path: hits.first().map(|h| h.path.clone()),
        top_confidence: hits.first().map(|h| h.confidence),
        top_dir: dirs
            .first()
            .cloned()
            .or_else(|| hits.first().and_then(|h| parent_dir_from_path(&h.path))),
        top_path_reasons: Vec::new(),
        typo_likely: false,
    })
}

fn lazy_path_fallback(
    root: &str,
    display_query: &str,
    min_confidence: f64,
    include_dirs: bool,
    show_preview: bool,
) -> Option<RepoMapFallback> {
    if !repo_map_ready(root) {
        return None;
    }

    let variants = query_variants_for_path(display_query);
    if variants.is_empty() {
        return None;
    }

    let ranked = rank_path_hits(display_query, merge_repo_map_hits(root, &variants, 6).ok()?)
        .into_iter()
        .take(3)
        .collect::<Vec<_>>();
    if ranked.is_empty() {
        return None;
    }
    if ranked
        .first()
        .map(|entry| entry.hit.confidence)
        .unwrap_or(0.0)
        < min_confidence
    {
        return None;
    }

    let mut out = String::new();
    out.push_str(&format!(
        "[repo_map fallback: '{}' — fuzzy path candidates]\n",
        display_query
    ));
    out.push_str(&format!("   query_terms: {}\n", variants.join(" | ")));

    for (idx, scored) in ranked.iter().enumerate() {
        let hit = &scored.hit;
        out.push_str(&format!(
            "{}. {} [{}] score={:.1} conf={:.2} ({}) margin={:.1} path_bias={:.1}\n",
            idx + 1,
            hit.path,
            hit.lang,
            hit.score,
            hit.confidence,
            hit.confidence_label,
            hit.margin_to_next,
            scored.path_bonus
        ));
        if !scored.path_reasons.is_empty() {
            out.push_str(&format!(
                "   path_bias: {}\n",
                scored.path_reasons.join(", ")
            ));
        }
        if let Some(ref explain) = hit.explain {
            let features = summarize_feature_scores(&explain.feature_scores, 3);
            if !features.is_empty() {
                out.push_str(&format!("   explain: {features}\n"));
            }
            if !explain.covered_terms.is_empty() {
                out.push_str(&format!(
                    "   covered: {}\n",
                    explain
                        .covered_terms
                        .iter()
                        .take(6)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            if !explain.path_hits.is_empty() {
                out.push_str(&format!(
                    "   path_hits: {}\n",
                    explain
                        .path_hits
                        .iter()
                        .take(6)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
        }
    }

    let top_scored = &ranked[0];
    let top_hit = &top_scored.hit;
    let dirs = if include_dirs {
        let top_hits = ranked
            .iter()
            .map(|entry| entry.hit.clone())
            .collect::<Vec<_>>();
        candidate_dirs(&top_hits, 3)
    } else {
        Vec::new()
    };
    if !dirs.is_empty() {
        out.push_str("\n[candidate dirs]\n");
        for (idx, dir) in dirs.iter().enumerate() {
            out.push_str(&format!("{}. {}\n", idx + 1, dir));
        }
    }

    if show_preview && top_hit.confidence >= 0.75 {
        if let Some(symbol) = top_hit.symbols.first() {
            if let Ok(preview) = show_repo_map_symbol(root, &top_hit.path, &symbol.qualname) {
                out.push_str("\n[repo_map preview]\n");
                out.push_str(&truncate_chars(&preview, 1200));
                out.push('\n');
            }
        }
    }

    Some(RepoMapFallback {
        content: out.trim_end().to_string(),
        top_path: Some(top_hit.path.clone()),
        top_confidence: Some(top_hit.confidence),
        top_dir: dirs
            .first()
            .cloned()
            .or_else(|| parent_dir_from_path(&top_hit.path)),
        top_path_reasons: top_scored.path_reasons.clone(),
        typo_likely: path_typo_likely(&top_scored.path_reasons),
    })
}

fn repo_map_script(root: &str) -> PathBuf {
    Path::new(root).join("scripts").join("repo_map.py")
}

fn repo_map_index(root: &str) -> PathBuf {
    Path::new(root).join(".spiral").join("repo_map.json")
}

pub fn repo_map_ready(root: &str) -> bool {
    repo_map_script(root).is_file() && repo_map_index(root).is_file()
}

fn python_candidates() -> Vec<String> {
    let mut out = Vec::new();
    if let Ok(cmd) = std::env::var("SPIRAL_CODER_REPO_MAP_PYTHON") {
        let trimmed = cmd.trim();
        if !trimmed.is_empty() {
            out.push(trimmed.to_string());
        }
    }
    if cfg!(target_os = "windows") {
        out.push("python".to_string());
        out.push("py".to_string());
    } else {
        out.push("python3".to_string());
        out.push("python".to_string());
    }
    out.dedup();
    out
}

fn run_repo_map_command(root: &str, args: &[&str]) -> Result<String> {
    let script = repo_map_script(root);
    if !script.is_file() {
        return Err(anyhow!("repo_map.py not found under tool_root"));
    }

    let mut last_err: Option<anyhow::Error> = None;
    for py in python_candidates() {
        let output = Command::new(&py)
            .arg(&script)
            .args(args)
            .current_dir(root)
            .output();
        match output {
            Ok(out) if out.status.success() => {
                let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
                return Ok(stdout);
            }
            Ok(out) => {
                let stderr = String::from_utf8_lossy(&out.stderr);
                last_err = Some(anyhow!(
                    "repo_map command failed via {} (exit {:?}): {}",
                    py,
                    out.status.code(),
                    stderr.trim()
                ));
            }
            Err(err) => {
                last_err = Some(anyhow!("failed to spawn {}: {}", py, err));
            }
        }
    }

    Err(last_err.unwrap_or_else(|| anyhow!("failed to run repo_map command")))
}

fn parse_embedded_json<T: DeserializeOwned>(stdout: &str) -> Result<T> {
    let mut starts: Vec<usize> = stdout
        .char_indices()
        .filter_map(|(idx, ch)| (ch == '{' || ch == '[').then_some(idx))
        .collect();
    starts.reverse();
    for idx in starts {
        let candidate = stdout[idx..].trim();
        if let Ok(parsed) = serde_json::from_str::<T>(candidate) {
            return Ok(parsed);
        }
    }
    Err(anyhow!("failed to parse repo_map JSON output"))
}

fn strip_show_preamble(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    for (idx, line) in lines.iter().enumerate() {
        if line.starts_with("# ") || line.starts_with("1 |") {
            return lines[idx..].join("\n");
        }
    }
    text.trim().to_string()
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max_chars).collect();
    out.push_str("\n[…truncated]");
    out
}

fn summarize_feature_scores(scores: &BTreeMap<String, f64>, limit: usize) -> String {
    let mut pairs: Vec<(&String, &f64)> = scores.iter().filter(|(_, v)| **v > 0.0).collect();
    pairs.sort_by(|a, b| b.1.partial_cmp(a.1).unwrap_or(std::cmp::Ordering::Equal));
    pairs
        .into_iter()
        .take(limit)
        .map(|(name, value)| format!("{}={:.1}", name, value))
        .collect::<Vec<_>>()
        .join(", ")
}

fn query_repo_map(root: &str, query: &str, top_k: usize) -> Result<Vec<RepoMapQueryHit>> {
    let stdout = run_repo_map_command(
        root,
        &[
            "query",
            query,
            "--root",
            root,
            "--top-k",
            &top_k.to_string(),
            "--json",
            "--explain",
        ],
    )?;
    parse_embedded_json::<Vec<RepoMapQueryHit>>(&stdout)
}

fn show_repo_map_symbol(root: &str, path: &str, symbol: &str) -> Result<String> {
    let stdout = run_repo_map_command(
        root,
        &[
            "show",
            "--root",
            root,
            "--file",
            path,
            "--symbol",
            symbol,
            "--context",
            "1",
        ],
    )?;
    Ok(strip_show_preamble(&stdout))
}

pub fn lazy_search_fallback(root: &str, query: &str) -> Option<RepoMapFallback> {
    lazy_query_fallback(root, query, query, 0.0, false, true)
}

pub fn lazy_read_fallback(root: &str, query: &str) -> Option<RepoMapFallback> {
    lazy_path_fallback(root, query, 0.45, false, true)
}

pub fn lazy_glob_fallback(root: &str, pattern: &str) -> Option<RepoMapFallback> {
    lazy_path_fallback(root, pattern, 0.20, true, false)
}

pub fn lazy_list_dir_fallback(root: &str, path: &str) -> Option<RepoMapFallback> {
    lazy_path_fallback(root, path, 0.20, true, false)
}

#[cfg(test)]
mod tests {
    use super::{
        levenshtein_distance_limited, normalize_pathish_query, parent_dir_from_path,
        parse_embedded_json, path_similarity_bonus, path_typo_likely, query_variants_for_path,
        strip_show_preamble,
    };

    #[test]
    fn parse_embedded_json_skips_banner_noise() {
        let raw = "[Spiralton] banner\n[1, 2, 3]\n";
        let parsed: Vec<u32> = parse_embedded_json(raw).expect("parse_embedded_json");
        assert_eq!(parsed, vec![1, 2, 3]);
    }

    #[test]
    fn strip_show_preamble_keeps_symbol_block() {
        let raw = "[Spiralton] banner\n# src/project.rs\n# symbol: ProjectContext [struct] 1:10\n1 | line\n";
        let stripped = strip_show_preamble(raw);
        assert!(stripped.starts_with("# src/project.rs"));
        assert!(!stripped.contains("[Spiralton]"));
    }

    #[test]
    fn normalize_pathish_query_drops_glob_noise() {
        assert_eq!(normalize_pathish_query("src/**/agent*.rs"), "src agent rs");
    }

    #[test]
    fn parent_dir_from_path_extracts_forward_slash_path() {
        assert_eq!(
            parent_dir_from_path("src/tui/agent.rs").as_deref(),
            Some("src/tui")
        );
    }

    #[test]
    fn query_variants_for_path_emits_basename_and_suffix_terms() {
        let variants = query_variants_for_path("src/tui/agnt.rs");
        assert!(variants.iter().any(|value| value == "src tui agnt rs"));
        assert!(variants.iter().any(|value| value == "agnt.rs"));
        assert!(variants.iter().any(|value| value == "agnt"));
        assert!(variants.iter().any(|value| value == "tui agnt.rs"));
    }

    #[test]
    fn levenshtein_distance_limited_handles_small_typos() {
        assert_eq!(levenshtein_distance_limited("agent", "agnt", 2), Some(1));
        assert_eq!(levenshtein_distance_limited("agent", "observer", 2), None);
    }

    #[test]
    fn path_similarity_bonus_prefers_close_basename_typos() {
        let (bonus, reasons) = path_similarity_bonus("src/tui/agnt.rs", "src/tui/agent.rs");
        assert!(bonus > 20.0);
        assert!(reasons
            .iter()
            .any(|reason| reason.contains("shared_parent")));
        assert!(reasons.iter().any(|reason| reason.contains("stem_edit")));
    }

    #[test]
    fn path_typo_likely_flags_edit_distance_cases() {
        assert!(path_typo_likely(&[
            "shared_parent=2".to_string(),
            "stem_edit=1".to_string(),
        ]));
        assert!(!path_typo_likely(&["term_overlap=2".to_string()]));
    }
}
