pub mod analyzer;
pub mod benchmark_plan;
pub mod coder_diagnostic;
pub mod detector;
pub mod engine;
pub mod memory;
pub mod repo_rules;
pub mod scorer;

#[cfg(test)]
mod outcome_tests;

use crate::observer::benchmark_plan::BenchmarkPlan;
use crate::observer::coder_diagnostic::CoderDiagnostic;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Warn,
    Crit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RiskAxis {
    Correctness,
    Security,
    Reliability,
    Performance,
    Maintainability,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Risk {
    pub axis: RiskAxis,
    pub severity: Severity,
    pub description: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DevPhase {
    Core,
    Feature,
    Polish,
    Any,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Cost {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProposalStatus {
    New,
    Unresolved,
    Escalated,
    Addressed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Proposal {
    pub title: String,
    pub to_coder: String,
    pub severity: Severity,
    pub score: u32,
    pub phase: DevPhase,
    pub impact: String,
    pub cost: Cost,
    pub status: ProposalStatus,
    pub quote: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub axis: Option<RiskAxis>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthScore {
    pub score: u32,
    pub rationale: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Critique {
    pub summary: String,

    #[serde(default)]
    pub risks: Vec<Risk>,

    #[serde(default)]
    pub proposals: Vec<Proposal>,

    pub phase: DevPhase,
    pub critical_path: String,
    pub health: HealthScore,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coder_diagnostic: Option<CoderDiagnostic>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub benchmark_plan: Option<BenchmarkPlan>,
}
