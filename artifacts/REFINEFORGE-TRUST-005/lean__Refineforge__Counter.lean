/-
EXAMPLE-002: Monotone counter (refined tutorial).

This file is the second of two tutorial claims shipped with
refineforge. Where EXAMPLE-001 demonstrates only the Lean → bundle
path, EXAMPLE-002 demonstrates the **full refinement pattern**:

  Lean model  ←→  Rust crate  ←→  refinement-argument doc

Read in this order:
  1. lean/Refineforge/Counter.lean       (this file)
  2. crates/example-counter/src/counter.rs
  3. crates/example-counter/tests/counter.rs   (one test per theorem)
  4. docs/refinement/EXAMPLE-002.md      (the trust-critical bridge)

Then run:
  refine lean   check EXAMPLE-002        → Verified
  refine scan   check EXAMPLE-002        → Verified
  refine bundle export EXAMPLE-002        → bundles + verifies

Doctrine
--------
LLM may propose. Lean must verify. Human operator must approve.

Policy
------
No `sorry`. No `admit`. No `axiom` declarations beyond Lean core.
-/

namespace Refineforge.Counter

/-- A monotonically-increasing counter. Held abstract here as `Nat`;
    the Rust refinement uses `u64` (see refinement doc §3 for the
    overflow idealisation). -/
structure Counter where
  value : Nat
  deriving Repr, DecidableEq

/-- Increment. In Lean this is total. -/
def incr (c : Counter) : Counter :=
  { value := c.value + 1 }

/-! ## EXAMPLE-002 theorems -/

/-- T1. **Main theorem.** `incr` is monotone: the new value is at
    least the old value. This is the weakest useful property; the
    Rust implementation refines this (not strict increase) because
    `u64` saturates at `u64::MAX`. -/
theorem incr_monotone (c : Counter) : (incr c).value ≥ c.value := by
  simp [incr]

/-- T2. `incr` is **strictly** increasing in Lean (where `Nat` has
    no upper bound). The Rust refinement does NOT preserve this at
    the `u64::MAX` boundary; the refinement doc identifies that
    idealisation explicitly. -/
theorem incr_strictly_increases (c : Counter) : (incr c).value > c.value := by
  simp [incr]

end Refineforge.Counter
