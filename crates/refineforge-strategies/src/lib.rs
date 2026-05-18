//! Pluggable `RepairStrategy` implementations for `refine repair`.
//!
//! See [`ARCHITECTURE.md`](../../ARCHITECTURE.md) §2: this crate is
//! owned by the ML Training Engineer section. Section 1 owns the
//! trait (in `refineforge_cli::repair::RepairStrategy`); this crate
//! consumes it.
//!
//! # What ships today
//!
//! - [`anthropic::AnthropicStrategy`]: real `RepairStrategy` impl
//!   over an abstract [`anthropic::AnthropicTransport`]. Real
//!   prompt construction (with Anthropic prompt caching markers)
//!   and real response parsing.
//! - [`anthropic::MockTransport`]: canned-response transport for
//!   the `anthropic-mock` CLI strategy + unit tests.
//! - [`reqwest_transport::ReqwestTransport`]: **real HTTP transport**
//!   targeting `https://api.anthropic.com/v1/messages`. Blocking
//!   `reqwest` client (`rustls-tls`); retry with exponential backoff
//!   for 429 and 5xx; distinct error reporting for 4xx
//!   (auth / bad request / model not found / payload too large).
//!
//! # What's intentionally NOT here
//!
//! - Local-LLM strategy (planned: Section 2 phase 2). The trait
//!   surface and the retry/error pattern in `reqwest_transport`
//!   are reusable as a starting point.
//! - Fine-tuned strategy (planned: Section 2 phase 3). Requires a
//!   trained model artifact — a research commitment, not a
//!   single-session engineering task.

pub mod anthropic;
pub mod reqwest_transport;

pub use anthropic::{
    anthropic_mock_strategy, AnthropicStrategy, AnthropicTransport, MockTransport,
};
pub use reqwest_transport::ReqwestTransport;

// ─── Convenience factories used by the refine CLI ───────────────────────

use anyhow::{anyhow, Result};
use refineforge_repair_api::RepairStrategy;

/// Build the real Anthropic strategy from the environment.
/// Used by the `refine repair --strategy anthropic` dispatch.
///
/// Reads `ANTHROPIC_API_KEY` (required) and optionally
/// `ANTHROPIC_MODEL` (default: `claude-opus-4-7`).
pub fn anthropic_strategy_from_env() -> Result<Box<dyn RepairStrategy>> {
    let key = std::env::var("ANTHROPIC_API_KEY").map_err(|_| {
        anyhow!("ANTHROPIC_API_KEY env var is not set — refine repair --strategy anthropic needs it")
    })?;
    let model = std::env::var("ANTHROPIC_MODEL").unwrap_or_else(|_| "claude-opus-4-7".into());
    let transport = ReqwestTransport::new(key.clone());
    Ok(Box::new(AnthropicStrategy::new(key, model, transport)))
}
