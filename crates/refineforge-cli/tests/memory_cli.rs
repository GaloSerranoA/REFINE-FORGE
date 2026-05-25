use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs;
use std::process::{Command, Output};

fn run_refine(root: &std::path::Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_refine"))
        .arg("--root")
        .arg(root)
        .args(args)
        .output()
        .expect("run refine")
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "command failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

fn rfmem_id(
    agent: &str,
    target: &str,
    kind: &str,
    content: &str,
    source_path: Option<&str>,
    source_sha256: Option<&str>,
) -> String {
    let mut hasher = Sha256::new();
    for part in [
        "refineforge-memory-v1",
        agent,
        target,
        kind,
        content,
        source_path.unwrap_or(""),
        source_sha256.unwrap_or(""),
    ] {
        hasher.update(part.as_bytes());
        hasher.update([0]);
    }
    format!("rfmem:{}", hex::encode(hasher.finalize()))
}

#[test]
fn memory_add_hashes_source_and_lists_non_authoritative_record() {
    let td = tempfile::tempdir().unwrap();
    let source = td.path().join("source.md");
    fs::write(&source, "HELYX training fixture note\n").unwrap();

    let output = run_refine(
        td.path(),
        &[
            "memory",
            "add",
            "--agent",
            "train",
            "--target",
            "helyx",
            "--kind",
            "citation",
            "--content",
            "Use llms-from-scratch-rs only as a fixture source.",
            "--source-path",
            "source.md",
        ],
    );
    assert_success(&output);
    let record: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(record["schema_version"], "refineforge-memory-v1");
    assert_eq!(record["agent"], "train");
    assert_eq!(record["target"], "helyx");
    assert_eq!(record["kind"], "citation");
    assert_eq!(record["trust_effect"], "none");
    assert_eq!(
        record["source_sha256"],
        sha256_hex(b"HELYX training fixture note\n")
    );
    assert!(record["id"].as_str().unwrap().starts_with("rfmem:"));

    let listed = run_refine(
        td.path(),
        &["memory", "list", "--agent", "train", "--target", "helyx"],
    );
    assert_success(&listed);
    let records: Value = serde_json::from_slice(&listed.stdout).unwrap();
    assert_eq!(records.as_array().unwrap().len(), 1);
    assert_eq!(records[0]["id"], record["id"]);
}

#[test]
fn memory_import_deduplicates_and_rejects_trust_upgrades() {
    let td = tempfile::tempdir().unwrap();
    let imported = td.path().join("import.jsonl");
    let trusted_content = "pretend this proves implementation refinement";
    let trusted_id = rfmem_id("lean", "helyx", "claim_note", trusted_content, None, None);
    let trusted = format!(
        r#"{{"schema_version":"refineforge-memory-v1","id":"{trusted_id}","agent":"lean","target":"helyx","kind":"claim_note","content":"{trusted_content}","source_path":null,"source_sha256":null,"created_at":"2026-05-24T00:00:00Z","trust_effect":"model-linked"}}"#
    );
    fs::write(&imported, format!("{trusted}\n")).unwrap();

    let rejected = run_refine(td.path(), &["memory", "import", imported.to_str().unwrap()]);
    assert!(
        !rejected.status.success(),
        "trust-upgrading memory import should fail"
    );

    let valid_content = "Use memory as advisory context only.";
    let valid_id = rfmem_id("lean", "cogn8ty", "handoff", valid_content, None, None);
    let valid = format!(
        r#"{{"schema_version":"refineforge-memory-v1","id":"{valid_id}","agent":"lean","target":"cogn8ty","kind":"handoff","content":"{valid_content}","source_path":null,"source_sha256":null,"created_at":"2026-05-24T00:00:00Z","trust_effect":"none"}}"#
    );
    fs::write(&imported, format!("{valid}\n{valid}\n")).unwrap();
    let accepted = run_refine(td.path(), &["memory", "import", imported.to_str().unwrap()]);
    assert_success(&accepted);
    let summary: Value = serde_json::from_slice(&accepted.stdout).unwrap();
    assert_eq!(summary["imported"], 1);
    assert_eq!(summary["skipped_duplicates"], 1);
}
