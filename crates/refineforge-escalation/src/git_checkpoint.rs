//! Git-aware checkpoint: commit a [`crate::Packet`] markdown
//! file, then wait for the operator to commit a decision into
//! it.
//!
//! Per criteria-doc v0.3: the wait is **indefinite** — there is
//! no auto-reject timer. Optional 7/14/30-day reminder
//! notifications are documented but deferred to the Phase 3
//! driver crate (this module ships the polling primitive).
//!
//! The [`GitOps`] trait abstracts the underlying VCS so unit
//! tests can drive the loop deterministically via
//! [`MockGitOps`]. Production uses [`SubprocessGitOps`] which
//! shells out to the system `git` binary.

use crate::decision_outcome::{parse_decision, DecisionOutcome, DecisionParseError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use thiserror::Error;

/// Opaque commit SHA from the underlying VCS.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CommitSha(pub String);

#[derive(Debug, Error)]
pub enum GitCheckpointError {
    #[error("git subprocess failed: {0}")]
    Subprocess(String),
    #[error("io error: {0}")]
    Io(String),
    #[error("packet file missing or unreadable: {0}")]
    PacketMissing(String),
    #[error("could not parse operator's decision: {0}")]
    ParseFailed(String),
}

impl PartialEq for GitCheckpointError {
    fn eq(&self, other: &Self) -> bool {
        format!("{:?}", self) == format!("{:?}", other)
    }
}

/// VCS operations the packet flow needs. Implementations:
/// [`SubprocessGitOps`] for production, [`MockGitOps`] for tests.
pub trait GitOps {
    /// Stage `file_rel` (path relative to `repo_root`), then
    /// create a commit with `message`. Returns the new commit's
    /// SHA.
    fn add_and_commit(
        &self,
        repo_root: &Path,
        file_rel: &Path,
        message: &str,
    ) -> Result<CommitSha, GitCheckpointError>;

    /// Read the current content of `file_rel` (relative to
    /// `repo_root`) from the working tree.
    fn read_file(
        &self,
        repo_root: &Path,
        file_rel: &Path,
    ) -> Result<String, GitCheckpointError>;

    /// Write `content` to `file_rel` under `repo_root` (caller
    /// uses this when staging the initial packet before the
    /// `add_and_commit` call).
    fn write_file(
        &self,
        repo_root: &Path,
        file_rel: &Path,
        content: &str,
    ) -> Result<(), GitCheckpointError>;
}

// =====================================================================
// SubprocessGitOps — production
// =====================================================================

/// Shells out to the system `git` binary. Caller is responsible
/// for ensuring `repo_root` is a valid git repo (e.g. by
/// running `git init` once during driver bootstrap).
#[derive(Debug, Default, Clone)]
pub struct SubprocessGitOps;

impl SubprocessGitOps {
    pub fn new() -> Self {
        Self
    }
}

impl GitOps for SubprocessGitOps {
    fn add_and_commit(
        &self,
        repo_root: &Path,
        file_rel: &Path,
        message: &str,
    ) -> Result<CommitSha, GitCheckpointError> {
        let add = Command::new("git")
            .arg("-C")
            .arg(repo_root)
            .args(["add", "--"])
            .arg(file_rel)
            .output()
            .map_err(|e| GitCheckpointError::Subprocess(format!("git add: {}", e)))?;
        if !add.status.success() {
            return Err(GitCheckpointError::Subprocess(format!(
                "git add failed: {}",
                String::from_utf8_lossy(&add.stderr)
            )));
        }

        let commit = Command::new("git")
            .arg("-C")
            .arg(repo_root)
            .args(["commit", "-m", message])
            .output()
            .map_err(|e| GitCheckpointError::Subprocess(format!("git commit: {}", e)))?;
        if !commit.status.success() {
            return Err(GitCheckpointError::Subprocess(format!(
                "git commit failed: {}",
                String::from_utf8_lossy(&commit.stderr)
            )));
        }

        let rev = Command::new("git")
            .arg("-C")
            .arg(repo_root)
            .args(["rev-parse", "HEAD"])
            .output()
            .map_err(|e| GitCheckpointError::Subprocess(format!("git rev-parse: {}", e)))?;
        if !rev.status.success() {
            return Err(GitCheckpointError::Subprocess(format!(
                "git rev-parse failed: {}",
                String::from_utf8_lossy(&rev.stderr)
            )));
        }
        let sha = String::from_utf8_lossy(&rev.stdout).trim().to_string();
        Ok(CommitSha(sha))
    }

    fn read_file(
        &self,
        repo_root: &Path,
        file_rel: &Path,
    ) -> Result<String, GitCheckpointError> {
        let full = repo_root.join(file_rel);
        std::fs::read_to_string(&full)
            .map_err(|e| GitCheckpointError::PacketMissing(format!("{}: {}", full.display(), e)))
    }

    fn write_file(
        &self,
        repo_root: &Path,
        file_rel: &Path,
        content: &str,
    ) -> Result<(), GitCheckpointError> {
        let full = repo_root.join(file_rel);
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| GitCheckpointError::Io(format!("mkdir {}: {}", parent.display(), e)))?;
        }
        std::fs::write(&full, content)
            .map_err(|e| GitCheckpointError::Io(format!("write {}: {}", full.display(), e)))
    }
}

