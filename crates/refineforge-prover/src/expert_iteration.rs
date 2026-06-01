//! Stage-1 **expert iteration** — adapt a downloaded prover to *our* exact task by
//! training it on the proofs it already found, **but only the ones Lean verified.**
//!
//! That last clause is the whole trust property: an expert-iteration round mines an
//! SFT dataset from a [`crate::SearchReport`], and only `solved` problems with a
//! `verified_proof` (a candidate the Lean checker accepted) become training
//! examples. The model is never adapted on an unverified proof. Across rounds the
//! [`Corpus`] accumulates verified `(statement → proof)` pairs with dedup, and a
//! [`RoundRecord`] ledger tracks the growth so progress is measurable.
//!
//! The mining, corpus, dedup, and ledger here are pure Rust and fully tested. The
//! actual LoRA/QLoRA fine-tune on the resulting dataset is an **external** step
//! (Python/PyTorch on the operator's GPU — e.g. the 24 GB P40); see
//! `training/scripts/lora_finetune_prover.py` and the runbook.

use crate::{Problem, ProblemResult, SearchReport};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::io::Write as _;
use std::path::Path;

/// Schema tag for the expert-iteration ledger.
pub const LEDGER_SCHEMA: &str = "refineforge-expert-iteration-ledger-v1";

/// One **verified** training pair: a problem statement and a proof the Lean
/// checker accepted. The only thing expert iteration trains on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SftExample {
    pub id: String,
    /// The prompt — the problem statement the prover was given.
    pub prompt: String,
    /// The completion — a Lean-verified proof for that statement.
    pub completion: String,
    #[serde(default)]
    pub split: Option<String>,
    /// Which expert-iteration round produced this example.
    pub round: u32,
}

/// Mine verified proofs from one search round into SFT examples. Pairs each
/// `solved` [`ProblemResult`] with its [`Problem`]'s statement by `id`; skips
/// unsolved problems and any solved result missing a `verified_proof` (defensive —
/// a solve always carries one). **Verified-only by construction.**
pub fn mine_verified(problems: &[Problem], results: &[ProblemResult], round: u32) -> Vec<SftExample> {
    let by_id: std::collections::HashMap<&str, &Problem> =
        problems.iter().map(|p| (p.id.as_str(), p)).collect();
    results
        .iter()
        .filter(|r| r.solved)
        .filter_map(|r| {
            let completion = r.verified_proof.as_ref()?;
            let problem = by_id.get(r.id.as_str())?;
            Some(SftExample {
                id: r.id.clone(),
                prompt: problem.statement.clone(),
                completion: completion.clone(),
                split: problem.split.clone(),
                round,
            })
        })
        .collect()
}

/// Convenience: mine straight from a [`SearchReport`] (its `results`).
pub fn mine_from_report(problems: &[Problem], report: &SearchReport, round: u32) -> Vec<SftExample> {
    mine_verified(problems, &report.results, round)
}

/// A cumulative, deduped corpus of verified examples across expert-iteration
/// rounds. The dedup key is `(id, completion)` — so the *same* proof is never
/// added twice, but a genuinely *different* proof for an already-solved problem
/// still counts (provers find alternate proofs, and variety helps SFT). Keys are
/// the literal strings (stable across processes — no randomized hashing).
#[derive(Debug, Default)]
pub struct Corpus {
    examples: Vec<SftExample>,
    keys: HashSet<(String, String)>,
}

impl Corpus {
    pub fn new() -> Self {
        Self::default()
    }

    /// Load an existing corpus from a JSONL file (one [`SftExample`] per line).
    /// A missing file yields an empty corpus (the first round starts here).
    pub fn load_jsonl(path: &Path) -> Result<Self> {
        let mut corpus = Self::new();
        if !path.exists() {
            return Ok(corpus);
        }
        let text =
            std::fs::read_to_string(path).with_context(|| format!("reading corpus {}", path.display()))?;
        for line in text.lines().filter(|l| !l.trim().is_empty()) {
            let ex: SftExample = serde_json::from_str(line).context("parsing a corpus row")?;
            corpus.insert(ex);
        }
        Ok(corpus)
    }

    fn insert(&mut self, ex: SftExample) -> bool {
        let key = (ex.id.clone(), ex.completion.clone());
        if self.keys.insert(key) {
            self.examples.push(ex);
            true
        } else {
            false
        }
    }

    /// Add a round's mined examples; returns how many were *newly* added (after
    /// dedup against everything already in the corpus).
    pub fn add(&mut self, examples: impl IntoIterator<Item = SftExample>) -> usize {
        examples.into_iter().filter(|ex| self.insert(ex.clone())).count()
    }

