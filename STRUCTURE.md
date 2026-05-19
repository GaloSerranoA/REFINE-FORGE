# Structure

Every file in the repo, what owns it, and how the pieces connect.

If you only have time for two files, read [`README.md`](README.md)
and [`docs/methodology.md`](docs/methodology.md). For the engineering
discipline split (Lean Specialist / ML Engineer / DevOps) read
[`ARCHITECTURE.md`](ARCHITECTURE.md) and [`ROLES.md`](ROLES.md).

## Top-level layout

```
refineforge/
├── README.md                   # entry point — what / why / quick start
├── ARCHITECTURE.md             # three-section structure (Lean / ML / DevOps)
├── ROLES.md                    # short version of ARCHITECTURE — who owns what
├── CHANGELOG.md                # version history
├── SECURITY.md                 # vuln reporting + how to verify a signed bundle
├── STRUCTURE.md                # this file
├── Cargo.toml                  # Rust workspace manifest
├── flake.nix                   # Section 3: Nix flake (lean4-nix + crane + rust-overlay)
├── .gitignore
├── .github/
│   ├── CODEOWNERS              # path → section mapping (advisory until remote)
│   └── workflows/ci.yml        # Section 3: multi-arch CI + Sigstore signing
├── claims/                     # claim registry (one YAML per claim)
├── lean/                       # Lake project: formal models + theorems
├── crates/                     # Rust workspace members (9)
│   ├── refineforge-repair-api/ # Section 1: stable trait + types
│   ├── refineforge-cli/        # Section 1: the `refine` binary + driver
│   ├── refineforge-strategies/ # Section 2: pluggable strategies (+ real HTTP transport)
│   ├── refineforge-eval/       # Section 2: `refine-eval` benchmark harness
│   ├── refineforge-trainer/    # Section 2: `refine-train` orchestration CLI
│   ├── refineforge-bitexact/   # Section 4: `refine-bitexact` gate primitive
│   ├── refineforge-escalation/ # Cross-section: AI-to-human escalation engine (criteria v0.2)
│   ├── refineforge-derive/     # Section 1: #[derive(LeanModel)] proc-macro
│   └── example-counter/        # EXAMPLE-002 tutorial impl (uses LeanModel)
├── training/
│   ├── configs/                # example experiment + sweep YAMLs
│   ├── scripts/                # stub-trainer.sh/.ps1 for tests
│   ├── data/                   # training datasets (empty)
│   └── runs/                   # refine-train per-experiment runs (gitignored)
├── kernels/                    # Section 4: GPU kernels + bit-exact gates
│   ├── configs/                # per-kernel gate YAMLs
│   ├── scripts/                # stub-deterministic + stub-nondeterministic
│   ├── src/                    # actual .cu source (empty; CUDA engineer fills)
│   └── runs/                   # refine-bitexact per-gate reports (gitignored)
├── eval/
│   ├── corpus/                 # broken-proof entries + ground truth
│   └── runs/                   # refine-eval JSON outputs (gitignored)
├── templates/                  # scaffolding for `refine new`
├── artifacts/                  # exported verification bundles
├── containers/                 # Section 3: Dockerfile.verifier and friends
├── release/                    # Section 3: release.sh / release.ps1 + signed-tag artifacts
└── docs/                       # methodology, policies, refinement docs
```

## Lean side (`lean/`)

| Path | Owner | Purpose |
|---|---|---|
| `lean/lakefile.toml` | Lake | Declares the `Refineforge` library. `defaultTargets = ["Refineforge"]` is the key the scaffolder reads to auto-discover the library name. |
| `lean/lean-toolchain` | elan / Lake | Pins `leanprover/lean4:v4.29.1`. Bundle export captures this verbatim so a third-party verifier reproduces the same compiler. |
| `lean/Refineforge.lean` | you | Library root. Every module must be imported (transitively) from here. `refine new` appends an `import` line automatically when the new module lives under the library namespace. |
| `lean/Refineforge/Example.lean` | tutorial | EXAMPLE-001 — `add_comm_demo` (wraps `Nat.add_comm`). Zero Rust binding; demonstrates the Lean-only path. |
| `lean/Refineforge/Counter.lean` | tutorial | EXAMPLE-002 — `incr_monotone` + `incr_strictly_increases` on `Nat`. Refined by `crates/example-counter/`. |

