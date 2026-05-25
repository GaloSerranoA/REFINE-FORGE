# Refine-Forge Native Training Framework Design

## Goal

Refine-Forge will grow from a training orchestration and evidence layer into a training control plane plus a native training framework. The first production slice is a deterministic local native backend that performs real gradient-based training on proof-repair JSONL data, emits progress records, writes checkpoint artifacts, and feeds the existing report and promotion surfaces without claiming production model quality.

## Current Boundary

Today `refineforge-trainer` audits datasets, launches external backends, records progress, resumes checkpoints, and emits reports. That is useful, but it is not enough for the ML Training Engineer Agent target. The current truthful level is orchestration readiness unless a live checkpoint, evaluation report, regression report, compute ledger, promotion manifest, and human approval exist.

## Target Boundary

Refine-Forge will own:

- The ML engineer agent.
- Dataset audit and lineage.
- Training lifecycle and checkpoint evidence.
- Native local training backend.
- Evaluation and regression gates.
- Promotion manifests and human-review gates.
- Quantized serving evaluation, including TurboQuant-style evidence.

HELYX compatibility remains required. Refine-Forge native training must read and write evidence formats that HELYX can consume, but it must not depend on HELYX binaries to prove local smoke training.

## Native Backend v0

`backend.kind = refineforge_native` adds a built-in training backend. It does not spawn a subprocess. It uses the existing run directory layout:

- `config.yaml`
- `train.log`
- `progress.jsonl`
- `checkpoints/step-N/native-checkpoint.json`
- `report.json`

The v0 trainer is intentionally small:

- JSONL proof-repair rows are read from `dataset.path`.
- `prompt` is the input text.
- `response` is parsed as JSON and `new_text` is the target text.
- A deterministic hashed feature vector is built from the prompt.
- A trainable linear model predicts deterministic target buckets derived from the patch text.
- Cross-entropy loss is computed.
- Weights are updated with SGD.
- Progress records report `loss`, `accuracy`, and `learning_rate`.

This is real local training, but it is not an LLM and it must not be described as proof of model quality.

## TurboQuant Track

TurboQuant is not the base trainer. It becomes a quantization and serving-readiness track after the native trainer can emit checkpoints:

- `refineforge-quant` CPU reference.
- Deterministic codebooks and quantization reports.
- MSE and inner-product distortion reports.
- KV-cache or activation quantization evaluation.
- Kernel bit-exact gate integration.
- Promotion blocking when quantized quality falls below policy.

Initial Lean claims for TurboQuant are model-only until linked to Rust and kernel evidence.

## Trust Rules

The Training Agent remains bounded by evidence:

- Native backend smoke run: `measured-only`.
- Training loss alone: no model-quality claim.
- Checkpoint plus eval plus regression plus compute ledger: candidate promotion evidence.
- Human-approved promotion manifest: eligible for higher trust.
- Quantized promotion: requires native or HELYX checkpoint evidence plus quantization and kernel reports.

## First Milestone Acceptance

The first implementation is complete only when:

- `backend.kind = refineforge_native` validates.
- A native run writes progress, logs, and a checkpoint without external trainer binaries.
- `report.json` records `compute_ledger.backend_kind = "refineforge_native"`.
- Tests prove the native backend can run on Windows and Unix.
- Documentation no longer says Refine-Forge can only orchestrate training.
