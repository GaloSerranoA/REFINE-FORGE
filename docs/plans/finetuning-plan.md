# refineforge fine-tuning plan — Knowledge-Foundry → axolotl → refineforge `--strategy local-finetune`

> **Status:** PLAN ONLY. v0.1. The probe-set spec in §10 is
> the file the operator copies into `Knowledge-Foundry` on
> their own schedule; the Knowledge-Foundry repo is not
> mutated by this commit. All compute / spend figures are
> **honest estimates** keyed to `resourcing-plan.md` v0.2 +
> the Option A grant path.

## 1. Goal + scope

Build a fine-tuned proof-repair model whose `RepairStrategy`
implementation replaces (or augments) `AnthropicStrategy` for
refineforge's autonomous driver. The goal is **lower
marginal cost per repair attempt** (own-weights inference vs
per-call API fees) and **operator-controlled trust base** (no
vendor dependency at the substrate level — consistent with
`docs/why-rust.md`).

The pipeline already exists in pieces:
- **Knowledge-Foundry** (`D:\AI-PROJECTS-GALO\PROJECTS\Knowledge-Foundry`)
  is a production-grade distillation pipeline. 330 source
  files; 1115 tests passing; 5 training-data modes
  (`sft_pair`, `dpo_preference`, `cot_trace`,
  `embedding_triplet`, `tool_call`); multi-teacher support;
  HuggingFace publishing.
- **refineforge-trainer** (`crates/refineforge-trainer/`)
  already orchestrates training runs over axolotl /
  HuggingFace Trainer / custom backends, with checkpoint
  resume + failure recovery + JSON reports.
- **refineforge-eval** (`crates/refineforge-eval/`) already
  drives `RepairStrategy` against a JSONL corpus + emits
  metrics.

What's MISSING is the wiring:
- A KF probe set tuned for Lean proof repair (§10 in this
  doc).
- A Mathlib mutation pipeline producing the broken-Lean
  + ground-truth-fix corpus (operator-side; multi-week per
  `docs/repair-evaluation.md` §9).
- A refineforge strategy that loads fine-tuned weights +
  serves the existing `RepairStrategy` trait
  (`refineforge-strategies/src/local_finetune.rs`).

This plan ties those gaps together. **No code lands in this
commit; the plan is reviewable before any spend.**

## 2. End-to-end data flow

```
┌───────────────────────────────────────────────────────────────┐
│ Mathlib mutation pipeline (operator side, multi-week)         │
│ - Scrape Mathlib                                              │
│ - Apply systematic mutations (8 kinds per repair-evaluation §3) │
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
│ - Quality gates: response_length, no_refusal, prompt_diversity│
│   + NEW gate: patch_well_formed (validates JSON shape +       │
│   start_line/start_char/end_line/end_char/new_text bounds)    │
│ → out: HuggingFace dataset packaged via KF's pack_writer       │
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
│ - Base model: Qwen2.5-Coder-1.5B → 13B as quality permits      │
│ - LoRA + QLoRA; FSDP for distributed (resourcing §3.1)        │
│ - 16,000 GPU-hours via Option A grants (§4)                    │
│ - refineforge-trainer monitors progress.jsonl + saves          │
│   checkpoints                                                  │
│ → out: fine-tuned weights (safetensors)                       │
└──────────────────────────┬────────────────────────────────────┘
                           │
                           ▼
┌───────────────────────────────────────────────────────────────┐
│ refineforge --strategy local-finetune                          │
│ - New crate module: refineforge-strategies/src/local_finetune.rs │
│ - Loads weights via candle (Rust-native, no PyTorch runtime    │
│   in deployment per docs/why-rust.md)                          │
│ - Implements RepairStrategy::propose_patch                     │
│ - Optionally: a vllm/sglang fallback for cases where Rust      │
│   inference doesn't match throughput needs                     │
└──────────────────────────┬────────────────────────────────────┘
                           │
                           ▼
┌───────────────────────────────────────────────────────────────┐
│ refineforge-eval acceptance gate                              │
│ - Run fine-tuned strategy against the eval split (held out)    │
│ - Compare repair-rate / latency / cost vs baseline.json        │
│ - GATE: fine-tuned ≥ baseline OR document the gap explicitly   │
│   and iterate (don't ship a regression)                        │
│ → out: shipped weights + the comparison report                 │
└───────────────────────────────────────────────────────────────┘
```

