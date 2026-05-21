# refineforge fine-tuning plan — Mathlib → Knowledge-Foundry → Cogn8ty → axolotl → refineforge `--strategy local-finetune`

> **Status:** PLAN + execution-aligned contract sketch. v0.2.
> This document describes the target pipeline; the local
> scaffolds already landed and the blocked production steps
> are recorded in
> `docs/plans/finetuning-execution-2026-05-20.md`.
> v0.1 (commit `8d119ba`) scoped the pipeline as
> Mathlib → KF → axolotl → refineforge. v0.2 inserts a
> **Cogn8ty symbolic-consistency filter** between KF's
> claim-level gate stack and the trainer, after the Cogn8ty
> brain server was verified live (see refineforge
> `docs/ecosystem.md` v0.2 + the Cogn8ty verification bundle
> captured 2026-05-19). v0.1 stays in git history for audit.
> All compute / spend figures are **honest estimates** keyed
> to `resourcing-plan.md` v0.2 + the Option A grant path.

## 0. What changed from v0.1 — and why

v0.1 assumed the data-production side (Knowledge-Foundry) and
the evaluation side (refineforge-eval) were the only
load-bearing pieces. That is still true. But Cogn8ty is now
verified as a **live system** — its brain server boots, binds
`127.0.0.1:7742`, and answers `brain_reason` JSON-RPC calls
through an 8-tier cognition pipeline that emits a **typed
refusal trace** and a **contradiction array** (verified
2026-05-19; evidence in the Cogn8ty verification bundle).

That changes the architecture of the fine-tuning corpus. The
corpus can now be filtered **twice**:

1. **Claim-level (KF):** Knowledge-Foundry's 14 shared
   anti-hallucination gates + the mode-specific gates filter
   each LLM-proposed repair as a *local* decision — is this
   single patch well-formed, non-refusing, schema-valid,
   provenanced?
2. **Corpus-level (Cogn8ty):** the symbolic substrate checks
   whether the *meaning* of a proposed repair is consistent
   with the surrounding mathematical context — a *global*
   coherence check a local gate stack structurally cannot do.

Training data that survives **both** filters is materially
higher quality than data that survives only one. Most
fine-tuning pipelines have only the first filter. This one
has both, because the operator's portfolio has both
Knowledge-Foundry and Cogn8ty.

**This is the line that differentiates this pipeline from
generic LLM fine-tuning.** Nobody else's proof-repair
fine-tune runs candidate pairs through a pure-Rust NARS +
Prolog consistency checker, because nobody else's portfolio
has one shipped and tested (Cogn8ty: 78 crates, 12,272 tests
passing, verified 2026-05-19).

## 1. Goal + scope

Build a fine-tuned proof-repair model whose `RepairStrategy`
implementation replaces (or augments) `AnthropicStrategy` for
refineforge's autonomous driver. The goal is **lower marginal
cost per repair attempt** (own-weights inference vs per-call
API fees) and **operator-controlled trust base** (no vendor
dependency at the substrate level — consistent with
`docs/why-rust.md`).

The four-project portfolio reads as one pipeline, not as
refineforge + auxiliary tooling (per `docs/ecosystem.md`
v0.2 — *HELYX is the LLM; Cogn8ty thinks for it, KF teaches
it, refineforge proves it*):

| Project | Role in THIS pipeline | State |
|---|---|---|
| **Knowledge-Foundry** (`D:\AI-PROJECTS-GALO\PROJECTS\Knowledge-Foundry`) | Claim-level corpus generation + 14-gate filter | Python; 337 files; ~1,035–1,115 tests passing (validation 2026-05-14) |
| **Cogn8ty / NANTAR INMORTAL RUST** (`D:\AI-PROJECTS-GALO\PROJECTS\NANTAR INMORTAL RUST`) | Corpus-level symbolic-consistency filter | Rust; 78 crates; 12,272 tests passing; quality-gate exit 0 (verified 2026-05-19) |
| **refineforge** (this repo) | Mathlib mutation, eval baseline, trainer orchestration, `local-finetune` strategy | Rust; 9 crates; 383 tests passing |
| **HELYX** (`C:\HELYX`) | The LLM the fine-tune ultimately serves; `helyx-train` is the eventual consumer of corpora produced this way | Rust; 50 crates; 4,643 tests passing |

The pipeline already exists in pieces:
- **Knowledge-Foundry** is a production-grade distillation
  pipeline. 6 training-data modes (`kb_triple`, `sft_pair`,
  `dpo_preference`, `cot_trace`, `embedding_triplet`,
  `tool_call`); 14 shared anti-hallucination gates +
  mode-specific gates; multi-teacher; HuggingFace publishing.
- **Cogn8ty** exposes `brain_reason` over JSON-RPC on
  `127.0.0.1:7742`. The symbolic substrate is `immortal-prolog`
  (SLD resolution + WAM), `immortal-nars` (NARS belief +
  bounded revision), and the contradiction detector inside
  `immortal-cognition`'s 8-tier pipeline.
- **refineforge-trainer** (`crates/refineforge-trainer/`)
  already orchestrates training runs over axolotl /
  HuggingFace Trainer / custom backends, with checkpoint
  resume + failure recovery + JSON reports.
