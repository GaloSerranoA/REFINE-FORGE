# LLM repair loop — design + swap-in guide

> **Status:** Structural skeleton landed (commit-id at the top of
> [`crates/refineforge-cli/src/repair/`](../crates/refineforge-cli/src/repair)).
> The LSP client, diagnostic parser, repair driver, and `RepairStrategy`
> trait are real. The shipped `MockStrategy` declines every diagnostic.
> This doc explains the architecture and how to swap in a real
> LLM-backed strategy.

## 1. Goal

Bounded-iteration repair: given a Lean proof that `lake build` rejects,
let an LLM propose patches until the proof type-checks or we hit a
ceiling. The doctrine — *LLM proposes, Lean verifies, human approves* —
is enforced by:

- Lean (via `lake build` through LSP) is the only oracle of correctness.
- The no-sorry policy gate runs after **every** applied patch — a
  proposal that would introduce `sorry`/`admit`/non-core `axiom` is
  rejected even if Lean would accept it.
- The repair report records every iteration (diagnostics, proposed
  patch, accept/reject) so a human reviewer can audit what changed and
  why.

## 2. Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│ refine repair <CLAIM-ID> [--max-iterations N] [--strategy NAME] │
│                          [--dry-run]                            │
└──────────────────────────────────┬──────────────────────────────┘
                                   │
                  ┌────────────────▼────────────────┐
                  │  repair::run_cli                │
                  │  - loads claim                  │
                  │  - selects RepairStrategy       │
                  │  - calls repair::repair()       │
                  └────────────────┬────────────────┘
                                   │
                  ┌────────────────▼────────────────┐
                  │  repair::repair (driver)        │
                  │  for i in 0..max_iterations {   │
                  │     diagnostics = lsp.collect() │
                  │     if empty → Fixed            │
                  │     patch = strategy.propose()  │
                  │     if None → NoProposal        │
                  │     reject if gate fails        │
                  │     apply patch + didChange     │
                  │  }                              │
                  └────┬──────────────────┬─────────┘
                       │                  │
              ┌────────▼─────┐    ┌───────▼──────────┐
              │  lsp.rs      │    │  strategy.rs     │
              │  spawn lake  │    │  trait           │
              │  initialize  │    │  RepairStrategy  │
              │  didOpen     │    │                  │
              │  didChange   │    │  impl:           │
              │  diagnostics │    │  - MockStrategy  │
              │  shutdown    │    │  - (your LLM)    │
              └──────┬───────┘    └──────────────────┘
                     │
              ┌──────▼────────────────────┐
              │  subprocess:              │
              │  lake env lean --server   │
              │  (JSON-RPC over stdio)    │
              └───────────────────────────┘
```

### File map

| File                                                                | Responsibility                                                              |
|---------------------------------------------------------------------|-----------------------------------------------------------------------------|
| [`repair/mod.rs`](../crates/refineforge-cli/src/repair/mod.rs)      | Public API, `RepairConfig`, `RepairReport`, `RepairOutcome`, driver loop    |
| [`repair/lsp.rs`](../crates/refineforge-cli/src/repair/lsp.rs)      | `LeanLspClient`: subprocess + JSON-RPC framing + message handling           |
| [`repair/diagnostic.rs`](../crates/refineforge-cli/src/repair/diagnostic.rs) | Plain `Diagnostic`/`Severity`/`Range` types + `From<lsp_types::*>` conversions |
| [`repair/strategy.rs`](../crates/refineforge-cli/src/repair/strategy.rs)    | `Patch`, `RepairStrategy` trait, `MockStrategy` (declines all)              |

## 3. Stop conditions and outcomes

| `RepairOutcome`                  | When                                                                 |
|----------------------------------|----------------------------------------------------------------------|
| `AlreadyClean`                   | Iteration 0 sees no diagnostics                                      |
| `Fixed { iterations }`           | An iteration sees no diagnostics after a previous patch              |
| `NoProposal`                     | Strategy returned `None` for the current diagnostic                  |
| `UnrecoverableError(reason)`     | Patch would introduce `sorry`/`admit`/axiom — rejected by gate       |
| `MaxIterationsReached`           | Hit the iteration ceiling without converging                         |

## 4. How to swap in a real LLM strategy

The trait is small:

```rust
pub trait RepairStrategy {
    fn propose_patch(
        &self,
        diagnostic: &Diagnostic,
        file_content: &str,
    ) -> anyhow::Result<Option<Patch>>;

    fn name(&self) -> &'static str;
}
```

To wire up Anthropic's Claude as a strategy:

### Step 1: add the dependency

```toml
# crates/refineforge-cli/Cargo.toml
[dependencies]
anthropic-sdk = "0.x"   # or whichever HTTP client you prefer
tokio          = { version = "1", features = ["rt", "macros"] }
```

(The skeleton is sync — for an async SDK you'll wrap the call in a
small `tokio::runtime::Runtime::block_on`.)

### Step 2: implement the strategy

```rust
// crates/refineforge-cli/src/repair/anthropic_strategy.rs
use super::diagnostic::Diagnostic;
use super::strategy::{Patch, RepairStrategy};
use anyhow::Result;

pub struct AnthropicStrategy {
    pub api_key: String,
    pub model: &'static str,    // e.g. "claude-opus-4-7"
    pub max_tokens: u32,
}

