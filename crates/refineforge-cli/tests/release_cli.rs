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

    let report: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(evidence.join("release-report.json")).unwrap(),
    )
    .unwrap();
    assert!(report["environment"]["runner_os"].is_string());
    assert!(report["environment"]["runner_arch"].is_string());
    assert!(report["environment"]["rustc_verbose_version"].is_string());
    assert_eq!(
        report["artifacts"]["sbom_sha256"].as_str().unwrap().len(),
        64
    );
    assert_eq!(
        report["artifacts"]["provenance_sha256"]
            .as_str()
            .unwrap()
            .len(),
        64
    );
    assert!(report["artifacts"]["verifier_container_digest"].is_null());
}

#[test]
fn release_offline_proof_records_local_signature_and_verifier_evidence() {
    let td = tempfile::tempdir().unwrap();
    let source = td.path().join("release-ready");
    let evidence = td.path().join("offline-proof");
    let signature = td.path().join("release.sig");
    let verifier_log = td.path().join("offline-verify.log");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(
        source.join("release-report.json"),
        r#"{"requested_version":"0.2.2","gates":[{"name":"docs-truth-audit","status":"passed","required":true}]}"#,
    )
    .unwrap();
    std::fs::write(source.join("release-report.md"), "# release\n").unwrap();
    std::fs::write(
        source.join("sbom.cyclonedx.json"),
        r#"{"bomFormat":"CycloneDX"}"#,
    )
    .unwrap();
    std::fs::write(
        source.join("provenance.intoto.json"),
        r#"{"_type":"https://in-toto.io/Statement/v1"}"#,
    )
    .unwrap();
    std::fs::write(&signature, "offline signature bytes\n").unwrap();
    std::fs::write(&verifier_log, "local offline verifier passed\n").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_refine"))
        .current_dir(workspace_root())
        .args([
            "--root",
            ".",
            "release",
            "offline-proof",
            "--version",
            "0.2.2",
            "--release-ready-dir",
        ])
        .arg(&source)
        .arg("--evidence-dir")
        .arg(&evidence)
        .arg("--signature-file")
        .arg(&signature)
        .arg("--key-fingerprint")
        .arg("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        .arg("--verifier-log")
        .arg(&verifier_log)
        .output()
        .expect("run refine release offline-proof");

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(evidence.join("release/release-report.json").exists());
    assert!(evidence.join("release/sbom.cyclonedx.json").exists());
    assert!(evidence.join("release/provenance.intoto.json").exists());

    let proof: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(evidence.join("release/offline-release-proof.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(proof["status"], "passed");
    assert_eq!(proof["profile"], "offline-local-release-proof");
    assert_eq!(proof["release_version"], "0.2.2");

    let signature: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(evidence.join("release/offline-signature.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(signature["status"], "passed");
    assert_eq!(signature["signature_mode"], "offline-local-key");
    assert_eq!(
        signature["key_fingerprint"],
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    );

    let verifier: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(evidence.join("release/offline-verifier.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(verifier["status"], "passed");
    assert_eq!(verifier["verifier"], "offline verifier log");

    let environment: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(evidence.join("release/local-environment.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(environment["status"], "passed");
    assert!(environment["os"].is_string());
    assert!(environment["arch"].is_string());
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

#[test]
fn ci_workflow_emits_devops_production_proof_evidence() {
    let workflow =
        std::fs::read_to_string(workspace_root().join(".github/workflows/ci.yml")).unwrap();
    let writer = std::fs::read_to_string(
        workspace_root().join("scripts/ci/write-release-production-evidence.sh"),
    )
    .unwrap();
    let evidence_surface = format!("{workflow}\n{writer}");

    assert!(workflow.contains("branches: [main, master]"));
    assert!(workflow.contains("scripts/ci/write-release-production-evidence.sh"));
    assert!(workflow.contains("production-proof/evidence/devops"));
    assert!(workflow.contains("refineforge-devops-production-evidence"));
    assert!(evidence_surface.contains("hosted-ci.json"));
    assert!(evidence_surface.contains("architecture-matrix.json"));
    assert!(evidence_surface.contains("cosign-verify.json"));
    assert!(evidence_surface.contains("nix-check.log"));
    assert!(evidence_surface.contains("verifier-container-digest.txt"));
    assert!(workflow.contains("id-token: write"));
}

#[test]
fn ci_workflow_nix_check_does_not_require_flakehub_or_preexisting_lock() {
    let workflow =
        std::fs::read_to_string(workspace_root().join(".github/workflows/ci.yml")).unwrap();
    let writer = std::fs::read_to_string(
        workspace_root().join("scripts/ci/write-release-production-evidence.sh"),
    )
    .unwrap();

    assert!(workflow.contains("DeterminateSystems/nix-installer-action"));
    assert!(
        !workflow.contains("magic-nix-cache-action"),
        "public CI must not require FlakeHub cache registration"
    );
    assert!(
        !workflow.contains("--no-update-lock-file"),
        "first public CI run must be able to generate flake.lock evidence"
    );
    assert!(
        writer.find("\"$@\"").unwrap() < writer.find("cp flake.lock").unwrap(),
        "nix-check evidence must copy flake.lock after nix has had a chance to generate it"
    );
    assert!(
        writer.contains("::error file=flake.nix,title=nix flake check"),
        "nix-check failures must publish a public diagnostic annotation"
    );
    assert!(
        writer.contains("nix-builder.log"),
        "nix-check failures must collect the failed builder log"
    );
}

#[test]
fn ci_workflow_nix_check_keeps_failed_build_directories_for_diagnostics() {
    let workflow =
        std::fs::read_to_string(workspace_root().join(".github/workflows/ci.yml")).unwrap();

    assert!(
        workflow.contains("nix flake check --print-build-logs --keep-failed"),
        "Nix CI must keep failed build directories so hidden cargo/test logs can be surfaced"
    );
}

#[test]
fn ci_script_nix_check_captures_kept_build_logs() {
    let writer = std::fs::read_to_string(
        workspace_root().join("scripts/ci/write-release-production-evidence.sh"),
    )
    .unwrap();

    assert!(
        writer.contains("nix-kept-build-logs.txt"),
        "Nix failure diagnostics must persist logs copied from kept build directories"
    );
    assert!(
        writer.contains("/tmp/nix-build-"),
        "Nix failure diagnostics must detect --keep-failed build directories"
    );
    assert!(
        writer.contains("find \"$kept_dir\""),
        "Nix failure diagnostics must inspect kept build directories for cargo/test logs"
    );
}

#[test]
fn ci_script_nix_check_annotates_primary_and_builder_logs() {
    let writer = std::fs::read_to_string(
        workspace_root().join("scripts/ci/write-release-production-evidence.sh"),
    )
    .unwrap();

    assert!(
        writer.contains("nix-failure-diagnostics.log"),
        "Nix failure diagnostics must combine useful snippets instead of choosing one opaque log"
    );
    assert!(
        writer.contains("append_nix_diagnostics \"$out_dir/nix-check.log\""),
        "Nix failure annotations must include the primary nix-check log"
    );
    assert!(
        writer.contains("append_nix_diagnostics \"$out_dir/nix-builder.log\""),
        "Nix failure annotations must include the builder log when available"
    );
}

#[test]
fn ci_workflow_uploads_nix_evidence_even_when_nix_fails() {
    let workflow =
        std::fs::read_to_string(workspace_root().join(".github/workflows/ci.yml")).unwrap();
    let workflow = workflow.replace("\r\n", "\n");

    assert!(
        workflow.contains("Upload Nix evidence\n        if: always()"),
        "Nix evidence artifacts must upload even when nix flake check fails"
    );
}

#[test]
fn ci_script_nix_check_prefers_failed_derivations_for_builder_logs() {
    let writer = std::fs::read_to_string(
        workspace_root().join("scripts/ci/write-release-production-evidence.sh"),
    )
    .unwrap();

    assert!(
        writer.contains("failed_drvs"),
        "Nix builder log collection must parse explicitly failed derivations"
    );
    assert!(
        writer.contains("error: builder for '/nix/store/"),
        "Nix builder log collection must read the canonical failed-builder error line"
    );
    assert!(
        writer.contains("drv_paths=\"${failed_drvs:-$all_drvs}\""),
        "Nix builder log collection must prefer failed derivations before falling back to all derivations"
    );
}

#[test]
fn ci_script_nix_check_prioritizes_failure_summary_annotations() {
    let writer = std::fs::read_to_string(
        workspace_root().join("scripts/ci/write-release-production-evidence.sh"),
    )
    .unwrap();

    assert!(
        writer.contains("nix-failure-summary.log"),
        "Nix failure diagnostics must produce a compact summary file"
    );
    assert!(
        writer.contains("append_nix_summary"),
        "Nix failure diagnostics must build summary lines separately from long tails"
    );
    assert!(
        writer.contains("tail -n 80 \"$out_dir/nix-failure-summary.log\""),
        "GitHub annotations must prefer the compact summary over a broad builder tail"
    );
}

#[test]
fn nix_flake_source_includes_cargo_lock() {
    let root = workspace_root();
    let gitignore = std::fs::read_to_string(root.join(".gitignore")).unwrap();

    assert!(
        root.join("Cargo.lock").exists(),
        "Nix crane builds require a workspace Cargo.lock"
    );
    assert!(
        !gitignore.lines().any(|line| line.trim() == "Cargo.lock"),
        "Cargo.lock must be committed for Nix crane builds, not ignored"
    );
}

#[test]
fn nix_flake_source_includes_cargo_test_support_files() {
    let flake = std::fs::read_to_string(workspace_root().join("flake.nix")).unwrap();

    for prefix in [
        "pkgs.lib.hasPrefix \"docs/\" relPath",
        ".github/workflows/",
        "scripts/ci/",
        "release/",
        "kernels/",
        "training/",
        "containers/",
        "schemas/",
    ] {
        assert!(
            flake.contains(prefix),
            "Nix cargoTest source filter must keep {prefix} for integration tests"
        );
    }
    for file in [".gitignore", "flake.nix", "SECURITY.md"] {
        assert!(
            flake.contains(file),
            "Nix cargoTest source filter must keep root file {file}"
        );
    }
}

#[test]
fn nix_flake_does_not_duplicate_cargo_locked_arg() {
    let flake = std::fs::read_to_string(workspace_root().join("flake.nix")).unwrap();

    assert!(
        flake.contains("cargoTestExtraArgs = \"--workspace\";"),
        "crane already injects --locked into cargo test"
    );
    assert!(
        !flake.contains("cargoTestExtraArgs = \"--workspace --locked\";"),
        "duplicating --locked makes cargo test fail under Nix"
    );
}

#[test]
fn nix_flake_cargo_test_provides_required_subprocess_tools() {
    let flake = std::fs::read_to_string(workspace_root().join("flake.nix")).unwrap();

    assert!(
        flake.contains("nativeBuildInputs = [ pkgs.git pkgs.lean.lean-all pythonForSmoke ];"),
        "Nix cargoTest must provide git, Lean/Lake, and python for agent subprocess integration tests"
    );
    assert!(
        flake.contains("pythonForSmoke = pkgs.writeShellScriptBin \"python\""),
        "Nix cargoTest must expose a python command backed by pkgs.python3"
    );
}

#[test]
fn ci_workflow_publishes_posix_cargo_test_failure_tail() {
    let workflow =
        std::fs::read_to_string(workspace_root().join(".github/workflows/ci.yml")).unwrap();

    assert!(workflow.contains("Run unit tests (POSIX)"));
    assert!(workflow.contains("cargo-test-release.log"));
    assert!(workflow.contains("cargo-test-failure-summary.log"));
    assert!(workflow.contains("grep -E -- '---- |panicked at| FAILED"));
    assert!(workflow.contains("::error file=Cargo.toml,title=cargo test --release"));
}

#[test]
fn ci_workflow_publishes_windows_cargo_test_failure_tail() {
    let workflow =
        std::fs::read_to_string(workspace_root().join(".github/workflows/ci.yml")).unwrap();

    assert!(workflow.contains("Run unit tests (Windows)"));
    assert!(workflow.contains("cargo-test-release.log"));
    assert!(workflow.contains("cargo-test-failure-summary.log"));
    assert!(workflow.contains("Select-String -Path cargo-test-release.log"));
    assert!(workflow.contains("-CaseSensitive"));
    assert!(workflow.contains("::error file=Cargo.toml,title=cargo test --release"));
}

#[test]
fn ci_workflow_publishes_verifier_container_failure_tail() {
    let workflow =
        std::fs::read_to_string(workspace_root().join(".github/workflows/ci.yml")).unwrap();

    assert!(workflow.contains("verifier-container-build.log"));
    assert!(workflow.contains("verifier-container-smoke.log"));
    assert!(workflow
        .contains("::error file=containers/Dockerfile.verifier,title=verifier container smoke"));
}

#[test]
fn verifier_container_builder_tracks_dependency_msrv() {
    let dockerfile =
        std::fs::read_to_string(workspace_root().join("containers/Dockerfile.verifier")).unwrap();

    assert!(
        dockerfile.contains("FROM rust:1.87-bookworm AS builder"),
        "verifier container must use a Rust builder new enough for locked dependencies"
    );
    assert!(
        !dockerfile.contains("rust:1.83-bookworm"),
        "Rust 1.83 cannot build the current locked dependency graph"
    );
}
