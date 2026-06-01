# Native GPT — GPU Phase (Phase 3) Design

## Context

The from-scratch `refineforge_native_gpt` backend
(`crates/refineforge-trainer/src/native_gpt/`) is a real decoder-only
transformer with hand-written, finite-difference-gradient-checked
backpropagation. It is **CPU / f64 / deterministic** and reaches
`human-reviewed` on a smoke checkpoint. CPU `f64` caps both model size and speed,
so smoke runs overfit tiny data. This phase adds a **hand-written CUDA path** so
the same architecture can train meaningfully larger models on the local
**RTX 3060 (compute 8.6, CUDA 13.2)** — no PyTorch, no `candle`/`burn`, staying
"from scratch" and evidence-first.

## Why this is feasible now

- `nvcc 13.2` + MSVC `cl.exe` + the RTX 3060 are confirmed working: the Kernel
  agent already compiles and runs a CUDA kernel **bit-exactly** on this GPU
  (`kernels/src/hvector_add.cu`, reproduced 2026-05-31). The build/run/evidence
  infra in `refineforge-bitexact` is reusable.
- The CPU implementation is the **correctness oracle**: every GPU kernel can be
  parity-checked against the already-verified CPU layer.

## Goals / non-goals

- **Goal:** a GPU-accelerated training path for the GPT, hand-written CUDA
  kernels launched from Rust, reusing the bit-exact build/evidence infra.
- **Non-goal:** building a DL framework, or claiming frontier scale. 6 GB VRAM
  bounds model + batch size to "small real transformer", not a production LLM.

## Key honesty constraint: precision & determinism

Consumer GPUs are ~30–60× slower in `f64`, so the GPU path is **`f32`**. GPU
reductions (matmul, softmax, layernorm) are **not bit-exact across runs** because
of nondeterministic parallel float accumulation. Therefore:

- The **CPU `f64` path remains the deterministic, bit-exact reference** (its
  `weights_sha256` stays reproducible).
- The GPU path's evidence must **explicitly record** `precision: f32` and
  `reproducibility: statistical-not-bit-exact`. We assert CPU/GPU **parity within
  an f32 tolerance**, never bit-exactness, and never silently relabel a
  noisy-but-close GPU run as deterministic.

## Architecture

- New module `crates/refineforge-trainer/src/native_gpt/gpu/` (or crate
  `refineforge-gpu`) holding:
  - `DeviceMat` — device buffer mirroring the CPU `Mat`, via **`cudarc`**
    (driver-API bindings; loads PTX, manages device memory, launches kernels)
    OR raw `nvcc`→PTX + the driver API.
  - Hand-written `.cu` kernels (compiled to PTX, reusing the bit-exact build
    wrapper): `matmul` (+ transposed variants for backward), elementwise
    (`add`, `gelu`, `gelu_grad`), row-wise causal `softmax`, `layernorm`
    (fwd/bwd), scaled-dot-product causal attention (fwd/bwd), `adamw_update`.
- The model code in `native_gpt/mod.rs` is **device-agnostic**: the same
  forward/backward structure dispatches tensor ops to CPU or GPU. Selected via a
  `device: cuda` hyperparameter (or a `refineforge_native_gpt_cuda` backend kind).

## Correctness strategy (the gate, mirroring the CPU gradient checks)

1. **Per-kernel parity tests:** each CUDA kernel output matches its CPU
   reference within f32 tolerance on seeded random inputs. This is the GPU analog
   of the CPU finite-difference gradient checks — a kernel that disagrees with the
   verified CPU layer is rejected.
2. **End-to-end parity:** train an identical tiny config on CPU and GPU; assert
   loss curves track within tolerance and final metrics agree within noise.
3. **Bit-exact build evidence:** wrap the kernel build/run in
   `refineforge-bitexact` to capture hardware-matrix / compiler-metadata evidence
   (as the hvector_add kernel already does), feeding the Kernel/Train trust lanes.

## Milestones (each independently shippable)

