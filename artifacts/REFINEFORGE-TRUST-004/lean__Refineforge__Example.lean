/-
EXAMPLE-001: Hello-world claim for refineforge.

This file exists to prove the refineforge pipeline works end-to-end:
no-sorry policy gate → `lake build` → bundle export/verify. Once
you've confirmed it runs green, replace this with your own claim
(`refine new --template <name> --module <Mod.Path> <CLAIM_ID>`).

Doctrine
--------
LLM may propose. Lean must verify. Human operator must approve.

Policy
------
No `sorry`. No `admit`. No `axiom` declarations beyond Lean core.

Lean toolchain: pinned in `lean/lean-toolchain` (currently v4.29.1).
-/

namespace Refineforge.Example

/-- Hello-world theorem: addition is commutative on Nat.

    Lean's standard library already proves this (`Nat.add_comm`); this
    wrapper exists so refineforge has a non-trivial theorem to drive
    through the policy gate, build, and bundle pipeline. -/
theorem add_comm_demo (a b : Nat) : a + b = b + a := Nat.add_comm a b

end Refineforge.Example
