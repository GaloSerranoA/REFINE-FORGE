# Repair-loop evaluation

How we measure whether `refine repair` is actually any good.
Owned by the **ML Training Engineer** section
([ARCHITECTURE.md](../ARCHITECTURE.md) §2).

> **Status (0.1.0):** This is the *design* doc. The shipped
> `MockStrategy` declines every proposal, so a baseline number
> exists (0 % repair rate) but there is no harness to compare
> strategies against each other yet. Building that harness is
> Section 2 phase 1, after `AnthropicStrategy` lands.

## 1. What we're measuring

A repair strategy `S` is evaluated on a corpus of *broken* Lean
proofs `B = {b_1, b_2, ..., b_n}` paired with the corresponding
*ground-truth fixed* proofs `F = {f_1, f_2, ..., f_n}`. The
metrics:

| Metric | Definition | Why it matters |
|---|---|---|
| **Repair rate** | fraction of `b_i` that `S` fixes within `max_iterations` such that `lake build` + no-sorry gate accept | The headline number. "Does the strategy work?" |
| **Iterations to fix** | distribution of `len(report.iterations)` over fixed cases | Cost-per-success: a strategy that needs 5 iterations is 5× more expensive than one that needs 1 |
| **Latency per iteration** | wall-clock time per `propose_patch` call | Operator UX: anything > 30s per iteration is painful |
| **Cost per attempted repair** | (latency × hourly compute) + (tokens × price-per-token if applicable) | Operational viability for a paid model |
| **False-fix rate** | fraction of "fixed" cases where the strategy's patch semantically differs from the ground truth `f_i` (even though both type-check) | The repair loop's HARDEST property — a patch that compiles but means something different is a refinement-doc invalidator |
| **Honesty rate** | fraction of attempts where `S` either succeeds OR cleanly declines, vs. attempts where `S` proposes a patch the no-sorry gate rejects | A strategy that frequently tries to land `sorry` is wasting compute |

## 2. The eval corpus

A real evaluation needs three corpora:

### 2.1 Tutorial-derived (smallest, sanity check)

Take EXAMPLE-002 (`Refineforge.Counter`) and HELYX's two claims
(`HELYX.AuditChain`, `HELYX.Capability`). For each theorem,
generate `k` broken variants via a mutation taxonomy (see §3).
Smallest possible corpus; useful for "does the strategy even run?"

| Source claim | Theorems | Mutations per theorem | Corpus size |
|---|---:|---:|---:|
| EXAMPLE-002 `Refineforge.Counter` | 2 | 5 | 10 |
| HELYX `AuditChain` | 3 | 5 | 15 |
| HELYX `Capability` | 3 | 5 | 15 |
| **Total** | 8 | 5 | **40** |

40 broken proofs is too small for headline numbers but big enough
to catch a strategy that consistently produces garbage. This is
the **smoke-test corpus**.

### 2.2 Mathlib-derived (mid-size, useful)

Scrape Mathlib's `Mathlib/` tree for theorems whose proofs are
**term-mode short and tactic-mode short** (say, < 5 lines). Apply
the mutation taxonomy. Filter for theorems that the strategy
hasn't seen during training (this matters once Section 2 fine-tunes
a model — see §6 on training/eval separation).

Target corpus size: **5,000-10,000 broken proofs**. Big enough
that repair-rate numbers are statistically meaningful (95 % CI
bounds around ±1 %).

### 2.3 In-the-wild claim repository (gold standard)

Once refineforge has been adopted by enough projects, the gold
standard is *real broken proofs from real refactors* — what
happens when a Mathlib upgrade breaks downstream theorems, or
when a refactor changes a structure field name. These are the
distribution shifts that synthetic mutations don't capture.

This corpus accumulates organically; we don't build it.

## 3. Mutation taxonomy

The training/eval corpora are produced by mutating a working proof
into a broken one in a *known* way. The mutation taxonomy:

| Mutation | Example | Frequency in real bug reports |
|---|---|---|
| **Drop a hypothesis** | `theorem t (h : P) : Q` → `theorem t : Q` | High |
| **Swap a lemma name** | `simp [foo]` → `simp [bar]` | High |
| **Weaken an inductive case** | Remove a `cases` arm | Medium |
| **Introduce a wrong tactic** | `rfl` → `trivial` where they're not equivalent | Medium |
| **Rename a structure field** | `c.value` → `c.val` | High (matches "Mathlib renamed a field" reality) |
| **Change a numeric literal** | `n + 1` → `n + 2` | Low (only matches arithmetic-specific theorems) |
| **Re-order tactic steps** | `intro x; rfl` → `rfl; intro x` | Medium |
| **Delete a `have` binding** | `have h : P := ...; exact h` → `exact h` | Medium |

Each mutation produces a labelled pair `(broken, fixed)`. The
strategy sees only `broken` + the diagnostic; ground truth is
`fixed`. Repair rate is "did the strategy produce a patch that
compiles?" — NOT "did the strategy produce the same patch as
ground truth." (Different patches can both be valid fixes.)

