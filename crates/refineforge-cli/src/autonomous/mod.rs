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

use anyhow::{Context, Result};
use refineforge_escalation::{
    ClaimSummary, Engine, ProjectContext, SubprocessGitOps, CRITERIA_VERSION,
};
use std::path::Path;

/// Top-level entry point invoked by `refine autonomous <CLAIM-ID>`.
///
/// MVP scope (per `docs/autonomous-driver-plan.md` Phase 3):
/// plans + executes the baseline workflow against `claim_id`,
/// honouring the cost gate, writing a `RunReport` JSON when the
/// run finishes, and respecting `--dry-run` (no commits, no
/// side effects).
pub fn run_cli(
    root: &Path,
    claim_id: &str,
    strategy: &str,
    max_cost_usd: f64,
    operator: Option<&str>,
    dry_run: bool,
) -> Result<()> {
    println!("refine autonomous {} (strategy={}, dry_run={}, max-cost-usd=${:.2})",
        claim_id, strategy, dry_run, max_cost_usd);
    if let Some(op) = operator {
        println!("operator: {}", op);
    }
    println!("criteria version: v{}", CRITERIA_VERSION);
    println!();

    let mut ctx = ProjectContext::test_default();
    ctx.claim = Some(ClaimSummary::test_default(claim_id));
    let cost_gate = CostGate::new(max_cost_usd);
    let generated_at = chrono::Utc::now().to_rfc3339();
    let git = SubprocessGitOps::new();

    let mut ex = Executor {
        engine: Engine::new(),
        git,
        repo_root: root.to_path_buf(),
        claim_id: claim_id.to_string(),
        strategy: strategy.to_string(),
        operator: operator.map(|s| s.to_string()),
        dry_run,
        project_ctx: ctx,
        cost_gate,
        generated_at: generated_at.clone(),
    };

    let plan = Planner::new().plan(claim_id);
    println!("plan ({} steps):", plan.len());
    for step in &plan {
        println!("  {:>2}. {:?} — {}", step.seq, step.kind, step.rationale);
    }
    println!();

    let mut outcomes: Vec<StepOutcome> = Vec::new();
    for step in &plan {
        let outcome = ex.run_step(step);
        match &outcome {
            StepOutcome::Proceeded { seq, kind, detail, elapsed_ms } => {
                println!("  step {:>2} [{}] PROCEEDED ({}ms): {}", seq, kind, elapsed_ms, detail);
            }
            StepOutcome::Escalated { seq, kind, category, packet_path, elapsed_ms } => {
                println!(
                    "  step {:>2} [{}] ESCALATED [{}] ({}ms) → packet: {}",
                    seq, kind, category, elapsed_ms, packet_path
                );
                println!("    (driver halts pending operator decision; per v0.3 no auto-reject)");
            }
            StepOutcome::Failed { seq, kind, error, elapsed_ms } => {
                println!("  step {:>2} [{}] FAILED ({}ms): {}", seq, kind, elapsed_ms, error);
            }
        }
        outcomes.push(outcome);
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
    };

    println!();
    println!("summary: total={} proceeded={} escalated={} failed={} success={}",
        report.summary.total_steps,
        report.summary.proceeded,
        report.summary.escalated,
        report.summary.failed,
        report.summary.success,
    );
    println!("cost: ${:.4} / ${:.4} (remaining ${:.4})",
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
        println!("no escalation packets found under {}", escalations_dir.display());
        return Ok(());
    }
    println!("{:<10} {:<40} {:<32} {}", "STATUS", "CLAIM", "PACKET", "MODIFIED");
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
