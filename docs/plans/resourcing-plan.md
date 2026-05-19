# refineforge — resourcing plan (operator-side, with refineforge AS the 4 specialists)

> **Status:** PLAN ONLY. v0.2 of this doc. The v0.1 of this
> plan (commit `2f60d9c`) framed the four sections as humans
> to hire — that was an inverted reading of the operator's
> brief and is preserved in git history for audit only. The
> correct framing, captured here, is that **refineforge IS the
> four specialists**. Operator-side resourcing is one human
> operator + one part-time maintainer + the compute that runs
> the four AI-driven specialist roles.
>
> All cost figures are **honest estimates** as of 2026-05;
> spot-instance + grant programs + LLM pricing all shift
> quarterly. Verify before signing contracts.

## 1. Doctrine + framing

The refineforge doctrine is **"LLM may propose, Lean must
verify, human operator must approve."** The four ARCHITECTURE
sections (Lean Specialist, ML Engineer, DevOps, CUDA Engineer)
are **roles refineforge performs**, not headcount lines:

- **Section 1 (Lean specialist)** is performed by the
  autonomous driver's planner + executor (lean check, scan,
  bundle export) + the escalation engine that surfaces only
  the v0.3-categorical decisions a human must judge.
- **Section 2 (ML engineer)** is performed by
  `refineforge-strategies` (live Anthropic with retry +
  prompt caching) + `refineforge-eval` (benchmark harness) +
  `refineforge-trainer` (training orchestration). The
  fine-tuned proof-repair model (the 16,000 GPU-hour line
  item) sharpens this section's quality but doesn't change
  its shape — refineforge already does ML-engineer work
  today; the fine-tune makes it cheaper + faster.
- **Section 3 (DevOps)** is performed by `.github/workflows/`
  (multi-arch CI + Sigstore signing) + `release/release.sh`
  (12-step release ritual) + the verifier Docker image + the
  Nix flake. These ship in v0.2.1.
- **Section 4 (CUDA engineer)** is performed by
  `refineforge-bitexact` (the gate primitive) +
  `docs/bit-exact-reproducibility.md` (the methodology).
  Real kernel authoring is operator-side; refineforge gates
  whatever you write.

So when the operator says "I would need [the four
specialists]," the answer is **refineforge provides them**.
What the operator actually needs is the SMALLER set below.

## 2. What the operator actually needs

The minimum viable resourcing for refineforge in production
(per the doctrine: the human operator is the trust anchor;
refineforge does the rest):

### 2.1 — One human operator (essential, non-negotiable)

- **Role:** the human in "human operator must approve."
  Signs escalation packets per criteria v0.3. Signs release
  tags. Owns the refinement-doc tradition (the trust-critical
  artifact). Reviews fine-tuned model outputs at acceptance
  gates.
- **This is YOU.** Not a hire. The whole framework is built
  so one operator can hold the trust surface for many claims.
- **Effort:** ~5-20% of operator's working week in
  steady-state, spiking during fine-tune rollouts and major
  claim audits.
- **Cost:** operator's own time. Not a cash line item.

### 2.2 — One part-time refineforge maintainer (recommended)

- **Role:** keeps the refineforge codebase healthy: Rust
  deps, Lean toolchain bumps, criteria-doc edits, packet
  template refinements, GUI maintenance if `refineforge-studio`
  ships. Could be the operator's own time.
- **Effort:** ~10-20 hours / week steady-state. Spikes for
  new sections + new escalation categories.
- **Cost (US fully-loaded, annual):** **$40k - $80k** at
  ~0.3-0.5 FTE rates. LATAM mid-band: **$18k - $36k**. OR
  operator's own time at zero cash cost.

### 2.3 — Compute that runs the four AI-driven specialists

This is the dominant line item. Three buckets:

**Bucket A — Ongoing inference (autonomous driver runtime):**
- Anthropic API for every `refine autonomous --strategy
  anthropic` invocation.
- Phase 4 audit benchmark: $0.35 spend per claim run.
- For an operator running ~10 claims/month through autonomous:
  ~$3.50/month. For ~100 claims/month: ~$35/month.
- **Estimate:** **$10 - $200 / month** depending on claim
  volume. Budgeted as $500-$2,500 / year worst case.

**Bucket B — Fine-tune training (one-shot, repeats annually):**
- The 16,000 GPU-hour line item.
- Replaces / sharpens Section 2's Anthropic dependency by
  fine-tuning a smaller open model on a Mathlib-mutation
  corpus.
- Once shipped, ongoing inference cost drops (own model
  served locally or via cheaper inference clouds).
- **Cost (cloud cash):** $40k spot-avg (Lambda/CoreWeave),
  $176k AWS on-demand. See §3 for full pricing.
