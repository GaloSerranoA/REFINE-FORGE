//! Checkpoint detection + pruning.
//!
//! Convention: backends write checkpoint subdirs named `step-N` or
//! `checkpoint-N` (HF's default). We scan the checkpoint dir, sort by
//! numeric N descending, and let the caller pick the latest or prune
//! older ones beyond `keep_last`.

use anyhow::Result;
use regex::Regex;
use std::cmp::Reverse;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checkpoint {
    pub step: u64,
    pub path: PathBuf,
}

/// Scan `dir` for checkpoint subdirectories, sorted by step descending.
pub fn list_checkpoints(dir: &Path) -> Result<Vec<Checkpoint>> {
    if !dir.exists() {
        return Ok(vec![]);
    }
    let re = Regex::new(r"^(?:step|checkpoint)-(\d+)$").unwrap();
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if let Some(c) = re.captures(&name_str) {
            if let Ok(step) = c[1].parse::<u64>() {
                out.push(Checkpoint {
                    step,
                    path: entry.path(),
                });
            }
        }
    }
    out.sort_by_key(|checkpoint| Reverse(checkpoint.step));
    Ok(out)
}

/// Latest checkpoint or None.
pub fn latest(dir: &Path) -> Result<Option<Checkpoint>> {
    Ok(list_checkpoints(dir)?.into_iter().next())
}

/// Delete checkpoints older than the N most recent. Returns the
/// paths that were removed.
pub fn prune(dir: &Path, keep_last: usize) -> Result<Vec<PathBuf>> {
    let all = list_checkpoints(dir)?;
    if all.len() <= keep_last {
        return Ok(vec![]);
    }
    let mut removed = Vec::new();
    for c in &all[keep_last..] {
        std::fs::remove_dir_all(&c.path)?;
        removed.push(c.path.clone());
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn make_ckpt(parent: &Path, name: &str) {
        let dir = parent.join(name);
        std::fs::create_dir_all(&dir).unwrap();
        let mut f = std::fs::File::create(dir.join("model.bin")).unwrap();
        f.write_all(b"fake-weights").unwrap();
    }

    #[test]
    fn list_handles_missing_dir() {
        let td = tempfile::tempdir().unwrap();
        let missing = td.path().join("does-not-exist");
        let v = list_checkpoints(&missing).unwrap();
        assert!(v.is_empty());
    }

    #[test]
    fn list_sorts_by_step_descending() {
        let td = tempfile::tempdir().unwrap();
        for n in ["step-100", "step-500", "step-50", "checkpoint-200"] {
            make_ckpt(td.path(), n);
        }
        // Add some non-matching dirs that should be ignored.
        std::fs::create_dir_all(td.path().join("random")).unwrap();
        std::fs::create_dir_all(td.path().join("step-abc")).unwrap();

        let v = list_checkpoints(td.path()).unwrap();
        let steps: Vec<u64> = v.iter().map(|c| c.step).collect();
        assert_eq!(steps, vec![500, 200, 100, 50]);
    }

    #[test]
    fn latest_returns_highest_step() {
        let td = tempfile::tempdir().unwrap();
        make_ckpt(td.path(), "step-100");
        make_ckpt(td.path(), "step-300");
        make_ckpt(td.path(), "step-200");
        let c = latest(td.path()).unwrap().unwrap();
        assert_eq!(c.step, 300);
    }

    #[test]
    fn latest_returns_none_for_empty_dir() {
        let td = tempfile::tempdir().unwrap();
        assert!(latest(td.path()).unwrap().is_none());
    }

    #[test]
    fn prune_keeps_top_n() {
        let td = tempfile::tempdir().unwrap();
        for n in 1..=5 {
            make_ckpt(td.path(), &format!("step-{n}00"));
        }
        let removed = prune(td.path(), 2).unwrap();
        assert_eq!(removed.len(), 3); // 5 total, keep 2, remove 3
        let remaining: Vec<u64> = list_checkpoints(td.path())
            .unwrap()
            .into_iter()
            .map(|c| c.step)
            .collect();
        assert_eq!(remaining, vec![500, 400]);
    }

    #[test]
    fn prune_noop_when_under_threshold() {
        let td = tempfile::tempdir().unwrap();
        make_ckpt(td.path(), "step-1");
        make_ckpt(td.path(), "step-2");
        let removed = prune(td.path(), 5).unwrap();
        assert!(removed.is_empty());
    }
}
