use super::*;
use crate::agent_session::SessionBridge;
use serde_json::Value;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct SessionBridgeView {
    bridge: Option<SessionBridge>,
}

impl SessionBridgeView {
    pub(super) fn resolve(persisted: Option<&SessionBridge>, messages: &[Value]) -> Self {
        // Older multi-task sessions stored the bridge without its task boundary.
        let task = crate::task_origin::current_task_messages(messages);
        let bridge = persisted
            .filter(|_| {
                !messages[..messages.len() - task.len()]
                    .iter()
                    .any(crate::task_origin::is_human_request)
            })
            .cloned()
            .or_else(|| crate::agent_session::session_bridge_from_messages(task))
            .filter(|bridge| !bridge.is_empty());
        Self { bridge }
    }

    pub(super) fn telemetry_payload(&self) -> Option<Value> {
        let bridge = self.bridge.as_ref()?;
        Some(serde_json::json!({
            "accepted_strategies": bridge.accepted_strategies.len(),
            "repeated_dead_ends": bridge.repeated_dead_ends.len(),
            "has_last_good_verification": bridge.last_good_verification.is_some(),
        }))
    }

    pub(super) fn prompt(&self) -> Option<String> {
        let bridge = self.bridge.as_ref()?;
        let mut out = String::from("[Session Bridge]\n");
        if let Some(verification) = bridge.last_good_verification.as_ref() {
            out.push_str(&format!(
                "last_good_verification: {}\n",
                compact_one_line(verification.command.as_str(), 200)
            ));
        }
        for strategy in bridge.accepted_strategies.iter().take(3) {
            out.push_str(&format!(
                "accepted_strategy: {} -> {}",
                compact_one_line(strategy.wrong_assumption.as_str(), 120),
                compact_one_line(strategy.next_minimal_action.as_str(), 140),
            ));
            if !strategy.matched_command.trim().is_empty() {
                out.push_str(&format!(
                    " (matched by {})",
                    compact_one_line(strategy.matched_command.as_str(), 120)
                ));
            }
            if strategy.count > 1 {
                out.push_str(&format!(" [count={}]", strategy.count));
            }
            out.push('\n');
        }
        for dead_end in bridge.repeated_dead_ends.iter().take(3) {
            out.push_str(&format!(
                "repeated_dead_end: {} [count={}] reason={}\n",
                compact_one_line(dead_end.command.as_str(), 140),
                dead_end.count,
                compact_one_line(dead_end.reason.as_str(), 120)
            ));
        }
        out.push_str(
            "This is resumable operational memory from the previous session.\n\
Prefer an accepted strategy or the last good verification path before revisiting a repeated dead-end.\n\
If current tool output contradicts this bridge, trust the current evidence.",
        );
        Some(out)
    }

    pub(super) fn compact_prompt(&self) -> Option<String> {
        let bridge = self.bridge.as_ref()?;
        let mut lines = vec![
            "[Session Bridge cache]".to_string(),
            format!(
                "- last_good_verification: {}",
                bridge
                    .last_good_verification
                    .as_ref()
                    .map(|verification| compact_one_line(verification.command.as_str(), 100))
                    .unwrap_or_else(|| "-".to_string())
            ),
            format!(
                "- accepted_strategies: {}",
                bridge.accepted_strategies.len()
            ),
            format!("- repeated_dead_ends: {}", bridge.repeated_dead_ends.len()),
        ];
        if let Some(strategy) = bridge.accepted_strategies.first() {
            lines.push(format!(
                "- top_strategy: {}",
                compact_one_line(strategy.next_minimal_action.as_str(), 100)
            ));
        }
        if let Some(dead_end) = bridge.repeated_dead_ends.first() {
            lines.push(format!(
                "- top_dead_end: {}",
                compact_one_line(dead_end.command.as_str(), 100)
            ));
        }
        Some(lines.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_session::{
        SessionAcceptedStrategy, SessionDeadEnd, SessionVerificationMemory,
    };

    #[test]
    fn session_bridge_prompt_includes_resume_memory() {
        let bridge = SessionBridge {
            last_good_verification: Some(SessionVerificationMemory {
                command: "cargo test 2>&1".to_string(),
            }),
            accepted_strategies: vec![SessionAcceptedStrategy {
                wrong_assumption: "reading again would help".to_string(),
                next_minimal_action: "patch src/lib.rs with the smallest change".to_string(),
                matched_command: "patch_file(path=src/lib.rs)".to_string(),
                count: 2,
            }],
            repeated_dead_ends: vec![SessionDeadEnd {
                command: "read_file(path=src/lib.rs)".to_string(),
                reason: "repeat observation without enough new progress".to_string(),
                count: 3,
            }],
        };
        let view = SessionBridgeView::resolve(Some(&bridge), &[]);

        let prompt = view.prompt().expect("prompt");
        assert!(prompt.contains("[Session Bridge]"));
        assert!(prompt.contains("last_good_verification"));
        assert!(prompt.contains("accepted_strategy"));
        assert!(prompt.contains("repeated_dead_end"));
    }

    #[test]
    fn session_bridge_telemetry_counts_entries() {
        let bridge = SessionBridge {
            last_good_verification: Some(SessionVerificationMemory {
                command: "cargo test 2>&1".to_string(),
            }),
            accepted_strategies: vec![SessionAcceptedStrategy {
                wrong_assumption: "reading again would help".to_string(),
                next_minimal_action: "patch src/lib.rs".to_string(),
                matched_command: "patch_file(path=src/lib.rs)".to_string(),
                count: 1,
            }],
            repeated_dead_ends: vec![SessionDeadEnd {
                command: "read_file(path=src/lib.rs)".to_string(),
                reason: "repeat observation without enough new progress".to_string(),
                count: 2,
            }],
        };
        let view = SessionBridgeView::resolve(Some(&bridge), &[]);
        let telemetry = view.telemetry_payload().expect("telemetry");

        assert_eq!(telemetry["accepted_strategies"].as_u64(), Some(1));
        assert_eq!(telemetry["repeated_dead_ends"].as_u64(), Some(1));
        assert_eq!(
            telemetry["has_last_good_verification"].as_bool(),
            Some(true)
        );
    }
}
