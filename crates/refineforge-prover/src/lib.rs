//! GPU-agnostic Lean **proof-search orchestration** — the Rust glue that turns a
//! *downloaded* open prover (DeepSeek-Prover-V2-7B, Goedel-Prover-SFT,
//! Kimina-Prover-7B, …) into a trust-bearing Refine-Forge agent.
//!
//! The expensive part — pretraining + RL of the prover — was already done by the
//! model authors and released as open weights, so we inherit it for ≈$0. What is
//! *ours* is everything around the model: generate best-of-k proof candidates,
//! **gate every candidate with the Lean checker** (the reward / trust boundary),
//! keep only verified proofs, and emit M14-style trust evidence.
//!
//! ## Why this is GPU-agnostic
//!
//! This crate never touches CUDA. It talks to a prover *inference server* over an
//! OpenAI-compatible HTTP endpoint ([`OpenAiProver`]); the server (vLLM /
//! llama.cpp) owns the GPU. So the heterogeneous reality of a workstation —
//! e.g. a **P40 (24 GB, Pascal, weak fp16)** beside an **RTX 5080 (16 GB,
//! Blackwell, fast fp16)** — is the *server's* problem: run the 7B prover on the
//! 5080, optionally stand up a second worker on the P40, and point this harness
//! at one (or both) endpoints. Swapping or adding a GPU changes nothing here.
//!
//! ## Honest scope
//!
//! This is the **inference-only Stage-0** harness: download a prover, search
//! proofs gated by Lean, collect verified ones. It does *not* train or RL the
//! prover (that is a separate, optional expert-iteration step). The prover server
//! and the Lean toolchain are **provided by the operator** — this crate is the
//! orchestration only, and is fully unit-tested against [`mock`] doubles so the
//! search logic is verified without a live GPU or Lean install.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::Write as _;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

/// Schema tag stamped into [`SearchReport`] for downstream evidence consumers.
pub const SEARCH_REPORT_SCHEMA: &str = "refineforge-lean-prover-search-v1";

// ─────────────────────────────────────────────────────────────────────────────
// Problem / result types
// ─────────────────────────────────────────────────────────────────────────────

/// One theorem-proving problem handed to the prover.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Problem {
    /// Stable identifier (used in evidence + per-problem result rows).
    pub id: String,
    /// What the prover sees — the prompt. Typically a Lean theorem statement with
    /// a `sorry`/goal the prover must complete, plus any few-shot scaffolding the
    /// operator's prompt template adds.
    pub statement: String,
    /// Optional split label (`train` / `dev` / `heldout`) for evidence bookkeeping.
    #[serde(default)]
    pub split: Option<String>,
    /// Optional Lean source template with a `{{proof}}` placeholder. When present,
    /// the verifier substitutes a candidate into it before checking (the
    /// "fill-the-hole" case). When absent, a candidate is treated as a complete
    /// Lean file. See [`assemble`].
    #[serde(default)]
    pub template: Option<String>,
}

impl Problem {
    /// Construct a problem whose candidates are complete Lean files.
    pub fn new(id: impl Into<String>, statement: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            statement: statement.into(),
            split: None,
            template: None,
        }
    }
}

/// Splice a candidate into a problem's Lean source: fills the `{{proof}}`
/// placeholder when a [`Problem::template`] is set, otherwise returns the
/// candidate verbatim (it is assumed to be a complete Lean file).
pub fn assemble(problem: &Problem, candidate: &str) -> String {
    match &problem.template {
        Some(t) => t.replace("{{proof}}", candidate),
        None => candidate.to_string(),
    }
}

/// Outcome of searching proofs for a single problem.
#[derive(Debug, Clone, Serialize)]
pub struct ProblemResult {
    pub id: String,
    /// True iff at least one generated candidate passed the verifier.
    pub solved: bool,
    /// How many candidates were verified before stopping (the winning index for a
    /// solve; the total tried for a miss). Best-of-k stops at the first success.
    pub attempts: usize,
    /// The first verified proof, if any.
    pub verified_proof: Option<String>,
    /// Verifier detail for the last failing attempt (e.g. the Lean error), if unsolved.
    pub last_detail: Option<String>,
}

