use super::*;

#[derive(Debug, Clone, Default)]
pub(super) struct ObservationSearchEvidence {
    pub(super) command: String,
    pub(super) pattern: String,
    pub(super) hit_count: usize,
    pub(super) paths: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub(super) struct ObservationReadEvidence {
    pub(super) command: String,
    pub(super) path: String,
}

#[derive(Debug, Clone, Default)]
pub(super) struct ObservationResolutionEvidence {
    pub(super) query: String,
    pub(super) canonical_path: String,
    pub(super) source: String,
}

#[derive(Debug, Clone, Default)]
pub(super) struct ObservationEvidence {
    pub(super) searches: Vec<ObservationSearchEvidence>,
    pub(super) reads: Vec<ObservationReadEvidence>,
    pub(super) resolutions: Vec<ObservationResolutionEvidence>,
}

impl ObservationEvidence {
    pub(super) fn remember_read(&mut self, command: &str, path: &str) {
        let command = compact_one_line(command.trim(), 200);
        let path = compact_one_line(path.trim(), 160);
        if command == "-" || path == "-" {
            return;
        }
        let sig = format!(
            "{}|{}",
            normalize_memory_entry(command.as_str()),
            normalize_memory_entry(path.as_str())
        );
        if sig.trim().is_empty() {
            return;
        }
        if let Some(pos) = self.reads.iter().position(|item| {
            format!(
                "{}|{}",
                normalize_memory_entry(item.command.as_str()),
                normalize_memory_entry(item.path.as_str())
            ) == sig
        }) {
            self.reads.remove(pos);
        }
        self.reads.push(ObservationReadEvidence { command, path });
        if self.reads.len() > 8 {
            self.reads.remove(0);
        }
    }

    pub(super) fn remember_resolution(&mut self, query: &str, canonical_path: &str, source: &str) {
        let query = compact_one_line(query.trim(), 180);
        let canonical_path = compact_one_line(canonical_path.trim(), 180);
        let source = compact_one_line(source.trim(), 80);
        if query == "-" || canonical_path == "-" || source == "-" {
            return;
        }
        let query_sig = normalize_path_alias(query.as_str());
        let canonical_sig = normalize_path_alias(canonical_path.as_str());
        if query_sig.is_empty() || canonical_sig.is_empty() {
            return;
        }
        if let Some(pos) = self.resolutions.iter().position(|item| {
            normalize_path_alias(item.query.as_str()) == query_sig
                || normalize_path_alias(item.canonical_path.as_str()) == canonical_sig
        }) {
            self.resolutions.remove(pos);
        }
        self.resolutions.push(ObservationResolutionEvidence {
            query,
            canonical_path,
            source,
        });
        if self.resolutions.len() > 12 {
            self.resolutions.remove(0);
        }
    }

    pub(super) fn remember_search(
        &mut self,
        command: &str,
        pattern: &str,
        hit_count: usize,
        paths: &[String],
    ) {
        let command = compact_one_line(command.trim(), 200);
        let pattern = compact_one_line(pattern.trim(), 120);
        if command == "-" || pattern == "-" {
            return;
        }
        let mut compact_paths = Vec::new();
        for path in paths.iter().take(8) {
            remember_recent_unique(&mut compact_paths, path.as_str(), 8, 160);
        }
        let sig = format!(
            "{}|{}",
            normalize_memory_entry(command.as_str()),
            normalize_memory_entry(pattern.as_str())
        );
        if let Some(pos) = self.searches.iter().position(|item| {
            format!(
                "{}|{}",
                normalize_memory_entry(item.command.as_str()),
                normalize_memory_entry(item.pattern.as_str())
            ) == sig
        }) {
            self.searches.remove(pos);
        }
        self.searches.push(ObservationSearchEvidence {
            command,
            pattern,
            hit_count,
            paths: compact_paths,
        });
        if self.searches.len() > 8 {
            self.searches.remove(0);
        }
    }

    pub(super) fn merge_session_cache(
        &mut self,
        cache: Option<&crate::agent_session::ObservationCache>,
    ) {
        let Some(cache) = cache else {
            return;
        };
        for read in &cache.reads {
            self.remember_read(read.command.as_str(), read.path.as_str());
        }
        for search in &cache.searches {
            self.remember_search(
                search.command.as_str(),
                search.pattern.as_str(),
                search.hit_count,
                search.paths.as_slice(),
            );
        }
        for resolution in &cache.resolutions {
            self.remember_resolution(
                resolution.query.as_str(),
                resolution.canonical_path.as_str(),
                resolution.source.as_str(),
            );
        }
    }

    pub(super) fn resolve_path_alias(&self, query: &str) -> Option<String> {
        let query_sig = normalize_path_alias(query);
        if query_sig.is_empty() {
            return None;
        }
        self.resolutions
            .iter()
            .rev()
            .find(|entry| path_alias_matches(query_sig.as_str(), entry.query.as_str()))
            .map(|entry| entry.canonical_path.clone())
    }

