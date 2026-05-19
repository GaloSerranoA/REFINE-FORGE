//! refineforge-cli library: exposes the framework's internals so
//! external strategy crates (e.g. `refineforge-strategies`) can
//! implement the `RepairStrategy` trait against the same types the
//! CLI binary uses.
//!
//! The CLI itself lives in `src/main.rs`.

pub mod autonomous;
pub mod bundle;
pub mod claim;
pub mod repair;
pub mod report;
pub mod runner;
pub mod scaffold;
pub mod scan;
pub mod sorry_gate;