- **refineforge-eval** (`crates/refineforge-eval/`) already
  drives `RepairStrategy` against a JSONL corpus + emits
  metrics.

What's still MISSING for the production run is the expensive
and operator-gated wiring:
- A Mathlib mutation pipeline producing the broken-Lean
  + ground-truth-fix corpus (operator-side; multi-week per
  `docs/repair-evaluation.md` §9).
- A full KF teacher run over that corpus.
- A live full-corpus Cogn8ty consistency pass over the
  KF-gate-passed pairs.
- Real checkpoint production and baseline comparison.
- Native candle/tch checkpoint loading once the final
  architecture and tokenizer layout are known.

The first local scaffolds are already tracked in
`docs/plans/finetuning-execution-2026-05-20.md`: the KF Lean
probe/gates, the Cogn8ty gate contract, a refineforge trainer
smoke fixture, and a `local-finetune` command-manifest
strategy bridge. This plan ties the remaining production gaps
together before real spend.

## 2. End-to-end data flow

```
┌───────────────────────────────────────────────────────────────┐
│ Mathlib mutation pipeline (operator side, multi-week)         │
│ - Scrape Mathlib                                              │
│ - Apply systematic mutations (8 kinds per repair-evaluation §3)│
│ - Validate each mutation actually breaks the proof            │
│ - Record (broken_lean, original_lean, mutation_kind) triples  │
│ → out: ~N≥1000 broken-Lean entries                            │
└──────────────────────────┬────────────────────────────────────┘
                           │
                           ▼
┌───────────────────────────────────────────────────────────────┐
│ Knowledge-Foundry sft_pair mode + Lean probe set (§10)        │
│ - Anthropic teacher answers each (broken_lean, diagnostic)    │
│   with an LSP-shaped patch JSON                               │
│ - 14 shared gates + NEW gate: patch_well_formed               │
│ → out: gate-filtered candidate corpus                         │
│   (CLAIM-LEVEL filter — local, per-patch)                     │
└──────────────────────────┬────────────────────────────────────┘
                           │
                           ▼
┌───────────────────────────────────────────────────────────────┐
│ ★ Cogn8ty symbolic consistency check  (NEW in v0.2)           │
│ - For each gate-passed pair, form the implied                 │
│   theorem-after-repair                                        │
│ - Call brain_reason on 127.0.0.1:7742                         │
│ - DROP the pair if the response carries EvidenceConflict,     │
│   a non-empty contradictions[] array, or NoDomainMatch with   │
│   knowledge_gap against the surrounding mathematical context  │
│ - KEEP the pair if the symbolic check is clean                │
│ → out: *consistency-filtered* corpus                          │
│   (CORPUS-LEVEL filter — global, semantic)                    │
│ → rejection log: symbolically-inconsistent pairs recorded     │
│   (see Open Questions §9b — log location is a v0.3 contract)  │
└──────────────────────────┬────────────────────────────────────┘
                           │
                           ▼
┌───────────────────────────────────────────────────────────────┐
│ refineforge-eval acceptance pre-gate                          │
│ - Split corpus 80/20 train/eval                               │
│ - Run claude-opus-4-7 baseline against the eval split          │
│ - Record repair-rate, p50/p95 latency, cost-per-attempt        │
│ → out: baseline.json (target fine-tuned model must beat)      │
└──────────────────────────┬────────────────────────────────────┘
                           │
                           ▼
┌───────────────────────────────────────────────────────────────┐
│ axolotl fine-tune (per refineforge-trainer scaffolding)       │
│ - Base model: Qwen2.5-Coder-1.5B → 7-13B as quality permits    │
│ - LoRA + QLoRA; FSDP for distributed                          │
│ - refineforge-trainer monitors progress.jsonl + checkpoints    │
│ → out: fine-tuned weights (safetensors)                       │
└──────────────────────────┬────────────────────────────────────┘
                           │
                           ▼
┌───────────────────────────────────────────────────────────────┐
│ refineforge --strategy local-finetune                          │
│ - New module: refineforge-strategies/src/local_finetune.rs     │
│ - Loads weights via candle (Rust-native; tch-rs fallback)      │
│ - Implements RepairStrategy::propose_patch                     │
└──────────────────────────┬────────────────────────────────────┘
                           │
                           ▼
┌───────────────────────────────────────────────────────────────┐
│ refineforge-eval acceptance gate                              │
│ - Run fine-tuned strategy against the held-out eval split      │
│ - GATE: fine-tuned ≥ baseline OR document the gap + iterate    │
└───────────────────────────────────────────────────────────────┘
```

The two-filter intersection is the point. A broken-Lean +
repair pair survives KF's gates if the repair is well-formed,
multi-probe-supported, schema-valid, and provenanced. It
survives Cogn8ty's check if the *meaning* of the repair (the
implied theorem-after-repair) is symbolically consistent with
the surrounding mathematical context. The intersection of
"well-formed repair" and "semantically sensible repair" is
much smaller than either set alone — and a model trained on
that intersection should be substantially better than one
trained on KF-filtered data alone.

