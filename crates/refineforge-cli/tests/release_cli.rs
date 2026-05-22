use std::path::Path;
use std::process::Command;

fn workspace_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
}

#[test]
fn release_ready_dry_run_writes_evidence_files() {
    let td = tempfile::tempdir().unwrap();
    let evidence = td.path().join("evidence");
    let output = Command::new(env!("CARGO_BIN_EXE_refine"))
        .current_dir(workspace_root())
        .args([
            "--root",
            ".",
            "release",
            "ready",
            "--version",
            "0.2.2",
            "--dry-run",
            "--allow-dirty",
            "--skip-docker",
            "--skip-signature",
            "--evidence-dir",
        ])
        .arg(&evidence)
        .output()
        .expect("run refine release ready");

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(evidence.join("release-report.json").exists());
    assert!(evidence.join("release-report.md").exists());
    assert!(evidence.join("sbom.cyclonedx.json").exists());
    assert!(evidence.join("provenance.intoto.json").exists());
}

#[test]
fn release_scripts_delegate_to_refine_release_ready() {
    let root = workspace_root();
    let sh = std::fs::read_to_string(root.join("release/release.sh")).unwrap();
    let ps1 = std::fs::read_to_string(root.join("release/release.ps1")).unwrap();

    assert!(sh.contains("release ready --version"));
    assert!(sh.contains("--evidence-dir"));
    assert!(!sh.contains("cargo nextest run --workspace"));
    assert!(ps1.contains("release ready --version"));
    assert!(ps1.contains("--evidence-dir"));
    assert!(!ps1.contains("cargo nextest run --workspace"));
}

#[test]
fn ci_workflow_runs_release_evidence_and_container_smoke() {
    let workflow =
        std::fs::read_to_string(workspace_root().join(".github/workflows/ci.yml")).unwrap();

    assert!(workflow.contains("release ready --version"));
    assert!(workflow.contains("lint check-all"));
    assert!(workflow.contains("bundle export EXAMPLE-003"));
    assert!(workflow.contains("Dockerfile.verifier"));
    assert!(workflow.contains("release/evidence"));
    assert!(workflow.contains("Record runner architecture"));
    assert!(workflow.contains("runner.arch"));
    assert!(workflow.contains("rustc -Vv"));
    assert!(!workflow.contains("scan check-all || echo"));
}
