//! Run best-of-k Lean proof search against a *locally-running* open prover,
//! gating every candidate with the Lean checker, and emit the trust evidence.
//!
//! ## 1. Serve a downloaded prover (the operator's machine + GPU)
//!
//! Download an open 7B prover (weights are free — the expensive training was
//! already done by the authors) and serve it with an OpenAI-compatible endpoint.
//! On a dual-GPU box (e.g. RTX 5080 16 GB + P40 24 GB) pin the prover to the fast
//! fp16 card:
//!   CUDA_VISIBLE_DEVICES=0 vllm serve deepseek-ai/DeepSeek-Prover-V2-7B \
//!       --port 8000 --max-model-len 4096
//! (or `llama-server -m goedel-prover-7b.gguf --port 8000` for a GGUF on the P40.)
//!
//! ## 2. Have a real Lean/Mathlib project (the verifier's work dir)
//!
//! `--lean-dir` must point at a `lake` project whose toolchain matches what the
//! prover was trained against; the checker run is `lake env lean <file>`.
//!
//! ## 3. Search
//!
//!   cargo run -p refineforge-prover --example lean_proof_search -- \
//!       --problems problems.jsonl \
//!       --base-url http://localhost:8000 --model deepseek-ai/DeepSeek-Prover-V2-7B \
//!       --lean-dir ./lean --samples 8 --out runs/prover-search-1
//!
//! `problems.jsonl` is one [`refineforge_prover::Problem`] per line:
//!   {"id":"thm_1","statement":"theorem foo : 1 + 1 = 2 := by {{proof}}",
//!    "template":"import Mathlib\ntheorem foo : 1 + 1 = 2 := by {{proof}}"}

use anyhow::{Context, Result};
use refineforge_prover::{
    CommandVerifier, OpenAiProver, Problem, ProofSearch, ProverApi,
};

fn arg(args: &[String], flag: &str) -> Option<String> {
    args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).cloned()
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let problems_path = arg(&args, "--problems")
        .context("usage: --problems <jsonl> --base-url <url> --model <name> --lean-dir <dir> [--samples N] [--out DIR] [--chat]")?;
    let base_url = arg(&args, "--base-url").unwrap_or_else(|| "http://localhost:8000".to_string());
    let model = arg(&args, "--model").context("--model <served model name> is required")?;
    let lean_dir = arg(&args, "--lean-dir").context("--lean-dir <lean project root> is required")?;
    let samples: usize = arg(&args, "--samples").and_then(|s| s.parse().ok()).unwrap_or(8);
    let out = arg(&args, "--out").unwrap_or_else(|| "runs/prover-search".to_string());

    // Load the problem set (one JSON Problem per line).
    let problems: Vec<Problem> = std::fs::read_to_string(&problems_path)
        .with_context(|| format!("reading {problems_path}"))?
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str::<Problem>(l).context("parsing a problem line"))
        .collect::<Result<_>>()?;
    eprintln!("loaded {} problems from {problems_path}", problems.len());

    // The prover (generation) — the GPU is the server's concern, not ours.
    let mut prover = OpenAiProver::new(&base_url, &model)?.with_max_tokens(2048);
    if args.iter().any(|a| a == "--chat") {
        prover = prover.with_api(ProverApi::Chat);
    }
    if let Ok(key) = std::env::var("PROVER_API_KEY") {
        prover = prover.with_api_key(Some(key));
    }

    // The verifier (the trust gate) — `lake env lean <candidate>` in the project.
    let verifier = CommandVerifier::new("lake", ["env", "lean"], &lean_dir, "ProverCandidate.lean");

    eprintln!("prover: {model} @ {base_url}  |  verifier: lake env lean (in {lean_dir})  |  best-of-{samples}");
    let report = ProofSearch::new(&prover, &verifier, samples).run(&problems, std::path::Path::new(&out))?;

    println!(
        "\nsolved {}/{} ({:.1}% pass) — evidence in {out}/",
        report.solved,
        report.problems,
        report.pass_rate * 100.0
    );
    println!("  progress.jsonl + proof-search-report.json written");
    Ok(())
}