Per `docs/why-rust.md` reviewer FAQ: training happens in
PyTorch via axolotl; the fine-tuned model is consumed via
Rust-native candle (or burn) in deployment. The
training/deployment language asymmetry is acknowledged and
preserved.

## 3. Phase-by-phase

### Phase 0 — Pre-work (operator)

- Confirm Anthropic API key + sufficient budget for the
  teacher run (~$50-300 for the corpus generation; cost
  depends on N + base teacher model).
- Confirm `D:\cargo-target` continues to be the target dir
  (avoid re-pinning across phases).
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
- A 10-entry smoke corpus generates without quality-gate failures.

### Phase 2 — Mathlib mutation pipeline (operator side)

- Per `docs/repair-evaluation.md` §9. Multi-week elapsed.
- Output: ~N broken-Lean entries with ground truth + mutation
  metadata.
- Per Mathlib's license: ensure attribution is propagated
  into the corpus's `dataset_card.py` output.

**Time:** 3-6 weeks elapsed (multi-week work; mostly
operator throughput-bound, not blocking other phases).

**Acceptance:**
- N≥1000 entries, each with `(broken_lean, original_lean,
  mutation_kind)` triple verified by `lake build` (broken =
  fails; original = passes).

### Phase 3 — KF corpus generation (parallel with Phase 2)

- Use KF's existing `multi_teacher.py` to route the broken-
  Lean entries through the Lean probe set.
- Anthropic teacher answers each entry.
- Quality gates run; failures dropped.
- KF's `pack_writer.py` packages the output as a HuggingFace
  dataset.
- KF's HuggingFace publisher (`catalog/publishers/huggingface`)
  publishes the dataset (or keeps it local; operator choice).

**Cost:**
- Anthropic teacher calls: ~$0.07-0.15 per entry × N.
- For N=1000: **~$70-150 in teacher API fees.**
- For N=5000: **~$350-750.**

**Time:** 1-2 weeks elapsed (KF runs are fast; the bottleneck
is Anthropic rate limits + the operator reviewing the first
~100 outputs for quality).

**Acceptance:**
- Final corpus has N entries with patch JSON that:
  - Round-trips through `Patch::apply` (refineforge-repair-api).
  - Validates `start <= end` invariants.
  - Doesn't reference `sorry` / `admit` / non-core axioms.

### Phase 4 — refineforge-eval baseline establishment

- Split the corpus 80/20 train/eval (operator chooses random
  seed; documented in the eval report).
- Run claude-opus-4-7 against the eval split via the existing
  refineforge-eval harness.
- Record baseline metrics: repair-rate, p50/p95 latency, cost
  per attempt, per-mutation-kind breakdown.

**Cost:** ~$0.07 × 0.2 × N = $14 for N=1000; $70 for N=5000.

**Time:** 1-3 days elapsed (eval is mostly Lean LSP-bound;
the AnthropicStrategy work is fast).

**Acceptance:**
- `eval/runs/baseline-claude-opus-4-7.json` shipped.
- Numbers documented in `docs/repair-evaluation.md` §results.

### Phase 5 — axolotl fine-tune

- Use `refineforge-trainer` to orchestrate (the existing
  scaffold). Configuration: axolotl YAML pointing at the
  KF-produced corpus.
- Base model: start with Qwen2.5-Coder-1.5B for the smoke
  fine-tune (~$50-150 in spot GPU); if quality looks
  promising, scale to a 7B or 13B base for the production
  fine-tune.
- LoRA + QLoRA at r=64; FSDP for >7B if used.
- 16,000 GPU-hours budgeted (production fine-tune); smoke
  fine-tune uses ~50-200 hours.

