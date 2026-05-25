//! Deterministic input-file manifest for bit-exact kernel gates.

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::hash::hash_file;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InputArtifact {
    pub path: String,
    pub sha256: String,
    pub size_bytes: u64,
}

pub fn build_input_manifest(paths: &[PathBuf]) -> Result<Vec<InputArtifact>> {
    let mut sorted = paths.to_vec();
    sorted.sort_by_key(|path| path.display().to_string());

    let mut out = Vec::with_capacity(sorted.len());
    for path in sorted {
        if !path.exists() {
            return Err(anyhow!("input file does not exist: {}", path.display()));
        }
        if !path.is_file() {
            return Err(anyhow!("input path is not a file: {}", path.display()));
        }
        let metadata = std::fs::metadata(&path)
            .with_context(|| format!("reading metadata for {}", path.display()))?;
        let sha256 = hash_file(&path)?;
        out.push(InputArtifact {
            path: path.display().to_string(),
            sha256,
            size_bytes: metadata.len(),
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_file(path: &std::path::Path, bytes: &[u8]) {
        let mut f = std::fs::File::create(path).unwrap();
        f.write_all(bytes).unwrap();
    }

    #[test]
    fn build_input_manifest_hashes_files_in_sorted_order() {
        let td = tempfile::tempdir().unwrap();
        let b = td.path().join("b.bin");
        let a = td.path().join("a.bin");
        write_file(&b, b"bbb");
        write_file(&a, b"aaa");

        let manifest = build_input_manifest(&[b.clone(), a.clone()]).unwrap();

        assert_eq!(manifest.len(), 2);
        assert_eq!(manifest[0].path, a.display().to_string());
        assert_eq!(manifest[0].size_bytes, 3);
        assert_eq!(
            manifest[0].sha256,
            "9834876dcfb05cb167a5c24953eba58c4ac89b1adf57f28f2f9d09af107ee8f0"
        );
        assert_eq!(manifest[1].path, b.display().to_string());
    }

    #[test]
    fn build_input_manifest_rejects_missing_file() {
        let td = tempfile::tempdir().unwrap();
        let missing = td.path().join("missing.bin");
        let err = build_input_manifest(&[missing]).unwrap_err();
        assert!(
            err.to_string().contains("input file does not exist"),
            "{err}"
        );
    }
}
