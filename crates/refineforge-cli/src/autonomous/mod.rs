//! `refine autonomous <CLAIM-ID>` — orchestration that drives a
//! claim through the four sections (Lean / ML / DevOps / CUDA)
//! without per-step human approval, escalating only when the
//! `refineforge-escalation` engine's `decide(...)` returns
//! `Decision::Escalate` per criteria v0.3.
//!
//! **Phase 3 MVP scope** (this commit):
//! - [`planner`] sequences a baseline workflow: lean check →
//!   scan → bundle export.
//! - [`executor`] runs each step, calling the engine for any
//!   step that proposes a categorised [`Action`]; non-engine
//!   system steps (lean check, scan, bundle) shell into the
//!   existing CLI modules.
//! - [`cost`] tracks cumulative API spend and fails closed when
//!   `--max-cost-usd` is exceeded.
//! - [`report`] produces a `RunReport` JSON summarising every
//!   step + every escalation + final outcome.
//!
//! **Deferred to Phase 3.5 (NOT in this commit):**
//! - Real Anthropic-strategy repair driven by the planner
//!   (today the repair step is opt-in via `--strategy`, the
//!   driver records what would have happened but does not call
//!   the live API in autonomous mode).
//! - File loaders building [`ProjectContext`] from claim YAMLs +
//!   `lake-manifest.json` + `Cargo.lock`. Today the autonomous
//!   driver populates a minimal context manually.
//! - Section 4 (`refine-bitexact`) and Section 2 trainer
//!   integration — these are Phase 3.5 per the original plan.

pub mod cost;
pub mod executor;
pub mod planner;
pub mod report;

pub use cost::{CostGate, CostGateError};
pub use executor::{ExecuteError, Executor, StepOutcome};
pub use planner::{PlannedStep, Planner, StepKind};
pub use report::{RunReport, RunSummary};
// `WorkRunConfig` + `run_worklist` are defined later in this file
// and re-exported automatically via `pub fn` / `pub struct`.

use anyhow::{Context, Result};
use refineforge_escalation::{
    load_project_context, AwaitConfig, ClaimSummary, DecisionOutcome, Engine, GitOps,
    ProjectContext, SubprocessGitOps, CRITERIA_VERSION,
};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Configuration for the worklist runner. Keeps `run_worklist`'s
/// signature stable as more knobs are added (auto-repair,
/// await-decisions, etc.).
#[derive(Debug, Clone)]
pub struct WorkRunConfig {
    pub strategy: String,
    pub auto_repair: bool,
    pub await_decisions: bool,
    pub repair_max_iterations: usize,
    pub max_repair_attempts: usize,
    pub await_poll_interval: Duration,
}

impl Default for WorkRunConfig {
    fn default() -> Self {
        Self {
            strategy: "mock".into(),
            auto_repair: false,
            await_decisions: false,
            repair_max_iterations: 5,
            max_repair_attempts: 2,
            await_poll_interval: Duration::from_secs(5),
        }
    }
}

