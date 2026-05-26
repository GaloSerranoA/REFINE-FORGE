use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn refine_train_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_refine-train"))
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
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

fn hex_sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn yaml_path(path: &Path) -> String {
    path.display().to_string().replace('\\', "/")
}

#[test]
fn hrm_text_dry_run_builds_runtime_command_with_space_safe_paths() {
    let temp = tempfile::tempdir().unwrap();
    let source_repo = temp.path().join("HRM Text Source");
    fs::create_dir_all(&source_repo).unwrap();
    fs::write(source_repo.join("pretrain.py"), "print('train')\n").unwrap();
    let dataset = temp.path().join("sft pack");
    fs::create_dir_all(&dataset).unwrap();
    let config = temp.path().join("cfg sft.yaml");
    fs::write(&config, "arch: hrm\n").unwrap();
    let exp = temp.path().join("hrm.yaml");
    fs::write(
        &exp,
        format!(
            r#"
id: hrm-text-dry-run
base_model:
  name: HRM-Text-XL
  source: local
dataset:
  path: '{}'
  format: sft_pack
backend:
  kind: hrm_text
  config_file: '{}'
  runtime:
    source_repo: '{}'
    nproc_per_node: "1"
  extra_args:
    - arch/size@arch=XL
    - weights_only_resume_from_ema=true
checkpoint:
  dir: checkpoints
retry:
  max_attempts: 1
"#,
            yaml_path(&dataset),
            yaml_path(&config),
            yaml_path(&source_repo)
        ),
    )
    .unwrap();

    let output = Command::new(refine_train_bin())
        .arg("--runs-root")
        .arg(temp.path().join("runs"))
        .arg("run")
        .arg(&exp)
        .arg("--dry-run")
        .output()
        .expect("run dry-run");

    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("torchrun"), "{stdout}");
    assert!(stdout.contains("--nproc_per_node=1"), "{stdout}");
    assert!(stdout.contains("pretrain.py"), "{stdout}");
    assert!(stdout.contains("HRM Text Source"), "{stdout}");
    assert!(stdout.contains("cfg sft.yaml"), "{stdout}");
    assert!(stdout.contains("arch/size@arch=XL"), "{stdout}");
}

#[test]
fn hrm_text_manifest_hashes_checkpoints_configs_and_runtime_probe() {
    let temp = tempfile::tempdir().unwrap();
    let checkpoint_dir = temp.path().join("checkpoints").join("fsdp_epoch_1");
    fs::create_dir_all(&checkpoint_dir).unwrap();
    let model = checkpoint_dir.join("model.safetensors");
    let carry = checkpoint_dir.join("carry_epoch_1.0.pt");
    fs::write(&model, b"weights").unwrap();
    fs::write(&carry, b"carry").unwrap();
    let config = temp.path().join("all_config.yaml");
    let tokenizer = temp.path().join("tokenizer.json");
    fs::write(&config, b"arch: hrm\n").unwrap();
    fs::write(&tokenizer, b"{\"vocab\":[]}").unwrap();
    let source_repo = temp.path().join("HRM-Text");
    fs::create_dir_all(&source_repo).unwrap();
    fs::write(source_repo.join("pretrain.py"), "print('train')\n").unwrap();
    fs::write(
        source_repo.join("simple_inference_engine.py"),
        "print('infer')\n",
    )
    .unwrap();
    let out = temp.path().join("manifest.json");

    let output = Command::new(refine_train_bin())
        .args(["hrm-text", "manifest"])
        .arg("--checkpoint-dir")
        .arg(&checkpoint_dir)
        .arg("--source-repo")
        .arg(&source_repo)
        .arg("--config-file")
        .arg(&config)
        .arg("--tokenizer-file")
        .arg(&tokenizer)
        .arg("--out")
        .arg(&out)
        .output()
        .expect("write manifest");

    assert_success(&output);
    let manifest = read_json(&out);
    assert_eq!(
        manifest["schema_version"],
        "refineforge-hrm-text-runtime-manifest-v1"
    );
    assert_eq!(manifest["status"], "ready");
    assert_eq!(manifest["source_project"], "HRM-Text");
    assert_eq!(manifest["checkpoint"]["file_count"], 2);
    assert_eq!(
        manifest["config"]["sha256"],
        hex_sha256(&fs::read(&config).unwrap())
    );
    assert_eq!(
        manifest["tokenizer"]["sha256"],
        hex_sha256(&fs::read(&tokenizer).unwrap())
    );
    assert!(manifest["checkpoint"]["sha256"].as_str().unwrap().len() == 64);
    assert!(manifest["helyx_handoff"]["requires_hash_verification"]
        .as_bool()
        .unwrap());
    assert_eq!(
        manifest["public_claim"],
        "hrm_text_runtime_artifacts_manifested_not_embedded_in_helyx_core"
    );
}

#[test]
fn hrm_text_probe_records_missing_python_as_blocked_not_success() {
    let temp = tempfile::tempdir().unwrap();
    let source_repo = temp.path().join("HRM-Text");
    fs::create_dir_all(&source_repo).unwrap();
    fs::write(source_repo.join("pretrain.py"), "print('train')\n").unwrap();
    let out = temp.path().join("probe.json");

    let output = Command::new(refine_train_bin())
        .args(["hrm-text", "probe"])
        .arg("--source-repo")
        .arg(&source_repo)
        .arg("--python")
        .arg("__missing_python_for_hrm_text_probe__")
        .arg("--out")
        .arg(&out)
        .output()
        .expect("write probe");

    assert_success(&output);
    let probe = read_json(&out);
    assert_eq!(
        probe["schema_version"],
        "refineforge-hrm-text-runtime-probe-v1"
    );
    assert_eq!(probe["status"], "blocked");
    assert_eq!(probe["source_repo"]["pretrain_py_exists"], true);
    assert!(probe["blockers"].as_array().unwrap().iter().any(|item| {
        item.as_str()
            .unwrap()
            .contains("python probe command failed")
    }));
}
