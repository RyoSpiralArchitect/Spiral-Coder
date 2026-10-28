//! List structure must not consume numeric data inside protocol fields.
use super::compact_one_line;

fn parse_numbered_steps(s: &str) -> Vec<String> {
    let normalized = s.replace("\r\n", "\n");
    let bytes = normalized.as_bytes();
    let mut markers: Vec<(usize, usize)> = Vec::new();
    let mut i = 0usize;

    while i < bytes.len() {
        if bytes[i].is_ascii_digit() && (i == 0 || bytes[i - 1].is_ascii_whitespace()) {
            let marker_start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            // Numeric data such as `exit_code == 0).`, versions (`1.2`),
            // and dates are not ordinal list markers. Preserve compact `1)Read`.
            let positive = normalized[marker_start..i]
                .parse::<usize>()
                .is_ok_and(|number| number > 0);
            let begins_item = normalized
                .get(i + 1..)
                .and_then(|rest| rest.chars().next())
                .is_some_and(|ch| ch.is_whitespace() || ch.is_alphabetic());
            if positive
                && begins_item
                && i < bytes.len()
                && matches!(bytes[i], b')' | b'.' | b':' | b'-')
            {
                i += 1;
                while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                    i += 1;
                }
                markers.push((marker_start, i));
                continue;
            }
        }
        i += 1;
    }

    if markers.is_empty() {
        return Vec::new();
    }

    // An explicitly multiline list owns its line-start ordinals. Numeric
    // data inside a step, such as `(exit 1) recorded`, is not another item.
    // Keep the legacy inline format when there is no multiline structure.
    let starts_line = |start: usize| {
        normalized[..start]
            .rsplit('\n')
            .next()
            .unwrap_or("")
            .trim()
            .is_empty()
    };
    if markers
        .iter()
        .filter(|(start, _)| starts_line(*start))
        .count()
        >= 2
    {
        markers.retain(|(start, _)| starts_line(*start));
    }

    let mut steps = Vec::new();
    for (idx, (_, content_start)) in markers.iter().enumerate() {
        let content_end = markers
            .get(idx + 1)
            .map(|(next_start, _)| *next_start)
            .unwrap_or(normalized.len());
        let step = normalized[*content_start..content_end]
            .replace('\n', " ")
            .trim()
            .trim_end_matches(';')
            .trim()
            .to_string();
        if !step.is_empty() {
            steps.push(step);
        }
    }

    steps
}

pub(super) fn parse_plan_items(s: &str) -> Vec<String> {
    let bullet = |line: &str| {
        line.trim()
            .strip_prefix("- ")
            .or_else(|| line.trim().strip_prefix("* "))
            .or_else(|| line.trim().strip_prefix("• "))
            .map(str::to_owned)
    };
    if s.lines()
        .find(|line| !line.trim().is_empty())
        .is_some_and(|line| bullet(line).is_some())
    {
        let mut items = Vec::<String>::new();
        for line in s.lines().map(str::trim).filter(|line| !line.is_empty()) {
            if let Some(item) = bullet(line) {
                items.push(item);
            } else if let Some(last) = items.last_mut() {
                last.push(' ');
                last.push_str(line);
            } else {
                items.push(line.into());
            }
        }
        return items;
    }
    let numbered = parse_numbered_steps(s);
    if !numbered.is_empty() {
        return numbered;
    }

    s.replace("\r\n", "\n")
        .split(['\n', ';'])
        .map(|part| compact_one_line(part.trim(), 120))
        .filter(|part| part != "-")
        .collect()
}
