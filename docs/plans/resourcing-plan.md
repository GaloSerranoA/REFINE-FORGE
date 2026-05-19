# refineforge — resourcing plan (people, compute, tools, funding)

> **Status:** PLAN ONLY. Captures the operator's resourcing
> brief in enterprise format alongside
> [`autonomous-driver-plan.md`](autonomous-driver-plan.md) and
> [`gui-plan.md`](gui-plan.md). All cost figures are **honest
> estimates** as of 2026-05; spot-instance + grant programs
> shift quarterly. Verify before signing contracts.

## 1. Goal + scope

**In scope** (refineforge proper):
- The 4 sections from [`../../ARCHITECTURE.md`](../../ARCHITECTURE.md):
  Lean Specialist, ML Training Engineer, Infrastructure/DevOps,
  CUDA/GPU Kernel Engineer.
- The autonomous driver build per `autonomous-driver-plan.md`
  (already shipped through Phase 3.8 + Phase 4 audit; this
  document covers next-quarter staffing + compute).
- The `refineforge-studio` GUI build per `gui-plan.md` if
  approved (separate 15-week engineering line).
- Section 2's fine-tuned proof-repair model — the **16,000
  GPU-hour line item** is the dominant compute spend.

**Out of scope** (separate ecosystem, not refineforge):
- HELYX substrate (the `helyx-proofforge` parent project +
  any HELYX-specific neural components).
- Cogn8ty stack (symbolic reasoning components the operator
  has separately).
- `immortal-nars` / `immortal-prolog` (operator's own
  port/rewrite into the HELYX namespace).
- Knowledge Foundry (Python codebase for the operator's
  wider research stack).

These external components share **some** of refineforge's
software dependencies (PyTorch, Lean 4, Mathlib, the Rust
crypto stack) but their staffing + compute + funding are
the operator's separate concern. They're listed in §7
("External dependencies") for context only.

## 2. People — 4 specialists matching the 4 sections

The 4 named sections each need a specialist. Seniority levels
are calibrated to the work scope each section carries today
(post-v0.2.1) plus the next-quarter roadmap.

> **Note on rates.** All salary figures are fully-loaded
> (salary + benefits + equipment + ~30% overhead) US-coast
> rates. **European / LATAM rates are typically 40-60% lower**
> for equivalent seniority. NANTAR AI ROBOTICS' location
> determines the practical band. Quoted ranges below are the
> US ceiling; subtract per region.

### Section 1 — Lean 4 Specialist (HIGHEST PRIORITY)

- **Role:** owns `lean/`, `claims/`, the no-sorry policy gate,
  the proof templates, the refinement-doc tradition. Authors
  + reviews refinement arguments — the trust-critical artifact.
- **Seniority:** senior (5+ years of theorem-prover work,
  Lean 4 + Mathlib + ideally Coq or Isabelle background for
  cross-system fluency).
- **Why highest priority:** refineforge's value depends on
  the Lean side being correct. Every other section produces
  artifacts (LLM patches, bundles, CUDA kernels); the Lean
  specialist produces the **evidence** that anchors trust.
  A weak Lean section = the whole framework is a productivity
  tool over an undefended ground.
- **Effort:** 100% FTE during the active build of new
  refinement claims. After a steady-state corpus exists,
  drops to ~40% FTE (maintaining + reviewing new claims as
  they come in).
- **Rate (US fully-loaded, annual):** **$200-300k**. Scarce
  talent; expect upward pressure. Academic-adjacent hires
  (PhD candidate / post-doc on sabbatical) sometimes work
  at $120-180k for the experience.

### Section 2 — ML Training Engineer

- **Role:** owns `refineforge-strategies` (the AnthropicStrategy
  family), `refineforge-eval` (benchmark harness),
  `refineforge-trainer` (training orchestration), the
  fine-tuned proof-repair model line. Drives the 16,000
  GPU-hour line item.
- **Seniority:** senior (5+ years ML, prior fine-tuning at
  scale, distributed-training fluency, prompt engineering
  for code/proof domains).
