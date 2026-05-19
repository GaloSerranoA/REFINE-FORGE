#!/usr/bin/env bash
# Stub: emits DIFFERENT bytes on every invocation. The bit-exact
# gate run against this MUST fail (non-zero exit). Proves the gate
# correctly detects non-determinism.
#
# In real CUDA kernels, the analogues are:
#   - atomicAdd ordering (concurrent writers race)
#   - cuBLAS GEMM algorithm selection (per-call)
#   - cuDNN convolution algorithm selection
#   - reduction tree ordering (warp-level)
# All of which produce slightly-different bit patterns across runs
# unless explicitly forced deterministic via env vars + algorithm
# pinning. See docs/bit-exact-reproducibility.md.

printf 'nondeterministic-output-%d-%d\n' "$RANDOM" "$$"
exit 0
