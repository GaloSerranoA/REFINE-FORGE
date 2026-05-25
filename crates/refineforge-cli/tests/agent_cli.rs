use serde_json::Value;
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
