/-
REFINEFORGE-TRUST-004 — The run-all aggregate trust never exceeds any member's trust.

DOGFOOD model+refined claim about Refine-Forge's OWN run-all trust aggregation.
The Lean model mirrors `lowest_trust` / `lowest_trust_ceiling` in
crates/refineforge-cli/src/agent/mod.rs: the `run_all` summary takes the LOWEST
trust across the four agents, so the aggregate can never claim more trust than
its weakest member.

HONEST SCOPE. Lean proves a property of the *model*; the refinement doc
(docs/refinement/REFINEFORGE-TRUST-004.md) bridges it to the Rust. NOT yet
human-reviewed → agent trust = model-linked.

Policy: no `sorry`, no `admit`, no axioms beyond Lean core.
-/

import Refineforge.AgentTrust

namespace Refineforge.AggregateTrust

open Refineforge.AgentTrust (TrustLevel rank)

/-- The lower-trust of two levels, by rank. -/
def lowerOf (a b : TrustLevel) : TrustLevel :=
  if rank a ≤ rank b then a else b

/-- The lowest trust across a list of member levels. Mirrors the Rust
    `lowest_trust` cascade (the lowest rank present wins); the empty list falls
    through to `humanReviewed`, matching the Rust `else` branch. -/
def lowest : List TrustLevel → TrustLevel
  | [] => TrustLevel.humanReviewed
  | t :: ts => lowerOf t (lowest ts)

theorem lowerOf_le_left (a b : TrustLevel) : rank (lowerOf a b) ≤ rank a := by
  unfold lowerOf; split <;> omega

theorem lowerOf_le_right (a b : TrustLevel) : rank (lowerOf a b) ≤ rank b := by
  unfold lowerOf; split <;> omega

/-- T1 (no over-trust). The aggregate trust never ranks above ANY member: the
    run-all summary cannot claim more trust than its weakest agent. False for an
    aggregator that returned the *highest* member — so it is not vacuous. -/
theorem lowest_le_member :
    ∀ (ts : List TrustLevel) (t : TrustLevel), t ∈ ts → rank (lowest ts) ≤ rank t := by
  intro ts
  induction ts with
  | nil => intro t h; simp at h
  | cons hd tl ih =>
      intro t h
      rw [List.mem_cons] at h
      simp only [lowest]
      cases h with
      | inl heq => rw [heq]; exact lowerOf_le_left hd (lowest tl)
      | inr htl => exact Nat.le_trans (lowerOf_le_right hd (lowest tl)) (ih t htl)

/-- T2 (concrete). A measured-only agent drags the run-all summary down to
    measured-only even alongside human-reviewed and model-linked agents. -/
theorem aggregate_picks_weakest :
    lowest [TrustLevel.humanReviewed, TrustLevel.measuredOnly, TrustLevel.modelLinked]
      = TrustLevel.measuredOnly := by decide

end Refineforge.AggregateTrust
