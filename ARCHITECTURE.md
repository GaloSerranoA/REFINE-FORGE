# Architecture — Three Sections

refineforge is organised around three engineering disciplines, each
owning a distinct slice of the platform. The boundaries are not
cosmetic: each section produces artifacts the others consume through
a small, stable interface, and each can ship without the others
being complete.

| Section | Discipline                         | Priority | What it ships                                         |
|---------|------------------------------------|----------|-------------------------------------------------------|
| 1       | Lean 4 specialist (foundations)    | highest  | The proof engineering core                            |
| 2       | ML training engineer (intelligence)| second   | The repair-strategy implementations and the model     |
| 3       | Infrastructure / DevOps (surface)  | third    | Reproducible builds, signed bundles, distribution     |

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
| **NEW:** `crates/refineforge-strategies/`       | Pluggable strategy crates (one per provider)     |
| **NEW:** `training/data/`                       | Synthetic-data generation pipeline               |
| **NEW:** `training/scripts/`                    | Fine-tuning scripts (HuggingFace, axolotl, etc.) |
| **NEW:** `training/eval/`                       | Held-out theorem corpus, success-rate harness    |
| **NEW:** `models/` *(or LFS / external mirror)* | Model checkpoints                                |
| **NEW:** `docs/repair-evaluation.md`            | Benchmark methodology and current numbers        |

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
  DeepSeek-Prover, or similar) on the broken→fixed corpus.
  Document base model, hyperparameters, hardware, and total
  training cost so the run is reproducible.
- **Evaluation.** Held-out broken proofs from claims not in the
  training set. Report success rate per strategy, latency
  distributions, and cost per repair attempt. Numbers go in
  `docs/repair-evaluation.md`.
- **Inference packaging.** Ship the trained model either as a
  weight file the user downloads, or via the LocalLLMStrategy
  pointing at a self-hosted endpoint.

**Current status**

- LSP client skeleton (framing, reader thread, JSON-RPC): ✅
- Diagnostic types + tests: ✅
- Driver loop with no-sorry gate after every patch: ✅
- `MockStrategy` (declines every proposal): ✅
- Real strategies: ❌ none yet
- Training pipeline: ❌ doesn't exist
- Evaluation harness: ❌ doesn't exist

**Open work, in order**

1. `AnthropicStrategy` against the existing trait — one file, no
   training. This is the cheapest path to a working
   `refine repair` and validates the trait surface against a real
   provider.
2. Evaluation harness against the EXAMPLE-001/002 corpus plus
   synthetic broken variants. Establishes the benchmark before
   investing in training.