    pub(super) fn to_session_cache(&self) -> crate::agent_session::ObservationCache {
        crate::agent_session::ObservationCache {
            reads: self
                .reads
                .iter()
                .map(|read| crate::agent_session::ObservationReadCache {
                    command: read.command.clone(),
                    path: read.path.clone(),
                })
                .collect(),
            searches: self
                .searches
                .iter()
                .map(|search| crate::agent_session::ObservationSearchCache {
                    command: search.command.clone(),
                    pattern: search.pattern.clone(),
                    hit_count: search.hit_count,
                    paths: search.paths.clone(),
                })
                .collect(),
            resolutions: self
                .resolutions
                .iter()
                .map(
                    |resolution| crate::agent_session::ObservationResolutionCache {
                        query: resolution.query.clone(),
                        canonical_path: resolution.canonical_path.clone(),
                        source: resolution.source.clone(),
                    },
                )
                .collect(),
        }
    }
}

pub(super) fn remember_repo_map_resolution(
    evidence: &mut ObservationEvidence,
    query: &str,
    fallback: &crate::repo_map::RepoMapFallback,
    source: &str,
) {
    let canonical = fallback.top_path.as_deref().or(fallback.top_dir.as_deref());
    let Some(canonical) = canonical else {
        return;
    };
    evidence.remember_resolution(query, canonical, source);
}

pub(super) fn rewrite_tool_call_with_resolution(
    tc: &ToolCallData,
    evidence: &ObservationEvidence,
) -> Option<(ToolCallData, String, String)> {
    let key = match tc.name.as_str() {
        "read_file" | "write_file" | "patch_file" | "apply_diff" => "path",
        "list_dir" | "glob" | "search_files" => "dir",
        _ => return None,
    };
    let mut args = serde_json::from_str::<serde_json::Value>(&tc.arguments).ok()?;
    let original = args.get(key)?.as_str()?.trim().to_string();
    if original.is_empty() {
        return None;
    }
    let canonical = evidence.resolve_path_alias(original.as_str())?;
    if normalize_path_alias(original.as_str()) == normalize_path_alias(canonical.as_str()) {
        return None;
    }
    args[key] = serde_json::Value::String(canonical.clone());
    let arguments = serde_json::to_string(&args).ok()?;
    Some((
        ToolCallData {
            thought_signature: None,
            id: tc.id.clone(),
            name: tc.name.clone(),
            arguments,
        },
        original,
        canonical,
    ))
}

pub(super) fn normalize_memory_entry(s: &str) -> String {
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_ascii_lowercase()
}

pub(super) fn normalize_path_alias(s: &str) -> String {
    strip_matching_quotes(s.trim())
        .trim()
        .trim_start_matches("./")
        .trim_matches('/')
        .replace('\\', "/")
        .to_ascii_lowercase()
}

pub(super) fn path_alias_matches(query_sig: &str, candidate: &str) -> bool {
    let candidate_sig = normalize_path_alias(candidate);
    if query_sig.is_empty() || candidate_sig.is_empty() {
        return false;
    }
    query_sig == candidate_sig
        || candidate_sig.ends_with(format!("/{query_sig}").as_str())
        || query_sig.ends_with(format!("/{candidate_sig}").as_str())
}

fn resolution_arg_keys_for_command(name: &str) -> &'static [&'static str] {
    match name {
        "read_file" | "write_file" | "patch_file" | "apply_diff" => &["path"],
        "list_dir" => &["dir", "path"],
        "glob" | "search_files" => &["dir", "path"],
        _ => &[],
    }
}

pub(super) fn canonicalize_evidence_command_with_resolution(
    command: &str,
    evidence: &ObservationEvidence,
) -> String {
    let sig = canonicalize_evidence_command(command);
    if sig.is_empty() {
        return sig;
    }
    let Some((name, mut args)) = parse_named_command_signature(sig.as_str()) else {
        return sig;
    };
    let mut changed = false;
    for key in resolution_arg_keys_for_command(name.as_str()) {
        let Some(value) = args.get(*key).cloned() else {
            continue;
        };
        let Some(canonical) = evidence.resolve_path_alias(value.as_str()) else {
            continue;
        };
        if normalize_path_alias(value.as_str()) == normalize_path_alias(canonical.as_str()) {
            continue;
        }
        args.insert((*key).to_string(), canonical);
        changed = true;
    }
    if !changed {
        return sig;
    }
    let args_vec = args.into_iter().collect::<Vec<_>>();
    canonicalize_named_command(name.as_str(), &args_vec).unwrap_or(sig)
}

pub(super) fn remember_recent_unique(
    items: &mut Vec<String>,
    value: &str,
    max_items: usize,
    max_chars: usize,
) {
    let candidate = compact_one_line(value.trim(), max_chars);
    if candidate == "-" {
        return;
    }
    let sig = normalize_memory_entry(candidate.as_str());
    if sig.is_empty() {
        return;
    }
    if let Some(pos) = items
        .iter()
        .position(|existing| normalize_memory_entry(existing) == sig)
    {
        items.remove(pos);
    }
    items.push(candidate);
    if items.len() > max_items {
        let drop_n = items.len() - max_items;
        items.drain(0..drop_n);
    }
}
