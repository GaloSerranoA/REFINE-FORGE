/-
EXAMPLE-003: Capability revocation.

Production-shaped dogfood claim for the Lean verification track. The
model proves that revocation is monotone: once a capability is revoked,
it authorizes no right.

Policy: no sorry, no admit, no axioms.
-/

namespace Refineforge.CapabilityRevocation

/-- The finite right set used by the Rust refinement. -/
inductive Right
  | read
  | write
  | admin
  deriving Repr, DecidableEq

/-- A capability is a finite set of rights plus a revocation bit. -/
structure Capability where
  read : Bool
  write : Bool
  admin : Bool
  revoked : Bool
  deriving Repr, DecidableEq

/-- Does this capability hold the requested right, ignoring revocation? -/
def holds (c : Capability) (r : Right) : Bool :=
  match r with
  | Right.read => c.read
  | Right.write => c.write
  | Right.admin => c.admin

/-- Authorization requires both a held right and an unrevoked capability. -/
def authorizes (c : Capability) (r : Right) : Prop :=
  c.revoked = false ∧ holds c r = true

/-- Revoke the capability. There is no inverse operation in the model. -/
def revoke (c : Capability) : Capability :=
  { c with revoked := true }

/-! ## EXAMPLE-003 theorems -/

/-- T1. Revoked capabilities authorize no right. -/
theorem revoked_authorizes_nothing (c : Capability) (r : Right) :
    ¬ authorizes (revoke c) r := by
  intro h
  exact Bool.noConfusion h.left

/-- T2. An unrevoked capability authorizes any right it holds. -/
theorem fresh_capability_authorizes_held_right
    (read write admin : Bool) (r : Right)
    (h : holds ({ read := read, write := write, admin := admin, revoked := false } : Capability) r = true) :
    authorizes ({ read := read, write := write, admin := admin, revoked := false } : Capability) r := by
  exact ⟨rfl, h⟩

/-- T3. Revocation is idempotent. -/
theorem revoke_is_idempotent (c : Capability) :
    revoke (revoke c) = revoke c := by
  cases c
  rfl

end Refineforge.CapabilityRevocation
