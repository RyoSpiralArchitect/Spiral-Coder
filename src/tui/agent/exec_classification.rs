//! Conservative command recognition, not a shell parser or a sandbox.
//! Unsupported shell syntax is an action; command names never match substrings
//! inside arguments. Keep exact configured checks separate from display hashes.
use crate::governor_contract;

use super::{ExecKind, VerificationLevel};

/// Only simple words, quotes, assignments, `&&`, and `2>&1` are understood.
/// Reject expansion, pipelines, redirection, multiline scripts, and status masking.
fn commands(command: &str) -> Option<Vec<Vec<String>>> {
    if command.contains(['\n', '\r', '\0']) {
        return None;
    }
    let mut chars = command.char_indices().peekable();
    let mut chain: Vec<Vec<String>> = Vec::new();
    let mut words: Vec<String> = Vec::new();
    let mut word = String::new();
    let mut started = false;
    let mut quoted_before_equals = false;
    let mut quote = None;
    while let Some((index, ch)) = chars.next() {
        if quote == Some('\'') {
            if ch == '\'' {
                quote = None;
            } else {
                word.push(ch);
            }
            continue;
        }
        if quote == Some('"') {
            match ch {
                '"' => quote = None,
                '$' | '`' => return None,
                '\\' => {
                    let (_, escaped) = chars.next()?;
                    if !matches!(escaped, '\\' | '"') {
                        return None;
                    }
                    word.push(escaped);
                }
                _ => word.push(ch),
            }
            continue;
        }
        match ch {
            '\'' | '"' => {
                quoted_before_equals |= !word.contains('=');
                quote = Some(ch);
                started = true;
            }
            '\\' => {
                quoted_before_equals |= !word.contains('=');
                word.push(chars.next()?.1);
                started = true;
            }
            ' ' | '\t' => {
                if started {
                    if quoted_before_equals
                        && assignment(&word)
                        && words.iter().all(|word| assignment(word))
                    {
                        return None;
                    }
                    words.push(std::mem::take(&mut word));
                    started = false;
                    quoted_before_equals = false;
                }
            }
            '2' if !started && command[index..].starts_with("2>&1") => {
                let end = index + 4;
                if command[end..]
                    .chars()
                    .next()
                    .is_some_and(|next| !matches!(next, ' ' | '\t' | '&'))
                {
                    return None;
                }
                for _ in 0..3 {
                    chars.next();
                }
            }
            '&' => {
                if chars.next()?.1 != '&' {
                    return None;
                }
                if started {
                    if quoted_before_equals
                        && assignment(&word)
                        && words.iter().all(|word| assignment(word))
                    {
                        return None;
                    }
                    words.push(std::mem::take(&mut word));
                    started = false;
                    quoted_before_equals = false;
                }
                if words.is_empty() {
                    return None;
                }
                chain.push(std::mem::take(&mut words));
            }
            '$' | '`' | ';' | '|' | '<' | '>' | '(' | ')' | '{' | '}' | '#' => return None,
            _ => {
                word.push(ch);
                started = true;
            }
        }
    }
    if quote.is_some() {
        return None;
    }
    if started {
        if quoted_before_equals && assignment(&word) && words.iter().all(|word| assignment(word)) {
            return None;
        }
        words.push(word);
    }
    if words.is_empty() {
        return None;
    }
    chain.push(words);
    Some(chain)
}

fn assignment(word: &str) -> bool {
    let Some((name, _)) = word.split_once('=') else {
        return false;
    };
    !name.is_empty()
        && name
            .bytes()
            .enumerate()
            .all(|(i, ch)| ch == b'_' || ch.is_ascii_alphabetic() || (i > 0 && ch.is_ascii_digit()))
}

fn executable(words: &[String]) -> &[String] {
    let mut start = 0;
    while words.get(start).is_some_and(|word| assignment(word)) {
        start += 1;
    }
    if words.get(start).map(String::as_str) == Some("env") {
        start += 1;
        if words.get(start).map(String::as_str) == Some("--") {
            start += 1;
        }
        while words.get(start).is_some_and(|word| assignment(word)) {
            start += 1;
        }
    }
    &words[start..]
}

