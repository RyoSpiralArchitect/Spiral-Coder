//! Preserve observed runner counts before display truncation. These are report
//! facts, never execution success, acceptance proof or a semantic code oracle.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Runner {
    Unittest,
    Libtest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Counts {
    runner: Runner,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tests_run: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    passed: Option<u64>,
    failure_events: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    errors: Option<u64>,
    skipped: u64,
}

fn tail(source: &str) -> &str {
    if source.len() <= 64 * 1024 {
        return source;
    }
    let mut start = source.len() - 64 * 1024;
    while !source.is_char_boundary(start) {
        start += 1;
    }
    source[start..]
        .split_once('\n')
        .map_or("", |(_, tail)| tail)
}

fn unittest_summary(run: &str, status: &str) -> Option<Counts> {
    let words: Vec<_> = run.split_whitespace().collect();
    if words.len() != 5
        || words[0] != "Ran"
        || !matches!(words[2], "test" | "tests")
        || words[3] != "in"
    {
        return None;
    }
    words[4].strip_suffix('s')?.parse::<f64>().ok()?;
    let tests_run = Some(words[1].parse().ok()?);
    let mut counts = Counts {
        runner: Runner::Unittest,
        tests_run,
        passed: None,
        failure_events: 0,
        errors: Some(0),
        skipped: 0,
    };
    if status == "OK" {
        return Some(counts);
    }
    let fields = status
        .strip_prefix("FAILED (")
        .or_else(|| status.strip_prefix("OK ("))?
        .strip_suffix(')')?;
    let mut seen = std::collections::BTreeSet::new();
    for field in fields.split(", ") {
        let (key, value) = field.split_once('=')?;
        if !seen.insert(key) {
            return None;
        }
        let value = value.parse().ok()?;
        match key {
            "failures" => counts.failure_events = value,
            "errors" => counts.errors = Some(value),
            "skipped" => counts.skipped = value,
            _ => return None, // expected/unexpected successes need distinct semantics
        }
    }
    let failed = counts.failure_events > 0 || counts.errors != Some(0);
    (failed == status.starts_with("FAILED (")).then_some(counts)
}

fn libtest(line: &str) -> Option<Counts> {
    let body = line
        .strip_prefix("test result: ok. ")
        .or_else(|| line.strip_prefix("test result: FAILED. "))?;
    let fields: Vec<_> = body.split("; ").collect();
    if fields.len() != 6 || !fields[5].starts_with("finished in ") {
        return None;
    }
    let number = |field: &str, suffix: &str| field.strip_suffix(suffix)?.parse::<u64>().ok();
    let passed = number(fields[0], " passed")?;
    let failed = number(fields[1], " failed")?;
    let skipped = number(fields[2], " ignored")?;
    number(fields[3], " measured")?;
    number(fields[4], " filtered out")?;
    if (failed == 0) != line.starts_with("test result: ok.") {
        return None;
    }
    Some(Counts {
        runner: Runner::Libtest,
        tests_run: None,
        passed: Some(passed),
        failure_events: failed,
        errors: None,
        skipped,
    })
}

fn parse(stdout: &str, stderr: &str) -> Option<Counts> {
    // A bounded excerpt cannot establish that earlier suites were absent.
    if stdout.len() > 64 * 1024 || stderr.len() > 64 * 1024 {
        return None;
    }
    let mut summaries = Vec::new();
    let mut candidates = 0;
    for stream in [stdout, stderr] {
        let lines: Vec<_> = tail(stream)
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect();
        for (index, line) in lines.iter().enumerate() {
            if line.starts_with("Ran ") {
                candidates += 1;
                if let Some(counts) = lines
                    .get(index + 1)
                    .and_then(|status| unittest_summary(line, status))
                {
                    summaries.push(counts);
                }
            } else if line.starts_with("test result: ") {
                candidates += 1;
                if let Some(counts) = libtest(line) {
                    summaries.push(counts);
                }
            }
        }
    }
    // Multiple suites, partial output and unknown formats stay unknown. Never
    // add counts, infer passed methods from subtest failures, or treat absent as 0.
    (candidates == 1 && summaries.len() == 1).then(|| summaries.remove(0))
}

pub(super) fn capture(mut message: Value, stdout: &str, stderr: &str) -> Value {
    if let Some(counts) = parse(stdout, stderr) {
        message["test_counts"] = serde_json::to_value(counts).expect("counts serialize");
    }
    message
}

