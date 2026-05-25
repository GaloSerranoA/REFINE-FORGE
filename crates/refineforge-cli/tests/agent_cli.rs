use serde_json::Value;
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

fn run_refine(args: &[&str], out: &Path) -> std::process::Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_refine"));
    cmd.current_dir(workspace_root())
        .args(["--root", "."])
        .args(args)
        .arg("--out")
        .arg(out)
        .arg("--json")
        .output()
        .expect("run refine agent command")
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

fn assert_enterprise_report(report: &Value, expected_agent: &str) {
    assert_eq!(report["liveness"]["state"], "alive");
    assert_eq!(report["liveness"]["agent"], expected_agent);
    assert!(
        report["capabilities"].as_array().unwrap().len() >= 3,
        "{expected_agent} report should declare enterprise capabilities"
    );
    assert!(
        !report["tool_checks"].as_array().unwrap().is_empty(),
        "{expected_agent} report should declare tool-gate checks"
    );
    assert_enterprise_runtime(report, expected_agent);
    assert_production_proof_envelope(report, expected_agent);
}

fn assert_enterprise_runtime(report: &Value, expected_agent: &str) {
    let runtime = &report["runtime"];
    assert_eq!(runtime["runtime_version"], "agent-runtime-v1");
    assert_eq!(runtime["authority"]["source_of_truth"], "cli_report");
    assert_eq!(runtime["authority"]["prompt_authority"], "advisory_only");
    assert_eq!(
        runtime["authority"]["memory_authority"],
        "non_authoritative"
    );
    assert!(
        runtime["authority"]["human_review_rule"]
            .as_str()
            .unwrap()
            .contains("human-reviewed"),
        "runtime authority must explain the human review boundary"
    );
    assert_eq!(runtime["agent"], expected_agent);
    assert_eq!(runtime["target"], report["target"]);
    assert_eq!(runtime["mode"], report["mode"]);
    assert!(
        runtime["action_intents"].as_array().unwrap().len() >= 2,
        "{expected_agent} runtime should expose role action intents"
    );
    assert!(
        !runtime["evidence_receipts"].as_array().unwrap().is_empty(),
        "{expected_agent} runtime should expose evidence receipts"
    );
    assert!(
        runtime["policy_decisions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|decision| decision["id"] == "no_prompt_trust_upgrade"),
        "{expected_agent} runtime should freeze the no-prompt trust-upgrade policy"
    );
    let receipts = runtime["evidence_receipts"].as_array().unwrap();
    let mut ids: Vec<_> = receipts
        .iter()
        .map(|receipt| receipt["id"].as_str().unwrap())
        .collect();
    let sorted = {
        let mut sorted = ids.clone();
        sorted.sort();
        sorted
    };
    assert_eq!(
        ids, sorted,
        "{expected_agent} receipts should be deterministic"
    );
    for receipt in receipts {
        let hash = receipt["sha256"].as_str().unwrap();
        assert_eq!(
            hash.len(),
            64,
            "{expected_agent} receipt hash should be a SHA-256 hex string: {receipt:?}"
        );
        assert!(
            hash.chars().all(|c| c.is_ascii_hexdigit()),
            "{expected_agent} receipt hash should be hex: {receipt:?}"
        );
    }
    ids.clear();
}

fn assert_production_proof_envelope(report: &Value, expected_agent: &str) {
    let proof = &report["production_proof"];
    assert_eq!(proof["agent"], expected_agent);
    assert!(proof["profile"]
        .as_str()
        .unwrap()
        .ends_with("-production-proof"));
    assert!(["blocked", "partial", "ready", "human-reviewed"]
        .contains(&proof["status"].as_str().unwrap()));
    assert_eq!(proof["trust_effect"], "bounded-by-evidence");
    assert!(
        proof["requirements"].as_array().unwrap().len() >= 4,
        "{expected_agent} production proof should declare concrete requirements"
    );
}

fn assert_warning_contains(report: &Value, needle: &str) {
    assert!(
        report["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|warning| warning.as_str().unwrap().contains(needle)),
        "expected warning containing {needle:?}, got {:?}",
        report["warnings"]
    );
}

fn assert_summary_contains(report: &Value, needle: &str) {
    let summary = report["summary"].as_str().unwrap();
    assert!(
        summary.contains(needle),
        "expected summary containing {needle:?}, got {summary:?}"
    );
}

fn receipt_hash(report: &Value, subject: &str) -> String {
    report["runtime"]["evidence_receipts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|receipt| receipt["subject"] == subject)
        .unwrap_or_else(|| panic!("missing evidence receipt for {subject}"))["sha256"]
        .as_str()
        .unwrap()
        .to_string()
}

fn write_stub(dir: &Path, name: &str, success: bool) -> std::path::PathBuf {
    #[cfg(windows)]
    let path = dir.join(format!("{name}.cmd"));
    #[cfg(not(windows))]
    let path = dir.join(name);

    #[cfg(windows)]
    let content = if success {
        "@echo off\r\necho stub ok\r\nexit /b 0\r\n"
    } else {
        "@echo off\r\necho stub failed 1>&2\r\nexit /b 7\r\n"
    };
    #[cfg(not(windows))]
    let content = if success {
        "#!/bin/sh\necho stub ok\nexit 0\n"
    } else {
        "#!/bin/sh\necho stub failed >&2\nexit 7\n"
    };

    std::fs::write(&path, content).unwrap();
    #[cfg(not(windows))]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&path, perms).unwrap();
    }
    path
}

fn write_kernel_output_stub(dir: &Path) -> std::path::PathBuf {
    #[cfg(windows)]
    let path = dir.join("refine-bitexact-output.cmd");
    #[cfg(not(windows))]
    let path = dir.join("refine-bitexact-output");

    #[cfg(windows)]
    let content = "@echo off\r\necho {\"status\":\"pass\"} > \"%5\"\r\nif errorlevel 1 exit /b 1\r\nexit /b 0\r\n";
    #[cfg(not(windows))]
    let content = "#!/bin/sh\necho '{\"status\":\"pass\"}' > \"$5\"\n";

    std::fs::write(&path, content).unwrap();
    #[cfg(not(windows))]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&path, perms).unwrap();
    }
    path
}

fn write_kernel_enterprise_stub(dir: &Path) -> std::path::PathBuf {
    #[cfg(windows)]
    let path = dir.join("refine-bitexact-enterprise.cmd");
    #[cfg(not(windows))]
    let path = dir.join("refine-bitexact-enterprise");

    #[cfg(windows)]
    let content = "@echo off\r\nif \"%1\"==\"lint\" (\r\n  echo {\"status\":\"pass\"} > \"%5\"\r\n  exit /b 0\r\n)\r\necho bitexact run ok\r\nexit /b 0\r\n";
    #[cfg(not(windows))]
    let content = "#!/bin/sh\nif [ \"$1\" = \"lint\" ]; then\n  echo '{\"status\":\"pass\"}' > \"$5\"\n  exit 0\nfi\necho bitexact run ok\nexit 0\n";

    std::fs::write(&path, content).unwrap();
    #[cfg(not(windows))]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&path, perms).unwrap();
    }
    path
}

