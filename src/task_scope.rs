//! Explicit artifact scope from the original human-approved benchmark plan.
//! Path mentions in an objective are subjects, not implicit edit targets.
use serde::Serialize;

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct ArtifactScope {
    pub objective: String,
    pub target_files: Vec<String>,
    pub referenced_subjects: Vec<String>,
}

pub(crate) fn artifact_scope(root: &str) -> Option<ArtifactScope> {
    let mut scope = ArtifactScope::default();
    let mut in_plan = false;
    let mut in_targets = false;
    let mut complete = false;
    for line in root.lines().map(str::trim) {
        if line.starts_with("<observer_benchmark_plan") {
            in_plan = true;
            continue;
        }
        if in_plan && line.starts_with("</observer_benchmark_plan>") {
            complete = true;
            break;
        }
        if !in_plan {
            continue;
        }
        if let Some(objective) = line.strip_prefix("objective:") {
            scope.objective = objective.trim().into();
        }
        if line == "target_files:" {
            in_targets = true;
        } else if in_targets {
            if let Some(path) = line.strip_prefix("- ") {
                let path = path.trim().trim_matches('`').trim_matches('"');
                if !path.is_empty() && !scope.target_files.iter().any(|p| p == path) {
                    scope.target_files.push(path.into());
                }
            } else if !line.is_empty() {
                in_targets = false;
            }
        }
    }
    if !complete || scope.target_files.is_empty() {
        return None;
    }
    for token in scope.objective.split_whitespace() {
        let path = token
            .trim_start_matches(['`', '"', '\'', '('])
            .trim_end_matches(['`', '"', '\'', ')', ',', ';', '.', ':', '!', '?']);
        if path.contains('/')
            && !path.contains("://")
            && !scope.target_files.iter().any(|p| p == path)
            && !scope.referenced_subjects.iter().any(|p| p == path)
        {
            scope.referenced_subjects.push(path.into());
        }
    }
    Some(scope)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_targets_outweigh_earlier_subjects_and_later_path_mentions() {
        let root = "<observer_benchmark_plan>\nobjective: Add coverage for `src/view.rs`.\ntarget_files:\n- .spiral-coder/tui_replay.json\nrequired_checks:\n- cat docs/reference.md\n</observer_benchmark_plan>\nExplain src/other.rs.";
        assert_eq!(
            artifact_scope(root),
            Some(ArtifactScope {
                objective: "Add coverage for `src/view.rs`.".into(),
                target_files: vec![".spiral-coder/tui_replay.json".into()],
                referenced_subjects: vec!["src/view.rs".into()],
            })
        );
    }

    #[test]
    fn incomplete_or_unstructured_mentions_do_not_create_artifact_scope() {
        assert!(artifact_scope("Edit src/view.rs").is_none());
        assert!(artifact_scope("<observer_benchmark_plan>\ntarget_files:\n- a.rs").is_none());
    }
}
