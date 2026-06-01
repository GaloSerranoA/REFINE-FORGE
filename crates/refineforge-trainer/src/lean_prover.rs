//! `refineforge_lean_prover` backend — orchestrate best-of-k Lean proof search
//! against a *downloaded* open prover (DeepSeek-Prover-V2-7B, Goedel-Prover,
//! Kimina-Prover, …), gating every candidate with the Lean checker, and emit the
//! standard trust evidence (`progress.jsonl` with the honest `proof_pass_rate`
//! metric → `report.json` → the eval/regression/approval ladder).
//!
//! GPU-agnostic by construction: the prover runs behind an OpenAI-compatible
//! inference server (vLLM / llama.cpp), so the GPU — a P40, a 5080, both, or a
//! future card — is the *server's* concern, not this backend's. See the
//! [`refineforge-prover`] crate for the orchestration engine; this module is the
//! thin trainer-pipeline adapter (config → engine → evidence contract).
//!
//! ## Config (`hyperparameters`)
//! - `problems_file` (required): JSONL of `{id, statement, split?, template?}`.
//! - `samples` (default 8): k — best-of-k generations per problem.
//! - Prover source — exactly one of:
//!   - `prover_replay_file`: replay pre-generated candidates (offline / dry-run / test).
//!   - `prover_base_url` (default `http://localhost:8000`) + `prover_model`
//!     (required for a live run) [+ `prover_api: chat`, `max_tokens`].
//! - Verifier — `verifier`:
//!   - `lean` (default): the real Lean checker via `lean_dir` [+ `lean_command`,
//!     default `lake env lean`]. This is the only trust-bearing mode.
//!   - `dry_run`: a labeled no-Lean plumbing stand-in (`verifier_substring`) — for
//!     wiring/CI only; grants no trust.

use crate::experiment::Experiment;
use crate::runner::RunPaths;
use anyhow::{Context, Result};
use chrono::Utc;
use refineforge_prover::{
    CommandVerifier, DryRunVerifier, OpenAiProver, Problem, ProofSearch, ProverApi, ProverClient,
    ReplayProver, SamplingMode, Verifier,
};
use std::collections::BTreeMap;
use std::io::Write as _;
use std::path::Path;

/// Mirrors the other native backends' outcome (the CLI turns this into a report).
pub struct LeanProverOutcome {
    pub progress_records: usize,
}

fn hyper_str<'a>(exp: &'a Experiment, key: &str) -> Option<&'a str> {
    exp.hyperparameters.get(key).and_then(|v| v.as_str())
}
fn hyper_usize(exp: &Experiment, key: &str, default: usize) -> usize {
    exp.hyperparameters
        .get(key)
        .and_then(|v| v.as_u64())
        .map(|n| n as usize)
        .unwrap_or(default)
}

