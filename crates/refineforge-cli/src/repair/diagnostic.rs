//! Diagnostic types live in `refineforge-repair-api` so the
//! strategies crate can use them without depending on this crate.
//! Re-exported here for backward-compat with the rest of `refineforge-cli`.

pub use refineforge_repair_api::{Diagnostic, Position, Range, Severity};
