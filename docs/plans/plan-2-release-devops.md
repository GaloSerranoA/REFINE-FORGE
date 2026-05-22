# Plan 2 - Release / Infrastructure / DevOps Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: use `superpowers:executing-plans` to implement this plan task-by-task. Do not use delegated agents unless the operator explicitly requests them in the current turn.

**Goal:** Turn Refine-Forge's open-source release infrastructure from local and CI-authored machinery into a remotely evidenced, reproducible, signed, reviewer-ready release path.

**Architecture:** Keep `refine release ready` as the release truth engine and keep CI, release scripts, Docker, Nix, and Sigstore as wrappers around that evidence model. Treat Docker, Nix, cosign, GitHub OIDC, and remote runners as explicit `passed`, `skipped`, `blocked`, or `pending-first-run` states, never as implied success.

**Tech Stack:** Rust, GitHub Actions, Docker, Sigstore `cosign`, Nix flakes, CycloneDX-shaped SBOM JSON, in-toto/SLSA-shaped provenance JSON, POSIX shell, PowerShell.

---

## Execution Status

Executed locally on 2026-05-22. Local release truth inventory, CI audit
blocker report, architecture-evidence workflow updates, signer-identity
fallback regression coverage, and docs links were created. Remote CI,
live Sigstore, Nix, Docker, and artifact download remain blocked by the live
environment: no Git remote output, `gh` not authenticated, and `nix`,
`cosign`, and `docker` not on PATH.

## 1. Operating Rules

- Do not claim live Sigstore signing has run until a real GitHub Actions OIDC run produces bundle signature artifacts and Rekor-backed verification evidence.
- Do not claim reproducible builds are proven until `flake.lock` exists and `nix flake check --no-update-lock-file` passes in a Nix-capable environment.
- Do not infer architecture coverage from runner labels alone. Record `runner.os`, `runner.arch`, `rustc -Vv`, and machine architecture in CI evidence.
- Do not remove the `extract_signer_identity()` gap note until the CLI reports a real signer identity from a fixture or real signed bundle.
- Generated release evidence under `release/evidence/` is not committed by default. Commit only selected release evidence when the operator explicitly requests it.
- Keep hosted SaaS, cloud accounts, billing, customer login, Kubernetes, Terraform, and long-running service monitoring out of this plan.

## 2. Current Snapshot

This snapshot was checked against the repo on 2026-05-22 while revising this plan. Re-run the commands in Section 7 before closing the work.

| Component | Current state | Evidence |
|---|---|---|
| CI workflow | Authored and broad, but remote execution evidence still required | `.github/workflows/ci.yml` includes OS matrix, release readiness dry run, bundle artifacts, verifier-container smoke, Nix job, bit-exact gate, and `sign-bundles` |
| Release readiness command | Implemented locally | `crates/refineforge-cli/src/release.rs` writes `release-report.json`, `release-report.md`, `sbom.cyclonedx.json`, and `provenance.intoto.json` |
| Release scripts | Present and delegated to readiness gate | `release/release.sh`, `release/release.ps1` call `refine release ready` |
| Verifier container | Authored and wired into CI smoke | `containers/Dockerfile.verifier`; CI runs `docker build` and `bundle verify /artifacts/EXAMPLE-003` |
| SBOM/provenance | Implemented as source-controlled baseline generators | `sbom_from_cargo_metadata()` and `provenance_from_report()` in `crates/refineforge-cli/src/release.rs` |
| Docs truth audit | Implemented as a release gate | `audit_docs_in_dir()` and `docs-truth-audit` gate in `crates/refineforge-cli/src/release.rs` |
| Sigstore verification | CLI delegates real verification to `cosign verify-blob`, but identity extraction is still a reporting gap | `crates/refineforge-cli/src/bundle.rs`; `SECURITY.md` says live Fulcio cert verification has not run in this session |
| Sigstore signing | CI job is authored; first real OIDC run still pending | `.github/workflows/ci.yml` `sign-bundles` uses `id-token: write` and `cosign sign-blob` |
| Nix flake | `flake.nix` exists; `flake.lock` does not | repo root has `flake.nix` and no `flake.lock` |
| Architecture coverage | OS coverage exists; explicit architecture evidence is incomplete | `.github/workflows/ci.yml` has `ubuntu-latest`, `macos-latest`, `windows-latest` without an explicit architecture matrix |

## 3. Real Gaps

