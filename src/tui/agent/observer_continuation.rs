//! Bind the current Observer handoff to the first native Coder response.
use super::{check_catalog::Catalog, task_obligations, ToolCallData};
use crate::agent_session::AgentSession;
use crate::tui::suggestion::{
    self, ObserverResponseMode, ObserverSuggestionEnvelope, ObserverSuggestionKind,
};
use serde_json::Value;

#[derive(Clone)]
pub(crate) struct Observation {
    tool: &'static str,
    arguments: Value,
    wire_arguments: Value,
}

tokio::task_local! {
    static OBSERVATION: Option<Observation>;
}

pub(crate) async fn scope<F: std::future::Future>(
    action: Option<Observation>,
    future: F,
) -> F::Output {
    OBSERVATION.scope(action, future).await
}

pub(super) fn current() -> Option<Observation> {
    OBSERVATION.try_with(Clone::clone).ok().flatten()
}

impl Observation {
    pub(crate) fn from_advice(
        envelope: &ObserverSuggestionEnvelope,
        session: &AgentSession,
        test_command: Option<&str>,
        tool_root: &str,
    ) -> Option<Self> {
        let action = suggestion::primary_suggestion(envelope)?;
        if action.confidence < 0.75 || session.tool_root.as_deref() != Some(tool_root) {
            return None;
        }
        match (action.kind, action.suggested_tool.as_deref()) {
            (ObserverSuggestionKind::Read, Some("read_file")) => Some(Self {
                tool: "read_file",
                arguments: action.suggested_args.clone(),
                wire_arguments: action.suggested_args.clone(),
            }),
            (ObserverSuggestionKind::Verify, Some("exec")) => {
                Self::verification(&action.suggested_args, session, test_command, tool_root)
            }
            _ => None,
        }
    }

    fn verification(
        args: &Value,
        session: &AgentSession,
        test_command: Option<&str>,
        root: &str,
    ) -> Option<Self> {
        let obligations =
            task_obligations::project_advisory(&session.messages, test_command, Some(root));
        if obligations.required_diagnostic_read.is_some() {
            return None;
        }
        let mut selection = ToolCallData {
            id: String::new(),
            name: "exec".into(),
            arguments: args.to_string(),
            thought_signature: None,
        };
        let task = crate::task_origin::root_user_text(&session.messages);
        Catalog::from_root(task)
            .resolve(&mut selection, Some(root))
            .ok()?;
        let arguments: Value = serde_json::from_str(&selection.arguments).ok()?;
        let command = arguments["command"].as_str()?;
        let cwd = arguments["cwd"]
            .as_str()
            .or(session.cur_cwd.as_deref())
            .unwrap_or(root);
        let is_pending_exact = cwd == root
            && obligations
                .pending_exact_checks
                .iter()
                .any(|c| c == command)
            && obligations
                .required_recheck
                .as_deref()
                .is_none_or(|c| c == command)
            && super::exec_verification::ExecVerificationContext::from_messages(
                test_command,
                &session.messages,
            )
            .classify(command)
            .kind
                == super::ExecKind::Verify;
        let is_focused_comparison = obligations
            .diagnostic_followup
            .is_some_and(|followup| followup.matches_unittest_comparison(&arguments));
        (is_pending_exact || is_focused_comparison).then_some(Self {
            tool: "exec",
            arguments,
            wire_arguments: args.clone(),
        })
    }

    pub(crate) fn guidance(&self) -> String {
        format!(
            "[Current Observer observation]\nAccept this observation only if it advances the current task. Use the admitted plan. Put the local purpose in think.goal and keep the original goal and acceptance criteria if revising the steps. To use this handoff, give decision: accept in the Observer response and emit one native {} with exactly these arguments: {}. For exec, think.next may be @observer. Required diagnosis, scope, approval and verification still apply.",
            self.tool, self.wire_arguments,
        )
    }

    pub(super) fn matches_accepted_call(&self, call: &ToolCallData, text: &str) -> bool {
        call.name == self.tool
            && suggestion::explicit_response_decision(text) == Some(ObserverResponseMode::Accept)
            && serde_json::from_str::<Value>(&call.arguments)
                .is_ok_and(|arguments| arguments == self.arguments)
    }

    pub(super) fn command(&self) -> Option<&str> {
        self.arguments["command"].as_str()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const CHECK: &str = "cargo test --locked --bin spiral-coder -q";
    const ROOT: &str = "/workspace";
    const ACCEPT: &str = "[Observer response]\ndecision: accept";

    fn session(cwd: &str) -> AgentSession {
        AgentSession::new(
            Some(ROOT.into()),
            None,
            Some(cwd.into()),
            None,
            vec![
                json!({"role":"user","content":format!("Repair the application.\nRun exactly: {CHECK}")}),
            ],
        )
    }

    fn advice(kind: &str, tool: &str, args: Value, confidence: f32) -> ObserverSuggestionEnvelope {
        serde_json::from_value(json!({
            "summary":"One observation","suggestions":[{"kind":kind,"reason":"Observe the current result",
                "confidence":confidence,"suggested_tool":tool,"suggested_args":args}]
        }))
        .unwrap()
    }

    fn exec(args: Value) -> ToolCallData {
        ToolCallData {
            id: "selected".into(),
            name: "exec".into(),
            arguments: args.to_string(),
            thought_signature: None,
        }
    }

    #[test]
    fn selected_check_keeps_wire_shape_and_requires_exact_resolved_call() {
        let env = advice("verify", "exec", json!({"check_id":"required-1"}), 0.95);
        let observation = Observation::from_advice(&env, &session(ROOT), None, ROOT).unwrap();
        assert!(observation
            .guidance()
            .contains(r#"{"check_id":"required-1"}"#));
        let arguments = json!({"check_id":"required-1","command":CHECK,"cwd":ROOT});
        assert!(observation.matches_accepted_call(&exec(arguments.clone()), ACCEPT));
        assert!(!observation.matches_accepted_call(&exec(arguments.clone()), "decision: override"));
        assert!(!observation.matches_accepted_call(&exec(arguments), "Proceed"));
        assert!(!observation.matches_accepted_call(
            &exec(json!({"check_id":"required-1","command":CHECK,"cwd":"/other"})),
            ACCEPT
        ));
    }

    #[test]
    fn unrelated_commands_uncertain_advice_and_new_roots_supply_no_observation() {
        for (kind, tool, args, confidence, cwd, root) in [
            (
                "verify",
                "exec",
                json!({"command":"git status"}),
                0.95,
                ROOT,
                ROOT,
            ),
            (
                "verify",
                "exec",
                json!({"command":CHECK}),
                0.95,
                "/other",
                ROOT,
            ),
            (
                "read",
                "read_file",
                json!({"path":"src/lib.rs"}),
                0.74,
                ROOT,
                ROOT,
            ),
            (
                "read",
                "read_file",
                json!({"path":"src/lib.rs"}),
                0.95,
                ROOT,
                "/other",
            ),
            (
                "edit",
                "patch_file",
                json!({"path":"src/lib.rs","search":"x","replace":"y"}),
                0.95,
                ROOT,
                ROOT,
            ),
        ] {
            let env = advice(kind, tool, args, confidence);
            assert!(Observation::from_advice(&env, &session(cwd), None, root).is_none());
        }
    }
}