**Cost:**
- Smoke fine-tune (1.5B): ~$50-150 on Lambda/CoreWeave spot.
- Production fine-tune (7-13B): ~$40k cloud, ~$5-10k cash
  outlay via Option A grants (NVIDIA Inception + AWS Activate
  + CoreWeave/Lambda startup credits — see
  `resourcing-plan.md` §4).

**Time:**
- Smoke fine-tune: ~1-3 days wall-clock.
- Production fine-tune: ~2-6 weeks wall-clock (subject to
  spot preemption; checkpoint resume already supported by
  `refineforge-trainer`).

**Acceptance:**
- Training loss converges or plateaus.
- Checkpoint manifest in `refineforge-trainer`'s `report.json`.
- Per-epoch eval pass (small sample from the eval split)
  shows monotone-or-flat improvement.

### Phase 6 — refineforge `--strategy local-finetune` wiring

New module: `crates/refineforge-strategies/src/local_finetune.rs`.

- Loads weights via `candle` (Rust-native; consistent with
  `docs/why-rust.md`). If candle doesn't support the chosen
  base architecture, fall back to `tch-rs` (PyTorch FFI; less
  ideal but unblocks).
- Implements `RepairStrategy::propose_patch`.
- New factory in `refineforge_strategies::local_finetune_from_path(weights_path)`.
- New `resolve_strategy` arm in `refineforge-cli/src/autonomous/executor.rs`.

**Time:** 1-2 weeks elapsed (mostly integration + matching
the AnthropicStrategy's prompt format from
`refineforge-strategies/src/anthropic.rs`).

**Acceptance:**
- `refine repair <claim> --strategy local-finetune
  --weights-path <path>` runs end-to-end against
  EXAMPLE-002.
- Token counts surface in `UsageStats` (the existing Phase
  3.7 reader; needs to be made strategy-agnostic).

### Phase 7 — Acceptance gate against baseline

- Run `refine-eval --corpus eval/corpus/full.jsonl --strategy
  local-finetune` against the held-out eval split.
- Compare to `baseline-claude-opus-4-7.json` from Phase 4.
- **Gate:** repair-rate of fine-tuned ≥ baseline. If not:
  - Document the gap honestly.
  - Iterate (more data, bigger base, different prompt format).
  - Don't ship a regression.

**Time:** 1-3 days elapsed.

**Acceptance:**
- `eval/runs/finetune-v1.json` shipped.
- The comparison either shows ≥-parity OR the iteration
  decision is documented.

### Phase 8 — Documentation + criteria v0.4 review

- Update `docs/llm-repair-design.md` to document the new
  `local-finetune` strategy.
- Update `docs/repair-evaluation.md` results section.
- **Criteria v0.4 review**: does switching from `anthropic`
  to `local-finetune` count as a Cat 8 (trust-base)
  escalation? Yes — the weights are now part of the trust
  base, and operator should sign off explicitly. Document
  the answer.
- New refineforge release (`v0.3.0`?) if the fine-tune
  ships.

**Time:** 3-5 days.

## 4. Total honest estimate

| Phase | Days (active) | Calendar | Cost (Option A path) |
|---|---:|---:|---:|
| 0 — Pre-work | 1 | 1 day | $0 |
| 1 — KF probe set | 3-5 | 1 week | $0 |
| 2 — Mathlib mutation | (multi-week elapsed) | 3-6 weeks | $0-200 cloud burst |
| 3 — KF corpus generation | 5-10 | 1-2 weeks | $70-750 Anthropic |
| 4 — Baseline establishment | 1-3 | 1-3 days | $14-70 Anthropic |
| 5 — Smoke + production fine-tune | 7-14 | 2-6 weeks | $5-10k cash via grants |
| 6 — `--strategy local-finetune` wiring | 7-14 | 1-2 weeks | $0 |
| 7 — Acceptance gate | 1-3 | 1-3 days | trivial |
| 8 — Docs + release | 3-5 | 3-5 days | $0 |
| **Total active engineering** | **~30-60 days** | — | — |
| **Total elapsed** | — | **~8-12 weeks** | **~$5-11k cash** |

= **~2-3 calendar months** with one focused operator + one
part-time maintainer (per `resourcing-plan.md` v0.2).
Bottleneck is Phase 2 (Mathlib mutation throughput) +
Phase 5 (training wall-clock), neither of which active
engineering can compress.

