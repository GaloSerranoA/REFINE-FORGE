//! Proof report structure shared by runner and bundle exporter.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProofStatus {
    /// Policy gate passed AND `lake build` succeeded.
    Verified,
    /// `lake build` failed (Lean rejected the source).
    BuildFailed,
    /// Policy gate failed (e.g. `sorry` present). Build was not run.
    PolicyViolation,
    /// Tooling error (Lake not installed, file missing, etc).
    ToolingError,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProofReport {
    pub claim_id: String,
    pub status: ProofStatus,
    pub sorry_count: usize,
    pub admit_count: usize,
    pub axiom_count: usize,
    pub lean_toolchain: String,
    pub lean_module: String,
    pub stdout: String,
    pub stderr: String,
    pub policy_notes: Vec<String>,
    pub checked_at: DateTime<Utc>,
}
