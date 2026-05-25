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

fn run_production_proof(evidence_dir: &Path, out: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_refine"))
        .current_dir(workspace_root())
        .args([
            "--root",
            ".",
            "production-proof",
            "verify",
            "--target",
            "helyx",
            "--evidence-dir",
        ])
        .arg(evidence_dir)
        .arg("--out")
        .arg(out)
        .arg("--json")
        .output()
        .expect("run production-proof verify")
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
    let content = std::fs::read_to_string(path).unwrap();
    serde_json::from_str(&content).unwrap()
}

fn write_json(path: &Path, value: Value) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
}

fn write_text(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, content).unwrap();
}

fn approval(operator: &str, role: &str) -> Value {
    json!({
        "schema_version": "refineforge-human-approval-v1",
        "human_operator": operator,
        "role": role,
        "decision": "approved",
        "approved_at": "2026-05-24T00:00:00Z",
        "evidence_summary": format!("{role} production evidence reviewed")
    })
}

fn complete_manifest(source_kind: &str) -> Value {
    json!({
        "schema_version": "refineforge-production-proof-evidence-v1",
        "target": "helyx",
        "release": {
            "hosted_ci_url": "https://github.com/refine-forge/refine-forge/actions/runs/123456789",
            "oidc_issuer": "https://token.actions.githubusercontent.com",
            "signed_bundle_path": "release/cosign-verify.json",
            "sbom_path": "release/sbom.cyclonedx.json",
            "provenance_path": "release/provenance.intoto.json",
            "verifier_container_digest": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "nix_lock_path": "release/flake.lock",
            "nix_check_log_path": "release/nix-check.log",
            "architecture_matrix_path": "release/architecture-matrix.json",
            "approval_path": "approvals/release.json"
        },
        "training": {
            "checkpoint_path": "training/checkpoint.safetensors",
            "eval_report_path": "training/eval-report.json",
            "regression_report_path": "training/regression-report.json",
            "compute_ledger_path": "training/compute-ledger.json",
            "promotion_manifest_path": "training/promotion-manifest.json",
            "approval_path": "approvals/training.json"
        },
        "kernel": {
            "source_kind": source_kind,
            "source_path": "kernels/src/hvector_add.cu",
            "reference_path": "kernels/reference-output.json",
            "bitexact_report_path": "kernels/bitexact-report.json",
            "hardware_matrix_path": "kernels/hardware-matrix.json",
            "compiler_metadata_path": "kernels/compiler-metadata.json",
            "performance_baseline_path": "kernels/performance-baseline.json",
            "helyx_handoff_path": "kernels/helyx-handoff.json",
            "approval_path": "approvals/kernel.json"
        },
        "lean": {
            "claims_report_path": "lean/claims-report.json",
            "proof_inventory_path": "lean/proof-inventory.md",
            "refinement_links_path": "lean/refinement-links.json",
            "bundle_hashes_path": "lean/bundle-hashes.json",
            "approval_path": "approvals/lean.json"
        }
    })
}

