# Architecture — Four Sections

refineforge is organised around four engineering disciplines, each
owning a distinct slice of the platform. The boundaries are not
cosmetic: each section produces artifacts the others consume through
a small, stable interface, and each can ship without the others
being complete.

| Section | Discipline                         | Priority | What it ships                                         |
|---------|------------------------------------|----------|-------------------------------------------------------|
| 1       | Lean 4 specialist (foundations)    | highest  | The proof engineering core                            |
| 2       | ML training engineer (intelligence)| second   | The repair-strategy implementations and the model     |
| 3       | Infrastructure / DevOps (surface)  | third    | Reproducible builds, signed bundles, distribution     |
| 4       | CUDA / GPU kernel engineer         | fourth   | Deterministic kernels and bit-exact gates             |

Read these in order. Each section is self-contained; later sections
depend on the earlier ones through narrow interfaces.

---

## Section 1 — Lean 4 Specialist (Foundations)

**Mission.** Own the formal-methods core: the Lean library, the claim
schema, the policy gate, the bundle format, and the refinement
methodology. This is the part of refineforge whose correctness
cannot be delegated to anything else — every other section trusts
that what comes out of Section 1 is mathematically honest.

**Subdirectories owned**

| Path                                       | What lives here                                  |
|--------------------------------------------|--------------------------------------------------|
| `lean/`                                    | Lake project, library root, every `.lean` module |
| `claims/`                                  | The claim registry (one YAML per claim)          |
| `templates/`                               | Scaffolding templates for new claims             |
| `docs/methodology.md`                      | The honest framing of what refineforge claims    |
| `docs/no-sorry-policy.md`                  | What the policy gate enforces                    |
| `docs/refinement-template.md`              | Skeleton for refinement-argument docs            |
| `docs/refinement/`                         | Filled-in refinement docs (one per refined claim)|
| `crates/refineforge-cli/src/claim.rs`      | YAML schema, loader                              |
| `crates/refineforge-cli/src/runner.rs`     | Policy gate → `lake build` driver                |
| `crates/refineforge-cli/src/sorry_gate.rs` | The no-sorry / no-admit / no-axiom enforcer      |
| `crates/refineforge-cli/src/bundle.rs`     | Bundle export / verify (SHA-256 manifest)        |
| `crates/refineforge-cli/src/scaffold.rs`   | `refine new` / `refine templates`                |
| `crates/refineforge-cli/src/scan.rs`       | Rust source name-presence check                  |

**Responsibilities**

The specialist authors Lean models, reviews refinement-argument
docs, maintains the template library, and curates the claim schema.
They are the only person with authority to upgrade a claim's
`status:` field from `model-only` to `model+refined`, and the only
person who can introduce a custom axiom (with documented
justification in the claim YAML and refinement doc).

**Current status (in refineforge today)**

- Library root + EXAMPLE-001 (Lean-only) + EXAMPLE-002 (refined): ✅
- Claim schema + loader: ✅
- Policy gate (sorry / admit / axiom, with comment stripping): ✅
- Bundle export + verify with cross-platform paths: ✅
- Three templates verified end-to-end: ✅
- Refinement template + filled-in EXAMPLE-002 answer-key: ✅

**Open work**

- More templates: linear types, capability-with-revocation,
  state-machine-with-invariants, refinement-types.
- Mathlib-aware bundle exporter — currently bundles inline the
  whole `lean/` directory; a Mathlib-using claim would need either
  pinned-Mathlib bundling or a separate transit-of-trust argument.
- Procedural macro that emits a Lean structure declaration from a
  Rust struct (`#[derive(LeanModel)]`), so the field-by-field
  correspondence in the refinement doc becomes mechanical to check
  rather than a prose comparison.
- Proof-tactic library for refinement obligations — e.g. a
  `refine_struct` tactic that discharges the trivial parts of a
  field correspondence and leaves only the genuine idealisations
  for the human to argue about.

**Interface to other sections**

Section 1 exposes two stable interfaces:

1. **The `RepairStrategy` trait** (in `repair/strategy.rs`). Section 2
   implements it; Section 1 promises never to break it.
2. **The bundle manifest schema** (`manifest.json`, `bundle_schema: 1`).
   Section 3 signs bundles and verifies signatures; Section 1
   promises the manifest is deterministic and the schema is
   versioned.

---

## Section 2 — ML Training Engineer (Repair Intelligence)

**Mission.** Make `refine repair` a working tool rather than a
skeleton. Build the data pipelines, train or wire the models, and
maintain the strategy implementations that turn a Lean diagnostic
into a candidate patch.

**Subdirectories owned**

