# Plan 4 - DevOps Agent Production Proof Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: use `superpowers:executing-plans` to implement this plan task-by-task. Do not use delegated agents unless the operator explicitly requests them in the current turn.

**Goal:** Raise the DevOps agent from `release-ready-local` to remotely evidenced, signed, reproducible, reviewer-approved release production proof.

**Architecture:** Keep `refine release ready` as the local truth engine and make remote CI artifacts, signed bundles, SBOM/provenance, verifier container, Nix reproducibility, architecture coverage, and human approval explicit production-proof requirements. The agent may reach `release-ready-ci` only from hosted CI evidence and `human-reviewed` only from review evidence.

**Tech Stack:** Rust, GitHub Actions, Docker, Sigstore `cosign`, Nix, CycloneDX-style SBOM, in-toto/SLSA-style provenance, PowerShell, POSIX shell.

---

## 1. Current Level

Current live level:

| Agent | Status | Trust |
|---|---|---|
| DevOps | `passed` | `release-ready-local` |

Meaning: local release readiness and evidence generation work. It does not prove hosted CI, live signing, container publishing, cloud deployment, or customer access.

## 2. Production-Proof Target

The DevOps agent reaches enterprise production proof only when:

- `refine agent devops --mode execute --target <semver> --allow-expensive` passes locally.
- Hosted GitHub Actions release workflow passes and uploads evidence artifacts.
- Sigstore keyless signing runs from GitHub OIDC and verification records signer identity.
- SBOM and provenance artifacts are uploaded and hash-linked to release bundles.
- Verifier container is built, smoke-tested, and digest-recorded.
- Nix flake is locked and `nix flake check --no-update-lock-file` passes.
- OS and CPU architecture are recorded in release evidence.
- Human release approval is present and recorded.

## 3. File Map

- Modify: `crates/refineforge-cli/src/agent/devops.rs`
  - Add CI evidence ingestion and production-proof requirements.
- Modify: `crates/refineforge-cli/src/release.rs`
  - Add architecture, workflow URL, artifact hash, container digest, and signing fields.
- Modify: `crates/refineforge-cli/src/bundle.rs`
  - Extract signer identity from cosign evidence or record exact reporting gap.
- Modify: `.github/workflows/ci.yml`
  - Upload production-proof evidence pack.
- Modify: `release/release.sh`
- Modify: `release/release.ps1`
- Modify: `schemas/agent-report.schema.json`
- Test: `crates/refineforge-cli/tests/release_cli.rs`
- Test: `crates/refineforge-cli/tests/agent_cli.rs`
- Create: `docs/release/devops-production-proof.md`

## 4. Work Breakdown

### Task 1 - Freeze DevOps Trust Boundaries

**Files:**
- Test: `crates/refineforge-cli/tests/agent_cli.rs`
- Modify: `crates/refineforge-cli/src/agent/devops.rs`

- [ ] **Step 1: Add hosted-CI boundary test**

Add:

```rust
#[test]
fn agent_devops_local_execute_cannot_claim_release_ready_ci() {
    let td = tempfile::tempdir().unwrap();
    let out = td.path().join("devops-local");
    let output = run_refine(
        &["agent", "devops", "--mode", "execute", "--target", "0.2.2"],
        &out,
    );
    assert_success(&output);
    let report = read_json(&out.join("devops.json"));
    assert_eq!(report["trust_level"], "release-ready-local");
    assert_ne!(report["trust_level"], "release-ready-ci");
    assert_eq!(report["production_proof"]["status"], "blocked");
}
```

- [ ] **Step 2: Run the test**

Run:

```powershell
cargo test -p refineforge-cli --test agent_cli agent_devops_local_execute_cannot_claim_release_ready_ci
```

Expected before production-proof implementation: fails only if `production_proof` is absent. After Plan 3 Task 1, it should pass once DevOps fills blockers.

- [ ] **Step 3: Add DevOps requirements**

In `devops.rs`, production-proof requirement ids must be:

```text
devops.local_release_ready
devops.hosted_ci_artifacts
devops.sigstore_oidc_signature
devops.verifier_container_digest
devops.sbom_provenance_uploaded
devops.nix_locked_check
devops.architecture_matrix
devops.human_release_approval
```

