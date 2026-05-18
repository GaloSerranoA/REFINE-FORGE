# Changelog

All notable changes to refineforge are documented here.

This project follows a loose [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
style and [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Pre-1.0 releases may break compatibility in either direction without
a major bump; the version field will start tracking strictly once the
CLI surface is declared stable.

## [Unreleased]

### Added — Section 2 deep: real LLM repair + evaluation harness

This is the "go deep on one section" pass. Section 2 (ML
Training Engineer) moves from skeleton-only to a working repair
loop + a measurement framework.

- **Real `AnthropicStrategy` with `ReqwestTransport`** —
  `crates/refineforge-strategies/src/reqwest_transport.rs`.
  Blocking `reqwest` client (`rustls-tls`, no OpenSSL). Real POST
  to `https://api.anthropic.com/v1/messages`. Retry-with-
  exponential-backoff (1s, 2s, 4s; default 3 retries) for HTTP
  429 (rate limit) and 5xx (server error); honest distinct error
  reporting for 4xx (auth → don't retry + tell user to check
  `ANTHROPIC_API_KEY`; 400 bad request → don't retry; 404 →
  include model name in error; 413 payload-too-large → don't retry).
  Configurable base URL for tests.
- **Prompt caching** — `anthropic.rs` wire types refactored from
  single-string content to content-block arrays. System prompt
  and file-content block are marked
  `cache_control: { type: "ephemeral" }`; the diagnostic block
  (changes per iteration) is not. Sends
  `anthropic-beta: prompt-caching-2024-07-31`. Across iterations
  within a session this should cut cost by ~90 %.
- **CLI dispatch `--strategy anthropic`** — wired through
  `anthropic_strategy_from_env()`. Reads `ANTHROPIC_API_KEY`
  (required) and optional `ANTHROPIC_MODEL` (default
  `claude-opus-4-7`).
- **New crate `refineforge-eval` with `refine-eval` binary** —
  JSONL corpus loader, runner that drives `refineforge_cli::repair`
  per entry, metrics aggregator (repair rate, median + p95
  latency, per-outcome counts), JSON report writer with run
  metadata.
- **3-entry tutorial corpus** at `eval/corpus/example.jsonl`
  exercising three mutations of EXAMPLE-002 (Counter):
  - `counter-swap-lemma` — wrong lemma in `simp` call
  - `counter-wrong-tactic` — `rfl` where `simp [incr]` is needed
  - `counter-rename-field` — `value` → `val` in the struct but
    callers not updated (cross-cutting break)
  All three confirmed broken by `refine lean check` and surfaced
  by `refine-eval` as `NoProposal` under the mock strategy.
- **Runner pre-warms the temp project's `.lake/` cache** by
  invoking `lake build` on the unmodified source before swapping
  in the broken file. Without this, cold lake elaboration exceeds
  the LSP diagnostic timeout (20s) and breaks register as false
  `AlreadyClean`. Fix discovered during smoke testing; honesty
  win — without it we would have shipped a harness that lies.

### Tests

- Workspace test count: **47/47 pass** (was 32/32 before this
  pass; +15 = 8 reqwest_transport (success / retry-429 / retry-5xx /
  exhaustion / 401-no-retry / 400-no-retry / 404-includes-model /
  headers-correct) + 3 new anthropic cache-control behaviour tests
  + 4 eval crate tests (1 corpus + 3 metrics)).
- The transport tests use an in-process `tiny_http` stub server
  with configurable per-attempt responses and `backoff_base_ms = 1`
  so retry tests don't sleep.
- Smoke-tested `refine-eval --corpus eval/corpus/example.jsonl
  --strategy mock`: 3/3 entries report `NoProposal` (correct —
  files are broken, mock declines), 0/3 fixed (correct — mock
  never proposes). Latency 1.8 s per entry with pre-warm.

### Honest disclosures

- **The real `--strategy anthropic` path has NOT been exercised
  against a live Anthropic API** in this session. I have no
  `ANTHROPIC_API_KEY` to test with. The transport's HTTP framing,
  retry semantics, header generation, error mapping, and JSON
  parsing are all unit-tested against a local stub server — those
  paths work. The Anthropic API contract (URL, headers, body
  shape) follows the published `2023-06-01` spec + the
  `prompt-caching-2024-07-31` beta header; first real call will
  surface any mismatch.
- **Prompt is a first draft.** Built from the diagnostic
  message, severity, range, and full file content. No iteration
  feedback ("the gate rejected my last patch because…") because
  the trait surface is stateless. Smarter prompts are a future
  enhancement requiring a richer trait.
- **Eval corpus is tiny (3 entries).** This is the
  smoke-test-tier corpus from `docs/repair-evaluation.md` §2.1.
  Bootstrap CIs in `metrics.rs` are NOT implemented because they
  would be meaningless at N=3. The Mathlib-mutation pipeline that
  delivers N≥1000 is still Section 2 phase 1 item 3 — multi-week
  work, not in this session.
- **No fine-tuned model.** That's a 6+ month research commitment
  (compute time, training runs, evaluation iterations). Section 2
  phase 2/3 in the architecture's sequencing.
- **Runner copies the whole project per entry.** With pre-warm
  this is ~1.8 s per entry on the dev machine; with N=1000 entries
  this is 30 min per eval run. Acceptable; a parallel runner is a
  future optimisation.

### Added — Tier 3: structural scaffolding

- **New workspace crate `refineforge-repair-api`** — the stable
  cross-section trait surface. Contains `RepairStrategy`, `Patch`,
  `Diagnostic`, `Severity`, `Range`, `Position`, `MockStrategy`,
  and the LSP-types conversions. Sits between `refineforge-cli`
  (driver) and `refineforge-strategies` (implementers) to break
  what would otherwise be a circular dep. Owned by Section 1.
  9 unit tests, all passing.
- **New workspace crate `refineforge-strategies`** —
  `AnthropicStrategy<MockTransport>` skeleton: real trait impl,
  real prompt construction, real response parsing; mocked HTTP
  transport. Includes a `MockTransport::returns(json)` for unit
  tests and `MockTransport::declines()` for the CLI's
  `anthropic-mock` strategy. 7 unit tests, all passing.
- **`refineforge-cli` refactored into lib + bin** — `src/lib.rs`
  exposes the framework modules so external crates (today:
  `refineforge-strategies`) can import them. `src/main.rs`
  switched from `mod claim;` to `use refineforge_cli::{claim, ...};`.
  All existing functionality unchanged.
- **New CLI strategy `--strategy anthropic-mock`** — wires
  `refineforge_strategies::anthropic_mock_strategy()` into
  `refine repair`. Exercises the AnthropicStrategy prompt + parsing
  code path with a canned-decline transport; same end-user
  behaviour as `--strategy mock` (`NoProposal`) but proves the
  cross-crate wiring works.
- **`containers/Dockerfile.verifier`** — Section 3's first concrete
  win. Multi-stage Docker image: stage 1 builds `refine` from
  source with `--locked`; stage 2 is a Debian slim with elan +
  Lean v4.29.1 preinstalled. Reviewers run
  `docker run --rm -v $(pwd)/artifacts:/artifacts:ro
  refineforge-verifier bundle verify /artifacts/<CLAIM-ID>` —
  no local elan install needed. Honest disclosures inline: not
  reproducible-build-grade (use the Nix flake when it lands).

### Tests (Tier 3)

- Workspace test count: **32/32 pass** (was 19/19 before Tier 3;
  added 9 in `refineforge-repair-api` + 7 in `refineforge-strategies`
  minus 3 duplicate diagnostic + 6 duplicate strategy tests that
  moved out of `refineforge-cli`).
- Smoke tests: `refine repair EXAMPLE-002 --strategy mock` and
  `--strategy anthropic-mock` both report `AlreadyClean` in 0
  iterations (clean files). The `anthropic-mock` smoke proves the
  full cross-crate wiring runs.

### Honest disclosures (Tier 3)

- The Dockerfile is **untested** — Docker isn't available in this
  session's shell. It's syntactically clean and follows the standard
  multi-stage pattern; first build will surface any silly mistakes.
- `AnthropicStrategy` still cannot fix anything. The
  `MockTransport::declines()` it ships with returns `{}` which
  parses to `None`. The skeleton's value is the trait wiring + the
  prompt + the parser, all of which are unit-tested. Wiring a real
  `ReqwestTransport` is the one-file change documented in
  `crates/refineforge-strategies/README.md`.
- The lib refactor exposes more of `refineforge-cli`'s internals
  as public than is strictly needed (every `mod` became `pub mod`).
  A v0.2 tightening pass could mark some sub-items
  `pub(crate)` again. Today everything that's `pub` was already
  reachable by the binary; no NEW data is exposed.

### Added — Tier 2: design stubs for Sections 2 & 3

- **docs/security.md** (Section 3) — threat model that names the
  adversaries refineforge does and does NOT defend against; supply
  chain (what's in vs not in a bundle); planned Sigstore signing
  chain with `--verify-signature` flag design; vuln-reporting
  policy with 90-day disclosure window.
- **docs/reproducible-build.md** (Section 3) — bit-identical-rebuild
  methodology; enumerated sources of non-determinism with
  per-source fix; Nix flake approach (chosen) vs Bazel /
  Docker-only / SOURCE_DATE_EPOCH alternatives (rejected with
  reasons); verification protocol modelled on
  reproducible-builds.org.
- **docs/repair-evaluation.md** (Section 2) — benchmark methodology
  for `refine repair`; six metrics (repair rate, iters,
  latency, cost, false-fix, honesty); three corpora (tutorial-40,
  mathlib-5000, in-the-wild); eight-mutation taxonomy;
  training/eval separation invariants for any fine-tuned strategy;
  bootstrap-CI statistical reporting requirement.
- README documentation map and STRUCTURE.md docs table updated to
  include the three new docs, with each row tagged by owning
  section.

### Added — Tier 1: organisational layer

- **ARCHITECTURE.md** at repo root — three-section structure
  (Lean 4 Specialist, ML Training Engineer, Infrastructure/DevOps).
  Defines mission, owned subdirectories, current status, open work,
  and the two stable cross-section interfaces (`RepairStrategy`
  trait + bundle-manifest schema). Explicitly priority-ordered, with
  the warning *"if all three sections start at once with one
  engineer, every section is 30 % done and nothing ships."*
- **ROLES.md** at repo root — short-form ownership guide. Maps
  symptoms to likely owners; defines what ownership does and does
  not mean; documents the cross-section change protocol.
- **.github/CODEOWNERS** — path → section mapping using role
  identifiers (`@refineforge/lean-specialist`,
  `@refineforge/ml-engineer`, `@refineforge/devops`). Advisory
  until the repo gets a remote; replaces role identifiers with real
  GitHub handles at that point.
- README documentation map updated to include ARCHITECTURE.md and
  ROLES.md.
- STRUCTURE.md updated to show `.github/CODEOWNERS` and reference
  the architecture/roles split.

### Notes

- No source code changed; this is a pure organisational layer.
- Honest disclosure: the three roles can be filled by one person
  wearing all three hats. The boundary holds because it's about
  concerns, not headcount.

## [0.1.0] — 2026-05-18

Initial release. Forked from `helyx-proofforge` (HELYX trust-claim
project) and generalised into a project-agnostic framework.

### Added

- **Core CLI** (`refine`) with subcommands:
  - `claims list` / `claims show <id>` — claim registry inspection
  - `lean check <id>` / `lean check-all` — policy gate + `lake build`
  - `bundle export <id>` / `bundle verify <bundle-dir>` — SHA-256-sealed
    proof bundles for independent re-verification
  - `scan check <id>` / `scan check-all` — static name-presence check of
    every claim's `rust_source` block against the cited Rust file
  - `new --template <t> --module <M> <ID>` — scaffold a new claim from a
    template; auto-detects the Lean library root name from
    `lakefile.toml`'s `defaultTargets`
  - `templates` — list available scaffolding templates
  - `repair <id>` — **SKELETON ONLY** — bounded LLM repair loop with a
    real LSP client to `lake env lean --server`, real diagnostic
    parser, real driver loop with no-sorry policy gate, and a mocked
    `RepairStrategy` trait
- **Three scaffolding templates** under `templates/`: `append_chain`,
  `capability`, `state_machine`. All three verified end-to-end via
  `refine new` + `refine lean check`.
- **Two tutorial claims:**
  - `EXAMPLE-001` (`claims/example.yaml` +
    `lean/Refineforge/Example.lean`) — Lean-only hello-world theorem
    (`Nat.add_comm` wrapper) to exercise the Lean → policy gate →
    bundle path with no Rust
  - `EXAMPLE-002` (`claims/example-counter.yaml` +
    `lean/Refineforge/Counter.lean` + `crates/example-counter/`) —
    full refinement pattern, including a deliberate Lean-vs-Rust
    idealisation (unbounded `Nat` ↔ saturating `u64`) so the
    refinement doc has something non-trivial to argue
- **Refinement-argument template** at `docs/refinement-template.md`
  with explicit `[machine-checked]` vs `[needs human]` checklist
  distinction
- **Filled-in refinement doc** for `EXAMPLE-002` at
  `docs/refinement/EXAMPLE-002.md` — readable as the answer-key for
  the template
- **LLM repair-loop design doc** at `docs/llm-repair-design.md` —
  architecture, file map, stop conditions, four-step recipe for
  wiring an Anthropic strategy, what's deliberately NOT in the
  skeleton
- **Methodology and policy docs**:
  `docs/methodology.md` (the honest framing), `docs/no-sorry-policy.md`
  (what the policy gate enforces), `docs/HELYX-CASE-STUDY.md` (pointer
  to the external worked example)
- **CI workflow** at `.github/workflows/ci.yml` — builds Lean, builds
  CLI, runs `cargo test`, verifies every claim, exports and re-verifies
  the EXAMPLE-001 bundle

### Tests

- `cargo nextest run --workspace`: **26/26 passing**
  - 12 unit tests in the `repair` module (diagnostic conversion, LSP
    framing, patch-apply semantics across single-line / multi-line /
    insert / out-of-bounds, mock-strategy honesty)
  - 7 unit tests in the `sorry_gate` module (clean source, sorry-in-
    proof, sorry-in-line-comment, sorry-in-block-comment, nested-block-
    comment, word-boundary, axiom-declaration)
  - 7 integration tests in `example-counter` (one per Lean theorem,
    plus the documented idealisation gap at `u64::MAX`)
- All tests are deterministic; CI-friendly; do not require `lake`
  on PATH (the LSP path is smoke-tested manually)

### Honest disclosures

- **`refine repair` is a structural skeleton, not a working tool.**
  The shipped `MockStrategy` declines every proposal, so `refine
  repair` on a broken proof exits with `NoProposal`. The
  infrastructure (LSP client, diagnostic parser, driver loop,
  no-sorry gate after every applied patch) is real and tested;
  swapping in a real LLM is documented as a one-file change in
  [`docs/llm-repair-design.md`](docs/llm-repair-design.md) §4.
- **LSP end-to-end is not in CI.** Unit tests cover framing and
  conversion; the live-server path was smoke-tested manually on a
  developer machine (`AlreadyClean` in 0 iterations for both
  example claims). Adding a Lake-bearing CI job is on the bench.
- **No GitHub remote.** This is a local-only repo per the
  maintainer's preference at fork time. Push to your own remote
  when ready.
- **Pre-existing item carried from helyx-proofforge fork:** the
  bundle exporter's Windows path-separator fix (manifest keys
  normalised to forward slash; flat filenames produced via
  `\\`/`/` → `__` flattening). Discovered in HELYX, ported here as
  part of the initial fork.

### Carried over from helyx-proofforge MVP

- The Lean MVP shape (sorry-free theorem registry, `lake build` as
  the source of truth)
- The YAML claim registry schema (`claim_id`, `lean`, `rust_source`,
  `policy`, `review` blocks)
- The no-sorry policy gate (handles nested block comments, word
  boundaries, `axiom` top-level declarations)
- The bundle export / verify model (SHA-256 manifest, schema v1,
  refinement doc bundled when present)

### Not yet (called out for the next iteration)

- Real LLM strategy implementation (the design doc walks through the
  Anthropic SDK wiring; the trait surface is stable)
- Syn-based scan (parse Rust source rather than regex-match names)
- CI job exercising the live LSP path (needs Lake on the runner)
- Multi-file repair, patch rollback, cross-iteration conversation
  memory (called out in `docs/llm-repair-design.md` §5)

[Unreleased]: https://example.invalid/refineforge/compare/v0.1.0...HEAD
[0.1.0]: https://example.invalid/refineforge/releases/tag/v0.1.0
