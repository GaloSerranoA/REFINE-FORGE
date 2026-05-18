/-
EVAL CORPUS ENTRY — mutation: rename_field

Original: `structure Counter where value : Nat`.
Broken:   field renamed to `val`. The `incr` body still uses `c.value`
          and the theorems still call `(incr c).value` — both fail
          because the field no longer exists under that name.

Expected outcome: rename `val` back to `value`, or update every
                  caller. Either is a multi-line edit; tests how the
                  strategy handles non-local cascading errors.
-/

namespace Refineforge.Counter

structure Counter where
  val : Nat
  deriving Repr, DecidableEq

def incr (c : Counter) : Counter :=
  { value := c.value + 1 }

theorem incr_monotone (c : Counter) : (incr c).value ≥ c.value := by
  simp [incr]

theorem incr_strictly_increases (c : Counter) : (incr c).value > c.value := by
  simp [incr]

end Refineforge.Counter
