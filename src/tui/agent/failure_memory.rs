use super::{
    classify_error, error_class_hint, error_signature, hash_output, hint_for_known_failure,
    parse_exec_command_from_args, parse_exec_tool_output_sections, suspicious_success_reason,
    ErrorClass,
};

#[derive(Debug, Default)]
pub(super) struct FailureMemory {
    pub(super) consecutive_failures: usize,

    pub(super) last_command_sig: Option<String>,
    pub(super) same_command_repeats: usize,

    pub(super) last_error_sig: Option<String>,
    pub(super) same_error_repeats: usize,

    pub(super) last_output_hash: Option<u64>,
    pub(super) same_output_repeats: usize,

    pub(super) last_error_class: ErrorClass,
}

impl FailureMemory {
    pub(super) fn repeated_failure_or_stall(&self) -> bool {
        self.same_error_repeats >= 2
            || self.same_command_repeats >= 3
            || self.same_output_repeats >= 2
    }

    pub(super) fn from_recent_messages(messages: &[serde_json::Value]) -> Self {
        let mut mem = FailureMemory::default();

        // Map tool_call_id -> command for exec calls.
        let mut exec_by_id: std::collections::HashMap<String, String> =
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
                        .trim();
                    if name != "exec" {
                        continue;
                    }
                    let args = tc
                        .get("function")
                        .and_then(|f| f.get("arguments"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .trim();
                    if let Some(cmd) = parse_exec_command_from_args(args) {
                        exec_by_id.insert(id.to_string(), cmd);
                    }
                }
                continue;
            }

            if role == "tool" {
                let tcid = msg
                    .get("tool_call_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim();
                if tcid.is_empty() {
                    continue;
                }
                let Some(command) = exec_by_id.remove(tcid) else {
                    continue;
                };
                let content = msg
                    .get("content")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let (exit_code, stdout, stderr) = parse_exec_tool_output_sections(&content);
                let Some(mut effective_exit_code) = exit_code else {
                    continue;
                };
                if effective_exit_code == 0
                    && suspicious_success_reason(stdout.as_str(), stderr.as_str()).is_some()
                {
                    effective_exit_code = 1;
                }
                let _ = mem.on_tool_result(
                    command.as_str(),
                    stdout.as_str(),
                    stderr.as_str(),
                    effective_exit_code,
                );
            }
        }

        mem
    }