- **Cost (grants):** ~$5-10k cash outlay after stacking
  Option A grants (see §4).

**Bucket C — CI + dev compute (small):**
- GitHub Actions free tier + ~$0-200/month for occasional
  GPU CI when bit-exact kernels land.
- Anthropic API for dev iteration: ~$10-100/month during
  active development.

## 3. Cloud + GPU pricing (unchanged from v0.1 of this plan)

For the 16,000 GPU-hour fine-tune in Bucket B, ranked by
$/hr per H100 (cheapest to most expensive):

| Provider | $/hr per H100 (typical) | Notes |
|---|---:|---|
| Crusoe | $2.00 - $3.50 | Carbon-neutral; smaller ecosystem |
| Lambda Labs | $2.50 - $3.50 | Strong H100 spot; clean instance shapes |
| CoreWeave | $2.50 - $4.00 | Largest GPU-specialized cloud |
| TogetherAI / RunPod | $2.00 - $4.00 | Spot-heavy; preemption risk |
| AWS Spot (p5) | $3.00 - $5.00 | Reliable spot; complex billing |
| GCP Spot (A3) | $3.00 - $5.00 | Similar to AWS |
| Azure Spot (ND H100 v5) | $3.00 - $5.00 | Similar |
| AWS On-Demand (p5) | $12.00 | Reserved drops to ~$7-8 |

**Recommended for 16,000-hr fine-tune:** Lambda OR CoreWeave
on H100 spot, with a small AWS/GCP fallback for the final
two checkpoints (where preemption stops being acceptable).
Cloud-cash total: **~$45,000 ± $10,000**.

## 4. Funding options — A / B / C with concrete numbers

### Option A — Compute grants (best for current refineforge posture)

**Best for:** the operator running refineforge today —
pre-revenue, research-flavoured, public open-source story.

Active programs (verify before applying; programs shift):

| Program | Typical award | Notes |
|---|---:|---|
| **NVIDIA Inception** (compute credits) | $5k - $25k | Application-based; early-stage AI startups; H100 credits via partner clouds |
| **AWS Activate** | up to $100k AWS credits | Bigger ticket; needs YC/VC backing OR accelerator association |
| **Google Cloud for Research / Startups** | $5k - $350k credits | Tiered by stage; needs program enrollment |
| **Azure for Startups** | up to $150k | Similar to AWS Activate |
| **HuggingFace community grants** | model-specific; usually smaller | Useful for the model release later, not the training spend |
| **Anthropic / OpenAI research grants** | $10k - $100k API credits | Useful for the Anthropic-strategy + eval-corpus generation, not GPU training |
| **CoreWeave / Lambda startup credits** | $5k - $50k | Direct-to-provider; sometimes more flexible than hyperscaler programs |
| **Academic / national supercomputing time** | varies | Spain BSC, LATAM HPC consortia, EU EuroHPC; needs research-affiliated PI |

**Realistic stack for 16,000 GPU-hours:**
- NVIDIA Inception ($15k credits) + CoreWeave startup ($25k
  credits) + AWS Activate Foundation tier ($5k credits) +
  ~5-10% top-up from cash = **~$45k effective spend at ~$5-10k
  cash outlay**.
- Wall-clock to assemble grants: 1-3 months of applications +
  onboarding. Apply in parallel.

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

**Model 2 — Embedded automation:**
- Customer pays refineforge a fixed fee ($30-80k/year) to
  run their claim verification end-to-end via the autonomous
  driver. Compute is cost-plus billed (~15-25% margin) OR
  bundled into the fee.
- Operator's time is contractor billing at $150-300/hr
  spot rates for the work the operator can't delegate to
  the framework (refinement docs, criteria-doc edits,
  escalation review).
- The customer gets verified claims they can sign; refineforge
  keeps the model + the methodology + first-use rights for
  future customers.

**Caveats:**
- IP-sharing is the friction point. Negotiate before any
  fine-tune work that touches customer data.
- Customer's compute account != refineforge's account.
  Customer-trained models may need data-share agreements
  for refineforge to keep using them.

### Option C — Direct cash purchase

**Best for:** late-stage — multiple revenue streams,
predictable annual burn, strategic value in owning the
hardware.

**Model 1 — Cloud cash:**
- Pay-as-you-go on Lambda / CoreWeave / Crusoe.
- 16,000 GPU-hours at $2.50/hr avg = **$40,000 cash per
  fine-tune**.
- Repeats annually OR semi-annually depending on model
  refresh cadence.

**Model 2 — Reserved instances:**
- 1-year H100 reservation on AWS / GCP at ~$7-8/hr per H100
  (vs $12 on-demand).