Per `docs/why-rust.md` reviewer FAQ: training happens in
PyTorch via axolotl; the fine-tuned model is consumed via
Rust-native candle (or burn) in deployment. The
training/deployment language asymmetry is acknowledged.

## 3. Phase-by-phase

### Phase 0 — Pre-work (operator)

- Confirm Anthropic API key + sufficient budget for the
  teacher run (~$50-300 for corpus generation; cost depends
  on N + base teacher model).
- Confirm the cargo target dir is pinned and stable across
  phases.
- Decide N (1000 / 2500 / 5000 corpus entries) per
  `docs/repair-evaluation.md` §2.

**Time:** ~1 day. Operator-only.

### Phase 1 — KF probe set authoring

- Copy §10's probe-set spec into Knowledge-Foundry's tree:
  - `kb_destiller/modes/sft_pair/probes/lean_proof_repair.yaml`
  - `kb_destiller/modes/sft_pair/presets/lean_proof_repair.yaml`
  - `kb_destiller/modes/sft_pair/gates/patch_well_formed.py`
    (new gate; ~50-line Python file)
- Add probe-set unit tests in KF's pytest suite.
- Smoke-run the probe set against a tiny (10-20 entry)
  fixture corpus.

**Time:** 3-5 days. Knowledge-Foundry maintainer effort.

**Acceptance:**
- `python -m pytest kb_destiller/modes/sft_pair/tests` passes.
- A 10-entry smoke corpus generates without quality-gate
  failures.

### Phase 2 — Mathlib mutation pipeline (operator side)

- Per `docs/repair-evaluation.md` §9. Multi-week elapsed.
- Output: ~N broken-Lean entries with ground truth + mutation
  metadata.
- Per Mathlib's license: ensure attribution is propagated
  into the corpus's `dataset_card.py` output.

**Time:** 3-6 weeks elapsed (operator throughput-bound, not
blocking other phases).

**Acceptance:**
- N≥1000 entries, each with `(broken_lean, original_lean,
  mutation_kind)` triple verified by `lake build` (broken =
  fails; original = passes).

### Phase 3 — KF corpus generation (parallel with Phase 2)

- Use KF's existing `multi_teacher.py` to route the
  broken-Lean entries through the Lean probe set.
- Anthropic teacher answers each entry.
- The 14 shared gates + `patch_well_formed` run; failures
  dropped.
- KF's `pack_writer.py` packages the gate-passed output.

**Cost:**
- Anthropic teacher calls: ~$0.07-0.15 per entry × N.
- N=1000: **~$70-150**; N=5000: **~$350-750**.

**Time:** 1-2 weeks elapsed (KF runs are fast; the bottleneck
is Anthropic rate limits + the operator reviewing the first
~100 outputs).

**Acceptance:**
- Gate-passed corpus has patch JSON that round-trips through
  `Patch::apply`, validates `start <= end`, and doesn't
  reference `sorry` / `admit` / non-core axioms.

### Phase 3.5 — ★ Cogn8ty symbolic consistency check (NEW in v0.2)

This is the unique-to-NANTAR contribution. The step takes the
KF-gate-passed corpus and runs each pair through Cogn8ty's
symbolic substrate, dropping pairs whose repair is
semantically inconsistent with the surrounding mathematical
context.

**Mechanism (verified API surface, 2026-05-19):**

1. Start the Cogn8ty brain server:
   `cogn8ty brain start` (binds `127.0.0.1:7742`).
2. For each gate-passed `(broken_lean, repair_patch)` pair,
   form a natural-language statement of the implied
   theorem-after-repair (the probe set §10 should emit this
   alongside the patch — see Open Question §9b-2).
3. POST to the brain server:
   ```json
   {"jsonrpc":"2.0","id":N,"method":"brain_reason",
    "params":{"request":{"text":"<implied theorem statement>"}}}
   ```
4. Inspect the response's `result`:
   - `refusal_trace.reasons[]` — typed variants observed in
     verification include `EvidenceConflict{source_a,
     source_b, conflict_summary}`, `NoDomainMatch`,
     `MetacognitionSuppressed`, `WeakCognitiveDimension`.
   - `contradictions[]` — `{fact_a, fact_b, reason}` triples.
   - `confidence`, `cii_score`.
5. **Drop** the pair if `contradictions[]` is non-empty OR
   `refusal_trace.reasons` contains an `EvidenceConflict`
   against the surrounding context. **Keep** otherwise.
   (`NoDomainMatch` alone is NOT a drop signal — it means
   Cogn8ty's KB has no coverage for the math domain, which is
   expected for Mathlib content and is not evidence of
   inconsistency. See Open Question §9b-3.)
6. Log every dropped pair with its trace to a rejection log.

**Cost:** $0 — Cogn8ty runs locally, no API fees.

**Latency (from verified measurement, 2026-05-19):**
`brain_reason` latency was 694 ms – 3,064 ms per query on
Windows CPU-only. For a gate-passed corpus of N pairs:
- N=1000: ~12 min – 51 min serial; minutes if parallelized
  across brain-server worker threads.
- N=5000: ~1 – 4 hr serial.
The check is embarrassingly parallel (stateless per-claim);
the brain server already handles concurrent JSON-RPC. See
Open Question §9b-4 for the corpus-scale latency budget.