1. **Toolchain + scaffold:** add `cudarc`, a "hello GPU" vector-add matching the
   existing bit-exact kernel, parity test. De-risks the Windows/CUDA/MSVC build.
2. **Core kernels:** matmul (+ transposed), elementwise, softmax, layernorm —
   each with a CPU-parity test.
3. **Attention kernels:** causal scaled-dot-product attention fwd/bwd.
4. **Full GPU train step:** GPU forward + backward + AdamW; CPU/GPU end-to-end
   parity on a tiny config.
5. **Scale up:** train a larger GPT on the 3060 (e.g. `n_embed` 256–512, more
   layers/steps, real Mathlib data) → genuinely better eval → trust ladder.
6. **Evidence + docs:** GPU compute ledger (device, kernel PTX hashes, hardware
   matrix), with the explicit `f32` / non-bit-exact reproducibility notes.

## Risks

- **6 GB VRAM** caps model + batch size — be explicit about achievable scale.
- **Hand-written CUDA backprop is error-prone** — parity tests are the gate; do
  not advance a milestone until its kernels pass parity.
- **f32 non-determinism** — never claim bit-exactness for the GPU path; keep the
  CPU path as the reference.
- Multi-session effort; ship milestone by milestone behind parity tests.

## Files (when implemented)

- `crates/refineforge-trainer/src/native_gpt/gpu/` (DeviceMat, kernel launchers,
  parity tests) — or a dedicated `crates/refineforge-gpu/` crate.
- `.cu` kernels under `kernels/src/gpt/`, built via the bit-exact wrapper.
- Backend dispatch in `runner.rs` / `mod.rs` for `device: cuda`.
- Docs: update `docs/training/train-llm-from-scratch-analysis.md` status.

## Implementation status (as built, 2026-05-31)

Realized as a dedicated crate **`crates/refineforge-gpu/`** (not the in-trainer
module sketched above). CUDA lives behind an optional **`cuda`** feature
(`cudarc 0.19`, runtime NVRTC); the **default build is CPU-only with no CUDA
dependency**, so the workspace + `nix flake check` stay green on CI without a GPU.
Every CUDA kernel keeps a CPU `f32` reference in the same crate as its parity
oracle. All GPU tests are `#[cfg(feature = "cuda")]` (hardware-gated, never
disabled) and pass on the local **RTX 3060** — **29/29** as of this writing.

As-built milestones (each parity-gated and shipped behind green CI):

- **M1 — toolchain + vector-add.** `cudarc` + NVRTC building/running on the 3060;
  vector-add parity vs CPU. De-risked the Windows/CUDA/MSVC path (needed CUDA 13.2
  → `cudarc 0.19` with `cuda-version-from-build-system`).
- **M2 — matmul.** `matmul_nn` / `matmul_nt` (Linear fwd) / `matmul_tn` (Linear
  dW), all parity-checked. Measured ~**28×** over the scalar CPU reference
  (512×1024×1024: GPU 2.42 ms vs CPU 68.68 ms, release).
- **M3 — elementwise + reductions.** `gelu`(+grad), row-wise `softmax`(+grad),
  `layernorm` fwd/bwd, `adamw_update` — each parity-checked.
- **M4 — device-resident `Linear` + AdamW step.** Tensors stay on the GPU across
  forward → backward → optimizer (the pattern that actually accelerates training),
  parity-checked end to end.
- **M5 — full transformer `Block`.** Device-resident `LayerNorm`, `Mlp`, multi-head
  causal `Attention`, and a pre-norm residual `Block`. Attention backward is
  verified by a **finite-difference gradient check**; the block trains a synthetic
  target end to end.
- **M6 — full GPU GPT + real-data training loop.** `GptModel` = trainable token +
  position embeddings → N `Block`s → final LayerNorm → LM head → softmax
  cross-entropy, with forward / backward / AdamW **all device-resident**. New
  parity-checked kernels: `embedding_forward` / `embedding_backward` (atomicAdd
  token grads; position grads sized to the full context table) and
  `softmax_cross_entropy`. Verified by embedding/cross-entropy parity tests, a
  synthetic end-to-end learning test, and the **`train_mathlib`** example.

