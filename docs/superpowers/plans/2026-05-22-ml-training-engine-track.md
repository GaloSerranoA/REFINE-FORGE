# ML / Training Engine Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Complete the Refine-Forge Section 2 training lane with HELYX-compatible dataset audit, `helyx-train` backend resolution, checkpoint promotion, and runtime-manifest evidence.

**Architecture:** Keep Refine-Forge as the orchestration and verification layer. Add small focused modules to `refineforge-trainer`: `dataset` for JSONL audits and `promotion` for checkpoint/runtime manifest promotion. Extend the existing backend resolver with `helyx_train` instead of introducing a new trainer runtime.

**Tech Stack:** Rust 2021, Cargo workspace, clap, serde/serde_json/serde_yaml, existing `refineforge-trainer`, `refineforge-eval`, and `refineforge-strategies` crates.

---

## File Map

- Create `crates/refineforge-trainer/src/dataset.rs`: JSONL SFT audit model, validation, SHA-256 hashing, JSON writer.
- Create `crates/refineforge-trainer/src/promotion.rs`: checkpoint promotion report and `refineforge-local-finetune.json` writer.
- Modify `crates/refineforge-trainer/src/lib.rs`: export `dataset` and `promotion`.
- Modify `crates/refineforge-trainer/src/main.rs`: add `data audit` and `promote` CLI subcommands.
- Modify `crates/refineforge-trainer/src/experiment.rs`: accept `backend.kind = helyx_train`.
- Modify `crates/refineforge-trainer/src/runner.rs`: resolve default `helyx-train` command.
- Add `training/configs/helyx-mathlib-proof-repair-smoke.yaml`: HELYX-compatible dry-run config.
- Update `training/README.md`, `docs/repair-evaluation.md`, `README.md`, `ARCHITECTURE.md`, and `STRUCTURE.md`.

## Task 1: Dataset Audit

**Files:**
- Create: `crates/refineforge-trainer/src/dataset.rs`
- Modify: `crates/refineforge-trainer/src/lib.rs`
- Modify: `crates/refineforge-trainer/src/main.rs`

- [ ] **Step 1: Write failing tests**

Add tests in `dataset.rs` for:

```rust
#[test]
fn audit_accepts_valid_proof_repair_sft_jsonl() { /* two rows, train/eval split */ }

#[test]
fn audit_rejects_duplicate_ids() { /* same id twice -> error contains duplicate */ }

#[test]
fn audit_rejects_unparseable_patch_response() { /* response is not patch JSON */ }
```

- [ ] **Step 2: Run RED**

Run:

```powershell
cargo test -p refineforge-trainer dataset
```

Expected: compile fails because `dataset` does not exist.

- [ ] **Step 3: Implement dataset audit**

Implement:

```rust
pub struct DatasetAudit { path, sha256, total_rows, unique_ids, split_counts, valid_patch_rows, invalid_rows, issues }
pub struct AuditExpectations { rows: Option<usize>, splits: BTreeMap<String, usize> }
pub fn audit_jsonl(path: &Path, expectations: &AuditExpectations) -> Result<DatasetAudit>
pub fn write_audit_json(audit: &DatasetAudit, out: &Path) -> Result<()>
```

The function must fail on duplicate IDs, missing `prompt`, missing `response`, missing split, and patch JSON missing required fields.

- [ ] **Step 4: Add CLI**

Add:

```text
refine-train data audit <path> --expect-rows N --expect-split train=800 --output audit.json
```

Print a concise summary and write JSON when `--output` is provided.

- [ ] **Step 5: Run GREEN**

Run:

```powershell
cargo test -p refineforge-trainer dataset
cargo run -p refineforge-trainer -- data audit training/data/mathlib-proof-repair-v1/anthropic-sft.train.jsonl --expect-rows 800 --expect-split train=800
```

Expected: tests pass; CLI reports 800 rows, 800 unique IDs, train=800, and zero invalid rows.

- [ ] **Step 6: Commit**

```powershell
git add crates/refineforge-trainer/src/dataset.rs crates/refineforge-trainer/src/lib.rs crates/refineforge-trainer/src/main.rs
git commit -m "feat(trainer): add proof-repair dataset audit"
```

## Task 2: HELYX Backend Adapter

**Files:**
- Modify: `crates/refineforge-trainer/src/experiment.rs`
- Modify: `crates/refineforge-trainer/src/runner.rs`
- Add: `training/configs/helyx-mathlib-proof-repair-smoke.yaml`

- [ ] **Step 1: Write failing tests**

Add tests that `backend.kind = helyx_train` validates and resolves to a command containing:

```text
helyx-train run --config <config> --dataset <dataset> --output <run_dir> --checkpoint-dir <checkpoint_dir>
```

- [ ] **Step 2: Run RED**

Run:

```powershell
cargo test -p refineforge-trainer helyx
```

Expected: tests fail because `helyx_train` is unsupported.

- [ ] **Step 3: Implement adapter**

Accept `helyx_train` in validation and add default command resolution in `build_command`.

- [ ] **Step 4: Add sample config**

Create `training/configs/helyx-mathlib-proof-repair-smoke.yaml` with `backend.kind: helyx_train`, `config_file: training/configs/helyx-proof-repair-smoke.toml`, and dataset path `training/data/mathlib-proof-repair-v1/anthropic-sft.train.jsonl`.

- [ ] **Step 5: Run GREEN**

Run:

```powershell
cargo test -p refineforge-trainer helyx
cargo run -p refineforge-trainer -- run training/configs/helyx-mathlib-proof-repair-smoke.yaml --dry-run
```

Expected: tests pass; dry-run prints `helyx-train run`.

- [ ] **Step 6: Commit**

