# Bit-exact reproducibility for GPU kernels

Owned by **Section 4: CUDA / GPU Kernel Engineer**
([../ARCHITECTURE.md](../ARCHITECTURE.md) §4).

> **Status:** Enterprise gate shipped
> (`crates/refineforge-bitexact`, binary `refine-bitexact`). The gate
> supports strict contract linting, expected output hashes, input-file
> manifests, per-run reports, and run-all aggregation. **No production
> CUDA kernel ships here**; HELYX owns real `helyx-kernels`.

## 1. What "bit-exact" means here

Two invocations of the same kernel, given the same input bytes,
must produce **byte-identical output**. Not "approximately equal."
Not "within floating-point tolerance." Identical SHA-256.

This matters because:
- HELYX (the consumer of refineforge) markets "mathematically
  verified, not just tested." A model whose outputs drift across
  invocations cannot be "verified" — only "statistically
  characterised."
- A signed bundle (Sigstore + Rekor — see SECURITY.md) is only as
  meaningful as the reproducibility of what's signed. A bundle
  containing a non-deterministic kernel signs a moving target.
- Audit / regulator / customer review needs to be able to re-run
  the kernel and get the same answer. Otherwise "you said it
  produced X" is unfalsifiable.

## 2. Where non-determinism comes from on a GPU

Real sources, ranked by how often they bite:

| Source | Symptom | Mitigation |
|---|---|---|
| **`atomicAdd` ordering** | concurrent threads accumulate in non-deterministic order; results drift in low bits | Avoid `atomicAdd` for float reductions; use deterministic reduction trees (`__shfl_down_sync` ladder) or accumulate in higher precision then round once |
| **cuBLAS algorithm selection** | `cublasGemmEx` picks different algorithms per call based on heuristics | Set `CUBLAS_WORKSPACE_CONFIG=:4096:8` AND pin algorithm via `cublasSetAlgorithm` |
| **cuDNN convolution algorithm** | `cudnnConvolutionForward` picks different algos per call | Use `CUDNN_DETERMINISTIC` algorithms (e.g. `CUDNN_CONVOLUTION_FWD_ALGO_IMPLICIT_GEMM`); set `CUDNN_DETERMINISTIC=1` env var |
| **Reduction tree ordering** | warp-level / block-level reductions complete in different orders across launches | Force serial reduction (slow) or use shuffle-based deterministic ladder |
| **TF32 / FP16 / BF16 mixed precision** | hardware may auto-downgrade; cast order varies | Explicit `torch.set_float32_matmul_precision('highest')`; avoid `tf32` allowlists |
| **Stream synchronization order** | kernels on different streams race | Single stream OR explicit `cudaStreamSynchronize` between dependent launches |
| **Memory allocator ordering** | `cudaMalloc` returns different addresses; downstream pointer-dependent code drifts | Use `cudaMallocAsync` + a single memory pool with `cudaMemPoolSetAttribute(cudaMemPoolAttrReleaseThreshold, UINT64_MAX)` |
| **`__half2`/`__bfloat162` fused ops** | hardware-fused mul-add (FMA) varies across GPU generations | Avoid FMA-sensitive math; cast to fp32 for the operation |
| **CUDA driver / runtime version** | algorithm internals can change across versions | Pin driver + runtime in CI matrix; record both in `bitexact-report.json`'s `hardware` field |

## 3. Framework-level mitigations (Python / PyTorch)

For PyTorch-based training (refineforge-trainer's most common
backend), set ALL of:

```python
import torch
import numpy as np
import random
import os

# Python
random.seed(0)
os.environ["PYTHONHASHSEED"] = "0"

# NumPy
np.random.seed(0)

# PyTorch CPU + CUDA
torch.manual_seed(0)
torch.cuda.manual_seed_all(0)

# Force deterministic algorithms (raises if any op has no det version)
torch.use_deterministic_algorithms(True, warn_only=False)

# cuBLAS workspace required for determinism
os.environ["CUBLAS_WORKSPACE_CONFIG"] = ":4096:8"

# cuDNN
torch.backends.cudnn.deterministic = True
torch.backends.cudnn.benchmark = False   # benchmark mode picks algos
torch.backends.cuda.matmul.allow_tf32 = False
```

Required env vars for the launcher (e.g. inside the
`refine-bitexact` experiment's `env:` block):

```yaml
env:
  CUBLAS_WORKSPACE_CONFIG: ":4096:8"
  CUDA_LAUNCH_BLOCKING: "1"   # serialises kernel launches
  PYTHONHASHSEED: "0"
  TF_DETERMINISTIC_OPS: "1"   # if TF is involved
```

## 4. The gate primitive: how `refine-bitexact` decides

Given a kernel-experiment YAML, the gate:
1. Runs the kernel command N times (N ≥ 2; recommended 5+).
2. Captures each run's output (stdout bytes OR a file's bytes).
3. SHA-256-hashes each output.
4. Hashes every declared `input_files` entry into the input manifest.
5. If all output hashes match and any configured `expected_sha256`
   matches → `Outcome::Pass` (exit 0).
