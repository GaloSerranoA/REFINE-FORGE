# HRM-Text Analysis

Source: <https://github.com/sapientinc/HRM-Text>

Snapshot inspected: `056c4ec` (`main`, shallow clone, 2026-05-25).
License: Apache-2.0.

## Verdict

HRM-Text is highly relevant to Refine-Forge's ML agent, but mostly as a
systems and evidence-design reference. It should not be vendored directly into
the trust boundary.

The repository is a serious pretraining framework: Hydra configs, FSDP2
training, PrefixLM sequence packing, FlashAttention 3, deterministic SFT data
packing, distributed multipack batching, checkpoint resume, W&B logging,
benchmark evaluation, inference, and conversion to Hugging Face/safetensors.

The useful lesson for Refine-Forge is not "copy this model." The useful lesson
is what a real training framework has to record: exact config, tokenizer,
dataset metadata, per-epoch packing, distributed world size, checkpoint shards,
EMA policy, benchmark results, conversion manifest, and hardware/runtime
requirements.

## Strong Ideas To Port

1. PrefixLM / target-only SFT masking

   `dataset_new.py` constructs instruction and response regions and masks
   instruction tokens when `target_only` is true. This maps directly to
   proof-repair SFT: diagnostics and broken Lean code are context, while the
   repaired patch/rationale should be the supervised target.

2. Deterministic SFT packing

   `scripts/prepare_sft_data.py` writes `tokens.npy`, `metadata.json`,
   `tokenizer.json`, and per-epoch shuffled index arrays. It uses a seed and
   stores the tokenizer copy for self-contained training data. Refine-Forge
   adopts this pattern in Rust through `refine-train data pack-sft`.

3. Multipack batch scheduling

   `multipack_sampler.py` uses longest-processing-time allocation to pack
   variable-length samples across distributed ranks. This is useful for GPU
   efficiency; Refine-Forge now records it in `packing_report.json` and
   `multipack-plan.json`: slot utilization, rank balance, max sequence length,
   and dropped batch policy.

4. Training lineage files

   HRM-Text writes `all_config.yaml`, `train_metadata.yaml`, FSDP2 checkpoint
   directories, and carry files. Refine-Forge should make analogous lineage
   files mandatory evidence for any promoted checkpoint.

5. Benchmark grouping and result schema

   `evaluation/main.py` groups prompts by generation config and dispatches
   metrics to benchmark wrappers. Refine-Forge can reuse the concept for
   proof-repair eval suites: group by decoding policy, hash prompts, hash
   outputs, and emit machine-readable metric reports.

6. Conversion manifest

   `conversion/convert_to_hf.py` maps FSDP2 checkpoints to Hugging
   Face/safetensors format and writes `config.json` plus `model.safetensors`.
   Refine-Forge now has `refine-train evidence`, which writes
   `training/conversion-manifest.json` and requires promotion manifests to
   hash that conversion evidence.

7. Hardware/runtime boundary

   The README states the training path targets Hopper-class GPUs because the
   attention path depends on FlashAttention 3. Refine-Forge should encode this
   explicitly in compute ledgers: GPU model, CUDA version, FlashAttention
   version, FSDP world size, and whether the run used a supported kernel path.

## Gaps That Matter For Refine-Forge

- `pretrain.py` has an internal `EVAL STACK: TBD TODO`; benchmark evaluation
  exists as a separate command, not as an in-loop promotion gate.
- It relies on W&B for metrics logging. Refine-Forge needs local JSON evidence
  first, with W&B only as an optional mirror.
- It is Python/PyTorch/FlashAttention/FSDP2-centered. Useful for HELYX
  compatibility and baseline studies, not the Rust-native core by itself.
- The repo's benchmark claims are external reference-run claims unless
  reproduced locally with artifact hashes and hardware evidence.
- Dataset acquisition is delegated to a companion `data_io` project; Refine-
  Forge still needs explicit dataset license, source, tokenizer, and sampling
  manifests.

## Refine-Forge Implementation Status

1. Implemented: `refine-train data pack-sft` emits a self-contained token
   pack:

   - `tokens.bin` or `tokens.npy` equivalent
   - tokenizer copy/hash
   - per-epoch shuffle indices
   - prompt/target span indices
   - dataset/source manifest
   - pack hash

2. Implemented: target-only loss masking is recorded as `loss-mask.bin` and
   the native causal backend consumes `dataset.format = sft_pack`.

3. Implemented: `packing_report.json` and `multipack-plan.json` capture:

   - total tokens
   - supervised target tokens
   - context tokens
   - max sequence length
   - slot utilization
   - rank balance
   - dropped/trimmed samples

4. Implemented: Training Agent and production-proof verification require
   checkpoint lineage:

   - config hash
   - train metadata hash
   - tokenizer hash
   - checkpoint shard hashes
   - EMA policy
   - resume source and epoch

5. Implemented: optional external backend presets are accepted through
   `backend.kind = hrm_text` and `backend.kind = pytorch_baseline`. They
   resolve to explicit `torchrun` command adapters and remain evidence-bound:

   ```yaml
   backend:
     kind: hrm_text
     config_file: training/configs/hrm-text.yaml
   ```

   This can be a HELYX/PyTorch baseline lane, but it must emit Refine-Forge
   evidence before any trust upgrade.

6. Implemented: `refine-train evidence <run_dir>` converts a successful run
   report into the existing `training/eval-report.json`,
   `training/regression-report.json`, `training/compute-ledger.json`,
   `training/conversion-manifest.json`, and
   `training/promotion-manifest.json` contracts.

## Boundary

HRM-Text can help the Training Agent become more real because it demonstrates
production-grade training mechanics. It cannot by itself make Refine-Forge
`human-reviewed`. Only locally generated eval, regression, compute ledger,
promotion manifest, checkpoint hash, and human approval evidence can do that.