fn prefix(words: &[String], signature: &str) -> bool {
    let pattern: Vec<_> = signature.split_whitespace().collect();
    !pattern.is_empty()
        && words.len() >= pattern.len()
        && words
            .iter()
            .zip(pattern)
            .all(|(actual, expected)| actual == expected)
}

fn matches_policy(words: &[String], signatures: &[String]) -> bool {
    signatures.iter().any(|pattern| prefix(words, pattern))
}

fn forbidden_flags(words: &[String]) -> bool {
    words.iter().any(|word| {
        matches!(
            word.as_str(),
            "--fix"
                | "--fix-only"
                | "--write"
                | "--update-snapshots"
                | "--updateSnapshot"
                | "--help"
                | "-h"
                | "--version"
                | "-V"
                | "--list"
                | "--collect-only"
        ) || word.starts_with("--fix=")
            || word.starts_with("--output=")
    })
}

fn diagnostic(words: &[String]) -> bool {
    if words.is_empty() {
        return false;
    }
    if matches!(words[0].as_str(), "test" | "[") {
        return true;
    }
    if !matches_policy(
        words,
        governor_contract::instruction_resolver_diagnostic_exec_signatures(),
    ) {
        return false;
    }
    if prefix(words, "git branch") {
        let args = &words[2..];
        let list = args.iter().any(|arg| arg == "--list");
        return args.iter().all(|arg| {
            matches!(
                arg.as_str(),
                "--list"
                    | "--show-current"
                    | "-a"
                    | "-r"
                    | "-v"
                    | "-vv"
                    | "--all"
                    | "--remotes"
                    | "--verbose"
                    | "--no-color"
            ) || (list && !arg.starts_with('-'))
        });
    }
    if prefix(words, "git remote") {
        return words.len() == 2
            || (words.len() == 3 && matches!(words[2].as_str(), "-v" | "--verbose"));
    }
    if prefix(words, "git diff") {
        return !words[2..].iter().any(|arg| {
            matches!(arg.as_str(), "--output" | "--ext-diff" | "--textconv")
                || arg.starts_with("--output=")
        });
    }
    if words[0] == "sed" {
        // Recognize numeric print ranges only. sed's e/w commands, -i, -f and
        // arbitrary scripts can execute commands or write files even with -n.
        let mut args = words[1..].iter();
        if args.next().map(String::as_str) != Some("-n") {
            return false;
        }
        let Some(mut script) = args.next() else {
            return false;
        };
        if script == "-e" {
            let Some(next) = args.next() else {
                return false;
            };
            script = next;
        }
        let Some(range) = script.strip_suffix('p') else {
            return false;
        };
        return range
            .chars()
            .all(|ch| ch.is_ascii_digit() || matches!(ch, ',' | '$'))
            && args.all(|arg| !arg.starts_with('-'));
    }
    if words[0] == "rg"
        && words.iter().any(|arg| {
            matches!(arg.as_str(), "--pre" | "--hostname-bin")
                || arg.starts_with("--pre=")
                || arg.starts_with("--hostname-bin=")
        })
    {
        return false;
    }
    true
}

fn verification(words: &[String]) -> Option<VerificationLevel> {
    if words.is_empty() || forbidden_flags(words) {
        return None;
    }
    if words[0] == "git" && !diagnostic(words) {
        return None;
    }
    if prefix(words, "cargo nextest") && words.get(2).map(String::as_str) != Some("run") {
        return None;
    }
    if matches!(words[0].as_str(), "python" | "python3") {
        let mut i = 1;
        while words
            .get(i)
            .is_some_and(|arg| matches!(arg.as_str(), "-S" | "-I" | "-B" | "-u"))
        {
            i += 1;
        }
        if words.get(i).map(String::as_str) == Some("-m") {
            return match words.get(i + 1).map(String::as_str) {
                Some("pytest" | "unittest") => Some(VerificationLevel::Behavioral),
                Some("compileall") => Some(VerificationLevel::Build),
                _ => None,
            };
        }
    }
    let policy = governor_contract::verification();
    if matches_policy(words, &policy.ignore_command_signatures) {
        return None;
    }
    if matches_policy(words, &policy.behavioral_command_signatures) {
        return Some(
            if prefix(words, "cargo test") && words.iter().any(|arg| arg == "--no-run") {
                VerificationLevel::Build
            } else {
                VerificationLevel::Behavioral
            },
        );
    }
    matches_policy(words, &policy.build_command_signatures).then_some(VerificationLevel::Build)
}

