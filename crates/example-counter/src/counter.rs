//! Monotone counter. Refines `Refineforge.Counter`.
//!
//! Key idealisation: Lean uses `Nat` (unbounded); Rust uses `u64`.
//! `incr` uses `saturating_add` so the function is total and the
//! Lean monotonicity theorem (`≥`) is preserved. The Lean strict-
//! increase theorem (`>`) is NOT preserved at `u64::MAX`; see
//! refinement doc §3 for the full argument.

use refineforge_derive::LeanModel;

/// A monotonically-increasing counter. Private field so external code
/// cannot bypass `Counter::new` / `Counter::from_value`.
/// Refines Lean `structure Counter where value : Nat`.
///
/// `#[derive(LeanModel)]` generates a `LEAN_MODEL` const containing
/// the Lean structure declaration — see the test
/// `lean_model_matches_hand_written_counter_lean` for verification
/// that the generated string matches what we wrote by hand in
/// `lean/Refineforge/Counter.lean`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, LeanModel)]
pub struct Counter {
    value: u64,
}

impl Counter {
    pub fn new() -> Self { Self { value: 0 } }
    pub fn from_value(value: u64) -> Self { Self { value } }
    pub fn value(&self) -> u64 { self.value }
}

/// Increment by 1. Refines Lean `def incr (c : Counter) : Counter :=
/// { value := c.value + 1 }`.
///
/// Uses `saturating_add` so the function is total even at
/// `u64::MAX`. This preserves the Lean monotonicity theorem
/// (`(incr c).value ≥ c.value`) but **breaks the Lean strict-increase
/// theorem** at exactly `c.value == u64::MAX`. The refinement
/// argument explains why this is the correct trade-off for the
/// stated claim. If your application needs strict monotonicity
/// even at the boundary, use `checked_incr` (returns `Option`).
pub fn incr(c: &Counter) -> Counter {
    Counter { value: c.value.saturating_add(1) }
}

/// Optional checked variant: returns `None` at `u64::MAX`.
/// Included to show how a stricter precondition can be encoded
/// when the Lean theorem requires it.
pub fn checked_incr(c: &Counter) -> Option<Counter> {
    c.value.checked_add(1).map(|v| Counter { value: v })
}
