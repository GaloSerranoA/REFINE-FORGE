//! Executor — runs the [`PlannedStep`] sequence the
//! [`Planner`] produced.
//!
//! Per step:
//! - **System steps** (LeanCheck / Scan / BundleExport) — Phase
//!   3 MVP records what would happen; the real subprocess /
//!   library-call wiring is deferred to Phase 3.5 (today the
//!   driver is the orchestration scaffold).
//! - **Engine actions** — the AI's proposed [`Action`] is sent
//!   through [`Engine::decide`]. On `Decision::Proceed` the
//!   step is recorded as proceeded. On `Decision::Escalate`,
//!   a [`Packet`] is built and (unless `dry_run`) committed
//!   via the [`GitOps`] handle the caller provided.
//!
//! Per criteria v0.3, the executor does **not** auto-reject
//! pending escalations. The Phase 2 `await_decision` poll is
//! reachable via [`Executor::await_packet`] but the MVP's
//! end-to-end test path is `dry_run = true` so we don't block
//! waiting for an operator commit in the test suite.

use crate::autonomous::cost::CostGate;
use crate::autonomous::planner::{PlannedStep, StepKind};
use refineforge_escalation::{
    commit_packet, AwaitConfig, Decision, Engine, GitOps, Packet, ProjectContext,
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Instant;
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "outcome")]
pub enum StepOutcome {
    /// Step finished without tripping any escalation category.
    Proceeded {
        seq: u32,
        kind: String,
        detail: String,
        elapsed_ms: u64,
    },
    /// Step tripped at least one category; a packet was (or
    /// would be — in dry-run) written and the executor halts
    /// pending an operator decision.
    Escalated {
        seq: u32,
        kind: String,
        category: String,
        packet_path: String,
        elapsed_ms: u64,
    },
    /// Step failed for a non-escalation reason (cost-gate
    /// exceeded, I/O error, etc.).
    Failed {
        seq: u32,
        kind: String,
        error: String,
        elapsed_ms: u64,
    },
}

#[derive(Debug, Error)]
pub enum ExecuteError {
    #[error("cost gate exceeded: {0}")]
    CostExceeded(String),
    #[error("escalation engine refused: {0}")]
    EngineRefused(String),
    #[error("git/packet error: {0}")]
    GitCheckpoint(String),
    #[error("I/O error: {0}")]
    Io(String),
}

pub struct Executor<G: GitOps> {
    pub engine: Engine,
    pub git: G,
    pub repo_root: PathBuf,
    pub claim_id: String,
    pub strategy: String,
    pub operator: Option<String>,
    pub dry_run: bool,
    pub project_ctx: ProjectContext,
    pub cost_gate: CostGate,
    pub generated_at: String,
}

impl<G: GitOps> Executor<G> {
    /// Convenience: a no-op executor that uses [`MockGitOps`]
    /// + an empty `ProjectContext` + zero cost-gate budget. Used
    /// by tests that want to drive only the planner / outcome
    /// surface without touching disk.
    pub fn for_tests(git: G, repo_root: impl Into<PathBuf>) -> Self {
        Self {
            engine: Engine::new(),
            git,
            repo_root: repo_root.into(),
            claim_id: "TEST-CLAIM".into(),
            strategy: "mock".into(),
            operator: None,
            dry_run: true,
            project_ctx: ProjectContext::test_default(),
            cost_gate: CostGate::new(0.0),
            generated_at: "2026-05-18T00:00:00Z".into(),
        }
    }