## Claim registry (`claims/`)

A claim YAML is the **single source of truth** linking a Lean module
to a Rust crate. Schema (parsed by
[`crates/refineforge-cli/src/claim.rs`](crates/refineforge-cli/src/claim.rs)):

```yaml
claim_id: <PROJECT>-<AREA>-<NNN>
title: "..."
description: |
  ...
scope: model-only | model+refined | tutorial
status: unformalized | drafted | builds | proven | broken
authors: [...]
rust_source:               # optional; omit for Lean-only claims
  - path: crates/.../<file>.rs
    types: [...]
    functions: [...]
lean:                      # required
  toolchain: leanprover/lean4:v4.29.1
  module: <ModulePath>
  file: lean/.../<File>.lean
  theorems: [...]
policy:                    # all default to true
  no_sorry: true
  no_admit: true
  no_axioms_beyond_lean_core: true
review:                    # operator-filled at sign-off
  human_operator: null
  reviewed_on: null
  notes: null
```

| File | Claim |
|---|---|
| `claims/example.yaml` | EXAMPLE-001 (Lean-only tutorial) |
| `claims/example-counter.yaml` | EXAMPLE-002 (refined tutorial) |

## Rust workspace (`crates/`)

The workspace is declared in the top-level `Cargo.toml`. The crate
dependency graph (kept acyclic):

```
            refineforge-repair-api  ◄── the stable cross-section trait + types
                ▲                ▲                 ▲
                │                │                 │
   refineforge-cli ─────► refineforge-strategies   refineforge-eval
       │                          ▲                     │
       └──────────────────────────┘                     │
       (binary's strategy registry)                     │ runs repair against
                                                        │ corpus, captures outcomes
   refineforge-derive  (proc-macro; consumed by example-counter)
                          │
                          ▼
   example-counter  (refines lean/Refineforge/Counter.lean;
                     #[derive(LeanModel)] from refineforge-derive)
```

Members:

### `crates/refineforge-repair-api/` — stable trait surface (Section 1)

The cross-section API that prevents `refineforge-cli` and
`refineforge-strategies` from depending on each other. Contains:
`RepairStrategy` trait, `Patch`, `Diagnostic`, `Severity`, `Range`,
`Position`, `MockStrategy`, and the LSP conversions. Owned by
Section 1 because changing this surface affects every consumer.

### `crates/refineforge-derive/` — `#[derive(LeanModel)]` proc-macro (Section 1)

Single proc-macro: `LeanModel` derive that generates a `pub const
LEAN_MODEL: &'static str` containing the Lean structure declaration
equivalent to the Rust struct. Type mapping table in the crate's
module-level docs.

Supported field types: `u8/u16/u32/u64/usize` → `Nat`; `i8/i16/i32/i64/isize`
→ `Int`; `bool` → `Bool`; `String`/`&str` → `String`; `[u8; N]` →
`ByteArray`; `Vec<T>` → `List T`. Unsupported (generics, lifetimes,
nested structs, tuple/unit-variant enums) yield a `syn::Error`
pointing at the offending field — normal compile error with
file:line. Demo: `example-counter::Counter` has `LeanModel` derived
and the test
[`lean_model_matches_hand_written_counter_lean`](crates/example-counter/tests/counter.rs)
pins the generated string against the hand-written
`lean/Refineforge/Counter.lean`.

### `crates/refineforge-strategies/` — concrete strategies (Section 2)

Plugin implementations of `RepairStrategy`. Ships:

- `AnthropicStrategy<MockTransport>` — canned-response transport for
  unit tests and the `anthropic-mock` CLI strategy.
- `AnthropicStrategy<ReqwestTransport>` — **real HTTP** to
  `https://api.anthropic.com/v1/messages` with prompt caching
  (`cache_control: ephemeral` on system + file blocks), retry-with-
  exponential-backoff for 429 and 5xx, distinct error reporting
  for 4xx (auth / bad request / model not found / payload too large).
