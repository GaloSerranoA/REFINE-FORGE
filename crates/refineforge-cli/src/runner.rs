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
use std::path::Path;
use std::process::Command;

use crate::claim::{self, Claim};
use crate::report::{ProofReport, ProofStatus};
use crate::sorry_gate;

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

    let lake = Command::new("lake")
        .arg("build")
        .current_dir(&lean_dir)
        .output();

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
