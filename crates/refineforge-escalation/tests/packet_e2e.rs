//! End-to-end packet flow against a real `git` subprocess in a
//! tempdir. POSIX-only — Windows runners can spawn git too, but
//! identity / config quirks (CRLF, signing, gpg paths) make a
//! single test cover less than it claims to, so we gate this on
//! `cfg(unix)`. The MockGitOps unit tests already cover the
//! logic; this test confirms the SubprocessGitOps wiring
//! actually shells out correctly on at least one platform.

#![cfg(unix)]

use refineforge_escalation::engine::EngineError;
use refineforge_escalation::{
    commit_packet, poll_decision_once, Action, BatchBlock, BatchItem, Category, ClaimSummary,
    DecisionOutcome, Engine, Evidence, LossKind, Packet, ProjectContext, SubprocessGitOps,
};
use std::path::Path;
use std::process::Command;

fn init_repo(dir: &Path) {
    Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["init", "-q", "--initial-branch=main"])
        .status()
        .expect("git init");
    Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["config", "user.email", "test@example.com"])
        .status()
        .expect("git config user.email");
    Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["config", "user.name", "Test"])
        .status()
        .expect("git config user.name");
    Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["config", "commit.gpgsign", "false"])
        .status()
        .expect("git config commit.gpgsign false");
}

fn run_engine_for_u64_to_nat() -> Packet {
    let mut ctx = ProjectContext::test_default();
    ctx.claim = Some(ClaimSummary::test_default("EXAMPLE-002"));
    let action = Action::MapRustToLean {
        rust_type: "u64".into(),
        lean_type: "Nat".into(),
        lossy_kinds: vec![LossKind::UnsignedOverflow],
    };
    let d = Engine::new()
        .decide(&action, &ctx)
        .expect("engine decision");
    let reason = match d {
        refineforge_escalation::Decision::Escalate(r) => r,
        other => panic!("expected Escalate, got {:?}", other),
    };
    Packet::build(
        reason,
        action,
        "EXAMPLE-002",
        "2026-05-18T20:30:45Z",
        "anthropic",
    )
}

#[test]
fn end_to_end_real_git_commit_then_poll_pending_then_approved() {
    let tmp = tempfile::tempdir().expect("tempdir");
    init_repo(tmp.path());

    let git = SubprocessGitOps::new();
    let packet = run_engine_for_u64_to_nat();
    let rel_path: std::path::PathBuf = "escalations/EXAMPLE-002/idealisation.md".into();

    // 1. commit_packet should land a real commit.
    let sha = commit_packet(
        &git,
        tmp.path(),
        &rel_path,
        &packet.to_markdown(),
        "escalation: idealisation for EXAMPLE-002",
    )
    .expect("commit_packet");
    assert_eq!(
        sha.0.len(),
        40,
        "expected full 40-char git sha, got {:?}",
        sha
    );

    // 2. First poll: section says `(pending)` → Ok(None).
    let r = poll_decision_once(&git, tmp.path(), &rel_path).unwrap();
    assert!(r.is_none(), "expected pending, got {:?}", r);

    // 3. Simulate operator commit: rewrite the file with
    //    APPROVED in the decision section.
    let body = std::fs::read_to_string(tmp.path().join(&rel_path)).unwrap();
    let approved = body.replace(
        "(pending)",
        "APPROVED: ok — saturating_add gap documented in refinement doc",
    );
    std::fs::write(tmp.path().join(&rel_path), approved).unwrap();
    Command::new("git")
        .arg("-C")
        .arg(tmp.path())
        .args(["add", "--"])
        .arg(&rel_path)
        .status()
        .unwrap();
    Command::new("git")
        .arg("-C")
        .arg(tmp.path())
        .args(["commit", "-m", "operator approval"])
        .status()
        .unwrap();

    // 4. Second poll: APPROVED detected.
    let r = poll_decision_once(&git, tmp.path(), &rel_path)
        .unwrap()
        .expect("decision should be readable");
    match r {
        DecisionOutcome::Approved { reason } => {
            assert!(reason.as_deref().unwrap_or("").starts_with("ok"));
        }
        other => panic!("expected Approved, got {:?}", other),
    }
}

#[test]
fn end_to_end_real_git_partial_decision_on_batched_packet() {
    let tmp = tempfile::tempdir().expect("tempdir");
    init_repo(tmp.path());

    let git = SubprocessGitOps::new();
    let mut packet = run_engine_for_u64_to_nat();
    packet = packet.with_batch(BatchBlock {
        items: vec![
            BatchItem {
                id: "counter.value".into(),
                summary: "u64 → Nat at value".into(),
                evidence: Evidence::Idealisation {
                    rust_type: "u64".into(),
                    lean_type: "Nat".into(),
                    lost_properties: vec![LossKind::UnsignedOverflow],
                },
            },
            BatchItem {
                id: "counter.timestamp".into(),
                summary: "u64 → Nat at timestamp".into(),
                evidence: Evidence::Idealisation {
                    rust_type: "u64".into(),
                    lean_type: "Nat".into(),
                    lost_properties: vec![LossKind::UnsignedOverflow],
                },
            },
        ],
        rationale_for_batching: "same struct, same trade-off".into(),
    });

    let rel_path: std::path::PathBuf = "escalations/EXAMPLE-002/batched.md".into();
    commit_packet(
        &git,
        tmp.path(),
        &rel_path,
        &packet.to_markdown(),
        "escalation: batched idealisation",
    )
    .expect("commit_packet");

    // Operator partial-approves: APPROVED 1, REJECTED 2.
    let body = std::fs::read_to_string(tmp.path().join(&rel_path)).unwrap();
    let partial = body.replace(
        "(pending)",
        "APPROVED: 1; REJECTED: 2 [timestamps wrap to zero in our threat model]",
    );
    std::fs::write(tmp.path().join(&rel_path), partial).unwrap();
    Command::new("git")
        .arg("-C")
        .arg(tmp.path())
        .args(["add", "--"])
        .arg(&rel_path)
        .status()
        .unwrap();
    Command::new("git")
        .arg("-C")
        .arg(tmp.path())
        .args(["commit", "-m", "operator partial"])
        .status()
        .unwrap();

    let r = poll_decision_once(&git, tmp.path(), &rel_path)
        .unwrap()
        .expect("decision parsable");
    if let DecisionOutcome::Partial(p) = r {
        assert_eq!(p.approved_indices, vec![1]);
        assert!(p.rejected(2));
        assert!(p.rejection_reason(2).unwrap().contains("timestamps wrap"));
    } else {
        panic!("expected Partial, got {:?}", r);
    }
}

#[test]
fn engine_still_refuses_mismatched_criteria_version_in_real_repo_setup() {
    // Defence-in-depth: even in an e2e test, the engine refuses
    // to operate on a context whose criteria_version doesn't
    // match the compiled-in `CRITERIA_VERSION` (currently "0.3").
    let ctx = ProjectContext::test_with_wrong_criteria_version("0.99");
    let action = Action::Reformat {
        paths: vec!["x.rs".into()],
    };
    let res = Engine::new().decide(&action, &ctx);
    assert!(matches!(
        res,
        Err(EngineError::CriteriaVersionMismatch { .. })
    ));
}
