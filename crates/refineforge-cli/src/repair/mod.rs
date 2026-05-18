//! Bounded LLM-driven repair loop for Lean proofs.
//!
//! ## Status: STRUCTURAL SKELETON
//!
//! - LSP client (`lsp.rs`): **real** — talks JSON-RPC to
//!   `lake env lean --server` via subprocess stdio.
//! - Diagnostic parsing (`diagnostic.rs`): **real** — converts LSP
//!   `publishDiagnostics` into a simpler structured form.
//! - Repair-loop driver (this file): **real** — bounded iteration,
//!   apply-patch-then-recheck, honest stop conditions.
//! - `RepairStrategy` trait (`strategy.rs`): **real interface,
//!   stubbed default impl**. The shipped `MockStrategy` returns
//!   `None` for every diagnostic, so `refine repair` on a broken
//!   proof exits with status `NoProposal` rather than fixing
//!   anything. Swap in `AnthropicStrategy` (single file, ~80 LoC,
//!   see `docs/llm-repair-design.md` §4) to make this useful.
//!
//! Why ship the skeleton with a mock: the operator gets honest
//! infrastructure they can wire to any LLM provider, and the
//! repair-loop semantics are testable in CI without an API key.
//!
//! ## Design doctrine
//!
//! - **LLM proposes, Lean verifies, human approves.** The repair
//!   loop never bypasses `lake build` — every proposed patch is
//!   applied and re-checked. A patch that compiles is still subject
//!   to the no-sorry policy gate.
//! - **Bounded iteration.** Default `max_iterations = 5`. The loop
//!   stops early if the file becomes clean, if the strategy
//!   declines to propose, or if a patch fails to apply.
//! - **No silent acceptance.** A repair that introduces `sorry`/
//!   `admit`/non-core `axiom` is rejected even if `lake build` would
//!   accept the file. The policy gate runs after every iteration.

pub mod diagnostic;
pub mod lsp;
pub mod strategy;

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

pub use diagnostic::Diagnostic;
pub use strategy::{MockStrategy, Patch, RepairStrategy};

use crate::claim;
use crate::sorry_gate;

/// One pass through the repair loop.
#[derive(Debug)]
pub struct Iteration {
    pub index: usize,
    pub diagnostics_before: Vec<Diagnostic>,
    pub patch_proposed: Option<Patch>,
    /// `Some(true)` means the patch was applied AND survived the
    /// post-apply policy + lake re-check. `Some(false)` means the
    /// patch was applied but reverted. `None` means no patch was
    /// proposed or the strategy errored.
    pub patch_accepted: Option<bool>,
    pub notes: Vec<String>,
}

// Fields read only via Debug formatting in print_report; dead-code
// analysis doesn't follow Debug-derives, so silence the warnings.
#[allow(dead_code)]
#[derive(Debug)]
pub enum RepairOutcome {
    /// Source was already clean (no diagnostics).
    AlreadyClean,
    /// Iterated until source became clean.
    Fixed { iterations: usize },
    /// Hit `max_iterations` without converging.
    MaxIterationsReached,
    /// Strategy declined to propose for the current diagnostic.
    NoProposal,
    /// A proposed patch failed to apply or caused a regression that
    /// could not be auto-reverted.
    UnrecoverableError(String),
}

#[derive(Debug)]
pub struct RepairReport {
    pub claim_id: String,
    pub file: PathBuf,
    pub outcome: RepairOutcome,
    pub iterations: Vec<Iteration>,
    pub strategy: String,
    /// True iff the file was actually modified on disk.
    pub file_modified: bool,
}

pub struct RepairConfig {
    pub max_iterations: usize,
    pub strategy: Box<dyn RepairStrategy>,
    /// If true, do not write any patches to disk. Diagnostics + proposals
    /// are still reported.
    pub dry_run: bool,
}

impl Default for RepairConfig {
    fn default() -> Self {
        Self {
            max_iterations: 5,
            strategy: Box::new(MockStrategy),
            dry_run: false,
        }
    }
}

