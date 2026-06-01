# SGS (Scaling Self-Play with Self-Guidance) for the Lean agent — design

**Status: DESIGN ONLY — not implemented, not runnable.** This records how
*"Scaling Self-Play with Self-Guidance"* (Bailey et al., Stanford, 2026) would map
onto Refine-Forge's Lean agent, what already exists, and — honestly — the
prerequisites we **do not have**, so we do not pretend to ship a working system.
M15 (self-distillation) and M16 (AVO tiled kernels) were implemented + verified;
this one is a plan, because it requires a capable Lean-prover model and an RL
training stack that are out of scope for this codebase + a 6 GB RTX 3060.

## What SGS is (faithful recap)

One base model is instantiated in three roles:
- **Conjecturer** — *conditioned on an unsolved target theorem `x`*, proposes a
  related-but-simpler lemma `x̃` (a stepping-stone).
- **Solver** — attempts `k` proofs of both real and synthetic problems; each is
  checked by the **Lean compiler** → binary reward `{0,1}`.
- **Guide** — an LLM judge that scores each proposed `x̃` against `x` on a rubric
  (relevance 0–5, redundant-premises 0/1, conclusion-complexity 0–4), auto-zeroing
  messy/disjunction-bloated statements. This is the anti-reward-hacking signal that
  stops the Conjecturer from collapsing to artificially complex, useless problems.

Training: the Solver is RL-updated on its correctness reward (the paper uses
REINFORCE½ — reward-weighted log-likelihood on problems with solve-rate ≤ 0.5, to
preserve Solver entropy); the Conjecturer is RL-updated on `R_solve · R_guide`. The
payoff: a 7B prover under SGS exceeded the *671B* DeepSeek-Prover-V2's pass@4 at
~6.3M generations, where plain RL and naive self-play had plateaued.

## Mapping onto Refine-Forge — what already exists vs. what's missing

| SGS ingredient | Refine-Forge today | Status |
|---|---|---|
| **Automatic verifier** (the binary reward) | The Lean agent already shells out to `lake` / `lean --server` (`refine agent lean --mode check`, `lake build verified` in the autonomous executor) | ✅ **exists** |
| **Seed problem corpus** | `training/data/mathlib-proof-repair-v1` (broken→fixed Lean, expected patches) + Mathlib lemmas | ✅ **exists** |
| **Conjecturer / Solver / Guide model** | A capable Lean-prover LLM (the paper used DeepSeek-Prover-V2-7B for all three roles) | ❌ **missing** — Refine-Forge's native GPT is a *smoke-grade* model, not a prover |
| **RL training stack** (rollout gen → reward → policy-gradient update) | The native-GPT path is **SFT-only** (cross-entropy / self-distillation); there is no PPO/GRPO/REINFORCE rollout+advantage machinery | ❌ **missing** |
| **Compute** | One RTX 3060 (6 GB). The paper ran multi-million-to-billion generations on 64+ GPUs + 128 CPU verifiers | ❌ **infeasible at meaningful scale** |

So the *checker and seed data are in place*; the blockers are a real prover model,
an RL stack, and compute. None of those is a small lift, and two of them
(prover model, cluster compute) are not obtainable inside this repo.

## The loop (pseudocode — not compiled)

```
roles: Solver π, Conjecturer g, Guide ρ   # from one prover base, untied weights
for round in 1..N:
    B = sample(target_corpus)
    synth = []
    for x in B where unsolved(x):
        x̃ = g(· | x)                       # propose a simpler related lemma
        synth.push(x̃)
    # rollouts + Lean verification (the reward)
    for p in B ∪ synth:
        attempts = π.sample(p, k=8)
        v[p]     = any(lake_check(a) for a in attempts)   # {0,1}, EXISTING verifier
    # solver update (entropy-preserving; e.g. REINFORCE½ on solve-rate ≤ 0.5)
    rl_update(π, {(p, a, v[p])})
    # conjecturer update with the self-guidance reward
    for x̃ in synth:
        R_solve = difficulty_band(solve_rate(x̃))           # 0 if impossible/too-easy
        R_guide = ρ.score(x̃ | x)                           # relevance/elegance rubric
        reward[x̃] = R_solve * R_guide
    rl_update(g, {(x, x̃, reward)})
```

`lake_check` is the **only** box already built (the Lean agent's checker); `rl_update`,
`π/g/ρ.sample`, and `ρ.score` all require the missing prover + RL stack.

## A feasible, honest first step (no RL, no new model)

If pursued, the *bounded* slice that fits Refine-Forge's current capabilities is
the **inference-only, verifier-gated proof-search / curriculum** — i.e. AVO-style
(M16) for proofs rather than RL self-play:
- Use an **agentic** Conjecturer + Guide (prompted LLM, like the existing agents)
  to propose simpler related lemmas and score their relevance/elegance.
- Gate every candidate with the **existing Lean checker** (accept iff `lake`
  verifies), keeping a git-commit lineage of solved lemmas (the AVO discipline).
- This yields a *self-guided proof-repair search* with no weight updates — useful,
  and entirely within reach — but it is **not SGS**: it does not train the model,
  so it cannot reproduce the paper's headline (a small model overtaking a large one
  *via learning*). That headline needs the RL stack + prover we do not have.

## Honest bottom line

SGS is the single best-matched paper for the Lean agent (it was demonstrated *on
Lean 4*, and our verifier + seed data already exist), but it is also the one we
**cannot implement** here: it is fundamentally an RL-at-scale recipe, and Refine-
Forge has neither a prover-grade model nor RL training nor cluster compute. The
right next moves, in increasing cost: (1) the inference-only self-guided search
above (feasible now); (2) acquire/fine-tune an open Lean-prover model and add a
minimal REINFORCE loop on the existing verifier (medium); (3) full SGS at scale
(out of scope). This document is the plan; no SGS code or results are claimed.
