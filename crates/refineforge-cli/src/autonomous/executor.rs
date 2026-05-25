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
use crate::claim::Claim;
use crate::repair::{self, RepairConfig, RepairOutcome};
use crate::report::ProofStatus;
use crate::scan::ScanStatus;
use crate::{bundle, runner, scan};
use refineforge_escalation::{
    commit_packet, AwaitConfig, Decision, Engine, GitOps, Packet, ProjectContext,
};
use refineforge_repair_api::{MockStrategy, RepairStrategy};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Instant;
use thiserror::Error;

/// Estimated USD cost per repair-loop attempt with the live
/// Anthropic strategy. Drawn from the eval-run numbers in
/// CHANGELOG (the v0.1 baseline cited `~$0.07/call` for
/// `claude-opus-4-7` against the 3-entry tutorial corpus).
/// Used by the cost gate to fail-closed before invoking
/// `--strategy anthropic`.
pub const ANTHROPIC_REPAIR_USD_PER_ATTEMPT: f64 = 0.07;

type StrategyWithUsage = (
    Box<dyn RepairStrategy>,
    std::sync::Arc<std::sync::Mutex<refineforge_strategies::UsageStats>>,
);

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
    /// Populated by `run_cli` from `refineforge_cli::claim::load`.
    /// `None` for unit-test executors that don't drive real
    /// system steps.
    pub claim: Option<Claim>,
    pub strategy: String,
    pub weights_path: Option<PathBuf>,
    pub operator: Option<String>,
    pub dry_run: bool,
    pub project_ctx: ProjectContext,
    pub cost_gate: CostGate,
    pub generated_at: String,
    /// Filled in by a Repair step that invokes a real Anthropic
    /// strategy. Phase 3.7: surfaced in the `RunReport`'s
    /// `anthropic_usage` field for post-run reporting.
    pub anthropic_usage_observed: Option<refineforge_strategies::UsageStats>,
    /// When `dry_run` is set, system steps short-circuit AND
    /// packet commits are skipped by default. Setting this to
    /// `true` keeps packet commits live even under `dry_run` —
    /// needed by integration tests (and any future operator
    /// flow) that want to exercise the await-resumption path
    /// without burning real Lake build time.
    pub commit_packets_in_dry_run: bool,
}

