//! Newest-failure routing and exact-check ownership are independent. Retain
//! older failures until their own native check succeeds; reads are not proof.
use super::*;

impl RecoveryFocus {
    pub(super) fn contains_check(&self, command: &str) -> bool {
        self.pending
            .iter()
            .chain(&self.deferred)
            .any(|f| f.check.trim() == command.trim())
    }

    pub(super) fn resolve(&mut self, command: &str) {
        self.deferred.retain(|f| f.check.trim() != command.trim());
        if self.check_matches(command) {
            self.pending = self.deferred.pop();
        }
    }

    pub(super) fn mark_repair(&mut self) {
        if let Some(focus) = self.pending.as_mut() {
            focus.observation = Observation::RepairAttempted;
        }
        for focus in &mut self.deferred {
            if matches!(
                focus.observation,
                Observation::Observed | Observation::RepairAttempted
            ) {
                focus.observation = Observation::RepairAttempted;
            }
        }
    }

    pub(super) fn record_failure(
        &mut self,
        check: &str,
        body: &str,
        root: Option<&str>,
        edit: Option<&ToolCallData>,
    ) {
        if check.trim().is_empty() || check.len() > 4096 {
            return;
        }
        let previous = self
            .pending
            .iter()
            .chain(&self.deferred)
            .find(|f| f.check.trim() == check.trim());
        let location = match baseline_failure::diagnostic(body, root) {
            Ok(Some(location)) => Some(location),
            Ok(None) => {
                // Preserve the narrow JSON/explicit-path fallback only when no
                // native Rust/Python locations contradict it.
                edit.and_then(|tc| {
                    localize(
                        tc,
                        &format!("[auto-test] failed\n{body}"),
                        root,
                        Some(check),
                    )
                    .map(|f| (f.path, f.diagnostic))
                })
                .or_else(|| {
                    let old = previous?;
                    (!old.path.is_empty())
                        .then(|| localized_diagnostic(&old.path, body, root))
                        .flatten()
                        .map(|diagnostic| (old.path.clone(), diagnostic))
                })
            }
            Err(()) => None,
        };
        let focus = if let Some((path, diagnostic)) = location {
            FailureFocus {
                path,
                diagnostic,
                check: check.trim().into(),
                observation: Observation::AwaitingRead,
                location: LocationEvidence::Current,
            }
        } else {
            // Initially unlocalized failures keep the generic governor. Once a
            // focus exists, a newer unlocalized failure must suppress stale reads.
            if self.pending.is_none() {
                return;
            }
            let path = previous.map(|f| f.path.clone()).unwrap_or_default();
            let location = if path.is_empty() {
                LocationEvidence::Unknown
            } else {
                LocationEvidence::LastConfirmed
            };
            FailureFocus {
                path,
                diagnostic: fresh_diagnostic(body),
                check: check.trim().into(),
                observation: Observation::Unlocalized,
                location,
            }
        };
        self.deferred.retain(|f| f.check.trim() != check.trim());
        if let Some(old) = self.pending.replace(focus) {
            if old.check.trim() != check.trim() {
                self.deferred.push(old);
            }
        }
    }
}
