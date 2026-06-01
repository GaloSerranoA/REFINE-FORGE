//! Drive the `refineforge_lean_prover` backend through `runner::run_once` fully
//! offline — a *replay* prover (no GPU / no server) and a *dry-run* verifier (no
//! Lean install) — and confirm it emits the standard trust evidence:
//! `progress.jsonl` carrying the honest `proof_pass_rate` metric, a
//! `proof-search-report.json`, and a `report.json` whose metric summary the
//! eval/regression/approval ladder consumes.
//!
//! This validates the orchestration + evidence contract end-to-end without any
//! live prover or Lean toolchain. A real run swaps `prover_replay_file` → a live
//! `prover_base_url`/`prover_model` and `verifier: dry_run` → `verifier: lean`.

use refineforge_trainer::experiment::Experiment;
use refineforge_trainer::report;
use refineforge_trainer::runner;
use serde_json::Value;
use std::fs;

fn fwd(p: &std::path::Path) -> String {
    p.display().to_string().replace('\\', "/")
}

#[test]
fn lean_prover_backend_runs_offline_and_emits_proof_pass_rate() {
    let temp = tempfile::tempdir().unwrap();

    // 3 problems; p1 & p3 have a candidate containing "rfl", p2 does not → 2/3.
    let problems = temp.path().join("problems.jsonl");
    fs::write(
        &problems,
        [
            r#"{"id":"p1","statement":"goal-1","split":"heldout"}"#,
            r#"{"id":"p2","statement":"goal-2","split":"heldout"}"#,
            r#"{"id":"p3","statement":"goal-3","split":"heldout"}"#,
            "",
        ]
        .join("\n"),
    )
    .unwrap();

    // Replay: pre-generated candidates keyed by each problem's statement.
    let replay = temp.path().join("replay.jsonl");
    fs::write(
        &replay,
        [
            r#"{"prompt":"goal-1","candidates":["nope","by rfl"]}"#,
            r#"{"prompt":"goal-2","candidates":["sorry"]}"#,
            r#"{"prompt":"goal-3","candidates":["by rfl"]}"#,
            "",
        ]
        .join("\n"),
    )
    .unwrap();

    let config = temp.path().join("lean-prover.yaml");
    fs::write(
        &config,
        format!(
            r#"
id: lean-prover-offline-test
base_model:
  name: deepseek-prover-v2-7b
  source: native
dataset:
  path: {problems}
  format: jsonl
backend:
  kind: refineforge_lean_prover
hyperparameters:
  prover_replay_file: {replay}
  verifier: dry_run
  verifier_substring: rfl
  samples: 4
monitoring:
  metrics_to_track:
    - proof_pass_rate
    - solved
retry:
  max_attempts: 1
  backoff_seconds: 0
"#,
            problems = fwd(&problems),
            replay = fwd(&replay),
        ),
    )
    .unwrap();

    let experiment = Experiment::load(&config).unwrap();
    let runs_root = temp.path().join("runs");
    let outcome = runner::run_once(&runs_root, &experiment).unwrap();

    // The run completes (proof quality is judged by the gates, not the exit code).
    assert!(outcome.exit_status.success());
    assert_eq!(outcome.progress_records, 3);

    let run_dir = runs_root.join("lean-prover-offline-test");
    let progress = fs::read_to_string(run_dir.join("progress.jsonl")).unwrap();
    assert_eq!(progress.lines().count(), 3, "one record per problem");

    // The per-problem search report: 2/3 solved, with the verified proofs kept.
    let search: Value = serde_json::from_str(
        &fs::read_to_string(run_dir.join("proof-search-report.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        search["schema_version"],
        "refineforge-lean-prover-search-v1"
    );
    assert_eq!(search["solved"], 2);
    assert_eq!(search["problems"], 3);
    assert_eq!(search["results"][0]["verified_proof"], "by rfl");
    assert_eq!(search["results"][0]["attempts"], 2);
    assert_eq!(search["results"][1]["solved"], false);

    // The standard report.json summarizes proof_pass_rate honestly (no warping).
    let paths = runner::RunPaths::for_experiment(&runs_root, &experiment);
    let report = report::build(&experiment, &paths, "success", 1).unwrap();
    assert_eq!(report.progress_record_count, 3);
    assert!(report.metric_summary.contains_key("proof_pass_rate"));
    assert!(report.metric_summary.contains_key("solved"));
    // Cumulative pass rate: 1/1 → 1/2 → 2/3, so the *final* (last) value is 2/3
    // and the peak (max, after the first solved problem) is 1.0.
    let pass = &report.metric_summary["proof_pass_rate"];
    assert!(
        (pass.last - 2.0 / 3.0).abs() < 1e-9,
        "final proof_pass_rate should be 2/3"
    );
    assert!(
        (pass.max - 1.0).abs() < 1e-9,
        "peak proof_pass_rate should be 1.0"
    );
}

#[test]
fn lean_prover_rejects_unknown_verifier() {
    let temp = tempfile::tempdir().unwrap();
    let problems = temp.path().join("problems.jsonl");
    fs::write(&problems, "{\"id\":\"p1\",\"statement\":\"g1\"}\n").unwrap();
    // A valid replay file so the prover builds — the error must come from the
    // verifier kind, not from a malformed replay.
    let replay = temp.path().join("replay.jsonl");
    fs::write(&replay, "{\"prompt\":\"g1\",\"candidates\":[\"by rfl\"]}\n").unwrap();
    let config = temp.path().join("bad.yaml");
    fs::write(
        &config,
        format!(
            r#"
id: lean-prover-bad-verifier
base_model: {{ name: m, source: native }}
dataset: {{ path: {problems}, format: jsonl }}
backend: {{ kind: refineforge_lean_prover }}
hyperparameters: {{ prover_replay_file: {replay}, verifier: nonsense }}
monitoring: {{ metrics_to_track: [proof_pass_rate] }}
retry: {{ max_attempts: 1, backoff_seconds: 0 }}
"#,
            problems = fwd(&problems),
            replay = fwd(&replay),
        ),
    )
    .unwrap();
    let experiment = Experiment::load(&config).unwrap();
    let runs_root = temp.path().join("runs");
    let err = runner::run_once(&runs_root, &experiment).unwrap_err();
    assert!(
        err.to_string().contains("unknown verifier") || format!("{err:#}").contains("nonsense")
    );
}