pub(super) fn facts(root: &str, messages: &[Value], test_cmd: Option<&str>) -> Vec<Value> {
    let messages = crate::task_origin::current_task_messages(messages);
    super::report_outcomes::selected_checks(root, messages, test_cmd)
        .iter()
        .map(|check| {
            let counts = check
                .call_id
                .as_deref()
                .and_then(|id| {
                    messages
                        .iter()
                        .rev()
                        .find(|m| m["role"] == "tool" && m["tool_call_id"] == id)
                })
                .and_then(|message| {
                    serde_json::from_value::<Counts>(message["test_counts"].clone()).ok()
                });
            json!({"phase":check.phase,"command":check.command,"status":check.status,
            "call_id":check.call_id,"counts":counts})
        })
        .collect()
}

fn number(word: &str) -> Option<u64> {
    if word == "no" {
        return Some(0);
    }
    word.parse().ok().or_else(|| {
        [
            "zero",
            "one",
            "two",
            "three",
            "four",
            "five",
            "six",
            "seven",
            "eight",
            "nine",
            "ten",
            "eleven",
            "twelve",
            "thirteen",
            "fourteen",
            "fifteen",
            "sixteen",
            "seventeen",
            "eighteen",
            "nineteen",
            "twenty",
        ]
        .iter()
        .position(|candidate| *candidate == word)
        .map(|n| n as u64)
    })
}

/// Bounded English before/baseline and after/final count assertions. This does
/// not claim to understand arbitrary prose, code behavior or unlabelled numbers.
pub(super) fn issues(summary: &str, facts: &[Value]) -> Vec<String> {
    let mut segments = Vec::new();
    let (mut raw, mut prose) = (String::new(), String::new());
    let mut quoted = false;
    for c in summary.chars().chain(std::iter::once('\n')) {
        if c == '`' {
            quoted = !quoted;
        }
        if !quoted && matches!(c, '.' | '!' | '?' | ';' | '\n') {
            segments.push((std::mem::take(&mut raw), std::mem::take(&mut prose)));
        } else {
            raw.push(c);
            prose.push(if !quoted && c != '`' { c } else { ' ' });
        }
    }
    let mut issues = Vec::new();
    for (raw, sentence) in segments {
        let lower = sentence.to_ascii_lowercase();
        let words: Vec<_> = lower
            .split(|c: char| !c.is_ascii_alphanumeric())
            .filter(|w| !w.is_empty())
            .collect();
        let before = words.iter().any(|w| matches!(*w, "before" | "baseline"));
        let after = words.iter().any(|w| matches!(*w, "after" | "final"));
        if !before && !after {
            continue;
        }
        let phase = if before && after {
            "Before/After"
        } else if before {
            "Before"
        } else {
            "After"
        };
        for (i, word) in words.iter().enumerate() {
            let Some(value) = number(word) else {
                continue;
            };
            if i > 0 && matches!(words[i - 1], "not" | "example") {
                continue;
            }
            let field = match (words.get(i + 1).copied(), words.get(i + 2).copied()) {
                (Some("failures" | "failure"), _) => "failure_events",
                (Some("failed"), _) => "failed_tests",
                (Some("errors" | "error"), _) => "errors",
                (Some("skipped"), _) => "skipped",
                (Some("passed"), _) => "passed",
                (Some("tests" | "test"), Some("passed")) => "passed",
                (Some("tests" | "test"), Some("failed")) => "failed_tests",
                (Some("tests" | "test"), _) => "tests_run",
                _ => continue,
            };
            let mut selected: Vec<_> = facts.iter().filter(|fact| fact["phase"] == phase).collect();
            let cited: Vec<_> = selected
                .iter()
                .copied()
                .filter(|fact| {
                    fact["command"]
                        .as_str()
                        .is_some_and(|command| raw.contains(&format!("`{command}`")))
                })
                .collect();
            if !cited.is_empty() {
                selected = cited;
            }
            if selected.is_empty()
                || selected.iter().any(|fact| {
                    let observed = if field == "failed_tests" {
                        (fact["counts"]["runner"] == "libtest")
                            .then(|| fact["counts"]["failure_events"].as_u64())
                            .flatten()
                    } else {
                        fact["counts"][field].as_u64()
                    };
                    observed != Some(value)
                })
            {
                issues.push(format!("supported {phase} {field} count: the authored value {value} is not established by the selected native receipts. Use final_answer.check_facts, label the phase, or omit the count; do not infer counts from partial diagnostics or rerun a check merely to repair prose."));
            }
        }
    }
    issues
}