**Real-data result (`cargo run -p refineforge-gpu --features cuda --release
--example train_mathlib -- production-proof/native-gpt-mathlib/sft-pack 12`):** a
smoke-scale GPT (`embed=128, heads=4, layers=4, hidden=512`) on the 32-record
Mathlib SFT pack (vocab ≈ 800, ctx = 128) drops **train loss 6.36 → 3.26** and
reaches **44.9 % train / 13.3 % held-out** next-token accuracy in **288 steps /
3.2 s** (~11 ms/step, ~7.4k target-tokens/s) — the entire forward + cross-entropy
+ backward + AdamW path on the GPU. Held-out 13.3 % is well above chance
(1/797 ≈ 0.13 %) and above the linear-smoke baseline (~5.5 %); it is honestly
**smoke-grade** (24 train sequences), not a production LLM.

Honesty constraints from this design are upheld: the GPU path is **`f32`** and
**parity-checked within tolerance, never claimed bit-exact**; the CPU `f64` path
remains the deterministic reference.

### Phase 2 — scale-up (M7–M8, 2026-05-31)

- **M7 — packed mini-batch path (segmented attention).** Several sequences in one
  buffer: segmented causal attention (a token attends only within its own
  sequence) + per-sequence-resetting position embeddings. New parity-checked
  kernels (`embedding_forward_packed`/`embedding_backward_packed`,
  `scale_segmented_causal_mask`(+grad)); `GptModel::{forward,backward,train_step,
  evaluate}_packed`. The gate (`gpu_packed_forward_matches_sequential`) asserts
  packed per-token logits equal the per-sequence forwards within 3e-3.
  **Honest throughput finding:** this is *not* a speed win for short-sequence
  batches — measured **0.95×** vs sequential, because segmented attention is
  O(packed_length²) (B short sequences ⇒ ~B× wasted attention). Its value is
  correct mini-batch gradient semantics; a **block-diagonal / flash-attention
  kernel** (O(Σ Lᵢ²)) is the throughput follow-up.

- **M8 — scale on the full Mathlib data.** A bigger GPT (`embed=256, heads=8,
  layers=4, hidden=1024, ctx=256`) trained on the **full 800-train / 100-held-out**
  SFT pack (vocab 7505, 143k tokens) via the `train_scale` example (lr warmup +
  cosine decay, per-epoch held-out eval). **Result (RTX 3060, 16k steps / ~10 min,
  37 ms/step):** held-out next-token accuracy peaks at **25.6 % (epoch 6)**, best
  held-out loss **4.74 (epoch 4)** — vs the **CPU `native-gpt` scale baseline of
  5.6 % / 6.40 after 40 steps** (the CPU `f64` path stalled at ~5 s/step and never
  reached convergence). That is a **~4.6× held-out accuracy gain**, and the
  decisive point of the GPU phase: the CPU was *compute-bound*; the GPU runs the
  thousands of steps it could not.

  **Honest caveat:** the model **overfits** past ~epoch 6 (train accuracy → 99.5 %
  by epoch 20 while held-out drifts to 22 % and held-out loss rises to 7.8). The
  bottleneck has moved from compute to **data / regularization** — and AdamW weight
  decay was `0.0` here. Early stopping, regularization (next), more data, and the
  GPU compute ledger are the follow-ups.

