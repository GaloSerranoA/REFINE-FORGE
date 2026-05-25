use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::process::Command;

fn workspace_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
}

fn run_refine(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_refine"))
        .current_dir(workspace_root())
        .args(args)
        .output()
        .expect("run refine")
}

fn assert_success(output: &std::process::Output) {
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn assert_failure(output: &std::process::Output) {
    assert!(
        !output.status.success(),
        "command unexpectedly succeeded\nstdout:\n{}\nstderr:\n{}",
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

fn hex_sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

fn write_role_policy(path: &Path) {
    write_text(
        path,
        r#"schema_version: refineforge-approval-policy-v1
allowed_operator_names:
  - Galo Kernel Operator
  - Galo Training Operator
roles:
  kernel:
    required_evidence:
      - kernels/src/hvector_add.cu
      - kernels/reference-output.json
      - kernels/bitexact-report.json
      - kernels/hardware-matrix.json
      - kernels/compiler-metadata.json
      - kernels/performance-baseline.json
      - kernels/helyx-handoff.json
  lean:
    evidence_required:
      - lean/refinement-doc.md
      - lean/rust-symbol-scan.json
      - lean/lean-proof-report.json
      - lean/exported-bundle-hashes.json
    candidate_files_required:
      - claim_file
      - refinement_doc
      - bundle_path
  training:
    allow_smoke_runs: true
    required_evidence:
      - training/checkpoint.safetensors
      - training/eval-report.json
      - training/regression-report.json
      - training/compute-ledger.json
      - training/conversion-manifest.json
      - training/promotion-manifest.json
    required_metrics:
      dev_loss_inverse:
        min_delta: 0.0
      heldout_exact_patch_acceptance:
        min_delta: 0.0
      target_token_accuracy:
        min_delta: 0.0
"#,
    );
}

fn write_kernel_evidence(evidence_dir: &Path) {
    write_text(
        &evidence_dir.join("kernels/src/hvector_add.cu"),
        "__global__ void hvector_add() {}\n",
    );
    write_json(
        &evidence_dir.join("kernels/reference-output.json"),
        json!({
            "schema_version": "refineforge-kernel-reference-v1",
            "status": "passed",
            "kernel_id": "helyx.hvector_add.cuda_v1",
            "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        }),
    );
    write_json(
        &evidence_dir.join("kernels/bitexact-report.json"),
        json!({
            "outcome": "Pass",
            "observed_sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "unique_hashes": ["aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"]
        }),
    );
    write_json(
        &evidence_dir.join("kernels/hardware-matrix.json"),
        json!({
            "runs": [{
                "status": "passed",
                "gpu_name": "NVIDIA GeForce RTX 3060 Laptop GPU",
                "gpu_arch": "8.6",
                "driver_version": "595.97",
                "cuda_toolkit": "13.2"
            }]
        }),
    );
    write_json(
        &evidence_dir.join("kernels/compiler-metadata.json"),
        json!({
            "status": "passed",
            "nvcc": "Cuda compilation tools, release 13.2, V13.2.78"
        }),
    );
    write_json(
        &evidence_dir.join("kernels/performance-baseline.json"),
        json!({
            "status": "passed",
            "bitexact_report_sha256": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
        }),
    );
    write_json(
        &evidence_dir.join("kernels/helyx-handoff.json"),
        json!({
            "status": "accepted",
            "kernel_id": "helyx.hvector_add.cuda_v1",
            "source": {"sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"},
            "bitexact_report": {"sha256": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"}
        }),
    );
    write_json(
        &evidence_dir.join("approvals/kernel.review-request.json"),
        json!({
            "schema_version": "refineforge-human-review-request-v1",
            "role": "kernel",
            "decision": "pending",
            "requested_at": "2026-05-25T00:00:00Z",
            "candidate": {
                "kernel_id": "helyx.hvector_add.cuda_v1",
                "evidence_dir": evidence_dir.display().to_string()
            },
            "review_required": [
                "Confirm kernel evidence."
            ],
            "non_approval_note": "This request is not an approval."
        }),
    );
}

fn write_training_evidence(evidence_dir: &Path) {
    let checkpoint = b"checkpoint bytes";
    let checkpoint_sha256 = hex_sha256(checkpoint);
    write_text(
        &evidence_dir.join("training/checkpoint.safetensors"),
        std::str::from_utf8(checkpoint).unwrap(),
    );
    write_json(
        &evidence_dir.join("training/eval-report.json"),
        json!({"status": "passed", "metrics": {"target_token_accuracy": 1.0}}),
    );
    write_json(
        &evidence_dir.join("training/regression-report.json"),
        json!({
            "status": "passed",
            "metric_deltas": {
                "dev_loss_inverse": 0.1,
                "heldout_exact_patch_acceptance": 0.0,
                "target_token_accuracy": 0.0
            }
        }),
    );
    write_json(
        &evidence_dir.join("training/compute-ledger.json"),
        json!({"status": "passed", "backend_kind": "refineforge_native"}),
    );
    write_json(
        &evidence_dir.join("training/conversion-manifest.json"),
        json!({"status": "passed", "checkpoint_sha256": checkpoint_sha256}),
    );
    let conversion_sha256 =
        hex_sha256(&std::fs::read(evidence_dir.join("training/conversion-manifest.json")).unwrap());
    write_json(
        &evidence_dir.join("training/promotion-manifest.json"),
        json!({
            "status": "approved",
            "decision": "promote",
            "model_id": "generic-training-smoke",
            "checkpoint_sha256": checkpoint_sha256,
            "conversion": {"manifest_sha256": conversion_sha256}
        }),
    );
    write_json(
        &evidence_dir.join("train-agent-report.stdout.json"),
        json!({
            "schema_version": "agent-report-v1",
            "agent": "train",
            "production_proof": {
                "requirements": [
                    {"id": "train.dataset_hashes", "status": "passed"},
                    {"id": "train.reproducible_config", "status": "passed"},
                    {"id": "train.live_checkpoint", "status": "passed"},
                    {"id": "train.benchmark_eval", "status": "passed"},
                    {"id": "train.baseline_regression", "status": "passed"},
                    {"id": "train.compute_ledger", "status": "passed"},
                    {"id": "train.conversion_manifest", "status": "passed"},
                    {"id": "train.promotion_manifest", "status": "passed"},
                    {"id": "train.human_promotion_approval", "status": "blocked"}
                ],
                "blockers": ["human promotion approval is missing"]
            }
        }),
    );
}

fn write_lean_evidence(evidence_dir: &Path, workspace: &Path) {
    let claim_file = workspace.join("claims/example-capability-revocation.yaml");
    let refinement_doc = workspace.join("docs/refinement/EXAMPLE-003.md");
    let bundle_path = workspace.join("artifacts/EXAMPLE-003");
    write_text(
        &claim_file,
        "claim_id: EXAMPLE-003\nscope: tutorial-production-shaped\n",
    );
    write_text(&refinement_doc, "# EXAMPLE-003\n\nRefinement doc.\n");
    std::fs::create_dir_all(&bundle_path).unwrap();
    write_json(
        &bundle_path.join("manifest.json"),
        json!({"claim_id": "EXAMPLE-003"}),
    );

    write_text(
        &evidence_dir.join("lean/refinement-doc.md"),
        "# EXAMPLE-003\n\nRefinement doc.\n",
    );
    write_json(
        &evidence_dir.join("lean/rust-symbol-scan.json"),
        json!({"status": "passed", "claim_id": "EXAMPLE-003"}),
    );
    write_json(
        &evidence_dir.join("lean/lean-proof-report.json"),
        json!({"status": "verified", "claim_id": "EXAMPLE-003"}),
    );
    write_json(
        &evidence_dir.join("lean/exported-bundle-hashes.json"),
        json!({
            "status": "passed",
            "bundles": [{
                "claim_id": "EXAMPLE-003",
                "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            }]
        }),
    );
    write_json(
        &evidence_dir.join("approvals/lean.review-request.json"),
        json!({
            "schema_version": "refineforge-human-review-request-v1",
            "role": "lean",
            "decision": "pending",
            "requested_at": "2026-05-25T00:00:00Z",
            "candidate": {
                "claim_id": "EXAMPLE-003",
                "claim_file": claim_file.display().to_string(),
                "refinement_doc": refinement_doc.display().to_string(),
                "bundle_path": bundle_path.display().to_string(),
                "evidence_dir": evidence_dir.display().to_string()
            },
            "review_required": [
                "Confirm Lean evidence."
            ],
            "non_approval_note": "This request is not an approval."
        }),
    );
}

#[test]
fn approval_kernel_draft_infers_role_from_review_request_without_final_approval() {
    let td = tempfile::tempdir().unwrap();
    let evidence_dir = td.path().join("kernel-evidence");
    let policy = td.path().join("approval-policy.yaml");
    write_kernel_evidence(&evidence_dir);
    write_role_policy(&policy);
    let request = evidence_dir.join("approvals/kernel.review-request.json");

    let output = run_refine(&[
        "--root",
        ".",
        "approval",
        "draft",
        "--review-request",
        request.to_str().unwrap(),
        "--policy",
        policy.to_str().unwrap(),
        "--operator",
        "Galo Kernel Operator",
        "--json",
    ]);

    assert_success(&output);
    assert!(evidence_dir.join("approvals/kernel.draft.json").exists());
    assert!(!evidence_dir.join("approvals/kernel.json").exists());
    let draft = read_json(&evidence_dir.join("approvals/kernel.draft.json"));
    assert_eq!(
        draft["schema_version"],
        "refineforge-human-approval-draft-v1"
    );
    assert_eq!(draft["role"], "kernel");
    assert_eq!(draft["decision"], "draft-ready");
    assert_eq!(draft["draft_operator"], "Galo Kernel Operator");
    assert!(draft.get("approved_at").is_none());
    assert!(draft.get("human_operator").is_none());
    let request = read_json(&request);
    assert_eq!(request["status"], "draft-ready");
}

#[test]
fn approval_kernel_approve_requires_explicit_human_review_flag() {
    let td = tempfile::tempdir().unwrap();
    let evidence_dir = td.path().join("kernel-evidence");
    let policy = td.path().join("approval-policy.yaml");
    write_kernel_evidence(&evidence_dir);
    write_role_policy(&policy);

    let output = run_refine(&[
        "--root",
        ".",
        "approval",
        "approve",
        "--role",
        "kernel",
        "--evidence-dir",
        evidence_dir.to_str().unwrap(),
        "--policy",
        policy.to_str().unwrap(),
        "--operator",
        "Galo Kernel Operator",
        "--json",
    ]);

    assert_failure(&output);
    assert!(!evidence_dir.join("approvals/kernel.json").exists());
}

#[test]
fn approval_kernel_approve_writes_final_approval_and_resolves_request() {
    let td = tempfile::tempdir().unwrap();
    let evidence_dir = td.path().join("kernel-evidence");
    let policy = td.path().join("approval-policy.yaml");
    write_kernel_evidence(&evidence_dir);
    write_role_policy(&policy);

    let output = run_refine(&[
        "--root",
        ".",
        "approval",
        "approve",
        "--role",
        "kernel",
        "--evidence-dir",
        evidence_dir.to_str().unwrap(),
        "--policy",
        policy.to_str().unwrap(),
        "--operator",
        "Galo Kernel Operator",
        "--i-reviewed-this-evidence",
        "--json",
    ]);

    assert_success(&output);
    let approval = read_json(&evidence_dir.join("approvals/kernel.json"));
    assert_eq!(approval["schema_version"], "refineforge-human-approval-v1");
    assert_eq!(approval["role"], "kernel");
    assert_eq!(approval["decision"], "approved");
    assert_eq!(approval["human_operator"], "Galo Kernel Operator");
    let request = read_json(&evidence_dir.join("approvals/kernel.review-request.json"));
    assert_eq!(request["status"], "approved");
    assert_eq!(request["resolved_by"], "Galo Kernel Operator");
}

#[test]
fn approval_approve_can_infer_role_from_draft_file() {
    let td = tempfile::tempdir().unwrap();
    let evidence_dir = td.path().join("kernel-evidence");
    let policy = td.path().join("approval-policy.yaml");
    write_kernel_evidence(&evidence_dir);
    write_role_policy(&policy);
    let draft = run_refine(&[
        "--root",
        ".",
        "approval",
        "draft",
        "--role",
        "kernel",
        "--evidence-dir",
        evidence_dir.to_str().unwrap(),
        "--policy",
        policy.to_str().unwrap(),
        "--operator",
        "Galo Kernel Operator",
        "--json",
    ]);
    assert_success(&draft);

    let approval = run_refine(&[
        "--root",
        ".",
        "approval",
        "approve",
        "--draft",
        evidence_dir
            .join("approvals/kernel.draft.json")
            .to_str()
            .unwrap(),
        "--evidence-dir",
        evidence_dir.to_str().unwrap(),
        "--policy",
        policy.to_str().unwrap(),
        "--operator",
        "Galo Kernel Operator",
        "--i-reviewed-this-evidence",
        "--json",
    ]);

    assert_success(&approval);
    assert!(evidence_dir.join("approvals/kernel.json").exists());
}

#[test]
fn approval_training_draft_uses_unified_role_policy() {
    let td = tempfile::tempdir().unwrap();
    let evidence_dir = td.path().join("training-evidence");
    let policy = td.path().join("approval-policy.yaml");
    write_training_evidence(&evidence_dir);
    write_role_policy(&policy);

    let output = run_refine(&[
        "--root",
        ".",
        "approval",
        "draft",
        "--role",
        "training",
        "--evidence-dir",
        evidence_dir.to_str().unwrap(),
        "--policy",
        policy.to_str().unwrap(),
        "--operator",
        "Galo Training Operator",
        "--json",
    ]);

    assert_success(&output);
    assert!(evidence_dir.join("approvals/training.draft.json").exists());
    assert!(!evidence_dir.join("approvals/training.json").exists());
    let request = read_json(&evidence_dir.join("approvals/training.review-request.json"));
    assert_eq!(request["status"], "draft-ready");
}

#[test]
fn approval_lean_draft_verifies_candidate_files_from_review_request() {
    let td = tempfile::tempdir().unwrap();
    let workspace = td.path().join("workspace");
    let evidence_dir = td.path().join("lean-evidence");
    let policy = td.path().join("approval-policy.yaml");
    write_lean_evidence(&evidence_dir, &workspace);
    write_role_policy(&policy);
    let request = evidence_dir.join("approvals/lean.review-request.json");

    let output = run_refine(&[
        "--root",
        ".",
        "approval",
        "draft",
        "--review-request",
        request.to_str().unwrap(),
        "--policy",
        policy.to_str().unwrap(),
        "--operator",
        "Galo Training Operator",
        "--json",
    ]);

    assert_success(&output);
    assert!(evidence_dir.join("approvals/lean.draft.json").exists());
    assert!(!evidence_dir.join("approvals/lean.json").exists());
    let draft = read_json(&evidence_dir.join("approvals/lean.draft.json"));
    assert_eq!(draft["role"], "lean");
    assert_eq!(draft["candidate_id"], "EXAMPLE-003");
    let request = read_json(&request);
    assert_eq!(request["status"], "draft-ready");
}