**Time:** 2-4 days active (write the KF↔Cogn8ty bridge
script + the rejection log; run the check; review the first
~50 drops to confirm they are genuine inconsistencies and not
false positives from KB-coverage gaps).

**Acceptance:**
- Every KF-gate-passed pair has been through `brain_reason`.
- The consistency-filtered corpus is strictly smaller than
  the gate-passed corpus (if it is not, the check is a no-op
  and the bridge is misconfigured — investigate).
- The rejection log records each drop with the brain trace
  that justified it (auditable — a reviewer can re-run any
  single drop).
- A spot-check of ~50 dropped pairs confirms the drops are
  genuine semantic inconsistencies, not KB-coverage false
  positives.

### Phase 4 — refineforge-eval baseline establishment

- Split the consistency-filtered corpus 80/20 train/eval.
- Run claude-opus-4-7 against the eval split via the existing
  refineforge-eval harness.
- Record baseline metrics: repair-rate, p50/p95 latency, cost
  per attempt, per-mutation-kind breakdown.

**Cost:** ~$0.07 × 0.2 × N (post-filter N).

**Time:** 1-3 days elapsed.

**Acceptance:**
- `eval/runs/baseline-claude-opus-4-7.json` shipped.
- Numbers documented in `docs/repair-evaluation.md` §results.

### Phase 5 — axolotl fine-tune

- Use `refineforge-trainer` to orchestrate (existing scaffold).
- Base model: start with Qwen2.5-Coder-1.5B for the smoke
  fine-tune; if quality looks promising, scale to 7B or 13B.
- LoRA + QLoRA at r=64; FSDP for >7B if used.

**Cost / resourcing arc (honest, staged):**
- **First signal ($50-200):** smoke fine-tune of
  Qwen2.5-Coder-1.5B on Lambda/CoreWeave spot. Answers "does
  the consistency-filtered corpus train a model that does
  *anything* sensible?" — go/no-go before any larger spend.
- **Working model ($5K-$20K):** a 7-13B production fine-tune
  that beats `claude-opus-4-7` on the refineforge-eval
  harness. Funded via Option A grants (NVIDIA Inception +
  AWS Activate + CoreWeave/Lambda startup credits — see
  `resourcing-plan.md` §4); $5-20K is the cash outlay against
  a larger cloud-equivalent.
- **Full-scale training (16,000 H100-hours):** reserved
  **only after** the smaller experiments justify it. Not a
  day-one commitment. The 16K-hour line is a ceiling, not a
  plan.

**Time:**
- Smoke fine-tune: ~1-3 days wall-clock.
- Production fine-tune: ~2-6 weeks wall-clock (subject to
  spot preemption; checkpoint resume already supported).

**Acceptance:**
- Training loss converges or plateaus.
- Checkpoint manifest in `refineforge-trainer`'s `report.json`.
- Per-epoch eval pass (small sample) shows monotone-or-flat
  improvement.

### Phase 6 — refineforge `--strategy local-finetune` wiring

First bridge shipped:
`crates/refineforge-strategies/src/local_finetune.rs`.

- Current shipped path loads a weights/runtime directory
  containing `refineforge-local-finetune.json`, invokes the
  declared command runtime, parses patch JSON, and records
  local token usage.
- Production checkpoint loading still targets `candle`
  (Rust-native; consistent with `docs/why-rust.md`) with a
  `tch-rs` fallback if candle lacks the chosen base
  architecture.
- Implements `RepairStrategy::propose_patch`.
- New factory `refineforge_strategies::local_finetune_from_path`.
- New `resolve_strategy` arm in
  `refineforge-cli/src/autonomous/executor.rs`.

**Time:** 1-2 weeks elapsed.

**Acceptance:**
- `refine repair <claim> --strategy local-finetune
  --weights-path <path>` runs end-to-end against EXAMPLE-002.
- Token counts surface in `UsageStats`.

### Phase 7 — Acceptance gate against baseline

- Run `refine-eval --corpus eval/corpus/full.jsonl --strategy
  local-finetune` against the held-out eval split.
- Compare to `baseline-claude-opus-4-7.json` from Phase 4.
- **Gate:** repair-rate of fine-tuned ≥ baseline. If not:
  document the gap honestly, iterate, don't ship a regression.

**Time:** 1-3 days elapsed.

**Acceptance:**
- `eval/runs/finetune-v1.json` shipped.
- Comparison shows ≥-parity OR the iteration decision is
  documented.

### Phase 8 — Documentation + criteria v0.4 review

- Update `docs/llm-repair-design.md` for the new
  `local-finetune` strategy.
- Update `docs/repair-evaluation.md` results section.
- **Criteria v0.4 review:** switching from `anthropic` to
  `local-finetune` is a Cat 8 (trust-base) escalation — the
  weights are now part of the trust base; operator signs off
  explicitly.
- New refineforge release if the fine-tune ships.

**Time:** 3-5 days.

## 4. Total honest estimate