6. If any hash disagrees, any run errored, any input is missing, or the
   stable output does not match `expected_sha256` → `Outcome::Fail`
   (exit non-zero).
7. Writes `bitexact-report.json` with per-run hashes, unique-hash count,
   input manifest, baseline hash, and per-run timing.

The gate does **not** care WHICH source of non-determinism is at
fault. It only tells you whether the kernel IS deterministic. If
it isn't, work down the table in §2 + apply the mitigations in §3.

## 5. HELYX strict contract profile

`profile: helyx_cuda` is the Refine-Forge contract for HELYX kernel
evidence. The linter requires:

```yaml
template_version: refineforge-bitexact-v1
producer: helyx-kernels
kernel_id: helyx.<domain>.<kernel>
profile: helyx_cuda
runs: 5
expected_sha256: "<64 lowercase hex chars>"
env:
  CUBLAS_WORKSPACE_CONFIG: ":4096:8"
  CUDA_LAUNCH_BLOCKING: "1"
hardware:
  gpu: "<runner GPU>"
  cuda: "<runtime version>"
  driver: "<driver version>"
```

Run:

```bash
refine-bitexact lint kernels/configs/<kernel>.yaml
refine-bitexact run kernels/configs/<kernel>.yaml
```

The shipped `kernels/configs/helyx-bitexact-smoke.yaml` is a contract
fixture only. It uses the deterministic PowerShell stub and does not claim
that a HELYX CUDA kernel exists in this repository.

## 6. CI integration

Add a job to `.github/workflows/ci.yml`:

```yaml
bit-exact-gate:
  runs-on: [self-hosted, gpu]   # needs a GPU runner
  steps:
    - uses: actions/checkout@v4
    - name: Build refine-bitexact
      run: cargo build --release --bin refine-bitexact
    - name: Run all bit-exact gates
      run: ./target/release/refine-bitexact run-all kernels/configs --summary-json kernels/runs/run-all-summary.json
```

A GPU runner is required because CPU-only CI cannot exercise real
CUDA non-determinism. The repo's stub-script-based tests run on
any runner and prove the gate primitive works; the actual kernel
gates need real hardware.

## 7. Cross-hardware verification (deferred)

True "bit-exact across hardware classes" requires N different
hardware classes (e.g. A100, H100, RTX 4090). Each needs its own
runner; each runs the same experiment; all should produce the
same hash. This is a CI matrix expansion, not a code change.

Strategy when it's time to wire this up:
- Tag each runner with hardware metadata (`gpu: A100-80GB`,
  `cuda: 12.4`, `driver: 550.54.15`).
- Run the same kernel-experiment on every tagged runner.
- Aggregate the per-runner reports; a "fully bit-exact" claim
  requires ALL runners to agree.
- If a kernel is bit-exact within hardware class but not across,
  document the class scope in the claim YAML's `notes` field.

## 8. What the gate does NOT do

- **Does not write CUDA kernels.** The CUDA engineer does. The
  gate tells them whether they succeeded.
- **Does not enforce determinism** — only detects its absence.
- **Does not pick algorithms** — that's `cublasSetAlgorithm` /
  `cudnnSetConvolutionAlgorithm` / etc. in the kernel itself.
- **Does not handle non-CUDA backends** explicitly. The gate is
  hardware-agnostic (any command that produces deterministic
  bytes will pass) but the mitigations in §2-§3 are CUDA-specific.
  ROCm has analogous knobs (`HIP_LAUNCH_BLOCKING`,
  `MIOPEN_DETERMINISTIC`); Metal has yet another set.
- **Does not benchmark performance.** A slow deterministic kernel
  passes; a fast non-deterministic kernel fails. Performance
  tracking is out of scope.
- **Does not test across hardware classes** by default — single-
  runner only. Cross-hardware is the CI matrix expansion in §7.
- **Does not make a HELYX implementation claim from a stub.** The HELYX
  smoke config proves contract compatibility only.

## 9. Reading list for a CUDA engineer joining the project

1. NVIDIA: *Floating Point and IEEE 754 Compliance for Nvidia GPUs*
2. NVIDIA cuBLAS docs: "Reproducibility" section
3. NVIDIA cuDNN docs: "Reproducibility and determinism"
4. PyTorch: `torch.use_deterministic_algorithms` docs
5. *Reproducible deep learning with PyTorch* (community blog;
   covers the env-var checklist)
6. `refineforge-bitexact` source — the gate is ~400 LoC and
   readable in an afternoon