**G1 - Remote evidence is missing.** The local command surface and CI YAML exist, but a reviewer still needs real GitHub Actions artifacts showing the workflow ran remotely.

**G2 - Sigstore is CI-pending.** The signing path is written, but keyless signing must be proven by a real GitHub OIDC run and a verified Rekor-backed signature.

**G3 - Signer identity is not surfaced.** `extract_signer_identity()` intentionally returns `None`, so reports can only say the identity regex matched, not which SAN identity was extracted.

**G4 - Nix reproducibility is not closed.** `flake.nix` exists, but without `flake.lock` and a passing `nix flake check`, it is not a proven reproducible-build path.

**G5 - Architecture coverage is not explicit.** The CI matrix covers three OS labels, but the release evidence does not yet prove which CPU architecture each lane ran on or whether aarch64 Linux/Darwin release gates are covered.

**G6 - Monitoring means CI audit artifacts, not service uptime.** This phase has no hosted service. The monitoring deliverable is release-gate observability: job summaries, uploaded evidence, signature status, SBOM, provenance, and docs-truth audit output.

## 4. Work Breakdown

### Task 1 - Build a Release Truth Inventory

**Files:**
- Create: `docs/release/release-readiness-inventory.md`
- Read: `.github/workflows/ci.yml`
- Read: `crates/refineforge-cli/src/release.rs`
- Read: `crates/refineforge-cli/src/bundle.rs`
- Read: `release/release.sh`
- Read: `release/release.ps1`
- Read: `SECURITY.md`
- Read: `docs/security.md`
- Read: `docs/reproducible-build.md`

- [ ] **Step 1: Create the inventory document**

Use this exact table structure:

```markdown
# Release Readiness Inventory

> Snapshot date: 2026-05-22
> Purpose: distinguish shipped, locally verified, stub-tested, CI-pending, and planned release infrastructure.

| Surface | Status | Evidence | Remaining closure |
|---|---|---|---|
| `refine release ready` | shipped-local | `crates/refineforge-cli/src/release.rs` | rerun final local gate before release |
| Release reports | shipped-local | `release-report.json`, `release-report.md` generated by CLI | upload from CI and inspect artifact |
| SBOM | shipped-local baseline | `sbom.cyclonedx.json` generated from Cargo metadata | verify generated artifact in release evidence |
| Provenance | shipped-local baseline | `provenance.intoto.json` generated from release report | verify generated artifact in release evidence |
| Verifier container | authored-ci-smoke | `.github/workflows/ci.yml`, `containers/Dockerfile.verifier` | first remote CI container artifact |
| Sigstore signing | ci-pending | `sign-bundles` job uses `cosign sign-blob` | first real GitHub OIDC run |
| Signature verification | stub-tested + cosign delegated | `verify_signature_impl()` calls `cosign verify-blob` | verify a real signed bundle |
| Signer identity extraction | reporting-gap | `extract_signer_identity()` returns `None` | implement extraction or keep explicit gap |
| Nix flake | authored-unlocked | `flake.nix`; no `flake.lock` | lock and run `nix flake check` |
| Architecture coverage | os-matrix-authored | `ubuntu-latest`, `macos-latest`, `windows-latest` | record actual arch and add explicit aarch64 lanes if available |
```

- [ ] **Step 2: Link the inventory from release docs**

Add a short link from `docs/reproducible-build.md` or `docs/security.md`:

```markdown
For the current shipped/stub-tested/CI-pending release infrastructure inventory, see `docs/release/release-readiness-inventory.md`.
```

- [ ] **Step 3: Run docs truth audit**

Run:

```powershell
cargo run -p refineforge-cli --bin refine -- release ready --version 0.2.2 --allow-dirty --skip-docker --skip-signature --evidence-dir release/evidence/plan2-inventory-check
```

Expected: command exits successfully, or reports exact docs-truth issues to fix. Remove generated evidence after inspection:

```powershell
Remove-Item -Recurse -Force release\evidence\plan2-inventory-check
```

### Task 2 - Close or Preserve Signer Identity Honestly

**Files:**
- Modify: `crates/refineforge-cli/src/bundle.rs`
- Modify: `SECURITY.md`
- Modify: `docs/security.md`
- Test: `crates/refineforge-cli/src/bundle.rs`

- [ ] **Step 1: Decide the implementation path**

Use exactly one path:

