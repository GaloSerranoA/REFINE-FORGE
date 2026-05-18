/-
EVAL CORPUS ENTRY — mutation: wrong_tactic

Original: `simp [incr]` closes both theorems.
Broken:   uses `rfl` instead — fails because `(c.value + 1) > c.value`
          is not definitional equality on Nat.

Expected outcome: change `rfl` back to `simp [incr]` or `decide` or
                  a substantive omega-style proof.
-/

namespace Refineforge.Counter

structure Counter where
  value : Nat
  deriving Repr, DecidableEq

def incr (c : Counter) : Counter :=
  { value := c.value + 1 }

theorem incr_monotone (c : Counter) : (incr c).value ≥ c.value := by
  rfl

theorem incr_strictly_increases (c : Counter) : (incr c).value > c.value := by
  rfl

end Refineforge.Counter
