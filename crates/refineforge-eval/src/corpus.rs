//! JSONL corpus loader.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorpusEntry {
    /// Stable identifier — used in reports.
    pub id: String,
    /// Existing claim that defines the Lean module path + theorem
    /// list. The runner uses this to know which file to overwrite
    /// with the broken version.
    pub claim_id: String,
    /// Tag describing the mutation (e.g. "swap_lemma", "delete_proof").
    /// See `docs/repair-evaluation.md` §3 for the taxonomy.
    pub mutation: String,
    /// Path (relative to project root) to the broken Lean source.
    pub broken_file: String,
    /// Path (relative to project root) to the ground-truth fixed
    /// source. Currently informational only — false-fix detection
    /// is documented as future work.
    #[serde(default)]
    pub fixed_file: Option<String>,
    /// Optional notes shown in the report.
    #[serde(default)]
    pub notes: Option<String>,
}

pub fn load(path: &Path) -> Result<Vec<CorpusEntry>> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let entry: CorpusEntry = serde_json::from_str(line)
            .with_context(|| format!("parsing line {} of {}", i + 1, path.display()))?;
        out.push(entry);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn loads_minimal_jsonl() {
        let dir = tempdir();
        let path = dir.path().join("c.jsonl");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(
            f,
            "{{\"id\":\"a\",\"claim_id\":\"X\",\"mutation\":\"m\",\"broken_file\":\"b.lean\"}}"
        )
        .unwrap();
        writeln!(f, "# this is a comment").unwrap();
        writeln!(f).unwrap();
        writeln!(f, "{{\"id\":\"b\",\"claim_id\":\"Y\",\"mutation\":\"n\",\"broken_file\":\"c.lean\",\"fixed_file\":\"d.lean\"}}").unwrap();
        drop(f);

        let entries = load(&path).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].id, "a");
        assert_eq!(entries[1].id, "b");
        assert_eq!(entries[1].fixed_file.as_deref(), Some("d.lean"));
    }

    fn tempdir() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }
}