pub fn run(paths: &RunPaths, exp: &Experiment) -> Result<LeanProverOutcome> {
    // Problems are the "dataset" for proof search; `hyperparameters.problems_file`
    // overrides the dataset path. Either way it is a JSONL of {id,statement,…}.
    let problems_path = hyper_str(exp, "problems_file")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| exp.dataset.path.clone());
    let problems = load_problems(&problems_path)?;
    anyhow::ensure!(
        !problems.is_empty(),
        "no problems in {}",
        problems_path.display()
    );
    let samples = hyper_usize(exp, "samples", 8);

    // ── prover (generation): replay (offline) XOR a live OpenAI-compatible server ──
    let prover: Box<dyn ProverClient> = if let Some(replay) = hyper_str(exp, "prover_replay_file") {
        Box::new(ReplayProver::from_jsonl(Path::new(replay))?)
    } else {
        let base = hyper_str(exp, "prover_base_url").unwrap_or("http://localhost:8000");
        let model = hyper_str(exp, "prover_model").context(
            "refineforge_lean_prover requires hyperparameters.prover_model (or prover_replay_file)",
        )?;
        // Best-of-k sampling: Auto (default) sends n=k and tops up with
        // single-sample requests if the server ignored n>1 (llama.cpp) — correct
        // on both vLLM and llama.cpp. Override with prover_sampling: batched|per_request.
        let sampling = match hyper_str(exp, "prover_sampling") {
            Some("batched") => SamplingMode::Batched,
            Some("per_request") => SamplingMode::PerRequest,
            _ => SamplingMode::Auto,
        };
        let mut p = OpenAiProver::new(base, model)?
            .with_max_tokens(hyper_usize(exp, "max_tokens", 2048))
            .with_sampling(sampling);
        if hyper_str(exp, "prover_api") == Some("chat") {
            p = p.with_api(ProverApi::Chat);
        }
        if let Ok(key) = std::env::var("PROVER_API_KEY") {
            p = p.with_api_key(Some(key));
        }
        Box::new(p)
    };

    // ── verifier (trust gate): the real Lean checker XOR a labeled dry-run stand-in ──
    let verifier: Box<dyn Verifier> = match hyper_str(exp, "verifier").unwrap_or("lean") {
        "dry_run" | "substring" => Box::new(DryRunVerifier::new(
            hyper_str(exp, "verifier_substring").unwrap_or(""),
        )),
        "lean" => {
            let lean_dir = hyper_str(exp, "lean_dir")
                .context("verifier=lean requires hyperparameters.lean_dir (a lake project root)")?;
            let cmd = hyper_str(exp, "lean_command").unwrap_or("lake env lean");
            let mut parts = cmd.split_whitespace();
            let program = parts.next().unwrap_or("lake").to_string();
            let args: Vec<String> = parts.map(String::from).collect();
            Box::new(CommandVerifier::new(
                program,
                args,
                lean_dir,
                "ProverCandidate.lean",
            ))
        }
        other => anyhow::bail!("unknown verifier {other:?} (use \"lean\" or \"dry_run\")"),
    };

    // ── search loop → native ProgressRecord per problem (+ log + report artifact) ──
    let search = ProofSearch::new(prover.as_ref(), verifier.as_ref(), samples);
    let mut progress_file = std::fs::File::create(&paths.progress_file)
        .with_context(|| format!("creating {}", paths.progress_file.display()))?;
    let mut log_file = std::fs::File::create(&paths.log_file)?;
    writeln!(
        log_file,
        "refineforge_lean_prover: {} problems, best-of-{samples}",
        problems.len()
    )?;

    let mut solved = 0usize;
    let mut results = Vec::with_capacity(problems.len());
    for (i, problem) in problems.iter().enumerate() {
        let step = (i + 1) as u64;
        let result = search.solve(problem)?;
        if result.solved {
            solved += 1;
        }
        let pass_rate = solved as f64 / (i + 1) as f64;
        let raw = format!(
            "step={step} id={} solved={} attempts={} proof_pass_rate={pass_rate:.6}",
            result.id, result.solved, result.attempts
        );
        writeln!(log_file, "{raw}")?;

        let mut metrics = BTreeMap::new();
        metrics.insert("proof_pass_rate".to_string(), pass_rate);
        metrics.insert("solved".to_string(), solved as f64);
        metrics.insert("attempts".to_string(), result.attempts as f64);
        let record = crate::progress::ProgressRecord {
            timestamp: Utc::now(),
            raw,
            metrics,
            step: Some(step),
        };
        writeln!(progress_file, "{}", serde_json::to_string(&record)?)?;
        results.push(result);
    }

    // Extra artifact alongside the standard evidence: the full per-problem search
    // report (verified proofs + misses), for human review / expert-iteration mining.
    let report = serde_json::json!({
        "schema_version": "refineforge-lean-prover-search-v1",
        "problems": problems.len(),
        "solved": solved,
        "pass_rate": solved as f64 / problems.len() as f64,
        "samples_per_problem": samples,
        "results": results,
    });
    std::fs::write(
        paths.run_dir.join("proof-search-report.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    writeln!(
        log_file,
        "refineforge_lean_prover completed: {solved}/{} solved",
        problems.len()
    )?;

    Ok(LeanProverOutcome {
        progress_records: problems.len(),
    })
}

fn load_problems(path: &Path) -> Result<Vec<Problem>> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str::<Problem>(l).context("parsing a problem line"))
        .collect()
}