Cash burn: **~$5-11k** total via Option A grant stack +
Anthropic teacher fees. Two orders of magnitude smaller than
the original 16,000-GPU-hour-via-on-demand line.

## 5. Resource requirements

Inherits from `resourcing-plan.md` v0.2:
- **1 human operator** (you).
- **1 part-time refineforge maintainer** (you, or ~0.3 FTE
  contractor).
- **0 additional hires** for this work. KF + refineforge +
  axolotl + Anthropic teacher are all already in the stack.
- **Compute:** Option A grants ($5-10k cash outlay against
  $40-45k cloud-equivalent).
- **Anthropic API:** ~$100-1000 over the lifetime of the
  fine-tune (corpus generation + baseline eval).

## 6. Risks

| Risk | Severity | Mitigation |
|---|---|---|
| Mathlib mutation throughput too slow → Phase 2 takes 3+ months | **HIGH** schedule | Smoke fine-tune in Phase 5 uses synthetic mutations from existing eval corpus; don't gate phases 5-7 on a fully-sized Phase 2 |
| KF Anthropic-teacher refusals on Lean (mathematical content occasionally trips safety filters) | **MEDIUM** | The existing `no_refusal_responses.py` gate filters; backup teacher (Gemini / GPT-4o) added as fallback |
| Fine-tuned model underperforms baseline | **MEDIUM** | Phase 7 acceptance gate explicitly says "don't ship a regression"; iterate on data + base model + prompt format |
| candle doesn't support the chosen base architecture | **MEDIUM** | tch-rs fallback exists; documented as the less-ideal path; doesn't block ship |
| Grant credits expire before Phase 5 starts | **MEDIUM** | Phase 1+3 can run on cash (~$100-1000); only Phase 5 needs the GPU credits; time the grant onboarding accordingly |
| Anthropic API key compromised mid-corpus-generation | **HIGH** | Rotate key + restrict by IP; KF's billing module tracks spend in real-time; cost-cap configured |
| KF's HF publisher publishes dataset that contains unintended Lean code | **HIGH** if happens | KF's existing `pii_leakage.py` gate; Lean code is not PII, but for HELYX-internal use the `allow_pii_by_design: false` preset keeps the dataset internal-only |
| Production fine-tune produces model that hallucinates `sorry`/`admit` | **MEDIUM** | The existing no-sorry policy gate catches at apply-time; LoRA training penalty on those tokens further suppresses |

## 7. Definition of done

1. ✅ KF probe set (`lean_proof_repair`) lands in
   Knowledge-Foundry with passing tests.
2. ✅ Mathlib mutation pipeline produces ≥1000 entries with
   verified-broken Lean.
3. ✅ KF corpus is packaged as a HuggingFace dataset (local
   OR published).
4. ✅ Baseline `claude-opus-4-7` numbers documented in
   `docs/repair-evaluation.md`.
5. ✅ Fine-tuned model checkpoint(s) shipped.
6. ✅ `refine repair <claim> --strategy local-finetune` runs
   end-to-end.
7. ✅ Acceptance gate: fine-tuned ≥ baseline OR documented
   gap + iteration plan.
8. ✅ Criteria v0.4 reflects the new trust-base entry (model
   weights).
9. ✅ Release `v0.3.0` tag if the fine-tune ships.

## 8. Out of scope (explicitly)

- **70B+ base models.** Out of budget for the first
  fine-tune. v0.3.x can revisit.
- **DPO / RLHF on the fine-tuned model.** Phase 2 of model
  training. Separate plan.
- **Multi-language proof systems.** Coq + Isabelle + Agda
  reuse refineforge's API but each needs its own probe set +
  corpus. Lean 4 only for now.
- **Real-time online learning.** The fine-tune is a
  point-in-time artifact. Continual learning is a separate
  trust-base concern.
- **Model serving as a SaaS API.** refineforge consumes the
  weights locally. Hosting a public inference endpoint is a
  separate (and out-of-scope) project.