```powershell
git add crates/refineforge-trainer/src/experiment.rs crates/refineforge-trainer/src/runner.rs training/configs/helyx-mathlib-proof-repair-smoke.yaml
git commit -m "feat(trainer): add HELYX training backend adapter"
```

## Task 3: Promotion Manifest

**Files:**
- Create: `crates/refineforge-trainer/src/promotion.rs`
- Modify: `crates/refineforge-trainer/src/lib.rs`
- Modify: `crates/refineforge-trainer/src/main.rs`

- [ ] **Step 1: Write failing tests**

Add tests for:

```rust
#[test]
fn promotion_writes_local_finetune_manifest_for_successful_checkpoint() { /* success report + step-5 checkpoint */ }

#[test]
fn promotion_blocks_failed_training_report() { /* final_outcome=failure -> blocked */ }
```

- [ ] **Step 2: Run RED**

Run:

```powershell
cargo test -p refineforge-trainer promotion
```

Expected: compile fails because `promotion` does not exist.

- [ ] **Step 3: Implement promotion**

Implement:

```rust
pub struct PromotionOptions { run_dir, out_dir, model_id, command, producer, require_success, baseline_eval, candidate_eval, min_repair_rate_delta, dataset_audit }
pub struct PromotionReport { status, blockers, run_dir, checkpoint_dir, model_id, producer, eval_comparison }
pub fn promote(opts: &PromotionOptions) -> Result<PromotionReport>
```

Write `refineforge-local-finetune.json` and `promotion-report.json` under `out_dir`.

- [ ] **Step 4: Add CLI**

Add:

```text
refine-train promote <run_dir> --out-dir <dir> --model-id <id> --command <program> [--command-arg <arg>...] --producer helyx-train --require-success
```

- [ ] **Step 5: Run GREEN**

Run:

```powershell
cargo test -p refineforge-trainer promotion
cargo run -p refineforge-trainer -- promote training/runs/2026-05-20-lean-proof-repair-smoke-stub --out-dir training/runs/2026-05-20-lean-proof-repair-smoke-stub/promoted-local-finetune --model-id helyx-proof-repair-smoke --command powershell --command-arg -NoProfile --command-arg -Command --command-arg "{}" --producer helyx-train --require-success
```

Expected: command exits 0 and writes both promotion files.

- [ ] **Step 6: Remove generated promotion output**

```powershell
Remove-Item -Recurse -Force training/runs/2026-05-20-lean-proof-repair-smoke-stub/promoted-local-finetune
```

- [ ] **Step 7: Commit**

```powershell
git add crates/refineforge-trainer/src/promotion.rs crates/refineforge-trainer/src/lib.rs crates/refineforge-trainer/src/main.rs
git commit -m "feat(trainer): promote checkpoints to local-finetune runtime"
```

## Task 4: Documentation And Compatibility Truth

**Files:**
- Modify: `training/README.md`
- Modify: `docs/repair-evaluation.md`
- Modify: `README.md`
- Modify: `ARCHITECTURE.md`
- Modify: `STRUCTURE.md`

- [ ] **Step 1: Update docs**

Document:

- HELYX owns `helyx-autograd` and `helyx-train`.
- Refine-Forge owns dataset audit, training orchestration, promotion reports, and local-finetune manifests.
- Real GPU training remains externally blocked until HELYX/Axolotl runtime and GPU are available.

- [ ] **Step 2: Run doc truth searches**

Run:

```powershell
rg -n "Training pipeline: ❌|no harness|placeholder|TBD|doesn't exist|NEW:.*training|NEW:.*models" README.md ARCHITECTURE.md STRUCTURE.md docs/repair-evaluation.md training/README.md
```

Expected: no stale Section 2 status language remains.

- [ ] **Step 3: Commit**

```powershell
git add training/README.md docs/repair-evaluation.md README.md ARCHITECTURE.md STRUCTURE.md
git commit -m "docs(ml): document HELYX-compatible training lane"
```

## Task 5: Final Verification And Merge

**Files:**
- No planned edits.

- [ ] **Step 1: Run focused tests**

```powershell
cargo test -p refineforge-trainer
cargo test -p refineforge-strategies
cargo test -p refineforge-eval
```

- [ ] **Step 2: Run CLI smokes**

```powershell
cargo run -p refineforge-trainer -- data audit training/data/mathlib-proof-repair-v1/anthropic-sft.train.jsonl --expect-rows 800 --expect-split train=800
cargo run -p refineforge-trainer -- run training/configs/helyx-mathlib-proof-repair-smoke.yaml --dry-run
cargo run -p refineforge-trainer -- promote training/runs/2026-05-20-lean-proof-repair-smoke-stub --out-dir training/runs/2026-05-20-lean-proof-repair-smoke-stub/promoted-local-finetune --model-id helyx-proof-repair-smoke --command powershell --command-arg -NoProfile --command-arg -Command --command-arg "{}" --producer helyx-train --require-success
Remove-Item -Recurse -Force training/runs/2026-05-20-lean-proof-repair-smoke-stub/promoted-local-finetune
```

- [ ] **Step 3: Run release readiness**

```powershell
cargo run -p refineforge-cli --bin refine -- release ready --version 0.2.2 --allow-dirty --skip-docker --skip-signature --evidence-dir release/evidence/ml-training-local-0.2.2
Remove-Item -Recurse -Force release/evidence/ml-training-local-0.2.2
```

- [ ] **Step 4: Check status**

```powershell
git diff --check
git status --short --branch
```

- [ ] **Step 5: Merge**

Fast-forward merge `codex/ml-training-engine-track` back to `master`, re-run `cargo test -p refineforge-trainer`, re-run the dataset audit smoke, remove generated files, and delete the worktree/branch.