pub(super) fn is_diagnostic(command: &str) -> bool {
    commands(command).is_some_and(|chain| chain.iter().all(|words| diagnostic(executable(words))))
}

pub(super) fn verify_level(command: &str, configured: Option<&str>) -> Option<VerificationLevel> {
    let chain = commands(command)?;
    let mut level = None;
    let mut unknown = 0;
    for words in &chain {
        let words = executable(words);
        if let Some(current) = verification(words) {
            level = Some(level.map_or(current, |prior: VerificationLevel| prior.max(current)));
        } else if !diagnostic(words) {
            unknown += 1;
        }
    }
    if unknown == 0 && level.is_some() {
        return level;
    }
    if !configured
        .is_some_and(|configured| !command.trim().is_empty() && command.trim() == configured.trim())
    {
        return None;
    }
    // Exact configured checks can combine custom scripts with known checks.
    // Authority applies only to this complete text, never to arbitrary prefixes.
    // Still reject unsupported syntax and known write semantics in every segment.
    for words in &chain {
        let words = executable(words);
        if words.is_empty()
            || forbidden_flags(words)
            || (chain.len() == 1
                && matches_policy(
                    words,
                    &governor_contract::verification().ignore_command_signatures,
                ))
            || matches!(
                words[0].as_str(),
                "rm" | "mv"
                    | "cp"
                    | "touch"
                    | "mkdir"
                    | "rmdir"
                    | "install"
                    | "tee"
                    | "truncate"
                    | "dd"
                    | "chmod"
                    | "chown"
                    | "ln"
                    | "unlink"
            )
            || (matches!(words[0].as_str(), "git" | "sed" | "rg") && !diagnostic(words))
        {
            return None;
        }
    }
    level.or(Some(VerificationLevel::Behavioral))
}

pub(super) fn configured_level(command: Option<&str>) -> Option<VerificationLevel> {
    let command = command?;
    verify_level(command, Some(command))
}

