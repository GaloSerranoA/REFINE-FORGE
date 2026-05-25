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

fn write_policy(path: &Path, dev_loss_inverse_min_delta: f64) {
    write_text(
        path,
        &format!(
            r#"schema_version: refineforge-training-approval-policy-v1
allowed_operators:
  - Galo Training Operator
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
    min_delta: {dev_loss_inverse_min_delta}
  heldout_exact_patch_acceptance:
    min_delta: 0.0
  target_token_accuracy:
    min_delta: 0.0
"#
        ),
    );
}

fn write_training_evidence(evidence_dir: &Path) {
    let checkpoint_bytes = b"checkpoint bytes";
    let checkpoint_sha256 = hex_sha256(checkpoint_bytes);
    write_text(
        &evidence_dir.join("training/checkpoint.safetensors"),
        std::str::from_utf8(checkpoint_bytes).unwrap(),
    );
    write_json(
        &evidence_dir.join("training/eval-report.json"),
        json!({
            "status": "passed",
            "baseline": {"model_id": "baseline-proof-repair"},
            "candidate": {"model_id": "live-heldout-smoke-candidate-2026-05-25"},
            "metrics": {
                "dev_loss_inverse": 0.42,
                "heldout_exact_patch_acceptance": 1.0,
                "target_token_accuracy": 1.0
            },
            "quality_metrics": {
                "heldout_exact_patch_acceptance": 1.0,
                "target_token_accuracy": 1.0
            }
        }),
    );
    write_json(
        &evidence_dir.join("training/regression-report.json"),
        json!({
            "status": "passed",
            "baseline_report": "baseline-eval-report.json",
            "candidate_report": "eval-report.json",
            "metric_deltas": {
                "dev_loss_inverse": 0.00556,
                "heldout_exact_patch_acceptance": 0.0,
                "target_token_accuracy": 0.0
            }
        }),
    );
    write_json(
        &evidence_dir.join("training/compute-ledger.json"),
        json!({
            "status": "passed",
            "backend_kind": "refineforge_native_causal_lm",
            "device": "cpu/native",
            "duration_ms": 100,
            "budget": {"max_steps": 3},
            "gpu_hours": 0.0
        }),
    );
    write_json(
        &evidence_dir.join("training/conversion-manifest.json"),
        json!({
            "schema_version": "refineforge-training-conversion-manifest-v1",
            "status": "passed",
            "source_format": "refineforge-native",
            "target_format": "safetensors",
            "checkpoint_sha256": checkpoint_sha256,
            "artifacts": [{
                "path": "training/checkpoint.safetensors",
                "sha256": checkpoint_sha256
            }]
        }),
    );
    let conversion_sha256 =
        hex_sha256(&std::fs::read(evidence_dir.join("training/conversion-manifest.json")).unwrap());
    write_json(
        &evidence_dir.join("training/promotion-manifest.json"),
        json!({
            "status": "approved",
            "decision": "promote",
            "model_id": "live-heldout-smoke-candidate-2026-05-25",
            "checkpoint_sha256": checkpoint_sha256,
            "rollback": {"available": true, "path": "models/baseline"},
            "lineage": {
                "config_sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "train_metadata_sha256": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                "tokenizer_sha256": "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
                "checkpoint_shards": [{
                    "path": "training/checkpoint.safetensors",
                    "sha256": checkpoint_sha256
                }],
                "ema_policy": "none",
                "resume_source": "fresh",
                "epoch": 1
            },
            "conversion": {
                "manifest_path": "training/conversion-manifest.json",
                "manifest_sha256": conversion_sha256
            }
        }),
    );
}

fn requirement(id: &str, status: &str) -> Value {
    json!({
        "id": id,
        "description": id,
        "status": status,
        "evidence": []
    })
}

fn write_agent_report(path: &Path) {
    write_json(
        path,
        json!({
            "schema_version": "agent-report-v1",
            "agent": "train",
            "status": "passed",
            "trust_level": "measured-only",
            "summary": "Training evidence passed; human approval is pending.",
            "production_proof": {
                "status": "blocked",
                "requirements": [
                    requirement("train.dataset_hashes", "passed"),
                    requirement("train.reproducible_config", "passed"),
                    requirement("train.live_checkpoint", "passed"),
                    requirement("train.benchmark_eval", "passed"),
                    requirement("train.baseline_regression", "passed"),
                    requirement("train.compute_ledger", "passed"),
                    requirement("train.conversion_manifest", "passed"),
                    requirement("train.promotion_manifest", "passed"),
                    requirement("train.human_promotion_approval", "blocked")
                ],
                "blockers": ["human promotion approval is missing"]
            }
        }),
    );
}