    pub(super) fn on_tool_result(
        &mut self,
        command: &str,
        stdout: &str,
        stderr: &str,
        effective_exit_code: i32,
    ) -> Option<String> {
        // Identity must preserve shell semantics. Display signatures truncate,
        // lowercase, and collapse quoted whitespace; they are not command identity.
        let cmd_sig = command.trim();
        let same_command = self.last_command_sig.as_deref() == Some(cmd_sig);
        if same_command {
            self.same_command_repeats = self.same_command_repeats.saturating_add(1);
        } else {
            self.last_command_sig = Some(cmd_sig.to_string());
            self.same_command_repeats = 1;
        }

        // Silence (or identical output) from distinct checks is not a loop.
        // Live execution and resume both count command/output pairs here.
        let oh = hash_output(stdout, stderr);
        if same_command && self.last_output_hash == Some(oh) {
            self.same_output_repeats = self.same_output_repeats.saturating_add(1);
        } else {
            self.last_output_hash = Some(oh);
            self.same_output_repeats = 1;
        }

        if effective_exit_code == 0 {
            self.consecutive_failures = 0;
            self.last_error_sig = None;
            self.same_error_repeats = 0;
            self.last_error_class = ErrorClass::Unknown;
            return None;
        }

        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
        self.last_error_class = classify_error(stderr, stdout);

        let sig = error_signature(command, stdout, stderr, effective_exit_code);
        if self.last_error_sig.as_deref() == Some(&sig) {
            self.same_error_repeats = self.same_error_repeats.saturating_add(1);
        } else {
            self.last_error_sig = Some(sig);
            self.same_error_repeats = 1;
        }

        // Emit hints only when crossing key thresholds to avoid spamming context.
        if self.same_error_repeats == 2 {
            if let Some(h) = hint_for_known_failure(command, stdout, stderr) {
                return Some(h);
            }
            return Some(
                "The SAME error happened twice.\n\
Action: stop repeating; gather diagnostics (`pwd`, `ls`, `git status`) then change strategy."
                    .to_string(),
            );
        }

        if self.same_command_repeats == 3 {
            return Some(
                "You ran the SAME command 3 times.\n\
Action: abandon this approach and try a different strategy (different cwd, different command, or add diagnostics)."
                    .to_string(),
            );
        }

        if self.consecutive_failures >= 3 {
            let class_ctx = error_class_hint(&self.last_error_class);
            let context = if class_ctx.is_empty() {
                String::new()
            } else {
                format!("\nLast error type: {class_ctx}")
            };
            return Some(format!(
                "3 consecutive failures.{context}\n\
Action: change strategy now; do NOT retry the same approach again."
            ));
        }

        if self.same_output_repeats >= 2 && self.same_command_repeats >= 2 {
            return Some(
                "Stuck detected: repeated identical output.\n\
Action: print diagnostics and change strategy; do not repeat the same command."
                    .to_string(),
            );
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::super::{validate_reflection, GoalDelta, ReflectionBlock, StrategyChange};
    use super::*;
    use serde_json::{json, Value};

    fn exec(messages: &mut Vec<Value>, command: &str, output: &str, exit: i32) {
        let id = format!("call_{}", messages.len());
        messages.push(json!({"role":"assistant", "tool_calls":[{
            "id":id, "type":"function", "function": {
                "name":"exec", "arguments":json!({"command":command}).to_string()
            }
        }]}));
        let status = if exit == 0 { "OK" } else { "FAILED" };
        messages.push(json!({"role":"tool", "tool_call_id":id,
            "content":format!("{status} (exit_code: {exit})\nstdout:\n{output}")
        }));
    }

    fn continuing_reflection() -> ReflectionBlock {
        ReflectionBlock {
            last_outcome: "the remaining required check passed".into(),
            goal_delta: GoalDelta::Closer,
            wrong_assumption: "none; fresh evidence supports finishing the plan".into(),
            strategy_change: StrategyChange::Keep,
            next_minimal_action: "report the verified result".into(),
        }
    }

    #[test]
    fn resumed_distinct_silent_checks_do_not_create_stall() {
        let commands = [
            "test -f report.md",
            "test -f receipt.txt",
            "printf updated > report.md",
            "test -f report.md",
        ];
        let mut messages = Vec::new();
        let mut live = FailureMemory::default();
        for command in commands {
            exec(&mut messages, command, "", 0);
            live.on_tool_result(command, "", "", 0);
        }
        let mut resumed = FailureMemory::from_recent_messages(&messages);
        for memory in [&mut live, &mut resumed] {
            memory.on_tool_result("test -f receipt.txt", "", "", 0);
            assert_eq!(memory.same_command_repeats, 1);
            assert_eq!(memory.same_output_repeats, 1);
            validate_reflection(&continuing_reflection(), memory, 0)
                .expect("different successful checks are forward progress");
        }
    }

    #[test]
    fn repeated_identical_success_or_failure_still_requires_change() {
        for exit in [0, 1] {
            let mut messages = Vec::new();
            let mut live = FailureMemory::default();
            for _ in 0..2 {
                exec(&mut messages, "test -f receipt.txt", "", exit);
                live.on_tool_result("test -f receipt.txt", "", "", exit);
            }
            let resumed = FailureMemory::from_recent_messages(&messages);
            for memory in [live, resumed] {
                assert_eq!(memory.same_output_repeats, 2);
                assert!(validate_reflection(&continuing_reflection(), &memory, 0).is_err());
            }
        }
    }

    #[test]
    fn command_identity_preserves_case_quotes_suffix_and_multiline_semantics() {
        for (first, second) in [
            ("test -f receipt-A", "test -f receipt-a"),
            ("test -f 'two  spaces'", "test -f 'two spaces'"),
            ("printf first\ntest -f a", "printf first\ntest -f b"),
        ].into_iter().chain(std::iter::once((
            concat!("test -f ", "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-A"),
            concat!("test -f ", "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-B"),
        ))) {
            let mut memory = FailureMemory::default();
            memory.on_tool_result(first, "", "", 0);
            memory.on_tool_result(second, "", "", 0);
            assert_eq!(memory.same_command_repeats, 1, "{first:?} != {second:?}");
            assert_eq!(memory.same_output_repeats, 1);
        }
    }
}
