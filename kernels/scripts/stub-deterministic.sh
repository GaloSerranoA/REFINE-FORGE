#!/usr/bin/env bash
# Stub: emits the SAME bytes on every invocation. The bit-exact
# gate run against this MUST pass.
#
# Real kernels: replace this with `nvcc`-compiled CUDA binary
# that takes a fixed input and writes a deterministic output.

printf 'deterministic-kernel-output-v1\n'
exit 0