fn write_complete_evidence(evidence_dir: &Path, manifest: Value) {
    write_json(&evidence_dir.join("evidence.json"), manifest);

    write_json(
        &evidence_dir.join("release/cosign-verify.json"),
        json!({"status": "passed"}),
    );
    write_json(
        &evidence_dir.join("release/sbom.cyclonedx.json"),
        json!({"bomFormat": "CycloneDX"}),
    );
    write_json(
        &evidence_dir.join("release/provenance.intoto.json"),
        json!({"predicateType": "https://slsa.dev/provenance/v1"}),
    );
    write_json(
        &evidence_dir.join("release/flake.lock"),
        json!({"nodes": {}}),
    );
    write_text(
        &evidence_dir.join("release/nix-check.log"),
        "nix flake check passed\n",
    );
    write_json(
        &evidence_dir.join("release/architecture-matrix.json"),
        json!({"status": "passed", "runners": [{"os": "ubuntu-latest", "arch": "x86_64"}]}),
    );

    write_text(
        &evidence_dir.join("training/checkpoint.safetensors"),
        "checkpoint bytes",
    );
    write_json(
        &evidence_dir.join("training/eval-report.json"),
        json!({"status": "passed", "metric": "proof_repair_smoke"}),
    );
    write_json(
        &evidence_dir.join("training/regression-report.json"),
        json!({"status": "passed"}),
    );
    write_json(
        &evidence_dir.join("training/compute-ledger.json"),
        json!({"status": "passed", "gpu_hours": 1.0}),
    );
    write_json(
        &evidence_dir.join("training/promotion-manifest.json"),
        json!({"status": "approved", "rollback": {"available": true}}),
    );

    write_text(
        &evidence_dir.join("kernels/src/hvector_add.cu"),
        "__global__ void hvector_add() {}\n",
    );
    write_json(
        &evidence_dir.join("kernels/reference-output.json"),
        json!({"status": "passed"}),
    );
    write_json(
        &evidence_dir.join("kernels/bitexact-report.json"),
        json!({"status": "passed"}),
    );
    write_json(
        &evidence_dir.join("kernels/hardware-matrix.json"),
        json!({"status": "passed", "gpus": [{"name": "NVIDIA A100", "driver": "555.42", "cuda": "12.5"}]}),
    );
    write_json(
        &evidence_dir.join("kernels/compiler-metadata.json"),
        json!({"nvcc": "12.5", "rustc": "1.87.0"}),
    );
    write_json(
        &evidence_dir.join("kernels/performance-baseline.json"),
        json!({"status": "passed", "throughput_gbps": 900.0}),
    );
    write_json(
        &evidence_dir.join("kernels/helyx-handoff.json"),
        json!({"status": "accepted"}),
    );

    write_json(
        &evidence_dir.join("lean/claims-report.json"),
        json!({
            "claims": [{
                "id": "HELYX-CLAIM-001",
                "scope": "implementation-linked",
                "refinement_doc": "docs/refinement/HELYX-CLAIM-001.md",
                "rust_symbols": ["helyx::kernel::hvector_add"],
                "lean_theorems": ["RefineForge.Helyx.hvector_add_refines"]
            }]
        }),
    );
    write_text(
        &evidence_dir.join("lean/proof-inventory.md"),
        "HELYX-CLAIM-001 implementation-linked\n",
    );
    write_json(
        &evidence_dir.join("lean/refinement-links.json"),
        json!({"status": "passed", "links": ["HELYX-CLAIM-001"]}),
    );
    write_json(
        &evidence_dir.join("lean/bundle-hashes.json"),
        json!({"status": "passed", "bundles": [{"sha256": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"}]}),
    );

    write_json(
        &evidence_dir.join("approvals/release.json"),
        approval("Galo Release Operator", "release"),
    );
    write_json(
        &evidence_dir.join("approvals/training.json"),
        approval("Galo Training Operator", "training"),
    );
    write_json(
        &evidence_dir.join("approvals/kernel.json"),
        approval("Galo Kernel Operator", "kernel"),
    );
    write_json(
        &evidence_dir.join("approvals/lean.json"),
        approval("Galo Lean Operator", "lean"),
    );
}

#[test]
fn production_proof_missing_manifest_writes_blocked_report() {
    let td = tempfile::tempdir().unwrap();
    let evidence = td.path().join("evidence");
    let out = td.path().join("out");
    std::fs::create_dir_all(&evidence).unwrap();

    let output = run_production_proof(&evidence, &out);

    assert_success(&output);
    let report = read_json(&out.join("summary.json"));
    assert_eq!(report["schema_version"], "agent-report-v1");
    assert_eq!(report["agent"], "run_all");
    assert_eq!(report["status"], "blocked");
    assert_eq!(report["trust_level"], "blocked");
    assert_eq!(report["production_proof"]["status"], "blocked");
    assert!(
        report["production_proof"]["blockers"]
            .as_array()
            .unwrap()
            .iter()
            .any(|b| b.as_str().unwrap().contains("evidence.json")),
        "missing manifest should block production proof"
    );
}

#[test]
fn production_proof_rejects_ai_or_placeholder_approvals() {
    let td = tempfile::tempdir().unwrap();
    let evidence = td.path().join("evidence");
    let out = td.path().join("out");
    write_complete_evidence(&evidence, complete_manifest("cuda"));
    write_json(
        &evidence.join("approvals/release.json"),
        approval("Codex GPT-5.5", "release"),
    );

    let output = run_production_proof(&evidence, &out);

    assert_success(&output);
    let report = read_json(&out.join("summary.json"));
    assert_eq!(report["status"], "blocked");
    assert_eq!(report["trust_level"], "blocked");
    assert!(
        report["production_proof"]["blockers"]
            .as_array()
            .unwrap()
            .iter()
            .any(|b| b.as_str().unwrap().contains("AI/automated approval")),
        "AI approval should never satisfy human-reviewed trust"
    );
}

#[test]
fn production_proof_stub_kernel_source_cannot_pass() {
    let td = tempfile::tempdir().unwrap();
    let evidence = td.path().join("evidence");
    let out = td.path().join("out");
    write_complete_evidence(&evidence, complete_manifest("stub"));

    let output = run_production_proof(&evidence, &out);

    assert_success(&output);
    let report = read_json(&out.join("summary.json"));
    assert_eq!(report["status"], "blocked");
    assert_eq!(report["trust_level"], "blocked");
    assert!(
        report["production_proof"]["blockers"]
            .as_array()
            .unwrap()
            .iter()
            .any(|b| b.as_str().unwrap().contains("kernel source kind is stub")),
        "stub source should block CUDA production proof"
    );
}

#[test]
fn production_proof_complete_fixture_reaches_human_reviewed() {
    let td = tempfile::tempdir().unwrap();
    let evidence = td.path().join("evidence");
    let out = td.path().join("out");
    write_complete_evidence(&evidence, complete_manifest("cuda"));

    let output = run_production_proof(&evidence, &out);

    assert_success(&output);
    let report = read_json(&out.join("summary.json"));
    assert_eq!(report["schema_version"], "agent-report-v1");
    assert_eq!(report["agent"], "run_all");
    assert_eq!(report["status"], "passed");
    assert_eq!(report["trust_level"], "human-reviewed");
    assert_eq!(report["production_proof"]["status"], "human-reviewed");
    assert!(report["production_proof"]["blockers"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(out.join("summary.md").exists());
    assert!(
        report["runtime"]["evidence_receipts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|receipt| receipt["subject"] == "artifact:evidence.json"),
        "manifest should be hashed as runtime evidence"
    );
}
