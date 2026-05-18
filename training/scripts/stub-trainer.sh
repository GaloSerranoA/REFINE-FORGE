#!/usr/bin/env bash
# Stub trainer — emits HuggingFace-style progress lines and writes a
# dummy checkpoint, then exits. Used by the trainer crate's
# end-to-end test and as a smoke-test backend that doesn't need a
# GPU.
#
# Usage: stub-trainer.sh <output_dir> [--steps N] [--fail-at STEP]
#   <output_dir>     where to write the dummy checkpoint
#   --steps N        how many progress lines to emit (default 10)
#   --fail-at STEP   exit non-zero after emitting STEP lines (for
#                    failure-recovery tests)
set -e

OUTPUT_DIR=""
STEPS=10
FAIL_AT=""

while [ $# -gt 0 ]; do
  case "$1" in
    --steps)   STEPS="$2"; shift 2 ;;
    --fail-at) FAIL_AT="$2"; shift 2 ;;
    -*)        echo "unknown flag: $1" >&2; exit 2 ;;
    *)         if [ -z "$OUTPUT_DIR" ]; then OUTPUT_DIR="$1"; shift; else echo "extra arg: $1" >&2; exit 2; fi ;;
  esac
done

if [ -z "$OUTPUT_DIR" ]; then
  echo "usage: stub-trainer.sh <output_dir> [--steps N] [--fail-at STEP]" >&2
  exit 2
fi

mkdir -p "$OUTPUT_DIR/checkpoints"

for i in $(seq 1 "$STEPS"); do
  # HF Trainer-shaped dict
  loss=$(awk -v s="$i" 'BEGIN { printf "%.4f", 1.0 / (1 + s) }')
  lr=$(awk -v s="$i" -v t="$STEPS" 'BEGIN { printf "%.6f", 0.0002 * (1 - s/t) }')
  echo "{'loss': $loss, 'learning_rate': $lr, 'epoch': $(awk "BEGIN {print $i/10}"), 'step': $i}"

  # Save a "checkpoint" every 5 steps so the checkpoint scanner has
  # something to find.
  if [ $((i % 5)) -eq 0 ]; then
    mkdir -p "$OUTPUT_DIR/checkpoints/step-$i"
    echo "fake-weights-at-step-$i" > "$OUTPUT_DIR/checkpoints/step-$i/model.bin"
  fi

  # Failure injection.
  if [ -n "$FAIL_AT" ] && [ "$i" = "$FAIL_AT" ]; then
    echo "RuntimeError: CUDA out of memory (simulated)" >&2
    exit 1
  fi
done

echo "training completed normally"
exit 0