impl RepairStrategy for AnthropicStrategy {
    fn propose_patch(
        &self,
        diagnostic: &Diagnostic,
        file_content: &str,
    ) -> Result<Option<Patch>> {
        // 1. Build prompt: include file_content, the diagnostic
        //    range/message, and the Lean theorem context (lookup
        //    by line number). Ask for a JSON response shaped like
        //    Patch (range + new_text + rationale).
        // 2. POST to https://api.anthropic.com/v1/messages
        //    with prompt caching on the file_content (this is the
        //    big payload).
        // 3. Parse the JSON response into Patch. Return None if
        //    the model refuses or returns malformed JSON.
        todo!("see prompt-caching guidance in CLAUDE.md")
    }

    fn name(&self) -> &'static str { "anthropic" }
}
```

### Step 3: wire it into the CLI dispatch

In [`repair/mod.rs::run_cli`](../crates/refineforge-cli/src/repair/mod.rs):

```rust
let strategy: Box<dyn RepairStrategy> = match strategy_name {
    "mock" => Box::new(MockStrategy),
    "anthropic" => {
        let key = std::env::var("ANTHROPIC_API_KEY")
            .map_err(|_| anyhow::anyhow!("ANTHROPIC_API_KEY not set"))?;
        Box::new(AnthropicStrategy {
            api_key: key,
            model: "claude-opus-4-7",
            max_tokens: 4096,
        })
    }
    other => anyhow::bail!("unknown strategy '{other}'"),
};
```

### Step 4: prompt-cache the file content

Because each repair iteration sends the **same** file (with one small
change), prompt caching saves ~90 % of the cost. Mark
`file_content` as a cached block; the diagnostic message + tail of
the prompt are the only per-request payload.

See [Anthropic prompt caching docs](https://docs.anthropic.com/) and
the project-level `CLAUDE.md` for the load-bearing rules.

## 5. What's deliberately NOT in the skeleton

- **Concurrency / async runtime.** The LSP client is sync (sync
  subprocess stdio + reader thread + `mpsc::channel`). Async would
  be cleaner but adds a tokio dependency. Defer until needed.
- **Multi-file repair.** Today the repair loop targets one Lean
  file per claim. Cross-file diagnostics (e.g. a fix in
  `Refineforge/Foo.lean` triggered by a problem in
  `Refineforge/Bar.lean`) aren't modelled. Most real claims live
  in one file so this is fine for v1.
- **Patch rollback.** A patch that compiles but introduces a
  semantic regression elsewhere is not detected. The honest answer
  is "rerun `refine lean check-all` after a repair session and
  review the diff manually." A future version could automate this.
- **Cost / token budgeting.** No per-session token caps. Add when
  you have a real strategy that hits the API.
- **Conversation memory across iterations.** Each call to
  `propose_patch` is stateless. A smarter strategy could keep a
  per-session conversation with the model and feed back which
  patches were rejected by the gate, but that's not the v1 trait.

## 6. Trusted code base (in addition to the framework's general TCB)

- `lake` and the Lean LSP server (same trust assumption as
  `refine lean check` — we trust Lean to honestly report
  diagnostics).
- Whatever LLM provider you plug in. The repair report records the
  strategy name so audit logs can pinpoint which model made which
  proposal.
- The strategy's network stack (TLS, HTTP) — same as any API client.

## 7. Tests

What ships:

| Test                                                       | What it proves                                                              |
|------------------------------------------------------------|-----------------------------------------------------------------------------|
| `diagnostic::tests::*` (3)                                 | LSP `Diagnostic`/`Severity`/`Range` conversions are correct                 |
| `lsp::tests::roundtrip_message_with_simple_payload`        | `Content-Length` framing round-trips                                        |
| `lsp::tests::read_message_ignores_extra_headers`           | The reader handles `Content-Type` and other unknown headers                 |
| `lsp::tests::path_to_uri_uses_forward_slashes`             | URI generation is cross-platform-safe                                       |
| `strategy::tests::mock_strategy_declines_all_proposals`    | `MockStrategy` is honest about doing nothing                                |
| `strategy::tests::patch_apply_*` (4)                       | `Patch::apply` correctly handles single-line, multi-line, insert, OOB cases |

What does NOT ship in CI (requires `lake` on PATH):

- End-to-end repair against a deliberately-broken Lean file.
- Spawn/initialize/didOpen/shutdown sequence against the real
  Lean LSP server.

Manual smoke tests covered both `AlreadyClean` outcomes for
EXAMPLE-001 and EXAMPLE-002 on the developer machine; production
CI would need a Lean-bearing job to exercise the LSP path.

## 8. Open questions for the next iteration

1. **Patch scope.** Should patches be allowed to span multiple
   files? Current API says no.
2. **Tactic vs term mode.** Some diagnostics are tactic-level
   ("unsolved goals"), others are term-level ("type mismatch").
   Should the strategy receive the elaborator state (Lean's
   `MessageData`) or just the raw range?
3. **Goal context.** Should we ask the LSP server for the goal at
   the diagnostic position (`$/lean/plainGoal`) and include it in
   the strategy input? Today the strategy only sees the diagnostic
   message and the file text — adding goal context would
   materially improve LLM proposals.
4. **Per-iteration timeout.** Currently `collect_diagnostics`
   timeout is 20s. Some Lean files take much longer to elaborate.
   Make this configurable.
