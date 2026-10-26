// Operator-owned behavioral regression, visible and protected in the candidate.
use super::*;
use crate::streaming::ToolCallData;
use crate::tui::agent::exec_verification::ExecVerificationContext;
use serde_json::{json, Value};

const CHECK: &str = "python3 -S -m unittest discover -s tests -v";
const SOURCE: &str = "src/repair.py";
const TEST: &str = "tests/test_repair.py";
const OTHER: &str = "docs/notes.md";

struct Trace {
    root: tempfile::TempDir,
    live: RecoveryGovernor,
    messages: Vec<Value>,
}

impl Trace {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        for (path, body) in [
            (SOURCE, "def value():\n    return 1\n\n# preserve this\n"),
            (TEST, "def test_value():\n    assert value() == 2\n"),
            (OTHER, "Unrelated documentation.\n"),
        ] {
            let path = root.path().join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, body).unwrap();
        }
        let mut trace = Self {
            root,
            live: RecoveryGovernor {
                required_verification: VerificationLevel::Behavioral,
                ..Default::default()
            },
            messages: vec![
                json!({"role":"user","content":"Fix src/repair.py and verify the existing tests. Preserve tests and documentation."}),
            ],
        };
        trace.failed_check(CHECK, true);
        assert!(trace.live.focus.requires_read());
        trace.read(TEST, false);
        assert!(trace.live.focus.observed());
        assert_eq!(trace.live.stage, Some(RecoveryStage::Fix));
        trace
    }

    fn call(&self, name: &str, arguments: Value) -> ToolCallData {
        ToolCallData {
            id: format!("frozen-call-{}", self.messages.len()),
            name: name.into(),
            arguments: arguments.to_string(),
            thought_signature: None,
        }
    }

    fn record(&mut self, call: &ToolCallData, result: String) {
        let context = ExecVerificationContext::from_messages(Some(CHECK), &self.messages);
        self.live.focus.record_result(
            call,
            &result,
            self.root.path().to_str(),
            Some(CHECK),
            &context,
        );
        self.messages.push(json!({"role":"assistant","tool_calls":[{
            "id":call.id,"type":"function","function":{"name":call.name,"arguments":call.arguments}
        }]}));
        self.messages
            .push(self.live.focus.tool_message(call, result));
    }

    fn failed_check(&mut self, command: &str, localized: bool) {
        let call = self.call("exec", json!({"command":command}));
        let diagnostic = if localized {
            format!(
                "  File \"{}\", line 2, in test_value\nAssertionError: expected 2, got 1\n",
                self.root.path().join(TEST).display()
            )
        } else {
            "error: unrelated integration check failed\n".into()
        };
        let result = crate::tui::agent::build_failed_tool_output("", &diagnostic, 1);
        let result = crate::tui::agent::inject_cwd(
            &result,
            &format!("cwd: {}", self.root.path().display()),
            None,
        );
        self.record(&call, result);
        self.live
            .on_exec_result(ExecKind::Verify, Some(VerificationLevel::Behavioral), false);
    }

    fn failed_patch(&mut self, path: &str) {
        let call = self.call(
            "patch_file",
            json!({"path":path,"search":"absent old anchor","replace":"correct replacement"}),
        );
        let (result, failed) = crate::file_tools::tool_patch_file(
            path,
            "absent old anchor",
            "correct replacement",
            self.root.path().to_str(),
        );
        assert!(failed);
        self.record(&call, result);
        self.live.on_fix_result(false, None);
    }

    fn read(&mut self, path: &str, ranged: bool) {
        let arguments = if ranged {
            json!({"path":path,"start_line":1,"end_line":2})
        } else {
            json!({"path":path})
        };
        let call = self.call("read_file", arguments.clone());
        let (result, failed) = if ranged {
            crate::file_tools::read_range::execute_args(&arguments, self.root.path().to_str())
        } else {
            crate::file_tools::tool_read_file(path, self.root.path().to_str())
        };
        assert!(!failed);
        self.record(&call, result);
        self.live.on_diagnostic_result(true);
    }

    fn restored(&self) -> RecoveryGovernor {
        let mut messages = self.messages.clone();
        messages.push(crate::task_origin::user_message(
            "Continue.",
            crate::task_origin::MessageOrigin::Runtime,
        ));
        let messages: Vec<Value> =
            serde_json::from_str(&serde_json::to_string(&messages).unwrap()).unwrap();
        let context = ExecVerificationContext::from_messages(Some(CHECK), &messages);
        let edits = EditFailureMemory::from_messages(&messages, &context);
        let failures = FailureMemory::from_recent_messages(&messages, Some(CHECK));
        assert!(
            failures.consecutive_failures > 0,
            "diagnostic progress must not erase the failed check"
        );
        let mut restored = RecoveryGovernor::restore_from_session(
            &failures,
            &messages,
            VerificationLevel::Behavioral,
        );
        restored.restore_pending_edits(&edits);
        restored.restore_focus(&messages, self.root.path().to_str(), Some(CHECK));
        assert!(
            restored.focus.is_pending(),
            "reading a source file is not proof of verification"
        );
        assert!(
            restored.focus.check_matches(CHECK),
            "original failed command must stay pending"
        );
        restored
    }

}

#[test]
fn completed_source_diagnosis_survives_handoff_without_clearing_verification() {
    for ranged in [false, true] {
        let mut trace = Trace::new();
        trace.failed_patch(SOURCE);
        trace.read(SOURCE, ranged);
        assert_eq!(trace.live.stage, Some(RecoveryStage::Fix));
        let restored = trace.restored();
        assert_eq!(
            restored.stage,
            Some(RecoveryStage::Fix),
            "a successful exact source read after the failed patch must survive handoff"
        );
        let patch = trace.call(
            "patch_file",
            json!({"path":SOURCE,"search":"return 1","replace":"return 2"}),
        );
        assert!(restored
            .maybe_block_tool(
                &patch,
                Some(CHECK),
                TaskHarness::infer("Fix existing source", false),
                false,
            )
            .is_none());
        assert_eq!(
            std::fs::read_to_string(trace.root.path().join(SOURCE)).unwrap(),
            "def value():\n    return 1\n\n# preserve this\n"
        );
    }
}

