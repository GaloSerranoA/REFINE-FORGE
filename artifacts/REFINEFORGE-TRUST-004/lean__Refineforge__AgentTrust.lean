/-
REFINEFORGE-TRUST-001 — Agent trust-ceiling enforcement never exceeds the ceiling.

DOGFOOD model+refined claim. A Lean model of Refine-Forge's OWN agent trust
lattice and the ceiling-enforcement step that makes the four specialist agents
un-spoofable. The refinement doc (docs/refinement/REFINEFORGE-TRUST-001.md)
bridges this model to the real Rust in
`crates/refineforge-cli/src/agent/common.rs` (`TrustLevel`, `trust_rank`,
`enforce_trust_ceiling`).

HONEST SCOPE. Lean proves a property of the *model*; the refinement argument is
the trust-critical bridge to the Rust. This claim is NOT yet human-reviewed
(`review.human_operator` is null), so the agent's honest trust ceiling for it is
`model-linked`, not `human-reviewed`.

Policy: no `sorry`, no `admit`, no axioms beyond Lean core.
-/

namespace Refineforge.AgentTrust

/-- The agent trust lattice, lowest to highest assurance. Mirrors the Rust
    `TrustLevel` enum (same variants, same order). -/
inductive TrustLevel where
  | blocked
  | measuredOnly
  | modelOnly
  | modelLinked
  | releaseReadyLocal
  | releaseReadyCi
  | humanReviewed
  deriving DecidableEq, Repr

/-- Numeric assurance rank. Mirrors Rust `trust_rank` (0..=6, same mapping). -/
def rank : TrustLevel → Nat
  | .blocked => 0
  | .measuredOnly => 1
  | .modelOnly => 2
  | .modelLinked => 3
  | .releaseReadyLocal => 4
  | .releaseReadyCi => 5
  | .humanReviewed => 6

/-- Ceiling enforcement: keep the reported level when it is at or below the
    ceiling, otherwise cap it to the ceiling. Mirrors the *resulting trust
    level* of Rust `enforce_trust_ceiling` (which additionally returns whether
    it capped and records a warning; those side effects are out of model scope —
    see the refinement doc). -/
def enforce (reported ceiling : TrustLevel) : TrustLevel :=
  if rank reported ≤ rank ceiling then reported else ceiling

/-- T1 (safety). The enforced level never ranks above the ceiling: no reported
    level can survive above its configured ceiling. This is the invariant that
    makes agent trust un-spoofable. It would be FALSE for an `enforce` that
    returned `reported` unconditionally — so the statement is not vacuous. -/
theorem enforce_never_exceeds_ceiling (reported ceiling : TrustLevel) :
    rank (enforce reported ceiling) ≤ rank ceiling := by
  unfold enforce
  by_cases h : rank reported ≤ rank ceiling
  · rw [if_pos h]
    exact h
  · rw [if_neg h]
    omega

/-- T2 (faithfulness). When the reported level is already within the ceiling,
    enforcement returns it unchanged. -/
theorem enforce_keeps_when_within_ceiling
    (reported ceiling : TrustLevel) (h : rank reported ≤ rank ceiling) :
    enforce reported ceiling = reported := by
  unfold enforce
  rw [if_pos h]

/-- T3 (idempotence). Enforcing an already-enforced level against the same
    ceiling changes nothing — a direct consequence of T1 and T2. -/
theorem enforce_idempotent (reported ceiling : TrustLevel) :
    enforce (enforce reported ceiling) ceiling = enforce reported ceiling :=
  enforce_keeps_when_within_ceiling
    (enforce reported ceiling) ceiling
    (enforce_never_exceeds_ceiling reported ceiling)

end Refineforge.AgentTrust
