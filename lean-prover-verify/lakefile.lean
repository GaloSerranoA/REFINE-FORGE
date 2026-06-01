import Lake
open Lake DSL

-- Lean verifier project matched to Goedel-Prover-V2 (Lean v4.9.0-rc1, see
-- ./lean-toolchain). Mathlib is pinned to the EXACT commit Goedel-Prover-V2's
-- mathlib4 submodule points at (2024-08-07); it is an upstream
-- leanprover-community commit, so `lake exe cache get` fetches prebuilt oleans
-- instead of compiling for hours. Matching this version is what makes a proof the
-- model emits actually check (a newer Mathlib would change lemma names/signatures
-- and reject correct proofs — a false negative, never a false positive, so a
-- mismatch is trust-safe but costs pass rate).
package proverVerify where

require mathlib from git
  "https://github.com/leanprover-community/mathlib4.git" @
  "2f65ba7f1a9144b20c8e7358513548e317d26de1"

@[default_target]
lean_lib ProverVerify where
