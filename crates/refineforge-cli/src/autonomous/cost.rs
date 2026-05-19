//! Cumulative API-cost tracker for the autonomous driver.
//!
//! Fails closed at `--max-cost-usd`: any call to [`CostGate::charge`]
//! that would push the running total past the budget returns
//! [`CostGateError::Exceeded`] and the driver halts the run.

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostGate {
    pub max_usd: f64,
    pub spent_usd: f64,
}

#[derive(Debug, Error, PartialEq)]
pub enum CostGateError {
    #[error(
        "cost gate exceeded: budget ${max_usd:.4}, already spent ${spent_usd:.4}, proposed charge ${proposed_usd:.4}"
    )]
    Exceeded {
        max_usd: f64,
        spent_usd: f64,
        proposed_usd: f64,
    },
    #[error("negative cost ${proposed_usd:.4} is not allowed")]
    NegativeCharge { proposed_usd: f64 },
}

impl CostGate {
    pub fn new(max_usd: f64) -> Self {
        Self {
            max_usd,
            spent_usd: 0.0,
        }
    }

    /// Charge `proposed_usd` against the budget. Returns the new
    /// running total on success; `Err(Exceeded)` if the charge
    /// would push past `max_usd`.
    pub fn charge(&mut self, proposed_usd: f64) -> Result<f64, CostGateError> {
        if proposed_usd < 0.0 {
            return Err(CostGateError::NegativeCharge { proposed_usd });
        }
        if self.spent_usd + proposed_usd > self.max_usd {
            return Err(CostGateError::Exceeded {
                max_usd: self.max_usd,
                spent_usd: self.spent_usd,
                proposed_usd,
            });
        }
        self.spent_usd += proposed_usd;
        Ok(self.spent_usd)
    }

    pub fn remaining(&self) -> f64 {
        (self.max_usd - self.spent_usd).max(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_gate_remaining_equals_budget() {
        let g = CostGate::new(10.0);
        assert_eq!(g.remaining(), 10.0);
        assert_eq!(g.spent_usd, 0.0);
    }

    #[test]
    fn charge_accumulates_total() {
        let mut g = CostGate::new(10.0);
        let t1 = g.charge(0.5).unwrap();
        let t2 = g.charge(1.25).unwrap();
        assert!((t1 - 0.5).abs() < 1e-9);
        assert!((t2 - 1.75).abs() < 1e-9);
        assert!((g.remaining() - 8.25).abs() < 1e-9);
    }

    #[test]
    fn charge_at_exactly_budget_succeeds() {
        let mut g = CostGate::new(1.0);
        let t = g.charge(1.0).unwrap();
        assert!((t - 1.0).abs() < 1e-9);
        assert_eq!(g.remaining(), 0.0);
    }

    #[test]
    fn charge_over_budget_fails_closed() {
        let mut g = CostGate::new(1.0);
        g.charge(0.9).unwrap();
        let err = g.charge(0.2).unwrap_err();
        assert!(matches!(err, CostGateError::Exceeded { .. }));
        // Failed charges do NOT count against spent.
        assert!((g.spent_usd - 0.9).abs() < 1e-9);
    }

    #[test]
    fn negative_charge_rejected() {
        let mut g = CostGate::new(10.0);
        let err = g.charge(-0.01).unwrap_err();
        assert!(matches!(err, CostGateError::NegativeCharge { .. }));
        assert_eq!(g.spent_usd, 0.0);
    }

    #[test]
    fn zero_budget_rejects_any_positive_charge() {
        let mut g = CostGate::new(0.0);
        let err = g.charge(0.0001).unwrap_err();
        assert!(matches!(err, CostGateError::Exceeded { .. }));
    }

    #[test]
    fn zero_charge_on_zero_budget_succeeds_trivially() {
        let mut g = CostGate::new(0.0);
        let t = g.charge(0.0).unwrap();
        assert_eq!(t, 0.0);
    }
}
