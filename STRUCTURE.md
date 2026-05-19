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
│   ├── refineforge-escalation/ # Cross-section: AI-to-human escalation engine (criteria v0.3)
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
├── artifacts/                  # exported verification bundles (committed for EXAMPLE-001)
├── escalations/                # `refine autonomous` decision packets, one dir per CLAIM-ID
├── autonomous/                 # `refine autonomous` per-run RunReport JSONs (gitignored)
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
[`docs/escalation-criteria.md`](docs/escalation-criteria.md) **v0.3**.
Library only — no binary, no I/O inside `Engine::decide`, no
`unsafe`, no `tokio`, no network.

Public API:
- `Engine::decide(action: &Action, ctx: &ProjectContext) -> Result<Decision, EngineError>`
- `Decision::{Proceed, Escalate(EscalationReason)}`
- `EscalationReason { categories: Vec<Category>, primary, summary, evidence }`
- `Action` (~30 variants: Lean / refinement / claim YAML / external-fact
  / 8 trust-base sub-actions / scope additions / 5 bit-exact sub-actions
  / 3 trivially-OK actions / `Unknown` catch-all)
- `Category` (9 variants matching criteria-doc §3 + Cat 9 bit-exact)
- `ProjectContext` (claim summary + sets of existing Mathlib
  imports / Lake packages / bundle-chain crates / approved
  Anthropic models / kernels with baselines / etc.)
- `Packet` markdown renderer + `BatchBlock` for v0.3-conformant
  batched packets + per-Evidence section dispatch
- `DecisionOutcome::{Approved, Rejected, EditAndResubmit,
  Partial(PartialDecision)}` + `parse_decision(markdown)` that
  walks the `## Human decision` section; partial form recognises
  `APPROVED: 1-5,7; REJECTED: 6,8 [reason]`
- `GitOps` trait + `SubprocessGitOps` (production, shells to
  `git`) + `MockGitOps` (in-memory, unit-test;
  `auto_approve_packets(reason)` test mode rewrites `(pending)`
  → `APPROVED: <reason>`) + `commit_packet` +
  `poll_decision_once` + indefinite `await_decision` (no
  auto-reject per v0.3)
- File loaders: `load_claim_summary` / `load_lake_manifest_packages`
  / `load_cargo_lock_bundle_chain` / `load_project_context`
  (Phase 3.5 — replaces the Phase-1 honest deferral)
- `CRITERIA_VERSION` constant (currently `"0.3"`); mismatch
  between this and `ctx.criteria_version` is a hard
  `EngineError`

Source modules: `category.rs` · `action.rs` · `decision.rs` ·
`context.rs` · `engine.rs` · `packet.rs` · `decision_outcome.rs`
· `git_checkpoint.rs` · `loaders.rs`.

Tests: **170** per `cargo nextest list` (Phase 1 category
coverage + Phase 2 packet/decision/git + Phase 3.5 loaders).
Inline tests in each src module + integration tests under
`tests/` (one file per category + `multi_category.rs` +
`edge_cases.rs` + POSIX-only `packet_e2e.rs`). Every positive
and negative example from criteria-doc §3 has a named test.

Phase 1 (engine), Phase 2 (packet + decision parser + git
checkpoint), and Phase 3.5 (file loaders) have all landed; the
driver lives in `crates/refineforge-cli/src/autonomous/` (next
section).

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

### `crates/refineforge-cli/src/autonomous/` — `refine autonomous` driver (Phase 3-3.7)

Lives inside `refineforge-cli` (not a separate crate) to avoid
a circular dep with the existing `runner` / `bundle` / `scan`
modules. Provides `refine autonomous <CLAIM-ID>` + `refine
escalations list`.

Submodules:
- `planner.rs` — sequences the baseline workflow (LeanCheck →
  Scan → BundleExport) + builder injection points:
  `Planner::with_engine_action(Action)`,
  `with_training_step(path)`, `with_bitexact_step(path)`.
  StepKinds: `LeanCheck`, `Scan`, `BundleExport`,
  `EngineAction(Action)`, `Repair { strategy, max_iterations }`,
  `RunTrainingExperiment { config_path }`,
  `RunBitExactGate { config_path }`.
- `executor.rs` — runs each step. System steps call real
  `runner::run` / `scan::scan_claim` / `bundle::export` library
  functions when not in `--dry-run` (Phase 3.5). `Repair` step
  calls `crate::repair::repair` with `resolve_strategy(name)`
  (`mock` / `anthropic-mock` / `anthropic`); cost-gate charges
  $0.07 × max_iterations upfront for `anthropic` (Phase 3.6).
  Training / bit-exact steps subprocess-shell to `refine-train`
  / `refine-bitexact` (binary path overridable via
  `REFINEFORGE_REFINE_TRAIN_BIN` / `REFINEFORGE_REFINE_BITEXACT_BIN`,
  Phase 3.7). Repair step reads `Arc<Mutex<UsageStats>>` after
  the strategy is consumed and surfaces token counts + per-call
  `stop_reasons` on the `Executor.anthropic_usage_observed`
  field (Phase 3.7 + 3.8). Engine actions go through
  `Engine::decide`; escalations commit a packet unless
  `--dry-run` (override: `commit_packets_in_dry_run = true`
  for tests). **Phase 3.8 cross-run preserve**: the Escalated
  branch reads the packet file BEFORE committing; if a parsable
  operator decision already exists, the executor preserves it
  instead of overwriting — APPROVED state survives across
  `refine autonomous` re-runs.
