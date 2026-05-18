# Structure

Every file in the repo, what owns it, and how the pieces connect.

If you only have time for two files, read [`README.md`](README.md)
and [`docs/methodology.md`](docs/methodology.md).

## Top-level layout

```
refineforge/
├── README.md                   # entry point — what / why / quick start
├── CHANGELOG.md                # version history
├── STRUCTURE.md                # this file
├── Cargo.toml                  # Rust workspace manifest
├── .gitignore
├── .github/workflows/ci.yml    # CI: lake build + cargo build + verify claims
├── claims/                     # claim registry (one YAML per claim)
├── lean/                       # Lake project: formal models + theorems
├── crates/                     # Rust workspace members
├── templates/                  # scaffolding for `refine new`
├── artifacts/                  # exported verification bundles
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

The workspace is declared in the top-level `Cargo.toml`. Members:

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
    └── repair/             # `refine repair` — LLM repair loop SKELETON
        ├── mod.rs          # public API, RepairConfig, RepairReport, driver loop
        ├── lsp.rs          # LeanLspClient: spawn lake env lean --server, JSON-RPC framing, reader thread
        ├── diagnostic.rs   # Diagnostic / Severity / Range types + LSP-types conversions
        └── strategy.rs     # RepairStrategy trait + MockStrategy + Patch::apply
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
| `llm-repair-design.md` | next-session devs | Architecture of the repair loop + four-step swap-in recipe for a real LLM strategy |
| `HELYX-CASE-STUDY.md` | adopters | Pointer to the external worked example (`helyx-proofforge`) — same pattern at production scale |

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