pub(super) fn classify(command: &str, configured: Option<&str>) -> ExecKind {
    if verify_level(command, configured).is_some() {
        ExecKind::Verify
    } else if is_diagnostic(command) {
        ExecKind::Diagnostic
    } else {
        ExecKind::Action
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mutations_cannot_hide_in_arguments_or_compound_scripts() {
        for command in [
            "printf 'x' > verification_receipt.txt",
            "printf '\\nupdated\\n' >> docs/runtime-architecture.md && printf stale > verification_receipt.txt",
            "echo 'cargo test' > file", "some-command cargo test",
            "cargo test && sed -i 's/old/new/' src/lib.rs", "git status && mutate",
            "cargo test\nprintf changed > file", "git status; rm file",
            "cargo test || true", "cargo test | cat", "cargo test $(touch file)",
            "cat `touch file`", "git branch -D feature", "git branch new-branch",
            "git remote add origin url", "sed -n '1w output' input", "sed -n '1,5p' -i input",
            "sed -n -f script input", "git diff --output patch", "rg --pre mutate pattern",
            "ruff check --fix", "eslint --fix src", "cargo test > results.log",
        ] {
            assert_eq!(classify(command, None), ExecKind::Action, "{command}");
            assert_eq!(verify_level(command, None), None, "{command}");
        }
    }

    #[test]
    fn known_read_only_and_verification_commands_remain_usable() {
        for command in [
            "pwd",
            "ls -la",
            "cat file",
            "rg 'a  b' src",
            "git status --short",
            "git branch --show-current",
            "git branch --list 'feature*'",
            "git remote -v",
            "sed -n '1,200p' src/lib.rs",
            "cd dir && git status",
            "env NAME=value cat file",
        ] {
            assert_eq!(classify(command, None), ExecKind::Diagnostic, "{command}");
        }
        for command in [
            "cargo test",
            "cargo test --lib 2>&1",
            "cd 'some dir' && ENV=x python3 -m unittest -q 2>&1",
            "env ENV='a  b' python3 -S -m pytest -q",
            "git status && cargo test",
        ] {
            assert_eq!(
                verify_level(command, None),
                Some(VerificationLevel::Behavioral),
                "{command}"
            );
        }
        assert_eq!(
            verify_level("cargo check && cargo test", None),
            Some(VerificationLevel::Behavioral)
        );
        assert_eq!(
            verify_level("cargo test --no-run", None),
            Some(VerificationLevel::Build)
        );
        assert_eq!(classify("echo 'cargo test'", None), ExecKind::Diagnostic);
        assert_eq!(verify_level("echo 'cargo test'", None), None);
    }

    #[test]
    fn matching_preserves_case_and_quoted_whitespace_without_truncation() {
        let configured = "python3 verify.py --expected 'A  B'";
        assert_eq!(
            verify_level(configured, Some(configured)),
            Some(VerificationLevel::Behavioral)
        );
        for altered in [
            "python3 verify.py --expected 'A B'",
            "python3 verify.py --expected 'a  b'",
            "python3 Verify.py --expected 'A  B'",
        ] {
            assert_eq!(verify_level(altered, Some(configured)), None);
        }
        for command in [
            "CARGO test",
            "'cargo test'",
            "'ENV=x' cargo test",
            "E'NV'=x cargo test",
            "catalog file",
            "git branch -d topic",
        ] {
            assert_eq!(classify(command, None), ExecKind::Action, "{command}");
        }
        let long = format!("cargo test -- {} && mutate", "x".repeat(500));
        assert_eq!(classify(&long, None), ExecKind::Action);
    }

    #[test]
    fn configured_checks_do_not_authorize_status_masking_or_shell_writes() {
        for command in [
            "cargo test || true",
            "cargo test; true",
            "cargo test\ntrue",
            "check > receipt",
            "check $(mutate)",
            "git branch -D old",
            "cargo test && sed -i 's/old/new/' src/lib.rs",
            "cargo test && git branch -D old",
            "cargo test && touch receipt",
            "cargo test && printf changed > receipt",
            "cargo test && bash scripts/smoke.sh || true",
            "cargo test && ruff check --fix",
        ] {
            assert_eq!(verify_level(command, Some(command)), None, "{command}");
        }
        let checks = "grep -q 'A  B' Spec.json && grep -q other README.md";
        assert_eq!(
            verify_level(checks, Some(checks)),
            Some(VerificationLevel::Behavioral)
        );
        assert_eq!(verify_level("git status", Some("git status")), None);
        let custom = "python3 -c \"assert 1 == 1\"";
        assert_eq!(
            verify_level(custom, Some(custom)),
            Some(VerificationLevel::Behavioral)
        );
    }

    #[test]
    fn exact_configured_fixture_chains_keep_their_verification_authority() {
        for fixture in [
            include_str!(
                "../../../tests/fixtures/runtime-self-fix-observer-rules/.spiral-coder.md"
            ),
            include_str!("../../../tests/fixtures/runtime-self-fix-pr-ready/.spiral-coder.md"),
            include_str!(
                "../../../tests/fixtures/runtime-benchmark-plan-tui-replay/.spiral-coder.md"
            ),
        ] {
            let command = fixture
                .lines()
                .find_map(|line| line.strip_prefix("test_cmd: "))
                .unwrap();
            assert_eq!(
                verify_level(command, Some(command)),
                Some(VerificationLevel::Behavioral)
            );
            assert_eq!(classify(command, Some(command)), ExecKind::Verify);
            assert_eq!(classify(command, None), ExecKind::Action);
            let different_config = format!("{command} --different");
            assert_eq!(verify_level(command, Some(&different_config)), None);
            let different_command = command
                .replace("scripts/", "Scripts/")
                .replace("review_panel", "Review_Panel");
            assert_ne!(different_command, command);
            assert_eq!(verify_level(&different_command, Some(command)), None);
        }
    }
}
