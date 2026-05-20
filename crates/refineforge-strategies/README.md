# refineforge-strategies

Pluggable `RepairStrategy` implementations for `refine repair`.

Owned by **Section 2: ML Training Engineer**
([ARCHITECTURE.md](../../ARCHITECTURE.md)).

## What ships today

| Strategy | Status | What it does |
|---|---|---|
| `AnthropicStrategy<MockTransport>` | **shipped** | Real trait wiring, real prompt construction, real response parsing — but the HTTP layer is mocked. `MockTransport::declines()` returns `{}` so the strategy parses to `None` and the repair loop reports `NoProposal`. |
| `AnthropicStrategy<ReqwestTransport>` | **shipped** | Real HTTP transport for `https://api.anthropic.com/v1/messages`, with retry/error mapping and prompt-cache headers. |
| `LocalFinetuneStrategy` | **shipped bridge** | Loads a local weights/runtime directory containing `refineforge-local-finetune.json`, invokes the declared command runtime, parses patch JSON, and records local token usage. |
| Native candle backend | not yet | Planned replacement for the command runtime once real checkpoint artifacts and architecture support are pinned. |
| `LocalLlmStrategy` | not yet | Ollama / llama.cpp local-inference transport. Section 2 phase 2. |

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
│   ↳ pub struct LocalFinetuneStrategy                    │
└─────────────────────────────────────────────────────────┘
```

The `AnthropicTransport` trait is the **internal** swap point inside
the AnthropicStrategy: it lets a real HTTP client be plugged in
without changing the prompt-construction or response-parsing code,
both of which are pure functions and fully unit-tested.

The `LocalFinetuneStrategy` is the fine-tuned model bridge. It keeps
the public `RepairStrategy` trait stable while the actual model
runtime can be swapped underneath.

## Local fine-tune runtime manifest

`--strategy local-finetune` expects a weights/runtime directory passed
as `--weights-path` or via `REFINEFORGE_LOCAL_FINETUNE_WEIGHTS`.
That directory must contain `refineforge-local-finetune.json`:

```json
{
  "runtime": "command",
  "model_id": "qwen-proof-repair-v1",
  "command": ["path/to/infer-once", "--weights", "path/to/weights"]
}
```

For each diagnostic, refineforge sends one JSON request to the
command's stdin. The command writes either a raw patch object:

```json
{"start_line":0,"start_char":1,"end_line":0,"end_char":2,"new_text":"trivial","rationale":"..."}
```

or an envelope with usage:

```json
{
  "patch": {"start_line":0,"start_char":1,"end_line":0,"end_char":2,"new_text":"trivial"},
  "usage": {"input_tokens": 120, "output_tokens": 32},
  "stop_reason": "end_turn"
}
```

`{}` or `"patch": null` is a clean decline. The native candle backend
is still open work; this command runtime is the stable integration
contract for the first trained checkpoint.

## Anthropic transport notes

`ReqwestTransport` is already wired. The recipe below remains as the
small-provider pattern for adding another HTTP-backed strategy without
changing the repair-loop trait:

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

`refine repair <CLAIM-ID> --strategy anthropic` uses the shipped
transport and requires `ANTHROPIC_API_KEY`.

## Honest disclosures

- **`anthropic-mock` cannot repair anything.** The mock transport
  returns `{}` which parses to `None`; its value is exercising the
  prompt and parsing code paths without an API key.
- **`local-finetune` does not train or embed a model.** It runs a
  local runtime command declared by the weights directory. The command
  is responsible for actual inference.
- **Native candle loading is not implemented yet.** It should land
  after the first real checkpoint fixes the base architecture,
  tokenizer, and safetensors layout.
- **The prompt is a first draft.** It will need iteration once
  real diagnostics come back from real claims. Section 2 phase 1
  includes an eval harness (see
  [`../../docs/repair-evaluation.md`](../../docs/repair-evaluation.md))
  that measures whether prompt changes actually improve repair rate.
- **No local retry policy.** The command runtime is invoked once per
  diagnostic; retry/backoff belongs in the runtime command or a later
  strategy update.
- **No multi-turn / conversation memory across iterations.** Each
  `propose_patch` call is stateless. The driver loop in
  `refineforge_cli::repair` is also stateless across iterations.
  Smarter strategies could remember "the gate rejected my last
  patch because it introduced `sorry`" — that's a future
  enhancement requiring a richer trait surface.

## Tests

Strategy tests cover Anthropic prompt construction, response parsing,
usage capture, HTTP transport retry/error mapping, and local fine-tune
manifest/command behavior. Representative tests:

- `build_request_uses_two_content_blocks_for_caching`
- `parse_response_into_patch_succeeds_on_valid_json`
- `parse_response_strips_markdown_fences`
- `parse_response_returns_none_on_empty_object`
- `parse_response_returns_none_on_malformed_json`
- `end_to_end_with_mock_transport_returning_patch`
- `anthropic_mock_strategy_factory_declines`
- `local_finetune_manifest_command_returns_patch_and_usage`

Run with `cargo test -p refineforge-strategies`. All tests are
pure-Rust; no network, no Lake, no API key, and no real model
checkpoint required.
