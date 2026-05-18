/-
EVAL CORPUS GROUND-TRUTH — identical to the as-shipped
`lean/Refineforge/Counter.lean`. Used for false-fix-rate
comparison (currently informational; see docs/repair-evaluation.md
§3 on why exact-match is a weak heuristic).
-/

namespace Refineforge.Counter

structure Counter where
  value : Nat
  deriving Repr, DecidableEq

def incr (c : Counter) : Counter :=
  { value := c.value + 1 }

theorem incr_monotone (c : Counter) : (incr c).value ≥ c.value := by
  simp [incr]

theorem incr_strictly_increases (c : Counter) : (incr c).value > c.value := by
  simp [incr]

end Refineforge.Counter