- `anthropic_strategy_from_env()` — factory wired into the CLI's
  `--strategy anthropic` path; reads `ANTHROPIC_API_KEY` and
  optional `ANTHROPIC_MODEL` (default `claude-opus-4-7`).

18 unit tests across the prompt-construction, response-parsing,
and transport layers (in-process `tiny_http` stub server for the
retry / header / error-mapping tests).

### `crates/refineforge-escalation/` — AI-to-human escalation engine (cross-section)

Pure-functional engine implementing the contract in
[`docs/escalation-criteria.md`](docs/escalation-criteria.md) v0.2.
Library only — no binary, no I/O inside `Engine::decide`, no
`unsafe`, no `tokio`, no network.

Public API:
- `Engine::decide(action: &Action, ctx: &ProjectContext) -> Result<Decision, EngineError>`
- `Decision::{Proceed, Escalate(EscalationReason)}`
- `EscalationReason { categories: Vec<Category>, primary, summary, evidence }`
- `Action` (~30 variants: Lean / refinement / claim YAML / external-fact
  / 8 trust-base sub-actions / scope additions / 5 bit-exact sub-actions
  / 3 trivially-OK actions / `Unknown` catch-all)
- `Category` (9 variants matching criteria-doc §3)
- `ProjectContext` (claim summary + sets of existing
  Mathlib imports / Lake packages / bundle-chain crates /
  approved Anthropic models / kernels with baselines / etc.)
- `CRITERIA_VERSION` constant (currently `"0.2"`); mismatch
  between this and `ctx.criteria_version` is a hard
  `EngineError`.

Modules: `category.rs` · `action.rs` · `decision.rs` · `context.rs` · `engine.rs`.

Tests: 117 total (25 inline + 92 integration files under
`tests/`, one per category + `multi_category.rs` + `edge_cases.rs`).
Every positive and negative example from criteria-doc §3 has a
named test.

**Phase 1 scope only.** File loaders (claim YAMLs / Cargo.lock /
lake-manifest.json → `ProjectContext`) are deferred to the
Phase 2 driver crate; this crate provides `test_default()`
constructors so tests + manual construction work today.

### `crates/refineforge-bitexact/` — bit-exact gate (Section 4)

Binary `refine-bitexact`. Runs a kernel N times, hashes each
output (SHA-256), fails the process if any hash disagrees. Does
NOT enforce determinism — only detects its absence. Custom
serde Deserialize for `OutputSource` accepts `output: stdout`
(bare scalar) and `output: {file: "..."}` (map) without YAML !tag
syntax.

Modules: `experiment` (KernelExperiment YAML + validation),
`hash` (streaming SHA-256 of bytes / files; `all_equal`),
`runner` (subprocess N times + env vars + per-run timing),
`report` (Pass/Fail outcome + unique-hash count + summary).

23 unit tests + 3 POSIX-only e2e tests using the shipped stub
scripts (deterministic → Pass; non-deterministic → Fail;
dry-run → no execution).

### `crates/refineforge-trainer/` — training orchestration (Section 2)

Binary `refine-train`. Wraps any training backend (axolotl,
HuggingFace Trainer, custom script) with run tracking, checkpoint
resume, retry-with-backoff failure recovery, and JSON training
reports. **Does NOT perform training itself** — the backend does.

Subcommands: `run <exp.yaml>` (+ `--dry-run`), `sweep <sweep.yaml>`
(cartesian or random:N), `monitor <run_dir>` (tail
`progress.jsonl`), `report <run_dir>` (build/refresh `report.json`),
`checkpoints <run_dir>` (list).

Modules: `experiment` (YAML schema), `runner` (subprocess +
log capture + per-line progress parsing), `progress` (HF /
axolotl / generic parsers), `checkpoint` (find latest / prune
old), `sweep` (cartesian + deterministic random sample),
`failure` (OOM / Interrupt / Network / BackendError / Unknown
classifier + recovery action chooser), `report` (final JSON
with metric summary stats + checkpoint manifest + failure
timeline).

35 unit tests + 2 POSIX-only end-to-end tests using a stub
trainer script. Stub trainer lives in
[`training/scripts/stub-trainer.sh`](training/scripts/stub-trainer.sh)
(POSIX) and `.ps1` (PowerShell).