fn fixture() -> (
    tempfile::TempDir,
    std::path::PathBuf,
    std::path::PathBuf,
    std::path::PathBuf,
) {
    let td = tempfile::tempdir().unwrap();
    let evidence_dir = td.path().join("evidence");
    let report = evidence_dir.join("train-agent-report.stdout.json");
    let policy = td.path().join("approval-policy.yaml");
    write_training_evidence(&evidence_dir);
    write_agent_report(&report);
    write_policy(&policy, 0.0);
    (td, evidence_dir, report, policy)
}

#[test]
fn training_approval_draft_writes_draft_but_not_final_approval() {
    let (_td, evidence_dir, report, policy) = fixture();
    let output = run_refine(&[
        "--root",
        ".",
        "training-approval",
        "draft",
        "--evidence-dir",
        evidence_dir.to_str().unwrap(),
        "--agent-report",
        report.to_str().unwrap(),
        "--policy",
        policy.to_str().unwrap(),
        "--operator",
        "Galo Training Operator",
        "--json",
    ]);

    assert_success(&output);
    let approvals = evidence_dir.join("approvals");
    assert!(approvals.join("training.draft.json").exists());
    assert!(approvals.join("training.review-request.json").exists());
    assert!(!approvals.join("training.json").exists());

    let draft = read_json(&approvals.join("training.draft.json"));
    assert_eq!(draft["decision"], "approved");
    assert_eq!(draft["human_operator"], "Galo Training Operator");
    let request = read_json(&approvals.join("training.review-request.json"));
    assert_eq!(request["status"], "pending-human-review");
    assert_eq!(
        request["candidate_model_id"],
        "live-heldout-smoke-candidate-2026-05-25"
    );
}

#[test]
fn training_approval_approve_requires_explicit_review_flag() {
    let (_td, evidence_dir, report, policy) = fixture();
    let output = run_refine(&[
        "--root",
        ".",
        "training-approval",
        "approve",
        "--evidence-dir",
        evidence_dir.to_str().unwrap(),
        "--agent-report",
        report.to_str().unwrap(),
        "--policy",
        policy.to_str().unwrap(),
        "--operator",
        "Galo Training Operator",
        "--json",
    ]);

    assert_failure(&output);
    assert!(!evidence_dir.join("approvals/training.json").exists());
}

#[test]
fn training_approval_approve_writes_final_approval_and_resolves_request() {
    let (_td, evidence_dir, report, policy) = fixture();
    let draft = run_refine(&[
        "--root",
        ".",
        "training-approval",
        "draft",
        "--evidence-dir",
        evidence_dir.to_str().unwrap(),
        "--agent-report",
        report.to_str().unwrap(),
        "--policy",
        policy.to_str().unwrap(),
        "--operator",
        "Galo Training Operator",
        "--json",
    ]);
    assert_success(&draft);

    let approval = run_refine(&[
        "--root",
        ".",
        "training-approval",
        "approve",
        "--evidence-dir",
        evidence_dir.to_str().unwrap(),
        "--agent-report",
        report.to_str().unwrap(),
        "--policy",
        policy.to_str().unwrap(),
        "--operator",
        "Galo Training Operator",
        "--i-reviewed-this-evidence",
        "--json",
    ]);

    assert_success(&approval);
    let final_approval = read_json(&evidence_dir.join("approvals/training.json"));
    assert_eq!(
        final_approval["schema_version"],
        "refineforge-human-approval-v1"
    );
    assert_eq!(final_approval["role"], "training");
    assert_eq!(final_approval["decision"], "approved");
    assert_eq!(final_approval["human_operator"], "Galo Training Operator");

    let request = read_json(&evidence_dir.join("approvals/training.review-request.json"));
    assert_eq!(request["status"], "approved");
    assert_eq!(request["resolved_by"], "Galo Training Operator");
}

#[test]
fn training_approval_rejects_policy_regression_failure() {
    let (_td, evidence_dir, report, policy) = fixture();
    write_policy(&policy, 1.0);

    let output = run_refine(&[
        "--root",
        ".",
        "training-approval",
        "draft",
        "--evidence-dir",
        evidence_dir.to_str().unwrap(),
        "--agent-report",
        report.to_str().unwrap(),
        "--policy",
        policy.to_str().unwrap(),
        "--operator",
        "Galo Training Operator",
        "--json",
    ]);

    assert_failure(&output);
    assert!(!evidence_dir.join("approvals/training.draft.json").exists());
    assert!(!evidence_dir.join("approvals/training.json").exists());
}
