//! Recognize the actual model-free replay subcommand, including global options.
use clap::Parser;

pub(super) fn is_replay(words: &[String]) -> bool {
    let Some(executable) = words.first() else {
        return false;
    };
    let name = executable.rsplit(['/', '\\']).next().unwrap_or("");
    if !matches!(name, "spiral-coder" | "spiral-coder.exe") {
        return false;
    }
    // Reuse the real CLI grammar so an option value containing `tui-replay`,
    // a help request, or `eval` cannot be mistaken for a replay execution.
    crate::Cli::try_parse_from(words)
        .is_ok_and(|cli| matches!(cli.command, Some(crate::Command::TuiReplay(_))))
}

#[cfg(test)]
mod tests {
    use super::super::{classify, verify_level};
    use crate::tui::agent::{ExecKind, VerificationLevel};

    #[test]
    fn replay_execution_uses_cli_grammar_instead_of_command_substrings() {
        for command in [
            "spiral-coder tui-replay",
            "spiral-coder --provider openai tui-replay --spec replay.json --filter existing",
            "./target/debug/spiral-coder tui-replay --provider gemini --spec 'a b.json'",
            "env MODE=test /opt/bin/spiral-coder tui-replay --max-cases 1 2>&1",
            "cd project && spiral-coder tui-replay --spec=replay.json && cargo test",
            "grep -q target replay.json && spiral-coder --provider openai tui-replay --spec replay.json --filter target",
        ] {
            assert_eq!(
                verify_level(command, None),
                Some(VerificationLevel::Behavioral),
                "{command}"
            );
        }
        for command in [
            "spiral-coder eval --spec tui-replay",
            "spiral-coder --model tui-replay agent 'fix the bug'",
            "spiral-coder tui-replay --unknown-flag",
            "spiral-coder tui-replay --help",
            "spiral-coder tui-replay --version",
            "spiral-coder tui-replay --spec",
            "spiral-coder tui-replay --max-cases invalid",
            "other-tool tui-replay",
            "spiral-coder tui-replay && touch changed",
            "spiral-coder tui-replay || true",
            "spiral-coder tui-replay > receipt",
        ] {
            assert_eq!(classify(command, None), ExecKind::Action, "{command}");
        }
        assert_eq!(verify_level("echo 'spiral-coder tui-replay'", None), None);
    }
}
