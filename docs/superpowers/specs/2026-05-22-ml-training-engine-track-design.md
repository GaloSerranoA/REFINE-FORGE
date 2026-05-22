# ML / Training Engine Track Design

Date: 2026-05-22

## Goal

Complete the Refine-Forge Section 2 training lane as an open-source, HELYX-compatible training engine surface. Refine-Forge will not claim to own the sovereign HELYX trainer stack (`helyx-autograd` + `helyx-train`), but it will provide the durable contract around it: dataset audit, backend invocation, checkpoint promotion, local-finetune runtime manifests, and acceptance evidence.

## Compatibility Boundary

The user-provided ownership split is binding:

| Part | Role | HELYX Surface | Refine-Forge Surface |
|---:|---|---|---|
| 3 | ML / Training | Sovereign Trainer Engine (`helyx-autograd` + `helyx-train`) -> HELYX | `refineforge-trainer` orchestration -> Refine-Forge |
| 4 | GPU / Kernels | `helyx-kernels` -> HELYX | `refineforge-bitexact` gate -> Refine-Forge |

For Part 3, Refine-Forge should speak to `helyx-train` as an external backend, the same way it can already speak to Axolotl or a custom script. The repo must produce artifacts that HELYX can consume without coupling Refine-Forge to private HELYX internals.

## Current State

The repository already has:

- `refineforge-trainer` with experiment YAML loading, backend spawning, progress parsing, retries, checkpoint discovery, reports, and sweeps.
- `refineforge-eval` with JSON report output for repair-strategy evaluation.
- `refineforge-strategies` with Anthropic and `local-finetune` command-manifest runtime support.
- `training/data/mathlib-proof-repair-v1/` with 1000 mutation rows and a finalized 1000-row Anthropic SFT split.
- `training/configs/mathlib-proof-repair-anthropic-qwen-1.5b-lora.yaml` and Axolotl config.

The remaining gap is not another trainer implementation. The gap is a stable release-grade ML handoff layer:

1. Prove the dataset is valid before spending GPU time.
2. Let experiments target `helyx-train` directly.
3. Promote successful checkpoints into the runtime manifest shape used by `refine --strategy local-finetune`.
4. Record acceptance evidence honestly, including baseline/candidate eval comparison when available.

## Design

### Dataset Audit

Add `refine-train data audit <jsonl>` backed by a new `dataset` module.

The audit validates proof-repair SFT JSONL:

- each row has a stable `id`, `prompt`, `response`, and `split`;
- `response` parses as a patch object with `start_line`, `start_char`, `end_line`, `end_char`, `new_text`, and `rationale`;
- no duplicate IDs exist;
- split counts are reported and optionally checked;
- the file SHA-256 is recorded for provenance.

This catches broken corpus rows before a paid or GPU-backed run starts.

### HELYX Backend Adapter

Extend `Experiment.backend.kind` to accept `helyx_train`.

If `backend.command` is omitted, `refineforge-trainer` resolves:

```text
helyx-train run --config {config_file} --dataset {dataset_path} --output {run_dir} --checkpoint-dir {checkpoint_dir} --resume {resume_from}
```

Dry-run mode will print the resolved command even when `helyx-train` is not installed. Real execution will fail honestly if the executable is absent.

### Promotion Manifest

Add `refine-train promote <run_dir>` backed by a new `promotion` module.

Promotion reads:

- `config.yaml`;
- `report.json`;
- the latest checkpoint directory;
- optional dataset audit JSON;
- optional baseline/candidate eval reports.

It writes:

- `refineforge-local-finetune.json`, loadable by `refine --strategy local-finetune`;
- `promotion-report.json`, containing status, blockers, source run, latest checkpoint, dataset hash, and eval comparison.

The manifest includes Refine-Forge runtime fields plus HELYX producer metadata. Unknown manifest fields are already tolerated by `local-finetune`, so this is backward-compatible.

### Acceptance Semantics

Promotion is `ready` only when:

- the training report says `final_outcome == "success"`;
- a checkpoint exists;
- if eval reports are provided, candidate repair rate is at least baseline repair rate plus the configured minimum delta.

If any condition fails, promotion writes a report and exits nonzero. This prevents a local checkpoint directory from being mistaken for an accepted model.

## Out Of Scope

- Implementing `helyx-autograd`.
- Implementing `helyx-train`.
- Running a real GPU fine-tune on this machine.
- Claiming a fine-tuned checkpoint exists.
- Native Candle inference. The command-manifest runtime remains the stable bridge until real checkpoint/tokenizer layout is known.

## Test Strategy

- TDD for dataset audit and promotion logic.
- Unit tests for `helyx_train` command resolution.
- CLI smoke tests for `refine-train data audit` and `refine-train promote`.
- Existing trainer, eval, and strategy package tests remain required.
- Final release readiness must still pass with Docker/signature skipped locally.

## Done Criteria

- `refine-train data audit training/data/mathlib-proof-repair-v1/anthropic-sft.train.jsonl --expect-rows 800 --expect-split train=800` exits 0.
- `refine-train run training/configs/helyx-mathlib-proof-repair-smoke.yaml --dry-run` prints a `helyx-train run ...` command.
- `refine-train promote <run_dir> ...` writes a local-finetune manifest and promotion report for the shipped smoke run.
- Docs describe the HELYX compatibility boundary and do not overclaim real training.
- Targeted Cargo tests and release readiness pass.