fn write_json(path: &Path, value: Value) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
}

fn write_training_approval(path: &Path, operator: &str) {
    write_role_approval(path, "training", operator);
}

fn write_role_approval(path: &Path, role: &str, operator: &str) {
    write_json(
        path,
        serde_json::json!({
            "schema_version": "refineforge-human-approval-v1",
            "human_operator": operator,
            "role": role,
            "decision": "approved",
            "approved_at": "2026-05-25T00:00:00Z",
            "evidence_summary": format!("{role} production evidence reviewed")
        }),
    );
}

fn write_complete_training_evidence(dir: &Path) {
    std::fs::create_dir_all(dir.join("training")).unwrap();
    let checkpoint_bytes = b"checkpoint bytes";
    std::fs::write(
        dir.join("training/checkpoint.safetensors"),
        checkpoint_bytes,
    )
    .unwrap();
    let checkpoint_sha256 = hex_sha256(checkpoint_bytes);
    write_json(
        &dir.join("training/eval-report.json"),
        serde_json::json!({
            "schema_version": "refineforge-training-eval-report-v1",
            "status": "passed",
            "baseline": {"model_id": "baseline-proof-repair"},
            "candidate": {"model_id": "candidate-proof-repair"},
            "metrics": {"exact_patch_acceptance": 1.0, "lean_recheck_pass_rate": 1.0}
        }),
    );
    write_json(
        &dir.join("training/regression-report.json"),
        serde_json::json!({
            "schema_version": "refineforge-training-regression-report-v1",
            "status": "passed",
            "baseline_report": "baseline-eval-report.json",
            "candidate_report": "eval-report.json",
            "metric_deltas": {"exact_patch_acceptance": 0.0, "lean_recheck_pass_rate": 0.0}
        }),
    );
    write_json(
        &dir.join("training/compute-ledger.json"),
        serde_json::json!({
            "schema_version": "refineforge-training-compute-ledger-v1",
            "status": "passed",
            "backend_kind": "refineforge_native",
            "device": "cpu/native",
            "duration_ms": 123,
            "run_budget": "local-smoke"
        }),
    );
    write_json(
        &dir.join("training/conversion-manifest.json"),
        serde_json::json!({
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
    let conversion_manifest_sha256 =
        hex_sha256(&std::fs::read(dir.join("training/conversion-manifest.json")).unwrap());
    write_json(
        &dir.join("training/promotion-manifest.json"),
        serde_json::json!({
            "schema_version": "refineforge-training-promotion-manifest-v1",
            "status": "approved",
            "decision": "promote",
            "model_id": "proof-repair-local-v1",
            "checkpoint_sha256": checkpoint_sha256,
            "rollback": {"restore_command": "refine-train promote --rollback proof-repair-local-v1"},
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
                "manifest_sha256": conversion_manifest_sha256
            }
        }),
    );
    write_training_approval(
        &dir.join("approvals/training.json"),
        "Galo Training Operator",
    );
}

fn hex_sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

fn requirement_status<'a>(report: &'a Value, id: &str) -> &'a str {
    report["production_proof"]["requirements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|requirement| requirement["id"] == id)
        .unwrap_or_else(|| panic!("missing production requirement {id}"))["status"]
        .as_str()
        .unwrap()
}

#[test]
fn agent_lean_inspect_writes_report() {
    let td = tempfile::tempdir().unwrap();
    let out = td.path().join("lean-agent");
    let output = run_refine(
        &["agent", "lean", "--mode", "inspect", "--target", "helyx"],
        &out,
    );

    assert_success(&output);
    let report = read_json(&out.join("lean.json"));
    assert_eq!(report["schema_version"], "agent-report-v1");
    assert_eq!(report["agent"], "lean");
    assert_eq!(report["mode"], "inspect");
    assert_eq!(report["target"], "helyx");
    assert_eq!(report["status"], "passed");
    assert_eq!(report["trust_level"], "model-only");
    assert_enterprise_report(&report, "lean");
    assert!(out.join("lean.md").exists());
}

#[test]
fn agent_lean_check_keeps_model_only_scope_as_trust_floor() {
    let td = tempfile::tempdir().unwrap();
    let out = td.path().join("lean-check");
    let output = run_refine(
        &["agent", "lean", "--mode", "check", "--target", "helyx"],
        &out,
    );

    assert_success(&output);
    let report = read_json(&out.join("lean.json"));
    assert_eq!(report["status"], "passed");
    assert_eq!(report["trust_level"], "model-only");
    assert!(
        report["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|warning| warning.as_str().unwrap().contains("model-only")),
        "Lean check must explain why passing gates did not upgrade trust to model-linked"
    );
}

#[test]
fn agent_lean_model_only_claims_block_production_proof() {
    let td = tempfile::tempdir().unwrap();
    let out = td.path().join("lean-production");
    let output = run_refine(
        &["agent", "lean", "--mode", "check", "--target", "helyx"],
        &out,
    );

    assert_success(&output);
    let report = read_json(&out.join("lean.json"));
    assert_eq!(report["trust_level"], "model-only");
    assert_eq!(report["production_proof"]["status"], "blocked");
    assert!(
        report["production_proof"]["blockers"]
            .as_array()
            .unwrap()
            .iter()
            .any(|b| b.as_str().unwrap().contains("model-only")),
        "model-only claims must block implementation production proof"
    );
}

#[test]
fn agent_lean_evidence_dir_consumes_bundle_evidence_without_overriding_model_only_scope() {
    let td = tempfile::tempdir().unwrap();
    let evidence_dir = td.path().join("lean-evidence");
    write_json(
        &evidence_dir.join("lean/claims-report.json"),
        serde_json::json!({
            "claims": [{
                "id": "CRS-001",
                "scope": "implementation-linked",
                "refinement_doc": "docs/refinement/CRS-001.md",
                "rust_symbols": ["refineforge::crs::example"],
                "lean_theorems": ["Refineforge.CRS.example"]
            }]
        }),
    );
    std::fs::write(
        evidence_dir.join("lean/proof-inventory.md"),
        "CRS-001 implementation-linked\n",
    )
    .unwrap();
    write_json(
        &evidence_dir.join("lean/refinement-links.json"),
        serde_json::json!({"status": "passed", "links": ["CRS-001"]}),
    );
    write_json(
        &evidence_dir.join("lean/bundle-hashes.json"),
        serde_json::json!({
            "status": "passed",
            "bundles": [{
                "claim_id": "CRS-001",
                "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            }]
        }),
    );
    write_role_approval(
        &evidence_dir.join("approvals/lean.json"),
        "lean",
        "Galo Lean Operator",
    );
    let out = td.path().join("lean-evidence-out");

    let output = Command::new(env!("CARGO_BIN_EXE_refine"))
        .current_dir(workspace_root())
        .env("REFINEFORGE_LEAN_EVIDENCE_DIR", &evidence_dir)
        .args([
            "--root", ".", "agent", "lean", "--mode", "check", "--target", "helyx", "--out",
        ])
        .arg(&out)
        .arg("--json")
        .output()
        .expect("run lean evidence check");

    assert_success(&output);
    let report = read_json(&out.join("lean.json"));
    assert_eq!(requirement_status(&report, "lean.bundle_hashes"), "passed");
    assert_eq!(requirement_status(&report, "lean.human_review"), "passed");
    assert_eq!(report["production_proof"]["status"], "blocked");
    assert!(
        report["production_proof"]["blockers"]
            .as_array()
            .unwrap()
            .iter()
            .any(|b| b.as_str().unwrap().contains("model-only")),
        "evidence files must not override current model-only claim scopes"
    );
}

#[test]
fn agent_runtime_hashes_artifact_receipts_deterministically() {
    let td = tempfile::tempdir().unwrap();
    let first_out = td.path().join("lean-first");
    let first_output = run_refine(
        &["agent", "lean", "--mode", "inspect", "--target", "helyx"],
        &first_out,
    );
    assert_success(&first_output);
    let first = read_json(&first_out.join("lean.json"));

    let second_out = td.path().join("lean-second");
    let second_output = run_refine(
        &["agent", "lean", "--mode", "inspect", "--target", "helyx"],
        &second_out,
    );
    assert_success(&second_output);
    let second = read_json(&second_out.join("lean.json"));

    let subject = "artifact:docs/verification/proof-inventory.md";
    assert_eq!(
        receipt_hash(&first, subject),
        receipt_hash(&second, subject)
    );
}

#[test]
fn agent_devops_train_and_kernel_inspect_reports_are_truth_bounded() {
    for (name, trust_level) in [
        ("devops", "release-ready-local"),
        ("train", "measured-only"),
        ("kernel", "measured-only"),
    ] {
        let td = tempfile::tempdir().unwrap();
        let out = td.path().join(format!("{name}-agent"));
        let output = run_refine(
            &["agent", name, "--mode", "inspect", "--target", "helyx"],
            &out,
        );

        assert_success(&output);
        let report = read_json(&out.join(format!("{name}.json")));
        assert_eq!(report["schema_version"], "agent-report-v1");
        assert_eq!(report["agent"], name);
        assert_eq!(report["mode"], "inspect");
        assert_eq!(report["status"], "passed");
        assert_eq!(report["trust_level"], trust_level);
        assert_enterprise_report(&report, name);
        assert!(
            !report["artifacts"].as_array().unwrap().is_empty(),
            "{name} report should record at least one inspected artifact"
        );
        assert!(out.join(format!("{name}.md")).exists());
    }
}

#[test]
fn agent_devops_default_report_cannot_claim_ci_or_live_signing() {
    let td = tempfile::tempdir().unwrap();
    let out = td.path().join("devops-inspect");
    let output = run_refine(
        &["agent", "devops", "--mode", "inspect", "--target", "helyx"],
        &out,
    );

    assert_success(&output);
    let report = read_json(&out.join("devops.json"));
    assert_eq!(report["status"], "passed");
    assert_eq!(report["trust_level"], "release-ready-local");
    assert_summary_contains(&report, "Live hosted signing is not claimed.");
    assert_warning_contains(
        &report,
        "inspect mode does not run Docker, Nix, cosign, or hosted CI evidence",
    );

    let tool_checks = report["tool_checks"].as_array().unwrap();
    for tool in ["docker", "cosign"] {
        let check = tool_checks
            .iter()
            .find(|check| check["name"] == tool)
            .unwrap_or_else(|| panic!("missing tool check for {tool}"));
        assert_eq!(check["status"], "skipped");
    }
}

#[test]
fn agent_devops_local_report_cannot_claim_release_ready_ci() {
    let td = tempfile::tempdir().unwrap();
    let out = td.path().join("devops-local");
    let output = run_refine(
        &["agent", "devops", "--mode", "inspect", "--target", "0.2.2"],
        &out,
    );

    assert_success(&output);
    let report = read_json(&out.join("devops.json"));
    assert_eq!(report["trust_level"], "release-ready-local");
    assert_ne!(report["trust_level"], "release-ready-ci");
    assert_eq!(report["production_proof"]["status"], "blocked");
    assert!(
        report["production_proof"]["blockers"]
            .as_array()
            .unwrap()
            .iter()
            .any(|b| b.as_str().unwrap().contains("hosted CI")),
        "hosted CI evidence must block DevOps production proof"
    );
}

#[test]
fn agent_devops_rejects_fake_env_presence_as_production_evidence() {
    let td = tempfile::tempdir().unwrap();
    let out = td.path().join("devops-fake-env");
    let output = Command::new(env!("CARGO_BIN_EXE_refine"))
        .current_dir(workspace_root())
        .env("REFINEFORGE_HOSTED_CI_EVIDENCE", "missing-ci-url-or-file")
        .env("REFINEFORGE_SIGSTORE_EVIDENCE", "missing-sigstore-file")
        .env("REFINEFORGE_VERIFIER_CONTAINER_DIGEST", "not-a-digest")
        .env("REFINEFORGE_HUMAN_RELEASE_APPROVAL", "Codex GPT-5.5")
        .args([
            "--root", ".", "agent", "devops", "--mode", "inspect", "--target", "0.2.2", "--out",
        ])
        .arg(&out)
        .arg("--json")
        .output()
        .expect("run devops fake-env inspect");

    assert_success(&output);
    let report = read_json(&out.join("devops.json"));
    assert_ne!(
        requirement_status(&report, "devops.hosted_ci_artifacts"),
        "passed"
    );
    assert_ne!(
        requirement_status(&report, "devops.sigstore_oidc_signature"),
        "passed"
    );
    assert_ne!(
        requirement_status(&report, "devops.verifier_container_digest"),
        "passed"
    );
    assert_ne!(
        requirement_status(&report, "devops.human_release_approval"),
        "passed"
    );
}

#[test]
fn agent_run_all_inspect_writes_dashboard_and_role_reports() {
    let td = tempfile::tempdir().unwrap();
    let out = td.path().join("helyx-readiness");
    let output = run_refine(
        &["agent", "run-all", "--mode", "inspect", "--target", "helyx"],
        &out,
    );

    assert_success(&output);
    let summary = read_json(&out.join("summary.json"));
    assert_eq!(summary["schema_version"], "agent-report-v1");
    assert_eq!(summary["agent"], "run_all");
    assert_eq!(summary["target"], "helyx");
    assert_eq!(summary["status"], "passed");
    assert_enterprise_report(&summary, "run_all");
    assert!(out.join("summary.md").exists());
    for name in ["lean", "devops", "train", "kernel"] {
        let role_report_path = out.join(format!("{name}.json"));
        assert!(role_report_path.exists());
        assert_enterprise_report(&read_json(&role_report_path), name);
        assert!(out.join(format!("{name}.md")).exists());
    }
}

#[test]
fn agent_docs_and_schema_exist() {
    let root = workspace_root();
    for path in [
        "docs/agents/README.md",
        "docs/agents/lean-agent.md",
        "docs/agents/devops-agent.md",
        "docs/agents/training-agent.md",
        "docs/agents/kernel-agent.md",
        "docs/agents/runtime.md",
        "docs/agents/production-proof-evidence.md",
        "docs/agents/central-memory-integration.md",
        "docs/agents/knowledge-source-audit.md",
        "docs/verification/lean-production-proof-checklist.md",
        "docs/release/devops-production-proof.md",
        "docs/training/training-production-proof.md",
        "docs/kernels/kernel-production-proof.md",
        "kernels/hardware-matrix.example.json",
        "training/evals/proof-repair-smoke.yaml",
        "schemas/agent-report.schema.json",
        "schemas/production-proof-evidence.schema.json",
        "schemas/memory-record.schema.json",
    ] {
        assert!(root.join(path).exists(), "{path} should exist");
    }
}

#[test]
fn agent_train_check_records_pass_fail_and_blocked_statuses() {
    let td = tempfile::tempdir().unwrap();

    let pass = write_stub(td.path(), "refine-train-pass", true);
    let pass_out = td.path().join("train-pass");
    let pass_output = Command::new(env!("CARGO_BIN_EXE_refine"))
        .current_dir(workspace_root())
        .env("REFINEFORGE_REFINE_TRAIN_BIN", &pass)
        .args([
            "--root", ".", "agent", "train", "--mode", "check", "--target", "helyx", "--out",
        ])
        .arg(&pass_out)
        .arg("--json")
        .output()
        .expect("run train pass");
    assert_success(&pass_output);
    let report = read_json(&pass_out.join("train.json"));
    assert_eq!(report["status"], "passed");
    assert_eq!(report["commands"][0]["status"], "passed");
    let command = report["commands"][0]["command"].as_array().unwrap();
    assert!(
        command
            .iter()
            .any(|arg| arg == "training/data/mathlib-proof-repair-v1/anthropic-sft.jsonl"),
        "training check should audit the valid Mathlib SFT fixture, got {command:?}"
    );

    let fail = write_stub(td.path(), "refine-train-fail", false);
    let fail_out = td.path().join("train-fail");
    let fail_output = Command::new(env!("CARGO_BIN_EXE_refine"))
        .current_dir(workspace_root())
        .env("REFINEFORGE_REFINE_TRAIN_BIN", &fail)
        .args([
            "--root", ".", "agent", "train", "--mode", "check", "--target", "helyx", "--out",
        ])
        .arg(&fail_out)
        .arg("--json")
        .output()
        .expect("run train fail");
    assert!(!fail_output.status.success());
    let report = read_json(&fail_out.join("train.json"));
    assert_eq!(report["status"], "failed");
    assert_eq!(report["commands"][0]["status"], "failed");

    let blocked_out = td.path().join("train-blocked");
    let blocked_output = Command::new(env!("CARGO_BIN_EXE_refine"))
        .current_dir(workspace_root())
        .env(
            "REFINEFORGE_REFINE_TRAIN_BIN",
            td.path().join("missing-refine-train"),
        )
        .args([
            "--root", ".", "agent", "train", "--mode", "check", "--target", "helyx", "--out",
        ])
        .arg(&blocked_out)
        .arg("--json")
        .output()
        .expect("run train blocked");
    assert!(!blocked_output.status.success());
    let report = read_json(&blocked_out.join("train.json"));
    assert_eq!(report["status"], "blocked");
    assert_eq!(report["commands"][0]["status"], "blocked");
}

#[test]
fn agent_train_live_run_without_eval_cannot_claim_model_quality() {
    let td = tempfile::tempdir().unwrap();
    let out = td.path().join("train-prod");
    let output = run_refine(
        &["agent", "train", "--mode", "execute", "--target", "helyx"],
        &out,
    );

    assert_success(&output);
    let report = read_json(&out.join("train.json"));
    assert_eq!(report["trust_level"], "measured-only");
    assert_eq!(report["production_proof"]["status"], "blocked");
    let audit = read_json(&out.join("training-data-audit.json"));
    let dataset_sha256 = audit["dataset_sha256"].as_str().unwrap();
    assert_eq!(dataset_sha256.len(), 64);
    assert!(dataset_sha256
        .chars()
        .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)));
    assert_eq!(audit["record_count"], audit["total_rows"]);
    assert!(audit["record_count"].as_u64().unwrap() >= 1);
    assert_eq!(audit["schema_version"], "training-data-audit-v1");
    assert!(
        report["production_proof"]["blockers"]
            .as_array()
            .unwrap()
            .iter()
            .any(|b| b.as_str().unwrap().contains("evaluation")),
        "evaluation evidence must block Training production proof"
    );
}

#[test]
fn agent_kernel_check_creates_output_dir_before_lint_command() {
    let td = tempfile::tempdir().unwrap();
    let stub = write_kernel_output_stub(td.path());
    let out = td.path().join("nested").join("kernel-check");
    let output = Command::new(env!("CARGO_BIN_EXE_refine"))
        .current_dir(workspace_root())
        .env("REFINEFORGE_REFINE_BITEXACT_BIN", &stub)
        .args([
            "--root", ".", "agent", "kernel", "--mode", "check", "--target", "helyx", "--out",
        ])
        .arg(&out)
        .arg("--json")
        .output()
        .expect("run kernel check");

    assert_success(&output);
    assert!(out.join("bitexact-lint.json").exists());
    let report = read_json(&out.join("kernel.json"));
    assert_eq!(report["status"], "passed");
    assert_eq!(report["commands"][0]["status"], "passed");
}

#[test]
fn agent_train_execute_runs_data_audit_and_dry_run_training() {
    let td = tempfile::tempdir().unwrap();
    let stub = write_stub(td.path(), "refine-train-execute", true);
    let out = td.path().join("train-execute");
    let output = Command::new(env!("CARGO_BIN_EXE_refine"))
        .current_dir(workspace_root())
        .env("REFINEFORGE_REFINE_TRAIN_BIN", &stub)
        .args([
            "--root", ".", "agent", "train", "--mode", "execute", "--target", "helyx", "--out",
        ])
        .arg(&out)
        .arg("--json")
        .output()
        .expect("run train execute");

    assert_success(&output);
    let report = read_json(&out.join("train.json"));
    assert_eq!(report["status"], "passed");
    assert_eq!(report["trust_level"], "measured-only");
    assert_summary_contains(
        &report,
        "This proves training control-plane readiness, not model improvement.",
    );
    assert_warning_contains(&report, "training execute used --dry-run");
    let command_names: Vec<_> = report["commands"]
        .as_array()
        .unwrap()
        .iter()
        .map(|cmd| cmd["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        command_names,
        ["training-data-audit", "training-run-dry-run"]
    );
    let run_command = report["commands"][1]["command"].as_array().unwrap();
    assert!(
        run_command.iter().any(|arg| arg == "--dry-run"),
        "execute mode should default to a safe trainer dry-run without --allow-expensive"
    );
    assert!(
        report["artifacts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|artifact| artifact.as_str().unwrap().contains("training-runs")),
        "training execute should record its runs_root artifact"
    );
}

#[test]
fn agent_train_allow_expensive_still_cannot_claim_model_quality() {
    let td = tempfile::tempdir().unwrap();
    let stub = write_stub(td.path(), "refine-train-expensive", true);
    let out = td.path().join("train-expensive");
    let output = Command::new(env!("CARGO_BIN_EXE_refine"))
        .current_dir(workspace_root())
        .env("REFINEFORGE_REFINE_TRAIN_BIN", &stub)
        .args([
            "--root",
            ".",
            "agent",
            "train",
            "--mode",
            "execute",
            "--target",
            "helyx",
            "--allow-expensive",
            "--out",
        ])
        .arg(&out)
        .arg("--json")
        .output()
        .expect("run train allow-expensive execute");

    assert_success(&output);
    let report = read_json(&out.join("train.json"));
    assert_eq!(report["status"], "passed");
    assert_eq!(report["trust_level"], "measured-only");
    assert_summary_contains(&report, "Model quality still requires evaluation evidence.");

    let command_names: Vec<_> = report["commands"]
        .as_array()
        .unwrap()
        .iter()
        .map(|cmd| cmd["name"].as_str().unwrap())
        .collect();
    assert_eq!(command_names, ["training-data-audit", "training-run"]);
    let run_command = report["commands"][1]["command"].as_array().unwrap();
    assert!(
        !run_command.iter().any(|arg| arg == "--dry-run"),
        "--allow-expensive should request live trainer execution while keeping trust measured-only"
    );
}

#[test]
fn agent_train_rejects_fake_production_evidence_env_paths() {
    let td = tempfile::tempdir().unwrap();
    let stub = write_stub(td.path(), "refine-train-fake-evidence", true);
    let out = td.path().join("train-fake-evidence");
    let missing = td.path().join("missing");
    let output = Command::new(env!("CARGO_BIN_EXE_refine"))
        .current_dir(workspace_root())
        .env("REFINEFORGE_REFINE_TRAIN_BIN", &stub)
        .env(
            "REFINEFORGE_TRAINING_CHECKPOINT",
            missing.join("checkpoint.safetensors"),
        )
        .env(
            "REFINEFORGE_TRAINING_EVAL_REPORT",
            missing.join("eval-report.json"),
        )
        .env(
            "REFINEFORGE_TRAINING_REGRESSION_REPORT",
            missing.join("regression-report.json"),
        )
        .env(
            "REFINEFORGE_TRAINING_COMPUTE_LEDGER",
            missing.join("compute-ledger.json"),
        )
        .env(
            "REFINEFORGE_TRAINING_PROMOTION_MANIFEST",
            missing.join("promotion-manifest.json"),
        )
        .env(
            "REFINEFORGE_TRAINING_HUMAN_APPROVAL",
            missing.join("approval.json"),
        )
        .env(
            "REFINEFORGE_TRAINING_CONVERSION_MANIFEST",
            missing.join("conversion-manifest.json"),
        )
        .args([
            "--root",
            ".",
            "agent",
            "train",
            "--mode",
            "execute",
            "--target",
            "helyx",
            "--allow-expensive",
            "--out",
        ])
        .arg(&out)
        .arg("--json")
        .output()
        .expect("run train fake-evidence execute");

    assert_success(&output);
    let report = read_json(&out.join("train.json"));
    assert_eq!(report["status"], "passed");
    assert_eq!(report["trust_level"], "measured-only");
    assert_eq!(report["production_proof"]["status"], "blocked");
    assert!(
        report["production_proof"]["blockers"]
            .as_array()
            .unwrap()
            .iter()
            .any(|b| b.as_str().unwrap().contains("evaluation evidence")),
        "missing eval file must block Training production proof"
    );
}

#[test]
fn agent_train_rejects_loss_only_eval_evidence() {
    let td = tempfile::tempdir().unwrap();
    let stub = write_stub(td.path(), "refine-train-loss-only", true);
    let evidence_dir = td.path().join("evidence");
    write_complete_training_evidence(&evidence_dir);
    write_json(
        &evidence_dir.join("training/eval-report.json"),
        serde_json::json!({
            "schema_version": "refineforge-training-eval-report-v1",
            "status": "passed",
            "baseline": {"model_id": "baseline-proof-repair"},
            "candidate": {"model_id": "candidate-proof-repair"},
            "metrics": {"loss": 0.01}
        }),
    );
    let out = td.path().join("train-loss-only");

    let output = Command::new(env!("CARGO_BIN_EXE_refine"))
        .current_dir(workspace_root())
        .env("REFINEFORGE_REFINE_TRAIN_BIN", &stub)
        .env("REFINEFORGE_TRAINING_EVIDENCE_DIR", &evidence_dir)
        .args([
            "--root",
            ".",
            "agent",
            "train",
            "--mode",
            "execute",
            "--target",
            "helyx",
            "--allow-expensive",
            "--out",
        ])
        .arg(&out)
        .arg("--json")
        .output()
        .expect("run train loss-only execute");

    assert_success(&output);
    let report = read_json(&out.join("train.json"));
    assert_eq!(report["trust_level"], "measured-only");
    assert_eq!(report["production_proof"]["status"], "blocked");
    assert!(
        report["production_proof"]["blockers"]
            .as_array()
            .unwrap()
            .iter()
            .any(|b| b.as_str().unwrap().contains("loss-only")),
        "loss-only eval evidence must block Training production proof"
    );
}

#[test]
fn agent_train_complete_production_evidence_reaches_human_reviewed() {
    let td = tempfile::tempdir().unwrap();
    let stub = write_stub(td.path(), "refine-train-complete-evidence", true);
    let evidence_dir = td.path().join("evidence");
    write_complete_training_evidence(&evidence_dir);
    let out = td.path().join("train-complete-evidence");

    let output = Command::new(env!("CARGO_BIN_EXE_refine"))
        .current_dir(workspace_root())
        .env("REFINEFORGE_REFINE_TRAIN_BIN", &stub)
        .env("REFINEFORGE_TRAINING_EVIDENCE_DIR", &evidence_dir)
        .args([
            "--root",
            ".",
            "agent",
            "train",
            "--mode",
            "execute",
            "--target",
            "helyx",
            "--allow-expensive",
            "--out",
        ])
        .arg(&out)
        .arg("--json")
        .output()
        .expect("run train complete-evidence execute");

    assert_success(&output);
    let report = read_json(&out.join("train.json"));
    assert_eq!(report["status"], "passed");
    assert_eq!(report["trust_level"], "human-reviewed");
    assert_eq!(report["runtime"]["trust_ceiling"], "human-reviewed");
    assert_eq!(report["production_proof"]["status"], "human-reviewed");
    assert!(report["production_proof"]["blockers"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(
        report["runtime"]["evidence_receipts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|receipt| receipt["subject"]
                .as_str()
                .unwrap()
                .contains("training/eval-report.json")),
        "training eval evidence should be hashed into runtime receipts"
    );
}

#[test]
fn agent_train_complete_evidence_without_approval_names_human_review_blocker() {
    let td = tempfile::tempdir().unwrap();
    let stub = write_stub(td.path(), "refine-train-pending-approval", true);
    let evidence_dir = td.path().join("evidence");
    write_complete_training_evidence(&evidence_dir);
    std::fs::remove_file(evidence_dir.join("approvals/training.json")).unwrap();
    let out = td.path().join("train-pending-approval");

    let output = Command::new(env!("CARGO_BIN_EXE_refine"))
        .current_dir(workspace_root())
        .env("REFINEFORGE_REFINE_TRAIN_BIN", &stub)
        .env("REFINEFORGE_TRAINING_EVIDENCE_DIR", &evidence_dir)
        .args([
            "--root",
            ".",
            "agent",
            "train",
            "--mode",
            "execute",
            "--target",
            "helyx",
            "--allow-expensive",
            "--out",
        ])
        .arg(&out)
        .arg("--json")
        .output()
        .expect("run train pending approval execute");

    assert_success(&output);
    let report = read_json(&out.join("train.json"));
    assert_eq!(report["status"], "passed");
    assert_eq!(report["trust_level"], "measured-only");
    assert_eq!(report["production_proof"]["status"], "blocked");
    assert_summary_contains(&report, "human approval");
    assert!(
        !report["summary"]
            .as_str()
            .unwrap()
            .contains("requires evaluation evidence"),
        "summary should not say eval is missing when eval evidence passed"
    );
}

#[test]
fn agent_train_rejects_promotion_manifest_with_checkpoint_hash_mismatch() {
    let td = tempfile::tempdir().unwrap();
    let stub = write_stub(td.path(), "refine-train-hash-mismatch", true);
    let evidence_dir = td.path().join("evidence");
    write_complete_training_evidence(&evidence_dir);
    write_json(
        &evidence_dir.join("training/promotion-manifest.json"),
        serde_json::json!({
            "schema_version": "refineforge-training-promotion-manifest-v1",
            "status": "approved",
            "decision": "promote",
            "model_id": "proof-repair-local-v1",
            "checkpoint_sha256": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "rollback": {"restore_command": "refine-train promote --rollback proof-repair-local-v1"},
            "lineage": {
                "config_sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "train_metadata_sha256": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                "tokenizer_sha256": "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
                "checkpoint_shards": [{
                    "path": "training/checkpoint.safetensors",
                    "sha256": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
                }],
                "ema_policy": "none",
                "resume_source": "fresh",
                "epoch": 1
            },
            "conversion": {
                "manifest_path": "training/conversion-manifest.json",
                "manifest_sha256": "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd"
            }
        }),
    );
    let out = td.path().join("train-hash-mismatch");

    let output = Command::new(env!("CARGO_BIN_EXE_refine"))
        .current_dir(workspace_root())
        .env("REFINEFORGE_REFINE_TRAIN_BIN", &stub)
        .env("REFINEFORGE_TRAINING_EVIDENCE_DIR", &evidence_dir)
        .args([
            "--root",
            ".",
            "agent",
            "train",
            "--mode",
            "execute",
            "--target",
            "helyx",
            "--allow-expensive",
            "--out",
        ])
        .arg(&out)
        .arg("--json")
        .output()
        .expect("run train hash-mismatch execute");

    assert_success(&output);
    let report = read_json(&out.join("train.json"));
    assert_eq!(report["trust_level"], "measured-only");
    assert_eq!(report["production_proof"]["status"], "blocked");
    assert!(
        report["production_proof"]["blockers"]
            .as_array()
            .unwrap()
            .iter()
            .any(|b| b.as_str().unwrap().contains("checkpoint hash")),
        "promotion checkpoint hash mismatch must block Training production proof"
    );
}

#[test]
fn agent_kernel_execute_runs_lint_and_bitexact_gate() {
    let td = tempfile::tempdir().unwrap();
    let stub = write_kernel_enterprise_stub(td.path());
    let out = td.path().join("kernel-execute");
    let output = Command::new(env!("CARGO_BIN_EXE_refine"))
        .current_dir(workspace_root())
        .env("REFINEFORGE_REFINE_BITEXACT_BIN", &stub)
        .args([
            "--root", ".", "agent", "kernel", "--mode", "execute", "--target", "helyx", "--out",
        ])
        .arg(&out)
        .arg("--json")
        .output()
        .expect("run kernel execute");

    assert_success(&output);
    let report = read_json(&out.join("kernel.json"));
    assert_eq!(report["status"], "passed");
    assert_eq!(report["trust_level"], "measured-only");
    assert_summary_contains(&report, "not CUDA semantic correctness.");
    let command_names: Vec<_> = report["commands"]
        .as_array()
        .unwrap()
        .iter()
        .map(|cmd| cmd["name"].as_str().unwrap())
        .collect();
    assert_eq!(command_names, ["bitexact-lint", "bitexact-run"]);
    assert!(
        report["artifacts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|artifact| artifact.as_str().unwrap().contains("kernel-runs")),
        "kernel execute should record its runs_root artifact"
    );
}

#[test]
fn agent_kernel_stub_fixture_cannot_claim_cuda_correctness() {
    let td = tempfile::tempdir().unwrap();
    let out = td.path().join("kernel-prod");
    let output = run_refine(
        &["agent", "kernel", "--mode", "execute", "--target", "helyx"],
        &out,
    );

    assert_success(&output);
    let report = read_json(&out.join("kernel.json"));
    assert_eq!(report["trust_level"], "measured-only");
    assert_eq!(report["production_proof"]["status"], "blocked");
    assert!(
        report["production_proof"]["blockers"]
            .as_array()
            .unwrap()
            .iter()
            .any(|b| b.as_str().unwrap().contains("source.kind is stub")),
        "stub source must block Kernel CUDA production proof"
    );
}

#[test]
fn agent_kernel_rejects_fake_env_presence_as_production_evidence() {
    let td = tempfile::tempdir().unwrap();
    let out = td.path().join("kernel-fake-env");
    let output = Command::new(env!("CARGO_BIN_EXE_refine"))
        .current_dir(workspace_root())
        .env(
            "REFINEFORGE_KERNEL_COMPILER_METADATA",
            "missing-compiler.json",
        )
        .env(
            "REFINEFORGE_KERNEL_PERFORMANCE_BASELINE",
            "missing-performance.json",
        )
        .env("REFINEFORGE_KERNEL_HELYX_HANDOFF", "missing-handoff.json")
        .env("REFINEFORGE_KERNEL_HUMAN_APPROVAL", "Codex GPT-5.5")
        .args([
            "--root", ".", "agent", "kernel", "--mode", "inspect", "--target", "helyx", "--out",
        ])
        .arg(&out)
        .arg("--json")
        .output()
        .expect("run kernel fake-env inspect");

    assert_success(&output);
    let report = read_json(&out.join("kernel.json"));
    assert_ne!(
        requirement_status(&report, "kernel.compiler_runtime_metadata"),
        "passed"
    );
    assert_ne!(
        requirement_status(&report, "kernel.performance_baseline"),
        "passed"
    );
    assert_ne!(
        requirement_status(&report, "kernel.helyx_handoff"),
        "passed"
    );
    assert_ne!(
        requirement_status(&report, "kernel.human_kernel_approval"),
        "passed"
    );
}