    /// Run a single [`PlannedStep`]. Returns the [`StepOutcome`]
    /// — propagation is the caller's responsibility (the
    /// orchestrator wraps the loop and decides whether to halt
    /// on Escalated / Failed).
    pub fn run_step(&mut self, step: &PlannedStep) -> StepOutcome {
        let started = Instant::now();
        match &step.kind {
            StepKind::LeanCheck => self.run_system_step(step, "LeanCheck", started),
            StepKind::Scan => self.run_system_step(step, "Scan", started),
            StepKind::BundleExport => self.run_system_step(step, "BundleExport", started),
            StepKind::EngineAction(action) => {
                let decision = match self.engine.decide(action, &self.project_ctx) {
                    Ok(d) => d,
                    Err(e) => {
                        return StepOutcome::Failed {
                            seq: step.seq,
                            kind: "EngineAction".into(),
                            error: format!("engine refused: {}", e),
                            elapsed_ms: started.elapsed().as_millis() as u64,
                        }
                    }
                };
                match decision {
                    Decision::Proceed => StepOutcome::Proceeded {
                        seq: step.seq,
                        kind: "EngineAction".into(),
                        detail: "engine: Proceed".into(),
                        elapsed_ms: started.elapsed().as_millis() as u64,
                    },
                    Decision::Escalate(reason) => {
                        let category = reason.primary;
                        let packet = Packet::build(
                            reason,
                            action.clone(),
                            &self.claim_id,
                            &self.generated_at,
                            &self.strategy,
                        );
                        let packet_rel = packet_path_for(&self.claim_id, category, step.seq);
                        let category_slug = category.slug().to_string();
                        let packet_path_str = packet_rel.display().to_string();
                        if !self.dry_run {
                            let msg = format!(
                                "escalation: {} for {}",
                                category.slug(),
                                self.claim_id
                            );
                            if let Err(e) = commit_packet(
                                &self.git,
                                &self.repo_root,
                                &packet_rel,
                                &packet.to_markdown(),
                                &msg,
                            ) {
                                return StepOutcome::Failed {
                                    seq: step.seq,
                                    kind: "EngineAction".into(),
                                    error: format!("commit_packet: {}", e),
                                    elapsed_ms: started.elapsed().as_millis() as u64,
                                };
                            }
                        }
                        StepOutcome::Escalated {
                            seq: step.seq,
                            kind: "EngineAction".into(),
                            category: category_slug,
                            packet_path: packet_path_str,
                            elapsed_ms: started.elapsed().as_millis() as u64,
                        }
                    }
                }
            }
        }
    }

    fn run_system_step(
        &self,
        step: &PlannedStep,
        kind_label: &'static str,
        started: Instant,
    ) -> StepOutcome {
        StepOutcome::Proceeded {
            seq: step.seq,
            kind: kind_label.into(),
            detail: if self.dry_run {
                format!("dry-run: would run `{}` for {}", kind_label, self.claim_id)
            } else {
                // Phase 3.5 will replace this stub with real
                // library calls into runner::lean_check_all,
                // scan::check, bundle::export etc.
                format!(
                    "(MVP scaffold) `{}` invocation deferred to Phase 3.5 wiring",
                    kind_label
                )
            },
            elapsed_ms: started.elapsed().as_millis() as u64,
        }
    }

    /// Block until the operator's decision on `packet_path` is
    /// parsable, polling every `config.poll_interval`. **No
    /// timeout** — per criteria v0.3, visible failure beats
    /// silent failure. Wrapper around the Phase 2 primitive.
    pub fn await_packet(
        &self,
        packet_rel: &Path,
        config: AwaitConfig,
    ) -> Result<refineforge_escalation::DecisionOutcome, ExecuteError> {
        refineforge_escalation::await_decision(&self.git, &self.repo_root, packet_rel, config)
            .map_err(|e| ExecuteError::GitCheckpoint(e.to_string()))
    }
}