- **Path A - implement extraction now.** Add a parser backed by a real cosign output fixture or a real sigbundle fixture. The test must assert a concrete signer identity string.
- **Path B - preserve the gap.** Keep `extract_signer_identity()` returning `None`, keep the docs explicit, and make the release report call it a reporting gap rather than a verification bypass.

Recommended path: Path A only if a stable fixture or real signed bundle is available. Otherwise Path B is more honest.

- [ ] **Step 2A: If implementing extraction, add a fixture test first**

Create a fixture under:

```text
crates/refineforge-cli/tests/fixtures/cosign/
```

The test must fail before implementation and must assert:

```rust
assert_eq!(
    extract_signer_identity_from_fixture(...),
    "https://github.com/<owner>/<repo>/.github/workflows/ci.yml@refs/heads/main"
);
```

- [ ] **Step 2B: If preserving the gap, add a regression test for honest fallback**

Keep the existing stub-cosign success path expecting:

```text
(identity matched but couldn't extract)
```

Also assert `SECURITY.md` contains:

```text
This is a reporting gap, not a signature-validation bypass.
```

- [ ] **Step 3: Run targeted signature tests**

Run:

```powershell
cargo test -p refineforge-cli signature_tests
```

Expected: all signature tests pass.

### Task 3 - Lock and Verify Nix Reproducibility

**Files:**
- Modify: `flake.lock`
- Modify only if failures require it: `flake.nix`
- Modify if status changes: `docs/reproducible-build.md`
- Modify if status changes: `SECURITY.md`

- [ ] **Step 1: Check whether Nix is available**

Run:

```powershell
nix --version
```

Expected if available: prints a Nix version.

If Nix is unavailable, record the exact error and do not claim reproducible builds are verified.

- [ ] **Step 2: Generate the lockfile**

Run only when Nix is available:

```powershell
nix flake lock
```

Expected: `flake.lock` appears in the repo root.

- [ ] **Step 3: Run the locked flake check**

Run:

```powershell
nix flake check --no-update-lock-file --print-build-logs
```

Expected: pass. If it fails, preserve the exact failing derivation and error.

- [ ] **Step 4: Build key release outputs**

Run:

```powershell
nix build .#refine --no-update-lock-file --print-build-logs
nix build .#bundle-EXAMPLE-003 --no-update-lock-file --print-build-logs
```

Expected: both builds pass and do not update `flake.lock`.

- [ ] **Step 5: Update docs only after the first locked check passes**

Allowed status language after success:

```markdown
Nix flake first-build verification passed with the committed `flake.lock`.
```

Allowed status language if blocked:

```markdown
Nix flake is authored, but first locked build verification is still blocked or pending.
```

### Task 4 - Make Architecture Coverage Explicit

**Files:**
- Modify: `.github/workflows/ci.yml`
- Modify: `crates/refineforge-cli/tests/release_cli.rs`
- Modify: `docs/release/release-readiness-inventory.md`

- [ ] **Step 1: Add CI architecture evidence**

Add a CI step in each release-relevant job that records:

```yaml
- name: Record runner architecture
  shell: bash
  run: |
    echo "runner.os=${{ runner.os }}" | tee -a "$GITHUB_STEP_SUMMARY"
    echo "runner.arch=${{ runner.arch }}" | tee -a "$GITHUB_STEP_SUMMARY"
    rustc -Vv | tee -a "$GITHUB_STEP_SUMMARY"
    uname -a | tee -a "$GITHUB_STEP_SUMMARY" || true
```

For Windows PowerShell-only steps, use:

```powershell
"runner.os=${{ runner.os }}" | Out-File -FilePath $env:GITHUB_STEP_SUMMARY -Append
"runner.arch=${{ runner.arch }}" | Out-File -FilePath $env:GITHUB_STEP_SUMMARY -Append
rustc -Vv | Out-File -FilePath $env:GITHUB_STEP_SUMMARY -Append
[System.Runtime.InteropServices.RuntimeInformation]::ProcessArchitecture | Out-File -FilePath $env:GITHUB_STEP_SUMMARY -Append
```

- [ ] **Step 2: Add explicit aarch64 lanes only when the runner is real**

Do not add a fake aarch64 row. Use one of these outcomes:

- GitHub-hosted arm runners are available for the repo: add explicit `aarch64-linux` and/or `aarch64-darwin` lanes and run build plus bundle verify.
- No usable arm runner is available: mark aarch64 as `blocked` in release inventory and do not claim coverage.

