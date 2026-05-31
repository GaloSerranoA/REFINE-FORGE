//! End-to-end integration: pack a tiny SFT dataset, train the from-scratch GPT
//! backend through `runner::run_once`, and confirm it emits the drop-in
//! evidence artifacts the report/evidence pipeline consumes.

use refineforge_trainer::experiment::Experiment;
use refineforge_trainer::pack::{pack_sft, PackSftOptions};
use refineforge_trainer::{report, runner};
use serde_json::{json, Value};
use std::fs;

fn sft_row(id: &str, split: &str, prompt: &str, new_text: &str) -> String {
    let resp = json!({"patch": {"new_text": new_text, "rationale": "because"}}).to_string();
    json!({"id": id, "split": split, "prompt": prompt, "response": resp}).to_string()
}

#[test]
fn native_gpt_backend_trains_and_writes_eval_ready_evidence() {
    let temp = tempfile::tempdir().unwrap();
    let dataset = temp.path().join("proof-repair.jsonl");
    fs::write(
        &dataset,
        [
            sft_row("a", "train", "goal alpha needs simp", "simp"),
            sft_row("b", "train", "goal beta needs rfl", "rfl"),
            sft_row("c", "heldout", "goal gamma needs simp", "simp"),
            String::new(),
        ]
        .join("\n"),
    )
    .unwrap();

    // 1. Deterministically pack the SFT data into a token stream.
    let pack_dir = temp.path().join("pack");
    pack_sft(&PackSftOptions {
        input: dataset.clone(),
        out_dir: pack_dir.clone(),
        epochs: 1,
        seed: 7,
        max_seq_len: 32,
        world_size: 1,
        target_only: false,
        template_library: None,
    })
    .unwrap();

    // 2. A GPT experiment pointed at the pack.
    let config = temp.path().join("gpt.yaml");
    fs::write(
        &config,
        format!(
            r#"
id: native-gpt-test
base_model:
  name: refineforge-native-gpt-smoke
  source: native
dataset:
  path: {}
  format: pack
backend:
  kind: refineforge_native_gpt
hyperparameters:
  steps: 5
  learning_rate: 0.02
  n_embed: 16
  n_head: 2
  n_layers: 2
  context_length: 32
  seed: 7
checkpoint:
  save_steps: 5
  keep_last: 2
monitoring:
  metrics_to_track:
    - train_loss
    - dev_loss
    - target_token_accuracy
    - learning_rate
retry:
  max_attempts: 1
  backoff_seconds: 0
"#,
            pack_dir.display()
        ),
    )
    .unwrap();

    let experiment = Experiment::load(&config).unwrap();
    let runs_root = temp.path().join("runs");
    let outcome = runner::run_once(&runs_root, &experiment).unwrap();
    assert!(outcome.exit_status.success());
    assert_eq!(outcome.progress_records, 5);

    let run_dir = runs_root.join("native-gpt-test");

    // 3. Checkpoint carries the real transformer architecture + metrics.
    let ckpt_path = run_dir
        .join("checkpoints")
        .join("step-5")
        .join("gpt-checkpoint.json");
    assert!(ckpt_path.exists(), "checkpoint json should exist");
    let ckpt: Value = serde_json::from_str(&fs::read_to_string(&ckpt_path).unwrap()).unwrap();
    assert_eq!(
        ckpt["schema_version"],
        "refineforge-native-gpt-checkpoint-v1"
    );
    assert_eq!(ckpt["backend_kind"], "refineforge_native_gpt");
    assert_eq!(ckpt["architecture"]["kind"], "decoder_only_transformer");
    assert_eq!(
        ckpt["architecture"]["attention"]["kind"],
        "multi_head_causal_self_attention"
    );
    assert!(ckpt["target_token_accuracy"].is_number());
    assert!(ckpt["parameter_count"].as_u64().unwrap() > 0);
    assert_eq!(ckpt["weights_sha256"].as_str().unwrap().len(), 64);

    // 4. Metadata + generation smoke (hash carriers for the conversion/eval lanes).
    let meta: Value =
        serde_json::from_str(&fs::read_to_string(run_dir.join("train-metadata.json")).unwrap())
            .unwrap();
    assert_eq!(
        meta["schema_version"],
        "refineforge-native-gpt-train-metadata-v1"
    );
    assert!(!meta["tokenizer_sha256"].as_str().unwrap().is_empty());
    let gen: Value =
        serde_json::from_str(&fs::read_to_string(run_dir.join("generation-smoke.json")).unwrap())
            .unwrap();
    assert_eq!(gen["output_sha256"].as_str().unwrap().len(), 64);

    // 5. The report exposes the metrics evidence.rs maps to quality metrics.
    let paths = runner::RunPaths::for_experiment(&runs_root, &experiment);
    let report = report::build(&experiment, &paths, "success", 1).unwrap();
    assert_eq!(report.compute_ledger.backend_kind, "refineforge_native_gpt");
    assert_eq!(report.progress_record_count, 5);
    assert!(report.metric_summary.contains_key("target_token_accuracy"));
    assert!(report.metric_summary.contains_key("dev_loss"));
    assert!(report.metric_summary.contains_key("train_loss"));
}
