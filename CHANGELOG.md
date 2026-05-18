# Changelog

All notable changes to refineforge are documented here.

This project follows a loose [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
style and [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Pre-1.0 releases may break compatibility in either direction without
a major bump; the version field will start tracking strictly once the
CLI surface is declared stable.

## [Unreleased]

### Added

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