    pub fn len(&self) -> usize {
        self.examples.len()
    }
    pub fn is_empty(&self) -> bool {
        self.examples.is_empty()
    }
    pub fn examples(&self) -> &[SftExample] {
        &self.examples
    }

    /// Number of distinct problems with at least one verified proof in the corpus.
    pub fn solved_problems(&self) -> usize {
        self.examples
            .iter()
            .map(|e| e.id.as_str())
            .collect::<HashSet<_>>()
            .len()
    }

    /// Persist the corpus as JSONL (one [`SftExample`] per line) — both the
    /// durable corpus and a directly-trainable `{id,prompt,completion,…}` dataset.
    pub fn write_jsonl(&self, path: &Path) -> Result<()> {
        let mut f = std::fs::File::create(path)
            .with_context(|| format!("creating {}", path.display()))?;
        for ex in &self.examples {
            writeln!(f, "{}", serde_json::to_string(ex)?)?;
        }
        Ok(())
    }

    /// Persist as chat-format JSONL (`{"messages":[user, assistant]}`) for SFT with
    /// a chat template. `system` is an optional system message prepended to each row.
    pub fn write_chat_jsonl(&self, path: &Path, system: Option<&str>) -> Result<()> {
        let mut f = std::fs::File::create(path)
            .with_context(|| format!("creating {}", path.display()))?;
        for ex in &self.examples {
            let mut messages = Vec::new();
            if let Some(sys) = system {
                messages.push(serde_json::json!({"role": "system", "content": sys}));
            }
            messages.push(serde_json::json!({"role": "user", "content": ex.prompt}));
            messages.push(serde_json::json!({"role": "assistant", "content": ex.completion}));
            writeln!(f, "{}", serde_json::to_string(&serde_json::json!({"messages": messages}))?)?;
        }
        Ok(())
    }
}

/// One row of the expert-iteration ledger — the measurable progress of a round.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoundRecord {
    pub round: u32,
    /// Problems searched this round.
    pub problems_searched: usize,
    /// Problems solved (verified) this round.
    pub solved_this_round: usize,
    /// Verified proofs newly added to the corpus this round (after dedup).
    pub newly_added: usize,
    /// Total corpus size after this round.
    pub cumulative_corpus: usize,
    /// Distinct solved problems in the corpus after this round.
    pub cumulative_solved_problems: usize,
    /// This round's pass rate (`solved_this_round / problems_searched`).
    pub pass_rate: f64,
}

/// The expert-iteration ledger: an append-only series of [`RoundRecord`]s.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ledger {
    pub schema_version: String,
    pub rounds: Vec<RoundRecord>,
}

impl Default for Ledger {
    fn default() -> Self {
        Self {
            schema_version: LEDGER_SCHEMA.to_string(),
            rounds: Vec::new(),
        }
    }
}

impl Ledger {
    pub fn new() -> Self {
        Self::default()
    }