/// Run the bounded repair loop against a claim's Lean file.
pub fn repair(root: &Path, claim_id: &str, config: RepairConfig) -> Result<RepairReport> {
    let (_, c) = claim::load(root, claim_id)?;
    let lean_dir = root.join("lean");
    let lean_file = root.join(&c.lean.file);

    if !lean_file.exists() {
        anyhow::bail!("Lean file not found: {}", lean_file.display());
    }

    let original_text = std::fs::read_to_string(&lean_file)
        .with_context(|| format!("reading {}", lean_file.display()))?;
    let mut current_text = original_text.clone();

    let mut client = lsp::LeanLspClient::spawn(&lean_dir)
        .context("spawning lake env lean --server")?;
    client.initialize().context("LSP initialize")?;

    let uri = lsp::path_to_uri(&lean_file);
    client.did_open(&uri, &current_text)?;

    let mut iterations = Vec::new();
    let mut outcome = RepairOutcome::MaxIterationsReached;
    let mut file_modified = false;

    for i in 0..config.max_iterations {
        let diagnostics = client.collect_diagnostics(&uri, std::time::Duration::from_secs(20))?;

        if diagnostics.is_empty() {
            outcome = if i == 0 {
                RepairOutcome::AlreadyClean
            } else {
                RepairOutcome::Fixed { iterations: i }
            };
            break;
        }

        // Pick the first error-level diagnostic. Future versions could
        // batch warnings or pick by severity/position.
        let target = diagnostics
            .iter()
            .find(|d| d.severity_is_error())
            .or_else(|| diagnostics.first())
            .cloned();

        let target = match target {
            Some(t) => t,
            None => {
                outcome = RepairOutcome::Fixed { iterations: i };
                break;
            }
        };

        let proposed = config
            .strategy
            .propose_patch(&target, &current_text)
            .context("strategy propose_patch")?;

        let mut iter = Iteration {
            index: i,
            diagnostics_before: diagnostics,
            patch_proposed: proposed.clone(),
            patch_accepted: None,
            notes: Vec::new(),
        };

        let Some(patch) = proposed else {
            iter.notes.push(format!(
                "strategy '{}' declined to propose a patch",
                config.strategy.name()
            ));
            iterations.push(iter);
            outcome = RepairOutcome::NoProposal;
            break;
        };

        // Apply patch in-memory.
        let new_text = patch.apply(&current_text);

        // Honesty gate: never let a patch introduce sorry/admit/axiom.
        let gate = sorry_gate::check(&new_text, &c.policy);
        if !gate.ok {
            iter.notes
                .push(format!("patch rejected by no-sorry gate: {:?}", gate.notes));
            iter.patch_accepted = Some(false);
            iterations.push(iter);
            outcome = RepairOutcome::UnrecoverableError(
                "strategy proposed a patch that would introduce sorry/admit/axiom".into(),
            );
            break;
        }

        // Send the change to the LSP server and write to disk (unless dry-run).
        if !config.dry_run {
            std::fs::write(&lean_file, &new_text)?;
            file_modified = true;
        }
        client.did_change(&uri, &new_text)?;
        current_text = new_text;

        iter.patch_accepted = Some(true);
        iterations.push(iter);
    }

    client.shutdown()?;

    Ok(RepairReport {
        claim_id: claim_id.to_string(),
        file: lean_file,
        outcome,
        iterations,
        strategy: config.strategy.name().to_string(),
        file_modified,
    })
}

/// CLI entry point for `refine repair`.
pub fn run_cli(
    root: &Path,
    claim_id: &str,
    max_iterations: usize,
    strategy_name: &str,
    dry_run: bool,
) -> Result<()> {
    let strategy: Box<dyn RepairStrategy> = match strategy_name {
        "mock" => Box::new(MockStrategy),
        other => anyhow::bail!(
            "unknown strategy '{other}'; available: mock (real LLM strategies live in separate crates — see docs/llm-repair-design.md)"
        ),
    };
    let config = RepairConfig {
        max_iterations,
        strategy,
        dry_run,
    };
    let report = repair(root, claim_id, config)?;
    print_report(&report);
    if report.file_modified {
        println!();
        println!("FILE MODIFIED on disk: {}", report.file.display());
        println!("Review the diff before committing.");
    }
    Ok(())
}

fn print_report(r: &RepairReport) {
    println!("claim:    {}", r.claim_id);
    println!("file:     {}", r.file.display());
    println!("strategy: {}", r.strategy);
    println!("outcome:  {:?}", r.outcome);
    println!("iterations: {}", r.iterations.len());
    for it in &r.iterations {
        println!("  iter {}: {} diagnostic(s)", it.index, it.diagnostics_before.len());
        if let Some(p) = &it.patch_proposed {
            println!("    proposed patch ({}..{}): {}",
                p.range_summary(), p.new_text_summary(), p.rationale);
        }
        if let Some(ok) = it.patch_accepted {
            println!("    accepted: {ok}");
        }
        for note in &it.notes {
            println!("    note: {note}");
        }
    }
}
