# `training/` — model-training experiments

Owned by **Section 2: ML Training Engineer** ([../ARCHITECTURE.md](../ARCHITECTURE.md) §2).

> **Status:** The training control plane is shipped
> (`crates/refineforge-trainer`, binary `refine-train`). Dataset audit,
> built-in `refineforge_native` proof-repair smoke training,
> Axolotl/custom/HELYX command resolution, run reports, and local-finetune
> promotion are implemented. No accepted production proof-repair checkpoint
> has been produced yet.

## What lives here

| Path | Owner | Status |
|---|---|---|
| `configs/` | ML engineer | example experiment + sweep YAMLs, including native, Axolotl, and HELYX-compatible Mathlib proof-repair configs |
| `scripts/` | ML engineer / DevOps | stub trainer scripts used by tests + as a template for the real one |
| `data/` | ML engineer | training datasets. Contains the two-row smoke fixture plus `mathlib-proof-repair-v1/`, a 1000-row Mathlib-derived mutation corpus and finalized Anthropic SFT split. |
| `runs/` | runtime | `refine-train` writes per-experiment run directories here. Gitignored content; structure is documented below. |

## Quick start

```bash
# 1. Audit the finalized Mathlib proof-repair training split:
refine-train data audit \
  training/data/mathlib-proof-repair-v1/anthropic-sft.train.jsonl \
  --expect-rows 800 \
  --expect-split train=800

# 2. Validate the built-in native smoke trainer without spending compute:
refine-train run training/configs/refineforge-native-proof-repair-smoke.yaml --dry-run

# 3. Run local native smoke training:
refine-train run training/configs/refineforge-native-proof-repair-smoke.yaml

# 4. Validate an external experiment config without running training:
refine-train run training/configs/example-qwen-1.5b.yaml --dry-run

# 5. Actually run an external trainer (assumes `axolotl`, `helyx-train`, or your training tool is on PATH):
refine-train run training/configs/example-qwen-1.5b.yaml

# 6. Tail progress in another shell:
refine-train monitor training/runs/<experiment-id>

# 7. After completion:
refine-train report training/runs/<experiment-id>
cat training/runs/<experiment-id>/report.json

# 8. Promote a successful checkpoint into the local-finetune runtime shape:
refine-train promote training/runs/<experiment-id> \
  --out-dir training/runs/<experiment-id>/promoted-local-finetune \
  --model-id proof-repair-local-v1 \
  --command your-infer-runtime \
  --command-arg --checkpoint \
  --command-arg "{checkpoint_dir}" \
  --producer helyx-train \
  --require-success
```

## Run-directory layout

`refine-train run` produces:

```
training/runs/<experiment-id>/
├── config.yaml          # the resolved Experiment config (audit trail)
├── train.log            # native trainer log or subprocess stdout+stderr
├── progress.jsonl       # per-step parsed metrics (one JSON object per line)
├── failures.jsonl       # one entry per failed attempt + category + recovery action
├── report.json          # final report (built by `report` subcommand or at end of `run`)
├── checkpoints/         # native or backend-written checkpoints (step-N or checkpoint-N)
└── promoted-local-finetune/  # optional `promote` output
    ├── refineforge-local-finetune.json
    └── promotion-report.json
```

The local-finetune promotion directory is the path passed to:

```bash
refine repair <claim-id> --strategy local-finetune --weights-path training/runs/<experiment-id>/promoted-local-finetune
```

`promote` records the source `report.json`, latest checkpoint, model id,
producer, and runtime command. It writes the runtime manifest only when
promotion is ready; blocked promotions still write `promotion-report.json`.

Checkpoint directories use the backend convention:

```
checkpoints/
    ├── step-100/
    └── step-200/
```

## Native smoke training WITHOUT an external backend

The built-in `refineforge_native` backend runs in-process. It reads
proof-repair JSONL, hashes prompt text into deterministic features, trains a
small linear softmax model with SGD against patch `new_text` buckets, writes
`progress.jsonl`, and emits `native-checkpoint.json` under `checkpoints/step-N/`.

```bash
refine-train run training/configs/refineforge-native-proof-repair-smoke.yaml --dry-run
refine-train run training/configs/refineforge-native-proof-repair-smoke.yaml
```

