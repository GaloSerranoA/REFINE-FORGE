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