/// Drive a [`PlannedStep`] list through the executor with
/// optional auto-repair injection and optional escalation-await
/// resumption. Generic over [`GitOps`] so tests can drive it
/// with [`refineforge_escalation::MockGitOps`].
///
/// Returns the full list of [`StepOutcome`]s including any
/// dynamically-injected Repair/recheck steps and any
/// post-Escalated resume outcomes.
pub fn run_worklist<G: GitOps>(
    ex: &mut Executor<G>,
    plan: Vec<PlannedStep>,
    cfg: &WorkRunConfig,
) -> Vec<StepOutcome> {
    let mut work: std::collections::VecDeque<PlannedStep> = plan.into_iter().collect();
    let mut next_seq = work.iter().map(|s| s.seq).max().unwrap_or(0) + 1;
    let mut outcomes: Vec<StepOutcome> = Vec::new();
    let mut repair_attempts = 0usize;
    while let Some(step) = work.pop_front() {
        let outcome = ex.run_step(&step);
        let is_lean_failed = matches!(
            &outcome,
            StepOutcome::Failed { kind, .. } if kind == "LeanCheck"
        );
        let escalated_packet: Option<(String, PathBuf)> = if let StepOutcome::Escalated {
            category,
            packet_path,
            ..
        } = &outcome
        {
            Some((category.clone(), PathBuf::from(packet_path)))
        } else {
            None
        };
        outcomes.push(outcome);

        if is_lean_failed && cfg.auto_repair && repair_attempts < cfg.max_repair_attempts {
            repair_attempts += 1;
            let repair_step = PlannedStep {
                seq: next_seq,
                kind: StepKind::Repair {
                    strategy: cfg.strategy.clone(),
                    max_iterations: cfg.repair_max_iterations,
                },
                rationale: format!(
                    "LeanCheck failed; --auto-repair attempt {}/{} (strategy={})",
                    repair_attempts, cfg.max_repair_attempts, cfg.strategy
                ),
            };
            let recheck_step = PlannedStep {
                seq: next_seq + 1,
                kind: StepKind::LeanCheck,
                rationale: "re-verify after auto-repair".into(),
            };
            next_seq += 2;
            work.push_front(recheck_step);
            work.push_front(repair_step);
            continue;
        }

        if let Some((category, packet_rel)) = escalated_packet {
            if !cfg.await_decisions {
                // Default behaviour: halt at first escalation, leaving the
                // packet pending. Operator runs `refine escalations list`
                // to see what's blocking.
                break;
            }
            let await_cfg = AwaitConfig {
                poll_interval: cfg.await_poll_interval,
            };
            match ex.await_packet(&packet_rel, await_cfg) {
                Ok(DecisionOutcome::Approved { reason }) => {
                    outcomes.push(StepOutcome::Proceeded {
                        seq: next_seq,
                        kind: "OperatorDecision".into(),
                        detail: format!(
                            "APPROVED ({}): {}",
                            category,
                            reason.unwrap_or_else(|| "<no reason given>".into())
                        ),
                        elapsed_ms: 0,
                    });
                    next_seq += 1;
                }
                Ok(DecisionOutcome::Rejected { reason }) => {
                    outcomes.push(StepOutcome::Failed {
                        seq: next_seq,
                        kind: "OperatorDecision".into(),
                        error: format!("REJECTED ({}): {}", category, reason),
                        elapsed_ms: 0,
                    });
                    break;
                }
                Ok(DecisionOutcome::EditAndResubmit { suggestions }) => {
                    outcomes.push(StepOutcome::Failed {
                        seq: next_seq,
                        kind: "OperatorDecision".into(),
                        error: format!("EDIT_AND_RESUBMIT ({}): {}", category, suggestions),
                        elapsed_ms: 0,
                    });
                    break;
                }
                Ok(DecisionOutcome::Partial(p)) => {
                    // Phase 3.7 MVP: we don't yet generate batched packets
                    // from the driver, so a Partial decision is unexpected
                    // — record it and halt for operator follow-up.
                    outcomes.push(StepOutcome::Failed {
                        seq: next_seq,
                        kind: "OperatorDecision".into(),
                        error: format!(
                            "PARTIAL ({}): approved={:?}, rejected={:?}",
                            category, p.approved_indices, p.rejected_indices
                        ),
                        elapsed_ms: 0,
                    });
                    break;
                }
                Err(e) => {
                    outcomes.push(StepOutcome::Failed {
                        seq: next_seq,
                        kind: "OperatorDecision".into(),
                        error: format!("await_decision: {}", e),
                        elapsed_ms: 0,
                    });
                    break;
                }
            }
        }
    }
    outcomes
}

pub struct RunCliOptions<'a> {
    pub root: &'a Path,
    pub claim_id: &'a str,
    pub strategy: &'a str,
    pub weights_path: Option<&'a Path>,
    pub max_cost_usd: f64,
    pub operator: Option<&'a str>,
    pub dry_run: bool,
    pub auto_repair: bool,
    pub await_decisions: bool,
    pub inject_counter_idealisation: bool,
    pub inject_training: &'a [String],
    pub inject_bitexact: &'a [String],
}

