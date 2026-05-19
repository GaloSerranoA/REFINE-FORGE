# `kernels/` — GPU kernels + bit-exact gates

Owned by **Section 4: CUDA / GPU Kernel Engineer** ([../ARCHITECTURE.md](../ARCHITECTURE.md) §4).

> **Status:** Orchestration scaffold shipped (`crates/refineforge-bitexact`,
> binary `refine-bitexact`). The `src/` directory is **empty** — a
> CUDA engineer fills it with actual kernel code. The stub scripts
> under `scripts/` prove the gate primitive works in both
> directions (catches a non-deterministic kernel; passes a
> deterministic one).

## What lives here

| Path | Owner | Status |
|---|---|---|
| `configs/` | CUDA engineer | kernel-experiment YAMLs (one per kernel under test). Two examples ship: `example-deterministic.yaml` (must pass) and `example-nondeterministic.yaml` (must fail — proves the gate catches drift). |
| `scripts/` | CUDA engineer | shim scripts the configs point at. Two stub scripts ship: `stub-deterministic.sh/.ps1` (echoes a fixed string) and `stub-nondeterministic.sh/.ps1` (echoes `$RANDOM`). |
| `src/` | CUDA engineer | **empty** — actual `.cu` / `.cuh` files go here. Replace the stub scripts in `scripts/` with `nvcc`-compiled binaries. |
| `runs/` | runtime | `refine-bitexact run` writes per-experiment run dirs here. Gitignored content; layout below. |

## Quick start

```bash
# Validate without running:
refine-bitexact run kernels/configs/example-deterministic.yaml --dry-run

# Run the gate (should PASS — stub-deterministic.sh emits the same bytes every call):
refine-bitexact run kernels/configs/example-deterministic.yaml

# Run the gate on a non-deterministic kernel (MUST FAIL — proves the gate catches drift):
refine-bitexact run kernels/configs/example-nondeterministic.yaml
echo "exit code: $?"   # expect non-zero
```

## Run-directory layout

```
kernels/runs/<experiment-id>/
└── bitexact-report.json   # outcome (Pass/Fail) + per-run hashes + unique-hash count
```

## How a CUDA engineer uses this

1. Write a CUDA kernel that takes a deterministic input file and
   writes its output to another file.
2. Compile it with `nvcc -o kernels/scripts/my-kernel kernels/src/my_kernel.cu`.
3. Write a kernel-experiment YAML pointing at the binary:
   ```yaml
   id: my-kernel-bit-exact
   command: "kernels/scripts/my-kernel --input fixed-input.bin --output {run_dir}/out-{run_index}.bin"
   runs: 5
   output:
     file: "{run_dir}/out-{run_index}.bin"
   env:
     CUBLAS_WORKSPACE_CONFIG: ":4096:8"
     CUDA_LAUNCH_BLOCKING: "1"
   hardware:
     gpu: "A100-80GB"
     cuda: "12.4"
   ```
4. Add `refine-bitexact run kernels/configs/my-kernel.yaml` to CI.
5. If the gate fails, debug per
   [`../docs/bit-exact-reproducibility.md`](../docs/bit-exact-reproducibility.md):
   - which env var is missing?
   - which kernel uses `atomicAdd`?
   - is cuBLAS picking different algorithms?

## What the scaffold does NOT do (honesty)

- **Does not write CUDA kernels.** That's the CUDA engineer's job.
  The scaffold gives them the gate.
- **Does not enforce determinism.** It DETECTS the absence of
  determinism. Achieving determinism (env vars, algorithm
  selection, kernel rewrites) is the engineer's domain.
- **Does not test across hardware classes.** A single CI run on a
  single GPU model proves intra-hardware determinism. Cross-
  hardware (A100 ↔ H100 ↔ consumer) requires a CI matrix with
  matching hardware in each runner — out of scope for the
  scaffold; the engineer wires up the matrix once such runners
  exist.
- **Does not handle non-CUDA backends.** The gate is hardware-
  agnostic in principle (it just runs a command N times and hashes
  outputs), so ROCm / Metal / CPU kernels work too — but the
  CUDA-specific mitigations in `docs/bit-exact-reproducibility.md`
  don't transfer.
- **Does not benchmark performance.** A fast non-deterministic
  kernel will fail the gate; a slow deterministic kernel will
  pass. Performance is a separate concern.

## Stub scripts as living tests

`scripts/stub-deterministic.sh` and `stub-nondeterministic.sh`
are the simplest possible expressions of "passes the gate" and
"fails the gate." They're useful in three ways:

1. **CI smoke tests** that exercise the gate primitive itself
   (without any real kernel installed).
2. **Onboarding** for a new CUDA engineer who wants to see the gate
   behave in both directions before plugging in real kernels.
3. **Regression tests for the gate itself** — if a refactor of
   `refineforge-bitexact` causes the deterministic case to fail or
   the non-deterministic case to pass, that's a gate bug.

The unit + end-to-end tests in `crates/refineforge-bitexact/`
already cover the first use.
