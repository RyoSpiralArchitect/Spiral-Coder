use super::*;

/// Classifies the kind of error from stderr/stdout so that targeted recovery
/// hints can be injected before the generic diagnosis protocol.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum ErrorClass {
    Environment, // command not found, permission denied, not recognized
    Syntax,      // parse error, unexpected token, syntax error
    Path,        // no such file, cannot find path, path not found
    Dependency,  // module not found, missing package/crate
    Network,     // connection refused, timeout, could not connect
    Logic,       // assertion failed, test failed, expected X got Y
    Unknown,
}

impl Default for ErrorClass {
    fn default() -> Self {
        ErrorClass::Unknown
    }
}

pub(super) fn classify_error(stderr: &str, stdout: &str) -> ErrorClass {
    let combined = format!("{stderr}\n{stdout}");
    let low = combined.to_ascii_lowercase();

    if low.contains("command not found")
        || low.contains("is not recognized as the name")
        || low.contains("is not recognized as an internal")
        || low.contains("permission denied")
        || low.contains("access is denied")
        || low.contains("access denied")
        || low.contains("commandnotfoundexception")
    {
        ErrorClass::Environment
    } else if low.contains("syntax error")
        || low.contains("unexpected token")
        || low.contains("parse error")
        || low.contains("parsererror")
        || low.contains("invalid syntax")
        || low.contains("missing expression")
        || low.contains("unexpected end of")
    {
        ErrorClass::Syntax
    } else if low.contains("no such file")
        || low.contains("cannot find path")
        || low.contains("path not found")
        || (low.contains("does not exist") && !low.contains("package"))
    {
        ErrorClass::Path
    } else if low.contains("modulenotfounderror")
        || low.contains("cannot find module")
        || low.contains("no module named")
        || low.contains("package not found")
        || low.contains("no such package")
        || (low.contains("could not find") && (low.contains("package") || low.contains("crate")))
    {
        ErrorClass::Dependency
    } else if low.contains("connection refused")
        || low.contains("timed out")
        || low.contains("network unreachable")
        || low.contains("could not connect")
        || low.contains("name resolution failed")
        || low.contains("could not resolve host")
        || low.contains("couldn't resolve host")
        || low.contains("temporary failure in name resolution")
    {
        ErrorClass::Network
    } else if low.contains("assertion")
        || low.contains("test failed")
        || (low.contains("expected") && low.contains("actual"))
    {
        ErrorClass::Logic
    } else {
        ErrorClass::Unknown
    }
}

/// Returns a targeted one-line recovery hint for each error class,
/// or an empty string for Unknown (no hint injected).
pub(super) fn error_class_hint(class: &ErrorClass) -> &'static str {
    match class {
        ErrorClass::Environment =>
            "⚠ ENVIRONMENT ERROR: The binary/permission is missing. Fix the environment first — do NOT modify source code.",
        ErrorClass::Syntax =>
            "⚠ SYNTAX ERROR: There is a language/parser mistake. Fix the exact line — do NOT change unrelated code.",
        ErrorClass::Path => {
            if cfg!(target_os = "windows") {
                "⚠ PATH ERROR: A file or directory is missing. Verify paths with Get-ChildItem/dir before creating or reading."
            } else {
                "⚠ PATH ERROR: A file or directory is missing. Verify paths with ls before creating or reading."
            }
        }
        ErrorClass::Dependency =>
            "⚠ DEPENDENCY ERROR: A required package is missing. Install it first, then retry the original command.",
        ErrorClass::Network =>
            "⚠ NETWORK ERROR: Cannot reach a remote service. Check if the service is running and proxy vars are clear.",
        ErrorClass::Logic =>
            "⚠ LOGIC ERROR: The code ran but produced wrong results. Re-read the relevant logic before re-running.",
        ErrorClass::Unknown => "",
    }
}

