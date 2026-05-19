# refineforge

Autor: Galo Serrano Abad
NANTAR AI ROOBOTICS

A Lean 4 proof engineering + refinement-bundle framework for trust-critical Rust.

> **Doctrine:** LLM may propose. Lean must verify. Human operator must approve.

## Documentation map

Read in this order — each doc is short and points at the next.

| Doc | What it covers |
|---|---|
| [README.md](README.md) (this file) | What refineforge is, how to install, the CLI surface |
| [ARCHITECTURE.md](ARCHITECTURE.md) | Three-section structure: Lean Specialist + ML Engineer + DevOps, with interfaces and sequencing |
| [ROLES.md](ROLES.md) | Short version of who owns what; map a task to a role |
| [STRUCTURE.md](STRUCTURE.md) | Every file in the repo, what owns it, how the pieces connect |
| [CHANGELOG.md](CHANGELOG.md) | Version history, what shipped in each release |
| [SECURITY.md](SECURITY.md) | How to report a vulnerability + how to verify a release signature |
| [docs/methodology.md](docs/methodology.md) | The honest framing: what refineforge claims, what it does NOT claim |
| [docs/no-sorry-policy.md](docs/no-sorry-policy.md) | What the policy gate catches and what it does not |
| [docs/refinement-template.md](docs/refinement-template.md) | Empty template for writing your own refinement-argument doc |
| [docs/refinement/EXAMPLE-002.md](docs/refinement/EXAMPLE-002.md) | A filled-in refinement doc you can read as the answer key |
| [docs/llm-repair-design.md](docs/llm-repair-design.md) | Architecture of the LLM repair loop + how to swap in a real strategy |
| [docs/repair-evaluation.md](docs/repair-evaluation.md) | How we'll measure whether `refine repair` is any good — corpus design, mutation taxonomy, statistical reporting |
| [docs/security.md](docs/security.md) | Threat model, supply chain, signing chain (shipped), vuln reporting |
| [docs/reproducible-build.md](docs/reproducible-build.md) | Bit-identical-rebuild methodology — Nix flake (authored), verification protocol |
| [docs/bit-exact-reproducibility.md](docs/bit-exact-reproducibility.md) | GPU kernel bit-exact reproducibility: non-determinism sources + mitigations + gate primitive |
| [docs/escalation-criteria.md](docs/escalation-criteria.md) | **CONTRACT v0.3** (operator-signed; v0.2 superseded same-day with Q1/Q3/Q4 revisions). 9 categories that always escalate to the human during `refine autonomous` runs. Enforced by `crates/refineforge-escalation` |
| [docs/autonomous-driver-plan.md](docs/autonomous-driver-plan.md) | Enterprise build plan for `refine autonomous`: 5 phases, ~2 weeks, $50-150 API budget, risks + mitigations. **Phase 1 shipped** (the escalation engine); Phases 2-5 pending |
| [docs/HELYX-CASE-STUDY.md](docs/HELYX-CASE-STUDY.md) | Pointer to the external worked example (helyx-proofforge) |

## What this is

refineforge is a **project template + CLI** for teams that want
formally-verified mathematical models linked to a running Rust
codebase. You write a Lean model, prove your theorems, write a
human-reviewed refinement argument bridging the model to the Rust
code, and refineforge bundles everything into a SHA-256-sealed
artifact a third party can independently re-verify.

Fork this repo, replace the example claim with your own, and you have
a verification pipeline.

## What this is *not*

It is **not** "AI that proves your code automatically." It does not
verify the Rust binary, the Rust compiler, the OS, or the hardware.
It proves properties of **mathematical models** of trust-critical
behaviour and links those models to specific Rust source files via
**a refinement argument that a human operator writes and reviews**.

The refinement argument — not the proof — is the trust-critical
artifact. See [`docs/methodology.md`](docs/methodology.md).

## Tutorials shipped in this repo

Two tutorial claims ship pre-wired so `refine lean check-all` works
the moment you `cargo build --release`:

