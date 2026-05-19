//! `RunReport` — the final JSON the autonomous driver writes
//! summarising every step taken, every escalation generated,
//! and the run-level outcome.
//!
//! Consumed by tests, by `refine escalations list` (the
//! queue-inspection command), and by the operator at the end
//! of a run.

use crate::autonomous::executor::StepOutcome;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunReport {
    pub claim_id: String,
    pub criteria_version: String,
    pub started_at: String,
    pub finished_at: String,
    pub dry_run: bool,
    pub strategy: String,
    pub operator: Option<String>,
    pub summary: RunSummary,
    pub steps: Vec<StepOutcome>,
    pub cost_usd_total: f64,
    pub cost_usd_max: f64,
    /// Anthropic API usage as reported by the API itself.
    /// Surfaced for post-run reporting; the cost-gate's
    /// upfront $0.07/attempt estimate remains authoritative
    /// for budget control. `None` when no Anthropic call ran.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anthropic_usage: Option<refineforge_strategies::UsageStats>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct RunSummary {
    pub total_steps: u32,
    pub proceeded: u32,
    pub escalated: u32,
    pub failed: u32,
    /// True iff every step finished with a non-failure outcome.
    pub success: bool,
}

impl RunSummary {
    pub fn from_outcomes(outcomes: &[StepOutcome]) -> Self {
        let mut s = Self::default();
        s.total_steps = outcomes.len() as u32;
        for o in outcomes {
            match o {
                StepOutcome::Proceeded { .. } => s.proceeded += 1,
                StepOutcome::Escalated { .. } => s.escalated += 1,
                StepOutcome::Failed { .. } => s.failed += 1,
            }
        }
        s.success = s.failed == 0;
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::autonomous::executor::StepOutcome;

    fn proceeded() -> StepOutcome {
        StepOutcome::Proceeded {
            seq: 1,
            kind: "LeanCheck".into(),
            detail: "ok".into(),
            elapsed_ms: 10,
        }
    }

    fn escalated() -> StepOutcome {
        StepOutcome::Escalated {
            seq: 2,
            kind: "EngineAction".into(),
            category: "idealisation".into(),
            packet_path: "escalations/X/p.md".into(),
            elapsed_ms: 5,
        }
    }

    fn failed() -> StepOutcome {
        StepOutcome::Failed {
            seq: 3,
            kind: "BundleExport".into(),
            error: "permission denied".into(),
            elapsed_ms: 1,
        }
    }

    #[test]
    fn summary_counts_each_outcome_kind() {
        let s = RunSummary::from_outcomes(&[proceeded(), escalated(), failed()]);
        assert_eq!(s.total_steps, 3);
        assert_eq!(s.proceeded, 1);
        assert_eq!(s.escalated, 1);
        assert_eq!(s.failed, 1);
        assert!(!s.success);
    }

    #[test]
    fn summary_all_proceeded_is_success() {
        let s = RunSummary::from_outcomes(&[proceeded(), proceeded()]);
        assert!(s.success);
    }

    #[test]
    fn summary_escalated_alone_is_success() {
        // An escalation isn't a failure — it's the contract working.
        let s = RunSummary::from_outcomes(&[escalated()]);
        assert!(s.success);
        assert_eq!(s.escalated, 1);
    }

    #[test]
    fn report_round_trips_via_json() {
        let r = RunReport {
            claim_id: "EXAMPLE-002".into(),
            criteria_version: "0.3".into(),
            started_at: "2026-05-18T20:30:00Z".into(),
            finished_at: "2026-05-18T20:32:00Z".into(),
            dry_run: true,
            strategy: "mock".into(),
            operator: Some("galo@serragi.com".into()),
            summary: RunSummary::from_outcomes(&[proceeded(), escalated()]),
            steps: vec![proceeded(), escalated()],
            cost_usd_total: 0.0,
            cost_usd_max: 10.0,
            anthropic_usage: None,
        };
        let j = serde_json::to_string(&r).expect("ser");
        let back: RunReport = serde_json::from_str(&j).expect("de");
        assert_eq!(back.claim_id, "EXAMPLE-002");
        assert_eq!(back.summary, r.summary);
        assert_eq!(back.steps.len(), 2);
    }

    #[test]
    fn report_with_anthropic_usage_round_trips() {
        let r = RunReport {
            claim_id: "X".into(),
            criteria_version: "0.3".into(),
            started_at: "t1".into(),
            finished_at: "t2".into(),
            dry_run: false,
            strategy: "anthropic".into(),
            operator: None,
            summary: RunSummary::default(),
            steps: Vec::new(),
            cost_usd_total: 0.35,
            cost_usd_max: 1.0,
            anthropic_usage: Some(refineforge_strategies::UsageStats {
                calls: 4,
                input_tokens: 2000,
                output_tokens: 400,
                cache_creation_input_tokens: 100,
                cache_read_input_tokens: 1500,
            }),
        };
        let j = serde_json::to_string(&r).expect("ser");
        let back: RunReport = serde_json::from_str(&j).expect("de");
        let u = back.anthropic_usage.expect("usage round-trips");
        assert_eq!(u.calls, 4);
        assert_eq!(u.input_tokens, 2000);
    }
}
