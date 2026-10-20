//! Bounded positive prose check directives; never infer commands from examples.

use super::prose_context;

pub(super) fn commands(root: &str) -> Vec<String> {
    let mut commands = Vec::new();
    let mut fence = None;
    for line in root.lines().map(str::trim) {
        if prose_context::is_fence_line(line) {
            let marker = prose_context::fence_marker(line);
            if fence == Some(marker) {
                fence = None;
            } else if fence.is_none() {
                fence = Some(marker);
            }
            continue;
        }
        if fence.is_some() || line.starts_with('>') {
            continue;
        }
        let lower = line.to_ascii_lowercase();
        for (start, _) in lower.match_indices("run exactly:") {
            let prefix = lower[..start].trim();
            if !matches!(prefix, "" | "also" | "then" | "please")
                && !prefix.ends_with(['.', '!', '?'])
            {
                continue;
            }
            if prefix.ends_with(['.', '!', '?'])
                && preceding_sentence_is_example_or_negation(&lower, start)
            {
                continue;
            }
            let tail = line[start + "run exactly:".len()..].trim();
            let command = tail
                .strip_prefix('`')
                .and_then(|s| s.strip_suffix('`'))
                .unwrap_or(tail)
                .trim();
            // "Run exactly" may request a mutation. Only known verification
            // commands gain proof obligations; this does not grant exec approval
            // or promote arbitrary shell text into a nonmutating custom check.
            if super::classify_verify_level(command, None).is_some()
                && !commands.iter().any(|seen| seen == command)
            {
                commands.push(command.to_string());
            }
        }
    }
    commands
}

fn preceding_sentence_is_example_or_negation(lower: &str, start: usize) -> bool {
    let prefix = lower[..start].trim().trim_end_matches(&['.', '!', '?'][..]);
    let sentence_start = prefix.rfind(['.', '!', '?']).map(|i| i + 1).unwrap_or(0);
    let sentence = prefix[sentence_start..].trim_start();
    prose_context::is_example_or_negation_sentence(sentence)
}
