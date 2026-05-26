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
//! - [`local_finetune::LocalFinetuneStrategy`]: local fine-tuned
//!   strategy runtime loaded from a weights-directory manifest.
//!
//! # What's intentionally NOT here
//!
//! - Local-LLM strategy (planned: Section 2 phase 2). The trait
//!   surface and the retry/error pattern in `reqwest_transport`
//!   are reusable as a starting point.
//! - Native candle-backed local inference. The shipped
//!   `local-finetune` strategy uses a command runtime manifest so
//!   real checkpoint artifacts can be wired without changing the
//!   `RepairStrategy` trait.

pub mod anthropic;
pub mod local_finetune;
pub mod reqwest_transport;

pub use anthropic::{
    anthropic_mock_strategy, anthropic_mock_strategy_with_usage, AnthropicStrategy,
    AnthropicTransport, MockTransport, UsageStats,
};
pub use local_finetune::{
    local_finetune_from_path, local_finetune_from_path_with_usage,
    local_finetune_typed_with_template, LocalFinetuneStrategy,
};
pub use reqwest_transport::ReqwestTransport;

use std::sync::{Arc, Mutex};

// ─── Convenience factories used by the refine CLI ───────────────────────

use anyhow::{anyhow, Result};
use refineforge_repair_api::RepairStrategy;

pub type StrategyWithUsage = (Box<dyn RepairStrategy>, Arc<Mutex<UsageStats>>);

/// Build the real Anthropic strategy from the environment.
/// Used by the `refine repair --strategy anthropic` dispatch.
///
/// Reads `ANTHROPIC_API_KEY` (required) and optionally
/// `ANTHROPIC_MODEL` (default: `claude-opus-4-7`).
pub fn anthropic_strategy_from_env() -> Result<Box<dyn RepairStrategy>> {
    anthropic_strategy_from_env_with_usage().map(|(s, _)| s)
}

/// Like [`anthropic_strategy_from_env`] but also returns a handle
/// to the shared usage accumulator. The autonomous driver uses
/// this so it can read total Anthropic token counts after the
/// repair loop completes.
pub fn anthropic_strategy_from_env_with_usage() -> Result<StrategyWithUsage> {
    let key = std::env::var("ANTHROPIC_API_KEY").map_err(|_| {
        anyhow!(
            "ANTHROPIC_API_KEY env var is not set — refine repair --strategy anthropic needs it"
        )
    })?;
    let model = std::env::var("ANTHROPIC_MODEL").unwrap_or_else(|_| "claude-opus-4-7".into());
    let transport = ReqwestTransport::new(key.clone());
    let handle = Arc::new(Mutex::new(UsageStats::default()));
    let strategy = AnthropicStrategy::with_usage_stats(key, model, transport, handle.clone());
    Ok((Box::new(strategy), handle))
}
