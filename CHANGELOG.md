# Changelog

All notable changes to refineforge are documented here.

This project follows a loose [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
style and [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Pre-1.0 releases may break compatibility in either direction without
a major bump; the version field will start tracking strictly once the
CLI surface is declared stable.

## [Unreleased]

### Added — Section 3 deep: production-grade CI + Sigstore + release scripting

This is the "go deep on Section 3" pass. Multi-arch CI matrix,
keyless Sigstore signing in CI, real `--verify-signature`
implementation in `refine bundle verify`, and reusable release
scripts. Deferred: Nix flake (Lean integration is genuinely 1-3
days of focused work).

- **Multi-arch CI matrix** —
  `.github/workflows/ci.yml`. `build-and-verify` job runs in a
  strategy matrix across `ubuntu-latest`, `macos-latest`, and
  `windows-latest` with `fail-fast: false`. Each runner gets:
  - `actions/cache@v4` for elan + the pinned Lean toolchain
    (key includes `LEAN_TOOLCHAIN` env so cache invalidates on
    Lean version bumps).
  - `actions/cache@v4` for the `lean/.lake/` build artifacts
    (key includes `hashFiles('lean/**/*.lean', 'lean/lakefile.toml',
    'lean/lake-manifest.json')`).
  - `actions/cache@v4` for cargo registry + git deps.
  - `actions/cache@v4` for the `target/` build dir keyed on
    `Cargo.lock` + Rust source.
  - POSIX/Windows-aware elan installation (curl + sh on
    POSIX; Invoke-WebRequest + the official Windows installer
    on Windows).
  - Build Lean, build CLI, run unit tests, verify all claims,
    export + re-verify both example bundles, upload bundle
    artifacts (retention 14 days) per-OS.

- **Sigstore keyless signing in CI** — `sign-bundles` job runs
  AFTER `build-and-verify` succeeds, only on push to `main` or
  tags `v*` (NOT on pull requests, because PR OIDC identities
  are not the canonical signer). The job:
  - Has `permissions: id-token: write` to get the OIDC token
    Fulcio needs for keyless cert issuance.
  - Installs cosign v2.4.1 via `sigstore/cosign-installer@v3`.
  - Downloads the canonical (Ubuntu) builder's bundle artifacts.
  - For each `artifacts/<CLAIM-ID>/manifest.json`, runs
    `cosign sign-blob --yes --bundle manifest.json.sigbundle
    --output-signature manifest.json.sig --output-certificate
    manifest.json.cert manifest.json`. Sigstore handles Fulcio
    cert issuance + Rekor transparency-log entry.
  - Runs `cosign verify-blob` locally as a sanity check before
    uploading.
  - Uploads `refineforge-bundles-signed` (retention 90 days).

- **`refine bundle verify --verify-signature` flag** —
  `crates/refineforge-cli/src/bundle.rs`. Real Sigstore verification:
  - New `VerifyOptions { verify_signature, identity_regex,
    oidc_issuer }` struct + `verify_with_options(path, opts)`
    entry point. The original `verify(path)` is preserved as a
    thin wrapper for backward compatibility.
  - Delegates the cryptographic work to `cosign verify-blob` as
    a subprocess (same security guarantees as cosign upstream;
    no reimplementation of Fulcio cert chain validation, Rekor
    inclusion proof, or signature math). Pure-Rust verification
    via the `sigstore` crate is documented as a future option
    in [SECURITY.md](SECURITY.md).
  - Sensible defaults: identity regex matches refineforge's
    canonical CI workflow; OIDC issuer is GitHub Actions. Both
    overridable via CLI flags (`--identity-regex`, `--oidc-issuer`)
    OR env vars (`REFINEFORGE_EXPECTED_IDENTITY_REGEX`,
    `REFINEFORGE_EXPECTED_OIDC_ISSUER`).
  - Honest error messages: missing `manifest.json.sigbundle`
    points at the CI workflow; missing cosign binary tells you
    to install it from sigstore/cosign and offers a `REFINEFORGE_COSIGN_BIN`
    env-var escape hatch.
  - `cosign` binary location overridable via `REFINEFORGE_COSIGN_BIN`
    (used by unit tests + air-gapped deployments).
  - 5 unit tests using stub cosign shell scripts: missing sigbundle,
    missing cosign binary, success path returns SignatureStatus,
    verify failure surfaces cosign's stderr, identity-regex
    override is honored.

- **Release scripts** — `release/release.sh` (POSIX) and
  `release/release.ps1` (PowerShell). 12 numbered steps:
  semver-validate, clean-tree check, on-main check, tag-uniqueness
  (local + remote), CHANGELOG `[Unreleased]` → `[<version>] — <date>`
  migration, Cargo.toml `[workspace.package].version` bump, cargo
  check + nextest, `refine lean check-all`, version-bump commit,
  annotated tag creation, optional `cosign sign-blob` over the
  tag commit SHA (best-effort; skipped if cosign not on PATH),
  push-instructions printout. Both scripts support `--dry-run`
  (`-DryRun`); neither pushes automatically.

- **SECURITY.md at repo root** — entry-point doc with:
  vulnerability-reporting policy (90-day disclosure window,
  CHANGELOG credit), `refine bundle verify --verify-signature`
  usage walkthrough, threat-model summary (what refineforge
  defends against and what it does NOT), current signing-chain
  status table, and honest disclosure that the verification code
  was unit-tested against stub cosign binaries but NOT against a
  real Fulcio cert in this session (requires a real CI run from a
  pushed remote).

- **docs/security.md §3 promoted from "planned" to "shipped"** —
  signing chain, signature-flag wiring, and the cosign-subprocess
  implementation choice are now documented as the current state,
  with the pure-Rust `sigstore` crate path documented as a future
  enhancement.

- **README documentation map + framework build plan + subcommand
  reference** updated for SECURITY.md, multi-arch CI, sigstore,
  release scripting.

- **STRUCTURE.md** updated: top-level tree shows `SECURITY.md` and
  `release/`; `.github/workflows/ci.yml` row notes the multi-arch
  + signing role.

### Tests

- `cargo nextest run --workspace`: **55/55 pass** (was 50/50; +5
  signature verification tests).
- Smoke-tested `refine bundle verify artifacts/EXAMPLE-002
  --verify-signature` on an unsigned bundle locally: correctly
  fails with the helpful "no signature found — expected
  manifest.json.sigbundle (signed bundles are produced by the
  CI signing job...)" error.
- Smoke-tested `refine bundle verify artifacts/EXAMPLE-002`
  (no `--verify-signature`) on the same unsigned bundle: still
  succeeds (backward compatible).

### Honest disclosures

- **The CI workflow file has NOT been exercised by a real GitHub
  Actions run this session.** This repo has no remote configured
  yet. The YAML follows the documented schema for `actions/cache@v4`,
  `sigstore/cosign-installer@v3`, `actions/upload-artifact@v4`,
  and `actions/download-artifact@v4`; first push to a real remote
  will surface any drift.
- **The Sigstore signing flow has NOT been end-to-end tested
  against the real Fulcio CA + Rekor log.** The signing happens
  only in CI (because keyless OIDC signing requires the GitHub
  Actions OIDC token); we can't simulate that locally. The
  verifier-side code path was unit-tested against stub cosign
  binaries that simulate the success / failure / missing-binary
  cases; the actual cryptographic verification is cosign's job
  and is tested by the cosign upstream.
- **`--verify-signature` requires `cosign` on the verifier's PATH.**
  This is a deliberate v1 choice. A future pure-Rust verifier
  using the `sigstore` crate (no cosign dep) is documented in
  SECURITY.md as an enhancement.
- **Nix flake is NOT in this commit.** Lean toolchain via Nix
  (`lean4-nix`) is non-trivial; honest estimate is 1-3 days of
  focused work. `docs/reproducible-build.md` documents the
  approach; Section 3 phase 2 will deliver it.
- **The release scripts have NOT been exercised on this repo.**
  They run dry through `--dry-run` mode; the live mode requires
  a CHANGELOG with an `[Unreleased]` section (which we have) and
  a clean working tree (which we have between commits). The
  `--dry-run` mode is the recommended first invocation for any
  human operator.

### First real eval run + bug fixes discovered by it

Real `--strategy anthropic` baseline against the 3-entry tutorial
corpus using `claude-opus-4-7`. The eval was honest about what it
found — including two real bugs that the eval itself surfaced and
that we fixed in this pass.

#### Real numbers (after bug fixes)

| Run | Result | Notes |
|---|---|---|
| `eval/runs/anthropic-baseline.json` (v1) | 1/3 fixed (33 %) | First real run; identified two bugs |
| `eval/runs/anthropic-baseline-v3-explicit-indexing.json` | 1/3 fixed (33 %) | Prompt clarified to say 0-indexed positions; no change |
| **`eval/runs/anthropic-v4-after-bugfix.json`** | **2/3 fixed (67 %)** | After fixing the two bugs below |
| `eval/runs/anthropic-v5-maxiter5.json` | 2/3 fixed (67 %) | `--max-iterations 5`; counter-swap-lemma still defeats Claude |

Median latency ~12 s per attempt. counter-wrong-tactic and
counter-rename-field both Fixed; counter-swap-lemma defeats Claude
at this prompt because the broken file's structure cascades errors
that the patches don't fully clean up across iterations.

#### Bugs found by the eval + fixed in this pass

1. **`repair::repair` under-reported `Fixed`** — the loop checked
   diagnostics *before* applying each patch and broke out on the
   first clean read, but never re-checked after the *last*
   iteration's patch. counter-rename-field's iter-2 patch produced
   a correct file but the loop reported `MaxIterationsReached`.
   **Fix**: after the loop exits without converging, do one final
   `collect_diagnostics` to decide between `Fixed { iterations:
   max }` and `MaxIterationsReached`. ([`repair/mod.rs`](crates/refineforge-cli/src/repair/mod.rs))
2. **`Patch::apply` didn't clamp character to line length** —
   when Claude's `end.character` overshot the line's content
   length, byte-offset arithmetic walked past the line terminator
   and into the next line. Result on counter-swap-lemma's first
   eval: `simp [incr]simp [incr]ncreases (c : Counter) ...`
   (concatenation of the original line tail with the start of the
   next line). **Fix**: compute the line's content end (excluding
   `\r\n` or `\n` terminator) and clamp character to that.
   ([`refineforge-repair-api/src/lib.rs`](crates/refineforge-repair-api/src/lib.rs))
   + 3 new unit tests covering LF clamping, CRLF clamping, and
   multi-line replacement-preserves-following-line.

#### Prompt clarification

The `build_request` user-message diagnostic block now states "0-indexed,
LSP convention" for both the diagnostic range and the patch
position keys, plus documents the patch-substring semantics
(`start_line:start_char` inclusive, `end_line:end_char` exclusive;
`new_text` applied verbatim; out-of-bounds positions clamped). Did
NOT move the headline number (1/3 → 1/3 between v1 and v3) — the
gain came from the two real bug fixes.

#### Eval harness enrichment

`EntryResult` now includes:
- `iteration_log: Vec<IterationSummary>` — per-iteration
  diagnostic count, first diagnostic message, patch range,
  patch new-text, rationale, accept/reject, notes.
- `final_file: Option<String>` — full file contents after the
  last iteration so a reviewer can diff against the ground truth
  without re-running.

Without these fields, the headline "1/3 fixed" was the only signal
— with them, the per-iteration record made the two bugs visible
within minutes of inspecting the JSON.

#### Honest disclosures (about the eval itself)

- **N=3 is a smoke-test corpus, not a benchmark.** The
  `docs/repair-evaluation.md` §2 plan requires N≥1000 from a
  mathlib mutation pipeline (Section 2 phase 1 item 3) for
  statistically meaningful numbers. The 67 % is real but it
  describes *these three claims*, not "refine repair's repair
  rate."
- **counter-swap-lemma genuinely defeats Claude.** Even at
  `--max-iterations 5`. The break introduces cascading parse
  errors that Claude's per-iteration patches can clean up
  partially but never fully within a few iterations. A smarter
  strategy with cross-iteration memory (or richer context — e.g.
  the full file plus the LSP elaborator state) might do better.
  Not a bug; a real limitation.
- **The two bugs above were latent in `refineforge-cli` from
  the LLM repair-loop skeleton commit (`662cf3f`).** They didn't
  surface until a real strategy started producing patches.
  Skeletons with `MockStrategy` cannot exercise these paths —
  honesty win for the doctrine "the harness *is* the test of
  the framework, not just of the strategy."

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