- **M9 — regularization (weight decay + label smoothing).** AdamW weight decay is
  now threaded through the matmul weights only (attention / MLP / LM head — *not*
  LayerNorm, biases, or embeddings, per standard practice; `set_weight_decay`), and
  the cross-entropy kernel gained **label smoothing** (`set_label_smoothing`,
  parity-checked vs CPU at ε = 0 and ε = 0.1). `train_scale` exposes both as args
  (defaults wd = 0.1, ls = 0.1). **Honest measured effect** (same model/data as M8,
  20 epochs):

  | config | best held-out acc | best held-out loss | peak epoch |
  |---|---|---|---|
  | baseline (wd 0, ls 0) | 25.6 % (e6) | 4.74 (e4) | 6 |
  | wd 0.01 | ~24.9 % | ~4.75 | 6 |
  | **wd 0.1 + ls 0.1** | 25.3 % (e10) | **4.55** (e4) | **10** |

  Regularization **measurably improves held-out loss / calibration (4.74 → 4.55,
  −4 %) and delays the overfitting peak (epoch 6 → 10)**, but it does **not** lift
  peak held-out **accuracy** (~25 % regardless). The accuracy ceiling is
  **data-bound**: 800 records cannot support more generalization, and the model
  still memorizes train to 99 %. Honest takeaway — past ~25 %, the lever is **more
  data**, not more regularization. The regularizers are correct and reusable
  (parity-gated); they buy calibration and a wider early-stopping window, not a new
  accuracy regime.

- **M10 — GPU compute ledger / evidence pack.** The originally-planned final GPU
  milestone. The `gpu_ledger` example emits a self-contained JSON evidence pack
  (`docs/training/gpu-compute-ledger.json`, schema
  `refineforge-gpu-compute-ledger-v1`): the live **device** (name + compute
  capability, queried via the new `GpuKernels::compute_capability`), the
  **kernel-source SHA-256** + kernel count, the **24-gate parity roster** (the
  CPU-oracle checks that verify every kernel; 32 cuda tests total), the **held-out
  eval** (CPU 5.6 % vs GPU best 25.3 % / peak 25.6 %, ~4.6×), the **milestone map
  M1–M10**, and the explicit **f32 / statistical-not-bit-exact reproducibility
  declaration** (CPU f64 stays the deterministic reference). This closes the GPU
  phase with the evidence artifact the project's trust model expects.

  Open follow-ups (beyond this phase): dropout, a block-diagonal / flash-attention
  kernel for real packed throughput, **more training data** (the accuracy lever),
  and wiring the GPU backend into the trainer's evidence / trust-ladder pipeline.

### Phase 3 — follow-ups

- **M11 — dropout (residual).** A deterministic counter-based dropout kernel
  (forward → `y` + keep-mask, backward `dx = dy ⊙ mask`), parity-tested (drop rate
  ≈ p, inverted-dropout mean ≈ 1, scale, determinism, backward). Residual dropout
  is wired into every `Block` (attention + MLP outputs, masks cached for backward;
  per-block + per-sublayer seeds) via `GptModel::set_dropout`. It is **off by
  default and always off at eval** (zero seed), so the prior 33 cuda tests and the
  packed-forward==sequential parity gate are unchanged; a new end-to-end test
  confirms the model trains with dropout active (34 cuda tests).

  **Honest measured verdict** (wd 0.1 + ls 0.1, ± dropout 0.1):

  | config | best held-out acc | best held-out loss | train acc @20 | ms/step |
  |---|---|---|---|---|
  | no dropout | 25.3 % (e10) | **4.55** | 99.1 % | 37 |
  | dropout 0.1 | 25.3 % (e9) | 4.58 | 96.6 % | 42.6 |

  Dropout **does not help**: identical peak accuracy, slightly worse best loss,
  +15 % compute. It *does* regularize (train memorization 99.1 % → 96.6 %), but
  that does not reach held-out — so **three independent regularizers (weight decay,
  label smoothing, dropout) now all confirm the ~25 % held-out ceiling is
  data-bound, not regularization-bound.** The dropout feature is correct and
  reusable (would help on larger/different data) but is defaulted off in
  `train_scale`. The remaining real accuracy lever is **more data**; the repo's
  Mathlib SFT set caps at ~1000 examples, so a genuine break past ~25 % needs more
  teacher-distilled data (out of scope here).