impl<G: GitOps> Executor<G> {
    /// Convenience: a no-op executor that uses [`MockGitOps`]
    /// + an empty `ProjectContext` + zero cost-gate budget. Used
    ///   by tests that want to drive only the planner / outcome
    ///   surface without touching disk.
    pub fn for_tests(git: G, repo_root: impl Into<PathBuf>) -> Self {
        Self {
            engine: Engine::new(),
            git,
            repo_root: repo_root.into(),
            claim_id: "TEST-CLAIM".into(),
            claim: None,
            strategy: "mock".into(),
            weights_path: None,
            operator: None,
            dry_run: true,
            project_ctx: ProjectContext::test_default(),
            cost_gate: CostGate::new(0.0),
            generated_at: "2026-05-18T00:00:00Z".into(),
            anthropic_usage_observed: None,
            commit_packets_in_dry_run: false,
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
            StepKind::Repair {
                strategy,
                max_iterations,
            } => self.run_repair_step(step, strategy, *max_iterations, started),
            StepKind::RunTrainingExperiment { config_path } => self.run_subprocess_step(
                step,
                "RunTrainingExperiment",
                "REFINEFORGE_REFINE_TRAIN_BIN",
                "refine-train",
                &["run", config_path, "--dry-run"],
                started,
            ),
            StepKind::RunBitExactGate { config_path } => self.run_subprocess_step(
                step,
                "RunBitExactGate",
                "REFINEFORGE_REFINE_BITEXACT_BIN",
                "refine-bitexact",
                &["run", config_path],
                started,
            ),
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
                        if !self.dry_run || self.commit_packets_in_dry_run {
                            // Phase 3.8: if a packet already exists at this
                            // path AND it has a parsable operator decision,
                            // skip the rewrite. This is what enables
                            // cross-run await-resume: the first run writes
                            // (pending), the operator commits APPROVED,
                            // the second run sees APPROVED + leaves it
                            // alone, await_decision returns the existing
                            // outcome. A still-pending existing file IS
                            // overwritten (harmless — content's identical
                            // modulo timestamp) so a re-run with updated
                            // evidence isn't masked by a stale packet.
                            let preserve = match self.git.read_file(&self.repo_root, &packet_rel) {
                                Ok(existing) => {
                                    use refineforge_escalation::decision_outcome::{
                                        parse_decision, DecisionParseError,
                                    };
                                    match parse_decision(&existing) {
                                        Ok(_) => true, // existing decision; keep it
                                        Err(DecisionParseError::Pending)
                                        | Err(DecisionParseError::MissingSection) => false,
                                        Err(_) => false, // malformed; re-write
                                    }
                                }
                                Err(_) => false, // file missing → write fresh
                            };
                            if !preserve {
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
        if self.dry_run {
            return StepOutcome::Proceeded {
                seq: step.seq,
                kind: kind_label.into(),
                detail: format!("dry-run: would run `{}` for {}", kind_label, self.claim_id),
                elapsed_ms: started.elapsed().as_millis() as u64,
            };
        }
        // Live mode requires a loaded Claim.
        let Some(claim) = self.claim.as_ref() else {
            return StepOutcome::Failed {
                seq: step.seq,
                kind: kind_label.into(),
                error: "no Claim loaded into executor — call load_project_context first".into(),
                elapsed_ms: started.elapsed().as_millis() as u64,
            };
        };
        match kind_label {
            "LeanCheck" => match runner::run(&self.repo_root, claim) {
                Ok(report) => match report.status {
                    ProofStatus::Verified => StepOutcome::Proceeded {
                        seq: step.seq,
                        kind: kind_label.into(),
                        detail: format!("lake build verified {} (status: Verified)", self.claim_id),
                        elapsed_ms: started.elapsed().as_millis() as u64,
                    },
                    other => StepOutcome::Failed {
                        seq: step.seq,
                        kind: kind_label.into(),
                        error: format!("lake build did not produce Verified status: {:?}", other),
                        elapsed_ms: started.elapsed().as_millis() as u64,
                    },
                },
                Err(e) => StepOutcome::Failed {
                    seq: step.seq,
                    kind: kind_label.into(),
                    error: format!("runner::run: {:#}", e),
                    elapsed_ms: started.elapsed().as_millis() as u64,
                },
            },
            "Scan" => match scan::scan_claim(&self.repo_root, claim) {
                Ok(report) => match report.status {
                    ScanStatus::Verified | ScanStatus::NoRustSource => StepOutcome::Proceeded {
                        seq: step.seq,
                        kind: kind_label.into(),
                        detail: format!(
                            "scan status: {} ({} rust_source items)",
                            report.status,
                            report.items.len()
                        ),
                        elapsed_ms: started.elapsed().as_millis() as u64,
                    },
                    other => StepOutcome::Failed {
                        seq: step.seq,
                        kind: kind_label.into(),
                        error: format!("scan reported {} — missing entities in rust_source", other),
                        elapsed_ms: started.elapsed().as_millis() as u64,
                    },
                },
                Err(e) => StepOutcome::Failed {
                    seq: step.seq,
                    kind: kind_label.into(),
                    error: format!("scan::scan_claim: {:#}", e),
                    elapsed_ms: started.elapsed().as_millis() as u64,
                },
            },
            "BundleExport" => match bundle::export(&self.repo_root, &self.claim_id, None) {
                Ok(()) => StepOutcome::Proceeded {
                    seq: step.seq,
                    kind: kind_label.into(),
                    detail: format!(
                        "bundle exported to artifacts/{} (SHA-256 manifest sealed)",
                        self.claim_id
                    ),
                    elapsed_ms: started.elapsed().as_millis() as u64,
                },
                Err(e) => StepOutcome::Failed {
                    seq: step.seq,
                    kind: kind_label.into(),
                    error: format!("bundle::export: {:#}", e),
                    elapsed_ms: started.elapsed().as_millis() as u64,
                },
            },
            other => StepOutcome::Failed {
                seq: step.seq,
                kind: other.into(),
                error: format!("unknown system step kind: {}", other),
                elapsed_ms: started.elapsed().as_millis() as u64,
            },
        }
    }

    fn run_subprocess_step(
        &self,
        step: &PlannedStep,
        kind_label: &'static str,
        env_var: &str,
        default_bin: &str,
        args: &[&str],
        started: Instant,
    ) -> StepOutcome {
        if self.dry_run {
            return StepOutcome::Proceeded {
                seq: step.seq,
                kind: kind_label.into(),
                detail: format!("dry-run: would shell `{} {}`", default_bin, args.join(" ")),
                elapsed_ms: started.elapsed().as_millis() as u64,
            };
        }
        let bin = std::env::var(env_var).unwrap_or_else(|_| default_bin.to_string());
        let output = std::process::Command::new(&bin)
            .args(args)
            .current_dir(&self.repo_root)
            .output();
        match output {
            Ok(out) if out.status.success() => StepOutcome::Proceeded {
                seq: step.seq,
                kind: kind_label.into(),
                detail: format!(
                    "{} {} exit 0 ({} bytes stdout)",
                    bin,
                    args.join(" "),
                    out.stdout.len()
                ),
                elapsed_ms: started.elapsed().as_millis() as u64,
            },
            Ok(out) => StepOutcome::Failed {
                seq: step.seq,
                kind: kind_label.into(),
                error: format!(
                    "{} exited {} — stderr: {}",
                    bin,
                    out.status,
                    String::from_utf8_lossy(&out.stderr).trim()
                ),
                elapsed_ms: started.elapsed().as_millis() as u64,
            },
            Err(e) => StepOutcome::Failed {
                seq: step.seq,
                kind: kind_label.into(),
                error: format!(
                    "spawn `{}` failed: {} — set {} to override the binary path",
                    bin, e, env_var
                ),
                elapsed_ms: started.elapsed().as_millis() as u64,
            },
        }
    }

    fn run_repair_step(
        &mut self,
        step: &PlannedStep,
        strategy_name: &str,
        max_iterations: usize,
        started: Instant,
    ) -> StepOutcome {
        if self.dry_run {
            return StepOutcome::Proceeded {
                seq: step.seq,
                kind: "Repair".into(),
                detail: format!(
                    "dry-run: would invoke repair loop for {} (strategy={}, max_iter={})",
                    self.claim_id, strategy_name, max_iterations
                ),
                elapsed_ms: started.elapsed().as_millis() as u64,
            };
        }
        // Cost-gate: charge before invoking, so a runaway loop
        // can't burn budget past the cap.
        let est_cost = if strategy_name == "anthropic" {
            ANTHROPIC_REPAIR_USD_PER_ATTEMPT * max_iterations as f64
        } else {
            0.0
        };
        if let Err(e) = self.cost_gate.charge(est_cost) {
            return StepOutcome::Failed {
                seq: step.seq,
                kind: "Repair".into(),
                error: format!("cost gate refused repair charge: {}", e),
                elapsed_ms: started.elapsed().as_millis() as u64,
            };
        }
        let (strategy, usage_handle) =
            match resolve_strategy(strategy_name, self.weights_path.as_deref()) {
                Ok(s) => s,
                Err(e) => {
                    return StepOutcome::Failed {
                        seq: step.seq,
                        kind: "Repair".into(),
                        error: format!("strategy resolution failed: {}", e),
                        elapsed_ms: started.elapsed().as_millis() as u64,
                    }
                }
            };
        let config = RepairConfig {
            max_iterations,
            strategy,
            dry_run: false,
        };
        match repair::repair(&self.repo_root, &self.claim_id, config) {
            Ok(report) => {
                let usage = usage_handle.lock().map(|s| s.clone()).unwrap_or_default();
                let usage_str = if usage.calls > 0 {
                    let stops: Vec<String> = usage
                        .stop_reasons
                        .iter()
                        .map(|s| s.as_deref().unwrap_or("<none>").to_string())
                        .collect();
                    format!(
                        " [api: {} calls, {} input + {} output tokens, {} cache-create + {} cache-read, stop_reasons: [{}]]",
                        usage.calls,
                        usage.input_tokens,
                        usage.output_tokens,
                        usage.cache_creation_input_tokens,
                        usage.cache_read_input_tokens,
                        stops.join(", ")
                    )
                } else {
                    String::new()
                };
                // Surface latest usage on the executor so RunReport can read it.
                self.anthropic_usage_observed = Some(usage);
                let detail = format!(
                    "repair[{}] outcome={:?}, iterations={}, file_modified={}{}",
                    strategy_name,
                    report.outcome,
                    report.iterations.len(),
                    report.file_modified,
                    usage_str
                );
                match report.outcome {
                    RepairOutcome::AlreadyClean | RepairOutcome::Fixed { .. } => {
                        StepOutcome::Proceeded {
                            seq: step.seq,
                            kind: "Repair".into(),
                            detail,
                            elapsed_ms: started.elapsed().as_millis() as u64,
                        }
                    }
                    RepairOutcome::MaxIterationsReached
                    | RepairOutcome::NoProposal
                    | RepairOutcome::UnrecoverableError(_) => StepOutcome::Failed {
                        seq: step.seq,
                        kind: "Repair".into(),
                        error: detail,
                        elapsed_ms: started.elapsed().as_millis() as u64,
                    },
                }
            }
            Err(e) => StepOutcome::Failed {
                seq: step.seq,
                kind: "Repair".into(),
                error: format!("repair::repair: {:#}", e),
                elapsed_ms: started.elapsed().as_millis() as u64,
            },
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

/// Resolve a `--strategy` CLI value to a concrete
/// [`RepairStrategy`] + a shared usage-accumulator handle.
/// `anthropic` reads `ANTHROPIC_API_KEY` + optional
/// `ANTHROPIC_MODEL` from the environment.
///
/// For non-Anthropic strategies (`mock`) the usage handle stays
/// at default `UsageStats { calls: 0, .. }`; this lets the
/// caller treat strategy-resolution uniformly.
pub fn resolve_strategy(
    name: &str,
    weights_path: Option<&Path>,
) -> Result<StrategyWithUsage, String> {
    match name {
        "mock" => Ok((
            Box::new(MockStrategy),
            std::sync::Arc::new(std::sync::Mutex::new(
                refineforge_strategies::UsageStats::default(),
            )),
        )),
        "anthropic-mock" => Ok(refineforge_strategies::anthropic_mock_strategy_with_usage()),
        "anthropic" => refineforge_strategies::anthropic_strategy_from_env_with_usage()
            .map_err(|e| e.to_string()),
        "local-finetune" => {
            let env_weights =
                std::env::var_os("REFINEFORGE_LOCAL_FINETUNE_WEIGHTS").map(PathBuf::from);
            let path = weights_path.or(env_weights.as_deref()).ok_or_else(|| {
                "local-finetune requires --weights-path <dir> or REFINEFORGE_LOCAL_FINETUNE_WEIGHTS".to_string()
            })?;
            refineforge_strategies::local_finetune_from_path_with_usage(path)
                .map_err(|e| e.to_string())
        }
        other => Err(format!(
            "unknown strategy `{}` (known: mock, anthropic-mock, anthropic, local-finetune)",
            other
        )),
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
        let plan = Planner::new().with_engine_action(action).plan(&ex.claim_id);
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
    fn phase_3_8_preexisting_approved_packet_is_not_overwritten() {
        // First run: write a fresh packet (pending). Second run:
        // the operator's APPROVED edit must survive — executor
        // must NOT re-commit over it.
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
            .with_engine_action(action.clone())
            .plan(&ex.claim_id);

        // Run 1: writes the pending packet.
        let _ = ex.run_step(&plan[1]);
        let commits_after_run1 = ex.git.commits().len();
        assert_eq!(
            commits_after_run1, 1,
            "run 1 should commit the fresh packet"
        );

        // Operator approves: overwrite the packet content directly
        // in MockGitOps (simulating their git commit).
        let pkt_path = packet_path_for(
            &ex.claim_id,
            refineforge_escalation::Category::Idealisation,
            plan[1].seq,
        );
        let approved_content = "---\ncriteria_version: '0.3'\nclaim_id: EXAMPLE-002\n---\n## Human decision\n\nAPPROVED: looks fine to me\n".to_string();
        ex.git.set_file_content(&pkt_path, approved_content.clone());

        // Run 2: executor must see the APPROVED packet and NOT re-commit.
        let _ = ex.run_step(&plan[1]);
        let commits_after_run2 = ex.git.commits().len();
        assert_eq!(
            commits_after_run2, commits_after_run1,
            "run 2 should NOT add a commit when APPROVED packet exists"
        );
        // And the APPROVED content must be intact.
        let stored = ex
            .git
            .read_file(std::path::Path::new(""), &pkt_path)
            .expect("packet exists");
        assert!(
            stored.contains("APPROVED:"),
            "APPROVED state was overwritten: {}",
            stored
        );
    }

    #[test]
    fn phase_3_8_preexisting_pending_packet_is_still_rewritten() {
        // If the existing packet is still (pending), re-running
        // should re-commit (refreshes evidence). This is harmless
        // and matches the prior MVP behaviour for the
        // un-decided case.
        let mut ex = mock_executor();
        ex.dry_run = false;
        ex.claim_id = "EXAMPLE-002".into();
        ex.project_ctx.claim = Some(ClaimSummary::test_default("EXAMPLE-002"));
        let action = Action::MapRustToLean {
            rust_type: "u64".into(),
            lean_type: "Nat".into(),
            lossy_kinds: vec![LossKind::UnsignedOverflow],
        };
        let plan = Planner::new().with_engine_action(action).plan(&ex.claim_id);
        let _ = ex.run_step(&plan[1]);
        let after_first = ex.git.commits().len();
        let _ = ex.run_step(&plan[1]);
        assert_eq!(
            ex.git.commits().len(),
            after_first + 1,
            "pending packets should be re-committed on re-run"
        );
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
        let plan = Planner::new().with_engine_action(action).plan(&ex.claim_id);
        let _ = ex.run_step(&plan[1]);
        let commits = ex.git.commits();
        assert_eq!(commits.len(), 1, "expected one packet commit");
        assert!(commits[0].message.contains("idealisation"));
        assert!(commits[0]
            .file
            .display()
            .to_string()
            .contains("EXAMPLE-002"));
    }

    #[test]
    fn packet_path_is_stable_and_zero_padded() {
        let p = packet_path_for(
            "EXAMPLE-002",
            refineforge_escalation::Category::Idealisation,
            7,
        );
        assert_eq!(
            p.display().to_string(),
            "escalations/EXAMPLE-002/007-idealisation.md"
        );
    }

    #[test]
    fn dry_run_repair_step_reports_proceeded_without_invoking_loop() {
        let mut ex = mock_executor();
        ex.dry_run = true;
        ex.claim_id = "EXAMPLE-001".into();
        let step = PlannedStep {
            seq: 99,
            kind: StepKind::Repair {
                strategy: "anthropic".into(),
                max_iterations: 5,
            },
            rationale: "test".into(),
        };
        let outcome = ex.run_step(&step);
        match outcome {
            StepOutcome::Proceeded { detail, kind, .. } => {
                assert_eq!(kind, "Repair");
                assert!(detail.starts_with("dry-run: "), "got: {}", detail);
                // dry-run must not charge the cost gate
                assert_eq!(ex.cost_gate.spent_usd, 0.0);
            }
            other => panic!("expected Proceeded in dry-run, got {:?}", other),
        }
    }

    #[test]
    fn non_dry_run_anthropic_repair_charges_cost_gate_before_invoking() {
        let mut ex = mock_executor();
        ex.dry_run = false;
        // budget too small for one anthropic repair attempt
        // (5 iter × $0.07 = $0.35) — cost gate must refuse.
        ex.cost_gate = CostGate::new(0.10);
        let step = PlannedStep {
            seq: 99,
            kind: StepKind::Repair {
                strategy: "anthropic".into(),
                max_iterations: 5,
            },
            rationale: "test".into(),
        };
        let outcome = ex.run_step(&step);
        match outcome {
            StepOutcome::Failed { error, kind, .. } => {
                assert_eq!(kind, "Repair");
                assert!(
                    error.contains("cost gate refused"),
                    "expected cost-gate refusal, got: {}",
                    error
                );
                // cost gate state untouched after a failed charge
                assert_eq!(ex.cost_gate.spent_usd, 0.0);
            }
            other => panic!("expected Failed, got {:?}", other),
        }
    }

    #[test]
    fn anthropic_constant_matches_eval_baseline() {
        // If this changes, audit the CHANGELOG cost rationale too.
        assert!((ANTHROPIC_REPAIR_USD_PER_ATTEMPT - 0.07).abs() < 1e-9);
    }

    #[test]
    fn resolve_strategy_recognises_mock() {
        let s = resolve_strategy("mock", None);
        assert!(s.is_ok());
    }

    #[test]
    fn resolve_strategy_recognises_anthropic_mock() {
        let s = resolve_strategy("anthropic-mock", None);
        assert!(s.is_ok());
    }

    #[test]
    fn resolve_strategy_rejects_unknown() {
        match resolve_strategy("definitely-not-a-real-strategy", None) {
            Err(e) => assert!(e.contains("unknown strategy"), "got: {}", e),
            Ok(_) => panic!("expected Err for unknown strategy"),
        }
    }

    #[test]
    fn resolve_strategy_requires_weights_for_local_finetune() {
        match resolve_strategy("local-finetune", None) {
            Err(e) => assert!(e.contains("--weights-path"), "got: {}", e),
            Ok(_) => panic!("expected Err for missing local-finetune weights"),
        }
    }

    #[test]
    fn resolve_strategy_recognises_local_finetune_with_manifest() {
        let td = tempfile::tempdir().unwrap();
        let weights = td.path().join("weights");
        std::fs::create_dir(&weights).unwrap();

        #[cfg(windows)]
        let command = vec![
            "cmd".to_string(),
            "/C".to_string(),
            "echo".to_string(),
            "{}".to_string(),
        ];
        #[cfg(not(windows))]
        let command = vec!["printf".to_string(), "{}".to_string()];

        std::fs::write(
            weights.join("refineforge-local-finetune.json"),
            serde_json::json!({
                "runtime": "command",
                "model_id": "fixture",
                "command": command
            })
            .to_string(),
        )
        .unwrap();

        let (strategy, usage) =
            resolve_strategy("local-finetune", Some(weights.as_path())).unwrap();
        assert_eq!(strategy.name(), "local-finetune");
        assert_eq!(usage.lock().unwrap().calls, 0);
    }

    #[test]
    fn dry_run_run_training_experiment_records_proceeded() {
        let mut ex = mock_executor();
        ex.dry_run = true;
        let step = PlannedStep {
            seq: 4,
            kind: StepKind::RunTrainingExperiment {
                config_path: "training/configs/example-qwen-1.5b.yaml".into(),
            },
            rationale: "test".into(),
        };
        let outcome = ex.run_step(&step);
        match outcome {
            StepOutcome::Proceeded { kind, detail, .. } => {
                assert_eq!(kind, "RunTrainingExperiment");
                assert!(
                    detail.contains("refine-train run") && detail.contains("--dry-run"),
                    "detail missing expected argv shape: {}",
                    detail
                );
            }
            other => panic!("expected Proceeded, got {:?}", other),
        }
    }

    #[test]
    fn dry_run_run_bitexact_gate_records_proceeded() {
        let mut ex = mock_executor();
        ex.dry_run = true;
        let step = PlannedStep {
            seq: 5,
            kind: StepKind::RunBitExactGate {
                config_path: "kernels/configs/matmul_fp32.yaml".into(),
            },
            rationale: "test".into(),
        };
        let outcome = ex.run_step(&step);
        match outcome {
            StepOutcome::Proceeded { kind, detail, .. } => {
                assert_eq!(kind, "RunBitExactGate");
                assert!(
                    detail.contains("refine-bitexact run"),
                    "detail missing expected argv shape: {}",
                    detail
                );
            }
            other => panic!("expected Proceeded, got {:?}", other),
        }
    }

    #[test]
    fn non_dry_run_subprocess_step_fails_helpfully_when_binary_missing() {
        let mut ex = mock_executor();
        ex.dry_run = false;
        // Override the binary path to something guaranteed not to exist.
        let bogus = "definitely-not-a-real-binary-9999.exe";
        std::env::set_var("REFINEFORGE_REFINE_TRAIN_BIN", bogus);
        let step = PlannedStep {
            seq: 4,
            kind: StepKind::RunTrainingExperiment {
                config_path: "x.yaml".into(),
            },
            rationale: "test".into(),
        };
        let outcome = ex.run_step(&step);
        std::env::remove_var("REFINEFORGE_REFINE_TRAIN_BIN");
        match outcome {
            StepOutcome::Failed { error, kind, .. } => {
                assert_eq!(kind, "RunTrainingExperiment");
                assert!(
                    error.contains("REFINEFORGE_REFINE_TRAIN_BIN"),
                    "error should mention the env-var override: {}",
                    error
                );
            }
            other => panic!("expected Failed, got {:?}", other),
        }
    }

    #[test]
    fn engine_refusal_on_criteria_mismatch_records_failed() {
        let mut ex = mock_executor();
        ex.project_ctx = ProjectContext::test_with_wrong_criteria_version("0.99");
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