### `crates/refineforge-eval/` — evaluation harness (Section 2)

Binary `refine-eval`. Drives `refine repair` against a JSONL
corpus of broken-Lean entries, captures per-entry outcomes +
latencies, emits a JSON report with summary statistics
(repair rate, median latency, p95 latency).

Ships a 3-entry tutorial corpus at [`eval/corpus/example.jsonl`](eval/corpus/example.jsonl)
exercising 3 mutations of EXAMPLE-002 (Counter):
`swap_lemma`, `wrong_tactic`, `rename_field`.

Architectural note: the runner pre-warms the temp project's
`.lake/` cache via `lake build` on the unmodified source before
swapping in the broken file. Without pre-warm, cold lake
elaboration exceeds the LSP diagnostic timeout and breaks
register as false `AlreadyClean`.

### `crates/refineforge-cli/` — the `refine` binary

```
crates/refineforge-cli/
├── Cargo.toml              # name = refineforge-cli; binary = refine
└── src/
    ├── main.rs             # clap entry point; dispatches to modules
    ├── claim.rs            # YAML schema + loader (Claim, RustSource, LeanInfo, Policy)
    ├── runner.rs           # `refine lean check[-all]` — policy gate → lake build → ProofReport
    ├── sorry_gate.rs       # comment-stripper + word-boundary scan for sorry/admit/axiom; unit-tested
    ├── report.rs           # ProofReport + ProofStatus enum (Verified / BuildFailed / PolicyViolation / ToolingError)
    ├── bundle.rs           # `refine bundle export/verify` — SHA-256 manifest + report.json + VERIFY.txt; cross-platform paths
    ├── scaffold.rs         # `refine new` + `refine templates` — template substitution + auto-import; reads lakefile defaultTargets
    ├── scan.rs             # `refine scan check[-all]` — regex name-presence check for rust_source entities
    └── repair/             # `refine repair` — LLM repair driver
        ├── mod.rs          # public API, RepairConfig, RepairReport, driver loop (with post-loop final diagnostic check)
        ├── lsp.rs          # LeanLspClient: spawn lake env lean --server, JSON-RPC framing, reader thread
        ├── diagnostic.rs   # re-exports from refineforge-repair-api
        └── strategy.rs     # re-exports from refineforge-repair-api (Patch::apply lives there with line-length-clamping fix)
```

| Module | Lines | Tests |
|---|---:|---:|
| `main.rs` | ~150 | — |
| `claim.rs` | ~150 | — |
| `runner.rs` | ~140 | — |
| `sorry_gate.rs` | ~180 | 7 |
| `report.rs` | ~30 | — |
| `bundle.rs` | ~230 | — |
| `scaffold.rs` | ~210 | — |
| `scan.rs` | ~225 | — |
| `repair/mod.rs` | ~210 | — |
| `repair/lsp.rs` | ~270 | 3 |
| `repair/diagnostic.rs` | ~115 | 3 |
| `repair/strategy.rs` | ~200 | 6 |

(Line counts approximate; check `wc -l` for current values.)

### `crates/example-counter/` — EXAMPLE-002 Rust side

```
crates/example-counter/
├── Cargo.toml
├── src/
│   ├── lib.rs              # re-exports
│   └── counter.rs          # Counter struct + incr + checked_incr
└── tests/
    └── counter.rs          # 7 refinement tests, each docstring cites a Lean theorem
```

Refines `Refineforge.Counter`. The refinement argument lives in
[`docs/refinement/EXAMPLE-002.md`](docs/refinement/EXAMPLE-002.md).

## Scaffolding templates (`templates/`)

Each template is a directory with two files:
- `lean.lean.tmpl` — Lean source with `{{CLAIM_ID}}`, `{{MODULE}}`,
  `{{LEAN_FILE}}`, `{{TITLE}}` placeholders
- `claim.yaml.tmpl` — YAML with the same placeholders