3. Mathlib mutation pipeline. Produces the training corpus.
4. Fine-tune a small open model and beat the AnthropicStrategy on
   the eval harness (or document honestly that it doesn't).

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
| **NEW:** `nix/` *(or `bazel/`)*     | Hermetic, reproducible build definitions        |
| **NEW:** `containers/`              | Dockerfiles: `refine`, `refine-verifier`        |
| **NEW:** `attestation/`             | Sigstore / in-toto signing pipeline             |
| **NEW:** `release/`                 | Release scripts, semver checks, changelog gates |
| **NEW:** `docs/security.md`         | Threat model, signing chain, vuln reporting     |
| **NEW:** `docs/reproducible-build.md` | How to rebuild a bundle bit-for-bit            |

**Responsibilities**

The DevOps engineer owns:

- **CI matrix.** At minimum: `x86_64-linux`, `aarch64-linux`,
  `x86_64-darwin`, `aarch64-darwin`. Each runs `lake build`,
  `cargo test`, `refine lean check-all`, `refine bundle export`,
  `refine bundle verify`.
- **Hermetic builds.** Nix flake (or Bazel) that pins every input:
  elan, the Lean toolchain, the Rust toolchain, every Cargo
  dependency by hash. Goal: two independent rebuilds produce
  byte-identical bundles.
- **Signing and attestation.** Sigstore signs every bundle in CI.
  `refine bundle verify` learns a new flag `--verify-signature`
  that checks the Rekor transparency log. The signature ties
  bundle hash → git commit → signer identity, so a third party can
  prove who built which bundle from which source.
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

- Single-runner CI (Ubuntu, single arch): ✅ basic
- Hermetic builds: ❌
- Signed bundles: ❌
- Container images: ❌
- Cache infrastructure: ❌ (GitHub Actions cache used, not Lean-aware)

**Open work, in order**

1. Multi-arch CI. Cheapest credibility win.
2. Container image for the verifier. Removes elan-install friction
   for every reviewer.
3. Sigstore signing in CI + verification in `refine bundle verify
   --verify-signature`. This is the artifact that turns "we built
   this bundle" into "we built this bundle, here's the cryptographic
   proof, and Rekor has a public log entry."
4. Nix flake for hermetic builds. Big lift; pays off when reviewers
   start asking "did you build the same bytes I did?"

**Interface to other sections**

- Wraps Section 1's bundle format with signatures. The unsigned
  bundle remains valid; signed bundles add a layer.
- Packages Section 2's model artifacts into container images and
  cache-friendly layers.
- Promises CI never lies: a green build means every test passed
  and every claim is verified end-to-end on every supported
  platform.

---

## How the three sections connect

```
┌─────────────────────────────────────────────────────────────────┐
│  Section 1 — Lean 4 Specialist                                  │
│                                                                 │
│  lean/  claims/  templates/  docs/methodology.md  …             │
│  +  bundle format, policy gate, RepairStrategy trait            │
└──────────┬──────────────────────────────────────────────┬───────┘
           │                                              │
           │ trait surface                                │ bundle schema
           │                                              │
           ▼                                              ▼
┌──────────────────────────────────┐     ┌──────────────────────────────────┐
│  Section 2 — ML Training         │     │  Section 3 — Infra / DevOps      │
│                                  │     │                                  │
│  RepairStrategy impls            │     │  CI matrix, hermetic builds      │
│  training/ pipelines             │     │  sigstore signing                │
│  models/ artifacts               │     │  containers/  attestation/       │
│  eval harness, benchmarks        │     │  reproducible bundles            │
│                                  │     │                                  │
└──────────┬───────────────────────┘     └──────────────────────────────────┘
           │                                              ▲
           │ model artifact + strategy crate              │
           └──────────────────────────────────────────────┘
                            packaged & distributed
```

The trait surface and the bundle schema are the only two interfaces
that must stay stable. Everything else can be reorganised inside its
section without affecting the others.

---

## Sequencing — what to do first

The sections are listed in priority order for a reason. The realistic
path forward, assuming one or two engineers at a time:

| Phase | Duration estimate | What ships                                            |
|-------|-------------------|-------------------------------------------------------|
| Now   | already shipped   | Section 1 complete enough to be useful                |
| +1 mo | 1 engineer-month  | Section 3 phase 1: multi-arch CI + verifier container |
| +3 mo | 1 engineer-month  | Section 2 phase 1: AnthropicStrategy + eval harness   |
| +6 mo | 1 engineer-month  | Section 3 phase 2: sigstore signing + Nix builds      |
| +9 mo | 2 engineer-months | Section 2 phase 2: mathlib mutation + fine-tuning     |

If all three sections start at once with one engineer, every section
is 30% done and nothing ships. If they start in priority order and
each ships before the next starts, refineforge grows credibility
incrementally.

---

## What this is *not*

This three-section structure is **not** a hiring document. It is a
boundary document. The boundaries hold whether one person wears all
three hats or three engineers own one each. The point is that the
Lean specialist never has to think about sigstore, the DevOps
engineer never has to think about whether `Valid` captures the
domain, and the ML engineer can swap models without touching the
proof code.

It is also **not** a roadmap to "AI proves Lean automatically."
Section 2 makes the repair loop usable. It does not make the
specification correct, the refinement argument honest, or the
trusted code base smaller. Those are still — and will remain —
human responsibilities.
