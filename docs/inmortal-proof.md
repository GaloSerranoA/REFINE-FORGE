# InmortalProof

InmortalProof is Refine-Forge's proof-search substrate for Lean claims.
It is inspired by the public AlphaProof-style research pattern: generate
candidate proof sketches, rate them, preserve a replayable search trail,
and let Lean remain the final verifier.

## Receipt Contract

`refine inmortal-proof run <CLAIM-ID>` prepares a deterministic receipt pack:

- `inmortal-proof-report.json` - machine-readable run receipt.
- `inmortal-proof-report.md` - reviewer-readable summary.
- `population.jsonl` - proof-sketch population seed.
- `episodes.jsonl` - deterministic search episode ledger.
- `goal-cache.jsonl` - theorem-to-goal cache seed.

The run scans the claim's Lean file for editable regions:

```lean
-- EVOLVE-BLOCK-START
-- proof body or helper declarations
-- EVOLVE-BLOCK-END

-- EVOLVE-VALUE-START
-- small value/expression candidate
-- EVOLVE-VALUE-END
```

Everything outside those regions is treated as protected source. The report
hashes the full source and a protected-source projection so future proof search
can mutate only declared regions without silently changing theorem statements,
imports, or surrounding trusted text.

## Active Search Contract

`refine inmortal-proof search <CLAIM-ID>` executes a bounded active search over
declared EVOLVE regions. It generates deterministic built-in candidates,
applies each candidate only inside one EVOLVE region, records validation
feedback, and rolls the Lean source back unless `--retain-verified` is passed.

Default validation is `--validator lean`, which writes each candidate to the
Lean file and calls the normal Refine-Forge Lean runner. A candidate is a proof
candidate only when Lean returns `verified` and the no-sorry policy gate passes.
When that happens, InmortalProof can export a proof bundle under the search
output directory.

`--validator receipt-only` exists for deterministic development and CI smoke
tests on machines without a configured Lean toolchain. It checks policy and
receipt integrity, but it does not prove anything and the public claim remains
`inmortalproof_search_receipt_only_not_lean_verified`.

Search output files:

- `inmortal-proof-search-report.json` - machine-readable active-search report.
- `inmortal-proof-search-report.md` - reviewer-readable search summary.
- `candidates.jsonl` - generated candidate edit ledger (each line includes
  `heuristic_score`).
- `search-episodes.jsonl` - attempted candidate and outcome ledger.
- `lean-feedback.jsonl` - validation feedback for each attempt.
- `population.jsonl` - current proof-sketch population view.

## Best-first ranking

`--ranking best-first` reorders the candidate queue by a deterministic
hand-tuned prior heuristic before the search loop runs. The default,
`--ranking insertion-order`, preserves the deterministic generator order
(unchanged baseline behavior).

The heuristic is a static per-generator table reflecting Lean tactic
cheapness and failure cost: cheaper, lower-failure tactics rank higher.
For block regions: `builtin_preserve` (1.00) > `rfl` (0.95) > `exact True.intro`
(0.90) > `trivial` (0.85) > `simp` (0.70) > `decide` (0.60) > `simp_all` (0.55).
For value regions: `builtin_preserve` (1.00) > `()` (0.85) > `true` (0.80) >
`false` (0.75) > `0` (0.70) > `1` (0.65). Unknown bodies fall back to 0.50.

This is a hand-tuned tactic prior, NOT a learned value function, NOT a
policy network, and NOT trained on prior episodes. It is identical across
runs and across machines. Each persisted candidate carries its
`heuristic_score` so the ranking is auditable, and the report's
`ranking_mode` field records which mode was used.

## Design Direction

The enterprise direction is:

1. Protected Lean evolution regions.
2. Goal cache from claim theorems and Lean diagnostics.
3. Candidate proof-sketch population.
4. P-UCB style selection receipts.
5. Search/replace edit API bounded to EVOLVE regions.
6. Lean feedback ingestion, failed-candidate rollback, and verified-candidate
   promotion.
7. Rater receipts for compile status, policy status, proof term quality, and
   protected-source stability.
8. Replayable JSONL ledgers for every search episode.
9. Lean verification as the only proof authority.

## Boundary

InmortalProof does not claim to copy AlphaProof, Gemini, or any proprietary
system. It does not claim a theorem is proven until `refine lean check` or a
bundle export verifies the claim through Lean and the no-sorry policy gate.

The current slice includes the deterministic receipt layer, bounded active
search with a built-in candidate bank, and opt-in best-first ranking via a
deterministic hand-tuned prior heuristic. The next engineering layer is richer
candidate generation: local model proposals, Lean goal-state extraction,
cross-region compound edits, and larger proof-search populations.