- **M12 — greedy generation.** `GptModel::generate` (forward → argmax of the last
  position → append, sliding a context window; deterministic, dropout-off). A cuda
  test trains a model to memorize a period-5 sequence and asserts greedy generation
  reproduces it (and is deterministic across calls); `train_scale` ends with a
  held-out-prompt generation demo (35 cuda tests). This is the natural capstone — a
  trained GPU GPT that actually *generates* — and the building block for the
  `generation-smoke` trust-evidence artifact. *(Block-diagonal attention, the
  original M12 candidate, was deprioritized: it optimizes the currently-unused
  packed path and, since accuracy is data-bound, would not improve eval.)*

- **M13 — GPU trust-ladder evidence (built, report-verified).** Makes the
  GPU-trained model emit the standard trust-ladder evidence so the trainer's tooling
  consumes it — via the chosen **GPU-emits-evidence** direction (no trainer-crate
  surgery). New GPU model API: `parameter_count`, `weight_values` (a deterministic
  `collect_weights` walk over every trainable tensor), and `generate` (M12), all
  cuda-tested (36 cuda tests). The `gpu_evidence` example trains the GPU GPT on a
  pack and writes a complete run directory — `config.yaml`, `progress.jsonl`
  (`ProgressRecord` per epoch), `checkpoints/step-N/gpt-checkpoint.json` (with a
  `weights_sha256` from `sha2(weight_values)`), `train-metadata.json`,
  `generation-smoke.json` — matching the schemas the trainer reads.

  **Verified end to end:** `refineforge-trainer report <gpu-run-dir>` consumes the
  GPU evidence and builds a valid `report.json` (all 4 metrics summarized, the
  checkpoint listed). The `backend.kind` stays the valid `refineforge_native_gpt`
  (the architecture *is* the native GPT); the GPU `f32` compute is recorded honestly
  in the `architecture` block (precision / device / compute) — `weights_sha256` is a
  per-run **integrity** hash, not cross-run-reproducible (the CPU `f64` path remains
  the deterministic reference), consistent with the f32 / non-bit-exact stance.

  **Honest boundary:** the downstream `evidence` → eval-report / regression /
  promotion manifests gate on `report.final_outcome == "success"`, which is set by
  the **live training pipeline** (`refine agent train` via `runner.rs`), not by the
  standalone report-rebuild CLI (which labels it `"rebuilt"`). Reaching that final
  approval gate legitimately is the **trainer-dispatch** integration (M14, below).
  The evidence *format* is verified-compatible; the run-*status* gate is not bypassed
  (hand-editing the outcome to force it would be evidence tampering).

- **M14 — trainer-dispatch integration (live pipeline; full approval chain).** Makes
  `refineforge_native_gpt_cuda` a first-class backend: `refineforge-trainer` gains a
  cuda-gated dependency on `refineforge-gpu` (a `cuda` feature → `refineforge-gpu/
  cuda`; the **default build stays CPU-only**, so CI is unaffected), the valid-kind
  list (`experiment.rs`) accepts it, `runner.rs` dispatches it (and errors helpfully
  if built without `--features cuda`), and a cuda-gated submodule
  `native_gpt::cuda` trains the device-resident GPU `GptModel` while **reusing the
  parent module's pack loading / config / split** and emitting the identical evidence
  contract. New config `training/configs/refineforge-native-gpt-cuda-smoke.yaml`.

  **Verified end to end on the RTX 3060** — the full trust ladder now runs on the
  GPU backend:
  - `refine-train run …cuda-smoke.yaml` → **`final: success`** report with real GPU
    metrics (dev target-token accuracy ≈ 22 % after 4 epochs), no failures.
  - `refine-train evidence <run> …` → **`eval=passed regression=passed
    promotion=…`**, emitting every production-proof manifest (`eval-report.json`,
    `regression-report.json`, `promotion-manifest.json`, `compute-ledger.json`,
    `conversion-manifest.json`, `checkpoint.safetensors`).

  The GPU-trained native GPT is now a real **trust-bearing** backend: it produces a
  `success` report and **approval-ready** evidence through the same pipeline as the
  CPU backend — closing the gate M13 honestly stopped at. Default builds remain
  CPU-only (176/176 trainer tests unchanged); the GPU path is opt-in via
  `--features cuda`. Trust is still bounded by the same eval/regression gates and a
  human `refine approval` — this wires the *evidence*, it does not auto-approve.