| Path                                            | What lives here                                  |
|-------------------------------------------------|--------------------------------------------------|
| `crates/refineforge-cli/src/repair/strategy.rs` | The trait + `MockStrategy`                       |
| `crates/refineforge-cli/src/repair/lsp.rs`      | Lean LSP client (also Section 1 reviews this)    |
| `crates/refineforge-cli/src/repair/diagnostic.rs` | LSP diagnostic types                           |
| `crates/refineforge-cli/src/repair/mod.rs`      | Driver loop, `RepairConfig`, `RepairReport`      |
| `crates/refineforge-strategies/`                | Pluggable strategy crates (Anthropic + local-finetune command runtime) |
| `crates/refineforge-eval/`                      | Held-out corpus evaluator (`refine-eval`)        |
| `crates/refineforge-trainer/`                   | Training orchestration, dataset audit, backend adapter, promotion handoff |
| `training/data/`                                | Proof-repair corpus and SFT split artifacts      |
| `training/scripts/`                             | Stub trainer scripts and backend shims           |
| `training/configs/`                             | Axolotl/custom/HELYX-compatible experiment YAMLs |
| `models/` *(or LFS / external mirror)*          | External model checkpoints, if a project chooses to mirror them |
| `docs/repair-evaluation.md`                     | Benchmark methodology and current measured run state |

**Responsibilities**

The ML engineer owns:

