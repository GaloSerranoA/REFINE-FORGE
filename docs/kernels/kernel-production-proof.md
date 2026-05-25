# Kernel Production Proof Checklist

The Kernel agent may emit `human-reviewed` only when the evidence pack
includes:

| Requirement | Evidence |
|---|---|
| Real kernel source | source path and SHA-256 |
| CPU reference | reference implementation or golden output hash |
| Bit-exact run | run report and fixture hash |
| Hardware matrix | GPU, driver, CUDA toolkit, OS, CPU architecture |
| Compiler/runtime metadata | rustc, nvcc, build flags |
| Tolerance policy | exact hash or numeric tolerance justification |
| Performance baseline | latency/throughput and regression threshold |
| HELYX handoff | config, source, and report hashes |
| Human approval | named reviewer, date, decision |

## Current Local Evidence

`production-proof/evidence/kernel-local-cuda-2026-05-25/` records a real
local CUDA smoke run for `kernels/src/hvector_add.cu` on an RTX 3060 Laptop
GPU with CUDA 13.2. The Kernel agent accepts the non-approval evidence and
keeps production proof blocked only on `approvals/kernel.json`.

This is intentionally a smoke proof for the Refine-Forge gate. It does not
claim full HELYX kernel correctness, cross-GPU portability, or production
performance until reviewed HELYX kernels and human kernel approval exist.
