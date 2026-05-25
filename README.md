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
| [ARCHITECTURE.md](ARCHITECTURE.md) | Four-section structure: Lean, Release/DevOps, ML, and CUDA/GPU ownership with interfaces and sequencing |
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
| [docs/security.md](docs/security.md) | Threat model, supply chain, signed-bundle verification, first-CI-signing boundary, vuln reporting |
| [docs/reproducible-build.md](docs/reproducible-build.md) | Bit-identical-rebuild methodology — Nix flake (authored), verification protocol |
| [docs/verification/proof-inventory.md](docs/verification/proof-inventory.md) | Current Lean-backed claim inventory: theorem shape, scope, and implementation-link status |
| [docs/release/release-readiness-inventory.md](docs/release/release-readiness-inventory.md) | Release infrastructure inventory: shipped-local, stub-tested, CI-pending, blocked, and planned surfaces |
| [docs/release/ci-audit-report.md](docs/release/ci-audit-report.md) | CI/release audit report template and current local blocker record |
| [docs/agents/README.md](docs/agents/README.md) | CLI-first HELYX specialist agents: Lean, DevOps, training, and kernel evidence reports |
| [docs/training/hrm-text-analysis.md](docs/training/hrm-text-analysis.md) | Analysis of HRM-Text and which production-training ideas can inform Refine-Forge |
| [docs/training/train-llm-from-scratch-analysis.md](docs/training/train-llm-from-scratch-analysis.md) | Analysis of an external PyTorch from-scratch LLM repo and what should/should not be ported |
| [docs/bit-exact-reproducibility.md](docs/bit-exact-reproducibility.md) | GPU kernel bit-exact reproducibility: non-determinism sources + mitigations + gate primitive |
| [docs/superpowers/plans/2026-05-22-gpu-kernel-bitexact-track.md](docs/superpowers/plans/2026-05-22-gpu-kernel-bitexact-track.md) | Part 4 execution plan for the HELYX `helyx-kernels` → Refine-Forge `refineforge-bitexact` handoff |
| [docs/escalation-criteria.md](docs/escalation-criteria.md) | **CONTRACT v0.3** (operator-signed; v0.2 superseded same-day with Q1/Q3/Q4 revisions). 9 categories that always escalate to the human during `refine autonomous` runs. Enforced by `crates/refineforge-escalation` |
| [docs/why-rust.md](docs/why-rust.md) | Reviewer-grade answer to "why not PyTorch?" — the trade-off, the costs, the ceiling-property reasons. Anchors on the operator's one-liner: **"PyTorch is good for humans. Rust is good for machines."** |
| [docs/ecosystem.md](docs/ecosystem.md) | **v0.2 (HELYX-centric reframe).** HELYX is the operator's LLM; the other three (Cogn8ty + Knowledge-Foundry + refineforge) are HELYX's infrastructure. **refineforge exists because the alternative was hiring four engineers to verify HELYX's load-bearing claims manually.** One-liner: **"HELYX is the LLM. Cogn8ty thinks for it, KF teaches it, refineforge proves it. Rust binds them."** v0.1 had flat-peers framing; superseded |
| [docs/plans/autonomous-driver-plan.md](docs/plans/autonomous-driver-plan.md) | Enterprise build plan for `refine autonomous`: 5 phases, ~2 weeks, $50-150 API budget, risks + mitigations. **All plan phases 0-5 + 3.5 + 3.6 + 3.7 + 3.8 closed** under criteria v0.3. Phase 4 acceptance gate exercised against real Anthropic ($0.35 spend); v0.2.0 + v0.2.1 released and tagged; only Nix-flake first-build verification remains (needs a Nix-capable runner) |
| [docs/plans/gui-plan.md](docs/plans/gui-plan.md) | **PLAN ONLY (no code yet).** Enterprise build plan for `refineforge-studio`, a professional Tauri-based production GUI exposing the four sections + operator console + autonomous driver. 9 phases, ~15 weeks, 6 named open questions to resolve before Phase 0 |
| [docs/plans/resourcing-plan.md](docs/plans/resourcing-plan.md) | **PLAN ONLY (v0.2).** Operator-side resourcing: 1 human operator + 1 part-time maintainer (or operator's own time) + compute. **refineforge IS the four specialists**, not headcount. 16,000 GPU-hours for the fine-tune via Option A grants (~$5-10k cash outlay). 12-month budget: **~$12-52k LATAM mid-band; ~$35-108k US ceiling.** v0.1 (commit `2f60d9c`) had the framing inverted; superseded |
| [docs/plans/finetuning-plan.md](docs/plans/finetuning-plan.md) | **PLAN + local corpus artifacts (v0.2).** End-to-end proof-repair fine-tuning pipeline: Mathlib mutation corpus (N≥1000) → Knowledge-Foundry `lean_proof_repair` SFT mode + claim-level gates → Cogn8ty `brain_reason` consistency filter → Axolotl or HELYX fine-tune → `refine --strategy local-finetune`. The first real Mathlib corpus and Anthropic SFT split are under `training/data/mathlib-proof-repair-v1/`; real training/checkpoint acceptance remains pending. |
| [docs/plans/finetuning-execution-2026-05-20.md](docs/plans/finetuning-execution-2026-05-20.md) | **Execution ledger.** Records the local KF Lean probe/gates, Cogn8ty gate contract, trainer smoke fixture, and `local-finetune` bridge. Updated by the corpus addendum; remaining blockers are live Cogn8ty corpus pass, real checkpoint, and acceptance comparison. |
| [docs/plans/finetuning-execution-2026-05-20-corpus-run.md](docs/plans/finetuning-execution-2026-05-20-corpus-run.md) | **Corpus execution addendum.** Records the 1000-row Mathlib mutation corpus, finalized 1000-row Anthropic SFT split, source commit, spend estimate, and current training/runtime blockers. |
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

## Four enterprise sections

Refine-Forge now carries the local automation surface for the four specialist
roles the operator originally described. HELYX owns the production LLM stack;
Refine-Forge owns proof, release, training evidence, native smoke training,
and gate artifacts around those systems.

| Part | Specialist role | Refine-Forge surface | HELYX boundary |
|---:|---|---|---|
| 1 | Lean 4 / verification engineer | `refine lean`, `refine scan`, bundle export/verify, template provenance, claim linting | Proves and packages claims about HELYX-adjacent Rust behavior |
| 2 | Release / infrastructure / DevOps engineer | CI gates, release readiness, verifier container, SBOM/provenance evidence, docs truth audit, signed-bundle flow | Makes releases auditable; first real remote/OIDC signing still depends on hosted CI |
| 3 | ML / training engineer | `refine-train` dataset audit, SFT packing, causal-LM preprocessing, built-in `refineforge_native` and `refineforge_native_causal_lm` smoke training, HELYX/HRM/PyTorch orchestration, checkpoints, reports, evidence generation, local-finetune promotion, and Training Agent evidence validation | Refine-Forge can run local native smoke training and produce evidence packs; production model trust still requires real held-out eval, regression, compute ledger, conversion, promotion lineage, and human approval evidence |
| 4 | GPU / kernel Rust engineer | `refine-bitexact` lint/run/run-all, HELYX-compatible kernel metadata, input manifests, expected SHA-256 baselines, local CUDA smoke source/evidence, CI summary JSON | `helyx-kernels` implements production kernels; Refine-Forge proves bit-exact gate evidence |

The four specialist roles are exposed as CLI agents:
`refine agent lean|devops|train|kernel|run-all`. Every agent report now carries
an explicit liveness record, capability inventory, tool-gate inventory, command
evidence, artifacts, warnings, blockers, and a bounded trust level. `execute`
mode runs the local role execution surface: Lean gates, release readiness,
trainer dry-run by default, and the bit-exact kernel gate. Pass
`--allow-expensive` only when live trainer backends or Docker/signature release
gates should run.

The Training agent accepts `REFINEFORGE_TRAINING_EVIDENCE_DIR` only as a
pointer to real files. It validates checkpoint, eval, regression, compute
ledger, promotion, and human approval artifacts before it can emit
`human-reviewed`; missing paths or placeholder approvals keep it
`measured-only`.

The Kernel lane now includes a real local CUDA smoke gate,
`kernels/src/hvector_add.cu` with `kernels/configs/hvector-add-cuda.yaml`.
That proves local source/hardware/bit-exact evidence capture on one RTX 3060
Laptop GPU. It does not prove full HELYX kernel correctness or cross-hardware
portability until HELYX production kernels, GPU CI runners, and human kernel
approval evidence are present.

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

`crates/refineforge-derive` is a supported documentation aid. Its
`#[derive(LeanModel)]` macro emits a Lean structure declaration for
review and refinement documentation; it does not generate proofs and
does not replace a human-reviewed refinement argument.

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
│   ├── refineforge-trainer/    # `refine-train` native smoke trainer + orchestration CLI (Section 2)
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
│   ├── configs/                # example, Axolotl, and HELYX-compatible experiment YAMLs
│   ├── scripts/                # stub-trainer for tests; backend shims
│   ├── data/                   # smoke fixture + mathlib-proof-repair-v1 corpus
│   └── runs/                   # refine-train per-experiment output (gitignored)
├── kernels/                    # Section 4: GPU kernels + bit-exact gates
│   ├── configs/                # per-kernel gate YAMLs + HELYX-compatible contract smoke
│   ├── fixtures/               # deterministic input bytes hashed into reports
│   ├── scripts/                # stub-deterministic + stub-nondeterministic (sh + ps1)
│   ├── src/                    # actual .cu source (empty; CUDA engineer fills)
│   └── runs/                   # refine-bitexact per-gate output (gitignored)
├── artifacts/                  # exported verification bundles
├── agent-reports/              # local `refine agent` run outputs (gitignored)
├── schemas/
│   └── agent-report.schema.json # machine-readable agent evidence contract
├── escalations/                # `refine autonomous` writes decision packets here (one dir per CLAIM-ID)
├── autonomous/                 # `refine autonomous` per-run RunReport JSONs (gitignored)
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
    ├── bit-exact-reproducibility.md  # GPU kernel determinism + gate primitive
    ├── agents/               # role prompts and CLI evidence rules for the four agents
    ├── escalation-criteria.md  # CONTRACT v0.3 — 9 categories `refine autonomous` escalates on
    ├── HELYX-CASE-STUDY.md     # link to external worked example
    └── plans/
        ├── autonomous-driver-plan.md  # 5-phase build plan; all phases shipped + 3.5/3.6/3.7/3.8
        ├── gui-plan.md               # 9-phase plan for refineforge-studio (no code yet)
        ├── resourcing-plan.md        # people/compute/tools/funding; 12-mo budget; A/B/C options
        ├── finetuning-plan.md        # 8-phase fine-tune via KF + Cogn8ty + axolotl
        └── finetuning-execution-2026-05-20.md # local execution ledger + blockers
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
| `refine lint check <id>` / `check-all` | Fast claim linter: Rust symbol drift, refinement-doc sections, explicit `review.human_operator`, fake reviewer names, and CRS model-only disclosure |
| `refine bundle export <id>`            | Bundle the sources + manifest + report                                  |
| `refine bundle verify <bundle-dir>`    | Re-hash every file in a bundle and confirm the manifest matches         |
| `refine bundle verify <bundle-dir> --verify-signature` | Hashes + Sigstore signature (via cosign). See [SECURITY.md](SECURITY.md) |
| `refine repair <id>`                   | Bounded LLM repair loop against Lean's LSP server. Strategies: `mock` (declines all), `anthropic-mock` (canned), `anthropic` (real HTTP, needs `ANTHROPIC_API_KEY`), and `local-finetune` (needs a promoted weights/runtime directory). See [`docs/llm-repair-design.md`](docs/llm-repair-design.md) |
| `refine autonomous <id> [flags]` | Drives a claim end-to-end (lean check → scan → bundle), escalating per criteria v0.3 via `crates/refineforge-escalation`. Flags: `--strategy mock\|anthropic-mock\|anthropic`, `--max-cost-usd N`, `--operator EMAIL`, `--dry-run`, `--auto-repair` (inject Repair after failed LeanCheck), `--await-decisions` (block-poll operator packet decisions; Phase 3.8 preserves APPROVED packets across re-runs), `--inject-counter-idealisation` (EXAMPLE-002 Cat 2 bait), `--inject-training PATH` (repeatable; appends `refine-train run PATH --dry-run` step), `--inject-bitexact PATH` (repeatable; appends `refine-bitexact run PATH` step). Anthropic `UsageStats` in the RunReport: per-call `input_tokens`, `output_tokens`, `cache_creation_input_tokens`, `cache_read_input_tokens`, and `stop_reasons` (`end_turn` / `max_tokens` / etc.). All plan §3 phases closed; see [docs/plans/autonomous-driver-plan.md](docs/plans/autonomous-driver-plan.md) |
| `refine agent <lean\|devops\|train\|kernel\|run-all>` | CLI-first HELYX specialist agents. Modes: `inspect`, `check`, `repair`, `execute`; each writes JSON + Markdown evidence with liveness, capability, tool-check, command, artifact, warning, blocker, and trust fields under `agent-reports/` using `schemas/agent-report.schema.json`. See [`docs/agents/README.md`](docs/agents/README.md) |
| `refine training-approval draft`       | Validate a Training Agent report, approval policy, checkpoint/eval/regression/ledger/conversion/promotion evidence, then write `approvals/training.draft.json` and `training.review-request.json` without creating final human approval |
| `refine training-approval approve --i-reviewed-this-evidence` | Rerun the same validation and write `approvals/training.json` only for a named policy-allowed human operator |
| `refine escalations list [--claim X] [--age-gt N]` | Operator queue dashboard for `escalations/<CLAIM-ID>/*.md`; PENDING vs DECIDED sorted by age. Per criteria v0.3 the driver never auto-rejects |
| `refine-eval --corpus … --strategy …`  | Drive `refine repair` against a JSONL corpus; emit JSON report. See [`docs/repair-evaluation.md`](docs/repair-evaluation.md) |
| `refine-train data audit <jsonl>`      | Validate proof-repair SFT JSONL counts, split counts, patch JSON, duplicate ids, and SHA-256 before training |
| `refine-train data pack-sft <jsonl>`   | Build deterministic SFT token packs with target-only loss masks, shuffle files, packing reports, and multipack rank plans |
| `refine-train data causal-lm-preprocess <jsonl>` | Convert JSONL/JSONL.zst text rows into causal-LM token/chunk manifests |
| `refine-train run <exp.yaml>`          | Run one training experiment (`refineforge_native`, `refineforge_native_causal_lm`, HELYX `helyx-train`, HRM/PyTorch `torchrun`, axolotl, HF Trainer, or custom backend). See [`training/README.md`](training/README.md). Always start with `--dry-run`. |
| `refine-train sweep <sweep.yaml>`      | Grid or random hyperparameter sweep |
| `refine-train monitor <run_dir>`       | Tail `progress.jsonl` and show latest metrics |
| `refine-train report <run_dir>`        | Build / refresh `report.json` for a run |
| `refine-train evidence <run_dir>`      | Generate training production-proof evidence: checkpoint carrier, eval, regression, compute ledger, conversion manifest, and promotion manifest |
| `refine-train promote <run_dir>`       | Convert a successful training report + latest checkpoint into a `refineforge-local-finetune.json` runtime directory plus `promotion-report.json` |
| `refine-bitexact lint <kernel.yaml> [--json] [--output <path>]` | Lint a kernel experiment for strict CUDA / HELYX contract readiness before execution |
| `refine-bitexact run <kernel.yaml>`    | Bit-exact reproducibility gate: run kernel N times, fail if SHA-256 hashes disagree or miss `expected_sha256`. See [`kernels/README.md`](kernels/README.md) and [`docs/bit-exact-reproducibility.md`](docs/bit-exact-reproducibility.md) |
| `refine-bitexact run-all <dir> [--include-examples] [--summary-json <path>]` | Deterministically discover kernel YAMLs, run every included gate, and write aggregate CI summary JSON. By default, demo `example-*` and `*-smoke.yaml` configs are skipped so CI does not confuse examples with production kernels. |
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
| Claim honesty linter                       | ✅ explicit `review.human_operator`, no fake AI reviewer placeholders, CRS `model-only` disclosure, Rust-symbol drift, refinement-doc sections |
| Refinement-argument template               | ✅ `docs/refinement-template.md` |
| LLM repair loop (LSP client)               | ✅ shipped: `mock`, `anthropic-mock`, **`anthropic`** (real HTTP with retry + prompt caching), and `local-finetune` (command-manifest runtime bridge) |
| `refineforge-strategies` workspace member  | ✅ `AnthropicStrategy` + `ReqwestTransport` plus `LocalFinetuneStrategy` command-manifest bridge. Native checkpoint loading is still blocked on the final model/tokenizer layout. |
| `refineforge-eval` (`refine-eval` binary)  | ✅ corpus-driven evaluation harness with JSON output; ships a 3-entry tutorial corpus under [`eval/corpus/`](eval/corpus) |
| `refineforge-trainer` (`refine-train` binary) | ✅ training control plane and local training framework with deterministic dataset audit, SFT packing, causal-LM preprocessing, built-in `refineforge_native` and `refineforge_native_causal_lm` smoke backends, HELYX `helyx-train` / HRM-Text / PyTorch baseline / axolotl / HF Trainer / custom orchestration, run tracking, checkpoint resume, failure recovery, JSON reports, production-proof evidence generation, and local-finetune promotion. Native smoke checkpoints are evidence of local training execution, not proof of production model quality. See [`training/README.md`](training/README.md) |
| `refineforge-bitexact` (`refine-bitexact` binary) | ✅ enterprise bit-exact gate: strict contract linting, HELYX-compatible metadata, input manifests, expected output baselines, per-run JSONL, run reports, and `run-all` CI aggregation. Real HELYX/CUDA kernels remain external kernel-engineering domain. See [`kernels/README.md`](kernels/README.md). |
| `refineforge-escalation` (library) | ✅ Phases 1 + 2 + 3.5 of [`docs/plans/autonomous-driver-plan.md`](docs/plans/autonomous-driver-plan.md) under criteria v0.3. Pure-functional engine (`Action` + `ProjectContext` → `Decision::Proceed`/`Escalate`); `Packet` markdown renderer with v0.3 `batch:` support; `DecisionOutcome` parser (`APPROVED:`/`REJECTED:`/`EDIT_AND_RESUBMIT:`/partial form); `GitOps` trait (subprocess `git` + `MockGitOps`); indefinite `await_decision` (no auto-reject); ProjectContext loaders (claim YAML + lake-manifest.json + Cargo.lock). 170 tests; 2 POSIX-only e2e git tests gated `#[cfg(unix)]` |
| `refine autonomous` driver (in `refineforge-cli`) | ✅ Phases 3 MVP + 3.5 + 3.6 + 3.7 + 3.8 + Phase 4 audit. `Planner` + `Executor<G: GitOps>` + `WorkRunConfig` + `run_worklist`; real `runner::run`/`scan::scan_claim`/`bundle::export` library calls; `Repair` step with `resolve_strategy` (mock / anthropic-mock / **anthropic** real HTTP / `local-finetune` command-manifest bridge) + cost-gate ($0.07/attempt upfront for Anthropic); `RunTrainingExperiment`/`RunBitExactGate` subprocess-shell; `--auto-repair` + `--await-decisions` + `--inject-counter-idealisation` + `--inject-training` + `--inject-bitexact` flags. **Phase 3.8 cross-run await-resume**: existing APPROVED packets survive re-runs. Live LLM auto-repair confirmed end-to-end ([60d2a81](#)). EXAMPLE-002 forced-Counter dogfood passes as integration test AND was exercised against real Anthropic in the Phase 4 audit ($0.35 spend) |
| Verifier Docker image                      | ✅ `containers/Dockerfile.verifier` — multi-stage build, elan + Lean v4.29.1 preinstalled |
| Multi-arch CI matrix                       | ✅ Ubuntu + macOS + Windows with elan / lake / cargo caches |
| Sigstore signing in CI + `--verify-signature` | ⚠️ CI workflow authored and verifier-side `refine bundle verify --verify-signature` shipped; first real GitHub OIDC signed-bundle run pending until the repo has a remote |
| Release scripting (`release/release.{sh,ps1}`) | ✅ semver check, CHANGELOG check, version bump, `refine release ready` evidence gate, tag + optional cosign tag-commit sig |
| Nix flake for hermetic builds              | ⚠️ authored (`flake.nix`); first-build verification pending (see [docs/reproducible-build.md](docs/reproducible-build.md) §8) |
| Mathlib mutation pipeline (corpus at N≥1000) | ✅ shipped: `training/data/mathlib-proof-repair-v1/` has 1000 Mathlib mutation rows plus finalized Anthropic SFT train/val/heldout splits |
| `local-finetune` runtime bridge            | ✅ command-manifest strategy shipped; `refine-train promote` writes the runtime manifest for successful checkpoints. Real accepted checkpoint and held-out comparison are still pending |
| Fine-tuned proof-repair model              | not yet (6+ month research commitment) |
| Syn-based scan (parse, not regex)          | not yet             |

**Verification snapshot (2026-05-22, local `master`):** `cargo test -p refineforge-bitexact`,
`cargo test -p refineforge-cli`, `refine-bitexact lint kernels/configs/helyx-bitexact-smoke.yaml`,
`refine-bitexact run kernels/configs/helyx-bitexact-smoke.yaml`, and
`refine release ready --version 0.2.2 --allow-dirty --skip-docker --skip-signature`
all pass. The HELYX smoke gate observed stable SHA-256
`d5ca9a70b50179b497870748df66099e49197509f40ae6785513648a87eb2e03`.
Historical v0.2.0 full-workspace nextest snapshot remains `383/383` at commit
`6486c6a`; newer Part 3 and Part 4 work is tracked under `[Unreleased]`.

## License

Dual-licensed under Apache-2.0 or MIT, at your option.
