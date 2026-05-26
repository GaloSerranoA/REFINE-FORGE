use serde_json::Value;
use std::path::Path;
use std::process::Command;

fn run_refine(root: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_refine"))
        .arg("--root")
        .arg(root)
        .args(args)
        .output()
        .expect("run refine inmortal-proof command")
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
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn write_claim(root: &Path, claim_id: &str, lean_file: &str, theorem: &str) {
    let claims = root.join("claims");
    std::fs::create_dir_all(&claims).unwrap();
    std::fs::write(
        claims.join(format!("{claim_id}.yaml")),
        format!(
            r#"claim_id: {claim_id}
title: Nexus smoke
description: Test claim for Nexus receipt generation.
scope: model-only
status: drafted
authors: [test]
lean:
  toolchain: leanprover/lean4:v4.29.1
  module: Refineforge.NexusSmoke
  file: {lean_file}
  theorems: [{theorem}]
policy:
  no_sorry: true
  no_admit: true
  no_axioms_beyond_lean_core: true
review:
  human_operator: null
  reviewed_on: null
  notes: test fixture
"#
        ),
    )
    .unwrap();
}

fn write_lean(root: &Path, relative: &str, source: &str) {
    let path = root.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, source).unwrap();
}

#[test]
fn inmortal_proof_run_writes_proof_search_receipt() {
    let td = tempfile::tempdir().unwrap();
    let root = td.path();
    write_claim(
        root,
        "CLAIM-NEXUS-001",
        "lean/Refineforge/NexusSmoke.lean",
        "target",
    );
    write_lean(
        root,
        "lean/Refineforge/NexusSmoke.lean",
        r#"namespace Refineforge.NexusSmoke

-- EVOLVE-BLOCK-START
lemma helper : True := by
  trivial
-- EVOLVE-BLOCK-END

def selectedAnswer : Bool :=
-- EVOLVE-VALUE-START
true
-- EVOLVE-VALUE-END

theorem target : True := by
-- EVOLVE-BLOCK-START
  exact helper
-- EVOLVE-BLOCK-END

end Refineforge.NexusSmoke
"#,
    );

    let out = root.join("nexus-out");
    let output = run_refine(
        root,
        &[
            "inmortal-proof",
            "run",
            "CLAIM-NEXUS-001",
            "--episodes",
            "2",
            "--out",
            out.to_str().unwrap(),
            "--json",
        ],
    );
    assert_success(&output);

    let report = read_json(&out.join("inmortal-proof-report.json"));
    assert_eq!(
        report["schema_version"],
        "refineforge-inmortalproof-report-v1"
    );
    assert_eq!(report["status"], "ready_for_search");
    assert_eq!(
        report["public_claim"],
        "inmortalproof_receipt_only_not_alphaproof"
    );
    assert_eq!(report["system_name"], "InmortalProof");
    assert_eq!(report["claim_id"], "CLAIM-NEXUS-001");
    assert_eq!(report["editable_regions"].as_array().unwrap().len(), 3);
    assert_eq!(report["episodes"].as_array().unwrap().len(), 2);
    assert_eq!(report["population"].as_array().unwrap().len(), 1);
    assert_eq!(report["goal_cache"].as_array().unwrap().len(), 1);
    assert!(report["blockers"].as_array().unwrap().is_empty());
    assert_eq!(
        report["integrity"]["protected_sha256"]
            .as_str()
            .unwrap()
            .len(),
        64
    );
    assert_eq!(report["population"][0]["selection"]["algorithm"], "p_ucb");
    assert_eq!(report["rater"]["rubric_version"], "inmortalproof-rater-v1");

    assert!(out.join("inmortal-proof-report.md").exists());
    assert!(out.join("population.jsonl").exists());
    assert!(out.join("episodes.jsonl").exists());
    assert!(out.join("goal-cache.jsonl").exists());
}

#[test]
fn inmortal_proof_run_blocks_claim_without_evolve_markers() {
    let td = tempfile::tempdir().unwrap();
    let root = td.path();
    write_claim(
        root,
        "CLAIM-NEXUS-002",
        "lean/Refineforge/NoMarkers.lean",
        "target",
    );
    write_lean(
        root,
        "lean/Refineforge/NoMarkers.lean",
        r#"namespace Refineforge.NoMarkers
theorem target : True := by
  trivial
end Refineforge.NoMarkers
"#,
    );

    let out = root.join("nexus-out");
    let output = run_refine(
        root,
        &[
            "inmortal-proof",
            "run",
            "CLAIM-NEXUS-002",
            "--out",
            out.to_str().unwrap(),
            "--json",
        ],
    );
    assert_success(&output);

    let report = read_json(&out.join("inmortal-proof-report.json"));
    assert_eq!(report["status"], "blocked");
    assert_eq!(report["editable_regions"].as_array().unwrap().len(), 0);
    assert!(report["blockers"]
        .as_array()
        .unwrap()
        .iter()
        .any(|blocker| blocker
            .as_str()
            .unwrap()
            .contains("no EVOLVE-BLOCK or EVOLVE-VALUE markers")));
}

