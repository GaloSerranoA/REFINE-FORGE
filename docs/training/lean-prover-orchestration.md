# Lean Prover Orchestration (`refineforge_lean_prover`)

**Inherit a trained open prover; own the verifier, the trust, and the
orchestration.** This is the inference-stage complement to the from-scratch GPU
training path (`docs/training/native-gpt-gpu-phase.md`): instead of *training* a
Lean prover, we *download* one and wrap it in Refine-Forge's verifier + trust +
orchestration.

Status: **as-built and validated offline** (M19). The orchestration engine
(`refineforge-prover`) and the trainer backend (`refineforge_lean_prover`) are
implemented, unit- and integration-tested, and exercised end-to-end through the
real `refine-train run → report.json` path with a replay prover + dry-run
verifier. A *live* run additionally requires the operator to download a prover,
serve it, and provide a Lean/Mathlib project (see the runbook).

---

## The core insight

The expensive part of a prover — pretraining + reinforcement learning at cluster
scale (64 GPUs × weeks, ~$50K–500K) — **was already done by the model authors and
released as open weights.** So Stage 4 of the honest staged plan collapses to
≈$0: we download the *result* of that training, not run it.

| Open Lean-4 prover | Sizes | MiniF2F | License | Consumer-runnable? |
|---|---|---|---|---|
| [DeepSeek-Prover-V2](https://github.com/deepseek-ai/DeepSeek-Prover-V2) | **7B** + 671B MoE | 88.9% (671B) | MIT (code) + DeepSeek model license | **7B yes**; 671B cluster-only |
| [Goedel-Prover-SFT](https://huggingface.co/Goedel-LM/Goedel-Prover-SFT) | 7B | 57.6% (Pass@32) | open | **yes** (built on expert iteration + verifier-guided self-correction) |
| [Kimina-Prover](https://huggingface.co/AI-MO/Kimina-Prover-72B) | **1.5B + 7B** + 72B | 80.7% (72B) | open | **7B/1.5B yes**; RL pipeline ([`kimina-prover-rl`](https://github.com/project-numina/kimina-prover-rl)) also open |

The **7B** variants are the consumer-hardware sweet spot. What stays *ours* — and
is the actual value of Refine-Forge — is everything around the model:

- the **Lean checker** as the reward and the trust boundary (no proof is
  "accepted" unless `lake env lean` exits clean);
- best-of-k **proof search**;
- the **trust evidence** (`proof_pass_rate` → `report.json` → eval / regression /
  approval ladder);
- the **orchestration** that schedules, retries, and audits all of it.

---

## Why this is GPU-agnostic (and how the P40 + 5080 fit)

The backend **never touches CUDA.** It talks to a prover *inference server*
(vLLM / llama.cpp) over an OpenAI-compatible HTTP endpoint; the server owns the
GPU. So a heterogeneous workstation is the *server's* concern, not ours:

```
  problems.jsonl ─▶ refineforge_lean_prover ──HTTP──▶ prover server ─▶ GPU(s)
                          │  (Rust, no CUDA)            (vLLM / llama.cpp)
                          ▼
                    Lean checker  (lake env lean)  ◀── the trust gate
                          │
                          ▼
        progress.jsonl (proof_pass_rate) ─▶ report.json ─▶ trust ladder
```

**Recommended split for a P40 (24 GB, Pascal sm_61, weak fp16) + RTX 5080 (16 GB,
Blackwell, fast fp16):**

- Run the **7B prover on the 5080** — a 7B in fp16 (~14 GB) fits 16 GB, and
  Blackwell gives real fp16/bf16/fp8 throughput. `CUDA_VISIBLE_DEVICES=0 vllm
  serve …`.
- Use the **P40 as a second worker** (a second `vllm serve` for more best-of-k
  throughput) or to host a **larger int8/4-bit model** via llama.cpp (its 24 GB is
  the win; its weak fp16 is not). Point a second config at its endpoint.
- A *single* model tensor-paralleled across both is possible but awkward (arch
  mismatch + the P40 bottlenecks fp16) — prefer two independent workers.
- Swapping in any future single big card changes nothing in this backend; the
  hand-written CUDA *training* path separately auto-detects it (M18).

---

## Components

### `refineforge-prover` (the engine)

A standalone, CUDA-free crate:

- `ProverClient` — generate up to *k* candidates. Impls: `OpenAiProver`
  (vLLM/llama.cpp, completion or chat API), `ReplayProver` (replay a JSONL of
  pre-generated candidates — offline / dry-run / re-verification).
- `Verifier` — the trust gate. Impls: `CommandVerifier` (`lake env lean
  <candidate>`, exit 0 ⇒ accepted — **the only trust-bearing verifier**) and
  `DryRunVerifier` (a *labeled* substring stand-in for plumbing tests; grants no
  trust).
- `ProofSearch` — best-of-k: generate, verify in order, keep the first accepted
  proof; emit `progress.jsonl` + `proof-search-report.json`.

15 unit tests cover best-of-k stopping, the sample cap, evidence emission,
determinism, template assembly, the OpenAI request/response shapes, replay
loading, and a real subprocess verifier round-trip.

### `refineforge_lean_prover` (the trainer backend)

A thin adapter (`crates/refineforge-trainer/src/lean_prover.rs`) that reads the
prover/verifier/problem config from an experiment, runs `ProofSearch`, and emits
the **standard trust evidence** — `progress.jsonl` with the honest
`proof_pass_rate` metric, so `report.json`'s metric summary (and the downstream
eval/regression/approval ladder) consumes it with **no metric warping**.

Config (`hyperparameters`): `samples` (k); prover source `prover_replay_file`
*or* `prover_base_url`+`prover_model` (+`prover_api`, `max_tokens`); verifier
`lean` (`lean_dir`, `lean_command`) *or* `dry_run` (`verifier_substring`).
Problems come from `dataset.path` (JSONL of `{id, statement, split?, template?}`)
unless `problems_file` overrides.

---

## Honest staging

- **Stage 0 — inference proof search (this, done).** Download a 7B prover, search
  proofs gated by Lean, collect verified proofs + `proof_pass_rate`. No training.
- **Stage 1 — cheap adaptation (optional, future).** Mine the verified proofs as
  SFT data and LoRA/QLoRA-adapt the 7B to *our* exact task / Lean version on a
  single 24 GB card (reusing Goedel's expert-iteration recipe or Kimina's open RL
  pipeline). Still ≈ commodity compute.
- **Not attempted — paper-scale RL from scratch** (the 671B/72B frontier). That is
  exactly what we *avoid* by downloading.

## What's verified vs. operator-provided

| Verified here (offline) | Operator-provided (for a live run) |
|---|---|
| Orchestration engine (15 unit tests) | A downloaded prover (multi-GB; your bandwidth/disk/license) |
| Trainer dispatch + evidence (`run → report.json`, 2 integration tests) | A served endpoint (vLLM/llama.cpp on your GPU) |
| `proof_pass_rate` flows through the trust ladder honestly | A lake/Mathlib project matching the prover's toolchain |
| Committed smoke (3/4 solved, dry-run) | The real Lean checker + a real problem set |

A downloaded prover targets a *specific* Lean/Mathlib version and competition-style
theorems; expect a toolchain-match/adaptation gap on our proof-repair task. 7B
pass rates are strong on easy benchmarks (~57–80%) and far lower on hard/research
theorems — a good prover, not a universal one.

---

## Runbook

**Offline smoke (no GPU / prover / Lean):**
```
cargo run -p refineforge-trainer -- run training/configs/refineforge-lean-prover-smoke.yaml
```
→ 3/4 "solved" (dry-run), `report.json` with `proof_pass_rate` last=0.75.

**Live run:**
1. Download + serve a prover (pin to the fast card):
   `CUDA_VISIBLE_DEVICES=0 vllm serve deepseek-ai/DeepSeek-Prover-V2-7B --port 8000`
2. Provide a lake/Mathlib project at `lean_dir` whose toolchain matches the prover.
3. `cargo run -p refineforge-trainer -- run training/configs/refineforge-lean-prover-live.yaml`
4. Take the evidence through the ladder: `refine-train evidence runs/… --out-dir …`.

Or drive the engine directly (no trainer):
`cargo run -p refineforge-prover --example lean_proof_search -- --problems … --base-url … --model … --lean-dir … --samples 8`.