| Claim | What it demonstrates | Files |
|-------|----------------------|-------|
| **EXAMPLE-001** | The minimum path: Lean theorem → policy gate → bundle. No Rust. | [`lean/Refineforge/Example.lean`](lean/Refineforge/Example.lean), [`claims/example.yaml`](claims/example.yaml) |
| **EXAMPLE-002** | The full refinement pattern: Lean theorem + Rust crate + refinement-argument doc. `refine scan` reports `Verified`. | [`lean/Refineforge/Counter.lean`](lean/Refineforge/Counter.lean), [`crates/example-counter/`](crates/example-counter), [`claims/example-counter.yaml`](claims/example-counter.yaml), [`docs/refinement/EXAMPLE-002.md`](docs/refinement/EXAMPLE-002.md) |

EXAMPLE-002 deliberately includes a real Lean-vs-Rust idealisation
(unbounded `Nat` vs saturating `u64`) so the refinement doc has
something non-trivial to argue. Read it as the answer-key for what
[`docs/refinement-template.md`](docs/refinement-template.md) asks
you to write for your own claims.

## External worked example

The HELYX trust-claim project (separate repo `helyx-proofforge` —
see [`docs/HELYX-CASE-STUDY.md`](docs/HELYX-CASE-STUDY.md)) is the
production-shape consumer of this framework. It demonstrates the
same pattern as EXAMPLE-002 with two real claims (append-only
audit chain + capability subsumption) and walks through the
`docs/refinement/` doc structure at full scale.

## Repository layout

```
refineforge/
├── README.md / ARCHITECTURE.md / ROLES.md / STRUCTURE.md
├── CHANGELOG.md / SECURITY.md
├── Cargo.toml                  # Rust workspace manifest
├── flake.nix                   # Nix flake (lean4-nix + crane + rust-overlay)
├── .github/
│   ├── CODEOWNERS              # path → section ownership
│   └── workflows/ci.yml        # multi-arch CI + Sigstore signing
├── lean/                       # Lake project — your formal models
│   ├── lakefile.toml
│   ├── lean-toolchain          # pinned: leanprover/lean4:v4.29.1
│   ├── Refineforge.lean        # library root
│   └── Refineforge/
│       ├── Example.lean        # EXAMPLE-001: Lean-only hello world
│       └── Counter.lean        # EXAMPLE-002: refined tutorial (Lean side)
├── claims/                     # claim registry (YAML, one per claim)
│   ├── example.yaml            # EXAMPLE-001
│   └── example-counter.yaml    # EXAMPLE-002
├── crates/                     # Rust workspace (9 members)
│   ├── refineforge-repair-api/ # stable trait + types (Section 1)
│   ├── refineforge-cli/        # `refine` binary + driver (Section 1)
│   ├── refineforge-derive/     # #[derive(LeanModel)] proc-macro (Section 1)
│   ├── refineforge-strategies/ # AnthropicStrategy + ReqwestTransport (Section 2)
│   ├── refineforge-eval/       # `refine-eval` benchmark harness (Section 2)
│   ├── refineforge-trainer/    # `refine-train` orchestration CLI (Section 2)
│   ├── refineforge-bitexact/   # `refine-bitexact` gate primitive (Section 4)
│   ├── refineforge-escalation/ # AI-to-human escalation engine (cross-section)
│   └── example-counter/        # EXAMPLE-002 Rust side
├── templates/                  # scaffolding for `refine new`
│   ├── append_chain/           # append-only linked chain with hash check
│   ├── capability/             # capability-based authorization
│   ├── capability_with_revocation/ # capability + monotone revocation
│   ├── linear_types/           # single-use token (consume-once)
│   └── state_machine/          # state-machine transitions
├── eval/
│   ├── corpus/                 # broken-proof corpus for refine-eval
│   └── runs/                   # refine-eval JSON outputs (gitignored)
├── training/                   # Section 2: training experiments
│   ├── configs/                # example experiment + sweep YAMLs
│   ├── scripts/                # stub-trainer for tests; backend shims
│   ├── data/                   # training datasets (empty; mathlib mut. pipeline pending)
│   └── runs/                   # refine-train per-experiment output (gitignored)
├── kernels/                    # Section 4: GPU kernels + bit-exact gates
│   ├── configs/                # per-kernel gate YAMLs
│   ├── scripts/                # stub-deterministic + stub-nondeterministic (sh + ps1)
│   ├── src/                    # actual .cu source (empty; CUDA engineer fills)
│   └── runs/                   # refine-bitexact per-gate output (gitignored)
├── artifacts/                  # exported verification bundles
├── containers/
│   └── Dockerfile.verifier     # elan + Lean preinstalled for reviewers
├── release/
│   ├── release.sh              # POSIX release script
│   └── release.ps1             # PowerShell release script
└── docs/
    ├── methodology.md          # how refineforge thinks about trust
    ├── no-sorry-policy.md      # what the policy gate enforces
    ├── refinement-template.md  # empty refinement-doc skeleton
    ├── refinement/EXAMPLE-002.md  # filled-in answer key
    ├── llm-repair-design.md    # repair-loop architecture
    ├── repair-evaluation.md    # benchmark methodology
    ├── security.md             # threat model + signing chain
    ├── reproducible-build.md   # Nix flake + bit-identical methodology
    └── HELYX-CASE-STUDY.md     # link to external worked example
```

