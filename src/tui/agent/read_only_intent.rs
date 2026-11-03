//! Scope "read-only" to the subject of an explicit artifact edit.
use super::super::{explicit_edit_scope::Scope, requirement_proof};

pub(super) fn directs_scoped_edit(root: &str) -> bool {
    let root = root.trim_start();
    let line = root.lines().next().unwrap_or("");
    let opening = line.split_once(". ").map_or(line, |(sentence, _)| sentence);
    let lower = opening.to_ascii_lowercase();
    if !matches!(
        lower.split_whitespace().next(),
        Some("repair" | "change" | "update" | "revise" | "correct" | "edit")
    ) {
        return false;
    }
    // In "repair the guide for the read-only API in docs/guide.md", the
    // adjective belongs to the topic after "for", not the working mode.
    let Some((_, topic)) = lower
        .split_once(" for ")
        .or_else(|| lower.split_once(" about "))
    else {
        return false;
    };
    let topic = topic
        .split_once(" in ")
        .map_or(topic, |(subject, _)| subject);
    let full = root.to_ascii_lowercase();
    let mut descriptor_count = 0;
    for marker in ["read-only", "read only"] {
        for (start, _) in topic.match_indices(marker) {
            let owner = topic[..start].split_whitespace().last().unwrap_or("");
            let tail = &topic[start + marker.len()..];
            if !tail.starts_with(' ')
                || !tail.trim_start().starts_with(char::is_alphabetic)
                || !(matches!(owner, "" | "a" | "an" | "the")
                    || owner.ends_with("'s")
                    || owner.ends_with("’s"))
            {
                return false;
            }
            descriptor_count += 1;
        }
    }
    if descriptor_count == 0
        || descriptor_count != full.matches("read-only").count() + full.matches("read only").count()
    {
        return false;
    }
    if [
        "do not edit",
        "don't edit",
        "no edit",
        "never edit",
        "without editing",
        "do not modify",
        "do not change",
        "do not write",
        "only inspect",
        "inspection only",
        "inspection-only",
        "no modifications",
        "no changes",
    ]
    .iter()
    .any(|limit| full.contains(limit))
    {
        return false;
    }
    let (Some(scope), Ok(Some(contract))) = (Scope::parse(root), requirement_proof::parse(root))
    else {
        return false;
    };
    opening
        .split_whitespace()
        .map(|word| word.trim_matches(['`', '"', '\'', ',', '.', ':', ';', '(', ')']))
        .any(|path| contract.names_artifact(path) && scope.allows(path, None))
}

#[cfg(test)]
mod tests {
    use super::super::is_root_read_only_observation_task as read_only;
    const REAL: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/benchmarks/self-dogfood-v2/replay-catalog/guide/prompt.txt"
    ));

    #[test]
    fn a_read_only_subject_does_not_turn_an_authorized_guide_repair_into_inspection() {
        assert!(!read_only(REAL));
        assert!(!read_only(&REAL.replace(
            "Spiral-Coder's read-only replay catalog",
            "the read-only endpoint"
        )));
        for task in [
            REAL.replacen("Repair the published", "Inspect the published", 1),
            REAL.replacen("in docs/evaluation.md.", "in docs/other.md.", 1),
            REAL.replacen("Allowed paths only:", "Unscoped examples:", 1),
            REAL.replacen("<task_requirements>", "<sample_requirements>", 1),
            REAL.replacen(
                "Change only the guide.",
                "Read only. Do not edit anything.",
                1,
            ),
            REAL.replacen("Change only the guide.", "Work in read-only mode.", 1),
            REAL.replace(
                "for Spiral-Coder's read-only replay catalog",
                "in a read-only workspace",
            ),
        ] {
            assert!(read_only(&task), "{}", task.lines().next().unwrap());
        }
    }
}