- **Strategy implementations.** At minimum: `AnthropicStrategy`
  (calls the API), `LocalLLMStrategy` (Ollama or llama.cpp),
  `FineTunedStrategy` (loads the project's own model). The
  `MockStrategy` stays as a control case.
- **Data pipeline.** Scrape mathlib + Batteries + community proofs;
  mutate working proofs to produce broken / fixed pairs; deduplicate
  and split into train / val / held-out. The mutation taxonomy
  itself is a research artifact (drop a hypothesis, swap a lemma
  name, weaken an inductive case, introduce a wrong tactic).
- **Training.** Fine-tune an open base model (Qwen-Coder,
  DeepSeek-Prover, or similar) on the broken-to-fixed corpus through
  HELYX `helyx-train`, Axolotl, HF Trainer, or a custom backend.
  Document base model, hyperparameters, hardware, and total training
  cost so the run is reproducible.
- **Evaluation.** Held-out broken proofs from claims not in the
  training set. Report success rate per strategy, latency
  distributions, and cost per repair attempt. Numbers go in
  `docs/repair-evaluation.md`.
- **Inference packaging.** Promote successful checkpoints into the
  `refineforge-local-finetune.json` runtime contract, then evaluate them
  via `local-finetune` before any release claim.

**Current status**

- LSP client skeleton (framing, reader thread, JSON-RPC): ✅
- Diagnostic types + tests: ✅
- Driver loop with no-sorry gate after every patch: ✅
- `MockStrategy` (declines every proposal): ✅
- Real strategies: ✅ `anthropic`, `anthropic-mock`, and `local-finetune` command-manifest runtime. Local Ollama/llama.cpp strategy remains open.
- Training pipeline: ✅ Mathlib proof-repair corpus, deterministic SFT audit, trainer orchestration, HELYX-compatible backend adapter, run reports, and promotion handoff. Real accepted checkpoint remains open.
- Evaluation harness: ✅ `refine-eval` with JSON output and tutorial corpus. Held-out comparison for a real promoted checkpoint remains open.

**Open work, in order**

1. Run HELYX or Axolotl training on the audited Mathlib SFT split.
2. Promote the latest successful checkpoint with `refine-train promote`.
3. Evaluate the promoted local-finetune runtime on the held-out split and
   compare it against the Anthropic strategy.
4. Ship only the measured result: either the local model beats the hosted
   baseline on the target corpus, or the docs record that it does not.

**Interface to other sections**

- Consumes Section 1's `RepairStrategy` trait. Promises that every
  strategy honours the no-sorry contract (i.e. produces patches
  that the policy gate in `runner.rs` will accept).
- Hands Section 3 a model artifact + a strategy crate. Section 3
  packages it for distribution.

---

## Section 3 — Infrastructure / DevOps (Production Surface)

**Mission.** Make refineforge reproducible, signed, and shippable.
Turn "fork the repo and run `cargo build`" into "pull the container,
run `refineforge verify` against an attested bundle." This is the
section that makes the difference between an open-source library
and production-grade verification infrastructure.

**Subdirectories owned**

| Path                                | What lives here                                 |
|-------------------------------------|-------------------------------------------------|
| `.github/workflows/`                | CI matrix: Lean + Rust across OSes / arches     |
| `flake.nix`                         | Authored hermetic-build definition; first-build pending |
| `containers/`                       | Verifier Dockerfile                             |
| `release/`                          | Release scripts, semver checks, changelog gates |
| `docs/security.md`                  | Threat model, signing boundary, vuln reporting  |
| `docs/reproducible-build.md`        | How to rebuild a bundle bit-for-bit             |
| `attestation/` *(planned)*          | Future in-toto attestations beyond Sigstore blob signing |

**Responsibilities**

The DevOps engineer owns:

- **CI matrix.** At minimum: `x86_64-linux`, `aarch64-linux`,
  `x86_64-darwin`, `aarch64-darwin`. Each runs `lake build`,
  `cargo test`, `refine lean check-all`, `refine bundle export`,
  `refine bundle verify`.
- **Hermetic builds.** The authored Nix flake pins every input:
  elan, the Lean toolchain, the Rust toolchain, every Cargo
  dependency by hash. Goal: two independent rebuilds produce
  byte-identical bundles.
- **Signing and attestation.** The CI workflow is authored to
  Sigstore-sign bundles, and `refine bundle verify
  --verify-signature` shells out to cosign for reviewer-side
  verification. The first real GitHub OIDC signed-bundle run is
  still pending because this checkout has no remote configured.
- **Container distribution.** A `refineforge-verifier` image with
  Lean v4.29.1 preinstalled. Reviewers don't install elan; they
  `docker run` against a bundle directory and get an exit code.
- **Cache infrastructure.** Lean `.olean` caches via GitHub Actions
  cache; `sccache` for Rust. The first verifier run takes time;
  subsequent runs are seconds.
- **Optional, later:** hosted verification service. gRPC API:
  upload a bundle, get back a signed report. Multi-tenant,
  OIDC-authenticated. This is a real product surface and a real
  operational commitment.

**Current status**

- Multi-OS CI workflow: ✅ authored for Ubuntu, macOS, and Windows
- Hermetic builds: ⚠️ `flake.nix` authored; first-build verification pending
- Signed bundles: ⚠️ CI workflow authored and verifier-side checks shipped; first real GitHub OIDC run pending
- Container images: ✅ verifier Dockerfile shipped
- Cache infrastructure: ✅ GitHub Actions caches Cargo, lake, and elan inputs

**Open work, in order**

1. Release-readiness evidence: CI gates, SBOM/provenance artifacts,
   docs truth audit, and signed bundle readiness reports.
2. First real GitHub OIDC signing run after the repo has a remote.
3. First green Nix flake build on a Nix-capable runner.
4. Future in-toto attestation beyond the current Sigstore blob-signing
   workflow.

**Interface to other sections**

- Wraps Section 1's bundle format with signatures. The unsigned
  bundle remains valid; signed bundles add a layer.
- Packages Section 2's model artifacts into container images and
  cache-friendly layers.
- Promises CI never lies: a green build means every test passed
  and every claim is verified end-to-end on every supported
  platform.

---

## Section 4 — CUDA / GPU Kernel Engineer (bit-exact reproducibility)

**Mission.** Own the kernel-level work that makes refineforge's
bit-exact-across-hardware claim hold. Modern GPU kernels have
many sources of non-determinism (atomicAdd ordering, cuBLAS
algorithm selection, cuDNN, mixed precision); achieving bit-exact
reproducibility across hardware classes requires deliberate
kernel-level discipline.

**Subdirectories owned**

| Path                                            | What lives here                                  |
|-------------------------------------------------|--------------------------------------------------|
| `kernels/src/`                                  | Actual CUDA / HIP / Metal source                 |
| `kernels/configs/`                              | Per-kernel bit-exact-gate YAMLs                  |
| `kernels/scripts/`                              | Compiled binaries (or wrappers around them)      |
| `kernels/runs/`                                 | runtime: per-experiment gate outputs (gitignored)|
| `crates/refineforge-bitexact/`                  | the gate primitive (`refine-bitexact` binary)    |
| `docs/bit-exact-reproducibility.md`             | methodology — sources of non-determinism + mitigations |

**Responsibilities**

The CUDA engineer owns:

- **Kernel implementations.** Real `.cu` / `.cuh` source under
  `kernels/src/`, compiled to binaries that read deterministic
  inputs and write deterministic outputs.
- **Determinism hygiene.** Apply the mitigations table in
  `docs/bit-exact-reproducibility.md` §2-§3: deterministic
  reduction trees, pinned cuBLAS / cuDNN algorithms, no
  `atomicAdd` for float accumulation, proper env-var setup.
- **Bit-exact gate authorship.** One `kernels/configs/<kernel>.yaml`
  per kernel under test. The CI job runs them all on every push.
- **Cross-hardware verification.** Once multiple GPU classes are
  available in CI (A100 / H100 / consumer), aggregate per-runner
  reports; a "fully bit-exact" claim requires all runners to
  agree on hashes.
- **CUDA-version pin maintenance.** Record `gpu` / `cuda` /
  `driver` in each experiment's `hardware:` block; bump pinned
  versions in CI when validated.

**Current status (in refineforge today)**

- Gate primitive (`crates/refineforge-bitexact`): ✅ shipped + tested
- `refine-bitexact` CLI (`run` + `report` subcommands): ✅ shipped
- Stub deterministic + non-deterministic scripts (prove gate works
  in both directions): ✅ shipped
- `kernels/src/` actual CUDA kernels: ❌ empty (CUDA engineer fills)
- `docs/bit-exact-reproducibility.md` methodology: ✅ shipped
- CI matrix with GPU runners: ❌ not yet (requires self-hosted GPU
  CI infrastructure)
- Cross-hardware-class verification: ❌ deferred (requires multiple
  GPU runners)

**Open work, in order**

1. Hire a CUDA engineer (or assign one). The scaffold is ready.
2. Write the first real kernel under `kernels/src/`. Compile to
   `kernels/scripts/<name>`.
3. Author the first bit-exact gate config; verify it passes on
   a single GPU.
4. Add a self-hosted GPU runner to CI; wire the `bit-exact-gate`
   job.
5. Add a second GPU class (different hardware); verify
   cross-hardware bit-exactness.
6. Iterate on every kernel HELYX (or other consumers) want
   reproducibility-attested.

**Interface to other sections**

- Section 4's outputs (signed bit-exact gates per kernel)
  attach to Section 3's bundles. A bundle can advertise:
  "the cited Rust source compiles to the GPU kernels at
  `kernels/src/`, and those kernels pass the bit-exact gate at
  commit `<sha>` on hardware classes [A100, H100]."
- Section 4 does NOT depend on Section 1's `RepairStrategy` or
  Section 2's training pipeline. The orchestration scaffold
  (`refineforge-bitexact`) is independent of the LLM repair loop.
- Section 4's `bit-exact-gate` CI job is added by Section 3
  alongside the other CI jobs.

---

## How the four sections connect

```
┌─────────────────────────────────────────────────────────────────┐
│  Section 1 — Lean 4 Specialist                                  │
│  lean/ claims/ templates/ docs/methodology.md                   │
│  +  bundle format, policy gate, RepairStrategy trait            │
└────────┬─────────────────────────────────────────────────┬──────┘
         │ trait surface                                   │ bundle schema
         ▼                                                 ▼
┌──────────────────────────┐  ┌──────────────────────────────────┐
│  Section 2 — ML Training │  │  Section 3 — Infra / DevOps      │
│  RepairStrategy impls    │  │  CI matrix, hermetic builds      │
│  training/ pipelines     │  │  sigstore signing                │
│  eval harness, benchmarks│  │  containers/ release/            │
└──────┬───────────────────┘  └────────────────────────┬─────────┘
       │ model artifact                                ▲
       │ + strategy crate                              │ kernel reports
       └──────────────────────────────────┐            │ attach to bundles
                                          ▼            │
                          ┌──────────────────────────────────┐
                          │  Section 4 — CUDA / GPU Kernels  │
                          │  kernels/ src/ configs/ scripts/ │
                          │  refineforge-bitexact gate       │
                          │  bit-exact reproducibility       │
                          └──────────────────────────────────┘
```

The trait surface, the bundle schema, and the bit-exact gate's
report format are the only three interfaces that must stay stable.
Everything else can be reorganised inside its section without
affecting the others.

---

## Sequencing — what to do first

The sections are listed in priority order for a reason. The realistic
path forward, assuming one or two engineers at a time:

| Phase | Duration estimate | What ships                                            |
|-------|-------------------|-------------------------------------------------------|
| Now   | already shipped   | Section 1 complete enough to be useful                |
| +1 mo | 1 engineer-month  | Section 3 phase 1: release readiness, CI evidence, verifier container |
| +3 mo | 1 engineer-month  | Section 2 phase 1: AnthropicStrategy + eval harness   |
| +6 mo | 1 engineer-month  | Section 3 phase 2: sigstore signing + Nix builds      |
| +9 mo | 2 engineer-months | Section 2 phase 2: mathlib mutation + fine-tuning     |
| later | hardware-bound    | Section 4: real kernels + self-hosted GPU runners     |

If all four sections start at once with one engineer, every section
is partially done and nothing ships. If they start in priority order
and each ships before the next starts, refineforge grows credibility
incrementally.

---

## What this is *not*

This four-section structure is **not** a hiring document. It is a
boundary document. The boundaries hold whether one person wears all
four hats or four engineers own one each. The point is that the Lean
specialist never has to think about sigstore, the DevOps engineer
never has to think about whether `Valid` captures the domain, the ML
engineer can swap models without touching the proof code, and the
GPU engineer can harden kernels without changing claim semantics.

It is also **not** a roadmap to "AI proves Lean automatically."
Section 2 makes the repair loop usable. It does not make the
specification correct, the refinement argument honest, or the
trusted code base smaller. Those are still — and will remain —
human responsibilities.
