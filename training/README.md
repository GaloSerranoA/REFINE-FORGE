# `training/` — model-training experiments

Owned by **Section 2: ML Training Engineer** ([../ARCHITECTURE.md](../ARCHITECTURE.md) §2).

> **Status:** Orchestration scaffold shipped (`crates/refineforge-trainer`,
> binary `refine-train`). NO actual model training has been performed
> against this tree. The directories are wired and the CLI runs; an
> ML engineer with GPU access fills in real configs + datasets.

## What lives here

| Path | Owner | Status |
|---|---|---|
| `configs/` | ML engineer | example experiment + sweep YAMLs (real training NOT executed) |
| `scripts/` | ML engineer / DevOps | stub trainer scripts used by tests + as a template for the real one |
| `data/` | ML engineer | training datasets (e.g. mathlib-mutations corpus). Contains only `lean-proof-repair-smoke.jsonl`, a two-row local smoke fixture. The real Mathlib corpus is still Phase 2 work. |
| `runs/` | runtime | `refine-train` writes per-experiment run directories here. Gitignored content; structure is documented below. |

## Quick start (assumes you have a real training backend installed)

```bash
# 1. Validate an experiment config without running training:
refine-train run training/configs/example-qwen-1.5b.yaml --dry-run

# 2. Actually run (assumes `axolotl` or your training tool is on PATH):
refine-train run training/configs/example-qwen-1.5b.yaml

# 3. Tail progress in another shell:
refine-train monitor training/runs/<experiment-id>

# 4. After completion:
refine-train report training/runs/<experiment-id>
cat training/runs/<experiment-id>/report.json
```

## Run-directory layout

`refine-train run` produces:

```
training/runs/<experiment-id>/
├── config.yaml          # the resolved Experiment config (audit trail)
├── train.log            # subprocess stdout+stderr (interleaved)
├── progress.jsonl       # per-step parsed metrics (one JSON object per line)
├── failures.jsonl       # one entry per failed attempt + category + recovery action
├── report.json          # final report (built by `report` subcommand or at end of `run`)
└── checkpoints/         # backend-written checkpoints (step-N or checkpoint-N)
    ├── step-100/
    └── step-200/
```

## Smoke-testing the scaffold WITHOUT a real backend

The repo ships [`scripts/stub-trainer.sh`](scripts/stub-trainer.sh) (POSIX)
and [`scripts/stub-trainer.ps1`](scripts/stub-trainer.ps1) (PowerShell).
Each emits 10 HF-style progress lines, writes a dummy
"checkpoint" file, then exits cleanly. Useful for:
- Validating the runner / progress parser / checkpoint detection
  without GPU access.
- CI smoke tests that exercise the orchestration paths.

The end-to-end test in
[`crates/refineforge-trainer/tests/end_to_end.rs`](../crates/refineforge-trainer/tests/end_to_end.rs)
uses these stubs to verify the full pipeline without needing a model.

The fine-tuning plan also ships a concrete smoke experiment:

```bash
refine-train run training/configs/lean-proof-repair-smoke-stub.yaml --dry-run
refine-train run training/configs/lean-proof-repair-smoke-stub.yaml
```

This uses `training/data/lean-proof-repair-smoke.jsonl` and the
PowerShell stub backend. It is not a model-training result; it proves
the local orchestration lane is wired before a real axolotl run exists.

## What the scaffold does NOT do (honesty)

- **Does not perform model training.** The backend (axolotl, HF
  Trainer, your script) does that. The scaffold orchestrates.
- **Does not allocate GPUs / cloud resources.** Your training script
  uses whatever GPU is available; the scaffold just spawns the
  subprocess.
- **Does not optimise hyperparameters automatically** (no Bayesian
  / population-based search). It executes a sweep config you write.
- **Does not produce a trained model.** The model produced is whatever
  your backend writes to `checkpoints/`.
- **No distributed-training coordination.** Use `accelerate launch ...`
  or `torchrun ...` in your backend command for that.
- **No W&B / TensorBoard integration.** `progress.jsonl` is the
  source of truth; pipe it into any visualisation tool.
- **No model serving.** That's downstream of training; out of scope.

## Sequencing (from `docs/repair-evaluation.md` §9)

1. ✅ AnthropicStrategy + eval harness (shipped earlier)
2. ✅ Training orchestration scaffold (this commit)
3. ⚠️ **Mathlib mutation pipeline → mathlib-5000 corpus** (multi-week)
4. ⚠️ First fine-tune run + held-out eval (depends on 3)
5. ⚠️ Distribution-shift evals (depends on 4)

Items 3-5 are real engineering work that needs a person with GPU
access and weeks of focused time. The scaffold here is the runway
they land on.

## Cost-discipline reminder

A real training run on a 7B model can easily burn $50-500 in cloud
GPU time per attempt. The `--dry-run` flag exists so you can
validate config without spending anything. **Always `--dry-run`
first.** Always check the resolved `argv` matches your intent
before spending compute.
