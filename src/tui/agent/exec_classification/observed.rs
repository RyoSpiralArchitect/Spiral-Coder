//! Recognition for Observer receipts, sharing the Coder shell/CLI grammar.
//! This does not grant Coder execution or completion authority.
use clap::Parser;
use std::path::{Component, Path};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum HarnessCheck {
    Replay,
    RuntimeEval,
}

pub(crate) fn is_observed_verification(command: &str) -> bool {
    super::verify_level(command, None).is_some()
        || is_required_harness_check(command, HarnessCheck::Replay)
        || is_required_harness_check(command, HarnessCheck::RuntimeEval)
}

pub(crate) fn is_required_harness_check(command: &str, expected: HarnessCheck) -> bool {
    let Some(chain) = super::commands(command) else {
        return false;
    };
    let mut matched = false;
    for words in &chain {
        let words = super::executable(words);
        // Relative spec identity cannot survive an untracked directory change.
        if words.first().map(String::as_str) == Some("cd")
            && words.get(1).map(String::as_str) != Some(".")
        {
            return false;
        }
        if let Some((kind, spec, root)) = invocation(words) {
            let expected_spec = match kind {
                HarnessCheck::Replay => ".spiral-coder/tui_replay.json",
                HarnessCheck::RuntimeEval => ".spiral-coder/runtime_eval.json",
            };
            if kind == expected
                && same_relative_path(&spec, expected_spec)
                && root.as_deref().is_none_or(|root| root == ".")
            {
                matched = true;
            }
        } else if !super::diagnostic(words) && super::verification(words).is_none() {
            return false;
        }
    }
    matched
}

fn same_relative_path(left: &str, right: &str) -> bool {
    let parts = |path| {
        Path::new(path)
            .components()
            .filter(|c| *c != Component::CurDir)
            .collect::<Vec<_>>()
    };
    parts(left) == parts(right)
}

fn invocation(words: &[String]) -> Option<(HarnessCheck, String, Option<String>)> {
    let first = words.first()?;
    let name = first.rsplit(['/', '\\']).next()?;
    let cli_words = if name == "cargo" && words.get(1).map(String::as_str) == Some("run") {
        let separator = words.iter().position(|word| word == "--")?;
        if !words[2..separator].iter().all(|word| {
            matches!(
                word.as_str(),
                "--quiet" | "-q" | "--release" | "--locked" | "--offline" | "--frozen"
            )
        }) {
            return None;
        }
        std::iter::once("spiral-coder".to_string())
            .chain(words[separator + 1..].iter().cloned())
            .collect::<Vec<_>>()
    } else if matches!(name, "spiral-coder" | "spiral-coder.exe") {
        words.to_vec()
    } else {
        return None;
    };
    let cli = crate::Cli::try_parse_from(cli_words).ok()?;
    match cli.command? {
        crate::Command::TuiReplay(args) => Some((
            HarnessCheck::Replay,
            args.spec.to_string_lossy().into_owned(),
            args.tool_root,
        )),
        crate::Command::Eval(args) => Some((
            HarnessCheck::RuntimeEval,
            args.spec.to_string_lossy().into_owned(),
            args.tool_root,
        )),
        _ => None,
    }
}