/// Aggregate report over a problem set — the proof-search trust evidence.
#[derive(Debug, Clone, Serialize)]
pub struct SearchReport {
    pub schema_version: String,
    pub problems: usize,
    pub solved: usize,
    /// `solved / problems` (0.0 for an empty set).
    pub pass_rate: f64,
    /// k — the best-of-k sampling budget per problem.
    pub samples_per_problem: usize,
    pub results: Vec<ProblemResult>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Prover client (generation) + Verifier (the trust gate)
// ─────────────────────────────────────────────────────────────────────────────

/// A source of proof candidates — usually a locally-served open prover.
pub trait ProverClient {
    /// Generate up to `n` candidate proofs for `prompt`. Implementations may
    /// return fewer than `n` (the search treats that as the available budget).
    fn complete(&self, prompt: &str, n: usize) -> Result<Vec<String>>;
}

/// The verifier's judgement of one candidate.
#[derive(Debug, Clone)]
pub struct Verdict {
    /// True iff the candidate is an accepted proof (the Lean checker exited clean).
    pub verified: bool,
    /// Human-readable detail (empty on success; the checker's error on failure).
    pub detail: String,
}

/// Checks whether a candidate proof actually holds — the reward signal and the
/// trust boundary. The only thing that can mark a proof "accepted".
pub trait Verifier {
    fn verify(&self, problem: &Problem, candidate: &str) -> Result<Verdict>;
}

// ─────────────────────────────────────────────────────────────────────────────
// The search loop
// ─────────────────────────────────────────────────────────────────────────────

/// Best-of-k proof search: for each problem, generate k candidates and return the
/// first the verifier accepts.
pub struct ProofSearch<'a> {
    prover: &'a dyn ProverClient,
    verifier: &'a dyn Verifier,
    /// k — candidates requested per problem (at least 1).
    samples: usize,
}

impl<'a> ProofSearch<'a> {
    pub fn new(prover: &'a dyn ProverClient, verifier: &'a dyn Verifier, samples: usize) -> Self {
        Self {
            prover,
            verifier,
            samples: samples.max(1),
        }
    }

    /// Search one problem: generate k candidates, verify in order, stop at the
    /// first accepted proof (best-of-k). Generation/verification errors propagate.
    pub fn solve(&self, problem: &Problem) -> Result<ProblemResult> {
        let candidates = self
            .prover
            .complete(&problem.statement, self.samples)
            .with_context(|| format!("prover generation failed for `{}`", problem.id))?;
        let mut last_detail = None;
        for (i, candidate) in candidates.iter().enumerate() {
            let verdict = self
                .verifier
                .verify(problem, candidate)
                .with_context(|| format!("verifier failed for `{}` (attempt {})", problem.id, i + 1))?;
            if verdict.verified {
                return Ok(ProblemResult {
                    id: problem.id.clone(),
                    solved: true,
                    attempts: i + 1,
                    verified_proof: Some(candidate.clone()),
                    last_detail: None,
                });
            }
            last_detail = Some(verdict.detail);
        }
        Ok(ProblemResult {
            id: problem.id.clone(),
            solved: false,
            attempts: candidates.len(),
            verified_proof: None,
            last_detail,
        })
    }

