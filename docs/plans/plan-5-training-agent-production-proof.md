# Plan 5 - Training Agent Production Proof Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: use `superpowers:executing-plans` to implement this plan task-by-task. Do not use delegated agents unless the operator explicitly requests them in the current turn.

**Goal:** Raise the ML training agent from `measured-only` orchestration evidence to reviewer-approved model-quality production proof for HELYX proof-repair training.

**Architecture:** Keep training execution non-destructive by default and promote a model only through explicit dataset lineage, reproducible config, checkpoint manifest, benchmark evaluation, regression comparison, cost/compute ledger, and human approval. Memory and PDFs may seed configs or citations, but only benchmark and checkpoint evidence can raise the production-proof status.

**Tech Stack:** Rust, `refineforge-trainer`, HELYX train compatibility, JSONL datasets, checkpoint manifests, evaluation reports, `llms-from-scratch-rs` advisory fixtures, Refine-Forge memory JSONL.

---

## 1. Current Level

Current live level:

| Agent | Status | Trust |
|---|---|---|
| Train | `passed` | `measured-only` |

Meaning: dataset audit and trainer orchestration are alive. It does not prove training quality, checkpoint usefulness, benchmark improvement, or production readiness.

## 2. Production-Proof Target

The Training agent reaches enterprise production proof only when:

- Dataset audit passes with deterministic content hashes.
- Training config is reproducible and references immutable dataset hashes.
- Live training run produces checkpoint metadata and run report.
- Evaluation harness compares baseline and candidate on committed benchmarks.
- Promotion manifest records model id, checkpoint hash, metrics, and rollback path.
- Cost/compute ledger records backend, device, duration, and run budget.
- Human reviewer approves promotion.
- The agent keeps `trust_level = "measured-only"` until benchmark and human approval evidence exist; only then may it emit `human-reviewed`.

## 3. File Map

- Modify: `crates/refineforge-cli/src/agent/train.rs`
  - Add production-proof requirements and promotion evidence ingestion.
- Modify: `crates/refineforge-trainer/src/promotion.rs`
  - Include benchmark metrics and rollback path.
- Modify: `crates/refineforge-trainer/src/report.rs`
  - Include dataset hashes and compute ledger.
- Modify: `crates/refineforge-cli/tests/agent_cli.rs`
  - Add trust-boundary regression tests.
- Create: `training/evals/proof-repair-smoke.yaml`
  - Local benchmark config.
- Create: `docs/training/training-production-proof.md`
  - Human promotion checklist.
- Modify: `docs/agents/training-agent.md`

## 4. Work Breakdown

### Task 1 - Freeze Training Trust Boundary

**Files:**
- Test: `crates/refineforge-cli/tests/agent_cli.rs`
- Modify: `crates/refineforge-cli/src/agent/train.rs`

- [ ] **Step 1: Add regression test**

Add:

```rust
#[test]
fn agent_train_live_run_without_eval_cannot_claim_model_quality() {
    let td = tempfile::tempdir().unwrap();
    let out = td.path().join("train-prod");
    let output = run_refine(
        &["agent", "train", "--mode", "execute", "--target", "helyx"],
        &out,
    );
    assert_success(&output);
    let report = read_json(&out.join("train.json"));
    assert_eq!(report["trust_level"], "measured-only");
    assert_eq!(report["production_proof"]["status"], "blocked");
    assert!(report["production_proof"]["blockers"]
        .as_array()
        .unwrap()
        .iter()
        .any(|b| b.as_str().unwrap().contains("evaluation")));
}
```

- [ ] **Step 2: Add production-proof requirements**

In `train.rs`, emit these requirement ids:

```text
train.dataset_hashes
train.reproducible_config
train.live_checkpoint
train.benchmark_eval
train.baseline_regression
train.compute_ledger
train.promotion_manifest
train.human_promotion_approval
```

Dry runs must mark `live_checkpoint`, `benchmark_eval`, `promotion_manifest`, and `human_promotion_approval` as blocked.

- [ ] **Step 3: Run focused test**

Run:

