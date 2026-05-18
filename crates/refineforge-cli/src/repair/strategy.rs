//! Strategy types live in `refineforge-repair-api` so the
//! strategies crate can implement against them without depending on
//! this crate. Re-exported here so the driver code in `mod.rs`
//! continues to compile unchanged.

pub use refineforge_repair_api::{MockStrategy, Patch, RepairStrategy};
