use serde_json::{json, Value};
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

fn write_json(path: &Path, value: Value) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
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

fn write_proof_repair_sft(path: &Path) {
    let rows = [
        json!({
            "id": "repair-a",
            "prompt": "Diagnostic: unsolved goals\nSource: theorem a := by sorry",
            "response": serde_json::to_string(&json!({
                "patch": {
                    "start_line": 0,
                    "start_char": 0,
                    "end_line": 0,
                    "end_char": 5,
                    "new_text": "simp",
                    "rationale": "close by simplification"
                }
            })).unwrap(),
            "metadata": {"split": "train"}
        }),
        json!({
            "id": "repair-b",
            "prompt": "Diagnostic: unknown identifier\nSource: theorem b := by exact missing",
            "response": serde_json::to_string(&json!({
                "patch": {
                    "start_line": 1,
                    "start_char": 0,
                    "end_line": 1,
                    "end_char": 12,
                    "new_text": "exact h",
                    "rationale": "reuse the provided hypothesis"
                }
            })).unwrap(),
            "metadata": {"split": "heldout"}
        }),
    ];
    let content = rows
        .into_iter()
        .map(|row| serde_json::to_string(&row).unwrap())
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(path, format!("{content}\n")).unwrap();
}

#[test]
fn data_pack_sft_writes_target_only_masks_and_packing_report() {
    let temp = tempfile::tempdir().unwrap();
    let dataset = temp.path().join("proof-repair.jsonl");
    write_proof_repair_sft(&dataset);
    let out_dir = temp.path().join("sft-pack");

    let output = Command::new(refine_train_bin())
        .args(["data", "pack-sft"])
        .arg(&dataset)
        .arg("--out")
        .arg(&out_dir)
        .args([
            "--epochs",
            "2",
            "--seed",
            "7",
            "--max-seq-len",
            "16",
            "--world-size",
            "2",
            "--target-only",
        ])
        .output()
        .expect("run refine-train data pack-sft");

    assert_success(&output);
    let manifest = read_json(&out_dir.join("pack-manifest.json"));
    assert_eq!(manifest["schema_version"], "refineforge-sft-pack-v1");
    assert_eq!(manifest["record_count"], 2);
    assert_eq!(manifest["target_only"], true);
    assert_eq!(manifest["tokenizer"]["sha256"].as_str().unwrap().len(), 64);
    assert!(manifest["pack_sha256"].as_str().unwrap().len() == 64);

    assert!(out_dir.join("tokens.bin").exists());
    let loss_mask = fs::read(out_dir.join("loss-mask.bin")).unwrap();
    assert!(loss_mask.contains(&0), "context tokens must be masked out");
    assert!(loss_mask.contains(&1), "target tokens must contribute loss");
    assert!(out_dir.join("epoch-000-shuffle.json").exists());
    assert!(out_dir.join("epoch-001-shuffle.json").exists());

    let packing = read_json(&out_dir.join("packing_report.json"));
    assert_eq!(packing["schema_version"], "refineforge-packing-report-v1");
    assert!(packing["total_tokens"].as_u64().unwrap() > 0);
    assert!(packing["supervised_target_tokens"].as_u64().unwrap() > 0);
    assert!(packing["context_tokens"].as_u64().unwrap() > 0);
    assert!(packing["slot_utilization"].as_f64().unwrap() > 0.0);
    assert_eq!(packing["rank_balance"].as_array().unwrap().len(), 2);

    let plan = read_json(&out_dir.join("multipack-plan.json"));
    assert_eq!(plan["schema_version"], "refineforge-multipack-plan-v1");
    assert_eq!(plan["world_size"], 2);
}

#[test]
fn data_causal_lm_preprocess_writes_manifest_and_chunks() {
    let temp = tempfile::tempdir().unwrap();
    let input = temp.path().join("causal.jsonl");
    fs::write(
        &input,
        [
            serde_json::to_string(&json!({"text": "alpha beta gamma"})).unwrap(),
            serde_json::to_string(&json!({"text": "beta gamma delta"})).unwrap(),
            String::new(),
        ]
        .join("\n"),
    )
    .unwrap();
    let out_dir = temp.path().join("causal-pack");

    let output = Command::new(refine_train_bin())
        .args(["data", "causal-lm-preprocess"])
        .arg(&input)
        .arg("--out")
        .arg(&out_dir)
        .args(["--chunk-len", "5", "--stride", "3"])
        .output()
        .expect("run causal-lm-preprocess");

    assert_success(&output);
    let manifest = read_json(&out_dir.join("causal-lm-manifest.json"));
    assert_eq!(manifest["schema_version"], "refineforge-causal-lm-pack-v1");
    assert_eq!(manifest["document_count"], 2);
    assert_eq!(manifest["chunk_length"], 5);
    assert!(manifest["token_count"].as_u64().unwrap() >= 6);
    assert_eq!(
        manifest["input_sha256"].as_str().unwrap(),
        hex_sha256(&fs::read(&input).unwrap())
    );
    assert_eq!(manifest["output_sha256"].as_str().unwrap().len(), 64);
    assert!(out_dir.join("tokens.bin").exists());
    assert!(out_dir.join("chunks.json").exists());
}