// =====================================================================
// MockGitOps — testing
// =====================================================================

/// In-memory fake git for unit tests. Commits are append-only;
/// `read_file` returns the latest content; tests can mutate the
/// content directly via [`MockGitOps::set_file_content`].
#[derive(Debug, Default, Clone)]
pub struct MockGitOps {
    files: Arc<Mutex<HashMap<PathBuf, String>>>,
    commits: Arc<Mutex<Vec<MockCommit>>>,
    /// When `Some(reason)`, every `write_file` call substitutes
    /// `(pending)` in the written content with `APPROVED: <reason>`.
    /// Used by EXAMPLE-002-style dogfood tests to simulate operator
    /// approval without a real commit-then-edit dance.
    auto_approve_reason: Arc<Mutex<Option<String>>>,
}

#[derive(Debug, Clone)]
pub struct MockCommit {
    pub file: PathBuf,
    pub message: String,
    pub sha: String,
}

impl MockGitOps {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_file_content(&self, file_rel: &Path, content: impl Into<String>) {
        self.files
            .lock()
            .unwrap()
            .insert(file_rel.to_path_buf(), content.into());
    }

    /// Test-only mode: every subsequent `write_file` call
    /// rewrites the content's `(pending)` marker with
    /// `APPROVED: <reason>`. Lets tests simulate "operator
    /// approves the packet between commit and next poll."
    pub fn auto_approve_packets(&self, reason: impl Into<String>) {
        *self.auto_approve_reason.lock().unwrap() = Some(reason.into());
    }

    pub fn commits(&self) -> Vec<MockCommit> {
        self.commits.lock().unwrap().clone()
    }
}

impl GitOps for MockGitOps {
    fn add_and_commit(
        &self,
        _repo_root: &Path,
        file_rel: &Path,
        message: &str,
    ) -> Result<CommitSha, GitCheckpointError> {
        let mut commits = self.commits.lock().unwrap();
        let sha = format!("mock-{:08x}", commits.len() as u32 + 1);
        commits.push(MockCommit {
            file: file_rel.to_path_buf(),
            message: message.to_string(),
            sha: sha.clone(),
        });
        Ok(CommitSha(sha))
    }

    fn read_file(
        &self,
        _repo_root: &Path,
        file_rel: &Path,
    ) -> Result<String, GitCheckpointError> {
        self.files
            .lock()
            .unwrap()
            .get(file_rel)
            .cloned()
            .ok_or_else(|| GitCheckpointError::PacketMissing(file_rel.display().to_string()))
    }

    fn write_file(
        &self,
        _repo_root: &Path,
        file_rel: &Path,
        content: &str,
    ) -> Result<(), GitCheckpointError> {
        let effective = if let Some(reason) = self.auto_approve_reason.lock().unwrap().as_ref()
        {
            content.replace("(pending)", &format!("APPROVED: {}", reason))
        } else {
            content.to_string()
        };
        self.files
            .lock()
            .unwrap()
            .insert(file_rel.to_path_buf(), effective);
        Ok(())
    }
}

// =====================================================================
// Public flow: commit_packet + poll_decision_once + await_decision
// =====================================================================

/// Configuration for the `await_decision` poll loop. Per
/// criteria-doc v0.3, the loop has **no timeout** — only the
/// polling cadence is configurable. The 7/14/30-day reminder
/// hooks are deferred to the Phase 3 driver crate.
#[derive(Debug, Clone)]
pub struct AwaitConfig {
    pub poll_interval: Duration,
}

impl Default for AwaitConfig {
    fn default() -> Self {
        Self {
            poll_interval: Duration::from_secs(5),
        }
    }
}

/// Write the packet markdown into `escalations/<file_rel>` and
/// commit it. Returns the commit SHA so the driver can record
/// it in its run log.
pub fn commit_packet<G: GitOps>(
    git: &G,
    repo_root: &Path,
    file_rel: &Path,
    packet_markdown: &str,
    message: &str,
) -> Result<CommitSha, GitCheckpointError> {
    git.write_file(repo_root, file_rel, packet_markdown)?;
    git.add_and_commit(repo_root, file_rel, message)
}

/// One non-blocking poll. Returns:
/// - `Ok(Some(outcome))` if the operator has filled in the
///   `## Human decision` section with a recognised verdict.
/// - `Ok(None)` if the section is still pending OR the file
///   doesn't yet contain the section heading.
/// - `Err(_)` for I/O or parse-format errors.
pub fn poll_decision_once<G: GitOps>(
    git: &G,
    repo_root: &Path,
    file_rel: &Path,
) -> Result<Option<DecisionOutcome>, GitCheckpointError> {
    let content = git.read_file(repo_root, file_rel)?;
    match parse_decision(&content) {
        Ok(outcome) => Ok(Some(outcome)),
        Err(DecisionParseError::Pending) | Err(DecisionParseError::MissingSection) => Ok(None),
        Err(e) => Err(GitCheckpointError::ParseFailed(e.to_string())),
    }
}

