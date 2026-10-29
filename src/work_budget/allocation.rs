//! Allocation is frozen before the first wire request; classification is bounded.
use super::*;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct BudgetPolicy {
    pub transport_repair_limit: usize,
    pub observer_reserved_requests: usize,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RequestCharge {
    #[default]
    Task,
    Transport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TransportRejection {
    NativeArguments,
    ObserverEnvelope,
}

impl WorkState {
    pub(crate) fn task_requests(&self) -> usize {
        self.requests
            .iter()
            .filter(|r| r.charge == RequestCharge::Task)
            .count()
    }

    fn role_tasks(&self, role: &str) -> usize {
        self.requests
            .iter()
            .filter(|r| r.role == role && r.charge == RequestCharge::Task)
            .count()
    }

    pub(super) fn coder_remaining(&self) -> usize {
        self.request_limit
            .saturating_sub(self.policy.observer_reserved_requests)
            .saturating_sub(self.role_tasks("coder"))
            .min(self.request_limit.saturating_sub(self.task_requests()))
    }
}

impl WorkBudget {
    pub(crate) fn configure(&self, policy: BudgetPolicy) -> Result<()> {
        let mut state = self.state.lock().unwrap();
        anyhow::ensure!(
            state.requests.is_empty(),
            "budget policy is frozen after the first request"
        );
        anyhow::ensure!(
            policy.transport_repair_limit <= 8,
            "transport repair limit must be 0..8"
        );
        anyhow::ensure!(
            policy.observer_reserved_requests < state.request_limit,
            "Observer reservation must leave a Coder request"
        );
        state.policy = policy;
        self.persist(&mut state)
    }

    pub(crate) fn remaining_for(&self, role: &str) -> usize {
        let state = self.state.lock().unwrap();
        if role == "coder" {
            state.coder_remaining()
        } else {
            state.request_limit.saturating_sub(state.task_requests())
        }
    }

    pub(crate) fn slice_remaining(&self, role: &str) -> usize {
        let remaining = self
            .report_closeout_ceiling
            .map_or(self.remaining_for(role), |ceiling| {
                self.remaining_for(role)
                    .min(ceiling.saturating_sub(self.snapshot().requests.len()))
            });
        self.request_ceiling.map_or(remaining, |ceiling| {
            remaining.min(ceiling.saturating_sub(self.snapshot().task_requests()))
        })
    }

    pub(crate) fn transport_remaining(&self) -> usize {
        let state = self.state.lock().unwrap();
        state
            .policy
            .transport_repair_limit
            .saturating_sub(state.requests.len() - state.task_requests())
    }

    /// Distribute remaining Coder work over the protected Observer opportunities.
    /// Success may stop early: a reservation is never a requirement to waste calls.
    pub(crate) fn paced_coder_allowance(&self, suggested: usize) -> usize {
        let state = self.state.lock().unwrap();
        let reserved = state
            .policy
            .observer_reserved_requests
            .saturating_sub(state.role_tasks("observer"));
        let remaining = state.coder_remaining();
        if reserved == 0 {
            suggested.min(remaining)
        } else {
            suggested.min(remaining.div_ceil(reserved)).min(remaining)
        }
    }

    /// Charge a completed, wholly rejected response to the transport pool.
    /// The caller supplies the cursor captured before this response. Only one
    /// matching completed wire attempt is eligible; HTTP retries, interruption,
    /// semantic rejection, proof failures and executed actions stay task work.
    /// Receipts are never removed and the hard wire cap stays task + transport.
    pub(crate) fn reject_transport(
        &self,
        role: &str,
        cursor: usize,
        reason: TransportRejection,
    ) -> Result<bool> {
        let mut state = self.state.lock().unwrap();
        if state.requests.len() - state.task_requests() >= state.policy.transport_repair_limit {
            return Ok(false);
        }
        let ids: Vec<_> = state
            .requests
            .iter()
            .enumerate()
            .skip(cursor)
            .filter(|(_, r)| r.role == role)
            .map(|(i, _)| i)
            .collect();
        let [id] = ids.as_slice() else {
            return Ok(false);
        };
        let receipt = &mut state.requests[*id];
        if receipt.status != "completed" || receipt.charge != RequestCharge::Task {
            return Ok(false);
        }
        receipt.charge = RequestCharge::Transport;
        receipt.transport_rejection = Some(reason);
        self.persist(&mut state)?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejected_transport_is_bounded_persisted_and_never_erases_wire_attempts() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("budget.json");
        let budget = WorkBudget::new("task", 4, Some(path.clone()));
        budget
            .configure(BudgetPolicy {
                transport_repair_limit: 2,
                observer_reserved_requests: 1,
            })
            .unwrap();
        for i in 0..3 {
            let request = budget.begin("coder", &Value::Null).unwrap();
            request.status("completed");
            drop(request);
            assert_eq!(
                budget
                    .reject_transport("coder", i, TransportRejection::NativeArguments)
                    .unwrap(),
                i < 2
            );
        }
        let restored = WorkBudget::load("task", path).unwrap();
        assert_eq!(restored.snapshot().requests.len(), 3);
        assert_eq!(restored.remaining(), 3);
        assert_eq!(restored.remaining_for("coder"), 2);
        assert!(restored.configure(BudgetPolicy::default()).is_err());
        for _ in 0..2 {
            drop(restored.begin("coder", &Value::Null).unwrap());
        }
        assert!(restored
            .begin("coder", &Value::Null)
            .err()
            .unwrap()
            .is::<ChunkExhausted>());
        drop(restored.begin("observer", &Value::Null).unwrap());
        assert_eq!(restored.snapshot().requests.len(), 6);
        assert!(restored.begin("observer", &Value::Null).is_err());
    }

    #[test]
    fn observer_share_is_inaccessible_to_coder_and_paced_across_the_episode() {
        let budget = WorkBudget::new("task", 32, None);
        budget
            .configure(BudgetPolicy {
                transport_repair_limit: 4,
                observer_reserved_requests: 8,
            })
            .unwrap();
        for _ in 0..8 {
            assert_eq!(budget.paced_coder_allowance(8), 3);
            for _ in 0..3 {
                drop(budget.begin("coder", &Value::Null).unwrap());
            }
            drop(budget.begin("observer", &Value::Null).unwrap());
        }
        assert_eq!(budget.remaining(), 0);
        assert_eq!(budget.transport_remaining(), 4);
        assert!(budget.begin("coder", &Value::Null).is_err());
    }

    #[test]
    fn uncertain_http_or_multi_attempt_responses_do_not_gain_transport_credit() {
        let budget = WorkBudget::new("task", 8, None);
        budget
            .configure(BudgetPolicy {
                transport_repair_limit: 4,
                observer_reserved_requests: 2,
            })
            .unwrap();
        let first = budget.begin("observer", &Value::Null).unwrap();
        first.status("http_200");
        drop(first);
        assert!(!budget
            .reject_transport("observer", 0, TransportRejection::ObserverEnvelope)
            .unwrap());
        let second = budget.begin("observer", &Value::Null).unwrap();
        second.status("completed");
        drop(second);
        assert!(!budget
            .reject_transport("observer", 0, TransportRejection::ObserverEnvelope)
            .unwrap());
        assert!(budget
            .reject_transport("observer", 1, TransportRejection::ObserverEnvelope)
            .unwrap());
        assert!(!budget
            .reject_transport("observer", 1, TransportRejection::ObserverEnvelope)
            .unwrap());
        assert_eq!(budget.remaining(), 7);
    }
}
