use serde_json::{json, Value};
use std::path::Path;
use std::process::Command;

fn workspace_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
}

fn run_enterprise_ready(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_refine"))
        .current_dir(workspace_root())
        .args(["--root", ".", "enterprise", "ready"])
        .args(args)
        .output()
        .expect("run refine enterprise ready")
}

fn assert_success(output: &std::process::Output) {
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn read_json(path: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn write_json(path: &Path, value: Value) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
}

#[test]
fn enterprise_ready_blocks_without_external_evidence() {
    let td = tempfile::tempdir().unwrap();
    let out = td.path().join("enterprise-ready");

    let output = run_enterprise_ready(&["--out", out.to_str().unwrap(), "--json"]);
    assert_success(&output);

    let report = read_json(&out.join("enterprise-readiness.json"));
    assert_eq!(
        report["schema_version"],
        "refineforge-enterprise-readiness-v1"
    );
    assert_eq!(report["status"], "blocked");
    assert_eq!(
        report["public_claim"],
        "enterprise_readiness_blocked_until_external_evidence_present"
    );
    assert_eq!(report["gates"].as_array().unwrap().len(), 6);
    assert!(report["blockers"]
        .as_array()
        .unwrap()
        .iter()
        .any(|blocker| blocker.as_str().unwrap().contains("remote_ci_proof")));
    assert!(out.join("enterprise-readiness.md").exists());
}

#[test]
fn enterprise_ready_passes_with_complete_evidence_pack() {
    let td = tempfile::tempdir().unwrap();
    let evidence = td.path().join("evidence");
    let out = td.path().join("enterprise-ready");

    write_json(
        &evidence.join("hosted-ci.json"),
        json!({
            "status": "passed",
            "url": "https://github.com/refine-forge/refine-forge/actions/runs/123456789",
            "commit": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        }),
    );
    write_json(
        &evidence.join("signed-release.json"),
        json!({
            "status": "passed",
            "signature": "sigstore",
            "bundle_sha256": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "identity": "https://github.com/refine-forge/refine-forge/.github/workflows/ci.yml@refs/heads/master"
        }),
    );
    write_json(
        &evidence.join("checkpoint-manifest.json"),
        json!({
            "status": "ready",
            "model_id": "proof-repair-local-v1",
            "checkpoint": {
                "path": "training/checkpoint.safetensors",
                "sha256": "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
            },
            "helyx_handoff": {
                "requires_hash_verification": true,
                "target": "helyx"
            }
        }),
    );
    write_json(
        &evidence.join("helyx-live.json"),
        json!({
            "status": "passed",
            "target": "helyx",
            "command": "helyx --version && refine production-proof verify"
        }),
    );
    write_json(
        &evidence.join("cleanup-report.json"),
        json!({
            "status": "passed",
            "scope": "older accumulated complexity",
            "reviewed_paths": ["crates/refineforge-cli", "docs", "training"]
        }),
    );

    let output = run_enterprise_ready(&[
        "--out",
        out.to_str().unwrap(),
        "--hosted-ci-evidence",
        evidence.join("hosted-ci.json").to_str().unwrap(),
        "--signed-release-evidence",
        evidence.join("signed-release.json").to_str().unwrap(),
        "--checkpoint-manifest",
        evidence.join("checkpoint-manifest.json").to_str().unwrap(),
        "--helyx-integration-evidence",
        evidence.join("helyx-live.json").to_str().unwrap(),
        "--cleanup-report",
        evidence.join("cleanup-report.json").to_str().unwrap(),
        "--json",
    ]);
    assert_success(&output);

    let report = read_json(&out.join("enterprise-readiness.json"));
    assert_eq!(report["status"], "ready");
    assert_eq!(
        report["public_claim"],
        "enterprise_readiness_evidence_complete_local_check"
    );
    assert!(report["blockers"].as_array().unwrap().is_empty());
    assert!(report["gates"]
        .as_array()
        .unwrap()
        .iter()
        .all(|gate| gate["status"] == "passed"));
}

#[test]
fn enterprise_ready_rejects_checkpoint_manifest_without_sha256() {
    let td = tempfile::tempdir().unwrap();
    let evidence = td.path().join("evidence");
    let out = td.path().join("enterprise-ready");

    write_json(
        &evidence.join("hosted-ci.json"),
        json!({"status": "passed"}),
    );
    write_json(
        &evidence.join("signed-release.json"),
        json!({
            "status": "passed",
            "signature": "sigstore",
            "bundle_sha256": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
        }),
    );
    write_json(
        &evidence.join("checkpoint-manifest.json"),
        json!({
            "status": "ready",
            "checkpoint": {
                "path": "training/checkpoint.safetensors"
            },
            "helyx_handoff": {
                "requires_hash_verification": true
            }
        }),
    );
    write_json(
        &evidence.join("helyx-live.json"),
        json!({"status": "passed"}),
    );
    write_json(
        &evidence.join("cleanup-report.json"),
        json!({"status": "passed"}),
    );

    let output = run_enterprise_ready(&[
        "--out",
        out.to_str().unwrap(),
        "--hosted-ci-evidence",
        evidence.join("hosted-ci.json").to_str().unwrap(),
        "--signed-release-evidence",
        evidence.join("signed-release.json").to_str().unwrap(),
        "--checkpoint-manifest",
        evidence.join("checkpoint-manifest.json").to_str().unwrap(),
        "--helyx-integration-evidence",
        evidence.join("helyx-live.json").to_str().unwrap(),
        "--cleanup-report",
        evidence.join("cleanup-report.json").to_str().unwrap(),
        "--json",
    ]);
    assert_success(&output);

    let report = read_json(&out.join("enterprise-readiness.json"));
    assert_eq!(report["status"], "blocked");
    let checkpoint_gate = report["gates"]
        .as_array()
        .unwrap()
        .iter()
        .find(|gate| gate["id"] == "accepted_model_checkpoint")
        .unwrap();
    assert_eq!(checkpoint_gate["status"], "blocked");
    assert!(checkpoint_gate["blocker"]
        .as_str()
        .unwrap()
        .contains("checkpoint sha256"));
}