- [ ] **Step 3: Add a static workflow test**

Add a test that checks `.github/workflows/ci.yml` contains:

```text
Record runner architecture
runner.arch
rustc -Vv
```

- [ ] **Step 4: Run the static workflow test**

Run:

```powershell
cargo test -p refineforge-cli ci_workflow
```

Expected: static workflow tests pass.

### Task 5 - First Remote CI Evidence Run

**Files:**
- No local code edits required unless CI fails due to real repo defects.
- Save downloaded evidence under: `release/evidence/remote-smoke-YYYYMMDD/` only if the operator wants it committed.

- [ ] **Step 1: Confirm remote prerequisites**

Run:

```powershell
git remote -v
gh auth status
```

Expected: a GitHub remote exists and `gh` can access it. If either is missing, mark this task blocked.

- [ ] **Step 2: Trigger the remote workflow**

Use the least invasive operator-approved path:

```powershell
git push origin HEAD:<branch-name>
```

For signing evidence, use a protected branch or tag only after operator approval:

```powershell
git push origin master
git tag -a v0.2.2-rc.1 -m "refineforge v0.2.2-rc.1"
git push origin v0.2.2-rc.1
```

- [ ] **Step 3: Inspect remote run status**

Run:

```powershell
gh run list --limit 5
gh run view <run-id> --log
```

Expected: the relevant run is green, or the exact failed job and failing command are recorded.

- [ ] **Step 4: Download artifacts**

Run:

```powershell
gh run download <run-id> --dir release/evidence/remote-smoke-YYYYMMDD
```

Expected artifacts:

- unsigned bundle artifacts from pull/branch runs;
- release evidence artifacts containing `release-report.json`, `release-report.md`, `sbom.cyclonedx.json`, and `provenance.intoto.json`;
- signed bundle artifacts from `main` or `v*` push runs only;
- GitHub job summary containing runner architecture evidence.

### Task 6 - First Live Sigstore Verification

**Files:**
- Read: downloaded signed bundle artifact directory from Task 5
- Modify if needed: `SECURITY.md`
- Modify if needed: `docs/security.md`
- Modify if needed: `crates/refineforge-cli/src/bundle.rs`

- [ ] **Step 1: Verify cosign is available**

Run:

```powershell
cosign version --json
```

Expected: cosign prints JSON version output. If missing, install or mark local live verification blocked.

- [ ] **Step 2: Verify a signed bundle**

Run against a signed bundle produced by the remote signing job:

```powershell
cargo run -p refineforge-cli --bin refine -- bundle verify <signed-bundle-dir> --verify-signature
```

Expected:

- raw bundle hashes pass;
- `cosign verify-blob` accepts the signature;
- the cert chain roots in Fulcio;
- Rekor inclusion proof is accepted;
- signer identity is either extracted or explicitly reported as matched-but-not-extracted.

- [ ] **Step 3: Update security docs only with real evidence**

Allowed after success:

```markdown
At least one GitHub OIDC-produced bundle signature has been verified with `cosign verify-blob`; see the linked CI artifact/run id.
```

Do not write a statement that treats one successful signing run as a permanent guarantee for all future releases.

### Task 7 - CI Monitoring and Audit Report

**Files:**
- Create: `docs/release/ci-audit-report.md`
- Modify if useful: `crates/refineforge-cli/src/release.rs`
- Modify if useful: `.github/workflows/ci.yml`

- [ ] **Step 1: Create the CI audit report**

Use this exact structure:

```markdown
# CI Audit Report

> Snapshot date: YYYY-MM-DD
> Remote run id: <run-id or blocked>

| Gate | Local evidence | CI evidence | Status | Notes |
|---|---|---|---|---|
| Rust tests | `cargo test -p refineforge-cli` | `<run-url>` | passed/failed/blocked | exact failure if any |
| Lean gate | `refine lean check-all` | `<run-url>` | passed/failed/blocked | exact failure if any |
| Scan gate | `refine scan check-all` | `<run-url>` | passed/failed/blocked | exact failure if any |
| Lint gate | `refine lint check-all` | `<run-url>` | passed/failed/blocked | exact failure if any |
| Bundle export/verify | EXAMPLE bundles | `<artifact-name>` | passed/failed/blocked | manifest hashes |
| Release evidence | `release-report.*` | `<artifact-name>` | passed/failed/blocked | SBOM/provenance present |
| Verifier container | Docker local or skipped | `<run-url>` | passed/failed/blocked | image build + smoke |
| Nix | `nix flake check` | `<run-url>` | passed/failed/blocked | lockfile state |
| Sigstore | local cosign verify | `<artifact-name>` | passed/failed/blocked | signer identity / Rekor |
| Architecture | local host arch | job summary | passed/failed/blocked | runner.arch values |
```

