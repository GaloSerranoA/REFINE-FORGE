//! refineforge tutorial: monotone counter.
//!
//! Refines `Refineforge.Counter` from `lean/Refineforge/Counter.lean`.
//! Refinement argument: `docs/refinement/EXAMPLE-002.md`.

pub mod counter;

pub use counter::{checked_incr, incr, Counter};