| Phase | Days (active) | Calendar | Cost (Option A path) |
|---|---:|---:|---:|
| 0 — Pre-work | 1 | 1 day | $0 |
| 1 — KF probe set | 3-5 | 1 week | $0 |
| 2 — Mathlib mutation | (multi-week elapsed) | 3-6 weeks | $0-200 cloud burst |
| 3 — KF corpus generation | 5-10 | 1-2 weeks | $70-750 Anthropic |
| 3.5 — Cogn8ty consistency check | 2-4 | 2-4 days | $0 (runs locally) |
| 4 — Baseline establishment | 1-3 | 1-3 days | $14-70 Anthropic |
| 5 — Smoke + production fine-tune | 7-14 | 2-6 weeks | $50-200 first signal; $5-20k working model |
| 6 — `--strategy local-finetune` wiring | 7-14 | 1-2 weeks | $0 |
| 7 — Acceptance gate | 1-3 | 1-3 days | trivial |
| 8 — Docs + release | 3-5 | 3-5 days | $0 |
| **Total active engineering** | **~32-64 days** | — | — |
| **Total elapsed** | — | **~8-13 weeks** | **~$5-21k cash** |

= **~2-3 calendar months** with one focused operator + one
part-time maintainer (per `resourcing-plan.md` v0.2).
Bottleneck is Phase 2 (Mathlib mutation throughput) + Phase 5
(training wall-clock), neither of which active engineering
can compress. Phase 3.5 adds 2-4 days and **$0** — the
consistency filter is the cheapest quality lever in the
pipeline.

## 5. Resource requirements

Inherits from `resourcing-plan.md` v0.2:
- **1 human operator** (you).
- **1 part-time refineforge maintainer** (you, or ~0.3 FTE
  contractor).
- **0 additional hires.** KF + Cogn8ty + refineforge +
  axolotl + Anthropic teacher are all already in the stack.
- **Compute:** Option A grants ($5-20k cash outlay).
- **Anthropic API:** ~$100-1000 over the fine-tune lifetime.
- **Cogn8ty brain server:** runs locally during Phase 3.5;
  no incremental cost; one CPU box is sufficient (verified
  CPU-only 2026-05-19).

## 6. Risks

| Risk | Severity | Mitigation |
|---|---|---|
| Mathlib mutation throughput too slow → Phase 2 takes 3+ months | **HIGH** schedule | Smoke fine-tune in Phase 5 uses synthetic mutations from the existing eval corpus; don't gate phases 5-7 on a fully-sized Phase 2 |
| Cogn8ty consistency check drops too aggressively (KB-coverage false positives mistaken for inconsistency) | **MEDIUM** | Phase 3.5 acceptance requires a 50-drop spot-check; `NoDomainMatch` alone is explicitly NOT a drop signal; tune the drop predicate on the spot-check results before the full run |
| Cogn8ty consistency check is a no-op (drops nothing) | **MEDIUM** | Phase 3.5 acceptance flags this; if the filtered corpus equals the gate-passed corpus, the bridge is misconfigured (wrong claim text, wrong response field) — investigate before Phase 4 |
| Cogn8ty's KB has near-zero coverage for advanced Mathlib domains → consistency check has little to check | **MEDIUM** | Honest open question §9b-3; the check still catches *internal* contradictions (a repair that asserts X and ¬X); KB-grounded checks are a bonus, not the floor |
| KF Anthropic-teacher refusals on Lean content | **MEDIUM** | KF's `no_refusal` gate filters; backup teacher (Gemini / GPT-4o) as fallback |
| Fine-tuned model underperforms baseline | **MEDIUM** | Phase 7 gate says "don't ship a regression"; iterate on data + base + prompt |
| candle doesn't support the chosen base architecture | **MEDIUM** | tch-rs fallback documented; doesn't block ship |
| Grant credits expire before Phase 5 | **MEDIUM** | Phases 1-4 run on cash (~$100-1000); only Phase 5 needs GPU credits; time grant onboarding accordingly |
| Production fine-tune hallucinates `sorry`/`admit` | **MEDIUM** | No-sorry policy gate catches at apply-time; LoRA training penalty on those tokens further suppresses |

## 7. Definition of done

1. ✅ KF probe set (`lean_proof_repair`) lands in
   Knowledge-Foundry with passing tests.
2. ✅ Mathlib mutation pipeline produces ≥1000 entries with
   verified-broken Lean.
3. ✅ KF corpus is gate-filtered and packaged.
4. ✅ **Cogn8ty consistency check has run over the full
   gate-passed corpus; rejection log shipped; 50-drop
   spot-check confirms genuine inconsistencies.**
5. ✅ Baseline `claude-opus-4-7` numbers documented.
6. ✅ Fine-tuned model checkpoint(s) shipped.
7. ✅ `refine repair <claim> --strategy local-finetune` runs
   end-to-end.
8. ✅ Acceptance gate: fine-tuned ≥ baseline OR documented
   gap + iteration plan.
9. ✅ Criteria v0.4 reflects the new trust-base entry.
10. ✅ New refineforge release tag if the fine-tune ships.

## 8. Out of scope (explicitly)

- **70B+ base models.** Out of budget for the first
  fine-tune.
- **DPO / RLHF on the fine-tuned model.** Separate plan.
- **Multi-language proof systems.** Lean 4 only for now.
- **Real-time online learning.** The fine-tune is a
  point-in-time artifact.
