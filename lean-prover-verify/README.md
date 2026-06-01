# Lean verifier project — matched to Goedel-Prover-V2

The trust gate for `refineforge_lean_prover`: the `CommandVerifier` writes each
candidate proof to `ProverCandidate.lean` here and runs `lake env lean
ProverCandidate.lean`; exit 0 ⇒ the proof is **accepted**. Nothing else can mark a
proof trusted.

## Versions (do not bump casually)

| | Pinned to | Why |
|---|---|---|
| Lean | **`leanprover/lean4:v4.9.0-rc1`** (`./lean-toolchain`) | Goedel-Prover-V2's toolchain |
| Mathlib | **`2f65ba7f1a9144b20c8e7358513548e317d26de1`** (2024-08-07, `./lakefile.lean`) | the exact commit Goedel-Prover-V2's `mathlib4` submodule (`xinhjBrant/mathlib4`) points at; it is an upstream `leanprover-community` commit, so the prebuilt cache applies |

These are **deliberately old** — the rest of Refine-Forge's `lean/` trust-base is on
v4.29.1. They are pinned because a proof Goedel-Prover-V2 emits was learned against
*this* Mathlib API; a newer Mathlib renames/retypes lemmas and would reject correct
proofs. A version mismatch only ever causes **false negatives** (correct proofs
rejected), never false positives — so it is trust-safe, but it costs pass rate.
Matching maximizes how many real proofs the gate accepts.

## Build (operator step — ~GB download)

The toolchain is already validated (`lean Smoke.lean` → exit 0, no Mathlib). To make
the real (Mathlib) gate ready:

```bash
cd lean-prover-verify
lake exe cache get     # ~2-4 GB of prebuilt Mathlib oleans (NOT a multi-hour compile,
                       # because the pinned commit is upstream + cache-backed)
lake build             # builds ProverVerify.lean (`import Mathlib`) → confirms the env
```

`lake exe cache get` is the heavy step (bandwidth + disk). On first `lake` run, elan
auto-installs `v4.9.0-rc1` and lake clones Mathlib into `.lake/packages/` (gitignored).
After this, `lake env lean ProverCandidate.lean` resolves `import Mathlib` in seconds.

### Windows note
Mathlib's olean cache is built by leanprover-community CI; if `cache get` reports
misses on Windows, the missed files compile locally (slower). If that is painful, the
trust-safe fallback is a *recent* cache-backed Mathlib (edit `lakefile.lean` to a
current commit + matching `lean-toolchain`): the gate stays sound, you just accept
fewer of the model's proofs until the versions line up.

## Wiring

`training/configs/refineforge-lean-prover-live.yaml` sets `lean_dir:
lean-prover-verify` and `lean_command: lake env lean`. Problem `template`s should
emit a complete file beginning `import Mathlib` so the candidate type-checks here.

## What's validated vs. pending
- ✅ Toolchain `v4.9.0-rc1` installs; `Smoke.lean` (Mathlib-free) checks → the pin
  is real and the write-file → check → exit-code plumbing works.
- ⏳ `lake exe cache get` + `lake build` (Mathlib) — the operator's ~GB step; until
  then, real (Mathlib-importing) proofs can't be checked here.