#[test]
fn inmortal_proof_search_generates_candidates_and_rolls_back_source() {
    let td = tempfile::tempdir().unwrap();
    let root = td.path();
    write_claim(
        root,
        "CLAIM-NEXUS-003",
        "lean/Refineforge/SearchSmoke.lean",
        "target",
    );
    write_lean(
        root,
        "lean/Refineforge/SearchSmoke.lean",
        r#"namespace Refineforge.SearchSmoke

theorem target : True := by
-- EVOLVE-BLOCK-START
  sorry
-- EVOLVE-BLOCK-END

end Refineforge.SearchSmoke
"#,
    );
    let lean_path = root.join("lean/Refineforge/SearchSmoke.lean");
    let original_source = std::fs::read_to_string(&lean_path).unwrap();

    let out = root.join("inmortal-search-out");
    let output = run_refine(
        root,
        &[
            "inmortal-proof",
            "search",
            "CLAIM-NEXUS-003",
            "--validator",
            "receipt-only",
            "--max-candidates",
            "8",
            "--out",
            out.to_str().unwrap(),
            "--no-export-bundle",
            "--json",
        ],
    );
    assert_success(&output);

    let report = read_json(&out.join("inmortal-proof-search-report.json"));
    assert_eq!(
        report["schema_version"],
        "refineforge-inmortalproof-search-v1"
    );
    assert_eq!(report["system_name"], "InmortalProof");
    assert_eq!(report["status"], "candidate_found_receipt_only");
    assert_eq!(report["validation_mode"], "receipt_only");
    assert_eq!(
        report["public_claim"],
        "inmortalproof_search_receipt_only_not_lean_verified"
    );
    assert_eq!(report["bundle_export"]["status"], "not_requested");
    assert_eq!(
        report["accepted_candidate"]["validation"]["status"],
        "receipt_only_passed"
    );
    assert_eq!(report["accepted_candidate"]["protected_hash_stable"], true);
    assert!(report["attempts"].as_array().unwrap().len() >= 2);
    assert_eq!(
        report["attempts"][0]["validation"]["status"],
        "policy_violation"
    );

    assert_eq!(
        std::fs::read_to_string(&lean_path).unwrap(),
        original_source
    );
    assert!(out.join("candidates.jsonl").exists());
    assert!(out.join("search-episodes.jsonl").exists());
    assert!(out.join("lean-feedback.jsonl").exists());
    assert!(out.join("population.jsonl").exists());
}

fn read_jsonl(path: &Path) -> Vec<Value> {
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn run_search(root: &Path, claim_id: &str, out: &Path, ranking: &str) {
    let output = run_refine(
        root,
        &[
            "inmortal-proof",
            "search",
            claim_id,
            "--validator",
            "receipt-only",
            "--max-candidates",
            "8",
            "--ranking",
            ranking,
            "--out",
            out.to_str().unwrap(),
            "--no-export-bundle",
            "--json",
        ],
    );
    assert_success(&output);
}

#[test]
fn inmortal_proof_search_best_first_reorders_candidates() {
    let td = tempfile::tempdir().unwrap();
    let root = td.path();
    write_claim(
        root,
        "CLAIM-NEXUS-004",
        "lean/Refineforge/BestFirstSmoke.lean",
        "target",
    );
    write_lean(
        root,
        "lean/Refineforge/BestFirstSmoke.lean",
        r#"namespace Refineforge.BestFirstSmoke

theorem target : True := by
-- EVOLVE-BLOCK-START
  sorry
-- EVOLVE-BLOCK-END

end Refineforge.BestFirstSmoke
"#,
    );

    let out_insertion = root.join("out-insertion");
    let out_best_first = root.join("out-best-first");
    run_search(root, "CLAIM-NEXUS-004", &out_insertion, "insertion-order");
    run_search(root, "CLAIM-NEXUS-004", &out_best_first, "best-first");

    let insertion = read_jsonl(&out_insertion.join("candidates.jsonl"));
    let best_first = read_jsonl(&out_best_first.join("candidates.jsonl"));
    assert!(
        insertion.len() >= 4,
        "insertion-order produced {} candidates",
        insertion.len()
    );
    assert_eq!(insertion.len(), best_first.len());

    // Every candidate carries a heuristic_score field.
    for cand in &insertion {
        assert!(
            cand.get("heuristic_score").is_some(),
            "missing heuristic_score on {:?}",
            cand
        );
    }

    // Best-first: scores are monotonically non-increasing.
    let scores: Vec<f64> = best_first
        .iter()
        .map(|c| c["heuristic_score"].as_f64().unwrap())
        .collect();
    for window in scores.windows(2) {
        assert!(
            window[0] >= window[1],
            "best-first scores not monotonically non-increasing: {:?}",
            scores
        );
    }

    // First candidate under best-first is the preserve baseline (score 1.00).
    assert_eq!(
        best_first[0]["generator"].as_str().unwrap(),
        "builtin_preserve"
    );
    assert_eq!(best_first[0]["heuristic_score"].as_f64().unwrap(), 1.00);

    // Best-first should reorder relative to insertion-order: the second candidate
    // differs (insertion-order yields "trivial"; best-first yields "rfl").
    let insertion_second = insertion[1]["body"].as_str().unwrap().trim();
    let best_first_second = best_first[1]["body"].as_str().unwrap().trim();
    assert_eq!(insertion_second, "trivial");
    assert_eq!(best_first_second, "rfl");

    // Report exposes the chosen ranking mode.
    let insertion_report = read_json(&out_insertion.join("inmortal-proof-search-report.json"));
    let best_first_report = read_json(&out_best_first.join("inmortal-proof-search-report.json"));
    assert_eq!(insertion_report["ranking_mode"], "insertion_order");
    assert_eq!(best_first_report["ranking_mode"], "best_first");
}
