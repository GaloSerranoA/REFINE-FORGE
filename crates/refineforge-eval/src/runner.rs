//! Per-entry runner: makes a temp copy of the project, swaps in the
//! broken Lean source, runs `refineforge_cli::repair::repair`, and
//! reports the outcome.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::Instant;

use refineforge_cli::repair::{self, RepairConfig, RepairOutcome};
use refineforge_repair_api::{MockStrategy, RepairStrategy};

use crate::corpus::CorpusEntry;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntryResult {
    pub outcome: String,
    pub iterations: usize,
    pub strategy: String,
    pub duration_ms: u64,
    pub file_modified: bool,
    /// Whether the strategy proposed any patch in any iteration
    /// (regardless of whether it was accepted).
    pub any_patch_proposed: bool,
}

pub fn run_one(
    project_root: &Path,
    entry: &CorpusEntry,
    strategy_name: &str,
    max_iterations: usize,
) -> Result<EntryResult> {
    let broken_path = project_root.join(&entry.broken_file);
    let broken_text = std::fs::read_to_string(&broken_path)
        .with_context(|| format!("reading broken file {}", broken_path.display()))?;

    // Build a temp copy of the project, then overwrite the Lean
    // file the claim points at with our broken version.
    let tempdir = tempfile::Builder::new()
        .prefix("refine-eval-")
        .tempdir()
        .context("creating tempdir")?;
    copy_project(project_root, tempdir.path())?;

    // Locate the Lean file in the temp copy via the claim metadata.
    let (_, claim) = refineforge_cli::claim::load(tempdir.path(), &entry.claim_id)
        .with_context(|| format!("loading claim '{}' from temp copy", entry.claim_id))?;
    let target = tempdir.path().join(&claim.lean.file);

    // Pre-warm `.lake/` cache on the GOOD source. Otherwise the LSP
    // server has to build every dependency from scratch on first
    // didOpen, which can exceed repair::repair's 20s diagnostic
    // timeout and surface as a false "AlreadyClean" reading.
    // We intentionally ignore the exit status — if pre-warm fails
    // on unmodified source, the per-entry repair will surface the
    // same issue via its own error path.
    let _ = std::process::Command::new("lake")
        .arg("build")
        .current_dir(tempdir.path().join("lean"))
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();

    // NOW swap in the broken version.
    std::fs::write(&target, &broken_text)
        .with_context(|| format!("overwriting {} with broken version", target.display()))?;

    // Build the strategy. We re-use the CLI's strategy registry to
    // avoid duplication.
    let strategy = build_strategy(strategy_name)?;

    let config = RepairConfig {
        max_iterations,
        strategy,
        // Always dry-run in eval: we don't actually want to mutate
        // the temp project on disk for the next iteration; the
        // driver's LSP didChange notification already handles that.
        // Actually — for the repair loop to make progress across
        // iterations we DO need writes; the temp dir is throwaway.
        dry_run: false,
    };

    let started = Instant::now();
    let report = repair::repair(tempdir.path(), &entry.claim_id, config)?;
    let duration_ms = started.elapsed().as_millis() as u64;

    let any_patch_proposed = report.iterations.iter().any(|i| i.patch_proposed.is_some());

    // tempdir drops + cleans up here.
    drop(tempdir);

    Ok(EntryResult {
        outcome: classify(&report.outcome),
        iterations: report.iterations.len(),
        strategy: report.strategy,
        duration_ms,
        file_modified: report.file_modified,
        any_patch_proposed,
    })
}

fn classify(o: &RepairOutcome) -> String {
    match o {
        RepairOutcome::AlreadyClean => "AlreadyClean".into(),
        RepairOutcome::Fixed { .. } => "Fixed".into(),
        RepairOutcome::MaxIterationsReached => "MaxIterationsReached".into(),
        RepairOutcome::NoProposal => "NoProposal".into(),
        RepairOutcome::UnrecoverableError(_) => "UnrecoverableError".into(),
    }
}

fn build_strategy(name: &str) -> Result<Box<dyn RepairStrategy>> {
    Ok(match name {
        "mock" => Box::new(MockStrategy),
        "anthropic-mock" => refineforge_strategies::anthropic_mock_strategy(),
        "anthropic" => refineforge_strategies::anthropic_strategy_from_env()?,
        other => anyhow::bail!(
            "unknown strategy '{other}'; available: mock, anthropic-mock, anthropic"
        ),
    })
}

/// Copy the project files needed for `lake build` + `refine repair`.
/// Skips `target/`, `.git/`, `lean/.lake/`, `artifacts/` for speed.
fn copy_project(src: &Path, dst: &Path) -> Result<()> {
    for entry in walkdir::WalkDir::new(src).into_iter().filter_entry(|e| {
        let p = e.path();
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
        !(name == "target"
            || name == ".git"
            || name == ".lake"
            || name == "artifacts"
            || name == "node_modules")
    }) {
        let entry = entry?;
        let rel = entry.path().strip_prefix(src).unwrap();
        let to = dst.join(rel);
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&to)?;
        } else if entry.file_type().is_file() {
            if let Some(parent) = to.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(entry.path(), &to)?;
        }
    }
    Ok(())
}
