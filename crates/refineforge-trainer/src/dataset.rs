//! Dataset audit for proof-repair SFT JSONL.
//!
//! This validates the handoff surface before a backend such as
//! `helyx-train` or Axolotl spends GPU time. It does not know model
//! internals; it checks the row contract that Refine-Forge repair
//! strategies consume later.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

#[derive(Debug, Clone, Default)]
pub struct AuditExpectations {
    pub rows: Option<usize>,
    pub splits: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetAudit {
    pub schema_version: String,
    pub path: String,
    pub sha256: String,
    pub dataset_sha256: String,
    pub total_rows: usize,
    pub record_count: usize,
    pub unique_ids: usize,
    pub split_counts: BTreeMap<String, usize>,
    pub valid_patch_rows: usize,
    pub invalid_rows: Vec<RowIssue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RowIssue {
    pub line: usize,
    pub id: Option<String>,
    pub message: String,
}

pub fn audit_jsonl(path: &Path, expectations: &AuditExpectations) -> Result<DatasetAudit> {
    let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    let sha256 = hex_sha256(&bytes);
    let text = String::from_utf8(bytes).context("dataset is not UTF-8")?;

    let mut seen_ids = BTreeSet::new();
    let mut split_counts = BTreeMap::new();
    let mut invalid_rows = Vec::new();
    let mut total_rows = 0usize;
    let mut valid_patch_rows = 0usize;

    for (idx, raw_line) in text.lines().enumerate() {
        let line_no = idx + 1;
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        total_rows += 1;

        let value = match serde_json::from_str::<Value>(line) {
            Ok(value) => value,
            Err(e) => {
                invalid_rows.push(RowIssue {
                    line: line_no,
                    id: None,
                    message: format!("invalid row JSON: {e}"),
                });
                continue;
            }
        };

        let id = value.get("id").and_then(|v| v.as_str()).map(str::to_string);
        let Some(id_str) = id.clone() else {
            invalid_rows.push(RowIssue {
                line: line_no,
                id,
                message: "missing id".into(),
            });
            continue;
        };
        if !seen_ids.insert(id_str.clone()) {
            anyhow::bail!("duplicate id {id_str:?} at line {line_no}");
        }

        if value
            .get("prompt")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .is_empty()
        {
            invalid_rows.push(RowIssue {
                line: line_no,
                id: Some(id_str.clone()),
                message: "missing prompt".into(),
            });
        }

        let split = split_for(&value);
        match split {
            Some(split) => {
                *split_counts.entry(split.to_string()).or_insert(0) += 1;
            }
            None => invalid_rows.push(RowIssue {
                line: line_no,
                id: Some(id_str.clone()),
                message: "missing split".into(),
            }),
        }

        match patch_response(&value).and_then(validate_patch_value) {
            Ok(()) => valid_patch_rows += 1,
            Err(e) => invalid_rows.push(RowIssue {
                line: line_no,
                id: Some(id_str),
                message: format!("invalid patch: {e}"),
            }),
        }
    }

    if let Some(expected) = expectations.rows {
        if total_rows != expected {
            anyhow::bail!("expected {expected} row(s), found {total_rows}");
        }
    }
    for (split, expected) in &expectations.splits {
        let got = split_counts.get(split).copied().unwrap_or(0);
        if got != *expected {
            anyhow::bail!("expected split {split:?} to have {expected} row(s), found {got}");
        }
    }

    if !invalid_rows.is_empty() {
        anyhow::bail!(
            "dataset audit found {} invalid row(s); first issue: line {}: {}",
            invalid_rows.len(),
            invalid_rows[0].line,
            invalid_rows[0].message
        );
    }

    Ok(DatasetAudit {
        schema_version: "training-data-audit-v1".into(),
        path: path.display().to_string(),
        dataset_sha256: sha256.clone(),
        sha256,
        total_rows,
        record_count: total_rows,
        unique_ids: seen_ids.len(),
        split_counts,
        valid_patch_rows,
        invalid_rows,
    })
}

pub fn write_audit_json(audit: &DatasetAudit, out: &Path) -> Result<()> {
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(out, serde_json::to_string_pretty(audit)?)?;
    Ok(())
}

fn split_for(value: &Value) -> Option<&str> {
    value
        .get("split")
        .and_then(|v| v.as_str())
        .or_else(|| value.pointer("/metadata/split").and_then(|v| v.as_str()))
}

fn patch_response(value: &Value) -> Result<Value> {
    let response = value
        .get("response")
        .and_then(|v| v.as_str())
        .context("missing response")?;
    let parsed: Value = serde_json::from_str(response).context("response is not JSON")?;
    Ok(parsed.get("patch").cloned().unwrap_or(parsed))
}

fn validate_patch_value(value: Value) -> Result<()> {
    for field in [
        "start_line",
        "start_char",
        "end_line",
        "end_char",
        "new_text",
        "rationale",
    ] {
        if value.get(field).is_none() {
            anyhow::bail!("missing {field}");
        }
    }
    for field in ["start_line", "start_char", "end_line", "end_char"] {
        if value.get(field).and_then(|v| v.as_u64()).is_none() {
            anyhow::bail!("{field} must be an unsigned integer");
        }
    }
    if value.get("new_text").and_then(|v| v.as_str()).is_none() {
        anyhow::bail!("new_text must be a string");
    }
    if value.get("rationale").and_then(|v| v.as_str()).is_none() {
        anyhow::bail!("rationale must be a string");
    }
    Ok(())
}

fn hex_sha256(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    h.finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::io::Write;

    fn write_jsonl(lines: &[&str]) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data.jsonl");
        let mut file = std::fs::File::create(&path).unwrap();
        for line in lines {
            writeln!(file, "{line}").unwrap();
        }
        (dir, path)
    }

    #[test]
    fn audit_accepts_valid_proof_repair_sft_jsonl() {
        let (_dir, path) = write_jsonl(&[
            r#"{"id":"a","prompt":"Diagnostic A","response":"{\"start_line\":0,\"start_char\":1,\"end_line\":0,\"end_char\":2,\"new_text\":\"simp\",\"rationale\":\"fix A\"}","split":"train"}"#,
            r#"{"id":"b","prompt":"Diagnostic B","response":"{\"patch\":{\"start_line\":1,\"start_char\":0,\"end_line\":1,\"end_char\":4,\"new_text\":\"exact h\",\"rationale\":\"fix B\"}}","metadata":{"split":"eval"}}"#,
        ]);
        let mut splits = BTreeMap::new();
        splits.insert("train".to_string(), 1);
        splits.insert("eval".to_string(), 1);
        let audit = audit_jsonl(
            &path,
            &AuditExpectations {
                rows: Some(2),
                splits,
            },
        )
        .unwrap();

        assert_eq!(audit.total_rows, 2);
        assert_eq!(audit.unique_ids, 2);
        assert_eq!(audit.valid_patch_rows, 2);
        assert!(audit.invalid_rows.is_empty());
        assert_eq!(audit.split_counts["train"], 1);
        assert_eq!(audit.split_counts["eval"], 1);
        assert_eq!(audit.sha256.len(), 64);
    }

    #[test]
    fn audit_rejects_duplicate_ids() {
        let (_dir, path) = write_jsonl(&[
            r#"{"id":"dup","prompt":"A","response":"{\"start_line\":0,\"start_char\":0,\"end_line\":0,\"end_char\":1,\"new_text\":\"simp\",\"rationale\":\"fix\"}","split":"train"}"#,
            r#"{"id":"dup","prompt":"B","response":"{\"start_line\":0,\"start_char\":0,\"end_line\":0,\"end_char\":1,\"new_text\":\"simp\",\"rationale\":\"fix\"}","split":"train"}"#,
        ]);
        let err = audit_jsonl(&path, &AuditExpectations::default()).unwrap_err();
        assert!(err.to_string().contains("duplicate id"), "{err}");
    }

    #[test]
    fn audit_rejects_unparseable_patch_response() {
        let (_dir, path) =
            write_jsonl(&[r#"{"id":"bad","prompt":"A","response":"not json","split":"train"}"#]);
        let err = audit_jsonl(&path, &AuditExpectations::default()).unwrap_err();
        assert!(err.to_string().contains("invalid patch"), "{err}");
    }
}
