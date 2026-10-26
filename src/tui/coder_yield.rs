//! Native slice outcomes used by both interactive and headless orchestration.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CoderYieldReason {
    VerificationFailure,
}