- **Model serving as a SaaS API.** refineforge consumes the
  weights locally.
- **Compiling Cogn8ty into refineforge as a Rust dependency.**
  Phase 3.5 uses the brain server over JSON-RPC, not an
  in-process crate dependency. The in-process path is a
  separate, larger scope (see `docs/ecosystem.md` §7.4 for
  the analogous HELYX↔Cogn8ty integration).

## 9. Open questions

### 9a. For the operator (corpus + training decisions)

1. **N (corpus size).** 1000 / 2500 / 5000? Affects Phase 2
   wall-clock + Phase 3 cost.
2. **Base model.** Qwen2.5-Coder-1.5B (smoke) → 7B or 13B
   (production)?
3. **Inference runtime.** candle (Rust-native) vs tch-rs
   (PyTorch FFI)? `why-rust.md` favors candle.
4. **HuggingFace publishing.** Publish corpus + weights
   publicly, or keep internal?
5. **Teacher diversity.** Anthropic-only or multi-teacher?
6. **CoT-trace mode.** sft_pair only, or add cot_trace?
7. **Operator availability for Phase 5 monitoring.**

### 9b. For v0.3 — the KF ↔ Cogn8ty contract (NEW in v0.2)

These four were flagged as open in the v0.2 review. Partial
answers below come from the Cogn8ty verification
(2026-05-19); the unresolved parts are genuine v0.3 work.

1. **Which Cogn8ty crate provides the consistency-check
   entry point?** *Partially answered.* The verified entry
   point is `immortal-brain`'s `brain_reason` JSON-RPC method
   on `127.0.0.1:7742`. The symbolic substrate underneath is
   `immortal-prolog` + `immortal-nars` + the contradiction
   detector in `immortal-cognition`. **Unresolved:** whether
   Phase 3.5 should call the JSON-RPC server (simple, but a
   network hop + a running-server dependency) or compile
   `immortal-prolog` / `immortal-nars` directly as a Rust
   dependency of a small bridge crate (no server, but more
   integration work). v0.2 assumes JSON-RPC; v0.3 decides.

2. **What is the contract between KF and Cogn8ty for passing
   a corpus through the symbolic system?** *Unresolved.*
   Needs: (a) a defined input — does Cogn8ty receive the raw
   patch JSON, or a natural-language "implied theorem"
   statement the KF probe set must additionally emit?
   v0.2's Phase 3.5 assumes the latter and adds it as a probe
   requirement (§10) — confirm in v0.3. (b) a defined output
   contract — which response fields are the drop signal
   (`contradictions[]` non-empty + `EvidenceConflict` in
   `refusal_trace.reasons`). (c) batch vs per-claim API —
   the brain server is per-claim today; a batch endpoint
   would cut HTTP overhead at corpus scale.

3. **Where does the rejection log for symbolically-inconsistent
   repairs live?** *Partially answered.* KF already has a
   `rejection_log.py` gate concept (`kb_destiller/gates/`).
   The cleanest design: a new KF gate `cogn8ty_consistent.py`
   that calls the brain server and writes drops to the same
   rejection-log sink KF's other gates use — so symbolic
   drops and claim-level drops are in one auditable place.
   **Unresolved:** whether the Cogn8ty trace (which can be
   multi-KB and large) is stored inline in the rejection log
   or by reference (trace hash → separate trace store).
   v0.3 decides.

4. **What is the latency budget for the consistency check at
   corpus scale?** *Partially answered.* Measured
   `brain_reason` latency 2026-05-19 was 694 ms – 3,064 ms
   per query, CPU-only. Serial worst case: N=5000 → ~4 hr.
   The check parallelizes cleanly (stateless per-claim; the
   server handles concurrent JSON-RPC). **Unresolved:** the
   target — is a 4-hour serial pass acceptable as a one-time
   batch step (probably yes, it runs once per corpus), or
   does it need to be minutes (then: parallel workers + a
   batch endpoint)? v0.3 sets the budget once N is fixed.

## 10. Appendix — Probe-set spec (files to copy into Knowledge-Foundry)

When Phase 1 starts, the operator copies these files into
`D:\AI-PROJECTS-GALO\PROJECTS\Knowledge-Foundry\`.

> **v0.2 note:** the probe templates below add a
> `implied_theorem` field to the response shape — a
> natural-language statement of what the file proves *after*
> the repair is applied. Phase 3.5's Cogn8ty check consumes
> that field. If v0.3 decides the consistency check should
> operate on raw patch JSON instead, this field becomes
> optional.

### 10.1 — `kb_destiller/modes/sft_pair/probes/lean_proof_repair.yaml`

```yaml
# Lean 4 proof-repair probes for refineforge.
#
# Each probe emits a prompt + expected response shape pair
# suitable for SFT fine-tuning a code-repair model. The probe
# templates use {topic} substitution where {topic} is the
# specific mutation case (e.g. "wrong-tactic", "swap-lemma",
# "rename-field").
#
# See docs/plans/finetuning-plan.md in the refineforge repo
# for the full data-flow context.