#[test]
fn native_causal_lm_backend_trains_and_writes_eval_ready_checkpoint() {
    let temp = tempfile::tempdir().unwrap();
    let dataset = temp.path().join("proof-repair.jsonl");
    write_proof_repair_sft(&dataset);
    let pack_dir = temp.path().join("sft-pack");
    assert_success(
        &Command::new(refine_train_bin())
            .args(["data", "pack-sft"])
            .arg(&dataset)
            .arg("--out")
            .arg(&pack_dir)
            .args(["--epochs", "1", "--max-seq-len", "16", "--target-only"])
            .output()
            .unwrap(),
    );

    let config = temp.path().join("causal.yaml");
    fs::write(
        &config,
        format!(
            r#"
id: native-causal-test
base_model:
  name: refineforge-native-causal-lm
  source: native
dataset:
  path: {}
  format: sft_pack
backend:
  kind: refineforge_native_causal_lm
hyperparameters:
  steps: 4
  learning_rate: 0.05
  hidden_size: 8
  eval_split: heldout
checkpoint:
  save_steps: 4
  keep_last: 2
monitoring:
  metrics_to_track:
    - train_loss
    - dev_loss
    - target_token_accuracy
retry:
  max_attempts: 1
  backoff_seconds: 0
"#,
            pack_dir.display()
        ),
    )
    .unwrap();
    let runs_root = temp.path().join("runs");

    let output = Command::new(refine_train_bin())
        .arg("--runs-root")
        .arg(&runs_root)
        .arg("run")
        .arg(&config)
        .output()
        .expect("run native causal backend");

    assert_success(&output);
    let run_dir = runs_root.join("native-causal-test");
    let report = read_json(&run_dir.join("report.json"));
    assert_eq!(report["final_outcome"], "success");
    assert!(report["metric_summary"]["train_loss"].is_object());
    assert!(report["metric_summary"]["dev_loss"].is_object());
    assert!(report["metric_summary"]["target_token_accuracy"].is_object());

    let checkpoint = read_json(
        &run_dir
            .join("checkpoints")
            .join("step-4")
            .join("causal-lm-checkpoint.json"),
    );
    assert_eq!(
        checkpoint["schema_version"],
        "refineforge-native-causal-lm-checkpoint-v1"
    );
    assert!(checkpoint["architecture"]["token_embedding"].is_object());
    assert!(checkpoint["architecture"]["causal_attention"].is_object());
    assert!(checkpoint["architecture"]["mlp_block"].is_object());
    assert!(run_dir.join("generation-smoke.json").exists());
}

