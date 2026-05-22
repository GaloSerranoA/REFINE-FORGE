//! End-to-end gate test using the shipped stub scripts. POSIX-only
//! because the stubs are bash scripts (the .ps1 variants are
//! tested manually on Windows).

#![cfg(unix)]

use std::path::PathBuf;
use std::process::Command;

fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_refine-bitexact"))
}

#[test]
fn gate_passes_against_deterministic_stub() {
    let root = project_root();
    let runs = tempfile::tempdir().unwrap();
    let cfg = root.join("kernels/configs/example-deterministic.yaml");
    assert!(cfg.exists(), "{} must exist", cfg.display());
    let status = Command::new(bin())
        .arg("--runs-root")
        .arg(runs.path())
        .arg("run")
        .arg(&cfg)
        .current_dir(&root)
        .status()
        .expect("refine-bitexact must run");
    assert!(status.success(), "gate must PASS on deterministic stub");

    // Report must say Pass.
    let report_path = runs
        .path()
        .join("example-deterministic/bitexact-report.json");
    let report_text = std::fs::read_to_string(&report_path).expect("report.json must exist");
    let report: serde_json::Value = serde_json::from_str(&report_text).unwrap();
    assert_eq!(report["outcome"], "Pass");
    assert_eq!(report["unique_hashes"].as_array().unwrap().len(), 1);
}

#[test]
fn gate_fails_against_nondeterministic_stub() {
    let root = project_root();
    let runs = tempfile::tempdir().unwrap();
    let cfg = root.join("kernels/configs/example-nondeterministic.yaml");
    let status = Command::new(bin())
        .arg("--runs-root")
        .arg(runs.path())
        .arg("run")
        .arg(&cfg)
        .current_dir(&root)
        .status()
        .expect("refine-bitexact must run");
    assert!(
        !status.success(),
        "gate must FAIL on non-deterministic stub"
    );

    let report_path = runs
        .path()
        .join("example-nondeterministic/bitexact-report.json");
    let report_text = std::fs::read_to_string(&report_path).unwrap();
    let report: serde_json::Value = serde_json::from_str(&report_text).unwrap();
    assert_eq!(report["outcome"], "Fail");
    // Expect multiple unique hashes (likely 5, could be fewer if
    // $RANDOM collided — but never just 1).
    let unique = report["unique_hashes"].as_array().unwrap().len();
    assert!(
        unique > 1,
        "expected > 1 unique hashes for non-deterministic stub, got {unique}"
    );
}

#[test]
fn dry_run_does_not_execute_kernel() {
    let root = project_root();
    let runs = tempfile::tempdir().unwrap();
    let cfg = root.join("kernels/configs/example-nondeterministic.yaml");
    let status = Command::new(bin())
        .arg("--runs-root")
        .arg(runs.path())
        .arg("run")
        .arg(&cfg)
        .arg("--dry-run")
        .current_dir(&root)
        .status()
        .expect("dry-run must succeed");
    assert!(status.success(), "dry-run must exit 0 (no kernel executed)");
    // No report.json should exist after dry-run.
    let report_path = runs
        .path()
        .join("example-nondeterministic/bitexact-report.json");
    assert!(!report_path.exists(), "dry-run must not write a report");
}