- **Effort:** 100% FTE during the fine-tune phase (~6 months
  by the operator's stated commitment). After the model
  ships, drops to ~50% FTE for eval-corpus expansion + the
  eventual second fine-tune.
- **Rate (US fully-loaded, annual):** **$250-400k**. Higher
  than Lean because the broader job market is hotter; modern
  AI labs are bidding aggressively.

### Section 3 — Infrastructure / DevOps Engineer

- **Role:** owns `.github/workflows/`, the Sigstore signing
  chain, the Nix flake (still pending first-build),
  `release/release.sh`, the verifier Docker image, the
  multi-arch CI matrix. Future: signing the GUI binary if
  Phase 0 of `gui-plan.md` greenlights.
- **Seniority:** mid-to-senior (3+ years; ideally has run a
  reproducible-builds-style supply chain before).
- **Effort:** 60% FTE steady state. Spikes to 100% during
  release rituals + Nix flake first-build + GUI packaging
  if that lands.
- **Rate (US fully-loaded, annual):** **$180-250k**.

### Section 4 — CUDA / GPU Kernel Engineer

- **Role:** owns `kernels/`, `refineforge-bitexact`, the
  CUDA-side methodology in `docs/bit-exact-reproducibility.md`.
  Authors actual `.cu` source (the `src/` directory currently
  empty per honest scoping). Defines hardware-class
  baselines.
- **Seniority:** senior (3+ years CUDA kernel optimization,
  ideally nvcc + cuBLAS + cuDNN + Triton fluency).
- **Effort:** 30-50% FTE initially (no active CUDA work
  needed for refineforge proper until the operator commits
  to a kernel pipeline). Scales up to 100% if the operator
  introduces real kernels.
- **Rate (US fully-loaded, annual):** **$250-400k**. Comparable
  to ML engineer; CUDA optimization is a tight market.

### Fully-loaded annual burn (4 FTE at peak)

| Region | Low | High |
|---|---:|---:|
| US fully-loaded (4 FTE, all senior) | $880k | $1,350k |
| European mid-band (40% discount) | $530k | $810k |
| LATAM mid-band (55% discount) | $400k | $610k |

These ranges include 30% overhead (benefits, taxes, equipment,
contracted services). They DO NOT include the compute spend
in §3 below; those are separate line items.

## 3. Compute — the 16,000 GPU-hour line + smaller items

The compute needs split into three distinct workloads:

### 3.1 — Fine-tuned proof-repair model (Section 2)

- **Budget:** ~**16,000 H100-equivalent GPU-hours** for one
  fine-tune of a ~7-13B model on a corpus of N≥1000 broken-
  proof examples. This is the operator-stated number; refines
  to ±25% based on model size + dataset complexity.
- **Wall-clock:** depends on parallelism. 16 H100s for 6
  weeks (16 × 24 × 42 = 16,128 GPU-hours) is a realistic
  shape. 64 H100s for 2 weeks (64 × 24 × 14 = 21,504; over-budget)
  is shorter wall-clock at the cost of bigger spot-instance
  reservations.
- **Cost (see §5 for funding options):**
  - **Spot-friendly clouds** (Lambda, CoreWeave, Crusoe)
    at ~$2.50/hr per H100 average: **~$40,000**.
  - **AWS / GCP / Azure spot** at ~$3.50/hr per H100
    average (with the inevitable preemption rework
    overhead): **~$56,000**.
  - **AWS / GCP / Azure on-demand** at ~$11/hr per H100:
    **~$176,000**. Mostly not the right shape for this
    workload; included for completeness.

### 3.2 — Mathlib mutation pipeline (Section 2 phase 1 item 3)

- **Goal:** produce the N≥1000 broken-proof corpus that the
  fine-tune §3.1 needs.
- **Compute:** CPU-bound (Lean elaboration). One workstation
  + a small cloud burst (~$200-500/month) for parallel
  elaboration sweeps. **NOT** in the 16,000 GPU-hour budget.
- **Wall-clock:** multi-week per the existing
  `docs/repair-evaluation.md` §9 plan. Operator + Lean
  specialist time, not GPU time.

### 3.3 — Development + CI compute

- **Anthropic API budget:** ~**$50-500 / month** during active
  development. The Phase 4 dogfood spent $0.35 in one run;
  daily-developer iteration burns more.
- **CI compute:** GitHub Actions free tier handles current
  workspace; Nix-flake-check job will increase minutes.
  Estimate ~**$0-200 / month** depending on push frequency.
- **GPU CI (for bitexact + future kernel work):** self-hosted
  GPU runner OR per-job GPU minutes on GitHub Actions
  Larger Runners (~$3-6 / hr per A10 / L4 / H100-PCIe).
  Estimate ~**$200-1500 / month** if active.

## 4. Software + tools

### 4.1 — Already in use (zero new cost)

These ship as runtime/build dependencies of v0.2.1; no
operator decision needed:

- **Rust toolchain** — stable channel, currently pinning
  via `rust-toolchain.toml` would be a nice-to-have addition.
- **Lean 4 toolchain** — pinned to `leanprover/lean4:v4.29.1`
  in `lean/lean-toolchain`. Managed via `elan` (one-line
  install).
- **Python 3** — for `release/release.sh`'s Cargo.toml
  version bump step (small) + the Mathlib mutation pipeline
  + the eventual fine-tune scripts.
- **Git + GitHub** — version control; CI; releases.
- **`cargo-nextest`** — test runner (already in workspace).
- **`cosign`** — Sigstore signing in CI + locally on release.
- **`lake`** — Lean build tool, ships with elan.
- **`pnpm` / Node.js** — IF the GUI lands (gui-plan.md
  Phase 1); not needed today.

### 4.2 — Training framework decision (Section 2 phase 2/3)

**Operator must pick** before the fine-tune starts:

- **PyTorch + FSDP (Fully Sharded Data Parallel)** — most
  common shape for 7-70B fine-tunes. Largest ecosystem;
  best community resources for proof-domain LLMs.
- **JAX + FLAX** — used by some research groups (notably
  Google + a few prover-LLM labs). Better XLA optimization
  on TPUs; not relevant if we're on H100s.
- **`axolotl`** — the `refineforge-trainer` scaffold already
  has `backend: axolotl` as a first-class kind. Wraps
  PyTorch + FSDP with a YAML-driven UX. Recommended unless
  the operator's ML engineer prefers raw PyTorch.

**Recommendation:** axolotl on PyTorch + FSDP. The trainer
crate already targets this shape; no scaffold-work to switch.

### 4.3 — Experiment tracking

The operator's choice. All have free tiers sufficient for
solo / small-team work:

- **Weights & Biases** — best UX; SaaS-first; free for
  personal/academic projects. Paid tiers for teams.
- **MLflow** — open source; self-hosted; lower friction
  for air-gapped projects.
- **TensorBoard** — local-only; minimal; included with
  PyTorch.

**Recommendation:** Weights & Biases free tier during the
fine-tune phase. Switch to MLflow self-hosted if the project
moves to a fully-air-gapped posture later.

### 4.4 — Cloud infrastructure providers

For the 16,000 GPU-hour fine-tune, ranked by $/hr (cheapest
to most expensive on a like-for-like basis):

| Provider | $/hr per H100 (typical) | Notes |
|---|---:|---|
| Crusoe | $2.00 - $3.50 | Often discounted; carbon-neutral; smaller ecosystem |
| Lambda Labs | $2.50 - $3.50 | Strong on H100 spot; clean instance shapes |
| CoreWeave | $2.50 - $4.00 | Largest GPU-specialized cloud |
| TogetherAI / RunPod | $2.00 - $4.00 | Spot-heavy; preemption risk |
| AWS Spot (p5) | $3.00 - $5.00 | Reliable spot; complex billing |
| GCP Spot (A3) | $3.00 - $5.00 | Similar to AWS |
| Azure Spot (ND H100 v5) | $3.00 - $5.00 | Similar |
| AWS On-Demand (p5) | $12.00 | Reserved instances drop to ~$7-8 |

**Recommendation for 16,000 hr fine-tune:** Lambda OR
CoreWeave on H100 spot, with a small AWS/GCP fallback for
the final two checkpoints (where preemption stops being
acceptable). Total: **~$45,000 ± $10,000**.

## 5. Funding strategy — Options A / B / C with concrete numbers

The operator-stated three options, with realistic figures
attached.

### Option A — Compute grants (best for early-stage)

**Best for:** the current refineforge posture — pre-revenue,
research-flavoured, with a public open-source story.

Active programs (verify before applying; programs shift):

| Program | Typical award | Notes |
|---|---:|---|
| **NVIDIA Inception** (compute credits) | $5k - $25k | Application-based; early-stage AI startups; H100 credits via partner clouds |
| **AWS Activate** | up to $100k AWS credits | Bigger ticket; need YC/VC backing OR accelerator association |
| **Google Cloud for Research / Startups** | $5k - $350k credits | Tiered by stage; needs program enrollment |
| **Azure for Startups** | up to $150k | Similar to AWS Activate |
| **HuggingFace community grants** | model-specific; usually smaller | Good for the model release later, not the training spend |
| **Anthropic / OpenAI research grants** | $10k - $100k API credits | Useful for the Anthropic-strategy + eval-corpus generation, not GPU training |
| **CoreWeave / Lambda startup credits** | $5k - $50k | Direct-to-provider; sometimes more flexible than hyperscaler programs |
| **Academic/national supercomputing time** | varies | Spain (BSC), LATAM HPC consortia, EU EuroHPC; needs research-affiliated PI |

**Realistic stack for 16,000 GPU-hours:**
- NVIDIA Inception ($15k credits) + CoreWeave startup ($25k
  credits) + AWS Activate Foundation tier ($5k credits) +
  a 5-10% top-up from cash = **~$45k effective spend at
  ~$5k cash outlay**.
- Wall-clock to assemble grants: 1-3 months of application
  + onboarding. Apply in parallel.

**Caveats:**
- Grants come with strings (logos, citations, sometimes
  publication requirements). Read terms before accepting.
- Credits often expire 6-12 months after issue. Time the
  fine-tune to fit.
- Grants are NOT renewable indefinitely — bridge to Option B
  or C before the second fine-tune.

### Option B — Customer-funded compute

**Best for:** mid-stage — refineforge has at least one
production customer who needs verified proofs they can sign.

**Model 1 — Strategic POC pricing:**
- Customer pays $50k-$250k for a verification engagement
  scoped to their specific claim set.
- Compute is line-itemed; customer covers it directly OR
  refineforge eats it from the engagement fee.
- For 16,000 GPU-hours at $2.50/hr = $40k, fits comfortably
  inside a $100k POC.

**Model 2 — Embedded researcher:**
- Customer hires Section 2 ML engineer for 6 months at
  ~$150k + benefits via refineforge contract.
- Customer pays full GPU burn separately (their AWS account
  or via refineforge cost-plus billing with a ~15-25% margin).
- Refineforge keeps the model weights + the methodology;
  customer gets first-use rights for ~12-18 months.

**Caveats:**
- IP-sharing is the friction point. Negotiate before the
  fine-tune starts.
- Customer's compute account != refineforge's account.
  Means the fine-tune lives in the customer's cloud; export
  for refineforge ongoing use requires explicit data-share
  agreement.
- Less leverage than direct purchase for the second + third
  fine-tune.

### Option C — Direct cash purchase

**Best for:** late-stage — multiple revenue streams,
predictable annual burn, strategic value in owning the
hardware.

**Model 1 — Cloud cash:**
- Pay-as-you-go on Lambda / CoreWeave / Crusoe.
- Marginal flexibility; can scale up/down per fine-tune
  cycle.
- 16,000 GPU-hours at $2.50/hr average = **$40,000 cash
  per fine-tune**.

**Model 2 — Reserved instances:**
- 1-year H100 reservation on AWS / GCP at ~$7-8/hr per
  H100 (vs $12 on-demand).
- 16,000 hours over a year = 1.83 GPUs running 24/7 for a
  year = ~$120k for the reservation, much more if you want
  bursts.
- Only makes sense if the operator has a SECOND fine-tune
  pipeline running through the year.

**Model 3 — Bare metal:**
- 8x H100 server: **$300k-$450k capex** (Supermicro, Dell,
  or Lambda Labs node).
- Colo + power: **$5k-$15k / month** depending on region +
  cooling.
- 16,000 hours on one box = ~83 days elapsed (8 GPUs × 24 hr
  × 83 = 15,936). Three months of training time, your
  hardware, your data, your IP.
- **Payback math:** at $2.50/hr cloud cost = 16,000 hr =
  $40k; need (450k / 40k) = 11+ fine-tune-equivalents
  before bare metal pays back the cloud. With the colo
  overhead, more like 15-20. Only sensible if
  refineforge is doing fine-tuning AND inference AND
  bitexact testing AND has revenue justifying the burn.

**Caveats:**
- Bare metal locks in the hardware generation. H100 is
  current; B100/GB200 will obsolete it in 12-18 months.
- Operations overhead (ops engineer time, cooling,
  power resilience) is a real ongoing cost.
- Resale market for used H100 nodes is illiquid below
  $200k discount.

### Recommended path (operator-region-dependent)

Given NANTAR AI ROBOTICS' likely scale + the existing
refineforge state (one-engineer + active build):

1. **Phase 1 (now → next 6 months):** Option A. Stack
   NVIDIA Inception + CoreWeave/Lambda startup credits +
   AWS Activate. Aim for $30-50k in grants; ~$5-10k cash
   top-up to cover overflow. **Goal:** ship the first
   fine-tuned model on grant credits.
2. **Phase 2 (months 6-12):** Option B if a paying
   customer emerges. Embedded-researcher model is the
   highest-leverage if a HELYX-aligned customer wants
   verified proofs.
3. **Phase 3 (year 2+):** Option C if revenue justifies.
   Bare metal only with at least $1-2M ARR backing
   refineforge specifically.

## 6. Software libraries — broken down by section

### 6.1 — Section 1 (Lean Specialist)
- **Lean 4** (theorem prover) — pinned v4.29.1; install via elan
- **Mathlib** — Lean's mathematical library; the proven theorems
  refineforge builds on. NOT currently in `lake-manifest.json`
  (the existing EXAMPLE-* claims use zero Lake deps). First
  Mathlib import is a **Cat 8 escalation** per
  `docs/escalation-criteria.md` v0.3.
- **Lean-specific verification libraries** as the operator
  needs them (e.g. `Aesop`, `Qq` for term-level metaprogramming).

### 6.2 — Section 2 (ML Training Engineer)
- **PyTorch** (deep learning framework) — runtime + training
- **`transformers`** (HuggingFace) — model loading + tokenizers
- **`accelerate`** — distributed training launcher
- **`axolotl`** — recommended training-orchestration wrapper
- **`peft`** — parameter-efficient fine-tuning (LoRA, QLoRA)
- **`datasets`** (HuggingFace) — corpus management
- **`bitsandbytes`** — quantization for inference
- **`diffusers`** — only if a diffusion-style proof model
  is on the roadmap (not today)

### 6.3 — Section 3 (DevOps)
- **Rust toolchain** — for `refine` + the workspace
- **`cargo-nextest`** — test runner
- **`cosign`** — Sigstore signing
- **`lake`** — Lean build tool
- **GitHub Actions** — CI
- **Nix + `lean4-nix` + `crane` + `rust-overlay`** —
  hermetic builds (Nix flake authored; first-build pending)
- **Docker / OCI** — for the verifier image

### 6.4 — Section 4 (CUDA/GPU Kernel Engineer)
- **CUDA toolkit** — current is 12.x; tracks NVIDIA's release
- **`nvcc`** — CUDA compiler
- **cuBLAS / cuDNN** — typical accelerated linear algebra
- **`refineforge-bitexact`** — already shipped; gate primitive

### 6.5 — Cross-cutting cryptography (Rust)
- **`sha2`**, **`sha3`**, **`blake3`** — hashing (operator
  already uses these in HELYX)
- **`ed25519-dalek`** — signing (operator already uses this)
- **`sigstore-rs`** — future pure-Rust verifier (currently
  shells out to cosign; documented as future enhancement)

## 7. External dependencies (NOT refineforge proper)

These ship in the operator's wider ecosystem; refineforge
shares some libraries with them but does NOT own them:

- **HELYX substrate** — operator's parent project, the
  Rust runtime for trust-critical claims. refineforge was
  forked from `helyx-proofforge` per the v0.1.0 CHANGELOG.
- **Cogn8ty** — operator's symbolic reasoning stack.
- **`immortal-nars`**, **`immortal-prolog`** — operator's
  ports/rewrites for HELYX. Not external dependencies in
  the npm-package sense — the operator wrote them — but
  also not part of refineforge proper.
- **Knowledge Foundry** — operator's Python codebase for
  the wider research stack.

These items get their own resourcing plans in the operator's
project portfolio. The refineforge build does not depend on
their landing for refineforge proper to ship.

## 8. Risks (resourcing-level)

| Risk | Severity | Mitigation |
|---|---|---|
| Lean specialist scarcity — small talent pool | **HIGH** | Cast wide: academic-adjacent + post-doc + Mathlib contributors; pay above-market if hire fails twice |
| ML engineer poached mid-fine-tune | **HIGH** | Retention bonus tied to fine-tune ship; document the methodology so a replacement can pick it up |
| Grant credits expire before fine-tune starts | **MEDIUM** | Apply 3-6 months early; accept smaller bursts to keep credits "warm" |
| Spot preemption invalidates a long training run | **MEDIUM** | Checkpoint every N steps (refineforge-trainer already supports); accept ~10% rework overhead |
| Cloud price spikes (H100 demand) | **MEDIUM** | Spread the 16k hr across two providers; reserve a fallback contract |
| Section 4 (CUDA) hire delayed indefinitely | **LOW** for refineforge proper | The bitexact gate is operator-agnostic; real kernels stay deferred |
| Currency exchange (LATAM operator, USD cloud) | **MEDIUM** | Pre-pay credits when USD weak; hedge if engagement is multi-year |
| Customer-funded compute on customer cloud — IP exfil risk | **HIGH** if Option B path | Negotiate data-share + model-weight retention clauses BEFORE the fine-tune |

## 9. Open questions for the operator BEFORE committing

These need answers before any grant application / hire offer
goes out:

1. **Operator region.** US / EU / LATAM determines the
   salary band — all subsequent budgets depend on this.
2. **Hire order.** Lean specialist FIRST (highest priority);
   ML engineer second. DevOps + CUDA can lag. Confirm.
3. **Fine-tune model size.** 7B vs 13B vs 34B vs 70B —
   compute cost scales ~linearly in active params. 16,000
   GPU-hours fits ~13B comfortably; 70B is tight.
4. **Training framework choice.** axolotl + PyTorch + FSDP
   recommended; operator's ML engineer may override.
5. **Cloud provider preference.** Lambda + CoreWeave +
   Crusoe rank cheapest for H100 spot; AWS/GCP/Azure for
   on-demand reliability + grant programs. Pick a primary +
   a fallback.
6. **Funding lane.** Option A (grants) for Phase 1 is the
   recommendation. Confirm the operator's appetite for
   Option B (customer-funded) given the IP-sharing
   tradeoff.
7. **Experiment-tracker choice.** W&B free vs MLflow
   self-hosted. Determines what credentials the ML engineer
   needs on day one.
8. **GUI engineering line.** Does `refineforge-studio`
   (per `gui-plan.md`) get a dedicated engineer, or is it
   the DevOps engineer's 40% slot during steady-state?

## 10. Headline budget — first 12 months

Honest estimate for the first 12 months **post-v0.2.1**,
assuming operator picks the recommended path (Option A
grants for compute; Lean + ML engineers FTE; DevOps part-time
contractor; CUDA engineer deferred):

| Line item | US ceiling | LATAM mid-band |
|---|---:|---:|
| Lean specialist (1.0 FTE) | $300k | $135k |
| ML engineer (1.0 FTE) | $400k | $180k |
| DevOps (0.6 FTE) | $150k | $68k |
| CUDA engineer (0.3 FTE) | $120k | $54k |
| GPU compute (16k hrs via grants + $10k cash top-up) | $10k | $10k |
| Anthropic API + CI compute | $5k | $5k |
| Experiment-tracker + observability | $1k | $1k |
| Code-signing certs (if GUI ships) | $0.5k | $0.5k |
| Contingency (15%) | $145k | $68k |
| **Total** | **$1,131k** | **$521k** |

LATAM mid-band is roughly **$520k / year** to run the
4-section team + the first fine-tune. US ceiling is
**~$1.1M / year**.

These numbers are **honest estimates**; actual quotes from
recruiters + grant officers + cloud sales reps will vary by
±20-30%. Re-quote before signing.

## 11. Definition of done (this plan, not the project)

This document is "done" when:

1. ✅ All four section staffing bands are quoted with
   seniority + effort + rate ranges.
2. ✅ The 16,000 GPU-hour line is quoted across all three
   funding options with realistic $/hr.
3. ✅ Software dependencies are listed per section.
4. ✅ External components (HELYX, Cogn8ty, immortal-*,
   Knowledge Foundry) are noted as out-of-scope.
5. ✅ 8 named project-level risks with mitigations.
6. ✅ Headline 12-month budget table with both US ceiling +
   LATAM mid-band columns.
7. ✅ 8 named open questions the operator must resolve.

The plan does NOT commit to any of the numbers — they're
honest estimates ready for sharpening when the operator
goes out to hire, apply for grants, or sign cloud contracts.