#[test]
fn evidence_command_generates_lineage_conversion_eval_regression_and_promotion_files() {
    let temp = tempfile::tempdir().unwrap();
    let dataset = temp.path().join("proof-repair.jsonl");
    write_proof_repair_sft(&dataset);
    let pack_dir = temp.path().join("sft-pack");
    assert_success(
        &Command::new(refine_train_bin())
            .args(["data", "pack-sft"])
            .arg(&dataset)
            .arg("--out")
            .arg(&pack_dir)
            .args(["--epochs", "1", "--max-seq-len", "16", "--target-only"])
            .output()
            .unwrap(),
    );
    let config = temp.path().join("causal.yaml");
    fs::write(
        &config,
        format!(
            r#"
id: evidence-causal-test
base_model:
  name: refineforge-native-causal-lm
  source: native
dataset:
  path: {}
  format: sft_pack
backend:
  kind: refineforge_native_causal_lm
hyperparameters:
  steps: 4
  learning_rate: 0.05
  hidden_size: 8
checkpoint:
  save_steps: 4
  keep_last: 1
monitoring:
  metrics_to_track:
    - train_loss
    - dev_loss
    - target_token_accuracy
retry:
  max_attempts: 1
  backoff_seconds: 0
"#,
            pack_dir.display()
        ),
    )
    .unwrap();
    let runs_root = temp.path().join("runs");
    assert_success(
        &Command::new(refine_train_bin())
            .arg("--runs-root")
            .arg(&runs_root)
            .arg("run")
            .arg(&config)
            .output()
            .unwrap(),
    );
    let run_dir = runs_root.join("evidence-causal-test");
    let baseline = temp.path().join("baseline-eval.json");
    write_json(
        &baseline,
        json!({
            "schema_version": "refineforge-training-eval-report-v1",
            "status": "passed",
            "candidate": {"model_id": "baseline"},
            "metrics": {
                "heldout_exact_patch_acceptance": 0.0,
                "target_token_accuracy": 0.0
            },
            "quality_metrics": {
                "heldout_exact_patch_acceptance": 0.0,
                "target_token_accuracy": 0.0
            }
        }),
    );
    let evidence_dir = temp.path().join("evidence");

    let output = Command::new(refine_train_bin())
        .args(["evidence"])
        .arg(&run_dir)
        .arg("--out-dir")
        .arg(&evidence_dir)
        .arg("--baseline-report")
        .arg(&baseline)
        .args(["--model-id", "refineforge-native-causal-smoke-v1"])
        .output()
        .expect("run evidence generation");

    assert_success(&output);
    let checkpoint = evidence_dir.join("training/checkpoint.safetensors");
    assert!(checkpoint.exists());
    let checkpoint_sha = hex_sha256(&fs::read(&checkpoint).unwrap());

    let eval = read_json(&evidence_dir.join("training/eval-report.json"));
    assert_eq!(eval["status"], "passed");
    assert!(eval["quality_metrics"].is_object());
    assert!(eval["metrics"].as_object().unwrap().len() > 1);

    let regression = read_json(&evidence_dir.join("training/regression-report.json"));
    assert_eq!(regression["status"], "passed");
    assert!(regression["metric_deltas"].is_object());

    let ledger = read_json(&evidence_dir.join("training/compute-ledger.json"));
    assert_eq!(ledger["status"], "passed");
    assert!(ledger["duration_ms"].is_number());
    assert_eq!(ledger["backend_kind"], "refineforge_native_causal_lm");

    let conversion = read_json(&evidence_dir.join("training/conversion-manifest.json"));
    assert_eq!(conversion["status"], "passed");
    assert_eq!(conversion["checkpoint_sha256"], checkpoint_sha);

    let promotion = read_json(&evidence_dir.join("training/promotion-manifest.json"));
    assert_eq!(promotion["decision"], "promote");
    assert_eq!(promotion["checkpoint_sha256"], checkpoint_sha);
    assert!(
        promotion["lineage"]["config_sha256"]
            .as_str()
            .unwrap()
            .len()
            == 64
    );
    assert!(
        promotion["lineage"]["tokenizer_sha256"]
            .as_str()
            .unwrap()
            .len()
            == 64
    );
    assert!(
        promotion["conversion"]["manifest_sha256"]
            .as_str()
            .unwrap()
            .len()
            == 64
    );
}

#[test]
fn hrm_text_and_pytorch_baseline_backends_have_explicit_external_command_adapters() {
    let temp = tempfile::tempdir().unwrap();
    let dataset = temp.path().join("data.jsonl");
    fs::write(&dataset, "{\"text\":\"hello\"}\n").unwrap();
    let hrm_config = temp.path().join("hrm.yaml");
    fs::write(
        &hrm_config,
        format!(
            r#"
id: hrm-adapter
base_model:
  name: HRM-Text
  source: external
dataset:
  path: {}
backend:
  kind: hrm_text
  config_file: config/cfg_sft.yaml
"#,
            dataset.display()
        ),
    )
    .unwrap();
    let py_config = temp.path().join("pytorch.yaml");
    fs::write(
        &py_config,
        format!(
            r#"
id: pytorch-adapter
base_model:
  name: train-llm-from-scratch
  source: external
dataset:
  path: {}
backend:
  kind: pytorch_baseline
  command: python train.py --data {{dataset_path}} --out {{run_dir}}
"#,
            dataset.display()
        ),
    )
    .unwrap();

    let output = Command::new(refine_train_bin())
        .arg("--runs-root")
        .arg(temp.path().join("runs"))
        .arg("run")
        .arg(&hrm_config)
        .arg("--dry-run")
        .output()
        .expect("dry-run hrm adapter");
    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("torchrun"));
    assert!(stdout.contains("pretrain.py"));
    assert!(stdout.contains("--config-name"));

    let output = Command::new(refine_train_bin())
        .arg("--runs-root")
        .arg(temp.path().join("runs"))
        .arg("run")
        .arg(&py_config)
        .arg("--dry-run")
        .output()
        .expect("dry-run pytorch adapter");
    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("python train.py"));
    assert!(stdout.contains(dataset.to_string_lossy().as_ref()));
}
