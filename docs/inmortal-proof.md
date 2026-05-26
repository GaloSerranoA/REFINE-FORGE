# InmortalProof

InmortalProof is Refine-Forge's proof-search substrate for Lean claims.
It is inspired by the public AlphaProof-style research pattern: generate
candidate proof sketches, rate them, preserve a replayable search trail,
and let Lean remain the final verifier.

## Current Contract

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

## Design Direction

The enterprise direction is:

1. Protected Lean evolution regions.
2. Goal cache from claim theorems and Lean diagnostics.
3. Candidate proof-sketch population.
4. P-UCB style selection receipts.
5. Search/replace edit API bounded to EVOLVE regions.
6. Rater receipts for compile status, policy status, proof term quality, and
   protected-source stability.
7. Replayable JSONL ledgers for every search episode.
8. Lean verification as the only proof authority.

## Boundary

InmortalProof does not claim to copy AlphaProof, Gemini, or any proprietary
system. It does not claim a theorem is proven until `refine lean check` or a
bundle export verifies the claim through Lean and the no-sorry policy gate.

The current slice is the deterministic substrate and receipt layer. The next
engineering layer is active search: model/tool-generated candidate edits,
Lean feedback ingestion, population promotion, rollback, and final bundle
export when Lean verifies.