/// Top-level entry point invoked by `refine autonomous <CLAIM-ID>`.
///
/// MVP scope (per `docs/plans/autonomous-driver-plan.md` Phase 3):
/// plans + executes the baseline workflow against `claim_id`,
/// honouring the cost gate, writing a `RunReport` JSON when the
/// run finishes, and respecting `--dry-run` (no commits, no
/// side effects).
pub fn run_cli(opts: RunCliOptions<'_>) -> Result<()> {
    let RunCliOptions {
        root,
        claim_id,
        strategy,
        weights_path,
        max_cost_usd,
        operator,
        dry_run,
        auto_repair,
        await_decisions,
        inject_counter_idealisation,
        inject_training,
        inject_bitexact,
    } = opts;
    println!("refine autonomous {} (strategy={}, dry_run={}, max-cost-usd=${:.2}, auto_repair={}, await_decisions={})",
        claim_id, strategy, dry_run, max_cost_usd, auto_repair, await_decisions);
    if inject_counter_idealisation {
        println!(
            "**INJECTED BAIT**: Cat 2 counter-idealisation Action (u64→Nat, UnsignedOverflow)"
        );
    }
    for p in inject_training {
        println!("**INJECTED TRAINING**: refine-train run {} --dry-run", p);
    }
    for p in inject_bitexact {
        println!("**INJECTED BITEXACT**: refine-bitexact run {}", p);
    }
    if let Some(op) = operator {
        println!("operator: {}", op);
    }
    if let Some(path) = weights_path {
        println!("weights_path: {}", path.display());
    }
    println!("criteria version: v{}", CRITERIA_VERSION);
    println!();

    // Try to load the real claim + project context from disk.
    // On failure, fall back to a test_default context so the
    // run can still proceed in dry-run / smoke modes.
    let project_ctx = match load_project_context(root, Some(claim_id)) {
        Ok(ctx) => {
            println!(
                "loaded ProjectContext: {} lake packages, {} bundle-chain crates, claim={}",
                ctx.lake_packages_existing.len(),
                ctx.bundle_chain_crates.len(),
                ctx.claim
                    .as_ref()
                    .map(|c| c.id.as_str())
                    .unwrap_or("(none)")
            );
            ctx
        }
        Err(e) => {
            eprintln!(
                "WARNING: load_project_context failed: {} — falling back to test_default",
                e
            );
            let mut ctx = ProjectContext::test_default();
            ctx.claim = Some(ClaimSummary::test_default(claim_id));
            ctx
        }
    };
    // Load the underlying Claim YAML for runner/scan/bundle calls.
    let claim = match crate::claim::load(root, claim_id) {
        Ok((_, c)) => Some(c),
        Err(e) => {
            eprintln!(
                "WARNING: claim::load failed: {} — system steps will fail in non-dry-run mode",
                e
            );
            None
        }
    };

    let cost_gate = CostGate::new(max_cost_usd);
    let generated_at = chrono::Utc::now().to_rfc3339();
    let git = SubprocessGitOps::new();

    let mut ex = Executor {
        engine: Engine::new(),
        git,
        repo_root: root.to_path_buf(),
        claim_id: claim_id.to_string(),
        claim,
        strategy: strategy.to_string(),
        weights_path: weights_path.map(|p| p.to_path_buf()),
        operator: operator.map(|s| s.to_string()),
        dry_run,
        project_ctx,
        cost_gate,
        generated_at: generated_at.clone(),
        anthropic_usage_observed: None,
        commit_packets_in_dry_run: false,
    };

    let mut planner = Planner::new();
    if inject_counter_idealisation {
        planner = planner.with_engine_action(refineforge_escalation::Action::MapRustToLean {
            rust_type: "u64".into(),
            lean_type: "Nat".into(),
            lossy_kinds: vec![refineforge_escalation::LossKind::UnsignedOverflow],
        });
    }
    for p in inject_training {
        planner = planner.with_training_step(p);
    }
    for p in inject_bitexact {
        planner = planner.with_bitexact_step(p);
    }
    let plan = planner.plan(claim_id);
    println!("plan ({} steps):", plan.len());
    for step in &plan {
        println!("  {:>2}. {:?} — {}", step.seq, step.kind, step.rationale);
    }
    println!();

    let cfg = WorkRunConfig {
        strategy: strategy.to_string(),
        auto_repair,
        await_decisions,
        repair_max_iterations: 5,
        max_repair_attempts: 2,
        await_poll_interval: Duration::from_secs(5),
    };
    let outcomes = run_worklist(&mut ex, plan, &cfg);
    // Print outcomes (run_worklist itself is silent so tests can
    // consume it cleanly).
    for o in &outcomes {
        match o {
            StepOutcome::Proceeded {
                seq,
                kind,
                detail,
                elapsed_ms,
            } => {
                println!(
                    "  step {:>2} [{}] PROCEEDED ({}ms): {}",
                    seq, kind, elapsed_ms, detail
                );
            }
            StepOutcome::Escalated {
                seq,
                kind,
                category,
                packet_path,
                elapsed_ms,
            } => {
                println!(
                    "  step {:>2} [{}] ESCALATED [{}] ({}ms) → packet: {}",
                    seq, kind, category, elapsed_ms, packet_path
                );
                if !await_decisions {
                    println!("    (--await-decisions not set; driver halts here)");
                }
            }
            StepOutcome::Failed {
                seq,
                kind,
                error,
                elapsed_ms,
            } => {
                println!(
                    "  step {:>2} [{}] FAILED ({}ms): {}",
                    seq, kind, elapsed_ms, error
                );
            }
        }
    }

    let report = RunReport {
        claim_id: claim_id.to_string(),
        criteria_version: CRITERIA_VERSION.to_string(),
        started_at: generated_at.clone(),
        finished_at: chrono::Utc::now().to_rfc3339(),
        dry_run,
        strategy: strategy.to_string(),
        operator: operator.map(|s| s.to_string()),
        summary: RunSummary::from_outcomes(&outcomes),
        steps: outcomes,
        cost_usd_total: ex.cost_gate.spent_usd,
        cost_usd_max: ex.cost_gate.max_usd,
        anthropic_usage: ex.anthropic_usage_observed.clone(),
    };

    println!();
    println!(
        "summary: total={} proceeded={} escalated={} failed={} success={}",
        report.summary.total_steps,
        report.summary.proceeded,
        report.summary.escalated,
        report.summary.failed,
        report.summary.success,
    );
    println!(
        "cost: ${:.4} / ${:.4} (remaining ${:.4})",
        report.cost_usd_total,
        report.cost_usd_max,
        ex.cost_gate.remaining(),
    );

    if !dry_run {
        let runs_dir = root.join("autonomous").join("runs");
        std::fs::create_dir_all(&runs_dir).context("create autonomous/runs dir")?;
        let stamp = generated_at.replace(':', "-");
        let report_path = runs_dir.join(format!("{}-{}.json", claim_id, stamp));
        let json = serde_json::to_string_pretty(&report).context("serialize report")?;
        std::fs::write(&report_path, json)
            .with_context(|| format!("write {}", report_path.display()))?;
        println!("report written to {}", report_path.display());
    } else {
        println!("(dry-run: report not written to disk)");
    }
    Ok(())
}

