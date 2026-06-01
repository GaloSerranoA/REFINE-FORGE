//! One expert-iteration **mine** step: turn a completed proof-search round into a
//! cumulative, deduped SFT dataset of **Lean-verified proofs only**, ready for an
//! external LoRA fine-tune.
//!
//! The loop (operator-driven, because the fine-tune is external Python on the GPU):
//!
//! 1. `refine-train run <prover-config>` — Stage-0 search → `proof-search-report.json`.
//! 2. `cargo run -p refineforge-prover --example mine_proofs -- …` — this step:
//!    `--problems <jsonl> --search-report runs/<run>/proof-search-report.json`
//!    `--corpus corpus.jsonl --round N --out-sft sft.jsonl --out-chat sft-chat.jsonl --ledger ledger.json`.
//! 3. `python training/scripts/lora_finetune_prover.py --data sft-chat.jsonl …` — external, on the P40.
//! 4. re-serve the adapted model, bump `--round`, go to 1.
//!
//! Only `solved` problems carrying a `verified_proof` enter the corpus — the model
//! is never adapted on an unverified proof.

use anyhow::{Context, Result};
use refineforge_prover::expert_iteration::{mine_from_report, Corpus, Ledger, RoundRecord};
use refineforge_prover::{Problem, SearchReport};
use std::path::Path;

fn arg(args: &[String], flag: &str) -> Option<String> {
    args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).cloned()
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let problems_path = arg(&args, "--problems")
        .context("usage: --problems <jsonl> --search-report <json> --corpus <jsonl> --round N --out-sft <jsonl> [--out-chat <jsonl>] [--ledger <json>] [--system <msg>]")?;
    let report_path = arg(&args, "--search-report").context("--search-report <proof-search-report.json> required")?;
    let corpus_path = arg(&args, "--corpus").context("--corpus <corpus.jsonl> required (created if absent)")?;
    let round: u32 = arg(&args, "--round").and_then(|s| s.parse().ok()).unwrap_or(1);
    let out_sft = arg(&args, "--out-sft").unwrap_or_else(|| corpus_path.clone());

    // Load this round's problems + search report.
    let problems: Vec<Problem> = std::fs::read_to_string(&problems_path)
        .with_context(|| format!("reading {problems_path}"))?
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str::<Problem>(l).context("parsing a problem line"))
        .collect::<Result<_>>()?;
    let report: SearchReport = serde_json::from_str(
        &std::fs::read_to_string(&report_path).with_context(|| format!("reading {report_path}"))?,
    )
    .context("parsing proof-search-report.json")?;

    // Mine verified proofs and fold them into the cumulative corpus.
    let mined = mine_from_report(&problems, &report, round);
    let mut corpus = Corpus::load_jsonl(Path::new(&corpus_path))?;
    let before = corpus.len();
    let newly_added = corpus.add(mined);

    // Persist the corpus + the trainable datasets.
    corpus.write_jsonl(Path::new(&corpus_path))?;
    if out_sft != corpus_path {
        corpus.write_jsonl(Path::new(&out_sft))?;
    }
    if let Some(chat) = arg(&args, "--out-chat") {
        let system = arg(&args, "--system");
        corpus.write_chat_jsonl(Path::new(&chat), system.as_deref())?;
    }

    // Append the round to the ledger (progress evidence).
    if let Some(ledger_path) = arg(&args, "--ledger") {
        let mut ledger = Ledger::load(Path::new(&ledger_path))?;
        ledger.push(RoundRecord {
            round,
            problems_searched: report.problems,
            solved_this_round: report.solved,
            newly_added,
            cumulative_corpus: corpus.len(),
            cumulative_solved_problems: corpus.solved_problems(),
            pass_rate: report.pass_rate,
        });
        ledger.write(Path::new(&ledger_path))?;
    }

    println!(
        "round {round}: searched {} · solved {} ({:.1}%) · corpus {} → {} (+{newly_added} new verified proofs, {} distinct problems)",
        report.problems,
        report.solved,
        report.pass_rate * 100.0,
        before,
        corpus.len(),
        corpus.solved_problems(),
    );
    println!("  SFT dataset: {out_sft}  (train the LoRA on this, then re-serve + bump --round)");
    Ok(())
}