- id: lean_proof_repair_v1
  template: |
    SFT-LEAN-PROOF-REPAIR topic={topic}

    The user will give you ONE diagnostic and the full source
    of one Lean 4 file. Propose ONE minimal patch that, when
    applied, may fix the diagnostic.

    Constraints:
    - You MUST NOT use sorry, admit, or non-core axiom
      declarations.
    - Prefer the smallest possible patch range.
    - Return strict JSON with keys: prompt, response.
    - The "prompt" field is the diagnostic + file body
      verbatim (preserved for the eval harness).
    - The "response" field is a single JSON object with
      keys: start_line, start_char, end_line, end_char,
      new_text, rationale, implied_theorem (each line/char
      0-indexed; LSP convention; implied_theorem is a
      natural-language statement of what the file proves
      after the patch — consumed by the Cogn8ty consistency
      check, finetuning-plan §3.5).

    Generate ONE training pair where:
    - prompt = realistic Lean 4 diagnostic message +
      surrounding source file
    - response = the LSP-shaped patch JSON the model should
      learn to emit

- id: lean_proof_repair_with_context
  template: |
    SFT-LEAN-PROOF-REPAIR-WITH-CONTEXT topic={topic}

    Same as lean_proof_repair_v1 but the prompt additionally
    includes the imports + the module's full signature
    context (helpful for type-driven repairs).

    Return strict JSON with keys: prompt, response.
```

### 10.2 — `kb_destiller/modes/sft_pair/presets/lean_proof_repair.yaml`

```yaml
preset: lean_proof_repair
sft_templates:
  - lean_proof_repair_v1
  - lean_proof_repair_with_context
response_min_tokens: 30
response_max_tokens: 800
quality_floor: 0.65
min_jaccard_distance: 0.10
allow_pii_by_design: false
dataset_license_recommendation: Apache-2.0
# Lean / Mathlib are Apache-2.0; the corpus inherits.

# Lean-proof-repair-specific gates that augment the
# common sft_pair gates.
extra_gates:
  - patch_well_formed
  - patch_within_bounds
  - no_sorry_admit_axiom
```

### 10.3 — `kb_destiller/modes/sft_pair/gates/patch_well_formed.py`

```python
"""patch_well_formed.py — gate for Lean proof-repair sft_pair entries.

Validates that the response field parses as JSON with the
required LSP-shaped keys. Drops entries whose response is
either non-JSON or missing fields.

Per the refineforge fine-tuning plan §10.3.
"""

from __future__ import annotations
import json
from kb_destiller.modes.sft_pair.gates.common import GateResult


REQUIRED_KEYS = {
    "start_line",
    "start_char",
    "end_line",
    "end_char",
    "new_text",
}


def evaluate(entry: dict) -> GateResult:
    response = entry.get("response", "")
    if not isinstance(response, str):
        return GateResult(passed=False, reason="response is not a string")
    try:
        patch = json.loads(response)
    except json.JSONDecodeError as e:
        return GateResult(passed=False, reason=f"response not JSON: {e}")
    if not isinstance(patch, dict):
        return GateResult(passed=False, reason="patch is not a JSON object")
    missing = REQUIRED_KEYS - set(patch.keys())
    if missing:
        return GateResult(
            passed=False,
            reason=f"patch missing required keys: {sorted(missing)}",
        )
    # Integer-range sanity.
    for k in ("start_line", "start_char", "end_line", "end_char"):
        v = patch.get(k)
        if not isinstance(v, int) or v < 0:
            return GateResult(
                passed=False,
                reason=f"patch.{k} must be a non-negative int (got {v!r})",
            )
    # End >= start invariant (line-then-char).
    sl, sc = patch["start_line"], patch["start_char"]
    el, ec = patch["end_line"], patch["end_char"]
    if (el, ec) < (sl, sc):
        return GateResult(
            passed=False,
            reason=f"patch end {(el, ec)} precedes start {(sl, sc)}",
        )
    # new_text must be a string.
    if not isinstance(patch.get("new_text"), str):
        return GateResult(passed=False, reason="patch.new_text is not a string")
    return GateResult(passed=True, reason="patch_well_formed")
```

### 10.4 — `kb_destiller/gates/cogn8ty_consistent.py` (NEW in v0.2 — Phase 3.5)

> **Sketch only.** This gate is the KF ↔ Cogn8ty bridge. Its
> exact shape depends on Open Question §9b-2 (the contract).
> Shown here so Phase 1 authoring has a target; finalize in
> v0.3.

```python
"""cogn8ty_consistent.py — corpus-level symbolic consistency gate.

Routes a gate-passed sft_pair entry's implied theorem through
the Cogn8ty brain server (brain_reason JSON-RPC) and drops the
entry if the symbolic substrate reports a contradiction.

DROP signal:  response.result.contradictions is non-empty
              OR refusal_trace.reasons contains EvidenceConflict
KEEP signal:  clean trace, or NoDomainMatch only (KB-coverage
              gap is not inconsistency — see finetuning-plan
              §9b-3).

Requires the brain server running on 127.0.0.1:7742.
Per the refineforge fine-tuning plan §3.5 + §10.4.
"""

