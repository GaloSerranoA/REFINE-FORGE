# Lean verifier project — matched to Goedel-Prover-V2

The trust gate for `refineforge_lean_prover`: the `CommandVerifier` writes each
candidate proof to `ProverCandidate.lean` here and runs `lake env lean
ProverCandidate.lean`; exit 0 ⇒ the proof is **accepted**. Nothing else can mark a
proof trusted.

## Versions (do not bump casually)

| | Pinned to | Why |
|---|---|---|
| Lean | **`leanprover/lean4:v4.9.0-rc1`** (`./lean-toolchain`) | Goedel-Prover-V2's toolchain |
| Mathlib | **`xinhjBrant/mathlib4` @ `2f65ba7f`** (`./lakefile.lean`) | the EXACT commit Goedel-Prover-V2's `mathlib4` submodule points at — maximum fidelity |

These are **deliberately old** — the rest of Refine-Forge's `lean/` trust-base is on
v4.29.1. They are pinned because a proof Goedel-Prover-V2 emits was learned against
*this* Mathlib API; a different Mathlib renames/retypes lemmas and rejects correct
proofs. A version mismatch only ever causes **false negatives** (correct proofs
rejected), never false positives — so a mismatch is trust-safe, but it costs pass
rate. The exact match maximizes how many real proofs the gate accepts.

**Why a fork that compiles from source (operator-chosen).** Goedel's pinned commit
lives only in the third-party fork `xinhjBrant/mathlib4`, not upstream. Two upstream
alternatives were ruled out: (1) Goedel's commit *isn't* on upstream — GitHub's
fork-object sharing made API lookups falsely succeed, but `git checkout` fails
"unable to read tree"; (2) a same-era upstream rc1 commit is **bit-rotted** — its
transitive dep pins (e.g. `Qq`/quote4 @ `44f57616`) were deleted from the dependency
repos, so it no longer resolves. So the exact match means the fork, which (a)
requires trusting the fork author's build-time code, and (b) has no prebuilt cache →
Mathlib **compiles from source** (~1–3 hrs, one-time; the fork's own dep pins are
self-consistent, so the graph resolves). Chosen explicitly over a recent-but-
big-delta upstream Mathlib.

## Build (operator step — one-time source compile, ~1–3 hrs)

The toolchain is already validated (`lean Smoke.lean` → exit 0, no Mathlib). To make
the real (Mathlib) gate ready:

```bash
cd lean-prover-verify
lake update            # clones the fork's Mathlib + its (self-consistent) deps
lake exe cache get     # EXPECTED to fail: the fork's `cache` tool references a Lake
                       # symbol absent in rc1 (undefined symbol initialize_Lake_Build_Trace),
                       # and the fork commit isn't cached anyway. Skip it — go to build.
lake build             # COMPILES Mathlib from source — ~1-3 hrs, one-time. Incremental:
                       # if interrupted, re-run `lake build` and it resumes. (The cache
                       # tool's link failure does NOT affect the library compile.)
```

The compile is the heavy step (CPU + disk; the oleans land in `.lake/`, gitignored).
On first `lake` run, elan auto-installs `v4.9.0-rc1`. Once built,
`lake env lean ProverCandidate.lean` resolves `import Mathlib` in seconds, and every
subsequent verification reuses the cached oleans.

### If the compile is too costly
The trust-safe fallback is a *recent* cache-backed upstream Mathlib (edit
`lakefile.lean` + `lean-toolchain` to a current commit): it builds in minutes from
cache and the gate stays sound — you just accept fewer of the model's proofs until
the versions line up (a bigger delta than this exact match).

## Wiring

`training/configs/refineforge-lean-prover-live.yaml` sets `lean_dir:
lean-prover-verify` and `lean_command: lake env lean`. Problem `template`s should
emit a complete file beginning `import Mathlib` so the candidate type-checks here.

## What's validated
- ✅ Toolchain `v4.9.0-rc1` installs; `Smoke.lean` (Mathlib-free) checks → the pin
  is real and the write-file → check → exit-code plumbing works.
- ✅ The fork's dependency graph resolves (`lake update` checks out `2f65ba7f` + its
  self-consistent deps without the "unable to read tree" failures that upstream hit).
- ✅ **Full Mathlib compiled from source** (`lake build`, 4653 targets, exit 0) and
  the real gate works: `lake env lean` on an `import Mathlib` proof checks (exit 0).
  The verifier is live. (Build artifacts live in `.lake/`, gitignored; rebuild on a
  fresh checkout per the Build section.)
