// Append to src/tui/agent/recovery.rs in a disposable copy of the pinned runtime.
// Supply the retained r11 pilot session and workspace through these environment
// variables. This reads receipts only and makes no provider calls or edits.
#[cfg(test)]
mod r11_recovery_probe {
    use super::*;
    use crate::tui::agent::exec_verification::ExecVerificationContext;

    #[test]
    fn native_source_diagnosis_survives_a_restored_continuation() {
        let session = std::env::var("SPIRAL_DIAGNOSTIC_SESSION").unwrap();
        let root = std::env::var("SPIRAL_DIAGNOSTIC_WORKSPACE").unwrap();
        let saved: serde_json::Value =
            serde_json::from_slice(&std::fs::read(session).unwrap()).unwrap();
        let messages = saved["messages"].as_array().unwrap();
        let check = "python3 -S -m unittest discover -s tests -p test_coupled_pilot.py -v";
        let source = "scripts/run_coupled_pilot.py";
        let mut diagnosed = Vec::new();
        for (index, message) in messages.iter().enumerate() {
            if message["role"] != "tool"
                || !message["content"]
                    .as_str()
                    .is_some_and(|s| s.starts_with(&format!("[{source}]")))
            {
                continue;
            }
            let mut prefix = messages[..=index].to_vec();
            prefix.push(crate::task_origin::user_message(
                "Continue.",
                crate::task_origin::MessageOrigin::Runtime,
            ));
            let context = ExecVerificationContext::from_messages(Some(check), &prefix);
            let edits = EditFailureMemory::from_messages(&prefix, &context);
            if edits.count() == 0 || !edits.target_was_read() {
                continue;
            }
            let failures = FailureMemory::from_recent_messages(&prefix, Some(check));
            let mut restored = RecoveryGovernor::restore_from_session(
                &failures,
                &prefix,
                VerificationLevel::Behavioral,
            );
            restored.restore_pending_edits(&edits);
            restored.restore_focus(&prefix, Some(&root), Some(check));
            assert!(restored.focus.is_pending() && restored.focus.check_matches(check));
            println!(
                "call={} edits={} target_read={} focus_observed={} pending_check=true restored={}",
                message["tool_call_id"],
                edits.count(),
                edits.target_was_read(),
                restored.focus.observed(),
                restored.stage_label(),
            );
            diagnosed.push(restored.stage);
        }
        assert_eq!(diagnosed.len(), 3, "all three post-failure native reads retained");
        assert!(
            diagnosed.iter().all(|stage| *stage == Some(RecoveryStage::Fix)),
            "completed source diagnosis must survive without clearing the pending failed check"
        );
    }
}