    /// Load an existing ledger JSON (missing file ⇒ a fresh ledger).
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::new());
        }
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading ledger {}", path.display()))?;
        serde_json::from_str(&text).context("parsing expert-iteration ledger")
    }

    pub fn push(&mut self, record: RoundRecord) {
        self.rounds.push(record);
    }

    pub fn write(&self, path: &Path) -> Result<()> {
        std::fs::write(path, serde_json::to_vec_pretty(self)?)
            .with_context(|| format!("writing ledger {}", path.display()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solved(id: &str, proof: &str) -> ProblemResult {
        ProblemResult {
            id: id.into(),
            solved: true,
            attempts: 1,
            verified_proof: Some(proof.into()),
            last_detail: None,
        }
    }
    fn unsolved(id: &str) -> ProblemResult {
        ProblemResult {
            id: id.into(),
            solved: false,
            attempts: 4,
            verified_proof: None,
            last_detail: Some("no candidate verified".into()),
        }
    }

    #[test]
    fn mine_keeps_only_verified_proofs() {
        let problems = vec![
            Problem::new("p1", "goal-1"),
            Problem::new("p2", "goal-2"),
            Problem::new("p3", "goal-3"),
        ];
        let results = vec![solved("p1", "by rfl"), unsolved("p2"), solved("p3", "by simp")];
        let mined = mine_verified(&problems, &results, 1);
        assert_eq!(mined.len(), 2, "only the two verified problems are mined");
        assert_eq!(mined[0].prompt, "goal-1");
        assert_eq!(mined[0].completion, "by rfl");
        assert_eq!(mined[1].id, "p3");
        assert!(mined.iter().all(|e| e.round == 1));
    }

    #[test]
    fn mine_skips_unknown_ids() {
        let problems = vec![Problem::new("p1", "g1")];
        // A result whose id has no matching problem is dropped (can't form a prompt).
        let results = vec![solved("ghost", "by rfl")];
        assert!(mine_verified(&problems, &results, 1).is_empty());
    }

    #[test]
    fn corpus_dedups_identical_proofs_but_keeps_alternates() {
        let mut corpus = Corpus::new();
        let r1 = vec![SftExample {
            id: "p1".into(),
            prompt: "g1".into(),
            completion: "by rfl".into(),
            split: None,
            round: 1,
        }];
        assert_eq!(corpus.add(r1.clone()), 1);
        // Same proof again → no growth.
        assert_eq!(corpus.add(r1), 0);
        assert_eq!(corpus.len(), 1);
        // A *different* proof for the same problem → counts (alternate proof).
        let alt = vec![SftExample {
            id: "p1".into(),
            prompt: "g1".into(),
            completion: "by simp".into(),
            split: None,
            round: 2,
        }];
        assert_eq!(corpus.add(alt), 1);
        assert_eq!(corpus.len(), 2);
        assert_eq!(corpus.solved_problems(), 1, "still one distinct problem solved");
    }

    #[test]
    fn corpus_round_trips_through_jsonl() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("corpus.jsonl");
        let mut corpus = Corpus::new();
        corpus.add(mine_verified(
            &[Problem::new("p1", "g1"), Problem::new("p2", "g2")],
            &[solved("p1", "by rfl"), solved("p2", "by omega")],
            1,
        ));
        corpus.write_jsonl(&path).unwrap();

        let reloaded = Corpus::load_jsonl(&path).unwrap();
        assert_eq!(reloaded.len(), 2);
        // Re-adding the same proofs after reload is a no-op (stable dedup keys).
        let mut reloaded = reloaded;
        assert_eq!(
            reloaded.add(mine_verified(
                &[Problem::new("p1", "g1")],
                &[solved("p1", "by rfl")],
                2
            )),
            0
        );
    }

    #[test]
    fn missing_corpus_file_loads_empty() {
        let dir = tempfile::tempdir().unwrap();
        let corpus = Corpus::load_jsonl(&dir.path().join("nope.jsonl")).unwrap();
        assert!(corpus.is_empty());
    }

    #[test]
    fn chat_jsonl_has_user_and_assistant_turns() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("chat.jsonl");
        let mut corpus = Corpus::new();
        corpus.add(vec![SftExample {
            id: "p1".into(),
            prompt: "prove g1".into(),
            completion: "by rfl".into(),
            split: None,
            round: 1,
        }]);
        corpus.write_chat_jsonl(&path, Some("You are a Lean prover.")).unwrap();
        let line = std::fs::read_to_string(&path).unwrap();
        let v: serde_json::Value = serde_json::from_str(line.lines().next().unwrap()).unwrap();
        let msgs = v["messages"].as_array().unwrap();
        assert_eq!(msgs[0]["role"], "system");
        assert_eq!(msgs[1]["role"], "user");
        assert_eq!(msgs[1]["content"], "prove g1");
        assert_eq!(msgs[2]["role"], "assistant");
        assert_eq!(msgs[2]["content"], "by rfl");
    }

    #[test]
    fn ledger_accumulates_rounds() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ledger.json");
        let mut ledger = Ledger::load(&path).unwrap(); // missing → fresh
        ledger.push(RoundRecord {
            round: 1,
            problems_searched: 10,
            solved_this_round: 4,
            newly_added: 4,
            cumulative_corpus: 4,
            cumulative_solved_problems: 4,
            pass_rate: 0.4,
        });
        ledger.write(&path).unwrap();

        let mut ledger = Ledger::load(&path).unwrap();
        assert_eq!(ledger.rounds.len(), 1);
        ledger.push(RoundRecord {
            round: 2,
            problems_searched: 10,
            solved_this_round: 6,
            newly_added: 3,
            cumulative_corpus: 7,
            cumulative_solved_problems: 6,
            pass_rate: 0.6,
        });
        ledger.write(&path).unwrap();
        let ledger = Ledger::load(&path).unwrap();
        assert_eq!(ledger.rounds.len(), 2);
        assert_eq!(ledger.schema_version, LEDGER_SCHEMA);
        assert_eq!(ledger.rounds[1].cumulative_corpus, 7);
    }
}
