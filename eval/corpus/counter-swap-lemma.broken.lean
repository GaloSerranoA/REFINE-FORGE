/-
EVAL CORPUS ENTRY — mutation: swap_lemma

Original: `simp [incr]` closes both theorems via the unfolding lemma
          for `incr`.
Broken:   uses `simp [Nat.mul_comm]` — multiplication commutativity
          is unrelated to the goal; tactic fails to make progress
          and leaves an unsolved goal.

Expected outcome with a competent repair strategy: swap back to
`simp [incr]` (single-token edit), or any other tactic that
unfolds `incr` and closes the arithmetic goal.
-/

namespace Refineforge.Counter

structure Counter where
  value : Nat
  deriving Repr, DecidableEq

def incr (c : Counter) : Counter :=
  { value := c.value + 1 }

theorem incr_monotone (c : Counter) : (incr c).value ≥ c.value := by
  simp [Nat.mul_comm]

theorem incr_strictly_increases (c : Counter) : (incr c).value > c.value := by
  simp [Nat.mul_comm]

end Refineforge.Counter
