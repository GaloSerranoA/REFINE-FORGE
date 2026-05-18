//! Refinement tests: each maps to a theorem in
//! `lean/Refineforge/Counter.lean`.

use example_counter::{checked_incr, incr, Counter};

// ---- T1: refines Lean theorem `incr_monotone` ----
#[test]
fn incr_is_monotone() {
    let c = Counter::from_value(42);
    assert!(incr(&c).value() >= c.value());
}

#[test]
fn incr_is_monotone_at_zero() {
    let c = Counter::new();
    assert!(incr(&c).value() >= c.value());
}

#[test]
fn incr_is_monotone_at_u64_max() {
    // At u64::MAX the strict-increase theorem fails (see test below);
    // monotonicity (≥) still holds because saturating_add returns
    // u64::MAX, which equals (not exceeds) the input.
    let c = Counter::from_value(u64::MAX);
    assert!(incr(&c).value() >= c.value());
}

// ---- T2: refines Lean theorem `incr_strictly_increases` ----
#[test]
fn incr_strictly_increases_below_boundary() {
    let c = Counter::from_value(100);
    assert!(incr(&c).value() > c.value());
}

/// **Idealisation test.** The Lean theorem `incr_strictly_increases`
/// proves `(incr c).value > c.value` for all `c : Counter` (where
/// `value : Nat`). The Rust impl uses `saturating_add` on `u64`, so
/// the property FAILS at `c.value == u64::MAX`. This test documents
/// that gap so a future refactor (e.g. switching to `checked_add`)
/// can re-evaluate it.
#[test]
fn incr_does_not_strictly_increase_at_u64_max() {
    let c = Counter::from_value(u64::MAX);
    assert_eq!(
        incr(&c).value(),
        c.value(),
        "saturating_add at u64::MAX returns u64::MAX — strict increase fails. See refinement doc §3."
    );
}

#[test]
fn checked_incr_returns_none_at_u64_max() {
    let c = Counter::from_value(u64::MAX);
    assert!(checked_incr(&c).is_none());
}

#[test]
fn checked_incr_returns_some_below_boundary() {
    let c = Counter::from_value(0);
    assert_eq!(checked_incr(&c).unwrap().value(), 1);
}

// ─── Section 1: #[derive(LeanModel)] demo ────────────────────────────
//
// The proc-macro auto-generates a Lean structure declaration string.
// This test pins down the contract: the generated string must match
// the structure shape we hand-wrote in lean/Refineforge/Counter.lean.
// If the macro drifts (or the hand-written Lean drifts), this test
// fails — flagging the mismatch before a refinement-doc reviewer has
// to spot it.

#[test]
fn lean_model_matches_hand_written_counter_lean() {
    // The hand-written Lean has additional `deriving Repr, DecidableEq`
    // clauses which #[derive(LeanModel)] does NOT generate (deliberate —
    // those are an operator-level choice). The structural part must match.
    let expected_struct_shape = "structure Counter where\n  value : Nat";
    assert_eq!(Counter::LEAN_MODEL, expected_struct_shape);
    // Sanity: the runtime accessor returns the same const.
    assert_eq!(Counter::lean_model(), expected_struct_shape);
}

#[test]
fn lean_model_is_const_and_static() {
    // The generated const is `&'static str` — usable in const contexts.
    const X: &str = Counter::LEAN_MODEL;
    assert!(X.contains("value : Nat"));
}