```powershell
cargo test -p refineforge-cli --test agent_cli agent_train_live_run_without_eval_cannot_claim_model_quality
```

Expected: pass after production-proof envelope exists and Train blockers are emitted.

### Task 2 - Add Dataset Hash Evidence

**Files:**
- Modify: `crates/refineforge-trainer/src/report.rs`
- Modify: `crates/refineforge-cli/src/agent/train.rs`
- Test: `crates/refineforge-cli/tests/agent_cli.rs`

- [ ] **Step 1: Add report assertion**

Extend existing training tests to assert `training-data-audit.json` includes:

```json
{
  "dataset_sha256": "64 lowercase hex characters",
  "record_count": 1,
  "schema_version": "training-data-audit-v1"
}
```

- [ ] **Step 2: Implement hashing**

Hash the exact JSONL bytes before parsing. Store SHA-256 in the audit output and copy it into the agent production requirement evidence list.

- [ ] **Step 3: Run training agent check**

Run:

```powershell
cargo test -p refineforge-cli --test agent_cli agent_train_check_records_pass_fail_and_blocked_statuses
```

Expected: pass and audit JSON contains deterministic dataset hash.

### Task 3 - Add Evaluation Harness Contract

**Files:**
- Create: `training/evals/proof-repair-smoke.yaml`
- Modify: `crates/refineforge-trainer/src/promotion.rs`
- Test: `crates/refineforge-trainer/tests/end_to_end.rs`

- [ ] **Step 1: Create eval config**

Create:

```yaml
schema_version: proof-repair-eval-v1
id: proof-repair-smoke
dataset: training/data/mathlib-proof-repair-v1/anthropic-sft.jsonl
metrics:
  - exact_patch_acceptance
  - lean_recheck_pass_rate
minimums:
  exact_patch_acceptance: 0.0
  lean_recheck_pass_rate: 0.0
baseline:
  kind: current-local
  report: null
```

- [ ] **Step 2: Extend promotion manifest**

The promotion manifest must contain:

```json
{
  "evaluation": {
    "config": "training/evals/proof-repair-smoke.yaml",
    "metrics": {},
    "baseline_report": null,
    "candidate_report": "path"
  },
  "rollback": {
    "previous_model_id": null,
    "restore_command": "refine-train promote --rollback <model_id>"
  }
}
```

- [ ] **Step 3: Run trainer tests**

Run:

```powershell
cargo test -p refineforge-trainer
```

Expected: trainer tests pass and promotion manifest includes evaluation and rollback fields.

### Task 4 - Add Human Promotion Checklist

**Files:**
- Create: `docs/training/training-production-proof.md`
- Modify: `docs/agents/training-agent.md`

- [ ] **Step 1: Create checklist**

Create:

```markdown
# Training Production Proof Checklist

The Training agent may emit `human-reviewed` only when the model promotion record includes:

| Requirement | Evidence |
|---|---|
| Dataset lineage | dataset path, SHA-256, record count |
| Reproducible config | training YAML and config SHA-256 |
| Live run | run report and checkpoint metadata |
| Evaluation | benchmark report with baseline and candidate metrics |
| Regression guard | no required metric regresses below threshold |
| Compute ledger | backend, device, duration, cost/budget |
| Promotion manifest | model id, checkpoint hash, rollback command |
| Human approval | named reviewer, date, decision |
```

- [ ] **Step 2: Link from training agent doc**

Add:

```markdown
Model-quality production proof is governed by `docs/training/training-production-proof.md`.
```

## 5. Acceptance Gate

Run:

```powershell
cargo clippy -p refineforge-cli --all-targets -- -D warnings
cargo test -p refineforge-cli --test agent_cli agent_train_allow_expensive_still_cannot_claim_model_quality
cargo test -p refineforge-trainer
cargo run -p refineforge-cli --bin refine -- --root . agent train --mode execute --target helyx --out agent-reports/train-prod --json
```

Expected local final state: Training passes as `measured-only`, production proof remains blocked until live checkpoint, evaluation, promotion, and human approval evidence exist.
