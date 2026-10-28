//! Separate temporary HTTP throttling from explicit account exhaustion.
use anyhow::Error;
use reqwest::StatusCode;

pub(super) fn retryable(status: StatusCode, body: &str) -> bool {
    if !super::is_retryable_status(status) {
        return false;
    }
    if status != StatusCode::TOO_MANY_REQUESTS {
        return true;
    }
    let Ok(value) = serde_json::from_str::<serde_json::Value>(body) else {
        return true;
    };
    // Only explicit structured provider codes establish account exhaustion.
    // A prose mention, an unknown code or a generic 429 retains bounded retries.
    !["code", "type"].iter().any(|field| {
        matches!(
            value["error"][field].as_str(),
            Some("insufficient_quota" | "credit_balance_exhausted")
        )
    })
}

pub(super) fn retain_previous(error: Error, previous: Option<&Error>) -> Error {
    match previous {
        Some(previous) => error.context(format!(
            "Provider request could not continue after the previous failure: {previous:#}"
        )),
        None => error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quota_codes_stop_retries_but_prose_and_transient_errors_do_not() {
        for field in ["code", "type"] {
            for code in ["insufficient_quota", "credit_balance_exhausted"] {
                let body = serde_json::json!({"error":{(field):code}}).to_string();
                assert!(!retryable(StatusCode::TOO_MANY_REQUESTS, &body));
            }
        }
        for body in [
            "",
            "not JSON",
            r#"{"error":{"message":"insufficient_quota is one possible cause"}}"#,
            r#"{"error":{"code":"rate_limit_exceeded"}}"#,
            r#"{"error":{"code":"unknown"}}"#,
        ] {
            assert!(retryable(StatusCode::TOO_MANY_REQUESTS, body));
        }
        assert!(retryable(StatusCode::SERVICE_UNAVAILABLE, ""));
        assert!(retryable(StatusCode::REQUEST_TIMEOUT, ""));
        assert!(!retryable(StatusCode::UNAUTHORIZED, ""));
    }

    #[test]
    fn previous_http_context_does_not_erase_the_typed_budget_error() {
        let previous = anyhow::anyhow!("HTTP 429: rate_limit_exceeded");
        let error = retain_previous(crate::work_budget::ChunkExhausted.into(), Some(&previous));
        assert!(error.is::<crate::work_budget::ChunkExhausted>());
        let description = format!("{error:#}");
        assert!(description.contains("HTTP 429"));
        assert!(description.contains("chunk exhausted"));
    }
}
