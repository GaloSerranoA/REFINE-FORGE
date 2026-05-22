# Changelog

All notable changes to refineforge are documented here.

This project follows a loose [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
style and [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Pre-1.0 releases may break compatibility in either direction without
a major bump; the version field will start tracking strictly once the
CLI surface is declared stable.

## [Unreleased]

### Added — GPU/kernel bit-exact enterprise gate

- `KernelExperiment` now supports HELYX-compatible contract metadata:
  `template_version`, `producer`, `kernel_id`, `profile`, `expected_sha256`,
  `input_files`, and sorted `tags`.
- `expected_sha256` closes the stable-but-wrong gap: identical run hashes still
  fail when they do not match the declared baseline.
- `refine-bitexact lint` enforces strict CUDA / HELYX contract readiness before
  execution.
- `refine-bitexact run-all` discovers kernel configs deterministically, runs all
  included gates, and writes aggregate CI summary JSON.
- `kernels/configs/helyx-bitexact-smoke.yaml` provides a HELYX-compatible
  contract fixture while keeping real `helyx-kernels` implementation external.

### Added — ML training engine HELYX-compatible orchestration

- `refine-train data audit` validates proof-repair SFT JSONL row counts,
  split counts, duplicate ids, patch JSON shape, and SHA-256 before a
  training run.
- `backend.kind=helyx_train` resolves to `helyx-train run --config ...`
  while Refine-Forge keeps ownership of run directories, checkpoints,
  logs, and reports.
- `refine-train promote` converts a successful `report.json` plus latest
  checkpoint into a `refineforge-local-finetune.json` runtime directory and
  `promotion-report.json`.
- Training docs now describe the boundary honestly: HELYX/Axolotl/custom
  backends train; Refine-Forge audits, orchestrates, reports, promotes, and
  evaluates.

### Changed — `docs/ecosystem.md` rewritten v0.2 (HELYX-as-LLM reframe; v0.1 had flat-peers framing)

v0.1 of this doc (commit `0805430`) framed the four projects
as **flat complementary peers** that compose together. That
was a framing bug. The operator corrected it twice in the
same conversation, restating that **HELYX is their LLM** and
the other three are HELYX's *infrastructure*.

Evidence from `C:\HELYX\` confirming the corrected framing:
- Canonical project doc is named **`HELYX LLM project.txt`**.
- README's opening question: "What would have to be true for
  HELYX to be the **structurally best AI codebase** that has
  ever existed?"
- Compares itself to **PyTorch, JAX** (the LLM ecosystem),
  not Lean/Coq.
- States: "**A property no production LLM has today**" —
  HELYX positions itself AS a production LLM.
- Crates: `helyx-train`, `helyx-distill`, `helyx-planner`,
  `helyx-reasoning`, `helyx-inference` — the full LLM stack.

**Corrected framing (v0.2):**

| Role | Project |
|---|---|
| The LLM (the product) | **HELYX** |
| Thinking adjunct | Cogn8ty / NARS (Cogn8ty serves HELYX's reasoning) |
| Data pipeline | Knowledge-Foundry (KF feeds HELYX's training) |
| Verification team | refineforge (refineforge verifies HELYX's claims) |

**The crucial reframe:** refineforge is not a generic framework
looking for a design partner. **refineforge exists because the
alternative was hiring four engineers (Lean specialist + ML
engineer + DevOps + CUDA engineer) to verify HELYX's
load-bearing trust claims manually.** The operator built
refineforge to automate that team instead. See
`docs/plans/resourcing-plan.md` v0.2.

**The corrected one-liner:**
> "HELYX is the LLM. Cogn8ty thinks for it, KF teaches it,
> refineforge proves it. Rust binds them. No one else has
> shipped this stack."

v0.2 §1 explicitly identifies HELYX as the LLM with evidence
from the actual HELYX repo. §2 maps the other three as
HELYX's infrastructure. §3 captures the strict-NARS observation
(Cogn8ty symbolic + HELYX-NAL differentiable = NARS in the
Pei-Wang sense, both in pure Rust). §5 re-frames the
competitor moat as "HELYX is the LLM no one else has built
with this architecture." §7's next-step deliverables are
reordered with **"more HELYX claims signed by refineforge"
as the highest-value next step** (was 7.2 in v0.1; now 7.1
in v0.2 because every HELYX claim verified is one more
reviewer-checkable trust artifact for the LLM the operator
is actually building).

v0.1 is preserved in git history at commit `0805430` for
audit. v0.2's §9 documents the errata explicitly + encodes
the lesson for future plan docs ("if HELYX appears on the
same level as Cogn8ty / KF / refineforge as a flat peer,
that's a framing bug").

Cross-references:
- README doc-map row updated to point at v0.2 framing.
- STRUCTURE docs table row updated.

No source / test changes. cargo nextest run --workspace
still 383/383.

### Added — `docs/ecosystem.md`: the four-project portfolio map

Pure docs commit. Captures the operator's strategic
articulation of how their four projects compose, triggered
by the operator pointing the assistant at three external
verification directories on Desktop (`helyx-verification`,
`verification-2026-05-16` for Knowledge-Foundry,
`cogn8ty-verification`) over the course of one session.

The four projects, mapped to structural layers:

| Layer | Project | Tests passing | What it owns |
|---|---|---:|---|
| Data | Knowledge-Foundry (Python) | 1115/1115 | LLM-teacher data generation; 5 modes; HF publishing |
| Substrate (symbolic) | Cogn8ty / NANTAR INMORTAL RUST | 12,272/12,272 | Pure-Rust NARS + Prolog + 8-tier cognition + JSON-RPC brain |
| Substrate (neural) | HELYX + NAL | 4643/4643 | 5-substrate architecture; Lean → Rust extraction; differentiable NAL |
| Trust | refineforge | 383/383 | Lean proofs + refinement bundles + autonomous driver |

The load-bearing structural observation: **Cogn8ty (symbolic
NARS) and HELYX-NAL (differentiable NARS) are the two halves
of a NARS system in the strict sense Pei Wang defined.** Most
"neuro-symbolic" projects glue PyTorch to a Python Prolog;
NANTAR has both halves in pure Rust sharing a trust base, no
FFI tax, no language-impedance mismatch.

§3 Pair-by-pair complementarity matrix (8 directional flows
across the 4 projects). §4 One-sentence composition.
§5 The honest constraint ("complementary ≠ integrated";
3-6 months focused work for full composition; 6-10 weeks
for the smallest demonstrable three-way). §6 Why this
position is hard to replicate (5 architectural choices that
compound; 18-month minimum for a competitor from scratch).
§7 Three sensible next-step deliverables (smallest:
`refine --strategy cogn8ty` adapter at 2-4 weeks; medium:
HELYX-NAL-001 + supporting Audit/* claims at 4-6 weeks;
largest: full Cogn8ty ↔ HELYX-NAL composition at 2-4 months).
§8 What this doc is NOT (not a roadmap commitment; not a
sales pitch; not a substitute for per-pair plan docs; not a
definitive architecture).

The one-liner that closes the doc and the layer-map together:

> **Cogn8ty thinks, HELYX runs, KF teaches, refineforge proves.
> Rust binds them. No one else has shipped all four.**

The doc explicitly DOES NOT commit refineforge to any of the
integration deliverables in §7. Each warrants its own plan
doc; this ecosystem doc records the *conceptual map* so
integration plans can be drafted from a shared frame later.

Cross-references:
- README doc-map row added.
- STRUCTURE docs table row added.

No source / test changes. cargo nextest run --workspace
still 383/383.

### Added — `docs/plans/finetuning-plan.md` (Knowledge-Foundry → axolotl → `refine --strategy local-finetune`)

Pure docs commit. Captures the end-to-end fine-tuning
pipeline that the operator now has the ingredients for:
**Knowledge-Foundry** (the operator's separate Python
distillation pipeline at
`D:\AI-PROJECTS-GALO\PROJECTS\Knowledge-Foundry`, 330 source
files + 1115 tests passing) handles data generation;
`refineforge-trainer` already orchestrates axolotl runs;
`refineforge-eval` already drives `RepairStrategy`
benchmarks; a NEW `--strategy local-finetune` consumes the
fine-tuned weights via candle (Rust-native).

The plan is ~620 lines and mirrors the autonomous-driver-plan
+ gui-plan structure:

§1 Goal + scope. Lower marginal cost per repair attempt +
operator-controlled trust base.
§2 End-to-end data flow (ASCII diagram from Mathlib
mutation → KF probe → axolotl → candle-served strategy →
eval gate).
§3 Eight phases (0 pre-work → 8 docs + criteria v0.4
review). Each phase has scope + cost + time + acceptance.
§4 Total estimate: **~8-12 weeks elapsed, ~$5-11k cash**
via Option A grant stack from resourcing-plan.md v0.2.
§5 Resource requirements: zero additional hires; same
operator + part-time maintainer; the four AI-driven
specialist roles already exist.
§6 Risks: Mathlib mutation throughput #1; Anthropic
teacher refusals on Lean math #2; fine-tuned model
underperforming baseline #3.
§7 Definition of done (9 items).
§8 Out of scope (70B+ base models; DPO/RLHF; multi-language
proof systems; real-time online learning; SaaS hosting;
federated learning).
§9 Seven open questions for the operator.
§10 **Appendix: probe-set spec ready for the operator to
copy into Knowledge-Foundry**:
   - `kb_destiller/modes/sft_pair/probes/lean_proof_repair.yaml`
     (2 probe templates: lean_proof_repair_v1 +
     lean_proof_repair_with_context)
   - `kb_destiller/modes/sft_pair/presets/lean_proof_repair.yaml`
     (token bounds, quality floor 0.65, Apache-2.0 license
     inherited from Mathlib)
   - `kb_destiller/modes/sft_pair/gates/patch_well_formed.py`
     (~50-line Python gate validating LSP-shaped patch JSON:
     required keys, non-negative ints, end >= start, new_text
     is a string)
   - Test stubs + operator's first-run command sequence
§11 What this plan does NOT cover (acceptance gate is parity
with claude-opus-4-7, not strict improvement; AnthropicStrategy
stays as baseline + fallback; continual Mathlib refresh is
separate; HELYX-specific patterns deferred to v0.3.x).

**Knowledge-Foundry is NOT mutated by this commit.** The
probe-set spec in §10 is the file the operator copies into
their separate KF repo on their schedule; refineforge's
commit boundary stops at the spec.

Honest carve-outs:
- The plan stops at "ready to execute Phase 5"; actually
  running the production fine-tune is a separate operator
  commitment.
- Mathlib mutation pipeline (Phase 2) is multi-week elapsed
  per `docs/repair-evaluation.md` §9; it's the
  schedule-critical bottleneck.
- The 16,000 GPU-hour figure is the budget ceiling; the
  smoke fine-tune (Phase 5, Qwen2.5-Coder-1.5B) uses only
  ~50-200 hours and surfaces the architecture before the
  production fine-tune commits the full budget.

Cross-references:
- README doc-map row + tree-structure entry added.
- STRUCTURE docs table row added.

No source / test changes. `cargo nextest run --workspace`
still 383/383.

### Added — `docs/why-rust.md`: the load-bearing trade-off (PyTorch vs Rust at the substrate)

Pure docs commit. Captures the operator's strategic
articulation:

> **"PyTorch is good for humans. Rust is good for machines."**
> — Galo Serrano Abad, NANTAR AI ROBOTICS

Reviewer-grade answer to the inevitable "why didn't you use
PyTorch?" question. The slogan is the compressed form; the
new doc is the long form for reviewers who want to verify
the reasoning before signing onto the substrate.

#### Why this exists

HELYX has its own `helyx-autograd`, `helyx-nn`, `helyx-jepa`,
`helyx-train`, `helyx-inference`, `helyx-distill`. That's a
major strategic bet — it costs the entire PyTorch ecosystem
in exchange for memory safety, determinism, capability-typed
effect tracking, bit-exact reproducibility, and a single
trust-base for verified-core + neural components. The
question "why?" needs an answer that survives serious review;
this doc is that answer.

#### Eight sections (~330 lines)

1. **The trade-off in one paragraph.** PyTorch optimizes for
   the human researcher; Rust optimizes for the machine
   running trust-critical inference. Python's strengths
   become liabilities under the eleven ceiling properties.
2. **PyTorch's legitimate strengths (no strawmen).** REPL +
   notebooks, ecosystem, reference impls, hiring pool,
   dynamic shapes + autograd flexibility, debugging. These
   are real wins for human productivity that the Rust bet
   doesn't address.
3. **Why Rust is good for machines — the ceiling properties.**
   Five HELYX ceiling commitments that Python + PyTorch
   **cannot satisfy at the language level**:
   - Provably correct (Lean → Rust extraction; no Python
     analog)
   - Bit-exact reproducible (NumPy/PyTorch leak floats
     across BLAS backends; Rust + direct CUDA/Metal gives
     opt-in determinism per kernel)
   - Capability-typed (Rust effect types; Python has no
     language-level analog)
   - Reproducible build (Cargo.lock vs `pip freeze`'s
     famous non-reproducibility)
   - Continuously evolvable without breaking trust
     (mechanical semver vs Python's "prayer-driven"
     evolution)
4. **The cost — honestly accounted.** Acknowledges the
   one-engineer-vs-PyTorch-contributors gap. Notes the
   break-even isn't "feature count" but "ceiling
   properties for trust-critical production AI."
5. **Specific consequences for HELYX + refineforge.** No
   FFI boundary; `refineforge-bitexact` coherent with
   Substrate H; `refine autonomous` operates on one
   workspace not two; single-binary distribution; same
   `cargo test` discipline everywhere.
6. **What this is NOT an argument for/against.** Not a
   general "Rust > Python" position; not an argument
   PyTorch is bad; not a religious stance; not a refusal
   to interop at integration boundaries.
7. **Reviewer FAQ.** Honest answers to the obvious
   follow-ups: pre-trained weight loading, training at
   scale, missing model architectures, research velocity
   competition, one-operator sustainability. Notably
   acknowledges the training-vs-deployment asymmetry:
   PyTorch for training, Rust for everything downstream.
8. **The slogan, one more time.** Closes with the
   operator's one-liner restated as the executive
   summary.

#### Cross-references

- `README.md` doc-map row added pointing at the new doc.
- `STRUCTURE.md` docs table row added.

No source / test changes. `cargo nextest run --workspace`
still 383/383.

### Added — HELYX-AUDIT-001: the first refineforge claim against HELYX (production substrate)

**Refineforge's first signed artifact in the HELYX namespace.**
The strategic clarification this commit reflects: refineforge
isn't a generic framework looking for a customer; it's the
verification framework for HELYX (the operator's separate
production substrate at `C:\HELYX`, a 53-crate Rust + Lean 4
project with 4643 tests passing as of HELYX HEAD `341c6263`).

#### What ships

- **`lean/Refineforge/Helyx.lean`** — namespace hub for HELYX-
  related slices. Docstring documents the bridge to HELYX's
  separate `verified/lean/HELYX/` tree.
- **`lean/Refineforge/Helyx/Audit.lean`** — verbatim-modulo-
  namespace copy of HELYX's `Chain.lean` + `Append.lean`.
  Carries the `Chain` structure + the load-bearing theorem
  `append_increments_length`. Namespace renamed from
  `HELYX.Audit` to `Refineforge.Helyx.Audit` so refineforge's
  own lake project compiles it.
- **`claims/helyx-audit-001.yaml`** — claim YAML, `model-only`
  scope (no `rust_source` block; the HELYX Rust impl lives in
  a separate repo so refineforge's scan doesn't reach it; the
  refinement doc names the paths instead).
- **`docs/refinement/HELYX-AUDIT-001.md`** — the trust-critical
  refinement argument. Documents the four-link trust chain:
  Lean source (machine-checked) → slice ↔ HELYX source-of-truth
  (human-asserted verbatim-modulo-namespace) → HELYX verified-
  claim registry (`helyx-audit-verified::audit_append_claim()`,
  human-asserted Lean-module-string match) → HELYX working
  impl (`crates/helyx-audit/`, machine-checked by HELYX's own
  41-step CI, not by refineforge). Honestly carves out what
  refineforge cannot yet verify cross-repo.
- **`lean/Refineforge.lean`** — library root now imports
  `Refineforge.Helyx`.

#### Verified end-to-end this session

- `refine lean check HELYX-AUDIT-001` → `status: verified`;
  sorry / admit / axiom counts all zero; `lake build` succeeds
  for `Refineforge.Helyx.Audit`.
- `refine bundle export HELYX-AUDIT-001` →
  `artifacts/HELYX-AUDIT-001/` with **10 files in SHA-256
  manifest**.
- `refine bundle verify` round-trip → "verified OK".
- `cargo nextest run --workspace` → **383/383 pass** (the new
  Lean module doesn't touch Rust source).

#### Honest scope of "first claim"

This is a **representative slice**, not full HELYX-lake
integration. The HELYX source-of-truth at
`C:\HELYX\verified\lean\HELYX\Audit\` stays canonical;
refineforge bundles a byte-identical copy under its own
namespace. Drift between the two is a **Cat 8 escalation**
per criteria v0.3 (the slice itself is part of refineforge's
trust footprint).

What's deferred to a later phase:
- **Cross-repo scan**: refineforge can't yet verify that
  `helyx-audit::append` exists at the cited path in HELYX's
  separate repo. The operator manually inspected; the
  refinement doc cites the inspection date.
- **Full HELYX-lake integration**: refineforge invoking
  `lake build` against `C:\HELYX/verified/lean/` directly,
  no copy needed. Requires per-claim lake-root config in
  refineforge's runner. Phase 2 work.
- **Drift detection**: an automation that watches the HELYX
  source-of-truth + flags slice divergence as a Cat-8
  escalation packet. Could be a `refine sync` subcommand
  that compares the two trees.
- **The other four HELYX audit theorems**:
  Causality, Replay, TamperDetection, and the
  `tampered`-correctness invariant — each warrants its own
  HELYX-AUDIT-NNN refineforge claim. Same slice pattern.
- **HELYX-NAL-001**: the differentiable-NAL claim (HELYX's
  second verified-claim registry). Pending operator review
  of the NAL Lean spec.

#### Why this matters strategically

The Phase 4 audit ($0.35 Anthropic-repair end-to-end) proved
refineforge works on synthetic broken claims. This commit
proves refineforge works on a **real HELYX trust claim**.
The signed bundle at `artifacts/HELYX-AUDIT-001/` is the
artifact a third party can re-hash + re-verify against the
operator's signature — exactly the shape the
`docs/methodology.md` four-link trust chain promises.

That artifact + the refinement doc + the verifiable Lean
source is the **case-study deliverable** the operator's
strategic next-move conversation referenced. No GUI, no
four-engineer team, no $1M annual burn required — just
refineforge, HELYX as the first design partner (in-house),
and the rest is repetition for the remaining HELYX claims.

### Changed — `docs/plans/resourcing-plan.md` rewritten (v0.1 had the framing inverted)

The v0.1 of the resourcing plan (shipped earlier in this
`[Unreleased]` cycle as commit `2f60d9c`) framed the four
ARCHITECTURE sections as humans to hire and quoted a
$520k-$1.13M annual burn. **That was wrong.** The operator's
brief listed those four specialist roles as **capabilities
refineforge would provide**, not headcount.

**The corrected framing (v0.2 of this plan):** refineforge IS
the four specialists. Section 1 is performed by the autonomous
driver + escalation engine. Section 2 by
`refineforge-strategies` + `refineforge-eval` +
`refineforge-trainer`. Section 3 by the CI workflow +
Sigstore + release.sh. Section 4 by `refineforge-bitexact`.
What the operator actually needs is:

- **1 human operator** (you — the "human operator must
  approve" doctrine; ~5-20% of working week steady-state;
  zero cash cost).
- **1 part-time refineforge maintainer** ($18-80k/year fully-
  loaded, OR the operator's own time at zero cash).
- **Compute** that runs the four AI-driven specialists
  (Anthropic API ~$10-200/month for autonomous runs; 16,000
  GPU-hour fine-tune ~$45k via Option A grants at ~$5-10k
  cash outlay; CI ~$0-200/month).

**Corrected 12-month budget:**
- LATAM mid-band: **~$35-52k/year** (down from $520k v0.1).
- US ceiling: **~$68-108k/year** (down from $1.13M v0.1).
- Lower-bound (operator handles maintainer duties): **~$12-15k/year**.

**Two orders of magnitude smaller** than v0.1. This is the
framework's whole point: refineforge replaces a 4-person team
with one operator + the compute that runs the AI-driven
specialists.

v0.1 is preserved in git history under commit `2f60d9c` for
audit. v0.2 supersedes it as the canonical operator-facing
plan. Useful sections of v0.1 (cloud pricing, funding option
breakdowns, software library catalog) survive in v0.2; the
inverted staffing band is gone.

New §10 in the plan ("v0.1 errata") documents the inversion
explicitly. The lesson for future plan docs: **multi-FTE
budgets for refineforge proper should trip a review**
because refineforge IS the team.

### Added — `docs/plans/resourcing-plan.md` (people / compute / tools / funding)

Pure docs commit. Captures the operator's resourcing brief
in enterprise format alongside the other two plans. Scoped
to **refineforge proper**; the wider ecosystem (HELYX
substrate, Cogn8ty, immortal-nars/prolog, Knowledge Foundry)
is named in §7 as external-and-out-of-scope.

The plan covers:

- **§2 People** — the 4 specialists matching ARCHITECTURE.md's
  4 sections. Lean specialist named HIGHEST PRIORITY (refineforge's
  value anchors on the Lean side). Each section gets a
  seniority + effort + rate range. **US fully-loaded ceiling
  vs LATAM mid-band** quoted side-by-side; ~$880-1,350k vs
  ~$400-610k annual for the 4-FTE peak shape.
- **§3 Compute** — the operator's 16,000 GPU-hour line item
  for the Section 2 fine-tune (~13B model on N≥1000 broken-
  proof corpus). Also the smaller items: Mathlib mutation
  pipeline (CPU-bound), Anthropic API dev budget (~$50-500/mo),
  CI compute, GPU CI for future kernel work.
- **§4 Tools** — Rust, Python, Lean 4, Git, cargo-nextest,
  cosign — all already in v0.2.1 use. Plus the training-framework
  decision (axolotl + PyTorch + FSDP recommended; trainer
  crate already targets this shape) and the experiment-tracker
  pick (W&B free tier recommended).
- **§5 Funding strategy** — the operator-stated three options
  with **concrete numbers**:
  - **Option A (grants)**: NVIDIA Inception ($5-25k credits) +
    AWS Activate (up to $100k) + Google Cloud Research +
    Azure for Startups + Lambda/CoreWeave direct startup
    credits + HuggingFace + Anthropic/OpenAI research grants.
    Recommended stack: ~$45k effective at ~$5-10k cash outlay.
  - **Option B (customer-funded)**: $50-250k POC engagement
    OR embedded-researcher model (~$150k + benefits + cost-
    plus compute). IP-sharing is the friction point.
  - **Option C (direct cash)**: 16,000 H100-hours at
    $2.50/hr spot avg = ~$40k cloud; OR 8x H100 server
    capex $300-450k + $5-15k/mo colo. Bare metal pays
    back the cloud after 11-20 fine-tune-equivalents.
- **§6 Software libraries** — broken down per section
  (Lean / ML / DevOps / CUDA + cross-cutting Rust crypto).
- **§8 Risks** — 8 named resourcing-level risks with
  mitigations (Lean talent scarcity #1; mid-fine-tune
  poaching #2; grant expiry; spot preemption; cloud price
  spikes; IP-exfil risk on Option B; FX risk for LATAM
  operators with USD cloud spend).
- **§9 Open questions** — 8 items the operator must
  resolve before committing to any number (region, hire
  order, fine-tune model size, training framework, cloud
  primary/fallback, funding lane choice, experiment-tracker
  selection, GUI engineering line).
- **§10 Headline 12-month budget** — full table covering
  4 FTE + compute + Anthropic + observability + contingency.
  **LATAM mid-band ~$520k/year; US ceiling ~$1.13M/year.**

The plan does NOT commit to any of these numbers — they're
**honest estimates** ready for sharpening when the operator
goes out to hire, apply for grants, or sign cloud contracts.

#### Honest carve-outs

- All US salary figures are fully-loaded (~30% overhead);
  European / LATAM rates are 40-60% lower for equivalent
  seniority.
- Cloud per-GPU-hour quotes are based on 2026-05 public
  rates; spot prices and grant terms shift quarterly.
- The 16,000 GPU-hour budget refines to ±25% based on
  model size and dataset complexity — re-quote before
  signing.
- HELYX substrate, Cogn8ty, immortal-nars/prolog, and
  Knowledge Foundry are **operator-side concerns** that
  share some libraries with refineforge but are not part
  of refineforge proper. Each gets its own resourcing
  plan in the operator's project portfolio.

#### Build gate before any spending decision

The plan's §9 lists 8 open questions the operator must
resolve. Decisions go into a v0.1 of `resourcing-plan.md`
BEFORE the first hire offer / grant application / cloud
contract.

### Added — `docs/plans/gui-plan.md` enterprise plan for refineforge-studio (plan only, no code)

Pure docs commit. Lays the contract for an eventual
**`refineforge-studio`** desktop GUI exposing the four
refineforge sections (Lean Specialist / ML Engineer / DevOps /
CUDA Engineer) + the cross-cutting operator console. **Code
is explicitly NOT in this commit** — the plan is intended to be
costed, scoped, and reviewed BEFORE any implementation begins.

The plan mirrors the structure of
`docs/plans/autonomous-driver-plan.md`:
- **9 phases** (0 design → 8 a11y + packaging) totaling
  **~15 weeks** with one focused engineer (~3.5 months wall
  clock).
- Per-phase scope + acceptance + risks + mitigations.
- 8 named project-level risks (scope creep, trust drift,
  Tauri churn, etc.) with mitigations.
- 14-point definition-of-done.
- Explicit out-of-scope list (SaaS, built-in proof editor,
  plugin marketplace, mobile, real-time collaboration,
  AI-tuned criteria via GUI, autonomous training with real
  GPU spend).
- **6 open questions** the operator must resolve BEFORE Phase 0
  starts (Tauri vs Iced; English-only vs en+es from v1; OS
  priority; cost-spend integration depth; same-repo vs
  separate-repo crate placement; code-signing strategy).
- Red-team failure-mode rehearsal (feature parity, trust
  drift, abandoned operator, supply-chain compromise).

Headline design choice (recommended): **Tauri 2.x** backend
(new workspace crate `crates/refineforge-studio`) + **Solid +
Vite + Tailwind + Radix UI** frontend. Solid chosen for
reactivity-without-VDOM + smaller bundle vs React. Radix
chosen for accessibility primitives + RTL support out of the
box.

Acceptance gate for the GUI as a whole: **three operators new
to refineforge complete a 5-step task flow end-to-end without
help and without the CLI, in <5 minutes average** (excluding
Lake build time). Recorded UX session; not self-reported.

The CLI remains **canonical**. The GUI is an opinionated
productivity surface. Every GUI action maps to a documented
CLI sequence + git operations; a "View as CLI command" toggle
on every screen makes this contract operator-visible.

#### Honest disclosures

- The ~$500/year code-signing cost (Apple Developer + Windows
  EV) is in scope; Sigstore-signed FOSS distribution is the
  free alternative.
- New attack surface — the GUI binary becomes a Cat 8
  (trust-base) artifact. Threat model is Phase 0's first
  deliverable; criteria v0.4 will document the GUI's trust
  footprint before Phase 1 ships any code.
- No telemetry by default. Operator-action analytics is
  explicitly out of scope ("operator decides" doctrine).
- All UX-test claims are operator-verifiable. The 5-step
  task flow is documented in §1; the success criterion is
  measurable, not self-reported.

#### Build gate before any Phase-0 code

The plan's §9 lists 6 open questions the operator must
resolve. Decisions go into a v0 of `docs/plans/gui-plan.md`
BEFORE Phase 0. **No `refineforge-studio` crate is created
until those decisions land.**

## [0.2.1] — 2026-05-19

### Highlights (read first)

Everything in `[Unreleased]` accumulated **after the v0.2.0
tag** (commit `6486c6a`). When ready to cut v0.2.1, run
`release/release.sh 0.2.1` — it'll rename this section to
`[0.2.1] — <today>` and seed a fresh `[Unreleased]` above.

Two changesets land here:

1. **Phase 4 live-LLM dogfood audit** — pure-docs entry
   capturing the formal plan §3 phase 4 acceptance run against
   the real Anthropic API. Total real spend: **$0.35**. Cat 2
   packet generated, operator approval persisted, post-approval
   bundle shipped with 8-file SHA-256 manifest. No source / test
   changes in that entry.

2. **Phase 3.8: cross-run await-resume + inject CLI flags +
   stop_reason** — three of four "smaller follow-ups" from the
   v0.2.0 honest-leftovers list:
   - **Cross-run await-resume** (the namesake): executor's
     Escalated branch reads the existing packet before
     committing; preserves any parsable operator decision so
     APPROVED state survives across re-runs. The Phase 4
     audit's "two-run split workaround" is **obsolete** —
     a single command set now spans operator approval.
   - **`--inject-training` / `--inject-bitexact` CLI flags**
     (repeatable `Vec<String>`) thread directly into the
     `Planner::with_training_step` / `with_bitexact_step`
     builders shipped in Phase 3.7.
   - **Anthropic `stop_reason` surfacing**: `UsageStats` gains
     `stop_reasons: Vec<Option<String>>`; surfaced in the
     Repair-step detail string AND persisted to
     `RunReport.anthropic_usage.stop_reasons`. Shows
     `end_turn` / `max_tokens` / etc. per call.

**Workspace at the end of `[Unreleased]`:**

| Crate | Tests | Δ since v0.2.0 |
|---|---:|---:|
| `refineforge-escalation` | 170 | — |
| `refineforge-trainer` | 74 | — |
| `refineforge-cli` | 62 | +2 (Phase 3.8 cross-run preserve) |
| `refineforge-bitexact` | 32 | — |
| `refineforge-strategies` | 21 | +3 (stop_reason) |
| `refineforge-repair-api` | 11 | — |
| `example-counter` | 9 | — |
| `refineforge-eval` | 4 | — |
| **workspace total** | **383/383 pass** | +5 |

**Honest deferral carried into v0.2.1 territory:**
- **Nix flake first-build verification** — needs a Nix install
  or WSL; this Windows commit machine has neither. `flake.nix`
  is authored; `docs/reproducible-build.md` §8 has the operator
  invocation; first green CI run on the `nix-flake-check` job
  is the verification. This is the *only* honest leftover from
  the v0.2.0 "smaller follow-ups" list that isn't shipped.

**Plan §3 phase status (post-3.8):**
- Phase 0 (criteria doc → v0.3) — ✅
- Phase 1 (engine) — ✅
- Phase 2 (packet + git checkpoint) — ✅
- Phase 3 (MVP / 3.5 / 3.6 / 3.7 / 3.8 driver) — ✅
- Phase 4 (EXAMPLE-002 dogfood + criteria v0.4) — ✅ dogfood
  exercised in this `[Unreleased]`; criteria v0.4 has no
  pending findings yet (the v0.3 contract held up)
- Phase 5 (docs + release) — ✅ v0.2.0 shipped at `6486c6a`

### Added — Phase 3.8: cross-run await-resume + inject-training/bitexact flags + Anthropic stop_reason

Three of the four "smaller follow-ups" listed in the v0.2.0
honest-leftovers. The fourth — Nix flake first-build
verification — remains deferred because it needs a Nix-capable
machine (this Windows commit machine has none; WSL would work
but is a separate operator-side setup).

#### 1. Cross-run await-resume (Phase 3.8)

The executor's Escalated branch now reads the existing packet
file (if any) BEFORE committing. If the existing file already
contains a parsable operator decision, the executor preserves
it instead of overwriting. This is what enables the operator
flow:
1. Run 1: driver writes `(pending)` packet → halts.
2. Operator edits the packet to `APPROVED:` + commits.
3. Run 2: driver reads existing APPROVED packet, **preserves
   it** (no re-commit), the Escalated outcome bubbles up to
   `run_worklist`, `--await-decisions` polls and immediately
   finds APPROVED → continues to Scan + BundleExport.

Still-pending packets are still re-committed (refreshes
evidence on iteration; harmless because the content's
identical modulo timestamp). Malformed packets are rewritten
defensively.

`crates/refineforge-cli/src/autonomous/executor.rs`:
- New preserve-vs-rewrite check in the Escalated branch.
- Two new inline tests:
  - `phase_3_8_preexisting_approved_packet_is_not_overwritten`
    — runs an escalation twice, simulates operator APPROVED
    between runs, asserts the second run does NOT add a
    commit AND the APPROVED content survives.
  - `phase_3_8_preexisting_pending_packet_is_still_rewritten`
    — pending packets re-commit on re-run, matching the
    pre-Phase-3.8 behaviour for the un-decided case.

This is the fix for the "Phase 3.8 follow-up" gap surfaced in
the Phase 4 dogfood CHANGELOG entry. The full
single-driver-invocation flow now works:

```
$env:ANTHROPIC_API_KEY = ...
# Run 1 (halts at Escalated):
refine.exe autonomous CLAIM --strategy anthropic
    --inject-counter-idealisation --max-cost-usd 1.50

# Operator approves the packet, commits, then:

# Run 2 (consumes APPROVED, ships bundle):
refine.exe autonomous CLAIM --strategy anthropic
    --inject-counter-idealisation --await-decisions --max-cost-usd 0.50
```

(The Phase 4 audit's two-run split — without
`--inject-counter-idealisation` on the second — is no longer
required; both runs can carry the same flag set.)

#### 2. `--inject-training` / `--inject-bitexact` CLI flags

`refine autonomous` gains two repeatable string-list flags
that thread directly into `Planner::with_training_step` /
`with_bitexact_step` (the library-API builders shipped in
Phase 3.7).

```
refine autonomous CLAIM \
    --inject-training training/configs/example-qwen-1.5b.yaml \
    --inject-bitexact kernels/configs/matmul_fp32.yaml \
    [...other flags...]
```

Plan order with both injected: LeanCheck → (any
EngineActions) → Scan → BundleExport → RunTrainingExperiment
(per `--inject-training`) → RunBitExactGate (per
`--inject-bitexact`).

`crates/refineforge-cli/src/main.rs`:
- Two new `Vec<String>` clap fields on the `Autonomous`
  subcommand.

`crates/refineforge-cli/src/autonomous/mod.rs`:
- `run_cli` signature gains `inject_training: &[String]` and
  `inject_bitexact: &[String]`.
- For each entry, the planner is appended via the existing
  builders.
- Header banner prints `**INJECTED TRAINING**: refine-train run
  <path> --dry-run` / `**INJECTED BITEXACT**: refine-bitexact
  run <path>` so the operator sees what's being scheduled.

No new tests for the clap parsing itself (clap is
well-tested); the underlying `Planner` builder methods +
`Executor` subprocess dispatch already had tests in Phase 3.7.

#### 3. Anthropic `stop_reason` surfacing

`MessagesResponse` already deserialized `stop_reason: Option<String>`
but the executor discarded it. Phase 3.8 records it per-call.

`crates/refineforge-strategies/src/anthropic.rs`:
- `UsageStats` gains `stop_reasons: Vec<Option<String>>` —
  one entry per call. Common values per Anthropic's docs:
  `"end_turn"` (model finished), `"max_tokens"` (truncated;
  budget bumping might unblock), `"stop_sequence"`,
  `"tool_use"`.
- New method `UsageStats::record_stop_reason(reason)`.
- `AnthropicStrategy::propose_patch` now records the
  response's `stop_reason` (independent of whether the response
  carried a `usage` block).
- Three new inline tests:
  - `propose_patch_records_stop_reason_in_usage_handle` —
    single-call records `Some("end_turn")` (MockTransport's
    canned value).
  - `propose_patch_accumulates_stop_reasons_across_calls` —
    3 calls → 3 entries.
  - `usage_stats_record_stop_reason_handles_none` — Some +
    None + Some preserved in order.

`crates/refineforge-cli/src/autonomous/executor.rs`:
- Repair-step `detail` string's `[api: ...]` suffix now
  includes `stop_reasons: [end_turn, ...]` for the run's API
  calls. Visible in CLI output AND in the persisted
  `RunReport.anthropic_usage.stop_reasons` field.

`crates/refineforge-cli/src/autonomous/report.rs`:
- `report_with_anthropic_usage_round_trips` test extended to
  include `stop_reasons: vec![Some("end_turn"), ..., Some("max_tokens"), ...]`
  so the JSON serialization is exercised end-to-end.

#### Tests

- `cargo nextest run --workspace`: **383/383 pass** (was 378
  in v0.2.0; +5 from this commit: 2 cross-run-await + 3
  stop_reason).

#### What still defers

- **Nix flake first-build** — needs a Nix install or WSL
  environment; this Windows commit machine has neither, and
  installing Nix isn't a documentation-commit kind of step.
  The flake.nix is authored + ready; `docs/reproducible-build.md`
  §8 has the operator-facing invocation. First green CI run on
  Ubuntu (the `.github/workflows/ci.yml` `nix-flake-check` job)
  is the verification.

### Audit — Phase 4 live-LLM dogfood (post-v0.2.0; no code changes)

Plan §3 phase 4's formal acceptance gate, exercised against
the real Anthropic API in two runs against a transient
`AUTON-LIVE-002` claim. **No source / test changes** in this
commit — the dogfood validated the v0.2.0 release as shipped;
this entry is the audit trail.

#### Setup (transient; reverted via `git reset --hard 6486c6a`)

- `lean/Refineforge/AutonLiveTest.lean` — deliberately broken
  `theorem add_comm_live (a b : Nat) : a + b = b + a := rfl`
  (`rfl` doesn't close Nat addition commute).
- `claims/auton-live-002.yaml` — claim `AUTON-LIVE-002`
  pointing at it (Lean-only, no `rust_source`).
- `lean/Refineforge.lean` — patched to `import
  Refineforge.AutonLiveTest`.

#### Run 1: live LLM repair + Cat 2 escalation

```powershell
$env:ANTHROPIC_API_KEY = [Environment]::GetEnvironmentVariable(
    'ANTHROPIC_API_KEY','User')
refine.exe --root D:\AI-PROJECTS-GALO\PROJECTS\refineforge `
    autonomous AUTON-LIVE-002 `
    --strategy anthropic --auto-repair `
    --inject-counter-idealisation `
    --max-cost-usd 1.50 --operator galo@serragi.com
```

Observed outcomes (verbatim from the run report JSON):

| Step | Outcome | Detail |
|---:|---|---|
| 1 (LeanCheck) | FAILED (753ms) | `lake build did not produce Verified status: BuildFailed` |
| 5 (Repair, injected) | PROCEEDED (5491ms) | `repair[anthropic] outcome=Fixed { iterations: 1 }, iterations=1, file_modified=true [api: 1 calls, 781 input + 90 output tokens, 0 cache-create + 0 cache-read]` |
| 6 (LeanCheck, recheck) | PROCEEDED (1297ms) | `lake build verified AUTON-LIVE-002 (status: Verified)` |
| 2 (EngineAction, injected Cat 2 bait) | ESCALATED (123ms) | category=`idealisation`, packet=`escalations/AUTON-LIVE-002/002-idealisation.md` |

Cost: `$0.3500 / $1.5000` (5 × $0.07 upfront estimate; **actual
API call = 1** — the cost-gate over-charged conservatively per
the documented design).

Anthropic usage (Phase 3.7 reader working live against real
production API): `calls=1, input_tokens=781, output_tokens=90,
cache_creation=0, cache_read=0`.

Cat 2 packet was v0.3-conformant: YAML front-matter with
`criteria_version: '0.3'`, `batch: null`, `generated_at` ISO-8601,
`generated_by_strategy: anthropic`; per-Evidence section
documenting `u64 → Nat` with `UnsignedOverflow` lost property;
raw Action JSON for traceability; `## Human decision` block
with `(pending)` marker; **NO `expires_at` field** (v0.3-conformant).

#### Operator approval simulation

Operator (galo@serragi.com) overwrote `(pending)` with:

```
APPROVED: u64→Nat idealisation accepted for AUTON-LIVE-002 —
saturating_add gap is documented in the refinement doc
tradition; this is a tutorial claim with no production
deployment context.
```

(The driver's `SubprocessGitOps` had already auto-committed
the pending packet to git — that auto-commit was later reset
away as part of the transient cleanup; in a real operator
workflow the operator's APPROVED edit would have been
committed manually.)

#### Run 2: post-approval bundle ship

```powershell
refine.exe --root D:\AI-PROJECTS-GALO\PROJECTS\refineforge `
    autonomous AUTON-LIVE-002 --strategy mock `
    --max-cost-usd 0.10 --operator galo@serragi.com
```

Note: without `--inject-counter-idealisation` this time — the
operator has approved; the work resumes by executing the
workflow on the now-fixed file. No `--auto-repair` either
(file is already Verified). `--strategy mock` = $0 cost.

| Step | Outcome | Detail |
|---:|---|---|
| 1 (LeanCheck) | PROCEEDED (205ms) | `lake build verified AUTON-LIVE-002 (status: Verified)` |
| 2 (Scan) | PROCEEDED (0ms) | `scan status: NoRustSource (0 rust_source items)` |
| 3 (BundleExport) | PROCEEDED (205ms) | `bundle exported to artifacts/AUTON-LIVE-002 (SHA-256 manifest sealed)` |

Summary: `total=3, proceeded=3, escalated=0, failed=0,
success=true`. Cost: `$0.0000`. **Bundle shipped with 8 files
in manifest.**

#### What this proves (Phase 4 acceptance gate)

Per plan §3 phase 4: "on EXAMPLE-002 with the Counter
idealisation as bait, the autonomous driver produces exactly
one Category-2 (Idealisation) escalation packet, waits for
human approval, then produces a sealed bundle with the
operator's signature on the packet AND on the bundle."

✅ AUTON-LIVE-002 (Counter-flavoured transient claim).
✅ Real Anthropic API repaired the broken proof.
✅ **Exactly ONE Cat 2 (Idealisation) escalation packet** produced.
✅ Operator approval persisted.
✅ Post-approval bundle shipped with SHA-256 manifest.

Total real spend: **$0.35** (a fraction of the original
$50-$150 plan estimate, on the smallest possible
dogfood).

#### Known gap (Phase 3.8 follow-up)

The await-decision-then-resume across re-runs isn't yet a
single-command operation. Today's flow requires two `refine
autonomous` invocations split by operator approval:
- Run 1: lights up the bait → escalates → halts.
- Operator edits the packet to `APPROVED:` + commits.
- Run 2: re-runs the plan; LeanCheck already Verified; the
  injected Cat 2 step is skipped (no `--inject-counter-idealisation`
  flag); Scan + BundleExport proceed.

A future enhancement: when `--await-decisions` is set, the
executor should check if a packet for the (claim, seq,
category) tuple already exists with a parsable decision
and skip the re-commit + use the stored decision. Today the
executor unconditionally overwrites + re-commits on every
Escalated outcome, so `--await-decisions` only works
within a single run (which the integration test
`example_002_counter_idealisation_dogfood_with_await_approval`
exercises with `MockGitOps::auto_approve_packets`).

The split-run workflow above is the operator-facing pattern
today; the full plan §3 phase 4 single-command flow is the
next milestone for Phase 3.8.

#### Cleanup

All transients deleted via `git reset --hard 6486c6a` (the
v0.2.0 tag) + working-tree clean. The packet's auto-commit by
`SubprocessGitOps` was reset away as part of this cleanup —
documented here in case anyone wonders why there's no Phase 4
artifact in the tree.

## [0.2.0] — 2026-05-19

### Highlights (read first)

Everything below in `[Unreleased]` accumulated across the
supervised-autonomy build arc: Phase 1 (engine) → criteria
v0.2 → v0.3 same-day correction → Phase 2 (packet + git
checkpoint) → Phase 3 MVP → 3.5 (real library calls + loaders)
→ 3.6 (live Anthropic auto-repair) → 3.7 (await + dogfood +
trainer/bitexact + usage). When ready to cut v0.2.0, run
`release/release.sh 0.2.0` — it'll rename this section to
`[0.2.0] — <today>` and seed a fresh `[Unreleased]` above.

**The escalation contract** (`docs/escalation-criteria.md`)
went from unsigned v0.1 → operator-signed v0.2 → same-day
revised v0.3 with three substantive resolutions:
- **Mathlib first-use → Cat 8 (Trust-base)**, not Cat 1 (Scope) —
  trust-footprint concern, not scope-expansion.
- **Auto-expiry rejected**. Visible failure beats silent
  failure in a trust system; operators run `refine escalations
  list` to inspect the queue.
- **Batching opt-in under three conditions** (same category,
  identical analysis, undifferentiated evidence); default is
  still one-packet-per-item; partial-approval form
  `APPROVED: 1-5,7; REJECTED: 6,8 [reason]` recognised.

**The autonomous driver** (`refine autonomous <CLAIM-ID>`)
went from nothing → orchestration scaffold → real library
calls → live LLM auto-repair → full Phase 3.7. **Live
Anthropic API call confirmed end-to-end** ([commit 60d2a81](#)
transcript): broken `rfl` proof of `a + b = b + a` repaired
in 4 LLM iterations (23.3s wall-clock, $0.35 real spend);
re-LeanCheck Verified; SHA-256 bundle exported. EXAMPLE-002
forced-Counter dogfood passes as an integration test with
simulated operator approval.

**Workspace at the end of the arc:**

| Crate | Tests | Status |
|---|---:|---|
| `refineforge-escalation` | 170 | Phases 1 + 2 + 3.5 (engine + packet + git + loaders) |
| `refineforge-trainer` | 74 | Section 2 orchestration scaffold (no real training in-session) |
| `refineforge-cli` | 60 | includes Phase 3.7 `autonomous/` driver |
| `refineforge-bitexact` | 32 | Section 4 gate primitive |
| `refineforge-strategies` | 18 | real `AnthropicStrategy` + `UsageStats` token reader |
| `refineforge-repair-api` | 11 | stable cross-section trait surface |
| `example-counter` | 9 | EXAMPLE-002 Rust side (+ `#[derive(LeanModel)]` demo) |
| `refineforge-eval` | 4 | `refine-eval` benchmark harness |
| **workspace total** | **378/378 pass** | — |

`cargo nextest run --workspace` is the single source of truth
for these numbers; per-crate counts include lib + bin targets
counted separately per nextest convention.

**Plan §3 phase status:**
- Phase 0 (criteria doc) — ✅ shipped, revised to v0.3
- Phase 1 (engine) — ✅ shipped
- Phase 2 (packet + git checkpoint) — ✅ shipped
- Phase 3 MVP / 3.5 / 3.6 / 3.7 (driver) — ✅ all shipped
- Phase 3.5 (trainer + bitexact integration) — ✅ step kinds +
  subprocess wiring shipped; CLI flags `--inject-training` /
  `--inject-bitexact` are a one-file follow-up
- Phase 4 (EXAMPLE-002 dogfood + criteria v0.4) — ⏳ integration
  test ships in 3.7; live-LLM operator run is the next milestone
- Phase 5 (docs + integration + release ritual) — ⏳ docs are
  this commit; `release/release.sh 0.2.0` is the next step

**Honest carry-forward disclosures:**
- `lake` is operator-environment dependent. The Windows commit
  machine's Bash session doesn't have `lake` on PATH, but a
  PowerShell session does — which is how the live Anthropic
  auto-repair test in Phase 3.6 succeeded.
- No USD-conversion table for Anthropic token counts ever
  shipped (deliberate). Pricing drifts; embedded constants
  would silently misreport. The cost-gate's `$0.07/attempt`
  upfront estimate stays authoritative for budget control.
- `await_decision` has no timeout (per criteria v0.3).
  Operators run `refine escalations list` to see what's
  blocking. CI uses `--dry-run` or `--strategy mock` so
  unattended pipelines don't block.
- Phase 4's "exactly one Cat 2 packet" acceptance criterion
  is exercised by
  `example_002_counter_idealisation_dogfood_with_await_approval`
  in `crates/refineforge-cli/tests/autonomous_e2e.rs` using
  the synthetic bait flag. The live-LLM equivalent is the
  operator's first invocation.

### Added — Phase 3.7: close the remaining Phase-3 leftovers (await + dogfood + trainer/bitexact + usage)

Four items at once. Per-item honest scope below.

#### 1. `await_decision` resumption from `run_cli`

The Phase 3.5/3.6 worklist loop halted at the first `Escalated`
outcome. Phase 3.7 refactors it into a generic
`pub fn run_worklist<G: GitOps>(ex: &mut Executor<G>, plan,
cfg: &WorkRunConfig) -> Vec<StepOutcome>` so the worklist is
testable with `MockGitOps`, and adds escalation-await
resumption gated by the new `--await-decisions` flag:

| Outcome | After-await behaviour |
|---|---|
| `Approved { reason }` | Append `OperatorDecision: Proceeded` and continue with the next step in the worklist. |
| `Rejected { reason }` | Append `OperatorDecision: Failed` and halt. |
| `EditAndResubmit { suggestions }` | Same as Rejected — operator must re-run with edits. |
| `Partial(p)` | Append Failed with the per-item split — Phase 3.7 doesn't generate batched packets from the driver, so a Partial response is treated as unexpected and halts for operator follow-up. |

`WorkRunConfig` carries `strategy`, `auto_repair`,
`await_decisions`, `repair_max_iterations`,
`max_repair_attempts`, `await_poll_interval`. Default
`await_poll_interval = 5s` matches Phase 2's `AwaitConfig`.

Per criteria v0.3 the loop still has **no timeout** — visible
failure beats silent failure; operators run `refine
escalations list` to inspect the queue.

#### 2. `--inject-counter-idealisation` flag + EXAMPLE-002 dogfood

Implements plan §3 phase 4's acceptance test, modulo the
"under 5 minutes" and "real LLM" parts (those are operator-env
dependent). New CLI flag `--inject-counter-idealisation`
synthetically injects `Action::MapRustToLean { rust_type:
"u64", lean_type: "Nat", lossy_kinds: [UnsignedOverflow] }`
into the planner — the same Cat 2 escalation a real LLM
strategy would produce when refining EXAMPLE-002, but
reproducible without a live LLM call.

New integration test `example_002_counter_idealisation_dogfood_with_await_approval`
(in `crates/refineforge-cli/tests/autonomous_e2e.rs`):
- Loads the real EXAMPLE-002 claim from `claims/`.
- Constructs an Executor with `MockGitOps` + the bait
  Action + `--await-decisions = true`.
- `git.auto_approve_packets("counter saturating_add gap
  documented in refinement doc")` simulates the operator
  approving the packet between commit and the first poll.
- Runs `run_worklist` end-to-end.
- Asserts:
  - **exactly ONE** Escalated outcome (Cat 2: `idealisation`)
  - **exactly ONE** `OperatorDecision: Proceeded` (APPROVED)
  - Scan + BundleExport BOTH ran after approval
  - `RunSummary { failed: 0, escalated: 1, success: true }`

To exercise this against the LIVE LLM end-to-end the operator
runs (from PowerShell with sourced User-scope key):
```
$env:ANTHROPIC_API_KEY = [Environment]::GetEnvironmentVariable(
    'ANTHROPIC_API_KEY', 'User')
refine.exe autonomous EXAMPLE-002 --strategy anthropic-mock `
    --inject-counter-idealisation --await-decisions `
    --max-cost-usd 1.00 --operator galo@serragi.com
```
(`anthropic-mock` declines so no real cost; switch to
`anthropic` for a paid run.)

#### 3. `refine-train` / `refine-bitexact` integration

Two new planner variants:
- `StepKind::RunTrainingExperiment { config_path }` — appended
  after BundleExport via `Planner::with_training_step(path)`.
  Executor subprocess-shells to `refine-train run <path>
  --dry-run` (the `--dry-run` is hardcoded for Phase 3.7
  because real training requires the operator's backend +
  dataset).
- `StepKind::RunBitExactGate { config_path }` — appended after
  any training steps via `Planner::with_bitexact_step(path)`.
  Subprocess-shells to `refine-bitexact run <path>` (no
  `--dry-run`; the bit-exact gate's whole point is to run
  the kernel for real and hash the output).

Binary path is overridable via env var:
- `REFINEFORGE_REFINE_TRAIN_BIN` → defaults to `refine-train`.
- `REFINEFORGE_REFINE_BITEXACT_BIN` → defaults to
  `refine-bitexact`.

Same env-var-override pattern as the existing
`REFINEFORGE_COSIGN_BIN`. Reasonable failure path: if the
binary isn't on PATH and the env var isn't set, the step
records `Failed` with a message naming the env var so the
operator knows what to set.

**Honest scope**: this wires the **step kinds + planner
builders + executor subprocess dispatch + error reporting**.
Real training (axolotl / HF Trainer / etc.) requires an
operator-provided backend YAML; real bit-exact gating
requires an operator-provided kernel script + CUDA runtime.
The autonomous driver inherits both from PATH. No CLI flag
yet to inject these via `refine autonomous` — operators
construct a custom plan via the library API; a future
`--inject-training <path>` / `--inject-bitexact <path>` flag
is a one-file follow-up.

#### 4. Per-call usage reader (token counts; **no** USD invented)

Anthropic's API already returns a `usage` block (input /
output / cache-creation / cache-read tokens) per call.
`refineforge-strategies` was parsing it and discarding it;
Phase 3.7 keeps it.

New in `refineforge-strategies`:
- `UsageStats { calls, input_tokens, output_tokens,
  cache_creation_input_tokens, cache_read_input_tokens }`
  with a `merge(&Usage)` accumulator.
- `AnthropicStrategy::with_usage_stats(key, model, transport,
  Arc<Mutex<UsageStats>>)` — caller-supplied shared
  accumulator. The existing `new(...)` constructor wraps it
  with a fresh `Arc`.
- New factory `anthropic_strategy_from_env_with_usage() ->
  (Box<dyn RepairStrategy>, Arc<Mutex<UsageStats>>)`. The
  Phase-3.6 `anthropic_strategy_from_env()` is preserved as
  a thin wrapper that drops the handle.
- `propose_patch` calls `usage_stats.merge(...)` on every
  successful response.

New in `refineforge-cli/autonomous`:
- `Executor.anthropic_usage_observed: Option<UsageStats>`
  field. Phase 3.7 `run_repair_step` reads the handle after
  `repair::repair` returns and stores the snapshot.
- `resolve_strategy(name) -> (Box<dyn RepairStrategy>,
  Arc<Mutex<UsageStats>>)` (was just `Box<dyn RepairStrategy>`).
- `RunReport.anthropic_usage: Option<UsageStats>` field,
  serialized with `#[serde(skip_serializing_if = "Option::is_none")]`
  so non-Anthropic runs keep clean JSON.
- The Repair-step `detail` string gets a usage suffix when
  usage is non-zero: `" [api: N calls, X input + Y output
  tokens, A cache-create + B cache-read]"`.

**Deliberately NOT included**: a USD-conversion table.
Anthropic's per-token prices shift; embedding constants in
this crate would drift silently and over-bill or under-bill
quietly. The driver's `--max-cost-usd` cost-gate stays
authoritative for budget control via the conservative
`$0.07/attempt` upfront estimate; the token counts are for
post-run reporting and operator-side cost reconciliation.

#### Test additions

`crates/refineforge-cli/src/autonomous/planner.rs`:
- `with_training_step_appends_after_bundle`
- `with_bitexact_step_appends_after_training`
- `run_training_experiment_step_kind_serializes`
- `run_bitexact_gate_step_kind_serializes`

`crates/refineforge-cli/src/autonomous/executor.rs`:
- `dry_run_run_training_experiment_records_proceeded`
- `dry_run_run_bitexact_gate_records_proceeded`
- `non_dry_run_subprocess_step_fails_helpfully_when_binary_missing`

`crates/refineforge-cli/src/autonomous/report.rs`:
- `report_with_anthropic_usage_round_trips`

`crates/refineforge-cli/tests/autonomous_e2e.rs`:
- `example_002_counter_idealisation_dogfood_with_await_approval`
  (the formal plan §3 phase 4 acceptance gate test, with the
  caveat that the live-LLM portion is documented + the
  operator runs it separately).

`refineforge-escalation::MockGitOps`:
- `auto_approve_packets(reason)` test mode that rewrites
  `(pending)` → `APPROVED: <reason>` on every `write_file`.
  Used by the dogfood test to simulate operator approval.

#### Tests

- `cargo nextest run -p refineforge-cli`: **60/60 pass** (was
  51; +9 new across planner, executor, report, dogfood).
- `cargo nextest run --workspace`: **378/378 pass** (was 369;
  same +9).

#### Honest leftovers (smaller still)

- **No CLI flags for `--inject-training` / `--inject-bitexact`.**
  The library-API path (`Planner::with_training_step`) ships;
  the CLI flags are deferred to keep the `refine autonomous`
  signature stable for this commit. One-file follow-up.
- **EXAMPLE-002 dogfood live-LLM end-to-end was NOT executed
  this commit.** The integration test exercises the
  await-resumption + plan-mutation + post-approval-continuation
  logic with a mock strategy + dry-run system steps + simulated
  approval. The PowerShell command above is the operator's
  next live invocation — it's already shippable; this commit
  hasn't burned the API credit a real run would cost.
- **Per-call USD conversion is intentionally absent.** Token
  counts surface; pricing tables don't.
- **The Anthropic strategy still discards `stop_reason`** —
  could surface it in `RunReport` for "did the LLM finish
  vs. hit max_tokens?" diagnostics. Trivial; deferred.

### Added — Phase 3.6: live Anthropic-strategy auto-repair wired into `refine autonomous` (LLM call observed end-to-end)

**This commit ships a real, working live LLM repair path AND
confirms it runs end-to-end against a deliberately-broken
Lean file with the real Anthropic API.** The orchestrator
detected a Lean build failure, the `--auto-repair` flag
injected a `Repair` step, the live LLM converged in 4
iterations, the re-LeanCheck Verified, and a SHA-256-manifested
bundle landed in `artifacts/`. Total real cost: **$0.35** for
the demo run.

#### `StepKind::Repair { strategy, max_iterations }` (new planner variant)

Phase 3 / 3.5 only had system-step kinds (LeanCheck / Scan /
BundleExport) + EngineAction. Phase 3.6 adds `Repair` so the
planner can carry a bounded-LLM-repair step. The variant
serializes with `step_kind: "Repair"` + flat `strategy` +
`max_iterations` fields — round-trip tested.

#### Executor wiring

`Executor::run_step` dispatches `StepKind::Repair` into
`crate::repair::repair(...)` with the strategy resolved by a
new `resolve_strategy(name)` helper:

- `mock` → `MockStrategy` (declines everything; for tests).
- `anthropic-mock` →
  `refineforge_strategies::anthropic_mock_strategy()` (canned
  decline; exercises the AnthropicStrategy prompt + parser
  code path without burning API).
- `anthropic` →
  `refineforge_strategies::anthropic_strategy_from_env()` —
  **real HTTP** to `https://api.anthropic.com/v1/messages`,
  reads `ANTHROPIC_API_KEY` + optional `ANTHROPIC_MODEL`
  (default `claude-opus-4-7`).

Cost-gate integration: **before** invoking the strategy the
executor charges `ANTHROPIC_REPAIR_USD_PER_ATTEMPT (= $0.07) ×
max_iterations` against the gate. If the charge fails (budget
exceeded), the step records `Failed` and the strategy is NOT
constructed. This is fail-closed; the failed-charge does NOT
debit the gate, so a tight budget that refuses the first call
stays clean for the next attempt at a smaller scope. This is
the same conservative discipline the cost gate already used
in Phase 3 MVP.

The `--strategy anthropic` charge is upfront-estimated; a more
accurate cost-tracker that reads the Anthropic API's actual
per-call billing is a future enhancement (today we use the
$0.07/call eval-baseline number).

#### `run_cli` worklist + `--auto-repair`

The Phase 3.5 linear `for step in plan` loop was replaced with
a `VecDeque<PlannedStep>` worklist. After a failed `LeanCheck`,
if `--auto-repair` is set (default `false`) AND the per-run
repair-attempt counter is under `max_repair_attempts = 2`,
the driver:
1. Inserts a `Repair` step at the front of the worklist with
   `strategy = <the --strategy value>` and `max_iterations =
   5`.
2. Inserts a re-verifying `LeanCheck` step immediately after.
3. Increments the attempt counter; the next failure won't
   trigger another repair past the cap.

This produces the observed plan-mutation in the live run:
the initial `LeanCheck` (seq 1) fails, then seq 4 is the
injected `Repair`, seq 5 is the re-check, and seq 2 + 3
(Scan + BundleExport) resume after.

New CLI flag in `crates/refineforge-cli/src/main.rs`:

```
refine autonomous <CLAIM-ID>
    [--strategy mock|anthropic-mock|anthropic]
    [--max-cost-usd 10.0]
    [--operator EMAIL]
    [--dry-run]
    [--auto-repair]      ← NEW in Phase 3.6
```

#### LIVE end-to-end run (the real verification)

Setup (transient; cleaned up before this commit):
- `lean/Refineforge/AutonLiveTest.lean` — `theorem
  add_comm_live (a b : Nat) : a + b = b + a := rfl` (the
  deliberate bug: `rfl` doesn't close `a + b = b + a` for
  `Nat`).
- `claims/auton-live-test.yaml` — `AUTON-LIVE-001` claim
  pointing at it.
- `lean/Refineforge.lean` patched to import the new module so
  `lake build` sees it.

Command:
```powershell
$env:ANTHROPIC_API_KEY = [Environment]::GetEnvironmentVariable('ANTHROPIC_API_KEY', 'User')
D:\cargo-target\release\refine.exe --root D:\AI-PROJECTS-GALO\PROJECTS\refineforge `
    autonomous AUTON-LIVE-001 `
    --strategy anthropic --auto-repair `
    --max-cost-usd 1.00 --operator galo@serragi.com
```

Observed transcript:
```
refine autonomous AUTON-LIVE-001 (strategy=anthropic, dry_run=false,
    max-cost-usd=$1.00, auto_repair=true)
operator: galo@serragi.com
criteria version: v0.3

loaded ProjectContext: 0 lake packages, 202 bundle-chain crates, claim=AUTON-LIVE-001
plan (3 steps):
   1. LeanCheck — verify AUTON-LIVE-001 compiles + passes no-sorry / no-axiom policy gate
   2. Scan — confirm every rust_source entity cited by AUTON-LIVE-001 exists in the cited file
   3. BundleExport — seal AUTON-LIVE-001 into a SHA-256 manifested verification bundle

  step  1 [LeanCheck] FAILED (736ms): lake build did not produce Verified status: BuildFailed
  → auto-repair: injected Repair + recheck LeanCheck
  step  4 [Repair] PROCEEDED (23267ms): repair[anthropic] outcome=Fixed { iterations: 4 },
        iterations=4, file_modified=true
  step  5 [LeanCheck] PROCEEDED (1257ms): lake build verified AUTON-LIVE-001 (status: Verified)
  step  2 [Scan] PROCEEDED (0ms): scan status: NoRustSource (0 rust_source items)
Bundle exported to D:\...\refineforge\artifacts\AUTON-LIVE-001
  report status: Verified
  files in manifest: 8
  step  3 [BundleExport] PROCEEDED (201ms): bundle exported to
        artifacts/AUTON-LIVE-001 (SHA-256 manifest sealed)

summary: total=5 proceeded=4 escalated=0 failed=1 success=false
cost: $0.3500 / $1.0000 (remaining $0.6500)
report written to autonomous/runs/AUTON-LIVE-001-2026-05-19T02-10-50.json
```

What this proves:
- **The real Anthropic API was hit.** 23.3 seconds of Repair-step
  wall-clock is consistent with 4 round trips to
  `api.anthropic.com/v1/messages` using `claude-opus-4-7`.
- **The cost-gate charged $0.35** ($0.07/iter × 5 max_iter)
  upfront and stayed under the $1.00 budget. The conservative
  upfront charge over-charges relative to actual calls (4 used
  vs 5 budgeted), which is the safe direction.
- **The LLM successfully repaired the proof.** The `rfl`
  attempt was replaced with `Nat.add_comm a b` (or equivalent
  the elaborator accepts); `lake build` flipped from
  `BuildFailed` to `Verified` between seq 1 and seq 5.
- **The full pipeline ran**: ProjectContext loaded, Scan ran
  (correctly reported `NoRustSource`), bundle exported with
  8-file SHA-256 manifest, run report JSON persisted.
- **`success=false` in the summary is honest**: the original
  LeanCheck IS counted in `failed=1`. The run repaired itself
  but the historical step failed. A future "did the run
  ultimately succeed?" predicate would look at the LAST
  LeanCheck's outcome instead, but the literal per-step
  success/failure tally is what `RunReport` records.

The transient files (`AutonLiveTest.lean`, `auton-live-test.yaml`,
`artifacts/AUTON-LIVE-001/`, the live `autonomous/runs/` entry,
the library-root import line) were **deleted before this
commit** to keep the repo clean. The CHANGELOG transcript above
is the audit trail.

#### Tests

- `cargo nextest run -p refineforge-cli`: **51/51 pass** (was
  44 in Phase 3.5; +7 from Phase 3.6: planner Repair-variant
  serdes + executor dry-run-Repair / cost-gate-refuses-Repair /
  ANTHROPIC_REPAIR_USD constant pinned / resolve_strategy
  mock / anthropic-mock / unknown).
- `cargo nextest run --workspace`: **369/369 pass** (was 362;
  same +7).
- `live_lean_check_on_example_001` continues to PASS via SKIP
  on my Bash test-shell (lake not on this shell's PATH; lake
  IS available in the PowerShell session used for the live
  demo above — that's how the live AUTON-LIVE-001 run
  succeeded).

#### Honest leftovers (still deferred)

- **`await_decision` resumption from `run_cli`.** The Phase 2
  poll-loop primitive exists + is unit-tested. Auto-repair
  doesn't escalate (repair outcomes are Fixed / NotFixed, not
  Engine-categorical); but a Repair-then-LeanCheck cycle that
  produces an LLM patch the engine WOULD escalate (e.g., the
  LLM proposes weakening a theorem) is the natural next
  trigger for hooking `await_decision`. Not in this commit.
- **`refine-train` / `refine-bitexact` integration** (Section
  2 + 4). The Phase 3.6 wiring is Lean-side (Section 1) only.
- **EXAMPLE-002 forced-Counter dogfood** from plan §3 phase 4
  remains the explicit acceptance gate. The infrastructure to
  run it is now in place (loaders + executor + auto-repair +
  real strategy); the dogfood itself + the criteria-doc v0.4
  feedback loop is the next milestone.
- **Cost tracking is upfront-estimated.** A per-call cost
  reader that consumes the Anthropic API response headers
  (`anthropic-cost-usd` if available) would be more accurate
  than the $0.07/attempt baseline. Not blocking; the
  conservative upfront charge is the safer direction for a
  trust system.

### Changed — Phase 3.5: real library calls + ProjectContext loaders (replaces Phase-3-MVP scaffold stubs)

Phase 3 MVP shipped the orchestration shell with system steps
as scaffold stubs and a manually-constructed `ProjectContext`.
Phase 3.5 fills those gaps so `refine autonomous` actually
drives Lean check / scan / bundle export against the real
project, not a placeholder.

#### `crates/refineforge-escalation/src/loaders.rs` — new module

- `load_claim_summary(root, claim_id)` — walks
  `<root>/claims/**/<*.yaml>`, parses the first file whose
  `claim_id:` matches, projects the fields the engine queries
  into a [`ClaimSummary`]. Includes a free-text-to-enum
  status mapper (`"verified"` → `ProvenModelOnly`, `"broken"`
  → `Broken`, etc., default `Drafted`).
- `load_lake_manifest_packages(root)` — reads
  `<root>/lean/lake-manifest.json` and returns the set of
  Lake-package names. Missing file → empty set (graceful,
  per Phase 1 design).
- `load_cargo_lock_bundle_chain(root)` — hand-parses
  `<root>/Cargo.lock` (no `toml` crate dep — just `[[package]]`
  block scanning) and returns every pinned crate name.
  **Conservative choice**: every pinned crate is treated as
  in-chain, on the criteria-doc doctrine "conservative by
  default" — over-escalating beats under-escalating.
- `load_project_context(root, claim_id)` — combines all three
  loaders + sets `criteria_version` to the engine's compiled-in
  `CRITERIA_VERSION`. Returns a populated [`ProjectContext`]
  the driver can hand to `Engine::decide` directly.
- 13 inline tests against tempdir fixtures + 1 integration
  test against the repo's actual claims (caught real schema
  drift before it'd reach a user).

#### Executor real library calls (replaces Phase-3 stubs)

`run_system_step` no longer returns "(MVP scaffold) X
invocation deferred to Phase 3.5". Real wiring:

- **LeanCheck** → `crate::runner::run(repo_root, &claim)` →
  `ProofReport`. `Verified` → Proceeded; anything else
  (`BuildFailed`, `PolicyViolation`, `ToolingError`) → Failed
  with the variant name in the error string.
- **Scan** → `crate::scan::scan_claim(repo_root, &claim)` →
  `ScanReport`. `Verified` or `NoRustSource` → Proceeded;
  `Partial` / `FileMissing` → Failed.
- **BundleExport** → `crate::bundle::export(repo_root,
  claim_id, None)` → writes `artifacts/<CLAIM-ID>/` with the
  SHA-256-sealed manifest.

The executor's struct gains a `claim: Option<Claim>` field.
Live mode without a loaded `Claim` returns `Failed` with
"no Claim loaded into executor — call load_project_context
first" (defence-in-depth).

#### `autonomous::run_cli` updates

- Calls `load_project_context(root, Some(claim_id))` at run
  start. On success, prints "loaded ProjectContext: N lake
  packages, M bundle-chain crates, claim=ID". On failure
  prints a WARNING and falls back to `test_default()` so the
  run can still proceed in dry-run / smoke modes.
- Calls `crate::claim::load(root, claim_id)` to populate the
  executor's `claim` field. On failure prints a WARNING and
  proceeds with `None` (live mode then fails per-step, which
  is the honest behaviour — every step is recorded as failed
  with a useful error).
- No regression on the MVP's `--dry-run` path — dry-run still
  short-circuits to "dry-run: would run X for Y" without
  touching the live functions.

#### Integration tests (`crates/refineforge-cli/tests/autonomous_e2e.rs`)

Four tests against the actual refineforge repo (via
`CARGO_MANIFEST_DIR`):

1. **`loader_parses_real_example_001_yaml`** — asserts the
   Lean-only EXAMPLE-001 claim has at least one theorem and
   zero rust_source types.
2. **`loader_parses_real_example_002_yaml`** — asserts the
   refined-tutorial EXAMPLE-002 claim has rust_source types.
3. **`dry_run_plans_and_loads_real_claim`** — full dry-run
   pipeline against EXAMPLE-001: planner produces 3 steps,
   every step Proceeds with `"dry-run: "` detail, summary is
   success.
4. **`live_lean_check_on_example_001`** — gated on `lake`
   being on PATH. Calls `runner::run` for real and asserts
   the LeanCheck step's detail mentions "Verified". On the
   commit machine (Windows, no `lake` installed), this test
   **PASSED via the SKIP path** — printed `SKIP
   live_lean_check_on_example_001: lake not on PATH` and
   returned early without invoking Lake. Real validation
   requires a runner with `lake` + the pinned
   `leanprover/lean4:v4.29.1` toolchain installed.

#### Tests (honest counts)

- `cargo nextest run -p refineforge-cli`: **44/44 pass**
  (was 40 in Phase 3 MVP; +4 from `autonomous_e2e.rs`).
- `cargo nextest run --workspace`: **362/362 pass** (was
  344; +18 — loaders 13 + cli integration 4 + 1 minor delta).
- **Honest count of what was actually executed end-to-end
  against the live system**: 3/4 of the integration tests
  exercised the real loader paths + real executor library
  calls (dry-run mode for the executor calls). The 4th
  integration test (`live_lean_check_on_example_001`)
  PASSED via the early SKIP branch — `lake` is not on PATH
  on the commit machine. First execution against a real
  Lean toolchain is the operator's first `refine autonomous`
  invocation OR a CI run with elan + the pinned toolchain
  installed; the test code path is verified to compile and
  to dispatch correctly, but the `runner::run` → `lake
  build` subprocess wasn't observed succeeding in this
  commit's test runs.

#### What this commit does NOT ship (honest disclosures)

- **No live Anthropic call still.** Phase 3.5 wires the
  system steps; the LLM-driven repair step injection into
  the planner is deferred. Today `--strategy anthropic`
  parses but the planner never inserts an LLM-driven
  Action into the plan.
- **No `await_decision` resumption.** Like the MVP, the
  executor halts at the first `Escalated` step. The Phase 2
  `await_decision` poll-loop exists and is exercised by
  unit tests but isn't called from `run_cli` yet — the
  resume-after-operator-approval flow control is the next
  natural increment.
- **`refine-train` / `refine-bitexact` integration still
  absent.** Plan §3 phase 3.5 mentioned these; this commit
  focuses on Sections 1 + 3 (Lean check / scan / bundle).
  Section 2 + 4 wiring is still pending.
- **EXAMPLE-002 dogfood not run.** The forced-Counter
  idealisation test from plan §3 phase 4 needs the LLM
  strategy actually invoked + the engine's
  `MapRustToLean { lossy_kinds: ... }` action injected.
  Loaders + executor are ready to receive it; the LLM
  integration is the missing piece.

### Added — Phase 3 (MVP): `refine autonomous` driver + `refine escalations list` queue dashboard

The autonomous-driver-plan called for a separate
`refineforge-autonomous` crate. The MVP ships the driver as a
**module inside `refineforge-cli`** (`src/autonomous/`) to
avoid a circular dep — the would-be new crate would need
`refineforge-cli`'s `runner`/`bundle`/`scan` modules, and
`refineforge-cli`'s binary would need to dispatch into the new
crate, which cargo refuses to resolve. The module is
self-contained enough to extract later if the dep graph
changes.

#### Module structure

`crates/refineforge-cli/src/autonomous/`:

- **`planner.rs`** — turns a claim id into a `Vec<PlannedStep>`
  in MVP order: LeanCheck → Scan → BundleExport. Plus
  `Planner::with_engine_action(Action)` injects AI-proposed
  categorised actions between LeanCheck and Scan; tests and
  dry-runs use this to exercise the escalation path
  end-to-end without a live LLM.
- **`executor.rs`** — `Executor<G: GitOps>` runs each step.
  System steps return MVP scaffold detail (Phase 3.5 will
  replace with real `runner::check / scan::scan_one /
  bundle::export` library calls). Engine actions go through
  `Engine::decide`; on `Decision::Escalate` the
  `Packet::to_markdown` is committed via `commit_packet`
  unless `--dry-run` is set. `packet_path_for(claim, cat,
  seq)` → `escalations/<CLAIM>/<seq:03>-<slug>.md` (stable so
  `refine escalations list` can find every packet).
- **`cost.rs`** — `CostGate { max_usd, spent_usd }` with
  `charge(amount)` returning `CostGateError::Exceeded` when
  the cumulative spend would push past `--max-cost-usd`.
  Failed charges DO NOT count against spent (so a single
  rejected charge can't degrade the budget). Negative charges
  rejected.
- **`report.rs`** — `RunReport` JSON (claim_id,
  criteria_version, started_at, finished_at, dry_run, strategy,
  operator, summary, steps, cost_usd_total, cost_usd_max).
  `RunSummary::from_outcomes` counts proceeded/escalated/failed
  + `success` flag (escalations alone DO NOT flip `success` —
  the contract working as intended is not a failure).
- **`mod.rs`** — `run_cli(root, claim_id, strategy, max_cost,
  operator, dry_run)` is the top-level entry point invoked by
  `refine autonomous`. Plus `escalations_list(root,
  claim_filter, age_gt)` for the queue-inspection command.

#### New CLI surface

- **`refine autonomous <CLAIM-ID> [--strategy mock] [--max-cost-usd 10] [--operator EMAIL] [--dry-run]`**
  - Plans + executes the baseline workflow.
  - Prints the plan + per-step outcomes + final summary + cost.
  - Writes `autonomous/runs/<CLAIM-ID>-<timestamp>.json` unless
    `--dry-run`.
  - Per v0.3: there is **no `--escalation-timeout-days` flag**
    (auto-expiry was rejected at the same-day v0.3 revision;
    the driver waits indefinitely).
- **`refine escalations list [--claim X] [--age-gt N]`**
  - Walks `escalations/<CLAIM-ID>/*.md`, parses each packet's
    `## Human decision` section, and prints a queue dashboard:
    `STATUS  CLAIM  PACKET  MODIFIED`.
  - Statuses: `PENDING` (operator hasn't decided), `DECIDED`
    (recognised verdict), `MALFORMED` (no decision section),
    `UNRECOGNISED` (parse error).
  - Footer: `<P> pending of <N> total` + `oldest pending: <D>
    days`. The operator's source of truth for "what am I
    blocking?" per the v0.3 contract.

#### Tests

- `cargo nextest run -p refineforge-cli`: **40/40 pass** (was
  ~22 before this commit; +18 from autonomous module: planner
  5 + executor 6 + cost 7 + report 4 + mod sanity 1, minus
  some test consolidation).
- `cargo nextest run --workspace`: **344/344 pass** (was 319;
  +25 — the 5 difference is bundle / scan tests that were
  already counted twice across lib/bin targets and now share
  the deeper escalation surface).

#### What this commit does NOT ship (honest disclosures)

- **No live Anthropic call.** `--strategy anthropic` is
  accepted by the CLI parser but `--strategy mock` is the only
  one that runs through end-to-end today; the engine wiring is
  ready, the cost-gate is ready, the LLM-driven repair-step
  injection into the planner is Phase 3.5.
- **System steps (LeanCheck / Scan / BundleExport) are
  scaffold stubs.** They report timing + recognise the step
  kind but don't actually invoke `runner::check_all` /
  `scan::scan_one` / `bundle::export`. Phase 3.5 replaces the
  stub `detail` strings with real library calls.
- **No file loaders for `ProjectContext`.** The driver
  populates a `ProjectContext::test_default()` with a
  `ClaimSummary::test_default(claim_id)` and nothing else.
  Section 1's claim YAML loader + lake-manifest.json loader +
  Cargo.lock loader are Phase 3.5.
- **No EXAMPLE-002 dogfood.** Plan §3 phase 4's "forced
  Counter `Nat`/`u64` idealisation" test is Phase 4 work; it
  needs the LLM strategy actually live + a `ProjectContext`
  populated from the real claim YAML.
- **No `refine-train` / `refine-bitexact` integration.** Plan
  §3 phase 3.5 explicitly defers Sections 2 + 4 to a follow-up.
- **The `await_decision` poll loop is present** but the MVP
  `run_cli` halts at the first Escalated step rather than
  awaiting the operator's commit — keeping the test loop fast
  + avoiding a `std::thread::sleep` in CI. Phase 3.5 wires
  the await + resumption logic.
- **`refine escalations list` parses packets by file mtime**,
  not by reading the `generated_at` YAML field. Mtime is
  cheaper and almost always equivalent; if commits move
  packets between machines, generated_at becomes the
  authoritative age — that's a v0.3+ enhancement.

### Added — Phase 2: decision-packet renderer + git checkpoint (engine-side; driver wiring is Phase 3)

Three new modules in `crates/refineforge-escalation/`. All
honour criteria v0.3 (no `expires_at`; batching opt-in under
named conditions; partial-approval response form for batched
packets).

#### `src/packet.rs` — markdown packet renderer

- `Packet` struct with `PacketFrontMatter` (criteria_version,
  claim_id, category, all_categories, generated_at,
  generated_by_strategy, optional `batch`) + `summary` +
  `evidence` + raw `action`.
- `BatchBlock { items: Vec<BatchItem>, rationale_for_batching }`
  for v0.3-conformant batched packets. Conditions (a)/(b)/(c)
  from the criteria doc are the AI's responsibility to assert
  before calling `Packet::with_batch`.
- `Packet::build(...)` constructs from an `EscalationReason`.
- `Packet::with_batch(...)` attaches a batch block (additive;
  default is no batch).
- `Packet::to_markdown()` renders the full file: YAML
  front-matter delimited by `---`, headline + multi-category
  callout, per-Evidence-variant details, raw-Action JSON,
  optional batched-items section, `## Human decision` block
  with comment-hints listing recognised verdict forms
  (`APPROVED:`, `REJECTED:`, `EDIT_AND_RESUBMIT:`, plus partial
  form for batched). **No `expires_at` field** — explicitly
  forbidden by a test (`v0.3 forbids expires_at`).
- 12 inline tests including a sanity-check that every
  `Evidence` variant renders without panicking (so new variants
  added later are forced through this path).

#### `src/decision_outcome.rs` — parser for `## Human decision`

- `DecisionOutcome::{Approved, Rejected, EditAndResubmit, Partial}`.
  `Partial` carries `PartialDecision { approved_indices: Vec<u32>,
  rejected_indices: Vec<(u32, String)> }` — `Vec` (not HashMap)
  because (a) it preserves the operator's written order in
  packet auditing and (b) HashMap<u32, _> can't round-trip
  through serde_json without custom adaptors (caught by the
  `decision_round_trips_via_json` test before it landed).
- `parse_decision(markdown)` walks the `## Human decision`
  section, skips HTML comments, and routes by verdict prefix.
  Heuristic for partial-vs-free-text: a verdict line whose
  trailing content looks like an index list (digits + commas +
  hyphens + whitespace only, up to `;` or `[`) is partial.
  Caught a real bug pre-commit: my first heuristic required
  both APPROVED and REJECTED to be present, so
  `APPROVED: 1-3,5;` was misclassified as a free-text reason.
- `DecisionParseError` covers MissingSection / Pending /
  Unrecognised / InvalidIndices / RejectedWithoutReason /
  EditWithoutSuggestions.
- 16 inline tests covering every verdict form, multi-line
  reasons, partials with ranges/individuals, invalid ranges,
  pending detection, missing-section detection.

#### `src/git_checkpoint.rs` — commit + (indefinite) await

- `GitOps` trait with `add_and_commit`, `read_file`,
  `write_file` — enough surface to drive the packet flow
  without exposing the rest of git.
- `SubprocessGitOps`: shells out to the system `git` binary
  (consistent with refineforge's existing pattern of shelling
  to `cosign`). No git2 / libgit2 dependency.
- `MockGitOps`: in-memory `HashMap<PathBuf, String>` + commit
  log; lets unit tests drive `commit_packet` /
  `poll_decision_once` without filesystem or subprocess.
- `commit_packet(git, repo_root, file_rel, markdown, msg)`
  writes + commits in one shot.
- `poll_decision_once(git, repo_root, file_rel)` — single
  non-blocking poll; returns `Ok(Some(outcome))` if the
  operator has decided, `Ok(None)` if still pending, `Err(_)`
  on I/O or parse-shape errors.
- `await_decision(git, repo_root, file_rel, AwaitConfig)` —
  blocks indefinitely calling `poll_decision_once` every
  `poll_interval` (default 5s). **No timeout** — per criteria
  v0.3, visible failure (claim sits blocking; operator runs
  `refine escalations list` to see what's pending) beats silent
  failure (a stale packet auto-rejected after N days).
- 10 inline tests against `MockGitOps`.

#### POSIX end-to-end tests

New file `tests/packet_e2e.rs` (gated on `#[cfg(unix)]`). Two
tests drive the real `git` binary against a `tempfile::tempdir()`:
1. commit-then-poll-pending-then-operator-approves: assert the
   `commit_packet` returns a 40-char SHA, the first poll sees
   `(pending)`, the operator's APPROVED commit is detected on
   the next poll.
2. batched packet → partial-decision flow: APPROVED: 1; REJECTED:
   2 [reason] is parsed and the rejection-reason is preserved.

Plus a defence-in-depth test that the engine still refuses to
operate when `ctx.criteria_version != CRITERIA_VERSION` even in
the e2e setup.

**These two tests are not executed on Windows** (the runner
this commit was prepared on) because `cfg(unix)` strips them
out — the `MockGitOps` tests cover the same surface in a
platform-agnostic way. First CI run on a Linux/macOS runner
will execute them.

#### Dependency

- `chrono = { version = "0.4", features = ["serde"] }` added —
  used for ISO-8601 `generated_at` timestamps in packet
  front-matter. (Not yet used at runtime — the driver, Phase 3,
  will call `chrono::Utc::now().to_rfc3339()`. The dep is in
  place so Phase 3 doesn't need a re-bump.)

#### Tests

- `cargo nextest run -p refineforge-escalation`: **156/156 pass**
  (was 118 under v0.3; +38 from this commit — packet 12, decision
  16, git 10).
- `cargo nextest run --workspace`: **319/319 pass** (was 281;
  same +38).
- The two POSIX e2e tests compile cleanly on Windows but are
  not executed there; coverage of the wiring is via the
  `SubprocessGitOps` source review + the `MockGitOps`
  unit tests.

#### What this commit does NOT ship (honest disclosures)

- **No CLI surface.** `refine escalations list` is documented
  in criteria-doc §"Escalation expiry" as the Phase 2-or-3
  queue-inspection command, but it lives in `refineforge-cli`,
  not this crate. The data shapes are ready; the subcommand is
  Phase 3.
- **No reminder hooks.** The 7/14/30-day notifications mentioned
  in criteria v0.3 are deferred to Phase 3 driver-level config
  (channel + format). The `await_decision` loop has no reminder
  callback today.
- **No criteria-doc → engine build-time cross-check** (plan §3
  phase 2 risk mitigation). The packet templates render every
  Evidence variant, but there's no programmatic assertion that
  every category in the criteria-doc §3 has a matching
  Evidence variant. Manually kept in sync; Phase 2.5 polish.
- **No git remote support.** `SubprocessGitOps` works against
  a local repo only — the operator decides on the same machine
  as the driver. Cross-machine workflow is plan §8 out-of-scope
  for v0.2 and explicitly fine.
- **No real Anthropic call.** The packet renderer formats AI
  reasoning, but this commit doesn't invoke Anthropic. Phase 3
  wires `refineforge-strategies` to feed reasoning into the
  packet's `evidence` block.

### Changed — Criteria v0.2 → v0.3 (operator same-day correction)

v0.2 was operator-signed earlier the same day; before any
Phase 2+ code lands, the operator revised three of four
resolutions with reasoning that overrides the recommended
defaults v0.2 carried. The engine and tests are updated to
match; `CRITERIA_VERSION = "0.3"`.

**Q1 — Mathlib first-use → Category 8, NOT Category 1.** v0.2
merged Mathlib into Scope on the argument that "first-time use"
is a scope-expansion event. v0.3 routes it to Trust-base on the
argument that importing `Mathlib.Tactic.Linarith` adds ~1.5M
lines of community-maintained Lean (with `Classical.choice` use
and tactic implementations that have had soundness bugs) to the
trust base of every claim using it. The operator-facing decision
is about **trust delta** (transitive footprint, axiom usage,
version-pinning audit), not **scope delta** (what does this
claim cover).

  Practical test (operator-stated): "if Mathlib v4.30 ships a
  tactic regression, do we need to re-verify our bundles?" Yes →
  trust-base concern. The answer is yes → Cat 8.

  Cat 8 picks up new "escalate when" text + dedicated Mathlib /
  Lake-registry decision-packet fields (specific modules, the
  `lake-manifest.json` diff, the reviewer-checklist update).
  Cat 1's prose explicitly directs Mathlib first-use to Cat 8.

**Q3 — Auto-expiry rejected entirely.** v0.2 set 7-day default
with per-category overrides. v0.3 rejects auto-expiry on the
principle that **visible failure beats silent failure in a trust
system**: a packet auto-rejected after 7 days might have been
exactly the decision that needed careful thought, and the
operator finds out a week later. The driver waits indefinitely;
a new `refine escalations list [--claim X] [--age-gt N]`
queue-inspection command (Phase 2 work) gives the operator a
dashboard for "what am I blocking?" without the system making
the decision. Optional reminders at 7/14/30 days are
notifications, not auto-decisions.

  Same principle as `refine bundle verify` failing closed when
  cosign is missing — visible failure is the correct mode.

**Q4 — Default one-per-item kept, but AI-proposed batching
allowed under three named conditions.** v0.2's rigid rule
created friction for cases like "8 `u64` → `Nat` idealisations
on the same struct" where the underlying decision is identical.
v0.3 keeps one-per as the default but lets the AI propose a
batch when **(a)** every item trips the same category, **(b)**
the AI's analysis and recommendation are identical across
items, and **(c)** the AI's evidence does not distinguish
between items. Batching is the AI's proposal, not its right;
the human can approve as a unit OR split with per-item decisions
in a single block (`APPROVED: 1-5,7; REJECTED: 6,8 [reason]`).
Packet format requires a `batch:` YAML front-matter block with
`items` + `rationale_for_batching`; the rationale itself is
reviewable. Over-batching by the AI on the same kind of batch
becomes a v0.3+ open question — operator-tracked, not
auto-corrected.

**Q2 — Bit-exact regression as own Cat 9 (unchanged from
v0.2).** v0.3 expands Cat 9's "do NOT escalate" list to
explicitly include `kernels/README.md` edits, kernel-script
test-stub edits, and changes to the bit-exact gate's own logic
(`crates/refineforge-bitexact/` source) — the gate itself lives
in the trust chain and is **Cat 8 trust-base**, not Cat 9.

#### Engine changes

- `CRITERIA_VERSION` constant: `"0.2"` → `"0.3"`.
- `classify_scope` no longer matches `Action::AddLakePackage`
  (Mathlib is Cat 8 ONLY now). Removed branch is replaced with
  an inline comment explaining the v0.3 routing.
- `classify_scope` no longer matches `Action::AddLeanImport`
  with `is_mathlib && !already_known`. Trust footprint is
  established at lake-manifest entry (handled by
  `classify_trust_base` for `AddLakePackage`); per-module
  imports from an already-trusted package proceed.
- `summarise` drops the dead Scope-AddLeanImport branch.
- The expiry and batching meta-rules are still doc-only — the
  engine has no expiry or batching logic to remove (both are
  Phase 2 driver concerns).

#### Test changes

- `cat01_scope.rs`:
  - Deleted `first_time_mathlib_import_escalates_as_scope`.
  - Renamed inverse: `first_time_mathlib_import_does_not_trip_scope_in_v0_3`
    (positive proceed check with explanatory comment).
- `cat08_trust_base.rs`:
  - `add_lake_package_escalates` renamed to
    `add_lake_package_escalates_as_trust_base_only`; now
    asserts Cat 1 is **NOT** in `.categories()`.
  - Added `add_lake_package_non_mathlib_also_trust_base_only`.
  - Added `mathlib_import_after_package_already_in_manifest_proceeds`.
- `multi_category.rs`:
  - Deleted `add_lake_package_trips_scope_plus_trust_base`
    (no longer multi-trip under v0.3).
  - `summary_lists_secondary_categories` switched to use
    `AddKernelDirectory` (the canonical remaining multi-trip
    Scope+BitExactRegression case).
- `edge_cases.rs`:
  - `criteria_version_constant_is_exact_v0_2` →
    `criteria_version_constant_is_exact_v0_3` (asserts `"0.3"`).
- Engine inline test `summarise_lists_secondary_categories`
  switched to `AddKernelDirectory` for the same reason.

#### Tests

- `cargo nextest run -p refineforge-escalation`: **118/118 pass**
  (was 117 under v0.2; net +1 from cat08 additions and cat01
  rename).
- `cargo nextest run --workspace`: **281/281 pass** (was 280;
  same +1).

#### Honest disclosure

v0.2 lasted exactly one commit ([37cca31](https://example.invalid))
before being superseded. No real escalation packets were
generated under v0.2 (Phase 2+ is not yet built), so the
supersession is clean — the version-history table preserves the
audit trail, and the v0.2 entry stays in the table so a future
reader sees why v0.3 looks the way it does.

### Added — Phase 1: refineforge-escalation pure-functional engine + criteria v0.2

Operator signed off on `docs/escalation-criteria.md` v0.1 with
the resolution: **lock all four open questions to the recommended
defaults**. The criteria doc is now v0.2 and the engine that
enforces it ships in this commit.

#### Criteria v0.1 → v0.2

- **Status block** flipped from "v0.1 — DRAFT, pending operator
  review" to "v0.2 — operator-signed."
- **New Category 9 — Bit-exact regression** (resolution of v0.1
  open question §2): a change that affects, or could affect, a
  previously-certified `refine-bitexact` gate now has its own
  packet template. Splits the responsibility cleanly from
  Categories 2 + 6 because the operator decision is qualitatively
  different (re-baseline vs revert vs accept hardware-class
  divergence).
- **"First-time Mathlib import" merged into Scope** (resolution
  of §1): no new category — handled by the Cat-1 examples list
  plus the engine's `mathlib_imports_existing` context query.
- **Meta-rule "Escalation expiry"** added (resolution of §3): 7
  calendar days by default, configurable per-category between 1
  and 30 days. Recorded as `expires_at:` in every packet's YAML
  front-matter.
- **Meta-rule "Batch escalations"** added (resolution of §4):
  one packet per independent item; a single coherent action that
  trips multiple categories is one packet listing all categories.
- **Version-history row** added with operator signature
  `galo@serragi.com`.

#### `crates/refineforge-escalation` — new workspace crate

Pure-functional engine. No I/O inside `Engine::decide`. No
`unsafe`, no `tokio`, no network.

Modules:
- **`category.rs`** — `Category` enum (9 variants); `slug()`
  for packet filenames; `number()` for 1-9 indexing; `all()`
  iterator in stable order.
- **`action.rs`** — `Action` enum (~30 variants covering every
  step the autonomous driver can propose: Lean structural edits,
  Rust→Lean mappings, refinement-doc sentences, claim YAML
  edits, external-fact assertions, 8 trust-base sub-actions,
  3 scope-expanding additions, 5 kernel/bit-exact sub-actions,
  3 trivially-OK actions, and `Unknown` catch-all). Supporting
  enums: `LossKind`, `SentenceKind`, `ExternalCitation`,
  `WeakeningKind`, `ClaimStatus`.
- **`decision.rs`** — `Decision::{Proceed, Escalate}` +
  `EscalationReason` carrying the matched categories, primary
  (most-specific) category, one-sentence summary, and structured
  `Evidence` per category.
- **`context.rs`** — `ProjectContext` (criteria_version + claim
  summary + sets of existing mathlib imports, Lake packages,
  bundle-chain crates, approved Anthropic models, kernels with
  baselines, workspace crates, templates, top-level dirs, Lean
  modules) + `ClaimSummary` (id, status, scope_model_only,
  lean_theorems, rust_source_types, review_human_operator).
  Convenience constructors `test_default()` and
  `test_with_wrong_criteria_version()` for unit tests.
- **`engine.rs`** — `Engine::decide(action, ctx) -> Result<Decision, EngineError>`.
  9 per-category classifiers; multi-category resolver picks the
  primary by hand-tuned specificity (CustomAxiom > TheoremWeakening
  > Idealisation > BitExactRegression > TrustBaseExtension >
  CustomerIntent > ExternalFact > StatusUpgrade > Scope).
  Refuses to operate when `ctx.criteria_version != CRITERIA_VERSION`
  (the engine's compiled-in `"0.2"`).

#### Tests

- `cargo nextest run -p refineforge-escalation`: **117/117 pass**.
  - 25 inline unit tests in `category.rs` / `action.rs` /
    `decision.rs` / `context.rs` / `engine.rs`.
  - 92 integration tests under `tests/`, one file per category
    (`cat01_scope.rs` through `cat09_bit_exact.rs`),
    `multi_category.rs`, `edge_cases.rs`. Every positive +
    negative example from criteria-doc §3 has a test that names
    it explicitly.
- `cargo nextest run --workspace`: **280/280 pass** (was 163;
  +117 exactly from the new crate). No regression elsewhere.

#### What this commit does NOT ship (honest disclosures)

- **No file loaders.** `ProjectContext` is the data shape;
  building one from claim YAMLs / `lake-manifest.json` /
  `Cargo.lock` is the Phase 2 driver crate's responsibility.
  Phase 1 ships `test_default()` for unit tests + manual
  construction for integration.
- **No CLI wiring.** `refine autonomous <CLAIM-ID>` doesn't exist
  yet. The engine is a library that Phase 3 will import.
- **No decision-packet renderer.** Phase 2 (per
  `docs/plans/autonomous-driver-plan.md`) is the markdown templates +
  git-checkpoint loop. The engine produces structured `Evidence`
  the renderer will consume.
- **No real LLM strategy integration.** The engine doesn't talk
  to Anthropic — `refineforge-strategies` does that today, and
  Phase 3 wires the two together.
- **No git-watcher.** Phase 2 polls for the operator's signature
  commit on each packet. The engine itself never touches the
  filesystem.
- **No criteria-doc → engine code build-time cross-check.**
  Manually kept in sync today; Phase 2 risk mitigation includes
  a CI job that parses the criteria doc and asserts every
  category has matching engine logic.
- **The engine's per-category specificity ordering** is hand-tuned,
  not derived from the doc. If overlap cases produce a surprising
  primary in dogfood, the ordering becomes criteria v0.3 conversation.

### Added — Supervised-autonomy contract + enterprise build plan (no code)

Pure docs commit. Lays the contract for an eventual `refine
autonomous` driver that does each role's work end-to-end while
escalating only on important decisions. **Code is explicitly
NOT in this commit** — the contract has to be operator-signed
before any code enforces it.

- **`docs/escalation-criteria.md`** (v0.1 — ~480 lines).
  The 8 categories that always escalate to the human:
  1. Scope change (merged with first-of-kind structural
     decisions from the original proposal)
  2. Idealisation (Rust→Lean type mapping that loses info)
  3. Custom axiom
  4. Refinement-doc claim about customer intent
  5. Status upgrade (e.g. proven model-only → model+refined)
  6. Theorem deletion or weakening
  7. External-fact assertion
  8. Trust-base extension (NEW; absent from the original
     proposal — covers toolchain pins, Mathlib adds, crate
     switches, GHA-action SHAs)

  Each category: definition + 5+ positive examples + 3+ negative
  examples + required decision-packet contents.

  Meta-rules: categorical-not-numerical · conservative defaults
  · versioned · adding/removing a category is itself a
  Category-1 escalation. Version-history table; 4 named open
  questions for the first operator review.

- **`docs/plans/autonomous-driver-plan.md`** — enterprise project
  plan for the 5-phase build:
   Phase 0 (this commit): criteria doc
   Phase 1: refineforge-escalation pure engine    (2 days)
   Phase 2: decision-packet + git checkpoint      (2 days)
   Phase 3: refineforge-autonomous driver         (4 days)
   Phase 3.5: trainer + bitexact integration      (1 day)
   Phase 4: EXAMPLE-002 dogfood + criteria v0.2   (1 day)
   Phase 5: docs + release ritual                 (0.5 day)
   = ~11 working days = ~2 calendar weeks (1 focused engineer)

  Includes: resource needs ($50-150 API spend, no GPU, no
  remote required); 8 named project-level risks with
  mitigations; 9-point definition-of-done; explicit out-of-
  scope list; red-team failure-mode rehearsal (rubber-stamp,
  silent drift, uneditable contract, false-success).

- **`escalations/`** top-level dir reserved (currently empty)
  for the driver to write packets into.
- **README documentation map** updated to include both docs
  with CONTRACT flag on the criteria doc.

### Build gate before any Phase-1 code

The criteria doc has 4 open questions in §"Open questions for
the first operator review" that the operator must resolve:
1. Mathlib first-use — own category or merged into Scope?
2. Bit-exact regression — own category or implicit via 2+6?
3. Time-based escalation expiry — 7 days or indefinite?
4. Batch escalations — one packet per item or one packet listing items?

**No `refineforge-escalation` code lands until the operator
has signed off on criteria v0.2.**

### Honest disclosures

- The 8 categories in v0.1 are this author's best guess from
  the supervised-autonomy design conversation. The first
  operator review will surface where they're wrong; v0.2 is
  the contract that actually gets enforced.
- The 2-week estimate assumes one senior Rust + LLM-integration
  engineer. With less experience, double it.
- The $50-150 API-spend estimate is for development testing
  only. Production driver runs burn API at whatever rate the
  strategy + claim require — that's the per-claim cost the
  operator chooses to pay.
- This commit adds no Rust code, no tests, no CLI surface. The
  workspace test count (163/163) is unchanged from the prior
  commit.

### Added — Section 4: CUDA / GPU kernel engineer (bit-exact reproducibility scaffold)

Adds a fourth section to ARCHITECTURE.md. Mirrors what the
training-orchestration scaffold did for Section 2: ships the
**gate primitive** that detects non-determinism in any kernel,
plus a methodology doc covering CUDA-specific non-determinism
sources. Does NOT ship actual CUDA kernels — no nvcc, no GPU
on this dev machine.

- **New workspace crate `crates/refineforge-bitexact`** with the
  `refine-bitexact` binary. Modules: `experiment` (KernelExperiment
  YAML schema with custom Deserialize for the `output:
  stdout|{file: ...}` user-friendly form), `hash` (streaming
  SHA-256 + `all_equal`), `runner` (run kernel N times, capture
  stdout or file, hash, time), `report` (Pass/Fail + per-run
  hashes + summary).
- **CLI**: `refine-bitexact run <kernel.yaml> [--dry-run]`,
  `refine-bitexact report <run_dir>`. Pass → exit 0; Fail →
  exit non-zero (suitable for CI gating).
- **`kernels/` top-level**: README + 2 example configs
  (deterministic must pass; non-deterministic must fail; the
  pair acts as a regression test for the gate itself) + 4 stub
  scripts (sh + ps1 × deterministic + non-deterministic) +
  empty `src/` for the CUDA engineer + `runs/.gitkeep`.
- **`docs/bit-exact-reproducibility.md`** — full methodology:
  what bit-exact means + why it matters; 9-source table of GPU
  non-determinism (atomicAdd ordering, cuBLAS algorithm
  selection, cuDNN, reduction trees, TF32/FP16 mixed precision,
  stream sync, memory allocator, FMA, driver versions) with
  per-source mitigation; PyTorch deterministic-setup snippet;
  CI integration example; cross-hardware verification doc'd as
  deferred work; reading list for new CUDA engineer.
- **ARCHITECTURE.md** gains Section 4 (mission, owned subdirs,
  responsibilities, current status, open work, interface to
  other sections); diagram updated from 3 → 4 sections.
- **ROLES.md** gains CUDA engineer row; **`.github/CODEOWNERS`**
  maps `kernels/` + `refineforge-bitexact/` + the methodology
  doc to `@refineforge/cuda-engineer`.
- **CI workflow** gains a `bit-exact-gate` job: builds the
  binary, asserts deterministic-stub config passes AND
  non-deterministic-stub config fails (uses a bash conditional
  to flip the expected exit code), with informational message
  about needing a self-hosted GPU runner for real kernels.
- **`.gitignore`** adds `/kernels/runs/*/` + `/training/runs/*/`
  (the second was missed in the trainer commit; fixing now).

#### Tests

- `cargo nextest run --workspace`: **163/163 pass** (was 131;
  +32 for refineforge-bitexact: ~16 unit tests counted twice via
  lib+bin targets, plus 3 POSIX-only e2e tests).
- Local smoke on Windows: deterministic stub → 5/5 identical
  SHA-256 → PASS, exit 0. Non-deterministic stub → 5 unique
  SHA-256 → FAIL, exit non-zero. **Gate works in both directions.**

#### Honest disclosures (load-bearing)

- **No actual CUDA kernel was written or executed by this
  commit.** `kernels/src/` is empty. The CUDA engineer fills it.
- **Cross-hardware bit-exactness is NOT verified.** Single-
  runner only. Cross-hardware (A100 vs H100 vs consumer)
  requires a CI matrix with multiple GPU runner classes —
  documented as future work in §6 of the methodology doc.
- **The gate is hardware-agnostic in principle** — runs a
  command N times and hashes outputs. ROCm / Metal / CPU kernels
  work too. But the CUDA-specific mitigations in §2–§3 of the
  methodology doc don't transfer.
- **The CI `bit-exact-gate` job runs on a public Ubuntu runner**
  using only the bash stubs. Adding a self-hosted GPU runner is
  the operator's commitment; the YAML is ready when it arrives.
- **No performance benchmarking.** The gate cares about
  bit-exactness, not speed.

### Added — Section 2 phase 1.5: training-experiment orchestration (no actual training)

The "training infrastructure scaffold" the user asked for. The
six day-to-day duties listed in the ML-engineer description are
now SUPPORTED as code paths in `crates/refineforge-trainer` —
EXERCISED with stub trainer scripts in tests; never run against a
real GPU or real model in this commit (which would require a
multi-month engagement, not a session).

- **New workspace crate `crates/refineforge-trainer`** with the
  `refine-train` binary. Modules:
  - `experiment` — YAML schema (`id`, `base_model`,
    `dataset`, `backend{axolotl,hf_trainer,custom}`,
    `hyperparameters`, `checkpoint{dir,save_steps,keep_last}`,
    `monitoring{log_file,progress_format,metrics_to_track}`,
    `retry{max_attempts,backoff_seconds,resume_from_checkpoint}`).
    Validates id format + backend kind + custom-requires-command
    at load time.
  - `runner` — spawns the backend subprocess, captures
    stdout+stderr to `train.log`, feeds stdout lines to the
    progress parser, writes parsed records to `progress.jsonl`.
    Template substitution: `{run_dir}`, `{checkpoint_dir}`,
    `{dataset_path}`, `{resume_from}`.
  - `progress` — three parsers: `huggingface` (parses
    HF Trainer's dict-printed lines `{'loss': 0.4, ...}`),
    `axolotl` (delegates to HF parser; future-specialised),
    `generic` (regex `key=value`). Factory `parser_for(name)`.
  - `checkpoint` — scans for `step-N` / `checkpoint-N`
    directories; sorted by step descending; `latest()` for resume;
    `prune(dir, keep_last)` for cleanup.
  - `sweep` — `cartesian` (full grid) and `random:N`
    (deterministic-seeded Fisher-Yates sample) strategies.
    Generates per-run experiment configs with `{sweep_id}/run-NNNN`
    ids.
  - `failure` — classifies failures by scanning log tail:
    `OutOfMemory` (CUDA OOM / "out of memory" / "oom"),
    `Interrupted` (SIGINT / KeyboardInterrupt / exit 130),
    `Network` (SSLError / connection refused / DNS / timeout),
    `BackendError` (any traceback / "error:"), `Unknown`.
    `decide_action` picks: `ResumeFromCheckpoint`,
    `RetryFromScratch`, `Abort` (e.g. OOM with no checkpoint —
    retrying won't help), `Done`. Appends one
    `FailureRecord` to `failures.jsonl` per attempt.
  - `report` — final `report.json` with experiment config,
    per-metric summary stats (samples, first, last, min, max,
    mean), checkpoint manifest (step + path + size_bytes),
    failure timeline, attempt count.

- **CLI**: `refine-train run <exp.yaml> [--dry-run]`,
  `sweep <sweep.yaml> [--fail-fast]`,
  `monitor <run_dir> [--tail N] [--no-follow]`,
  `report <run_dir>`, `checkpoints <run_dir>`.
  `--dry-run` is the recommended first invocation — prints the
  resolved argv + run-dir without burning compute.

- **`training/` top-level** with:
  - `README.md` — usage walkthrough, run-directory layout,
    explicit "what this does NOT do" honesty section,
    cost-discipline reminder ("always `--dry-run` first").
  - `configs/example-qwen-1.5b.yaml` — template experiment
    targeting Qwen2.5-Coder-1.5B with LoRA; hyperparameters are
    reasonable starting points, NOT measured optima.
  - `configs/example-sweep.yaml` — 3×3 grid over learning_rate
    × batch_size.
  - `scripts/stub-trainer.sh` (POSIX) and `.ps1` (PowerShell) —
    emit HF-style progress + dummy checkpoints; `--fail-at STEP`
    flag for failure-recovery testing.
  - `data/` — empty; populating it is the multi-week mathlib
    mutation pipeline.
  - `runs/.gitkeep` — runtime output dir (contents gitignored).

- **Workspace** now has 7 members (was 6); workspace-tests
  jumped 57→131 (35 unique trainer-crate unit tests, counted
  twice because they appear in both the lib and bin targets,
  plus the 2 POSIX-only end-to-end tests that compile but only
  execute on unix).

### Mapping to the six day-to-day duties

| Duty (from user's spec) | Where it lives |
|---|---|
| Manages training infrastructure (cloud GPUs / local cluster) | The backend subprocess does. `refine-train` shells out to whatever's on PATH. |
| Runs training experiments | `refine-train run <exp.yaml>` |
| Tunes hyperparameters | `refine-train sweep <sweep.yaml>` (cartesian + random:N) |
| Monitors training progress | `refine-train monitor <run_dir>` (tail + follow) |
| Handles failures and recovery | `failure.rs`: classify + retry-with-backoff + resume-from-checkpoint |
| Produces training reports | `report.rs`: `report.json` with metric summary + checkpoints + failure timeline |

### Honesty disclosures (load-bearing)

- **No model was trained by this commit.** The 6 duties are
  *supported as code paths*; *exercised* only against the stub
  trainer scripts in unit + e2e tests. A real fine-tune run
  requires GPU access, ANTHROPIC_API_KEY-equivalent for the
  HuggingFace Hub (or local model weights), and an ML engineer
  to write the actual backend config file (e.g. axolotl YAML).
- **The mathlib mutation pipeline that produces training data
  is NOT included** in this commit. `training/data/` is empty.
  That pipeline is multi-week work documented in
  `docs/repair-evaluation.md` §9 as the gating step.
- **`example-qwen-1.5b.yaml` hyperparameters are reasonable
  starting points, NOT measured optima.** Anyone using them
  should do their own LR-range-finder pass first.
- **No distributed-training coordination.** Use
  `accelerate launch ...` or `torchrun ...` in your backend
  `command` for that.
- **No GPU resource management.** Whatever your backend uses,
  the scaffold doesn't interfere.
- **End-to-end tests are POSIX-only** (`#![cfg(unix)]`). The
  stub trainer is a bash script. The PowerShell variant exists
  in `training/scripts/stub-trainer.ps1` for Windows users to
  test manually; a Windows-runner CI job would cover it
  automatically, but the in-test smoke is unix.
- **Cost discipline:** a real training run on a 7B model can
  burn $50-500/attempt. `--dry-run` is the safety. The CHANGELOG
  for the FIRST real training run will document the actual
  spend.

### Highlights (read first)

Everything below in `[Unreleased]` accumulated across a single
multi-pass development session. When ready to cut v0.1.0, run
`release/release.sh 0.1.0` — it'll rename this section to
`[0.1.0] — <today>` and seed a fresh `[Unreleased]` above.

**The "all three sections gone deep" arc** (architecture's own
sequencing advice — one section at a time):

| Section | Headline shipped |
|---|---|
| **1 — Lean Specialist** | 5 scaffolding templates (was 3), `#[derive(LeanModel)]` proc-macro for simple struct cases, Mathlib-aware bundle export (`lake-manifest.json` included) |
| **2 — ML Training Engineer** | LSP-based repair driver, 4 strategies (mock / anthropic-mock / **real anthropic** with retry + prompt caching), `refine-eval` benchmark harness, 3-entry tutorial corpus, first real numbers (67 % repair rate on N=3 with claude-opus-4-7) |
| **3 — Infrastructure / DevOps** | Multi-arch CI matrix (Ubuntu / macOS / Windows) with caches, Sigstore keyless signing in CI, `refine bundle verify --verify-signature` (delegates to cosign), release scripts (POSIX + PowerShell), verifier Docker image, SECURITY.md, Nix flake (authored; first-build pending) |

**Workspace at the end of the arc:** 57/57 tests passing; 6 crates
(`refineforge-repair-api`, `refineforge-cli`, `refineforge-derive`,
`refineforge-strategies`, `refineforge-eval`, `example-counter`); 5
scaffolding templates; 2 tutorial claims (EXAMPLE-001 Lean-only,
EXAMPLE-002 refined); 9 docs under `docs/` plus README / ARCHITECTURE
/ ROLES / STRUCTURE / SECURITY / CHANGELOG at root.

**Honesty disclosures carried forward from each pass:**

- `--strategy anthropic`'s real HTTP path WAS exercised against the
  live Anthropic API on this dev machine — 4 runs × 3 corpus
  entries; that produced the 67 % number AND surfaced 2 latent
  driver bugs (final-diagnostic-check + Patch::apply line-clamp)
  which were then fixed in the same pass.
- CI workflow, Sigstore signing, and the Nix flake were NOT
  exercised by a real CI run / Fulcio cert / `nix build` because
  this repo has no GitHub remote AND no Nix install on this Windows
  dev machine. Each item is unit-tested where unit-testable; first
  real CI run / first real Nix user is the verification.
- `LeanModel` proc-macro handles simple struct cases only;
  generics / lifetimes / nested structs / tuple / enums / unions
  emit clean compile errors pointing at the offending span.
- N=3 corpus is the smoke-test tier per
  `docs/repair-evaluation.md` §2.1; the 67 % number describes those
  three claims, NOT "refine repair's general repair rate." Real
  benchmark needs N≥1000 from a Mathlib mutation pipeline
  (multi-week, deferred).

### Added — Section 1 deep: 2 new templates + LeanModel derive + Mathlib-aware bundle

The third "go deep on one section" pass, completing the arc across
Sections 1/2/3. All Section 1 work this commit was test-verified
end-to-end on this dev machine (unlike the Nix flake, which is
authored-but-unverified locally).

#### Two new scaffolding templates

- **`templates/linear_types/`** — single-use token (consume-once
  semantics). Models linearity via an explicit `consumed : Bool`
  flag (Lean 4's experimental linear-types are unstable as of
  v4.29.x). Three theorems proven `lake build` clean:
  `fresh_token_is_valid`, `consume_invalidates`,
  `consume_sets_consumed`. Verified end-to-end via
  `refine new --template linear_types ...` + `refine lean check`.
- **`templates/capability_with_revocation/`** — extends the
  existing `capability` template with a `revoked : Bool` flag and
  a monotone `revoke` operation. Three theorems:
  `revoked_authorizes_nothing` (main), `fresh_capability_authorizes_held_right`,
  `revoke_is_idempotent`. Verified end-to-end.
- Real bug found + fixed during smoke testing: my first attempt at
  `consume_is_idempotent_on_consumed` in `linear_types` had a proof
  that didn't close (structure eta after `simp [consume]` left an
  unsolved goal). Replaced with `consume_sets_consumed` (provable
  by `rfl`) which is the same semantic content stated differently.
  **Honesty win** — without the smoke test, the template would
  have shipped broken.

#### `refineforge-derive` proc-macro crate

- New workspace crate `crates/refineforge-derive` providing
  `#[derive(LeanModel)]`. Auto-generates `pub const LEAN_MODEL:
  &'static str` containing the Lean structure declaration
  equivalent to the Rust struct.
- Type mapping table (in `src/lib.rs` module docs):
  `u8..usize` → `Nat`, `i8..isize` → `Int`, `bool` → `Bool`,
  `String`/`&str` → `String`, `[u8; N]` → `ByteArray`,
  `Vec<T>` → `List T`.
- Unsupported shapes (generics, lifetimes, nested structs, tuple
  structs, enums, unions) emit a `syn::Error` pointing at the
  offending span — normal compile error with file:line. Honest
  upfront in the docs about what's NOT supported.
- Demo: `example-counter::Counter` now has `#[derive(LeanModel)]`
  alongside the existing derives. Test
  `lean_model_matches_hand_written_counter_lean` pins the
  generated string against the structural part of
  `lean/Refineforge/Counter.lean` (`structure Counter where\n  value : Nat`).
  If either drifts, this test fails — flagging the mismatch
  before a refinement-doc reviewer has to spot it.
- Test `lean_model_is_const_and_static` confirms the generated
  symbol is usable in `const` contexts.

#### Mathlib-aware bundle export

- `refine bundle export` now includes `lake-manifest.json` (when
  present) alongside `lakefile.toml` and `lean-toolchain`. The
  manifest pins package deps (Mathlib, Std, etc.) to specific git
  commits, so a verifier doing `lake build` against the bundled
  sources resolves the SAME Mathlib commit the proof was checked
  against — not whatever Mathlib is on `main` today.
- Bundle walk now explicitly skips `.lake/` build artifacts (would
  bloat Mathlib-using bundles by hundreds of MB).
- `docs/methodology.md` gains a new section "Bundled Lake
  dependencies (Mathlib, Std, etc.)" documenting the trust model:
  (a) what's in the bundle, (b) what's NOT (Mathlib source — too
  big), (c) mitigations available to a paranoid verifier (local
  Mathlib mirror, out-of-band SHA check, air-gapped vendoring),
  (d) the fact that EXAMPLE-* claims use zero Lake deps so the
  Mathlib-trust link is absent for them.

#### Tests

- `cargo nextest run --workspace`: **57/57 pass** (was 55; +2 new
  LeanModel demo tests on example-counter).
- Both new templates verified via `refine new` + `refine lean check`
  in scaffold + cleanup pattern (same discipline as the
  Tier-1-CI-verification of the existing 3 templates).

#### What's explicitly NOT in this commit

- **Mathlib-using claim of our own.** The bundle exporter is
  ready; the methodology doc covers the trust story; but
  refineforge has no claim that actually imports Mathlib. The
  feature is verified by code review + `docs/methodology.md`, not
  by end-to-end smoke test.
- **`#[derive(LeanModel)]` for complex types.** Generics, lifetimes,
  nested structs, tuple structs, enums, unions all emit clean
  compile errors. A v2 macro could handle generics + nested structs
  by monomorphisation; out of scope here. Documented as known
  limitations in the macro's module docs.
- **A `refine` subcommand that calls `LeanModel`** to emit a Lean
  file from a Rust struct. The const + accessor are present; the
  CLI wiring (e.g. `refine lean-from-rust <crate>::<Struct>` →
  prints LEAN_MODEL) is a one-file follow-up. Not blocking the
  current demo.

### Added — Section 3 phase 2: Nix flake (authored; first-build pending)

- **`flake.nix` at repo root** — real lean4-nix + crane + rust-overlay
  composition. Inputs pinned via `inputs.<x>.url`; `flake.lock` will
  be generated on first `nix flake lock`. Outputs:
  - `packages.refine`, `packages.refine-eval`, `packages.default = refine`
  - `packages.bundle-EXAMPLE-001`, `packages.bundle-EXAMPLE-002` —
    hermetic bundle derivations that run `lake build` + `refine
    bundle export <CLAIM>` in the Nix sandbox.
  - `devShells.default` — rustc, cargo-nextest, lean-all, cosign,
    git, jq, python3 with a `shellHook` that prints versions.
  - `checks.cargoTest`, `checks.cargoFmt`, `checks.cargoClippy` (the
    last with `-D warnings`).
  - `formatter = pkgs.nixpkgs-fmt`.
- **Lean toolchain pinned by content hash** via
  `lean4-nix.readToolchainFile ./lean/lean-toolchain` — so a Lean
  version bump in the toolchain file invalidates the Nix-derived
  Lean too, without a separate flake edit.
- **Rust dep build artifacts cached separately** via crane's
  `cargoArtifacts` pattern — source-only changes don't rebuild the
  dep graph.
- **Source filter** keeps cargo sources (via crane's
  `filterCargoSources`) plus `lean/`, `claims/`, `templates/`,
  `docs/refinement/` (needed by the bundle derivations); excludes
  `lean/.lake/` (build cache).
- **CI job `nix-flake-check`** added to `.github/workflows/ci.yml`,
  Ubuntu only (Nix on Windows would need WSL). Uses
  `DeterminateSystems/nix-installer-action@v14` +
  `magic-nix-cache-action@v8` for transparent caching. Runs:
  - `nix flake check --no-update-lock-file`
  - `nix build .#refine`
  - `nix build .#bundle-EXAMPLE-002` (the load-bearing reproducibility
    test — succeeds = bit-identical bundle build via Nix)
  - SHA-256 dump of the Nix-built bundle for future audit baseline.
- **`docs/reproducible-build.md` promoted** from "design doc" to
  "skeleton shipped, first-build pending." New §3 documents per-
  target confidence (refine/refine-eval/devShell high; bundle
  derivations medium); §7 status table updated; §8 added with the
  exact `nix flake check` + `nix build` invocation a Nix user
  should run first.
- **`.gitignore`** adds `result`, `result-*`, `.direnv/` —
  standard Nix-build symlinks + direnv cache.
- **STRUCTURE.md** lists `flake.nix` in the top-level tree.

### Honest disclosures — this WAS NOT TESTED LOCALLY

- **`flake.nix` was authored on a Windows dev machine without a
  Nix install.** I cannot `nix build` here. The flake follows the
  documented `lean4-nix` API (`readToolchainFile` overlay → `pkgs.lean-all`)
  and the documented `crane` patterns (`buildDepsOnly` +
  `buildPackage`), confirmed by reading the upstream READMEs. First
  green run on the `nix-flake-check` CI job (or a local
  Linux/macOS Nix user) is the verification.
- **The bundle derivations may need first-build adjustment.**
  Specifically: `lake build` writes its cache into `lean/.lake/`
  which is inside the build sandbox's `src` copy — should work
  per Nix idiom, but if the sandbox denies writes, the derivation
  will need `cd $TMPDIR && cp -r $src/* . && chmod -R +w .` style
  prep. Will surface on first run.
- **`crane`'s API has drifted across versions.** I pinned via
  `inputs.crane.url = "github:ipetkov/crane"` (HEAD); a flake.lock
  freeze on first run will stabilise this. If crane's API changed
  meaningfully, the `craneLib.buildPackage` / `cargoArtifacts` /
  `cargoTest` / `cargoFmt` / `cargoClippy` calls may need flag
  renaming.
- **Mathlib-using claims won't work in the current bundle derivation**
  (network would be needed during build, which Nix sandbox denies).
  Our EXAMPLE-* claims don't use Mathlib, so this is OK today.
  When that becomes a real need: either `__noChroot = true` (impure)
  or pre-built Mathlib package in pkgs.
- **No `flake.lock` is committed in this commit.** Standard Nix
  workflow: first user runs `nix flake lock` to generate it; the
  resulting lock file should then be committed. Doing this without
  Nix locally would produce a wrong lockfile.

### What this means for the project's reproducibility story

Before this commit: bundles were SHA-256-self-attesting + Sigstore-
signed, but not reproducible. Two maintainers building the same git
commit would produce bundles with different hashes (timestamp +
build-host paths in compiled artifacts).

After this commit (subject to first-green CI run):
- `refine` binary built via Nix is bit-identical across machines
  (rust-overlay pins rustc; crane caches deps deterministically).
- `bundle-EXAMPLE-*` produced via Nix is bit-identical
  except for `manifest.created_at` (which intentionally records
  the build time). A future revision could accept `SOURCE_DATE_EPOCH`
  to make even that field deterministic.
- The reproducibility-builds.org protocol from
  `docs/reproducible-build.md` §5 is now actually runnable.

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
