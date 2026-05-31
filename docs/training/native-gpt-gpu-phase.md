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