/// Block until the operator's decision is parsable. **No
/// timeout** — per criteria-doc v0.3, visible failure (the
/// claim sits blocking) is preferred over silent failure
/// (a stale packet auto-rejected after N days).
///
/// Operators wanting visibility into pending packets use
/// `refine escalations list` (Phase 3 CLI surface).
pub fn await_decision<G: GitOps>(
    git: &G,
    repo_root: &Path,
    file_rel: &Path,
    config: AwaitConfig,
) -> Result<DecisionOutcome, GitCheckpointError> {
    loop {
        if let Some(outcome) = poll_decision_once(git, repo_root, file_rel)? {
            return Ok(outcome);
        }
        std::thread::sleep(config.poll_interval);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decision_outcome::DecisionOutcome;
    use std::path::PathBuf;

    #[test]
    fn mock_git_add_and_commit_returns_unique_sha() {
        let g = MockGitOps::new();
        let s1 = g
            .add_and_commit(Path::new(""), Path::new("a.md"), "first")
            .unwrap();
        let s2 = g
            .add_and_commit(Path::new(""), Path::new("b.md"), "second")
            .unwrap();
        assert_ne!(s1, s2);
    }

    #[test]
    fn mock_git_write_then_read_roundtrip() {
        let g = MockGitOps::new();
        let p: PathBuf = "escalations/X/packet.md".into();
        g.write_file(Path::new(""), &p, "hello\n").unwrap();
        let r = g.read_file(Path::new(""), &p).unwrap();
        assert_eq!(r, "hello\n");
    }

    #[test]
    fn mock_git_read_missing_errors() {
        let g = MockGitOps::new();
        let err = g
            .read_file(Path::new(""), Path::new("nope.md"))
            .unwrap_err();
        assert!(matches!(err, GitCheckpointError::PacketMissing(_)));
    }

    #[test]
    fn commit_packet_writes_and_commits() {
        let g = MockGitOps::new();
        let p: PathBuf = "escalations/EXAMPLE-002/packet.md".into();
        let sha = commit_packet(&g, Path::new(""), &p, "# hi\n", "escalation: idealisation")
            .unwrap();
        assert!(sha.0.starts_with("mock-"));
        let commits = g.commits();
        assert_eq!(commits.len(), 1);
        assert_eq!(commits[0].message, "escalation: idealisation");
        assert_eq!(g.read_file(Path::new(""), &p).unwrap(), "# hi\n");
    }

    #[test]
    fn poll_pending_returns_none() {
        let g = MockGitOps::new();
        let p: PathBuf = "p.md".into();
        g.set_file_content(
            &p,
            "## Human decision\n\n<!-- comment -->\n(pending)\n",
        );
        let r = poll_decision_once(&g, Path::new(""), &p).unwrap();
        assert!(r.is_none());
    }

    #[test]
    fn poll_missing_section_returns_none() {
        let g = MockGitOps::new();
        let p: PathBuf = "p.md".into();
        g.set_file_content(&p, "# Some packet\n\nno decision heading\n");
        let r = poll_decision_once(&g, Path::new(""), &p).unwrap();
        assert!(r.is_none());
    }

    #[test]
    fn poll_approved_returns_outcome() {
        let g = MockGitOps::new();
        let p: PathBuf = "p.md".into();
        g.set_file_content(&p, "## Human decision\n\nAPPROVED: ok\n");
        let r = poll_decision_once(&g, Path::new(""), &p).unwrap();
        assert_eq!(
            r,
            Some(DecisionOutcome::Approved {
                reason: Some("ok".into())
            })
        );
    }

    #[test]
    fn poll_rejected_returns_outcome() {
        let g = MockGitOps::new();
        let p: PathBuf = "p.md".into();
        g.set_file_content(&p, "## Human decision\n\nREJECTED: no good\n");
        let r = poll_decision_once(&g, Path::new(""), &p).unwrap();
        assert_eq!(
            r,
            Some(DecisionOutcome::Rejected {
                reason: "no good".into()
            })
        );
    }

    #[test]
    fn poll_partial_returns_partial_outcome() {
        let g = MockGitOps::new();
        let p: PathBuf = "p.md".into();
        g.set_file_content(
            &p,
            "## Human decision\n\nAPPROVED: 1-3; REJECTED: 4 [too lossy]\n",
        );
        let r = poll_decision_once(&g, Path::new(""), &p).unwrap();
        assert!(matches!(r, Some(DecisionOutcome::Partial(_))));
    }

    #[test]
    fn poll_unrecognised_verdict_errors() {
        let g = MockGitOps::new();
        let p: PathBuf = "p.md".into();
        g.set_file_content(&p, "## Human decision\n\nWAITING_ON_LEGAL: hmm\n");
        let r = poll_decision_once(&g, Path::new(""), &p);
        assert!(matches!(r, Err(GitCheckpointError::ParseFailed(_))));
    }

    #[test]
    fn await_config_default_is_5s() {
        let c = AwaitConfig::default();
        assert_eq!(c.poll_interval, Duration::from_secs(5));
    }
}