- [ ] **Step 2: Ensure CI uploads the audit-relevant artifacts**

The workflow must upload:

- bundle artifacts;
- release evidence directory;
- signed bundles on signing runs;
- logs or job summaries that show architecture;
- Nix logs on failure when available.

- [ ] **Step 3: Run docs truth audit again**

Run:

```powershell
cargo run -p refineforge-cli --bin refine -- release ready --version 0.2.2 --allow-dirty --skip-docker --skip-signature --evidence-dir release/evidence/plan2-ci-audit-check
Remove-Item -Recurse -Force release\evidence\plan2-ci-audit-check
```

Expected: docs truth audit passes, or exact issues are fixed.

## 5. Definition of Done

- `docs/release/release-readiness-inventory.md` exists and distinguishes shipped-local, stub-tested, CI-pending, blocked, and planned surfaces.
- `extract_signer_identity()` either extracts a real signer identity with fixture/live evidence or remains explicitly documented as a reporting gap.
- `flake.lock` exists and `nix flake check --no-update-lock-file` passes, or Nix closure is explicitly blocked with the exact error.
- CI records runner architecture evidence and does not imply aarch64 coverage without a real aarch64 lane.
- At least one remote CI run has uploaded release evidence artifacts, or remote execution is explicitly blocked by missing GitHub remote/auth.
- At least one live Sigstore-signed bundle has been verified, or live signing remains explicitly CI-pending with no shipped claim.
- `docs/release/ci-audit-report.md` exists after remote evidence is available, or records the exact remote blocker.
- Final local gates in Section 7 have been run and their exact results are recorded in the closeout.

## 6. Non-Goals

- No hosted customer portal.
- No cloud account automation.
- No Kubernetes, Terraform, Helm, Pulumi, or service uptime monitoring.
- No pure-Rust Sigstore verifier.
- No hardware-backed signing policy.
- No full proof semantics work; that belongs to Plan 1.
- No bit-exact kernel expansion; that belongs to Plan 4.

## 7. Final Verification Gates

- [ ] **Step 1: Run focused Rust tests**

```powershell
cargo test -p refineforge-cli
cargo test -p refineforge-derive
cargo test -p example-counter
cargo test -p example-capability
```

Expected: all pass.

- [ ] **Step 2: Run release-relevant refine gates**

```powershell
cargo run -p refineforge-cli --bin refine -- lean check-all
cargo run -p refineforge-cli --bin refine -- scan check-all
cargo run -p refineforge-cli --bin refine -- lint check-all
```

Expected: all required gates pass. Warnings are acceptable only when they accurately preserve known scope limitations.

- [ ] **Step 3: Run local release readiness**

```powershell
cargo run -p refineforge-cli --bin refine -- release ready --version 0.2.2 --allow-dirty --skip-docker --skip-signature --evidence-dir release/evidence/plan2-final-local-0.2.2
```

Expected: command exits 0 and writes `release-report.json`, `release-report.md`, `sbom.cyclonedx.json`, and `provenance.intoto.json`.

Remove generated evidence after inspection:

```powershell
Remove-Item -Recurse -Force release\evidence\plan2-final-local-0.2.2
```

- [ ] **Step 4: Run Nix if available**

```powershell
nix flake check --no-update-lock-file --print-build-logs
```

Expected: pass, or exact missing-Nix/error output is recorded as a blocker.

- [ ] **Step 5: Run Docker verifier smoke if Docker is available**

```powershell
docker build -t refineforge-verifier:local -f containers/Dockerfile.verifier .
docker run --rm -v "${PWD}\artifacts:/artifacts:ro" refineforge-verifier:local bundle verify /artifacts/EXAMPLE-003
```

Expected: pass, or exact Docker/daemon error is recorded as a blocker.

- [ ] **Step 6: Run whitespace/status checks**

```powershell
git diff --check
git status --short --branch
```

Expected: `git diff --check` passes and the status shows only intended changes.
