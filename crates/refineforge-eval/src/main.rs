//! `refine-eval` — measure repair-loop strategies against a corpus
//! of broken proofs.
//!
//! Usage:
//!
//! ```bash
//! refine-eval \
//!     --corpus eval/corpus/example.jsonl \
//!     --strategy mock \
//!     --max-iterations 3 \
//!     --output eval/runs/$(date +%s).json
//! ```
//!
//! Owned by Section 2 ([ARCHITECTURE.md](../../ARCHITECTURE.md)).
//!
//! Honest disclosures (see `docs/repair-evaluation.md`):
//! - With `--strategy mock` every result is `NoProposal`; the
//!   harness's value is exercising the infrastructure end-to-end
//!   so when `--strategy anthropic` (real) lands, numbers fall out
//!   immediately.
//! - The corpus is small (5-10 entries hand-crafted from EXAMPLE-002).
//!   A statistically-meaningful corpus needs the mathlib mutation
//!   pipeline (Section 2 phase 1, item 3).
//! - False-fix rate is NOT computed (the v1 heuristic
//!   `patched == ground_truth` under-counts valid alternative fixes;
//!   the v2 fuzz-based detector is future work).

mod corpus;
mod metrics;
mod runner;
mod report;

use anyhow::{Context, Result};
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "refine-eval",
    version,
    about = "Evaluate refine repair strategies against a corpus of broken proofs"
)]
struct Cli {
    /// Path to JSONL corpus file. Each line: {id, claim_id, broken_file, fixed_file, mutation}.
    #[arg(long)]
    corpus: PathBuf,

    /// Strategy name passed through to `refine repair` (mock,
    /// anthropic-mock, anthropic).
    #[arg(long, default_value = "mock")]
    strategy: String,

    /// Max iterations per corpus entry.
    #[arg(long, default_value_t = 3)]
    max_iterations: usize,

    /// Path to refineforge project root (the directory containing
    /// `lean/`, `claims/`, `Cargo.toml`). Defaults to current dir.
    #[arg(long, default_value = ".")]
    project_root: PathBuf,

    /// Where to write the JSON report. If omitted, prints to stdout.
    #[arg(long)]
    output: Option<PathBuf>,

    /// Limit the number of entries processed (useful for smoke tests).
    #[arg(long)]
    limit: Option<usize>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let entries = corpus::load(&cli.corpus)
        .with_context(|| format!("loading corpus {}", cli.corpus.display()))?;
    let entries = match cli.limit {
        Some(n) => entries.into_iter().take(n).collect(),
        None => entries,
    };

    eprintln!("refine-eval: {} entries, strategy={}, max_iterations={}",
        entries.len(), cli.strategy, cli.max_iterations);

    let run_started = chrono::Utc::now();
    let mut results = Vec::with_capacity(entries.len());
    for (i, entry) in entries.iter().enumerate() {
        eprintln!("[{}/{}] {} ({})", i + 1, entries.len(), entry.id, entry.mutation);
        let result = runner::run_one(
            &cli.project_root,
            entry,
            &cli.strategy,
            cli.max_iterations,
        );
        match &result {
            Ok(r) => eprintln!("    outcome: {} ({} iters, {} ms)",
                r.outcome, r.iterations, r.duration_ms),
            Err(e) => eprintln!("    ERROR: {e}"),
        }
        results.push((entry.clone(), result));
    }

    let report = report::Report::build(
        &cli.corpus,
        &cli.strategy,
        cli.max_iterations,
        run_started,
        results,
    );

    let json = serde_json::to_string_pretty(&report)?;
    if let Some(out) = &cli.output {
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        std::fs::write(out, &json)?;
        eprintln!("wrote report to {}", out.display());
    } else {
        println!("{json}");
    }

    eprintln!();
    eprintln!("SUMMARY: {}/{} fixed ({:.1}%), {}/{} proposed-but-rejected, {}/{} no-proposal, {}/{} errored",
        report.summary.fixed_count, report.summary.total,
        100.0 * report.summary.fixed_count as f64 / report.summary.total.max(1) as f64,
        report.summary.unrecoverable_count, report.summary.total,
        report.summary.no_proposal_count, report.summary.total,
        report.summary.error_count, report.summary.total,
    );
    eprintln!("    median latency: {} ms; p95 latency: {} ms",
        report.summary.median_duration_ms, report.summary.p95_duration_ms);

    Ok(())
}