/// `refine escalations list [--claim X] [--age-gt N]` — scans
/// `escalations/<CLAIM-ID>/` for packet files, parses each one's
/// front matter and decision section, and prints a queue
/// dashboard sorted by age.
///
/// Per criteria v0.3 this is the operator's "what am I
/// blocking?" view; the driver itself never auto-rejects.
pub fn escalations_list(
    root: &Path,
    claim_filter: Option<&str>,
    _age_gt_days: Option<u32>,
) -> Result<()> {
    let escalations_dir = root.join("escalations");
    if !escalations_dir.exists() {
        println!("no escalations/ directory under {}", root.display());
        return Ok(());
    }
    let mut found = Vec::<(String, String, String, std::time::SystemTime)>::new();
    for claim_entry in walkdir::WalkDir::new(&escalations_dir)
        .min_depth(1)
        .max_depth(1)
    {
        let claim_entry = claim_entry.context("walk escalations/")?;
        if !claim_entry.file_type().is_dir() {
            continue;
        }
        let claim_id = claim_entry.file_name().to_string_lossy().to_string();
        if let Some(f) = claim_filter {
            if claim_id != f {
                continue;
            }
        }
        for pkt in walkdir::WalkDir::new(claim_entry.path())
            .min_depth(1)
            .max_depth(1)
        {
            let pkt = pkt.context("walk packet")?;
            if !pkt.file_type().is_file() {
                continue;
            }
            let name = pkt.file_name().to_string_lossy().to_string();
            if !name.ends_with(".md") {
                continue;
            }
            let content = std::fs::read_to_string(pkt.path())
                .with_context(|| format!("read {}", pkt.path().display()))?;
            let status = match refineforge_escalation::decision_outcome::parse_decision(&content) {
                Ok(_) => "DECIDED",
                Err(refineforge_escalation::DecisionParseError::Pending) => "PENDING",
                Err(refineforge_escalation::DecisionParseError::MissingSection) => "MALFORMED",
                Err(_) => "UNRECOGNISED",
            };
            let mtime = pkt
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            found.push((claim_id.clone(), name, status.to_string(), mtime));
        }
    }
    found.sort_by_key(|(_, _, _, m)| *m);
    if found.is_empty() {
        println!(
            "no escalation packets found under {}",
            escalations_dir.display()
        );
        return Ok(());
    }
    println!("{:<10} {:<40} {:<32} MODIFIED", "STATUS", "CLAIM", "PACKET");
    let now = std::time::SystemTime::now();
    for (claim, name, status, mtime) in &found {
        let age = now
            .duration_since(*mtime)
            .map(|d| d.as_secs() / 86400)
            .unwrap_or(0);
        println!("{:<10} {:<40} {:<32} {} days ago", status, claim, name, age);
    }
    let pending = found.iter().filter(|(_, _, s, _)| s == "PENDING").count();
    println!();
    println!("{} pending of {} total", pending, found.len());
    if let Some((_, _, _, oldest_mtime)) = found.iter().find(|(_, _, s, _)| s == "PENDING") {
        let age_days = now
            .duration_since(*oldest_mtime)
            .map(|d| d.as_secs() / 86400)
            .unwrap_or(0);
        println!("oldest pending: {} days", age_days);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    // Sanity: every public re-export resolves to a real type.
    #[test]
    fn public_api_compiles() {
        let _ = super::CostGate::new(10.0);
        let _ = super::Planner::new();
    }
}
