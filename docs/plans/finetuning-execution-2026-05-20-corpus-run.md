# Fine-Tuning Execution Addendum - 2026-05-20

## Scope

This addendum covers the requested live execution of the previously blocked
Mathlib corpus, Anthropic teacher-generation, training, evaluation, and runtime
checkpoint phases.

## Completed Locally

- Fetched Mathlib source outside the repo at
  `D:\AI-PROJECTS-GALO\PROJECTS\_cache\mathlib4`.
- Source commit: `1563983d15a6074e0ccae939e29379738554942e`.
- Generated `training/data/mathlib-proof-repair-v1/all.jsonl` with 1000 real
  Mathlib-derived proof-repair examples.
- Split:
  - train: 800
  - val: 100
  - heldout: 100
- Mutation: replace the original `:= by` proof with
  `exact __refineforge_missing_proof__`.
- Diagnostic recorded for every row:
  `unknown identifier __refineforge_missing_proof__`.
- Expected patch: LSP-shaped range plus original Mathlib proof text.
- Added tested tooling:
  - `scripts/generate_mathlib_repair_corpus.py`
  - `scripts/anthropic_teacher_generate.py`
  - `scripts/test_mathlib_corpus_tools.py`
- Added real training config surfaces:
  - `training/configs/mathlib-proof-repair-anthropic-qwen-1.5b-lora.yaml`
  - `training/configs/axolotl-mathlib-proof-repair-qwen-1.5b-lora.yaml`

## Anthropic Teacher Generation

Corrected-corpus Anthropic generation started with:

```powershell
python scripts\anthropic_teacher_generate.py `
  --input training\data\mathlib-proof-repair-v1\all.jsonl `
  --output training\data\mathlib-proof-repair-v1\anthropic-sft.jsonl `
  --limit 1000 `
  --max-cost-usd 50 `
  --model claude-sonnet-4-6 `
  --max-tokens 512 `
  --concurrency 1 `
  --retries 6 `
  --backoff-seconds 10
```

The run was later resumed against the same ID-resumable output file and
completed:

- Corrected teacher rows: 1000
- Unique corrected IDs: 1000
- Final split files:
  - `anthropic-sft.train.jsonl`: 800 rows
  - `anthropic-sft.val.jsonl`: 100 rows
  - `anthropic-sft.heldout.jsonl`: 100 rows
- Manifest: `anthropic-sft.manifest.json`, `complete=true`
- Teacher model: `claude-sonnet-4-6`
- Estimated spend recorded in rows: `$5.390637`
- Final manifest reports `valid_patch_rows=1000`, `invalid_patch_rows=0`,
  `fallback_teacher_responses=0`, and `normalized_patch_rows=0`.
- The final cleanup connected to Anthropic again and retried only stale
  fallback/mismatch rows; no deterministic normalization remains in the clean
  training dataset.

The earlier concurrent run against a pre-fix corpus is intentionally gitignored
raw audit evidence at:

- `training/data/mathlib-proof-repair-v1/anthropic-sft.raw-before-doc-boundary-fix.jsonl`

It is not the clean training dataset.

## Still Blocked Externally

1. Cogn8ty validation is still blocked: `127.0.0.1:7742` was not listening.
2. Real training is still blocked on local training runtime availability:
   `axolotl`, `torch`, `transformers`, and `peft` were not installed in the
   active Python environment.
   - `refine-train ... --dry-run` resolved the Axolotl command.
   - Actual `refine-train ...` failed with `spawning training backend axolotl:
     program not found`.
3. Real checkpoint/runtime manifest is blocked until a successful training run
   produces adapters/checkpoints/tokenizer files.
4. Held-out repair-success evaluation is blocked until a real checkpoint exists.
