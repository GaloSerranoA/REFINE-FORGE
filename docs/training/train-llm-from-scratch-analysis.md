# train-llm-from-scratch Analysis

Source: <https://github.com/FareedKhan-dev/train-llm-from-scratch>

Snapshot inspected: `f3524df` (`main`, shallow clone, 2026-05-25).
License: MIT.

## Verdict

Useful as an educational reference and implementation checklist, not as code to
vendor into Refine-Forge.

The repository shows a clean small PyTorch causal-LM pipeline: Pile download,
Zstandard JSONL preprocessing, `tiktoken` tokenization into HDF5, a simple
Transformer stack, AdamW training, train/dev loss evaluation, checkpoint save,
and text generation. That helps Refine-Forge define what a minimal native
training lifecycle should expose.

It does not provide the enterprise evidence Refine-Forge needs by itself:
deterministic seeds, reproducibility manifests, resume lineage, benchmark
regression, signed promotion artifacts, compute ledger, human approval, and
production GPU/runtime proof are absent or out of scope.

## High-Impact Ideas To Port

1. Causal-LM token stream lane

   `refine-train data causal-lm-preprocess` converts JSONL or JSONL.zst text
   into a deterministic token stream with:

   - tokenizer id and tokenizer hash
   - input file hashes
   - token count
   - document count
   - deterministic chunking metadata
   - output artifact hash

   This is implemented in Rust for Refine-Forge. The Python HDF5 shape remains
   a useful reference, not the storage contract.

2. Mini causal-LM native backend

   `backend.kind = refineforge_native_causal_lm` extends the native lane beyond
   the proof-repair linear smoke trainer with a tiny causal-LM backend:

   - token embedding
   - position embedding
   - causal attention
   - MLP block
   - train/dev loss
   - generation smoke test

   This improves Refine-Forge's local training proof without claiming LLM
   production quality.

3. SFT and reasoning loss masks

   The notebook includes useful SFT/reasoning training ideas:

   - train only assistant-response tokens for chat/SFT data
   - keep prompt tokens as context but mask them from loss
   - optionally upweight structural reasoning tags

   For HELYX proof repair, Refine-Forge translates this into
   `loss-mask.bin`: train on the repaired patch and rationale fields, not on
   the diagnostic prompt itself.

4. Evidence fields for model runs

   Refine-Forge now requires these fields through run reports, conversion
   manifests, and promotion lineage checks where the local run can produce
   them:

   - parameter count
   - architecture config hash
   - tokenizer id/hash
   - train/dev loss with sample count
   - checkpoint hash
   - optimizer state hash when present
   - generation/eval prompt hash and output hash

   These fields feed the existing eval, regression, compute ledger, conversion,
   and promotion manifest gates.

5. External Python baseline adapter

   `backend.kind = pytorch_baseline` invokes an explicit external PyTorch
   command for comparison only. It must stay below HELYX/native production
   trust unless it emits Refine-Forge evidence files and passes the same
   production proof validator.

## What Not To Copy

- Do not copy PyTorch as Refine-Forge's core training backend. Refine-Forge is
  moving toward Rust-native training and evidence-first control.
- Do not treat train/dev loss as proof-repair quality. It is one metric, not an
  acceptance gate.
- Do not use the repo's default config as an enterprise production config. It
  lacks the reproducibility, resume, distributed, eval, and approval surfaces
  Refine-Forge requires.
- Do not import The Pile downloader as-is. Refine-Forge needs explicit dataset
  licensing, hash manifests, and reproducible acquisition records.

## Refine-Forge Implementation Status

1. Implemented: `refine-train data causal-lm-preprocess` converts JSONL or
   JSONL.zst `text` rows into a deterministic token stream, `chunks.json`, and
   `causal-lm-manifest.json`.
2. Implemented: `backend.kind = refineforge_native_causal_lm` runs a
   Rust-native causal smoke backend with token/position embeddings, causal
   prefix aggregation, an MLP block, SGD over a real next-token objective,
   held-out/dev metrics, generation smoke output, and checkpoint artifacts.
3. Implemented: proof-repair SFT packs include `loss-mask.bin`; prompt/context
   tokens can be masked out with `--target-only`.
4. Implemented: `refine-train evidence <run_dir>` generates eval, regression,
   compute ledger, conversion manifest, and promotion manifest files from a
   successful run report and a baseline report.
5. Implemented: both `refine agent train` and
   `refine production-proof verify` reject `human-reviewed` trust when eval
   evidence is loss-only.
6. Implemented: `backend.kind = refineforge_native_gpt` is a real from-scratch
   decoder-only transformer — trainable token/position embeddings, multi-head
   causal self-attention, pre-norm LayerNorm, GELU MLP, residual connections,
   final LayerNorm, LM head, cross-entropy, and AdamW — with full hand-written
   backpropagation. The backward pass is gradient-checked against finite
   differences (`crates/refineforge-trainer/src/native_gpt/nn.rs`), the run is
   deterministic (seeded init → reproducible `weights_sha256`), and it is
   CPU/`f64` with no Python, PyTorch, or GPU. It realizes the
   `train-llm-from-scratch` architecture in Rust and is a drop-in for the same
   evidence pipeline as the linear smoke backend. On 32 real Mathlib heldout
   rows it reaches roughly 5–6% target-token accuracy versus the linear smoke's
   ~3% — still smoke-grade, an over-parameterized model that overfits tiny data,
   and explicitly not an LLM-quality checkpoint.

The implementation is intentionally a local production-proof smoke framework,
not a claim that Refine-Forge has produced a production LLM checkpoint. A real
accepted checkpoint still requires a run with held-out quality comparison and
human approval evidence.