| Template | Shape |
|---|---|
| `append_chain/` | append-only linked sequence with hash check (3 theorems: `empty_valid`, `append_preserves_validity`, `tipHash_after_append`) |
| `capability/` | set-membership authorization (3 theorems: `subsumes_preserves_authorization`, `subsumes_refl`, `subsumes_trans`) |
| `capability_with_revocation/` | capability + monotone `revoke` (3 theorems: `revoked_authorizes_nothing`, `fresh_capability_authorizes_held_right`, `revoke_is_idempotent`) |
| `linear_types/` | single-use token with `consumed : Bool` flag (3 theorems: `fresh_token_is_valid`, `consume_invalidates`, `consume_sets_consumed`) |
| `state_machine/` | allowed-transitions predicate over an enumerated state space |

`refine new --template <name> --module <ModulePath> <CLAIM-ID>`
generates `lean/<ModulePath>.lean` + `claims/<slug>.yaml` and appends
the import to the library root.

## Artifacts (`artifacts/`)

`refine bundle export` writes per-claim bundles here:

```
artifacts/<CLAIM-ID>/
├── manifest.json           # SHA-256 of every file + bundle schema + toolchain pin
├── report.json             # ProofReport (status + counts + stdout/stderr)
├── VERIFY.txt              # human-readable re-verification instructions
├── claims__<file>.yaml     # flattened copy of the claim YAML
├── lean__<lib>.lean        # flattened copy of every Lean file under lean/
├── lean__<lib>__<mod>.lean # ...
├── lean__lakefile.toml
├── lean__lean-toolchain
└── docs__refinement__<CLAIM-ID>.md  # if a refinement doc exists
```

Paths use `__` flattening so the bundle stays a single flat directory.
Manifest keys use forward-slash (cross-platform; the Windows bug that
the bundle exporter once had is documented in `CHANGELOG.md`).

## Docs (`docs/`)

| File | Audience | What you'll learn |
|---|---|---|
| `methodology.md` | funders, reviewers | What refineforge claims and what it does NOT claim. The four-link trust chain. |
| `no-sorry-policy.md` | maintainers | Exactly what the policy gate catches, what it misses, when overrides are legitimate |
| `refinement-template.md` | claim authors | Empty skeleton to copy into `docs/refinement/<CLAIM-ID>.md` |
| `refinement/EXAMPLE-002.md` | claim authors | Answer-key showing a filled-in refinement argument with a real idealisation |
| `llm-repair-design.md` | ML engineer (Section 2) | Architecture of the repair loop + four-step swap-in recipe for a real LLM strategy |
| `repair-evaluation.md` | ML engineer (Section 2) | Benchmark methodology, mutation taxonomy, training/eval separation rules |
| `security.md` | DevOps (Section 3) | Threat model, supply chain, **shipped** signing chain, vuln reporting |
| `reproducible-build.md` | DevOps (Section 3) | Bit-identical-rebuild methodology, Nix flake (**authored — first-build pending**), verification protocol |
| `HELYX-CASE-STUDY.md` | adopters | Pointer to the external worked example (`helyx-proofforge`) |

## How the pieces connect

```
┌──────────────┐     ┌────────────────┐     ┌─────────────────┐
│ claims/*.yaml│────▶│ refineforge-cli│────▶│ lean/ + Lake    │
│ (the spec)   │     │ (the driver)   │     │ (the oracle)    │
└──────┬───────┘     └────────┬───────┘     └─────────────────┘
       │                      │
       │ rust_source          │ runs                ┌─────────────────┐
       │ block                ├────────────────────▶│ artifacts/      │
       ▼                      │                     │ (sealed output) │
┌──────────────┐              │                     └─────────────────┘
│ crates/<x>/  │◀─────────────┘
│ (the refined│   scan: name-presence
│  Rust impl) │   repair: LSP-driven patch loop
└─────┬────────┘
      │ refined by
      ▼
┌──────────────────────────┐
│ docs/refinement/         │
│ <CLAIM-ID>.md            │
│ (the trust-critical      │
│  bridge — human-written, │
│  human-reviewed)         │
└──────────────────────────┘
```

The CLI is the orchestrator. Lean is the oracle. The refinement doc
is the trust-critical artifact. The bundle exporter freezes all three
into a hash-verifiable archive a third party can re-check.

## Update this file when

You add a new top-level directory, a new CLI subcommand, a new
template, a new Rust workspace member, or a new docs file. Stale
structure docs are worse than no structure doc — they lie.
