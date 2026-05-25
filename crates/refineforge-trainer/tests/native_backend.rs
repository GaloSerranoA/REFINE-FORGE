use refineforge_trainer::experiment::Experiment;
use refineforge_trainer::report;
use refineforge_trainer::runner;
use serde_json::Value;
use std::fs;

#[test]
fn native_backend_runs_without_external_trainer_and_writes_checkpoint() {
    let temp = tempfile::tempdir().unwrap();
    let dataset = temp.path().join("proof-repair.jsonl");
    fs::write(
        &dataset,
        [
            r#"{"id":"a","prompt":"Diagnostic: unsolved goals\nSource: theorem a := by rfl","response":"{\"start_line\":0,\"start_char\":0,\"end_line\":0,\"end_char\":3,\"new_text\":\"simp\",\"rationale\":\"use simplifier\"}","split":"train"}"#,
            r#"{"id":"b","prompt":"Diagnostic: unknown identifier\nSource: theorem b := by exact missing","response":"{\"start_line\":0,\"start_char\":0,\"end_line\":0,\"end_char\":5,\"new_text\":\"exact h\",\"rationale\":\"reuse hypothesis\"}","split":"train"}"#,
            "",
        ]
        .join("\n"),
    )
    .unwrap();
    let config = temp.path().join("native.yaml");
    fs::write(
        &config,
        format!(
            r#"
id: native-v0-test
base_model:
  name: refineforge-native-linear-smoke
  source: native
dataset:
  path: {}
  format: jsonl
backend:
  kind: refineforge_native
hyperparameters:
  steps: 6
  learning_rate: 0.2
  feature_buckets: 32
  target_buckets: 8
checkpoint:
  save_steps: 3
  keep_last: 2
monitoring:
  metrics_to_track:
    - loss
    - accuracy
    - learning_rate
retry:
  max_attempts: 1
  backoff_seconds: 0
"#,
            dataset.display()
        ),
    )
    .unwrap();

    let experiment = Experiment::load(&config).unwrap();
    let runs_root = temp.path().join("runs");
    let outcome = runner::run_once(&runs_root, &experiment).unwrap();

    assert!(outcome.exit_status.success());
    assert_eq!(outcome.progress_records, 6);

    let run_dir = runs_root.join("native-v0-test");
    let progress = fs::read_to_string(run_dir.join("progress.jsonl")).unwrap();
    assert_eq!(progress.lines().count(), 6);

    let checkpoint = run_dir
        .join("checkpoints")
        .join("step-6")
        .join("native-checkpoint.json");
    assert!(checkpoint.exists());
    let checkpoint_json: Value =
        serde_json::from_str(&fs::read_to_string(checkpoint).unwrap()).unwrap();
    assert_eq!(checkpoint_json["backend_kind"], "refineforge_native");

    let paths = runner::RunPaths::for_experiment(&runs_root, &experiment);
    let report = report::build(&experiment, &paths, "success", 1).unwrap();
    assert_eq!(report.compute_ledger.backend_kind, "refineforge_native");
    assert_eq!(report.progress_record_count, 6);
    assert_eq!(report.checkpoints.len(), 2);
    assert!(report.metric_summary.contains_key("loss"));
    assert!(report.metric_summary.contains_key("accuracy"));
}