- **Federated learning across multiple HELYX operators.**
  Not relevant; HELYX is one-operator today.

## 9. Open questions for the operator

1. **N (corpus size).** 1000 / 2500 / 5000? Affects Phase 2
   wall-clock + Phase 3 cost.
2. **Base model.** Qwen2.5-Coder-1.5B (smoke) → 7B or 13B
   (production)? 13B fits the 16,000-GPU-hr budget
   comfortably; 7B leaves more room for re-runs.
3. **Inference runtime.** candle (Rust-native, smaller
   ecosystem) vs tch-rs (PyTorch FFI, fuller ecosystem)?
   `why-rust.md` doctrine favors candle.
4. **HuggingFace publishing.** Publish the corpus + the
   fine-tuned weights publicly, OR keep internal? Public =
   stronger community traction; internal = no Mathlib
   attribution scrutiny on the corpus.
5. **Teacher diversity.** Anthropic-only OR multi-teacher
   (Anthropic + Gemini + GPT-4o)? Diversity reduces single-
   vendor bias but raises cost ~2-3×.
6. **CoT-trace mode.** Use sft_pair only, OR add cot_trace
   (chain-of-thought reasoning traces)? CoT-trace is more
   work but produces a model that explains its patches.
7. **Operator availability for Phase 5 monitoring.** The
   production fine-tune is 2-6 weeks of wall-clock; operator
   needs to be reachable for spot-preemption recovery +
   checkpoint promotion decisions.

## 10. Appendix — Probe-set spec (files to copy into Knowledge-Foundry)

When Phase 1 starts, the operator copies these files into
`D:\AI-PROJECTS-GALO\PROJECTS\Knowledge-Foundry\`:

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
      new_text, rationale (each 0-indexed; LSP convention).

    Generate ONE training pair where:
    - prompt = realistic Lean 4 diagnostic message +
      surrounding source file
    - response = the LSP-shaped patch JSON that the model
      should learn to emit

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

### 10.4 — Test stubs for KF's pytest

`kb_destiller/modes/sft_pair/tests/test_lean_proof_repair_probe.py`:

- Loads the probe set; asserts both probe ids parse.
- Asserts the preset references both probe ids.
- Asserts `patch_well_formed` accepts a known-good entry +
  rejects each of the failure modes (non-JSON, missing keys,
  negative ints, end-before-start, non-string new_text).

(Standard KF pytest patterns; ~50-line test file.)

### 10.5 — Operator's first run after copying

```bash
# In Knowledge-Foundry repo
python -m pytest kb_destiller/modes/sft_pair/tests/test_lean_proof_repair_probe.py
python -m kb_destiller.cli run \
    --preset lean_proof_repair \
    --teacher anthropic \
    --target-n 10 \
    --out fixtures/smoke_corpus.jsonl
# Inspect fixtures/smoke_corpus.jsonl
# If quality looks reasonable, scale up:
python -m kb_destiller.cli run \
    --preset lean_proof_repair \
    --teacher anthropic \
    --target-n 1000 \
    --out corpora/lean_proof_repair_v1.jsonl
```

## 11. What this plan does NOT cover

- **Real fine-tune execution.** The plan stops at "ready to
  execute Phase 5." Actually running the production fine-tune
  is a separate operator commitment + grant-onboarding sequence.
- **Replacement of `AnthropicStrategy`.** The fine-tuned
  strategy is ADDITIVE. AnthropicStrategy stays as a baseline
  + a fallback. `refine autonomous --strategy local-finetune`
  is a new opt-in path, not a deprecation.
- **Continual updates as Mathlib evolves.** The corpus is a
  point-in-time snapshot; a periodic refresh (e.g. annually)
  is a separate operator concern.
- **HELYX-specific proof patterns.** The fine-tune is on
  Mathlib mutations, which is general Lean. HELYX-specific
  patterns (audit chain, capability, constitution) would need
  their own probe set + corpus; v0.3.x can revisit.

The plan does NOT commit to any of the numbers — they're
**honest estimates** for the operator to sharpen against
quotes from grant officers + cloud sales reps before the
first dollar gets spent.