    /// Run the search over a problem set, streaming one [`progress.jsonl`] record
    /// per problem into `run_dir` and writing the aggregate `proof-search-report.json`.
    ///
    /// The `progress.jsonl` rows are deliberately
    /// [`refineforge-trainer`-`ProgressRecord`-compatible](timestamp / raw /
    /// metrics / step) so the existing trust tooling can consume this run; the
    /// load-bearing metric is the honest `proof_pass_rate` (cumulative fraction of
    /// problems with an accepted proof), never dressed up as token accuracy.
    pub fn run(&self, problems: &[Problem], run_dir: &Path) -> Result<SearchReport> {
        std::fs::create_dir_all(run_dir)
            .with_context(|| format!("creating run dir {}", run_dir.display()))?;
        let mut progress = std::fs::File::create(run_dir.join("progress.jsonl"))
            .context("opening progress.jsonl")?;

        let mut results = Vec::with_capacity(problems.len());
        let mut solved = 0usize;
        for (i, problem) in problems.iter().enumerate() {
            let step = i + 1;
            let result = self.solve(problem)?;
            if result.solved {
                solved += 1;
            }
            let pass_rate = solved as f64 / step as f64;

            let mut metrics = BTreeMap::new();
            metrics.insert("proof_pass_rate".to_string(), pass_rate);
            metrics.insert("solved".to_string(), solved as f64);
            metrics.insert("attempts".to_string(), result.attempts as f64);
            metrics.insert("samples".to_string(), self.samples as f64);

            let record = serde_json::json!({
                "timestamp": chrono::Utc::now().to_rfc3339(),
                "raw": format!(
                    "step={step} id={} solved={} attempts={} proof_pass_rate={pass_rate:.6}",
                    result.id, result.solved, result.attempts
                ),
                "metrics": metrics,
                "step": step,
            });
            writeln!(progress, "{}", serde_json::to_string(&record)?)
                .context("writing progress.jsonl record")?;
            results.push(result);
        }
        progress.flush().ok();

        let report = SearchReport {
            schema_version: SEARCH_REPORT_SCHEMA.to_string(),
            problems: problems.len(),
            solved,
            pass_rate: if problems.is_empty() {
                0.0
            } else {
                solved as f64 / problems.len() as f64
            },
            samples_per_problem: self.samples,
            results,
        };
        std::fs::write(
            run_dir.join("proof-search-report.json"),
            serde_json::to_vec_pretty(&report).context("serializing report")?,
        )
        .context("writing proof-search-report.json")?;
        Ok(report)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// OpenAI-compatible prover client (vLLM / llama.cpp servers)
// ─────────────────────────────────────────────────────────────────────────────

/// Which OpenAI-style endpoint the server exposes for the prover.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProverApi {
    /// `POST /v1/completions` — a raw text prompt → `choices[].text`.
    Completion,
    /// `POST /v1/chat/completions` — a chat message → `choices[].message.content`.
    Chat,
}

/// A prover served behind an OpenAI-compatible HTTP API (vLLM `--api`,
/// `llama-server`, etc.). Blocking; one request yields up to `n` candidates.
pub struct OpenAiProver {
    base_url: String,
    model: String,
    api: ProverApi,
    max_tokens: usize,
    temperature: f64,
    stop: Vec<String>,
    api_key: Option<String>,
    client: reqwest::blocking::Client,
}

impl OpenAiProver {
    /// New prover client. `base_url` is the server root (e.g.
    /// `http://localhost:8000`), `model` the served model name. Defaults:
    /// completion API, 1024 max tokens, temperature 0.8 (best-of-k wants
    /// diversity), 60 s timeout, no stop strings, no auth.
    pub fn new(base_url: impl Into<String>, model: impl Into<String>) -> Result<Self> {
        Ok(Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            model: model.into(),
            api: ProverApi::Completion,
            max_tokens: 1024,
            temperature: 0.8,
            stop: Vec::new(),
            api_key: None,
            client: reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(60))
                .build()
                .context("building HTTP client")?,
        })
    }

    pub fn with_api(mut self, api: ProverApi) -> Self {
        self.api = api;
        self
    }
    pub fn with_max_tokens(mut self, n: usize) -> Self {
        self.max_tokens = n;
        self
    }
    pub fn with_temperature(mut self, t: f64) -> Self {
        self.temperature = t;
        self
    }
    pub fn with_stop(mut self, stop: Vec<String>) -> Self {
        self.stop = stop;
        self
    }
    pub fn with_api_key(mut self, key: Option<String>) -> Self {
        self.api_key = key;
        self
    }

    fn endpoint(&self) -> String {
        match self.api {
            ProverApi::Completion => format!("{}/v1/completions", self.base_url),
            ProverApi::Chat => format!("{}/v1/chat/completions", self.base_url),
        }
    }

    fn request_body(&self, prompt: &str, n: usize) -> serde_json::Value {
        let mut body = serde_json::json!({
            "model": self.model,
            "n": n,
            "max_tokens": self.max_tokens,
            "temperature": self.temperature,
        });
        if !self.stop.is_empty() {
            body["stop"] = serde_json::json!(self.stop);
        }
        match self.api {
            ProverApi::Completion => {
                body["prompt"] = serde_json::json!(prompt);
            }
            ProverApi::Chat => {
                body["messages"] = serde_json::json!([{ "role": "user", "content": prompt }]);
            }
        }
        body
    }
}

