import Lake
open Lake DSL

-- Lean verifier project matched to Goedel-Prover-V2 (Lean v4.9.0-rc1, see
-- ./lean-toolchain). Mathlib is pinned to the EXACT commit Goedel-Prover-V2's
-- mathlib4 submodule points at — a third-party fork (xinhjBrant/mathlib4) @
-- 2f65ba7f. This is the maximum-fidelity match (the model's proofs were learned
-- against precisely this Mathlib), explicitly chosen by the operator.
--
-- Trade-offs the operator accepted by choosing this over upstream:
--  * TRUST: building Mathlib runs this fork author's code (tactics/elaborators) at
--    build time — using it means trusting xinhjBrant.
--  * COST: the commit is fork-only (no upstream cache) AND the upstream rc1-era
--    Mathlib is bit-rotted (its dependency commits were deleted from upstream), so
--    this compiles Mathlib FROM SOURCE (~1-3 hrs, one-time). The fork's own dep
--    pins are self-consistent, so the dependency graph resolves.
package proverVerify where

require mathlib from git
  "https://github.com/xinhjBrant/mathlib4.git" @
  "2f65ba7f1a9144b20c8e7358513548e317d26de1"

@[default_target]
lean_lib ProverVerify where