False-fix rate (§1) is the metric for semantic correctness — a
patch that compiles AND matches the ground truth semantically.
Detecting "semantic match" is hard; the v1 heuristic is "does
`patched == ground_truth`?" which under-counts valid alternative
fixes. A v2 heuristic could test the patched theorem against a
fuzz suite that exercises the spec.

## 4. The harness

The proposed binary: `refine-eval` (separate crate to avoid
bloating the main `refine` binary).

```bash
refine-eval \
    --corpus eval/corpus-tutorial.jsonl \
    --strategy anthropic --strategy-config eval/anthropic-config.json \
    --max-iterations 5 \
    --output eval/runs/2026-05-18-anthropic.json
```

For a local fine-tuned checkpoint, pass the runtime directory:

```bash
refine-eval \
    --corpus eval/corpus-tutorial.jsonl \
    --strategy local-finetune \
    --weights-path training/runs/<experiment-id>/checkpoints/<checkpoint> \
    --max-iterations 5 \
    --output eval/runs/2026-05-18-local-finetune.json
```

Output is a JSON file with per-attempt:
- claim_id (or synthetic-id)
- mutation applied
- iterations consumed
- final outcome (`Fixed` / `NoProposal` / `UnrecoverableError` / `MaxIterationsReached`)
- per-iteration latency
- proposed patches (for debugging / audit)
- ground-truth comparison (for false-fix calculation)

Plus a summary at the top with the headline numbers.

The harness is part of the `RepairStrategy` ecosystem, not part
of the strategies themselves. It treats `S` as a black box and
measures end-to-end behaviour.

## 5. Baseline numbers (placeholder)

Once the harness lands, this section will be populated. Expected
shape:

| Strategy | Corpus | Repair rate | Median iters | Cost / attempt |
|---|---|---:|---:|---:|
| `mock` | tutorial-40 | 0.0 % | n/a | $0 |
| `anthropic` (claude-opus-4-7) | tutorial-40 | TBD | TBD | TBD |
| `anthropic` (claude-opus-4-7) | mathlib-5000 | TBD | TBD | TBD |
| `local-llm` (Qwen-Coder-32B) | mathlib-5000 | TBD | TBD | TBD |
| `local-finetune` (refineforge-prover-v1) | mathlib-5000 | TBD | TBD | TBD |

Updating these numbers is part of Section 2's deliverable. The
table itself must include the **commit hash of the corpus**, the
**commit hash of the strategy code**, and the **model identifier**,
so a number from 2026-05-18 is comparable (or not) to a number
from 2026-08-01.

## 6. Training/eval separation (critical for honesty)

When Section 2 fine-tunes a model on mutated Mathlib proofs, the
following invariants must hold:

1. **The eval corpus is split off BEFORE training.** Specifically:
   the held-out 10 % is chosen at corpus-creation time, hashed, and
   committed. Training scripts MUST NOT have access to held-out
   theorem names.
2. **Tutorial corpus claims (EXAMPLE-002, HELYX) are NEVER in
   training data.** Otherwise the smoke-test corpus is leaked.
3. **A model trained on Mathlib snapshot `M_train` is evaluated
   against held-out theorems from `M_train` only.** Cross-snapshot
   eval (model trained on `M_2026-01`, tested on theorems added in
   `M_2026-06`) is a separate experiment for measuring distribution
   shift.

The fine-tuned-strategy section of `docs/repair-evaluation.md`
must include the training-data hash, the held-out-set hash, and a
statement that the two are disjoint. Without those, a "97 % repair
rate" claim is unverifiable.

## 7. Statistical reporting

Headline numbers reported with bootstrap confidence intervals:

```
anthropic (claude-opus-4-7) on mathlib-5000:
  repair rate: 64.2 % (95 % CI: 62.9 % - 65.5 %)
  median iters: 2 (95 % CI: 2 - 2)
  median latency: 4.1 s (95 % CI: 3.9 s - 4.3 s)
```

A point estimate with no CI is "marketing-format." The harness
must emit CIs automatically.

## 8. What this doc does NOT promise

- A specific repair rate target. We don't know what's achievable
  with current LLMs on Lean 4 until the harness exists.
- A timeline for the harness. Section 2 phase 1 says "after
  AnthropicStrategy lands"; that's the binding constraint.
- A guarantee that fine-tuning beats `AnthropicStrategy`. It might
  not. The honest report says so if so.
- Specific cost/budget numbers. Compute cost depends on hardware
  choices, model size, and Anthropic's pricing — all of which
  drift faster than this doc.

## 9. Sequencing

| Step | Owner | When |
|---|---|---|
| `AnthropicStrategy` against existing trait | ML | Section 2 phase 1, item 1 |
| **Tutorial-40 harness** | **ML** | **Section 2 phase 1, item 2** |
| Mathlib mutation pipeline → mathlib-5000 corpus | ML | Section 2 phase 1, item 3 |
| First baseline numbers in this doc | ML | After Section 2 phase 1 |
| Local-LLM strategy (Ollama/llama.cpp) | ML | Section 2 phase 2 |
| `local-finetune` strategy + held-out eval | ML | Section 2 phase 2 |
| Distribution-shift evals (cross-snapshot) | ML | Section 2 phase 3 |