## Quick start

```bash
# 1. Install elan (Lean toolchain manager) — one time
curl -sSf https://raw.githubusercontent.com/leanprover/elan/master/elan-init.sh | sh

# 2. Build the CLI
cargo build --release

# 3. Build the Lean library (downloads pinned Lean 4.29.1 on first run)
(cd lean && lake build)

# 4. List claims
./target/release/refine claims list

# 5. Verify every claim end-to-end (policy gate + lake build)
./target/release/refine lean check-all

# 6. Export an independently verifiable bundle
./target/release/refine bundle export EXAMPLE-001

# 7. Re-verify the bundle (re-hashes everything; does not re-run Lean)
./target/release/refine bundle verify artifacts/EXAMPLE-001

# 8. Scaffold your first real claim from a template
./target/release/refine templates
./target/release/refine new \
    --template state_machine \
    --module Refineforge.OrderState \
    --title "Order state machine respects allowed transitions" \
    MYPROJ-STATE-001
```

## Subcommand reference

| Command                                | What it does                                                            |
|----------------------------------------|-------------------------------------------------------------------------|
| `refine claims list`                   | List every claim in `claims/`                                           |
| `refine claims show <id>`              | Print a claim's YAML                                                    |
| `refine lean check <id>`               | Verify one claim (policy gate + `lake build`)                           |
| `refine lean check-all`                | Verify every claim                                                      |
| `refine scan check <id>`               | Confirm a claim's `rust_source` entities exist in the cited Rust file   |
| `refine scan check-all`                | Same, for every claim                                                   |
| `refine bundle export <id>`            | Bundle the sources + manifest + report                                  |
| `refine bundle verify <bundle-dir>`    | Re-hash every file in a bundle and confirm the manifest matches         |
| `refine bundle verify <bundle-dir> --verify-signature` | Hashes + Sigstore signature (via cosign). See [SECURITY.md](SECURITY.md) |
| `refine repair <id>`                   | Bounded LLM repair loop against Lean's LSP server. Strategies: `mock` (declines all), `anthropic-mock` (canned), `anthropic` (real HTTP, needs `ANTHROPIC_API_KEY`). See [`docs/llm-repair-design.md`](docs/llm-repair-design.md) |
| `refine-eval --corpus … --strategy …`  | Drive `refine repair` against a JSONL corpus; emit JSON report. See [`docs/repair-evaluation.md`](docs/repair-evaluation.md) |
| `refine-train run <exp.yaml>`          | Run one training experiment (axolotl / HF Trainer / custom backend). See [`training/README.md`](training/README.md). Always start with `--dry-run`. |
| `refine-train sweep <sweep.yaml>`      | Grid or random hyperparameter sweep |
| `refine-train monitor <run_dir>`       | Tail `progress.jsonl` and show latest metrics |
| `refine-train report <run_dir>`        | Build / refresh `report.json` for a run |
| `refine-bitexact run <kernel.yaml>`    | Bit-exact reproducibility gate: run kernel N times, fail if SHA-256 hashes disagree. See [`kernels/README.md`](kernels/README.md) and [`docs/bit-exact-reproducibility.md`](docs/bit-exact-reproducibility.md) |
| `refine templates`                     | List scaffolding templates                                              |
| `refine new --template <t> --module <M> <ID>` | Scaffold a new claim from a template                             |

## Status enum (CLI output)

| status              | meaning                                                          |
|---------------------|------------------------------------------------------------------|
| `verified`          | policy gate passed AND `lake build` succeeded                    |
| `build_failed`      | Lean rejected the source                                         |
| `policy_violation`  | `sorry` / `admit` / non-core `axiom` found; build was not run    |
| `tooling_error`     | `lake` not installed or filesystem error                         |