Local runs must mark hosted CI, Sigstore OIDC, and approval as blocked unless explicit evidence files are provided.

### Task 2 - Add CI Evidence Pack

**Files:**
- Modify: `.github/workflows/ci.yml`
- Modify: `crates/refineforge-cli/src/release.rs`
- Test: `crates/refineforge-cli/tests/release_cli.rs`

- [ ] **Step 1: Add release evidence fields test**

Add a test asserting `release-report.json` contains:

```json
{
  "environment": {
    "runner_os": "string",
    "runner_arch": "string",
    "rustc_verbose_version": "string"
  },
  "artifacts": {
    "sbom_sha256": "string",
    "provenance_sha256": "string",
    "verifier_container_digest": "string|null"
  }
}
```

- [ ] **Step 2: Implement fields in release report**

In `release.rs`, collect:

```rust
std::env::var("RUNNER_OS").ok();
std::env::var("RUNNER_ARCH").ok();
std::process::Command::new("rustc").arg("-Vv").output();
```

Compute SHA-256 for generated `sbom.cyclonedx.json` and `provenance.intoto.json`.

- [ ] **Step 3: Update workflow artifact upload**

In `.github/workflows/ci.yml`, ensure the release job uploads:

```yaml
- name: Upload release evidence
  uses: actions/upload-artifact@v4
  with:
    name: refineforge-release-evidence-${{ matrix.os }}
    path: release/evidence/**
```

- [ ] **Step 4: Run release tests**

Run:

```powershell
cargo test -p refineforge-cli --test release_cli
```

Expected: all release CLI tests pass.

### Task 3 - Close Sigstore Evidence

**Files:**
- Modify: `crates/refineforge-cli/src/bundle.rs`
- Modify: `.github/workflows/ci.yml`
- Modify: `docs/security.md`
- Test: `crates/refineforge-cli/src/bundle.rs`

- [ ] **Step 1: Add signer identity fixture test**

Create a fixture string in the bundle tests that contains the expected GitHub workflow identity and assert extraction returns:

```text
https://github.com/<owner>/<repo>/.github/workflows/ci.yml@refs/tags/<tag>
```

- [ ] **Step 2: Implement identity parser**

Parse cosign JSON evidence by reading certificate SAN identities from the verified bundle output. If cosign output lacks the identity, return `None` and keep the report blocked for production proof.

- [ ] **Step 3: Run bundle tests**

Run:

```powershell
cargo test -p refineforge-cli bundle::signature_tests
```

Expected: signer identity fixture and existing cosign tests pass.

### Task 4 - Human Release Approval

**Files:**
- Create: `docs/release/devops-production-proof.md`
- Modify: `crates/refineforge-cli/src/agent/devops.rs`

- [ ] **Step 1: Create approval checklist**

Create:

```markdown
# DevOps Production Proof Checklist

The DevOps agent may emit `human-reviewed` only when the release evidence pack includes:

| Requirement | Evidence |
|---|---|
| Local release readiness | `release-report.json` |
| Hosted CI pass | workflow URL and uploaded artifact name |
| Signed bundles | cosign verification report and signer identity |
| SBOM | `sbom.cyclonedx.json` and SHA-256 |
| Provenance | `provenance.intoto.json` and SHA-256 |
| Verifier container | image digest and smoke-test log |
| Nix reproducibility | `flake.lock` and `nix flake check --no-update-lock-file` log |
| Architecture coverage | runner OS and CPU architecture records |
| Human approval | named reviewer, date, release version, decision |
```

- [ ] **Step 2: Link checklist in report**

Add `docs/release/devops-production-proof.md` to DevOps agent artifacts.

## 5. Acceptance Gate

Run locally:

```powershell
cargo clippy -p refineforge-cli --all-targets -- -D warnings
cargo test -p refineforge-cli --test release_cli
cargo test -p refineforge-cli --test agent_cli agent_devops_default_report_cannot_claim_ci_or_live_signing
cargo run -p refineforge-cli --bin refine -- --root . agent devops --mode execute --target 0.2.2 --out agent-reports/devops-prod --json
```

Expected local final state: DevOps passes as `release-ready-local`, production proof remains blocked until hosted CI, signing, Nix, container digest, and human approval evidence are present.