- 16,000 hours over a year = 1.83 GPUs running 24/7 for a
  year = ~$120k for the reservation.
- Only makes sense if refineforge has a SECOND inference
  pipeline running through the year (e.g. serving a
  customer-facing API).

**Model 3 — Bare metal:**
- 8x H100 server: **$300k-$450k capex** (Supermicro, Dell,
  Lambda Labs node).
- Colo + power: **$5k-$15k / month**.
- 16,000 hours on one box = ~83 elapsed days (8 × 24 × 83 =
  15,936). Three months training time on your hardware,
  your data, your IP.
- **Payback math:** at $2.50/hr cloud = $40k/fine-tune;
  payback after 11-20 fine-tune-equivalents vs cloud
  (including colo overhead). Only sensible at $1-2M+ ARR
  backing refineforge specifically.

### Recommended path (operator-region-dependent)

Given NANTAR AI ROBOTICS's likely scale + the existing
refineforge state (one-operator with v0.2.1 shipped):

1. **Phase 1 (now → next 6 months):** Option A. Stack NVIDIA
   Inception + CoreWeave/Lambda startup credits + AWS Activate.
   Aim for $30-50k credits; $5-10k cash top-up to cover
   overflow. **Goal:** ship the first fine-tuned model on
   grant credits.
2. **Phase 2 (months 6-12):** Option B if a paying customer
   emerges. Embedded-automation model (refineforge runs the
   verification; operator's hourly billing covers human-only
   work) is the highest-leverage shape because it doesn't
   require the customer to host the trained model.
3. **Phase 3 (year 2+):** Option C cloud-cash recurring.
   Bare metal only if revenue justifies a $1-2M capex
   commitment.

## 5. Software tools (refineforge runtime stack — nothing new to acquire)

These already ship as runtime/build dependencies of v0.2.1.
The operator does NOT acquire any new software contracts to
run refineforge today.

| Tool | Purpose | Cost | Already in use? |
|---|---|---:|---|
| Rust toolchain | `refine` binary + workspace | $0 | ✅ |
| Lean 4 toolchain (via elan) | `lake build`, the no-sorry gate | $0 | ✅ |
| Mathlib | If/when claims need it (Cat 8 escalation) | $0 | not yet |
| Python 3 | `release/release.sh`, fine-tune scripts | $0 | ✅ |
| Git + GitHub | Version control + CI | $0 (free tier) | ✅ |
| `cargo-nextest` | Test runner | $0 | ✅ |
| `cosign` | Sigstore signing | $0 | ✅ |
| `lake` | Lean build | $0 (ships with elan) | ✅ |
| `axolotl` | Recommended training-orchestration wrapper | $0 | first fine-tune |
| `transformers` / `accelerate` / `peft` / `bitsandbytes` | Fine-tune toolchain | $0 | first fine-tune |
| Anthropic API | Live `--strategy anthropic` repair | per-call (see §2.3 Bucket A) | ✅ |
| Weights & Biases (optional) | Experiment tracking during fine-tune | $0 free tier | first fine-tune |
| Code-signing certs | If GUI ships | ~$500/year (or $0 via Sigstore) | future |

Nothing on this list is a contract negotiation. Everything is
either free, already shipping, or pay-per-use API.

## 6. Risks (operator-side)

| Risk | Severity | Mitigation |
|---|---|---|
| Operator burnout — one human holding the trust surface | **HIGH** | Cap claim throughput per week; automate every non-trust-bearing operation; criteria v0.3's "operator decides" doctrine prevents drift but doesn't replace rest |
| Anthropic API cost runaway | **MEDIUM** | Cost-gate on `refine autonomous --max-cost-usd` (already shipped); monthly spend caps via Anthropic dashboard; fine-tune to own model reduces ongoing |
| Fine-tune model regression — worse than Anthropic baseline | **MEDIUM** | Eval-corpus gates the rollout (`refineforge-eval` already shipped); don't ship the model if it underperforms; iterate |
| Grant credits expire before fine-tune starts | **MEDIUM** | Apply 3-6 months early; accept smaller bursts to keep credits "warm" |
| Spot preemption invalidates a long training run | **MEDIUM** | Checkpoint every N steps (`refineforge-trainer` already supports); accept ~10% rework overhead |
| Cloud price spike (H100 demand) | **MEDIUM** | Multi-cloud spread; reserve a fallback contract |
| Criteria v0.3 drift — escalation categories evolving without operator review | **LOW** | criteria-doc is itself a Cat 1 escalation; documented in the contract |
| Customer-funded compute IP exfil (Option B path) | **HIGH** if Option B taken | Negotiate data-share + model-weight retention clauses BEFORE any fine-tune touches customer data |
| FX risk — LATAM operator with USD cloud spend | **MEDIUM** | Pre-pay credits when USD weak; hedge if engagement is multi-year |
| Single-operator bus factor | **HIGH** long-term | Document everything in `docs/`; the refinement-doc tradition + criteria-doc + run reports are the recovery surface |

