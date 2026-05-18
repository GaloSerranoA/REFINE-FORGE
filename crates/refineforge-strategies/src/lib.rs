//! Pluggable `RepairStrategy` implementations for `refine repair`.
//!
//! See [`ARCHITECTURE.md`](../../ARCHITECTURE.md) §2: this crate is
//! owned by the ML Training Engineer section. Section 1 owns the
//! trait (in `refineforge_cli::repair::RepairStrategy`); this crate
//! consumes it.
//!
//! # What ships today
//!
//! - [`anthropic::AnthropicStrategy`]: real trait wiring, real
//!   prompt construction, real response parsing. The HTTP layer is
//!   abstracted behind the [`anthropic::AnthropicTransport`] trait
//!   and a [`anthropic::MockTransport`] is provided. NO real HTTP
//!   client is included in dependencies; wiring a real transport
//!   (e.g. `reqwest`) is a one-file change documented in
//!   `crates/refineforge-strategies/README.md`.
//!
//! # What's intentionally NOT here
//!
//! - Real HTTP client. The skeleton is honest about being a skeleton.
//! - Local-LLM strategy (planned: Section 2 phase 2).
//! - Fine-tuned strategy (planned: Section 2 phase 2).

pub mod anthropic;

pub use anthropic::{
    AnthropicStrategy, AnthropicTransport, MockTransport,
    anthropic_mock_strategy,
};
