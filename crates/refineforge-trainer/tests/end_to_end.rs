//! End-to-end smoke test: write a minimal experiment YAML pointing
//! at the stub trainer, run it via `refine-train`, verify the run
//! directory has the expected layout + a non-empty report.
//!
//! Skipped on Windows (the stub trainer is a bash script). On
//! Windows the equivalent test would invoke the .ps1 variant; for
//! v1 we cover the POSIX path in CI and keep the Windows case
//! manual.

#![cfg(unix)]

use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn project_root() -> PathBuf {
    // tests/ is two levels deep from the workspace root.
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    PathBuf::from(manifest_dir)
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn refine_train_bin() -> PathBuf {
    // CARGO_BIN_EXE_<name> is set by cargo for integration tests.
    PathBuf::from(env!("CARGO_BIN_EXE_refine-train"))
}

#[test]
fn stub_trainer_run_produces_progress_and_report() {
    let root = project_root();
    let runs_root = tempfile::tempdir().unwrap();
    let stub = root.join("training/scripts/stub-trainer.sh");
    assert!(stub.exists(), "stub trainer must exist at {}", stub.display());

    // Build a one-off experiment YAML using the stub trainer as
    // the backend. The {run_dir} token gets substituted by the runner.
    let exp_yaml = format!(
        r#"
id: trainer-e2e-stub
description: "end-to-end stub run"
base_model:
  name: stub-model
dataset:
  path: /dev/null
backend:
  kind: custom
  command: "bash {stub} {{run_dir}} --steps 6"
hyperparameters:
  learning_rate: 0.0002
monitoring:
  progress_format: huggingface
retry:
  max_attempts: 1
"#,
        stub = stub.display()
    );
    let exp_dir = tempfile::tempdir().unwrap();
    let exp_path = exp_dir.path().join("exp.yaml");
    fs::write(&exp_path, exp_yaml).unwrap();

    let status = Command::new(refine_train_bin())
        .arg("--runs-root")
        .arg(runs_root.path())
        .arg("run")
        .arg(&exp_path)
        .status()
        .expect("refine-train must be runnable");
    assert!(status.success(), "refine-train run must succeed");

    // Check expected files exist in the run dir.
    let run_dir = runs_root.path().join("trainer-e2e-stub");
    assert!(run_dir.join("config.yaml").exists(), "config.yaml missing");
    assert!(run_dir.join("train.log").exists(), "train.log missing");
    assert!(run_dir.join("progress.jsonl").exists(), "progress.jsonl missing");
    assert!(run_dir.join("report.json").exists(), "report.json missing");

    // Progress JSONL should have ~6 lines (one per stub step).
    let progress = fs::read_to_string(run_dir.join("progress.jsonl")).unwrap();
    let n_lines = progress.lines().filter(|l| !l.trim().is_empty()).count();
    assert_eq!(n_lines, 6, "expected 6 progress records, got {n_lines}");

    // Report JSON should have the right shape.
    let report_str = fs::read_to_string(run_dir.join("report.json")).unwrap();
    let report: serde_json::Value = serde_json::from_str(&report_str).unwrap();
    assert_eq!(report["final_outcome"], "success");
    assert_eq!(report["progress_record_count"], 6);
    assert!(report["metric_summary"]["loss"].is_object(),
            "loss metric should be summarised");

    // Checkpoints: stub writes one at step 5.
    let ckpts = fs::read_dir(run_dir.join("checkpoints")).unwrap();
    let ckpt_count = ckpts.filter_map(|e| e.ok()).count();
    assert!(ckpt_count >= 1, "expected at least 1 checkpoint, got {ckpt_count}");
}

#[test]
fn stub_trainer_failure_is_classified_and_recorded() {
    let root = project_root();
    let runs_root = tempfile::tempdir().unwrap();
    let stub = root.join("training/scripts/stub-trainer.sh");

    // Force failure at step 3 (OOM message in stderr).
    let exp_yaml = format!(
        r#"
id: trainer-e2e-fail
description: "stub run that fails"
base_model:
  name: stub-model
dataset:
  path: /dev/null
backend:
  kind: custom
  command: "bash {stub} {{run_dir}} --steps 10 --fail-at 3"
monitoring:
  progress_format: huggingface
retry:
  max_attempts: 1
  backoff_seconds: 0
"#,
        stub = stub.display()
    );
    let exp_dir = tempfile::tempdir().unwrap();
    let exp_path = exp_dir.path().join("exp.yaml");
    fs::write(&exp_path, exp_yaml).unwrap();

    let status = Command::new(refine_train_bin())
        .arg("--runs-root")
        .arg(runs_root.path())
        .arg("run")
        .arg(&exp_path)
        .status()
        .expect("refine-train binary exists");
    assert!(!status.success(), "should have exited non-zero on training failure");

    let run_dir = runs_root.path().join("trainer-e2e-fail");
    let failures = fs::read_to_string(run_dir.join("failures.jsonl")).unwrap();
    let first_line = failures.lines().next().expect("at least one failure record");
    let rec: serde_json::Value = serde_json::from_str(first_line).unwrap();
    // The stub prints "CUDA out of memory" to stderr → log → classify.
    assert_eq!(rec["category"], "OutOfMemory");
    assert_eq!(rec["action"], "Abort"); // No checkpoint exists at step 3 (saves every 5).
}
