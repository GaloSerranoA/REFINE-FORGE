//! Lean runner.
//!
//! Two gates per claim:
//!   1. Policy gate (sorry/admit/axiom counts on the source).
//!   2. Build gate (`lake build` exit status).
//!
//! Policy gate runs FIRST. If it fails, build is skipped — we don't
//! want a "verified" proof file that contains `sorry`, even if Lake
//! happens to accept it under some elaboration order.
//!
//! Lake invocation is intentionally simple: we run `lake build` in
//! the `lean/` directory and trust its exit code. Stdout/stderr are
//! captured and embedded in the report verbatim (no parsing yet —
//! that's Phase 4 in the build plan).

use anyhow::{anyhow, Context, Result};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use crate::claim::{self, Claim};
use crate::report::{ProofReport, ProofStatus};
use crate::sorry_gate;

struct LakeBuildLock {
    path: PathBuf,
}

impl Drop for LakeBuildLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn lake_build_lock_timeout() -> Duration {
    std::env::var("REFINEFORGE_LAKE_BUILD_LOCK_TIMEOUT_SECS")
        .ok()
        .and_then(|raw| raw.parse::<u64>().ok())
        .filter(|seconds| *seconds > 0)
        .map(Duration::from_secs)
        .unwrap_or_else(|| Duration::from_secs(180))
}

fn acquire_lake_build_lock(lean_dir: &Path) -> Result<LakeBuildLock> {
    acquire_lake_build_lock_with_timeout(lean_dir, lake_build_lock_timeout())
}

fn acquire_lake_check_all_lock(lean_dir: &Path) -> Result<LakeBuildLock> {
    acquire_lake_lock_file_with_timeout(
        lean_dir,
        ".refineforge-lean-check-all.lock",
        lake_build_lock_timeout(),
    )
}

fn acquire_lake_build_lock_with_timeout(
    lean_dir: &Path,
    timeout: Duration,
) -> Result<LakeBuildLock> {
    acquire_lake_lock_file_with_timeout(lean_dir, ".refineforge-lake-build.lock", timeout)
}

fn acquire_lake_lock_file_with_timeout(
    lean_dir: &Path,
    file_name: &str,
    timeout: Duration,
) -> Result<LakeBuildLock> {
    let path = lean_dir.join(file_name);
    let started = Instant::now();

    loop {
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                writeln!(
                    file,
                    "pid={}\nacquired_at={}",
                    std::process::id(),
                    chrono::Utc::now().to_rfc3339()
                )
                .with_context(|| format!("writing Lake build lock {}", path.display()))?;
                return Ok(LakeBuildLock { path });
            }
            Err(err) if is_lake_lock_contention(&err) => {
                if started.elapsed() >= timeout {
                    let existing = std::fs::read_to_string(&path)
                        .unwrap_or_else(|read_err| format!("unreadable lock: {read_err}"));
                    return Err(anyhow!(
                        "timed out after {:?} waiting for Lake build lock at {}; last error: {}; existing lock: {}",
                        timeout,
                        path.display(),
                        err,
                        existing.trim()
                    ));
                }
                thread::sleep(Duration::from_millis(25));
            }
            Err(err) => {
                return Err(anyhow!(
                    "failed to create Lake build lock at {}: {err}",
                    path.display()
                ));
            }
        }
    }
}

fn is_lake_lock_contention(err: &std::io::Error) -> bool {
    matches!(
        err.kind(),
        std::io::ErrorKind::AlreadyExists | std::io::ErrorKind::PermissionDenied
    )
}

pub fn check(root: &Path, claim_id: &str) -> Result<()> {
    let (_, c) = claim::load(root, claim_id)?;
    let report = run(root, &c)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    match report.status {
        ProofStatus::Verified => Ok(()),
        other => Err(anyhow!(
            "claim {} is not verified (status: {:?})",
            claim_id,
            other
        )),
    }
}