## 7. Headline 12-month budget — corrected framing

For one human operator + one part-time maintainer (or
operator's own time) + the first fine-tune via grants:

| Line item | LATAM mid-band | US ceiling |
|---|---:|---:|
| Operator (you, 5-20% of working week) | $0 cash | $0 cash |
| Part-time maintainer (0.3-0.5 FTE) | $18-36k | $40-80k |
| Anthropic API (ongoing autonomous runs) | $1k | $2k |
| GPU compute for fine-tune (via grants + $10k top-up) | $10k | $10k |
| CI + dev compute | $1k | $2k |
| Experiment-tracker (W&B free tier) | $0 | $0 |
| Code-signing certs (if GUI lands) | $0.5k | $0.5k |
| Contingency (15%) | $5k | $14k |
| **Total** | **~$35-52k / year** | **~$68-108k / year** |

**Two orders of magnitude smaller than the v0.1 of this plan
(which assumed hiring 4 humans).** This is the framework's
whole point: refineforge replaces the cost of a 4-person team
with the cost of one operator + the compute that runs the
AI-driven specialists.

If the operator handles maintainer duties themselves
(plausible for a senior operator with deep refineforge
fluency), the cash burn drops to **~$12-15k / year** —
essentially just the compute line. That's the lower bound.

## 8. Open questions for the operator BEFORE committing

Smaller list than v0.1; everything else is internal automation:

1. **Maintainer model.** Operator's own time (zero cash) OR
   a part-time hire (~$18-80k/year)? Affects burnout risk.
2. **Fine-tune model size.** 7B vs 13B vs 34B determines
   the 16,000 GPU-hour line's actual sufficiency. ~13B fits
   comfortably; ~70B is tight.
3. **Funding lane.** Option A grants confirmed as Phase 1
   recommended path. Operator's appetite for Option B
   (customer-funded) is the next big decision.
4. **Cloud provider primary + fallback.** Lambda + CoreWeave
   for spot H100; AWS or GCP for grant-eligible fallback.
5. **Whether to fine-tune at all in year 1.** Anthropic API
   on `refine autonomous` already delivers Phase 4 dogfood
   quality at $0.35/claim. The fine-tune is for SCALE (when
   per-claim Anthropic cost × throughput exceeds the fine-tune
   amortized cost) and CONTROL (own model, no vendor
   dependency). Quantify the break-even before committing.
6. **Operator timezone + working pattern.** Affects how
   quickly escalations get decided. The criteria v0.3 contract
   has no auto-expiry — long pendings are visible failures,
   not silent ones — but operator wellness matters.
7. **GUI engineering line (deferred from gui-plan.md).** Does
   `refineforge-studio` get built in year 1 (Tauri + 15-week
   plan), or is the CLI sufficient for year 1 + GUI in year
   2? Affects whether the maintainer slot needs Tauri/Solid
   fluency.

## 9. What this plan is NOT

Honest framing-out:

- **Not a hiring plan.** v0.1 of this doc was. The corrected
  framing: refineforge IS the four specialists. The operator
  doesn't hire Lean + ML + DevOps + CUDA engineers; they run
  refineforge.
- **Not a funding pitch for a 4-person team.** The 12-month
  budget is for ONE operator + ONE part-time maintainer +
  compute. Two orders of magnitude smaller.
- **Not a roadmap for HELYX / Cogn8ty / immortal-* / Knowledge
  Foundry.** Those are operator-side concerns; refineforge
  shares some libraries with them but doesn't own them.
- **Not a commitment.** Numbers are honest estimates; spot
  prices, grant programs, and LLM pricing all shift quarterly.

## 10. v0.1 errata (commit 2f60d9c)

The v0.1 of this plan, shipped in commit `2f60d9c`, framed
the four sections as humans to hire and quoted a $520k-$1.13M
annual burn. That was an inverted reading of the operator's
brief — they listed those four specialist roles as
**capabilities refineforge would provide**, not headcount
lines.

v0.1 is preserved in git history for audit. This v0.2 replaces
it as the canonical operator-facing resourcing plan. The
useful sections of v0.1 — cloud provider pricing, funding
option breakdowns, software library catalog — survive in this
revision; the inverted staffing band is gone.

The lesson for the criteria doc: **future plan docs that
quote multi-FTE budgets for refineforge proper should trip a
review.** refineforge is the framework; the team is the
framework. Anything quoting four specialists as headcount is
probably mis-scoped.