This is real local gradient-based smoke training. It is not an LLM-quality
checkpoint and does not prove proof-repair model improvement.

## Smoke-testing external orchestration WITHOUT a real backend

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

The fine-tuning plan ships a concrete smoke experiment:

```bash
refine-train run training/configs/lean-proof-repair-smoke-stub.yaml --dry-run
refine-train run training/configs/lean-proof-repair-smoke-stub.yaml
```

This uses `training/data/lean-proof-repair-smoke.jsonl` and the
PowerShell stub backend. It is not a model-training result; it proves
the external orchestration lane is wired before a real axolotl run exists.

## Mathlib proof-repair corpus

`training/data/mathlib-proof-repair-v1/` contains the first real corpus lane:

- `all.jsonl`: 1000 Mathlib-derived broken/fixed Lean repair examples.
- `train.jsonl`, `val.jsonl`, `heldout.jsonl`: 800/100/100 mutation splits.
- `anthropic-sft.jsonl`: 1000 Anthropic-backed SFT rows from the mutation corpus.
- `anthropic-sft.train.jsonl`, `anthropic-sft.val.jsonl`,
  `anthropic-sft.heldout.jsonl`: finalized 800/100/100 SFT splits.
- `manifest.json` and `anthropic-sft.manifest.json`: source commit, counts,
  spend estimate, and validation summary.

Generation and retry tooling lives in repo-root `scripts/`:

```bash
python scripts/generate_mathlib_repair_corpus.py --help
python scripts/anthropic_teacher_generate.py --help
```

The real LoRA/QLoRA Axolotl config entry point is:

```bash
refine-train run training/configs/mathlib-proof-repair-anthropic-qwen-1.5b-lora.yaml --dry-run
```

The HELYX-compatible entry point is:

```bash
refine-train run training/configs/helyx-mathlib-proof-repair-smoke.yaml --dry-run
```

That resolves to:

```text
helyx-train run --config training/configs/helyx-proof-repair-qwen-1.5b-lora.yaml --dataset training/data/mathlib-proof-repair-v1/anthropic-sft.train.jsonl --output <run_dir> --checkpoint-dir <run_dir>/checkpoints
```

Production-scale LLM training still depends on a local Axolotl/PyTorch or
HELYX runtime and GPU.

## What the scaffold does NOT do (honesty)

- **Does not claim production model quality from native smoke training.**
  `refineforge_native` trains a small deterministic local model to prove the
  lifecycle and evidence path. Production model quality still requires held-out
  evaluation, regression evidence, compute ledger, promotion manifest, and
  human approval.
- **Does not allocate GPUs / cloud resources.** Your training script
  uses whatever GPU is available; the scaffold just spawns the
  subprocess.
- **Does not optimise hyperparameters automatically** (no Bayesian
  / population-based search). It executes a sweep config you write.
- **Does not certify a trained model by itself.** The backend writes
  `checkpoints/`; `promote` only packages a successful checkpoint into the
  local-finetune runtime contract after `report.json` passes its readiness
  checks.
- **No distributed-training coordination.** Use `accelerate launch ...`
  or `torchrun ...` in your backend command for that.
- **No W&B / TensorBoard integration.** `progress.jsonl` is the
  source of truth; pipe it into any visualisation tool.
- **No model serving.** That's downstream of training; out of scope.

## Sequencing (from `docs/repair-evaluation.md` §9)

1. ✅ AnthropicStrategy + eval harness (shipped earlier)
2. ✅ Training control plane, native smoke trainer, dataset audit, HELYX adapter, and promotion handoff
3. ✅ **Mathlib mutation pipeline → first N=1000 corpus**
4. ⚠️ First accepted fine-tune run + held-out eval (depends on Axolotl/PyTorch or HELYX + GPU)
5. ⚠️ Distribution-shift evals (depends on 4)

Items 4-5 are real engineering work that needs a person with GPU
access and focused time. The scaffold and first real corpus are now
in place.

## Cost-discipline reminder

A real training run on a 7B model can easily burn $50-500 in cloud
GPU time per attempt. The `--dry-run` flag exists so you can
validate config without spending anything. **Always `--dry-run`
first.** Always check the resolved `argv` matches your intent
before spending compute.