pub fn check_all(root: &Path) -> Result<()> {
    let mut any_fail = false;
    let claims = claim::all(root)?;
    if claims.is_empty() {
        println!("(no claims found)");
        return Ok(());
    }
    let lean_dir = root.join("lean");
    let _check_all_lock = if lean_dir.exists() {
        Some(acquire_lake_check_all_lock(&lean_dir)?)
    } else {
        None
    };
    for (_, c) in claims {
        match run(root, &c) {
            Ok(r) => {
                let status_str = format!("{:?}", r.status);
                println!(
                    "{:<22} {:<18} sorries={} admits={} axioms={}",
                    c.claim_id, status_str, r.sorry_count, r.admit_count, r.axiom_count
                );
                if r.status != ProofStatus::Verified {
                    any_fail = true;
                }
            }
            Err(e) => {
                println!("{:<22} ERROR              {}", c.claim_id, e);
                any_fail = true;
            }
        }
    }
    if any_fail {
        Err(anyhow!("one or more claims failed verification"))
    } else {
        Ok(())
    }
}

pub fn run(root: &Path, c: &Claim) -> Result<ProofReport> {
    let lean_dir = root.join("lean");
    if !lean_dir.exists() {
        return Err(anyhow!(
            "lean/ directory not found at {}",
            lean_dir.display()
        ));
    }

    let lean_file = root.join(&c.lean.file);
    let src = std::fs::read_to_string(&lean_file)
        .with_context(|| format!("reading {}", lean_file.display()))?;
    let gate = sorry_gate::check(&src, &c.policy);

    // If policy fails, do not even run lake — the result would be
    // misleading. Report PolicyViolation with empty stdout/stderr.
    if !gate.ok {
        return Ok(ProofReport {
            claim_id: c.claim_id.clone(),
            status: ProofStatus::PolicyViolation,
            sorry_count: gate.sorry_count,
            admit_count: gate.admit_count,
            axiom_count: gate.axiom_count,
            lean_toolchain: c.lean.toolchain.clone(),
            lean_module: c.lean.module.clone(),
            stdout: String::new(),
            stderr: String::new(),
            policy_notes: gate.notes,
            checked_at: chrono::Utc::now(),
        });
    }

    let lake = match acquire_lake_build_lock(&lean_dir) {
        Ok(_lock) => Command::new("lake")
            .arg("build")
            .current_dir(&lean_dir)
            .output(),
        Err(err) => {
            return Ok(ProofReport {
                claim_id: c.claim_id.clone(),
                status: ProofStatus::ToolingError,
                sorry_count: gate.sorry_count,
                admit_count: gate.admit_count,
                axiom_count: gate.axiom_count,
                lean_toolchain: c.lean.toolchain.clone(),
                lean_module: c.lean.module.clone(),
                stdout: String::new(),
                stderr: format!("failed to acquire Lake build lock: {err}"),
                policy_notes: gate.notes,
                checked_at: chrono::Utc::now(),
            });
        }
    };

    let (status, stdout, stderr) = match lake {
        Ok(o) => {
            let s = if o.status.success() {
                ProofStatus::Verified
            } else {
                ProofStatus::BuildFailed
            };
            (
                s,
                String::from_utf8_lossy(&o.stdout).into_owned(),
                String::from_utf8_lossy(&o.stderr).into_owned(),
            )
        }
        Err(e) => (
            ProofStatus::ToolingError,
            String::new(),
            format!("failed to invoke `lake`: {e}"),
        ),
    };

    Ok(ProofReport {
        claim_id: c.claim_id.clone(),
        status,
        sorry_count: gate.sorry_count,
        admit_count: gate.admit_count,
        axiom_count: gate.axiom_count,
        lean_toolchain: c.lean.toolchain.clone(),
        lean_module: c.lean.module.clone(),
        stdout,
        stderr,
        policy_notes: gate.notes,
        checked_at: chrono::Utc::now(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn lake_build_lock_serializes_same_lean_dir() {
        let td = tempfile::tempdir().unwrap();
        let first = acquire_lake_build_lock_with_timeout(td.path(), Duration::from_millis(10))
            .expect("first lock");

        let second = acquire_lake_build_lock_with_timeout(td.path(), Duration::from_millis(10));
        assert!(
            second.is_err(),
            "second holder must wait or fail while the first process holds the Lake build lock"
        );

        drop(first);
        acquire_lake_build_lock_with_timeout(td.path(), Duration::from_millis(10))
            .expect("lock should be acquirable after first holder drops");
    }
}