Scan additionally reports `Verified` / `Partial` / `FileMissing` / `NoRustSource`.

## Customising for your project

1. Rename `lean/Refineforge.lean` → `lean/<YourLib>.lean` and the
   directory `lean/Refineforge/` → `lean/<YourLib>/`.
2. Update `lean/lakefile.toml`: change `defaultTargets` and
   `[[lean_lib]] name`. The scaffolder auto-detects the new library
   name from `defaultTargets`, so `refine new` keeps working.
3. Delete `claims/example.yaml` and `lean/<YourLib>/Example.lean`.
4. Use `refine new` to scaffold your real claims.

## Framework build plan

Where each thing currently lives:

| Component                                  | Status              |
|--------------------------------------------|---------------------|
| Lean runner CLI                            | ✅ implemented      |
| No-sorry policy gate                       | ✅ implemented      |
| Claim registry (YAML schema)               | ✅ implemented      |
| Verification bundle exporter + verifier    | ✅ implemented      |
| Proof template generator (`refine new`)    | ✅ implemented      |
| Rust source scan (name-presence check)     | ✅ implemented      |
| Refinement-argument template               | ✅ `docs/refinement-template.md` |
| LLM repair loop (LSP client)               | ✅ shipped: `mock`, `anthropic-mock`, **`anthropic`** (real HTTP with retry + prompt caching) |
| `refineforge-strategies` workspace member  | ✅ `AnthropicStrategy` + `ReqwestTransport` (real HTTP, retry-with-backoff, error mapping; 18 unit tests) |
| `refineforge-eval` (`refine-eval` binary)  | ✅ corpus-driven evaluation harness with JSON output; ships a 3-entry tutorial corpus under [`eval/corpus/`](eval/corpus) |
| `refineforge-trainer` (`refine-train` binary) | ✅ training-experiment orchestration (axolotl / HF Trainer / custom); run tracking, checkpoint resume, failure recovery, JSON reports. Does NOT perform training itself — backend does. See [`training/README.md`](training/README.md) |
| `refineforge-bitexact` (`refine-bitexact` binary) | ✅ bit-exact reproducibility gate: runs kernel N times, hashes outputs, fails if any disagree. Stub scripts prove the gate catches non-determinism. Real CUDA kernels are the CUDA engineer's domain. See [`kernels/README.md`](kernels/README.md). |
| `refineforge-escalation` (library) | ✅ Phases 1+2 of [`docs/autonomous-driver-plan.md`](docs/autonomous-driver-plan.md) under criteria v0.3. Phase 1: pure-functional engine — `Action` + `ProjectContext` → `Decision::Proceed` or `Decision::Escalate(reason)`. Phase 2: `Packet` markdown renderer (with v0.3 `batch:` support) + `DecisionOutcome` parser (recognises `APPROVED:` / `REJECTED:` / `EDIT_AND_RESUBMIT:` / partial form `APPROVED: 1-5,7; REJECTED: 6,8 [reason]`) + `GitOps` trait (subprocess `git` + mock for tests) + `commit_packet` + indefinite `await_decision` (no auto-reject, per v0.3). 156 tests pass; 2 POSIX-only end-to-end git tests gated `#[cfg(unix)]`. File loaders + driver-CLI deferred to Phase 3. |
| Verifier Docker image                      | ✅ `containers/Dockerfile.verifier` — multi-stage build, elan + Lean v4.29.1 preinstalled |
| Multi-arch CI matrix                       | ✅ Ubuntu + macOS + Windows with elan / lake / cargo caches |
| Sigstore signing in CI + `--verify-signature` | ✅ keyless cosign sign-blob on main + tags; verifier-side `refine bundle verify --verify-signature` (cosign subprocess) |
| Release scripting (`release/release.{sh,ps1}`) | ✅ semver check, CHANGELOG check, version bump, test run, tag + optional cosign tag-commit sig |
| Nix flake for hermetic builds              | ⚠️ authored (`flake.nix`); first-build verification pending (see [docs/reproducible-build.md](docs/reproducible-build.md) §8) |
| Mathlib mutation pipeline (corpus at N≥1000) | not yet             |
| Fine-tuned proof-repair model              | not yet (6+ month research commitment) |
| Syn-based scan (parse, not regex)          | not yet             |

## License

Dual-licensed under Apache-2.0 or MIT, at your option.
