//! Bounded positive prose check directives; never infer commands from examples.

pub(super) fn commands(root: &str) -> Vec<String> {
    let mut commands = Vec::new();
    let mut fence = None;
    for line in root.lines().map(str::trim) {
        if line.starts_with("```") || line.starts_with("~~~") {
            let marker = &line[..3];
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