/// Build the canonical `escalations/<CLAIM-ID>/<seq>-<category>.md`
/// relative path. Stable so `refine escalations list` can find
/// every packet in a project.
pub fn packet_path_for(
    claim_id: &str,
    category: refineforge_escalation::Category,
    seq: u32,
) -> PathBuf {
    PathBuf::from(format!(
        "escalations/{}/{:03}-{}.md",
        claim_id,
        seq,
        category.slug()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::autonomous::planner::Planner;
    use refineforge_escalation::{Action, ClaimSummary, LossKind, MockGitOps};

    fn mock_executor() -> Executor<MockGitOps> {
        Executor::for_tests(MockGitOps::new(), std::env::temp_dir())
    }

    #[test]
    fn dry_run_baseline_plan_all_steps_proceed() {
        let mut ex = mock_executor();
        ex.dry_run = true;
        ex.claim_id = "EXAMPLE-001".into();
        let plan = Planner::new().plan(&ex.claim_id);
        let outcomes: Vec<_> = plan.iter().map(|s| ex.run_step(s)).collect();
        assert_eq!(outcomes.len(), 3);
        for o in &outcomes {
            assert!(matches!(o, StepOutcome::Proceeded { .. }), "{:?}", o);
        }
    }

    #[test]
    fn engine_action_that_proceeds_records_proceeded() {
        let mut ex = mock_executor();
        ex.dry_run = true;
        // Reformat is a no-op action for every category.
        let action = Action::Reformat {
            paths: vec!["x.rs".into()],
        };
        let plan = Planner::new()
            .with_engine_action(action)
            .plan("EXAMPLE-001");
        let mid = ex.run_step(&plan[1]);
        assert!(
            matches!(&mid, StepOutcome::Proceeded { kind, .. } if kind == "EngineAction"),
            "{:?}",
            mid
        );
    }

    #[test]
    fn engine_action_that_escalates_records_escalated() {
        let mut ex = mock_executor();
        ex.dry_run = true;
        ex.claim_id = "EXAMPLE-002".into();
        ex.project_ctx.claim = Some(ClaimSummary::test_default("EXAMPLE-002"));
        let action = Action::MapRustToLean {
            rust_type: "u64".into(),
            lean_type: "Nat".into(),
            lossy_kinds: vec![LossKind::UnsignedOverflow],
        };
        let plan = Planner::new()
            .with_engine_action(action)
            .plan(&ex.claim_id);
        let mid = ex.run_step(&plan[1]);
        match mid {
            StepOutcome::Escalated {
                category,
                packet_path,
                ..
            } => {
                assert_eq!(category, "idealisation");
                assert!(
                    packet_path.starts_with("escalations/EXAMPLE-002/"),
                    "{}",
                    packet_path
                );
                assert!(packet_path.ends_with("-idealisation.md"));
            }
            other => panic!("expected Escalated, got {:?}", other),
        }
    }

    #[test]
    fn dry_run_escalation_does_not_write_to_git() {
        let mut ex = mock_executor();
        ex.dry_run = true;
        let action = Action::WriteAxiom {
            module: "M".into(),
            axiom_name: "h".into(),
            statement: "True".into(),
        };
        let plan = Planner::new()
            .with_engine_action(action)
            .plan("EXAMPLE-001");
        let _ = ex.run_step(&plan[1]);
        // dry_run = true → no commits should have landed
        assert!(ex.git.commits().is_empty(), "dry-run leaked a commit");
    }

    #[test]
    fn non_dry_run_escalation_commits_a_packet() {
        let mut ex = mock_executor();
        ex.dry_run = false;
        ex.claim_id = "EXAMPLE-002".into();
        ex.project_ctx.claim = Some(ClaimSummary::test_default("EXAMPLE-002"));
        let action = Action::MapRustToLean {
            rust_type: "u64".into(),
            lean_type: "Nat".into(),
            lossy_kinds: vec![LossKind::UnsignedOverflow],
        };
        let plan = Planner::new()
            .with_engine_action(action)
            .plan(&ex.claim_id);
        let _ = ex.run_step(&plan[1]);
        let commits = ex.git.commits();
        assert_eq!(commits.len(), 1, "expected one packet commit");
        assert!(commits[0].message.contains("idealisation"));
        assert!(commits[0].file.display().to_string().contains("EXAMPLE-002"));
    }

    #[test]
    fn packet_path_is_stable_and_zero_padded() {
        let p = packet_path_for("EXAMPLE-002", refineforge_escalation::Category::Idealisation, 7);
        assert_eq!(
            p.display().to_string(),
            "escalations/EXAMPLE-002/007-idealisation.md"
        );
    }

    #[test]
    fn engine_refusal_on_criteria_mismatch_records_failed() {
        let mut ex = mock_executor();
        ex.project_ctx =
            ProjectContext::test_with_wrong_criteria_version("0.99");
        let action = Action::Reformat {
            paths: vec!["x".into()],
        };
        let plan = Planner::new()
            .with_engine_action(action)
            .plan("EXAMPLE-001");
        let mid = ex.run_step(&plan[1]);
        match mid {
            StepOutcome::Failed { error, .. } => {
                assert!(
                    error.contains("criteria-doc version mismatch"),
                    "got: {}",
                    error
                );
            }
            other => panic!("expected Failed, got {:?}", other),
        }
    }
}