impl ProverClient for OpenAiProver {
    fn complete(&self, prompt: &str, n: usize) -> Result<Vec<String>> {
        let mut req = self.client.post(self.endpoint()).json(&self.request_body(prompt, n));
        if let Some(key) = &self.api_key {
            req = req.bearer_auth(key);
        }
        let resp = req
            .send()
            .with_context(|| format!("POST {} failed", self.endpoint()))?
            .error_for_status()
            .context("prover server returned an error status")?;
        let value: serde_json::Value = resp.json().context("decoding prover response JSON")?;
        match self.api {
            ProverApi::Completion => parse_completion_response(&value),
            ProverApi::Chat => parse_chat_response(&value),
        }
    }
}

/// Extract `choices[].text` from an OpenAI `/v1/completions` response.
pub fn parse_completion_response(value: &serde_json::Value) -> Result<Vec<String>> {
    let choices = value
        .get("choices")
        .and_then(|c| c.as_array())
        .context("response missing `choices` array")?;
    Ok(choices
        .iter()
        .filter_map(|c| c.get("text").and_then(|t| t.as_str()).map(str::to_string))
        .collect())
}

/// Extract `choices[].message.content` from an OpenAI `/v1/chat/completions` response.
pub fn parse_chat_response(value: &serde_json::Value) -> Result<Vec<String>> {
    let choices = value
        .get("choices")
        .and_then(|c| c.as_array())
        .context("response missing `choices` array")?;
    Ok(choices
        .iter()
        .filter_map(|c| {
            c.get("message")
                .and_then(|m| m.get("content"))
                .and_then(|t| t.as_str())
                .map(str::to_string)
        })
        .collect())
}

/// Replays pre-generated candidates from a JSONL file instead of calling a live
/// prover — for offline dry-runs, deterministic pipeline tests, and re-verifying
/// a past generation batch without re-spending GPU time. Each line is
/// `{"prompt": "<the problem statement>", "candidates": ["<proof>", …]}`.
pub struct ReplayProver {
    by_prompt: std::collections::HashMap<String, Vec<String>>,
}

impl ReplayProver {
    /// Load a replay map from a JSONL file (one `{prompt, candidates}` per line).
    pub fn from_jsonl(path: &Path) -> Result<Self> {
        #[derive(Deserialize)]
        struct Row {
            prompt: String,
            candidates: Vec<String>,
        }
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading replay file {}", path.display()))?;
        let mut by_prompt = std::collections::HashMap::new();
        for line in text.lines().filter(|l| !l.trim().is_empty()) {
            let row: Row = serde_json::from_str(line).context("parsing a replay row")?;
            by_prompt.insert(row.prompt, row.candidates);
        }
        Ok(Self { by_prompt })
    }
}

