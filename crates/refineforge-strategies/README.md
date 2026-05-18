# refineforge-strategies

Pluggable `RepairStrategy` implementations for `refine repair`.

Owned by **Section 2: ML Training Engineer**
([ARCHITECTURE.md](../../ARCHITECTURE.md)).

## What ships today (skeleton)

| Strategy | Status | What it does |
|---|---|---|
| `AnthropicStrategy<MockTransport>` | **shipped** | Real trait wiring, real prompt construction, real response parsing — but the HTTP layer is mocked. `MockTransport::declines()` returns `{}` so the strategy parses to `None` and the repair loop reports `NoProposal`. |
| `AnthropicStrategy<ReqwestTransport>` | not yet | A real HTTP transport that hits `https://api.anthropic.com/v1/messages`. Recipe below. |
| `LocalLlmStrategy` | not yet | Ollama / llama.cpp local-inference transport. Section 2 phase 2. |
| `FineTunedStrategy` | not yet | refineforge-trained model. Section 2 phase 2/3. |

## How the trait surface works

The trait itself lives in
[`refineforge_cli::repair::RepairStrategy`](../refineforge-cli/src/repair/strategy.rs).
This crate imports it and provides implementations:

```
┌─────────────────────────────────────────────────────────┐
│  refineforge-cli  (Section 1)                           │
│   ↳ pub trait RepairStrategy { fn propose_patch … }     │
│   ↳ pub struct Patch / Diagnostic / Range / Position    │
└──────────────────┬──────────────────────────────────────┘
                   │ imports
                   ▼
┌─────────────────────────────────────────────────────────┐
│  refineforge-strategies  (Section 2 — this crate)       │
│   ↳ pub struct AnthropicStrategy<T: AnthropicTransport> │
│   ↳ pub trait AnthropicTransport { fn send … }          │
│   ↳ pub struct MockTransport (canned-response impl)     │
└─────────────────────────────────────────────────────────┘
```

The `AnthropicTransport` trait is the **internal** swap point inside
the AnthropicStrategy: it lets a real HTTP client be plugged in
without changing the prompt-construction or response-parsing code,
both of which are pure functions and fully unit-tested.

## Wiring a real transport

To turn the skeleton into a working tool, you write ONE file: a
`ReqwestTransport` that implements `AnthropicTransport`. Recipe:

### Step 1: add HTTP dependencies

```toml
# crates/refineforge-strategies/Cargo.toml
[dependencies]
reqwest    = { version = "0.12", features = ["blocking", "json"] }
# or, for async:
# reqwest = { version = "0.12", features = ["json"] }
# tokio   = { version = "1", features = ["rt", "macros"] }
```

### Step 2: implement the transport

```rust
// crates/refineforge-strategies/src/anthropic_reqwest.rs
use anyhow::{Context, Result};
use crate::anthropic::{AnthropicTransport, MessagesRequest, MessagesResponse};

pub struct ReqwestTransport {
    api_key: String,
    client: reqwest::blocking::Client,
}

impl ReqwestTransport {
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            client: reqwest::blocking::Client::new(),
        }
    }
}

impl AnthropicTransport for ReqwestTransport {
    fn send(&self, request: &MessagesRequest) -> Result<MessagesResponse> {
        let resp = self.client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .json(request)
            .send()
            .context("POST /v1/messages")?
            .error_for_status()
            .context("Anthropic API returned error status")?;
        let parsed: MessagesResponse = resp.json().context("parse Anthropic response")?;
        Ok(parsed)
    }
}
```

### Step 3: prompt-cache the file content

Each repair iteration sends the **same** file with one small
change. Anthropic's prompt caching saves ~90 % of the cost.
Mark the file block as cached:

```rust
// In build_request, restructure messages so the file_content
// is in its own cache_control block. See:
// https://docs.anthropic.com/en/docs/build-with-claude/prompt-caching
```

The current skeleton's `build_request` puts everything in one user
message for simplicity — restructure when you wire the real
transport.

### Step 4: register the strategy with the CLI

In [`refineforge-cli/src/main.rs`](../refineforge-cli/src/main.rs):

```rust
use refineforge_strategies::anthropic::{AnthropicStrategy, ReqwestTransport};

// inside repair::run_cli's strategy match:
"anthropic" => {
    let key = std::env::var("ANTHROPIC_API_KEY")
        .map_err(|_| anyhow::anyhow!("ANTHROPIC_API_KEY not set"))?;
    Box::new(AnthropicStrategy::new(
        key, "claude-opus-4-7", ReqwestTransport::new(key.clone())
    ))
}
```

Once wired, `refine repair <CLAIM-ID> --strategy anthropic`
becomes a working tool. Test with EXAMPLE-002 first (introduce a
deliberate typo in `Refineforge.Counter`, run the repair loop,
verify the strategy proposes a fix that lake build accepts).

## Honest disclosures

- **The strategy in this crate today CANNOT repair anything.** The
  mock transport returns `{}` which parses to `None`. The CLI's
  `--strategy anthropic-mock` reports `NoProposal` just like
  `--strategy mock` — the value is in exercising the prompt and
  parsing code paths, not in producing fixes.
- **No real HTTP client is in the dependency graph.** This is
  intentional: the skeleton's `cargo build` works in any
  environment without network deps. Adding `reqwest` is a
  deliberate step the operator takes.
- **The prompt is a first draft.** It will need iteration once
  real diagnostics come back from real claims. Section 2 phase 1
  includes an eval harness (see
  [`../../docs/repair-evaluation.md`](../../docs/repair-evaluation.md))
  that measures whether prompt changes actually improve repair rate.
- **No telemetry, no token-budget enforcement, no retry policy.**
  All three should land before this strategy sees production use.
- **No multi-turn / conversation memory across iterations.** Each
  `propose_patch` call is stateless. The driver loop in
  `refineforge_cli::repair` is also stateless across iterations.
  Smarter strategies could remember "the gate rejected my last
  patch because it introduced `sorry`" — that's a future
  enhancement requiring a richer trait surface.

## Tests

8 unit tests in `src/anthropic.rs`:

- `build_request_includes_diagnostic_message_and_file`
- `parse_response_into_patch_succeeds_on_valid_json`
- `parse_response_strips_markdown_fences`
- `parse_response_returns_none_on_empty_object`
- `parse_response_returns_none_on_malformed_json`
- `end_to_end_with_mock_transport_returning_patch`
- `anthropic_mock_strategy_factory_declines`

Run with `cargo nextest run -p refineforge-strategies`. All tests
are pure-Rust; no network, no Lake, no API key needed.
