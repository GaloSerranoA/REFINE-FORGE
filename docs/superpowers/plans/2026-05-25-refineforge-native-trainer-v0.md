# Refine-Forge Native Trainer v0 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a real deterministic native training backend to `refineforge-trainer` so Refine-Forge can perform local proof-repair smoke training without delegating to HELYX, Axolotl, HuggingFace Trainer, or a custom subprocess.

**Architecture:** Keep `refineforge-trainer` as the CLI and report surface. Add a focused `native` module that owns in-process training, and teach `runner::run_once` to dispatch `backend.kind = refineforge_native` directly while preserving the existing run directory, progress, checkpoint, and report contracts.

**Tech Stack:** Rust 2021, serde, serde_json, serde_yaml, chrono, sha2, existing `refineforge-trainer` modules.

---

### Task 1: Native Backend Red Test

**Files:**
- Create: `crates/refineforge-trainer/tests/native_backend.rs`

- [ ] **Step 1: Write the failing test**

```rust
use refineforge_trainer::experiment::Experiment;
use refineforge_trainer::runner;
use serde_json::Value;
use std::fs;

#[test]
fn native_backend_runs_without_external_trainer_and_writes_checkpoint() {
    let temp = tempfile::tempdir().unwrap();
    let dataset = temp.path().join("proof-repair.jsonl");
    fs::write(
        &dataset,
        [
            r#"{"id":"a","prompt":"Diagnostic: unsolved goals\nSource: theorem a := by rfl","response":"{\"start_line\":0,\"start_char\":0,\"end_line\":0,\"end_char\":3,\"new_text\":\"simp\",\"rationale\":\"use simplifier\"}","split":"train"}"#,
            r#"{"id":"b","prompt":"Diagnostic: unknown identifier\nSource: theorem b := by exact missing","response":"{\"start_line\":0,\"start_char\":0,\"end_line\":0,\"end_char\":5,\"new_text\":\"exact h\",\"rationale\":\"reuse hypothesis\"}","split":"train"}"#,
            "",
        ]
        .join("\n"),
    )
    .unwrap();
    let config = temp.path().join("native.yaml");
    fs::write(
        &config,
        format!(
            r#"
id: native-v0-test
base_model:
  name: refineforge-native-linear-smoke
  source: native
dataset:
  path: {}
  format: jsonl
backend:
  kind: refineforge_native
hyperparameters:
  steps: 6
  learning_rate: 0.2
  feature_buckets: 32
  target_buckets: 8
checkpoint:
  save_steps: 3
  keep_last: 2
monitoring:
  metrics_to_track:
    - loss
    - accuracy
    - learning_rate
retry:
  max_attempts: 1
  backoff_seconds: 0
"#,
            dataset.display()
        ),
    )
    .unwrap();

    let experiment = Experiment::load(&config).unwrap();
    let runs_root = temp.path().join("runs");
    let outcome = runner::run_once(&runs_root, &experiment).unwrap();

    assert!(outcome.exit_status.success());
    assert_eq!(outcome.progress_records, 6);

    let run_dir = runs_root.join("native-v0-test");
    let progress = fs::read_to_string(run_dir.join("progress.jsonl")).unwrap();
    assert_eq!(progress.lines().count(), 6);

    let checkpoint = run_dir
        .join("checkpoints")
        .join("step-6")
        .join("native-checkpoint.json");
    assert!(checkpoint.exists());
    let checkpoint_json: Value = serde_json::from_str(&fs::read_to_string(checkpoint).unwrap()).unwrap();
    assert_eq!(checkpoint_json["backend_kind"], "refineforge_native");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p refineforge-trainer --test native_backend native_backend_runs_without_external_trainer_and_writes_checkpoint`

Expected: FAIL because `backend.kind = refineforge_native` is not supported yet.

### Task 2: Native Backend Implementation

**Files:**
- Create: `crates/refineforge-trainer/src/native.rs`
- Modify: `crates/refineforge-trainer/src/lib.rs`
- Modify: `crates/refineforge-trainer/src/main.rs`
- Modify: `crates/refineforge-trainer/src/experiment.rs`
- Modify: `crates/refineforge-trainer/src/runner.rs`

- [ ] **Step 1: Add native module and dispatch**

Implement `native::run(paths, exp) -> Result<NativeRunOutcome>` with deterministic dataset loading, hashed features, cross-entropy SGD, checkpoint writing, and progress JSONL writing.

- [ ] **Step 2: Teach validation about `refineforge_native`**

Add `refineforge_native` to the accepted backend kinds and update the error message.

- [ ] **Step 3: Dispatch in `runner::run_once`**

If `exp.backend.kind == "refineforge_native"`, call the native module instead of building a subprocess command.

- [ ] **Step 4: Run native backend test**

Run: `cargo test -p refineforge-trainer --test native_backend native_backend_runs_without_external_trainer_and_writes_checkpoint`

Expected: PASS.

### Task 3: Reports and Docs

**Files:**
- Modify: `crates/refineforge-trainer/src/report.rs`
- Modify: `README.md`
- Modify: `STRUCTURE.md`
- Modify: `training/README.md`
- Modify: `CHANGELOG.md`

- [ ] **Step 1: Verify existing report shape records native backend**

Add or update a test so native runs keep `compute_ledger.backend_kind = "refineforge_native"` through `report::build`.

- [ ] **Step 2: Replace absolute orchestration-only wording**

Docs must say Refine-Forge now has a native smoke trainer plus external backend orchestration. They must not claim production LLM training.

- [ ] **Step 3: Run docs and trainer gates**

Run: `cargo test -p refineforge-trainer`

Expected: PASS.

### Task 4: Final Rust Gate

**Files:**
- All touched Rust files.

- [ ] **Step 1: Format**

Run: `cargo fmt --all`

Expected: no formatting diff outside touched/generated files.

- [ ] **Step 2: Lint**

Run: `cargo clippy -p refineforge-trainer --all-targets -- -D warnings`

Expected: PASS with zero warnings.

- [ ] **Step 3: Test**

Run: `cargo test -p refineforge-trainer`

Expected: PASS.