from __future__ import annotations
import json
import urllib.request
from kb_destiller.gates.common import GateResult  # adjust import per KF tree

BRAIN_URL = "http://127.0.0.1:7742"


def _brain_reason(text: str) -> dict:
    body = json.dumps({
        "jsonrpc": "2.0", "id": 1, "method": "brain_reason",
        "params": {"request": {"text": text}},
    }).encode()
    req = urllib.request.Request(
        BRAIN_URL, data=body,
        headers={"Content-Type": "application/json"},
    )
    with urllib.request.urlopen(req, timeout=60) as resp:
        return json.loads(resp.read())


def evaluate(entry: dict) -> GateResult:
    response = entry.get("response", "")
    try:
        patch = json.loads(response)
    except (json.JSONDecodeError, TypeError):
        # patch_well_formed should have caught this already.
        return GateResult(passed=False, reason="response not JSON")
    implied = patch.get("implied_theorem")
    if not implied:
        # No theorem statement to check — pass through, but flag.
        return GateResult(passed=True, reason="no implied_theorem; skipped")
    rpc = _brain_reason(implied)
    result = rpc.get("result", {})
    contradictions = result.get("contradictions") or []
    if contradictions:
        return GateResult(
            passed=False,
            reason=f"cogn8ty: {len(contradictions)} contradiction(s): "
                   f"{contradictions[0].get('reason', '')}",
        )
    reasons = (result.get("refusal_trace") or {}).get("reasons") or []
    for r in reasons:
        if isinstance(r, dict) and "EvidenceConflict" in r:
            ec = r["EvidenceConflict"]
            return GateResult(
                passed=False,
                reason=f"cogn8ty: EvidenceConflict — "
                       f"{ec.get('conflict_summary', '')}",
            )
    return GateResult(passed=True, reason="cogn8ty_consistent")
```

### 10.5 — Test stubs for KF's pytest

`kb_destiller/modes/sft_pair/tests/test_lean_proof_repair_probe.py`:
- Loads the probe set; asserts both probe ids parse.
- Asserts the preset references both probe ids.
- Asserts `patch_well_formed` accepts a known-good entry +
  rejects each failure mode.

`kb_destiller/gates/tests/test_cogn8ty_consistent.py`:
- Mocks `_brain_reason` (no live server in unit tests).
- Asserts a clean trace → pass.
- Asserts a non-empty `contradictions[]` → fail.
- Asserts an `EvidenceConflict` reason → fail.
- Asserts `NoDomainMatch`-only → pass (KB-gap is not
  inconsistency).
- A live-server integration test is marked `@pytest.mark.e2e`
  (KF's pytest.ini already excludes slow marks by default).

### 10.6 — Operator's first run after copying

```bash
# In Knowledge-Foundry repo
python -m pytest kb_destiller/modes/sft_pair/tests/test_lean_proof_repair_probe.py
python -m pytest kb_destiller/gates/tests/test_cogn8ty_consistent.py

python -m kb_destiller.cli run \
    --preset lean_proof_repair \
    --teacher anthropic \
    --target-n 10 \
    --out fixtures/smoke_corpus.jsonl
# Inspect fixtures/smoke_corpus.jsonl

# Phase 3.5 smoke — needs the brain server running:
#   (separate shell)  cogn8ty brain start
python -m kb_destiller.cli gate \
    --gate cogn8ty_consistent \
    --in fixtures/smoke_corpus.jsonl \
    --out fixtures/smoke_corpus_consistent.jsonl \
    --rejection-log fixtures/smoke_rejections.jsonl
```

## 11. What this plan does NOT cover

- **Real fine-tune execution.** The plan stops at "ready to
  execute Phase 5."
- **Replacement of `AnthropicStrategy`.** The fine-tuned
  strategy is ADDITIVE; AnthropicStrategy stays as baseline +
  fallback.
- **The KF↔Cogn8ty contract finalization.** v0.2 sketches the
  bridge (§10.4) and names the open questions (§9b); v0.3
  finalizes the contract once N is fixed and the
  JSON-RPC-vs-in-process decision is made.
- **Continual updates as Mathlib evolves.** The corpus is a
  point-in-time snapshot.
- **HELYX-specific proof patterns.** The fine-tune is on
  Mathlib mutations (general Lean). HELYX-specific patterns
  need their own probe set + corpus.

The plan does NOT commit to any of the numbers — they're
**honest estimates** for the operator to sharpen against
quotes from grant officers + cloud sales reps before the
first dollar gets spent.

## 12. Provenance

- **Authored by:** Galo Serrano Abad (operator) + Claude Code
  (Opus 4.7).
- **v0.1 → v0.2 delta:** v0.1 (commit `8d119ba`) was a
  three-stage pipeline (Mathlib → KF → axolotl → refineforge).
  v0.2 inserts Phase 3.5 — the Cogn8ty symbolic-consistency
  filter — after Cogn8ty was verified as a live system on
  2026-05-19 (brain server boots, binds 7742, `brain_reason`
  returns typed refusal traces + contradiction arrays). The
  Cogn8ty API surface, latency figures, and refusal-reason
  variants cited in §3.5 and §9b come from that verification,
  not from estimates.
- v0.1 stays in git history for audit.
