//! Library crate alongside the `refine-train` binary. Exposes the
//! orchestration modules so external tools / tests can drive them
//! programmatically.
//!
//! The CLI in `main.rs` is a thin dispatch over these modules.

pub mod checkpoint;
pub mod dataset;
pub mod experiment;
pub mod failure;
pub mod progress;
pub mod promotion;
pub mod report;
pub mod runner;
pub mod sweep;
