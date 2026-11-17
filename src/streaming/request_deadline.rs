//! One logical provider request may span endpoint fallbacks and connect
//! retries. Each attempt is bounded by `timeout_seconds`, but without an
//! overall cap a dead route multiplies that bound across URLs and retries
//! (~20min of in-flight silence). The whole attempt set shares one
//! wall-clock deadline: one full attempt plus the retry backoff chain, so
//! fast-failing endpoints keep their retries while a silent route cannot
//! multiply the per-attempt timeout.
use std::time::{Duration, Instant};

use crate::config::RunConfig;

#[derive(Clone, Copy)]
pub(super) struct RequestDeadline {
    deadline: Instant,
}

impl RequestDeadline {
    pub(super) fn start(cfg: &RunConfig, max_retries: usize) -> Self {
        let backoff: Duration = (0..max_retries)
            .map(|attempt| super::backoff_delay(&cfg.provider, None, attempt, None))
            .sum();
        Self {
            deadline: Instant::now() + Duration::from_secs(cfg.timeout_seconds) + backoff,
        }
    }

    fn remaining(&self) -> Duration {
        self.deadline.saturating_duration_since(Instant::now())
    }

    /// Per-attempt timeout for the next send, or `None` when the shared
    /// budget is already spent.
    pub(super) fn attempt_timeout(&self, cfg: &RunConfig) -> Option<Duration> {
        let remaining = self.remaining();
        (!remaining.is_zero()).then(|| remaining.min(Duration::from_secs(cfg.timeout_seconds)))
    }

    /// Retry backoff capped by the remaining shared budget.
    pub(super) async fn sleep(&self, delay: Duration) {
        tokio::time::sleep(delay.min(self.remaining())).await;
    }
}
