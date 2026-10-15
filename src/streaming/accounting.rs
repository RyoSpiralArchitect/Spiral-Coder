//! Count every wire attempt, not retained transcript length.
use super::*;

pub(super) struct RequestMeter {
    tx: mpsc::Sender<StreamToken>,
    started: std::time::Instant,
    estimated_input_tokens: usize,
    usage: Option<serde_json::Value>,
    status: String,
    guard: Option<crate::work_budget::RequestGuard>,
}

impl RequestMeter {
    pub async fn begin(
        payload: &serde_json::Value,
        tx: &mpsc::Sender<StreamToken>,
    ) -> Result<Self> {
        let guard = crate::work_budget::begin(payload)?;
        let estimated_input_tokens = payload.to_string().chars().count().div_ceil(4);
        let _ = tx
            .send(StreamToken::Telemetry(TelemetryEvent {
                event: "request_started".into(),
                data: json!({"estimated_input_tokens":estimated_input_tokens}),
            }))
            .await;
        Ok(Self {
            tx: tx.clone(),
            started: std::time::Instant::now(),
            estimated_input_tokens,
            usage: None,
            status: "interrupted_or_failed".into(),
            guard,
        })
    }
    pub fn usage(&mut self, usage: &serde_json::Value) {
        if usage.is_null() {
            return;
        }
        self.usage = Some(usage.clone());
        if let Some(guard) = &self.guard {
            guard.usage(usage);
        }
    }
    pub fn status(&mut self, status: &str) {
        self.status = status.into();
        if let Some(guard) = &self.guard {
            guard.status(status);
        }
    }
}
impl Drop for RequestMeter {
    fn drop(&mut self) {
        let _ = self.tx.try_send(StreamToken::Telemetry(TelemetryEvent {
            event: "request_usage".into(), data: json!({
                "estimated_input_tokens": self.estimated_input_tokens,
                "provider_usage":self.usage, "duration_ms":self.started.elapsed().as_millis(), "status":self.status,
            }),
        }));
    }
}
