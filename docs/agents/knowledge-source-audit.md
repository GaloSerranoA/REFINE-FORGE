# Agent Knowledge Source Audit

Generated from local source inspection on 2026-05-24.

Inputs:

- `D:\OneDrive\AI\DOCS\PDF'S`
- `D:\AI-PROJECTS-GALO\repositories\AGI RESEARCH\llms-from-scratch-rs`

Extraction artifact:

- `D:\OneDrive\AI\DOCS\PDF'S\_refineforge_pdf_analysis.json`

The PDF pass extracted all 21 PDFs without parse errors. The analysis file keeps
bounded samples, headings, metadata, page counts, and full-document keyword
counts for agent-role triage.

## High-Value Sources By Agent

### Lean 4 Specialist

Use these as proof-engineering and formal-methods background, not as verified
Refine-Forge claims:

- `mcs.pdf` — strongest logic/proof/computability source in the folder.
- `FoundationsOfComputation_2.3.2_6x9.pdf` — logic, proof, grammars, and
  Turing-machine foundations.
- `FUNDATIONS OF ML.pdf` — useful for formalizing learning-theory concepts if
  training claims later need model-level invariants.

### Infrastructure / DevOps

Use these mainly for risk, audit, and agent-safety gate design:

- `2512.20798v4.pdf` — outcome-driven constraint-violation benchmark for
  autonomous agents.
- `2604.20995v1.pdf` — value-conflict/alignment-faking diagnostics; useful for
  policy and monitoring reports.
- `Barkley negation.pdf` — peer/self-preservation style risk scenarios; useful
  for agent safety and shutdown-policy tests.
- `Cogn8ty vs State-of-the-Art LLMs - A Comparative Architectural Whitepaper.pdf`
  — internal architecture comparison; useful for HELYX/COGN8TY compatibility
  language.

### ML / Training

Use these as training/evaluation source material:

- `llms-from-scratch-rs` — Rust/Candle implementation of GPT-style training,
  tokenization, attention, pretraining, classification fine-tuning,
  instruction fine-tuning, LoRA, and CUDA feature flags.
- `BartoSutton.pdf` — reinforcement-learning reference.
- `FUNDATIONS OF ML.pdf` — generalization, PAC learning, and standard ML
  foundations.
- `2510.01171v3.pdf` — verbalized sampling and mode-collapse mitigation.
- `2603.19312v2.pdf` — latent world-model training ideas.
- `ARC-AGI-2 Benchmark.pdf` — reasoning benchmark design.
- `Legal_RAG_Hallucinations.pdf` — RAG grounding and hallucination evaluation.

### CUDA / GPU Kernel

Use these for bit-exact gates, numerical fixtures, and matrix/tensor workload
design:

- `Vectors-Matrices-Least-Squares.pdf` / `vmls.pdf` — duplicate files by
  SHA-256; keep one canonical reference for linear algebra fixtures.
- `paper.pdf` — matrix/evolution-strategy material with high kernel and
  training relevance.
- `FUNDATIONS OF ML.pdf` — matrix and optimization background.
- `llms-from-scratch-rs` — Candle tensor code, `Device::cuda_if_available`, and
  `cuda` feature wiring that can inform HELYX-compatible smoke fixtures.

### Agent Runtime

Use these for autonomy, orchestration, and safety boundaries:

- `Heavy Thinking as the Inner Skill in Agentic Harness.pdf`
- `2512.20798v4.pdf`
- `Barkley negation.pdf`
- `2604.20995v1.pdf`

## Concrete `llms-from-scratch-rs` Hooks

Evidence from the local repo:

- `README.md` lists chapters covering text data, attention, GPT
  implementation, pretraining, classification fine-tuning, and instruction
  fine-tuning.
- `Cargo.toml` uses Candle and exposes a `cuda` feature:
  `cuda = ["candle-core/cuda", "candle-nn/cuda"]`.
- `src/listings/ch03.rs` implements self-attention, causal attention, and
  multi-head attention.
- `src/listings/ch04.rs` implements GPT model components.
- `src/listings/ch05.rs`, `src/listings/ch06.rs`, and `src/listings/ch07/`
  cover loss, training, classification, and instruction-tuning flows.
- `src/listings/apdx_e.rs` includes LoRA layers and GPT2 loading/fine-tuning
  helpers.

Recommended Refine-Forge use:

- Training agent: add a future optional adapter that can run selected
  `llms-from-scratch-rs` examples as safe dry-run training fixtures.
- Kernel agent: use the CUDA feature and Candle tensor calls as a source for
  bit-exact fixture design, not as proof of CUDA correctness.
- Lean agent: use attention/GPT modules as future model-spec candidates only
  after claim YAML and refinement docs exist.

## Trust Boundary

These sources can seed prompts, memory, fixtures, and future claims. They do
not upgrade any agent `trust_level` by themselves. Any source promoted into a
claim must have:

- citation metadata,
- deterministic source hash,
- claim YAML,
- refinement document where applicable,
- CLI evidence receipts,
- human review when required.