### Research applications (Train agent)

- **M15 — KL self-distillation (anti-forgetting).** Applies the fix from *"Why
  Fine-Tuning Encourages Hallucinations and How to Fix It"* (SFT on new content
  causes factual forgetting via representational interference; a KL regularizer
  toward a frozen self-snapshot largely prevents it) to the native-GPT SFT path.
  New `kl_distill` kernel + `dev_kl_distill` (gradient `scale·τ·(p_s − p_t)`, loss
  `τ²·KL(p_t ‖ p_s)`, parity-checked vs CPU); `GptModel::train_step_distill`
  (`L = CE + λ·KL`); `load_weights`/`collect_weights_mut` for a frozen teacher
  snapshot (round-trip tested). The cuda backend warms up, snapshots a frozen
  teacher, then distills the remaining steps — opt-in via `self_distill_lambda`
  (default 0 → off, so existing runs are unchanged), with `distill_temperature` and
  `self_distill_warmup_steps`. Tests (39 cuda): `gpu_kl_distill_matches_cpu`,
  `gpu_distill_pulls_student_toward_teacher` (distillation provably reduces the
  student↔teacher KL), `gpu_load_weights_roundtrip`. **Verified end to end:**
  `refine-train run …cuda-distill.yaml` → log records *"snapshotted frozen teacher
  at step 3 (lambda=1, tau=2)"* → **`final: success`**. **Honest caveat:** the
  paper's evidence is single-hop factual recall on ≤8B non-reasoning models —
  transfer to proof-repair is a well-motivated hypothesis, not an established
  result; the regularizer is correct and wired, its eval benefit on Mathlib data is
  untested (and likely modest while the data ceiling dominates).

- **M16 — AVO verifier-gated kernel improvement (Kernel agent).** Applies *AVO:
  Agentic Variation Operators*' edit→verify→measure→accept loop to a real kernel,
  with the existing **CPU-parity test as the correctness gate** and measured
  throughput as the fitness. Two accepted iterations replaced the naive
  one-element-per-thread `matmul_nn` and `matmul_nt` with **tiled 16×16
  shared-memory** kernels (matching `cfg_2d`'s block dim). Same-run A/B
  (512×1024×1024, RTX 3060, warmed): **`matmul_nn` 2.40 → 1.76 ms = 1.36×**, and
  **`matmul_nt` 6.67 → 2.19 ms = 3.05×** — the big `nt` win comes from fixing the
  naive kernel's *uncoalesced* B reads (stride-k), and `nt` is the hottest training
  kernel (Linear forward + attention Q·Kᵀ), so this speeds up the whole training
  path. Every parity test (`gpu_matmul_{nn,nt}_matches_cpu`, the transposed-
  equivalence check) and the full 39-test cuda suite pass on the tiled kernels —
  the "accept only if correct AND faster" rule held. The throwaway A/B harness
  (naive variants + bench) was removed; the git history is the AVO lineage.
  Next iterations (not done): tile `matmul_tn`, register-block for higher
  arithmetic intensity.

- **M17 — SGS self-play for the Lean agent (design only, not implemented).** The
  best-matched paper for the Lean agent (*Scaling Self-Play with Self-Guidance* was
  demonstrated on Lean 4, and our Lean checker + proof-repair seed data already
  exist) — but the one we **cannot** build here: it is fundamentally an
  RL-at-scale recipe needing a prover-grade model + RL training + cluster compute,
  none of which this repo / a 6 GB 3060 has. Recorded honestly as a plan with the
  exact what-exists-vs-missing mapping and a feasible inference-only first step in
  `docs/training/sgs-lean-self-play-design.md`. No SGS code or results are claimed.

  *(The DevOps agent is intentionally omitted: none of the five surveyed papers is
  about release engineering / CI / bundle signing, so none was applied to it.)*