impl ProverClient for ReplayProver {
    fn complete(&self, prompt: &str, n: usize) -> Result<Vec<String>> {
        Ok(self
            .by_prompt
            .get(prompt)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .take(n)
            .collect())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Command verifier (the Lean checker)
// ─────────────────────────────────────────────────────────────────────────────

/// Verifies a candidate by writing it (via [`assemble`]) to a file inside a Lean
/// project and running a checker command — the canonical use is
/// `lake env lean <file>`, exit 0 ⇒ accepted. The project root (`work_dir`) must
/// be a real Lean/Mathlib project on the operator's machine; this crate only
/// orchestrates the invocation.
pub struct CommandVerifier {
    program: String,
    args: Vec<String>,
    work_dir: std::path::PathBuf,
    candidate_file: String,
}

impl CommandVerifier {
    /// `program`/`args` form the checker command (the candidate file path is
    /// appended as the final argument); `work_dir` is the Lean project root; the
    /// candidate is written to `work_dir/candidate_file` before each check.
    ///
    /// Example: `CommandVerifier::new("lake", ["env", "lean"], "lean/", "Candidate.lean")`.
    pub fn new(
        program: impl Into<String>,
        args: impl IntoIterator<Item = impl Into<String>>,
        work_dir: impl Into<std::path::PathBuf>,
        candidate_file: impl Into<String>,
    ) -> Self {
        Self {
            program: program.into(),
            args: args.into_iter().map(Into::into).collect(),
            work_dir: work_dir.into(),
            candidate_file: candidate_file.into(),
        }
    }
}

impl Verifier for CommandVerifier {
    fn verify(&self, problem: &Problem, candidate: &str) -> Result<Verdict> {
        let path = self.work_dir.join(&self.candidate_file);
        std::fs::write(&path, assemble(problem, candidate))
            .with_context(|| format!("writing candidate to {}", path.display()))?;
        let output = Command::new(&self.program)
            .args(&self.args)
            .arg(&path)
            .current_dir(&self.work_dir)
            .output()
            .with_context(|| format!("running checker `{}`", self.program))?;
        if output.status.success() {
            Ok(Verdict {
                verified: true,
                detail: String::new(),
            })
        } else {
            // Cap the captured error so a noisy checker can't blow up evidence.
            let detail: String = String::from_utf8_lossy(&output.stderr)
                .chars()
                .take(2000)
                .collect();
            Ok(Verdict {
                verified: false,
                detail,
            })
        }
    }
}

/// A **dry-run** verifier that accepts a candidate iff it contains `needle`.
///
/// This is NOT a proof verifier — it does no Lean checking and grants no trust.
/// Its only job is to exercise the orchestration plumbing (generation → gate →
/// evidence) end-to-end *before* a Lean toolchain is stood up, so the harness can
/// be wired and tested offline. For any real / trust-bearing run, use
/// [`CommandVerifier`] (the actual Lean checker).
pub struct DryRunVerifier {
    pub needle: String,
}

impl DryRunVerifier {
    pub fn new(needle: impl Into<String>) -> Self {
        Self {
            needle: needle.into(),
        }
    }
}

impl Verifier for DryRunVerifier {
    fn verify(&self, _problem: &Problem, candidate: &str) -> Result<Verdict> {
        let verified = candidate.contains(&self.needle);
        Ok(Verdict {
            verified,
            detail: if verified {
                String::new()
            } else {
                format!("dry-run: missing `{}` (NOT a Lean check)", self.needle)
            },
        })
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Mock doubles — verify the orchestration without a live prover or Lean install
// ─────────────────────────────────────────────────────────────────────────────

/// Test/demo doubles. These let the search logic be unit-tested offline; they are
/// **not** a substitute for a real prover or the Lean checker.
pub mod mock {
    use super::*;
    use std::collections::HashMap;

    /// Returns the same candidate list (truncated to `n`) for every prompt.
    pub struct FixedProver {
        pub candidates: Vec<String>,
    }
    impl FixedProver {
        pub fn new<S: Into<String>>(candidates: impl IntoIterator<Item = S>) -> Self {
            Self {
                candidates: candidates.into_iter().map(Into::into).collect(),
            }
        }
    }
    impl ProverClient for FixedProver {
        fn complete(&self, _prompt: &str, n: usize) -> Result<Vec<String>> {
            Ok(self.candidates.iter().take(n).cloned().collect())
        }
    }

    /// Maps each prompt to a scripted candidate list (unknown prompt ⇒ none).
    pub struct ScriptedProver {
        pub by_prompt: HashMap<String, Vec<String>>,
    }
    impl ScriptedProver {
        pub fn new() -> Self {
            Self {
                by_prompt: HashMap::new(),
            }
        }
        pub fn on<S: Into<String>>(mut self, prompt: &str, candidates: impl IntoIterator<Item = S>) -> Self {
            self.by_prompt
                .insert(prompt.to_string(), candidates.into_iter().map(Into::into).collect());
            self
        }
    }
    impl Default for ScriptedProver {
        fn default() -> Self {
            Self::new()
        }
    }
    impl ProverClient for ScriptedProver {
        fn complete(&self, prompt: &str, n: usize) -> Result<Vec<String>> {
            Ok(self
                .by_prompt
                .get(prompt)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .take(n)
                .collect())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::mock::*;
    use super::*;

    #[test]
    fn best_of_k_returns_first_verified() {
        let prover = FixedProver::new(["nope", "still nope", "theorem t : True := by trivial"]);
        let verifier = DryRunVerifier::new("trivial");
        let search = ProofSearch::new(&prover, &verifier, 8);
        let r = search.solve(&Problem::new("t1", "prove True")).unwrap();
        assert!(r.solved);
        assert_eq!(r.attempts, 3, "best-of-k should stop at the first verified candidate");
        assert!(r.verified_proof.unwrap().contains("trivial"));
        assert!(r.last_detail.is_none());
    }

    #[test]
    fn unsolved_when_no_candidate_verifies() {
        let prover = FixedProver::new(["nope", "also nope"]);
        let verifier = DryRunVerifier::new("trivial");
        let search = ProofSearch::new(&prover, &verifier, 8);
        let r = search.solve(&Problem::new("t2", "prove True")).unwrap();
        assert!(!r.solved);
        assert_eq!(r.attempts, 2);
        assert!(r.verified_proof.is_none());
        assert!(r.last_detail.as_deref().unwrap().contains("missing"));
    }

    #[test]
    fn samples_caps_the_candidate_budget() {
        // Prover offers 5, but k=2 — only 2 may be drawn, so an unsolved miss
        // reports exactly 2 attempts (the verifier never sees candidates 3..5).
        let prover = FixedProver::new(["a", "b", "c", "d", "e"]);
        let verifier = DryRunVerifier::new("zzz");
        let search = ProofSearch::new(&prover, &verifier, 2);
        let r = search.solve(&Problem::new("t3", "x")).unwrap();
        assert_eq!(r.attempts, 2);
        assert!(!r.solved);
    }

    #[test]
    fn samples_floored_at_one() {
        let prover = FixedProver::new(["only"]);
        let verifier = DryRunVerifier::new("only");
        let search = ProofSearch::new(&prover, &verifier, 0);
        assert_eq!(search.samples, 1);
        assert!(search.solve(&Problem::new("t4", "x")).unwrap().solved);
    }

    #[test]
    fn run_aggregates_and_writes_evidence() {
        let dir = tempfile::tempdir().unwrap();
        // P1 and P3 have a verifiable candidate; P2 does not → 2/3 solved.
        let prover = ScriptedProver::new()
            .on("goal-1", ["bad", "by rfl"])
            .on("goal-2", ["bad", "worse"])
            .on("goal-3", ["by rfl"]);
        let verifier = DryRunVerifier::new("rfl");
        let search = ProofSearch::new(&prover, &verifier, 4);
        let problems = vec![
            Problem::new("p1", "goal-1"),
            Problem::new("p2", "goal-2"),
            Problem::new("p3", "goal-3"),
        ];
        let report = search.run(&problems, dir.path()).unwrap();

        assert_eq!(report.problems, 3);
        assert_eq!(report.solved, 2);
        assert!((report.pass_rate - 2.0 / 3.0).abs() < 1e-9);
        assert_eq!(report.samples_per_problem, 4);
        assert!(!report.results[1].solved);
        assert_eq!(report.results[0].attempts, 2);

        // progress.jsonl: one trainer-compatible record per problem.
        let progress = std::fs::read_to_string(dir.path().join("progress.jsonl")).unwrap();
        let lines: Vec<&str> = progress.lines().collect();
        assert_eq!(lines.len(), 3);
        let last: serde_json::Value = serde_json::from_str(lines[2]).unwrap();
        assert_eq!(last["step"], 3);
        assert!(last.get("timestamp").is_some());
        assert!((last["metrics"]["proof_pass_rate"].as_f64().unwrap() - 2.0 / 3.0).abs() < 1e-9);

        // proof-search-report.json round-trips.
        let raw = std::fs::read_to_string(dir.path().join("proof-search-report.json")).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(parsed["schema_version"], SEARCH_REPORT_SCHEMA);
        assert_eq!(parsed["solved"], 2);
    }

    #[test]
    fn run_is_deterministic_for_a_fixed_prover() {
        let prover = FixedProver::new(["by rfl"]);
        let verifier = DryRunVerifier::new("rfl");
        let problems = vec![Problem::new("p1", "g1"), Problem::new("p2", "g2")];
        let a = ProofSearch::new(&prover, &verifier, 1)
            .run(&problems, tempfile::tempdir().unwrap().path())
            .unwrap();
        let b = ProofSearch::new(&prover, &verifier, 1)
            .run(&problems, tempfile::tempdir().unwrap().path())
            .unwrap();
        assert_eq!(a.solved, b.solved);
        assert_eq!(a.pass_rate, b.pass_rate);
    }

    #[test]
    fn assemble_fills_template_or_passes_through() {
        let with_t = Problem {
            id: "x".into(),
            statement: "s".into(),
            split: None,
            template: Some("theorem t : P := {{proof}}".into()),
        };
        assert_eq!(assemble(&with_t, "by simp"), "theorem t : P := by simp");
        let without = Problem::new("y", "s");
        assert_eq!(assemble(&without, "whole file"), "whole file");
    }

    #[test]
    fn parse_completion_response_extracts_choices() {
        let v: serde_json::Value = serde_json::from_str(
            r#"{"choices":[{"text":"proof A"},{"text":"proof B"}]}"#,
        )
        .unwrap();
        assert_eq!(parse_completion_response(&v).unwrap(), vec!["proof A", "proof B"]);
    }

    #[test]
    fn parse_chat_response_extracts_messages() {
        let v: serde_json::Value = serde_json::from_str(
            r#"{"choices":[{"message":{"role":"assistant","content":"by rfl"}}]}"#,
        )
        .unwrap();
        assert_eq!(parse_chat_response(&v).unwrap(), vec!["by rfl"]);
    }

    #[test]
    fn parse_rejects_malformed_response() {
        let v: serde_json::Value = serde_json::from_str(r#"{"error":"oom"}"#).unwrap();
        assert!(parse_completion_response(&v).is_err());
    }

    #[test]
    fn request_body_shapes_match_the_api() {
        let p = OpenAiProver::new("http://localhost:8000/", "deepseek-prover-v2-7b").unwrap();
        let comp = p.request_body("the goal", 4);
        assert_eq!(comp["prompt"], "the goal");
        assert_eq!(comp["n"], 4);
        assert!(comp.get("messages").is_none());

        let chat = p.with_api(ProverApi::Chat).request_body("the goal", 2);
        assert_eq!(chat["messages"][0]["content"], "the goal");
        assert_eq!(chat["messages"][0]["role"], "user");
        assert!(chat.get("prompt").is_none());
    }

    #[test]
    fn base_url_trailing_slash_is_normalized() {
        let p = OpenAiProver::new("http://localhost:8000/", "m").unwrap();
        assert_eq!(p.endpoint(), "http://localhost:8000/v1/completions");
    }

    #[test]
    fn command_verifier_accepts_zero_exit_and_rejects_nonzero() {
        // Drive a real subprocess deterministically without Lean: on Windows use
        // `cmd /c exit <code>`; elsewhere `sh -c "exit <code>"`. The candidate's
        // first char picks the exit code, exercising the success + failure paths.
        let dir = tempfile::tempdir().unwrap();
        #[cfg(windows)]
        let make = |code: &str| CommandVerifier::new("cmd", ["/c", "exit", code], dir.path(), "C.lean");
        #[cfg(not(windows))]
        let make = |code: &str| {
            CommandVerifier::new("sh", ["-c", &format!("exit {code}")], dir.path(), "C.lean")
        };
        let problem = Problem::new("p", "g");

        let ok = make("0").verify(&problem, "good proof").unwrap();
        assert!(ok.verified);
        // The candidate was written to the work dir (the checker would read it).
        assert_eq!(
            std::fs::read_to_string(dir.path().join("C.lean")).unwrap(),
            "good proof"
        );

        let bad = make("1").verify(&problem, "bad proof").unwrap();
        assert!(!bad.verified);
    }

    #[test]
    fn replay_prover_loads_and_serves_candidates() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("replay.jsonl");
        std::fs::write(
            &path,
            "{\"prompt\":\"goal-1\",\"candidates\":[\"by rfl\",\"by simp\"]}\n\
             {\"prompt\":\"goal-2\",\"candidates\":[\"sorry\"]}\n",
        )
        .unwrap();
        let prover = ReplayProver::from_jsonl(&path).unwrap();
        assert_eq!(prover.complete("goal-1", 8).unwrap(), vec!["by rfl", "by simp"]);
        assert_eq!(prover.complete("goal-1", 1).unwrap(), vec!["by rfl"]);
        assert!(prover.complete("unknown-goal", 8).unwrap().is_empty());
    }

    #[test]
    fn dry_run_verifier_is_substring_and_clearly_labeled() {
        let v = DryRunVerifier::new("rfl");
        let p = Problem::new("x", "g");
        assert!(v.verify(&p, "by rfl").unwrap().verified);
        let miss = v.verify(&p, "by simp").unwrap();
        assert!(!miss.verified);
        assert!(miss.detail.contains("NOT a Lean check"), "must not be mistaken for real verification");
    }
}
