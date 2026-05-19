//! Planner — turns a claim id into a sequence of steps the
//! [`crate::autonomous::Executor`] will run.
//!
//! Phase 3 MVP plans a baseline workflow:
//!
//! 1. **LeanCheck** — `refine lean check <id>` (no escalation).
//! 2. **Scan** — `refine scan check <id>` (no escalation).
//! 3. **BundleExport** — `refine bundle export <id>`
//!    (no escalation under v0.3; bundle creation is mechanical).
//!
//! Plus optional "engine actions" — categorised [`Action`]s the
//! AI proposes that go through the engine's escalation logic
//! before they're applied. The MVP doesn't generate these
//! itself (Phase 3.5 will, via the LLM strategy); the planner
//! exposes `with_engine_action` so tests and integration paths
//! can drive the escalation flow end-to-end.

use refineforge_escalation::Action;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "step_kind")]
pub enum StepKind {
    /// `refine lean check <claim_id>`.
    LeanCheck,
    /// `refine scan check <claim_id>`.
    Scan,
    /// `refine bundle export <claim_id>`.
    BundleExport,
    /// An AI-proposed [`Action`] routed through the escalation
    /// engine. Phase 3 MVP doesn't generate these from a live
    /// LLM (Phase 3.5 will); tests and dry-runs inject them.
    EngineAction(Action),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlannedStep {
    pub seq: u32,
    pub kind: StepKind,
    pub rationale: String,
}

#[derive(Debug, Default, Clone)]
pub struct Planner {
    extra_engine_actions: Vec<Action>,
}

impl Planner {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert an AI-proposed [`Action`] into the plan immediately
    /// after the LeanCheck step (i.e. it'll run before scan / bundle).
    /// Used by tests + dry-runs to exercise the escalation path.
    pub fn with_engine_action(mut self, action: Action) -> Self {
        self.extra_engine_actions.push(action);
        self
    }

    /// Produce the linear step list for `claim_id`.
    pub fn plan(&self, claim_id: &str) -> Vec<PlannedStep> {
        let mut out = Vec::new();
        let mut seq = 1u32;
        out.push(PlannedStep {
            seq,
            kind: StepKind::LeanCheck,
            rationale: format!(
                "verify {} compiles + passes no-sorry / no-axiom policy gate",
                claim_id
            ),
        });
        seq += 1;
        for a in &self.extra_engine_actions {
            out.push(PlannedStep {
                seq,
                kind: StepKind::EngineAction(a.clone()),
                rationale:
                    "AI-proposed action — routes through the escalation engine first".into(),
            });
            seq += 1;
        }
        out.push(PlannedStep {
            seq,
            kind: StepKind::Scan,
            rationale: format!(
                "confirm every rust_source entity cited by {} exists in the cited file",
                claim_id
            ),
        });
        seq += 1;
        out.push(PlannedStep {
            seq,
            kind: StepKind::BundleExport,
            rationale: format!("seal {} into a SHA-256 manifested verification bundle", claim_id),
        });
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use refineforge_escalation::{Action, LossKind};

    #[test]
    fn baseline_plan_is_three_steps() {
        let p = Planner::new().plan("EXAMPLE-001");
        assert_eq!(p.len(), 3);
    }

    #[test]
    fn baseline_plan_steps_are_lean_then_scan_then_bundle() {
        let p = Planner::new().plan("EXAMPLE-001");
        assert_eq!(p[0].kind, StepKind::LeanCheck);
        assert_eq!(p[1].kind, StepKind::Scan);
        assert_eq!(p[2].kind, StepKind::BundleExport);
    }

    #[test]
    fn baseline_plan_sequence_numbers_are_dense_one_based() {
        let p = Planner::new().plan("EXAMPLE-001");
        for (i, step) in p.iter().enumerate() {
            assert_eq!(step.seq as usize, i + 1);
        }
    }

    #[test]
    fn engine_action_is_inserted_between_lean_and_scan() {
        let action = Action::MapRustToLean {
            rust_type: "u64".into(),
            lean_type: "Nat".into(),
            lossy_kinds: vec![LossKind::UnsignedOverflow],
        };
        let p = Planner::new().with_engine_action(action).plan("EXAMPLE-002");
        assert_eq!(p.len(), 4);
        assert_eq!(p[0].kind, StepKind::LeanCheck);
        assert!(matches!(p[1].kind, StepKind::EngineAction(_)));
        assert_eq!(p[2].kind, StepKind::Scan);
        assert_eq!(p[3].kind, StepKind::BundleExport);
    }

    #[test]
    fn multiple_engine_actions_run_in_insertion_order() {
        let a1 = Action::Reformat {
            paths: vec!["x".into()],
        };
        let a2 = Action::Reformat {
            paths: vec!["y".into()],
        };
        let p = Planner::new()
            .with_engine_action(a1.clone())
            .with_engine_action(a2.clone())
            .plan("X");
        if let StepKind::EngineAction(act) = &p[1].kind {
            assert_eq!(act, &a1);
        } else {
            panic!("expected first injected action at step 2");
        }
        if let StepKind::EngineAction(act) = &p[2].kind {
            assert_eq!(act, &a2);
        } else {
            panic!("expected second injected action at step 3");
        }
    }

    #[test]
    fn rationale_mentions_the_claim_id() {
        let p = Planner::new().plan("MYPROJ-AUTH-001");
        for step in &p {
            // At least one of the rationale strings names the claim;
            // the LeanCheck / Scan / Bundle ones all do.
            // The EngineAction rationale is generic by design.
            if matches!(
                step.kind,
                StepKind::LeanCheck | StepKind::Scan | StepKind::BundleExport
            ) {
                assert!(
                    step.rationale.contains("MYPROJ-AUTH-001"),
                    "rationale should name the claim: {}",
                    step.rationale
                );
            }
        }
    }
}