- `cost.rs` — `CostGate { max_usd, spent_usd }` with
  fail-closed `charge(amount)`. Failed charges DO NOT debit
  the gate.
- `report.rs` — `RunReport` JSON: per-step outcomes + cost +
  summary + optional `anthropic_usage: UsageStats`. **No USD
  conversion** of token counts (Anthropic pricing drifts;
  documented design decision).
- `mod.rs` — `run_cli` is the top-level entry: loads `Claim`
  via `crate::claim::load`, loads `ProjectContext` via
  `refineforge_escalation::load_project_context`, constructs
  `Executor`, calls `run_worklist`. `escalations_list` is the
  queue-dashboard implementation. **`run_worklist<G: GitOps>(
  ex, plan, cfg: &WorkRunConfig)` is generic over the git
  backend** so tests can drive with `MockGitOps`.
  `WorkRunConfig` carries `auto_repair`, `await_decisions`,
  repair/poll knobs; handles all four `DecisionOutcome`
  variants after Escalated when `--await-decisions` is set.

CLI flags on `refine autonomous` (current as of Phase 3.8):
`--strategy`, `--max-cost-usd`, `--operator`, `--dry-run`,
`--auto-repair`, `--await-decisions`,
`--inject-counter-idealisation`, `--inject-training <PATH>`
(repeatable), `--inject-bitexact <PATH>` (repeatable).

Tests: **62 total** (~28 inline across submodules + 5
integration tests in `tests/autonomous_e2e.rs`):
- `loader_parses_real_example_001_yaml`,
  `loader_parses_real_example_002_yaml` — real-repo claim load.
- `dry_run_plans_and_loads_real_claim` — full dry-run pipeline.
- `live_lean_check_on_example_001` — gated on `lake` on PATH;
  result varies by shell environment (PowerShell PATH on the
  v0.2.0 commit machine had `lake`; Bash PATH did not).
- `example_002_counter_idealisation_dogfood_with_await_approval`
  — Plan §3 phase 4 acceptance test in mock-LLM form: exactly
  one Cat 2 escalation → simulated APPROVED via
  `MockGitOps::auto_approve_packets` → Scan + BundleExport
  resume → success. Live-LLM equivalent was exercised in the
  Phase 4 audit ($0.35 spend; see CHANGELOG).
- Phase 3.8 cross-run preserve tests:
  `phase_3_8_preexisting_approved_packet_is_not_overwritten`
  and `phase_3_8_preexisting_pending_packet_is_still_rewritten`.

**Live shipped end-to-end** against real Anthropic API:
- Phase 3.6 ([60d2a81](#)): broken `rfl` proof of `a + b =
  b + a` repaired in 4 LLM iterations (23.3s, $0.35 spend).
- Phase 4 audit ([92da3cf](#)): formal acceptance gate — broken
  proof → live LLM Repair (1 iteration, 5.5s, 781+90 tokens) →
  Cat 2 escalation → operator approval → SHA-256-sealed bundle
  shipped. Total spend $0.35.

Honest leftovers (smallest remaining):
- Per-call USD conversion intentionally absent (operator-side
  reconciliation against Anthropic invoices).
- Nix flake first-build verification: needs a Nix-capable
  runner; `docs/reproducible-build.md` §8 has the operator
  invocation.

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
| `bit-exact-reproducibility.md` | CUDA engineer (Section 4) | Sources of CUDA non-determinism + per-source mitigations + gate primitive |
| `escalation-criteria.md` | **all sections** | **CONTRACT v0.3** — 9 categories that always escalate to the human during `refine autonomous` runs. Operator-signed |
| `plans/autonomous-driver-plan.md` | maintainers, operators | 5-phase enterprise build plan for `refine autonomous`. All plan phases + 3.5-3.8 shipped; Phase 4 acceptance-gate exercised against real Anthropic ($0.35 spend); v0.2.0 + v0.2.1 released |
| `plans/gui-plan.md` | maintainers, operators | 9-phase enterprise plan for `refineforge-studio` production GUI (Tauri 2.x + Solid). PLAN ONLY — no code; 6 open questions to resolve before Phase 0 |
| `plans/resourcing-plan.md` | maintainers, operators | People (4 specialists) + compute (16,000 GPU-hours) + tools + libraries + 3 funding options (A grants / B customer-funded / C cash) with concrete numbers. 12-month budget: ~$520k LATAM mid-band / ~$1.13M US ceiling |
| `HELYX-CASE-STUDY.md` | adopters | Pointer to the external worked example (`helyx-proofforge`) |

## Workspace test counts (current)

`cargo nextest run --workspace` → **383/383 pass**. Per-crate
breakdown via `cargo nextest list --workspace` (each crate's
lib + bin targets counted separately per nextest convention):

| Crate | Tests | Δ since v0.2.0 tag |
|---|---:|---:|
| `refineforge-escalation` | 170 | — |
| `refineforge-trainer` | 74 | — |
| `refineforge-cli` | 62 | +2 (Phase 3.8 cross-run preserve) |
| `refineforge-bitexact` | 32 | — |
| `refineforge-strategies` | 21 | +3 (stop_reason) |
| `refineforge-repair-api` | 11 | — |
| `example-counter` | 9 | — |
| `refineforge-eval` | 4 | — |
| `refineforge-derive` | (proc-macro, consumed via example-counter) | — |

v0.2.0 tag is at commit `6486c6a` (378 tests). Phase 3.8 +
Phase 4 audit landed post-tag under `[Unreleased]` — see
CHANGELOG.

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