/// Extra targeted hints for known high-frequency Windows/Git failure modes.
pub(super) fn specific_recovery_hint(stderr: &str, stdout: &str) -> &'static str {
    let combined = format!("{stderr}\n{stdout}");
    let low = combined.to_ascii_lowercase();

    // GitHub HTTPS routed via a dead local proxy (common in locked-down networks).
    // Example: "Failed to connect to github.com port 443 via 127.0.0.1 ... Could not connect to server"
    let git_github_proxy = (low.contains("github.com") || low.contains("ssh.github.com"))
        && low.contains("port 443")
        && (low.contains("via 127.0.0.1") || low.contains("via localhost"))
        && (low.contains("could not connect") || low.contains("failed to connect"));
    if git_github_proxy {
        return "HINT: Git HTTPS appears to be routed via a dead local proxy (127.0.0.1/localhost).\n\
- Try clearing proxy env vars: HTTP_PROXY / HTTPS_PROXY / ALL_PROXY / GIT_HTTP_PROXY / GIT_HTTPS_PROXY.\n\
- For GitHub, prefer SSH-over-443:\n\
  git push ssh://git@ssh.github.com:443/<owner>/<repo>.git main\n\
  (and similarly for fetch/pull).";
    }

    let dns_failure = low.contains("could not resolve host")
        || low.contains("couldn't resolve host")
        || low.contains("temporary failure in name resolution")
        || low.contains("name resolution failed");
    if dns_failure && (low.contains("crates.io") || low.contains("index.crates.io")) {
        return "HINT: Cargo could not reach crates.io/index (DNS/network failure).\n\
- If this environment is offline/sandboxed, you cannot download new crates; stop retrying `cargo` in a loop.\n\
- Try `cargo ... --offline` only if dependencies are already cached.\n\
- Otherwise proceed with static edits and defer builds/tests to a networked environment.";
    }
    if dns_failure {
        return "HINT: Network/DNS failure (host name resolution failed).\n\
- Stop retrying the same command.\n\
- Check connectivity and clear proxy env vars if needed (HTTP_PROXY/HTTPS_PROXY/ALL_PROXY).\n\
- If running in an offline sandbox, installs/downloads will not work; switch to an offline strategy.";
    }

    // Windows: `cargo run` cannot overwrite a running .exe (locked file handle).
    // Example: "failed to remove file ... spiral-coder.exe ... access is denied (os error 5)"
    let cargo_exe_lock = low.contains("failed to remove file")
        && low.contains("spiral-coder.exe")
        && (low.contains("os error 5") || low.contains("access is denied"));
    if cargo_exe_lock {
        return "HINT: On Windows, a running .exe cannot be overwritten.\n\
- Stop the running process (`Stop-Process -Name spiral-coder -Force`) OR restart the terminal.\n\
- Or run cargo with an isolated target dir to avoid the lock:\n\
  $env:CARGO_TARGET_DIR = '.tmp/cargo-target-tui'; cargo run -- tui";
    }

    ""
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ExecKind {
    Diagnostic,
    Action,
    Verify,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub(super) enum VerificationLevel {
    #[default]
    Build,
    Behavioral,
}

impl VerificationLevel {
    pub(super) fn as_str(&self) -> &'static str {
        match self {
            Self::Build => "build",
            Self::Behavioral => "behavioral",
        }
    }

    pub(super) fn satisfies(self, required: Self) -> bool {
        matches!(
            (self, required),
            (VerificationLevel::Behavioral, _)
                | (VerificationLevel::Build, VerificationLevel::Build)
        )
    }

    pub(super) fn max(self, other: Self) -> Self {
        if self >= other {
            self
        } else {
            other
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(super) struct GoalCheckStatus {
    pub(super) attempts: usize,
    pub(super) ok: bool,
}

#[derive(Debug, Clone, Default)]
pub(super) struct GoalCheckTracker {
    pub(super) repo: GoalCheckStatus,
    pub(super) tests: GoalCheckStatus,
    pub(super) build: GoalCheckStatus,
}

impl GoalCheckTracker {
    pub(super) fn get_mut(&mut self, key: &str) -> Option<&mut GoalCheckStatus> {
        match key {
            "repo" => Some(&mut self.repo),
            "tests" => Some(&mut self.tests),
            "build" => Some(&mut self.build),
            _ => None,
        }
    }

    pub(super) fn any_attempted(&self) -> bool {
        self.repo.attempts > 0 || self.tests.attempts > 0 || self.build.attempts > 0
    }
}

pub(super) fn infer_required_verification_level(
    root_user_text: &str,
    test_cmd: Option<&str>,
) -> VerificationLevel {
    let low = root_user_text.to_ascii_lowercase();
    let verification = governor_contract::verification();
    let looks_doc_only = text_contains_any(&low, &verification.intent_doc_terms)
        && !text_contains_any(&low, &verification.intent_behavioral_terms);
    if looks_doc_only {
        return VerificationLevel::Build;
    }
    if test_cmd.is_some() || text_contains_any(&low, &verification.intent_behavioral_terms) {
        return VerificationLevel::Behavioral;
    }
    VerificationLevel::Build
}

pub(super) fn verification_level_from_plan(plan: &PlanBlock) -> Option<VerificationLevel> {
    if plan.acceptance_criteria.is_empty() {
        return None;
    }

    let verification = governor_contract::verification();

    let mut saw_build_like = false;
    for criterion in &plan.acceptance_criteria {
        let low = criterion.to_ascii_lowercase();
        if text_contains_any(&low, &verification.plan_behavioral_terms) {
            return Some(VerificationLevel::Behavioral);
        }
        if text_contains_any(&low, &verification.plan_build_terms) {
            saw_build_like = true;
            continue;
        }
        return Some(VerificationLevel::Behavioral);
    }

    if saw_build_like {
        Some(VerificationLevel::Build)
    } else {
        None
    }
}

pub(super) fn verification_level_for_mutation_path(path: &str) -> VerificationLevel {
    let low = path.trim().to_ascii_lowercase();
    if low.is_empty() {
        return VerificationLevel::Build;
    }
    let verification = governor_contract::verification();
    if text_contains_any(&low, &verification.doc_path_terms) {
        return VerificationLevel::Build;
    }

    if verification
        .behavioral_path_extensions
        .iter()
        .any(|ext| low.ends_with(ext))
    {
        return VerificationLevel::Behavioral;
    }

    VerificationLevel::Build
}

pub(super) fn effective_verify_ok_step(
    required: VerificationLevel,
    last_build_verify_ok_step: Option<usize>,
    last_behavioral_verify_ok_step: Option<usize>,
) -> Option<usize> {
    match required {
        VerificationLevel::Build => last_build_verify_ok_step
            .into_iter()
            .chain(last_behavioral_verify_ok_step)
            .max(),
        VerificationLevel::Behavioral => last_behavioral_verify_ok_step,
    }
}

pub(super) fn upgrade_required_verification_from_messages(
    messages: &[serde_json::Value],
    base: VerificationLevel,
) -> VerificationLevel {
    let mut required = base;
    let mut by_id: std::collections::HashMap<String, (String, String)> =
        std::collections::HashMap::new();

    for msg in messages {
        let role = msg.get("role").and_then(|v| v.as_str()).unwrap_or("");
        if role == "assistant" {
            let Some(tcs) = msg.get("tool_calls").and_then(|v| v.as_array()) else {
                continue;
            };
            for tc in tcs {
                let id = tc.get("id").and_then(|v| v.as_str()).unwrap_or("").trim();
                if id.is_empty() {
                    continue;
                }
                let name = tc
                    .get("function")
                    .and_then(|f| f.get("name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string();
                let args = tc
                    .get("function")
                    .and_then(|f| f.get("arguments"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim();
                let path = if matches!(name.as_str(), "write_file" | "patch_file" | "apply_diff") {
                    serde_json::from_str::<serde_json::Value>(args)
                        .ok()
                        .and_then(|v| {
                            v.get("path")
                                .and_then(|p| p.as_str())
                                .map(|s| s.to_string())
                        })
                        .unwrap_or_default()
                } else {
                    String::new()
                };
                by_id.insert(id.to_string(), (name, path));
            }
            continue;
        }

        if role != "tool" {
            continue;
        }

        let id = msg
            .get("tool_call_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        if id.is_empty() {
            continue;
        }
        let Some((name, path)) = by_id.remove(id) else {
            continue;
        };
        let content = msg.get("content").and_then(|v| v.as_str()).unwrap_or("");
        if crate::execution_evidence::file_edit_succeeded(&name, content)
            && content.contains("[hash]")
        {
            required = required.max(verification_level_for_mutation_path(path.as_str()));
        }
    }

    required
}

pub(super) fn is_diagnostic_tool_name(name: &str) -> bool {
    governor_contract::diagnostic_tool_names()
        .iter()
        .any(|tool| tool == name)
}

pub(super) fn is_diagnostic_command(command: &str) -> bool {
    exec_classification::is_diagnostic(command)
}

pub(super) fn verification_examples(level: VerificationLevel) -> String {
    let verification = governor_contract::verification();
    let mut commands: Vec<String> = verification
        .goal_check_runners
        .iter()
        .filter_map(|runner| match level {
            VerificationLevel::Build => runner.build_command.as_ref(),
            VerificationLevel::Behavioral => runner.test_command.as_ref(),
        })
        .map(|command| command.trim().to_string())
        .filter(|command| !command.is_empty())
        .collect();

    let signatures = match level {
        VerificationLevel::Build => &verification.build_command_signatures,
        VerificationLevel::Behavioral => &verification.behavioral_command_signatures,
    };
    for command in signatures {
        let trimmed = command.trim();
        if trimmed.is_empty() || commands.iter().any(|existing| existing == trimmed) {
            continue;
        }
        commands.push(trimmed.to_string());
        if commands.len() >= 6 {
            break;
        }
    }

    let quoted = commands
        .into_iter()
        .take(6)
        .map(|command| format!("`{command}`"))
        .collect::<Vec<_>>();
    match quoted.as_slice() {
        [] => match level {
            VerificationLevel::Build => {
                "`cargo check`, `cargo build`, `tsc --noemit`, `ruff check`, or `git diff --check`"
                    .to_string()
            }
            VerificationLevel::Behavioral => {
                "`cargo test`, `cargo nextest`, `pytest`, `npm test`, `go test`, or `dotnet test`"
                    .to_string()
            }
        },
        [only] => only.clone(),
        [head @ .., tail] => format!("{} or {}", head.join(", "), tail),
    }
}

pub(super) fn configured_test_cmd_verification_level(
    test_cmd: Option<&str>,
) -> Option<VerificationLevel> {
    exec_classification::configured_level(test_cmd)
}

pub(super) fn verification_requirement_hint(
    level: VerificationLevel,
    test_cmd: Option<&str>,
) -> String {
    let preferred = test_cmd
        .and_then(|cmd| {
            let cmd = cmd.trim();
            if cmd.is_empty() {
                return None;
            }
            let cmd_level = configured_test_cmd_verification_level(Some(cmd))?;
            if cmd_level.satisfies(level) {
                Some(format!("Preferred: run `{cmd}`."))
            } else {
                None
            }
        })
        .unwrap_or_else(|| format!("Examples: {}.", verification_examples(level)));
    format!(
        "run ONE real {} verification command. {preferred}",
        level.as_str()
    )
}

pub(super) fn verification_requirement_note(
    level: VerificationLevel,
    test_cmd: Option<&str>,
    plan: Option<&PlanBlock>,
) -> String {
    let contrast = match level {
        VerificationLevel::Build => {
            "Behavioral tests are welcome, but a real build/check/lint is the minimum requirement."
        }
        VerificationLevel::Behavioral => {
            "Build-only checks do NOT satisfy this task; use tests or another behavioral verification."
        }
    };
    let mut out = format!(
        "[Verification Requirement]\nBefore `done`, this task requires {} verification.\n{}\n{}",
        level.as_str(),
        contrast,
        verification_requirement_hint(level, test_cmd)
    );
    if let Some(plan) = plan {
        if !plan.acceptance_criteria.is_empty() {
            out.push_str("\nAcceptance criteria:\n");
            for criterion in &plan.acceptance_criteria {
                out.push_str(&format!("- {criterion}\n"));
            }
        }
    }
    out
}

pub(super) fn should_emit_verification_requirement_prompt(
    state: AgentState,
    recovery: &RecoveryGovernor,
    last_mutation_step: Option<usize>,
    last_verify_ok_step: Option<usize>,
    goal_checks: &GoalCheckTracker,
) -> bool {
    state == AgentState::Verifying
        || recovery.stage == Some(RecoveryStage::Verify)
        || last_mutation_step.unwrap_or(0) > last_verify_ok_step.unwrap_or(0)
        || goal_checks.any_attempted()
}

pub(super) fn classify_verify_level(
    command: &str,
    test_cmd: Option<&str>,
) -> Option<VerificationLevel> {
    exec_classification::verify_level(command, test_cmd)
}

pub(super) fn classify_exec_kind(command: &str, test_cmd: Option<&str>) -> ExecKind {
    exec_classification::classify(command, test_cmd)
}

pub(crate) fn exec_may_mutate_for_evaluation(command: &str, test_cmd: Option<&str>) -> bool {
    matches!(classify_exec_kind(command, test_cmd), ExecKind::Action)
}

pub(super) fn goal_check_max_attempts() -> usize {
    let max_attempts = governor_contract::verification()
        .goal_check_policy
        .max_attempts_per_goal;
    max_attempts.max(1)
}

pub(super) fn goal_check_order() -> Vec<String> {
    let order = governor_contract::verification()
        .goal_check_policy
        .goal_order
        .iter()
        .map(|item| item.trim())
        .filter(|item| !item.is_empty())
        .map(|item| item.to_string())
        .collect::<Vec<_>>();
    if order.is_empty() {
        vec!["repo".to_string(), "tests".to_string(), "build".to_string()]
    } else {
        order
    }
}

pub(super) fn should_auto_run_goal_checks(
    command_approval_required: bool,
    max_iters: usize,
) -> bool {
    let policy = &governor_contract::verification().goal_check_policy;
    if !policy.run_on_stop {
        return false;
    }
    if policy.require_exec_feature {
        // TUI agentic mode always provides exec.
    }
    if policy.require_longrun && max_iters <= 1 {
        return false;
    }
    if policy.require_command_approval_off && command_approval_required {
        return false;
    }
    true
}

pub(super) fn pick_interesting_error_line(stdout: &str, stderr: &str) -> String {
    interesting_failure_line(stdout, stderr)
}

pub(super) fn error_signature(command: &str, stdout: &str, stderr: &str, exit_code: i32) -> String {
    let cmd = command_sig(command);
    let line = pick_interesting_error_line(stdout, stderr);
    format!("exit={exit_code}|cmd={cmd}|err={line}")
}

pub(super) fn hash_output(stdout: &str, stderr: &str) -> u64 {
    let mut h = DefaultHasher::new();
    stdout.trim_end().hash(&mut h);
    stderr.trim_end().hash(&mut h);
    h.finish()
}

pub(super) fn hash_text(s: &str) -> u64 {
    let mut h = DefaultHasher::new();
    s.trim_end().hash(&mut h);
    h.finish()
}

pub(super) fn fmt_hash(h: u64) -> String {
    format!("{h:016x}")
}

pub(super) fn hint_for_known_failure(command: &str, stdout: &str, stderr: &str) -> Option<String> {
    let mut s = String::new();
    s.push_str(stdout);
    s.push('\n');
    s.push_str(stderr);
    let low = s.to_ascii_lowercase();
    let cmd_low = command.to_ascii_lowercase();

    if low.contains("the term '$' is not recognized")
        || (low.contains("the term '$'") && low.contains("not recognized"))
    {
        return Some(
            "Your command includes a transcript prompt marker like `$` / `PS>`.\n\
Fix: send ONLY the command (e.g. `git status`), not `$ git status`."
                .to_string(),
        );
    }
    if low.contains("unexpected token '}'")
        || (low.contains("unexpected token") && low.contains('}'))
    {
        return Some(
            "PowerShell saw a stray `}` in the command.\n\
Fix: remove the trailing `}` and retry."
                .to_string(),
        );
    }
    if low.contains("adding embedded git repository")
        || low.contains("does not have a commit checked out")
    {
        return Some(
            "You are trying to `git add` a nested repo directory.\n\
Fix: `cd` into the intended repo before `git add .`, or add the nested repo dir to `.gitignore` (or use `git submodule add ...`)."
                .to_string(),
        );
    }
    if low.contains("failed to remove file")
        && (low.contains("access is denied") || low.contains("permission denied"))
        && cmd_low.contains("cargo")
    {
        return Some(
            "Rust build failed because `spiral-coder.exe` is locked.\n\
Fix: stop the running process (or close the TUI/serve), then rebuild.\n\
Tip (Windows): use `scripts/run-tui.ps1` / `scripts/run-ui.ps1` which build in an isolated CARGO_TARGET_DIR and auto-kill old processes."
                .to_string(),
        );
    }
    if low.contains("could not resolve host")
        || low.contains("couldn't resolve host")
        || low.contains("temporary failure in name resolution")
        || low.contains("name resolution failed")
    {
        if cmd_low.contains("cargo") || low.contains("crates.io") || low.contains("index.crates.io")
        {
            return Some(
                "Network/DNS failure: cannot reach crates.io/index.\n\
Action: stop retrying; this is not a code bug.\n\
- If you are offline/sandboxed, you cannot download new crates here.\n\
- Try `cargo ... --offline` only if deps are already cached; otherwise proceed with static edits and defer builds/tests."
                    .to_string(),
            );
        }
        return Some(
            "Network/DNS failure: host name resolution failed.\n\
Action: stop retrying; check connectivity and clear proxy env vars (HTTP_PROXY/HTTPS_PROXY/ALL_PROXY)."
                .to_string(),
        );
    }
    if low.contains("could not connect to server")
        && low.contains("127.0.0.1")
        && cmd_low.contains("git")
    {
        return Some(
            "Git network failed via a dead local proxy (127.0.0.1).\n\
Fix: clear proxy env vars: `Remove-Item Env:HTTP_PROXY,Env:HTTPS_PROXY,Env:ALL_PROXY -ErrorAction SilentlyContinue`."
                .to_string(),
        );
    }
    if low.contains("incorrect api key")
        || low.contains("invalid_api_key")
        || (low.contains("http 401") && low.contains("api key"))
    {
        return Some(
            "Provider returned HTTP 401 (bad/missing API key).\n\
Fix: update the configured API key for the selected provider/model, then retry."
                .to_string(),
        );
    }

    None
}

pub(super) fn parse_exec_command_from_args(args: &str) -> Option<String> {
    // Standard tool schema uses JSON arguments: {"command":"...","cwd":"..."}.
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(args) {
        if let Some(cmd) = v.get("command").and_then(|x| x.as_str()) {
            let t = cmd.trim();
            if !t.is_empty() {
                return Some(t.to_string());
            }
        }
    }

    // Fallback: some providers/models might pass a raw string.
    let t = args.trim();
    if !t.is_empty() {
        return Some(t.to_string());
    }
    None
}

pub(super) fn parse_exit_code_from_tool_text(s: &str) -> Option<i32> {
    let low = s.to_ascii_lowercase();
    let key = "exit_code:";
    let idx = low.find(key)?;
    let after = low[idx + key.len()..].trim_start();
    let mut num = String::new();
    for ch in after.chars() {
        if ch == '-' || ch.is_ascii_digit() {
            num.push(ch);
        } else {
            break;
        }
    }
    if num.is_empty() {
        return None;
    }
    num.parse::<i32>().ok()
}

pub(super) fn parse_exec_tool_output_sections(tool_content: &str) -> (Option<i32>, String, String) {
    let t = tool_content.replace("\r\n", "\n");
    let exit_code = parse_exit_code_from_tool_text(t.as_str());

    // Prefer parsing known markers so the governor sees the "real" stdout/stderr.
    // - OK: "OK (exit_code: 0)\nstdout:\n..."
    // - FAILED: "...FAILED (exit_code: X)\n...stdout (tail):\n...\nstderr:\n..."
    let mut stdout = String::new();
    let mut stderr = String::new();

    let (before_stderr, stderr_part) = match t.split_once("\nstderr:\n") {
        Some((a, b)) => (a, Some(b)),
        None => (t.as_str(), None),
    };
    if let Some(b) = stderr_part {
        stderr = b.trim_end().to_string();
    }

    // stdout markers can exist with or without stderr.
    if let Some((_, s_out)) = before_stderr.rsplit_once("\nstdout (tail):\n") {
        stdout = s_out.trim_end().to_string();
    } else if let Some((_, s_out)) = before_stderr.rsplit_once("\nstdout:\n") {
        stdout = s_out.trim_end().to_string();
    }

    // If we can't parse anything meaningful, treat the whole tool output as stderr on failures.
    if stdout.is_empty() && stderr.is_empty() {
        if exit_code.unwrap_or(0) != 0 {
            stderr = t.trim_end().to_string();
        }
    }

    (exit_code, stdout, stderr)
}

pub(super) fn goal_check_support_line(summary: &str, fallback: &str) -> String {
    if summary.trim().is_empty() {
        fallback.to_string()
    } else {
        governor_contract::goal_check_supported_runners_message(summary)
    }
}

pub(super) fn error_class_name(class: &ErrorClass) -> &'static str {
    match class {
        ErrorClass::Environment => "environment",
        ErrorClass::Syntax => "syntax",
        ErrorClass::Path => "path",
        ErrorClass::Dependency => "dependency",
        ErrorClass::Network => "network",
        ErrorClass::Logic => "logic",
        ErrorClass::Unknown => "unknown",
    }
}

pub(super) fn goal_check_class_line(class: &ErrorClass) -> String {
    match class {
        ErrorClass::Unknown => String::new(),
        _ => format!("class: {}\n", error_class_name(class)),
    }
}

pub(super) fn goal_check_digest_line(digest: &str) -> String {
    let trimmed = digest.trim();
    if trimmed.is_empty() {
        String::new()
    } else {
        format!("{trimmed}\n")
    }
}

pub(super) fn goal_check_runner_command(
    level: VerificationLevel,
    cwd: &str,
    configured_test_cmd: Option<&str>,
) -> Option<String> {
    if matches!(level, VerificationLevel::Behavioral) {
        if let Some(command) = configured_test_cmd
            .map(str::trim)
            .filter(|command| !command.is_empty())
        {
            return Some(command.to_string());
        }
    }
    governor_contract::verification()
        .goal_check_runners
        .iter()
        .find(|runner| {
            runner
                .detect_files_any
                .iter()
                .filter(|path| !path.trim().is_empty())
                .any(|path| Path::new(cwd).join(path).exists())
        })
        .and_then(|runner| match level {
            VerificationLevel::Build => runner.build_command.clone(),
            VerificationLevel::Behavioral => runner.test_command.clone(),
        })
        .map(|command| command.trim().to_string())
        .filter(|command| !command.is_empty())
}

pub(super) fn goal_check_runner_summary(level: VerificationLevel) -> String {
    governor_contract::verification()
        .goal_check_runners
        .iter()
        .filter_map(|runner| {
            let command = match level {
                VerificationLevel::Build => runner.build_command.as_deref(),
                VerificationLevel::Behavioral => runner.test_command.as_deref(),
            }?
            .trim();
            if command.is_empty() {
                return None;
            }
            let files = runner
                .detect_files_any
                .iter()
                .map(|path| path.trim())
                .filter(|path| !path.is_empty())
                .collect::<Vec<_>>();
            if files.is_empty() {
                return None;
            }
            Some(format!("{} -> {command}", files.join("/")))
        })
        .collect::<Vec<_>>()
        .join(", ")
}

pub(super) async fn repo_goal_missing_labels(cwd: &str) -> Vec<String> {
    let mut missing = Vec::new();
    for requirement in &governor_contract::verification().repo_goal_requirements {
        let key = requirement.key.trim();
        let label = requirement.label.trim();
        let present = match requirement.probe.trim() {
            "dir_exists" => requirement
                .path
                .as_deref()
                .map(|path| Path::new(cwd).join(path).is_dir())
                .unwrap_or(false),
            "file_exists" => requirement
                .path
                .as_deref()
                .map(|path| Path::new(cwd).join(path).is_file())
                .unwrap_or(false),
            "git_head" => run_git_cmd(cwd, &["rev-parse", "HEAD"])
                .await
                .map(|head| !head.trim().is_empty())
                .unwrap_or(false),
            _ => false,
        };
        if !present {
            missing.push(if label.is_empty() {
                key.to_string()
            } else {
                label.to_string()
            });
        }
    }
    missing
}

#[derive(Debug)]
pub(super) struct GoalCheckExecResult {
    pub(super) command: String,
    pub(super) passed: bool,
    pub(super) error_class: ErrorClass,
    pub(super) digest: String,
}

pub(super) async fn run_goal_check_command(
    label: &str,
    command: &str,
    cwd: &str,
    tx: &mpsc::Sender<StreamToken>,
) -> GoalCheckExecResult {
    let sig = command_sig(command);
    let command_for_evidence = compact_one_line(command, 200);
    let _ = tx
        .send(StreamToken::Delta(format!(
            "\n{}\n",
            governor_contract::goal_check_exec_run_message(label, &sig)
        )))
        .await;

    let exec_result = exec::run_command(command, Some(cwd)).await;
    let (stdout, stderr, exit_code) = match exec_result {
        Ok(r) => (r.stdout, r.stderr, r.exit_code),
        Err(e) => (String::new(), e.to_string(), -1),
    };
    let suspicious = if exit_code == 0 {
        suspicious_success_reason(&stdout, &stderr)
    } else {
        None
    };
    let passed = exit_code == 0 && suspicious.is_none();
    let error_class = if passed {
        ErrorClass::Unknown
    } else {
        classify_error(&stderr, &stdout)
    };
    let digest = if passed {
        String::new()
    } else if let Some(reason) = suspicious {
        reason
    } else {
        let interesting = pick_interesting_error_line(&stdout, &stderr);
        if interesting.is_empty() {
            truncate_output_tail(&format!("{stdout}{stderr}"), 600)
        } else {
            interesting
        }
    };

    let summary = if passed {
        format!(
            "{}\n",
            governor_contract::goal_check_exec_ok_message(label, &sig)
        )
    } else {
        format!(
            "{}\n",
            governor_contract::goal_check_exec_fail_message(
                label,
                &sig,
                compact_one_line(&digest, 160).as_str(),
            )
        )
    };
    let _ = tx.send(StreamToken::Delta(summary)).await;

    GoalCheckExecResult {
        command: command_for_evidence,
        passed,
        error_class,
        digest,
    }
}
