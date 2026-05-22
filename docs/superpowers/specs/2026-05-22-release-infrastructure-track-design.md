# Release Infrastructure Track Design

Date: 2026-05-22
Status: Design approved in chat; awaiting review of this saved spec before implementation planning.

## Scope

This spec covers Part 2 of the four-part refineforge development sequence:
the Release / Infrastructure / DevOps engineer track. The selected scope is
the full open-source release infrastructure track:

- CI gates.
- Local release-readiness command.
- Signed bundle flow.
- Verifier container.
- SBOM and provenance artifacts.
- POSIX and Windows release scripts.
- Documentation truth audit.
- CI-produced monitoring and audit reports.

This phase does not build hosted SaaS infrastructure. Cloud account
management, multi-tenant customer auth, billing, uptime SLAs, hosted bundle
verification APIs, and customer data isolation are explicitly deferred to a
later customer-access phase. In this phase, "customer access" means public,
auditable release artifacts that a customer or reviewer can download and
verify without trusting the maintainer's machine.

## Current Context

The repository already contains a substantial DevOps foundation:

- `.github/workflows/ci.yml` with Rust, Lean, bundle, Nix, bit-exact, and
  Sigstore-signing jobs.
- `containers/Dockerfile.verifier`.
- `release/release.sh` and `release/release.ps1`.
- `flake.nix`.
- `SECURITY.md` and `docs/security.md`.
- `docs/reproducible-build.md`.
- `refine bundle verify --verify-signature`, implemented through the upstream
  `cosign` binary.
- `.gitattributes` pinning verification inputs and artifacts to LF line
  endings so byte-level bundle hashes survive Windows checkouts.

The current gap is not absence of scaffolding. The gap is release discipline:
the local command surface, CI evidence, release scripts, and docs need one
shared truth model so a release cannot silently pass with missing scan/lint
evidence, stale docs, unsigned artifacts, or unproven claims about CI-only
behavior.

## Goals

- Make release readiness a first-class local command, not a loose sequence of
  shell snippets.
- Make CI and local release checks use the same gate semantics where practical.
- Ensure CI cannot silently ignore framework-level failures in release lanes.
- Produce machine-readable release evidence for every release candidate.
- Produce SBOM and provenance artifacts that are useful even before a GitHub
  remote exists.
- Preserve the existing bundle manifest schema unless a concrete schema change
  is required.
- Keep signed bundles optional for local development but mandatory in tagged CI
  release evidence.
- Make the verifier container buildable and smoke-testable in CI.
- Keep release scripts thin: version bump, changelog/tag policy, and delegation
  to the readiness gate.
- Make docs distinguish four statuses: shipped and locally verified; shipped
  but only stub-tested; CI-only and pending first remote run; planned.

## Non-Goals

- No hosted verification service.
- No cloud provider account automation.
- No customer login, billing, subscriptions, entitlement checks, or tenant
  database.
- No Kubernetes, Terraform, Helm, Pulumi, or long-running production service.
- No hardware-backed release-tag signing unless the operator provides hardware
  and policy in a later phase.
- No pure-Rust Sigstore implementation in this phase. The existing `cosign`
  delegation remains the security boundary for signature verification.
- No claim that a keyless CI signature has been exercised until a real GitHub
  OIDC run produces one.

## Architecture

The Release Infrastructure track adds a release-evidence layer around the
existing verification core.

```text
source tree
   |
   | refine release ready
   v
release gates
   |-- Rust checks
   |-- Lean checks
   |-- claim lint + scan
   |-- bundle export + hash verify
   |-- docs truth audit
   |-- verifier container smoke test when Docker is available
   |
   v
release evidence directory
   |-- release-report.json
   |-- release-report.md
   |-- sbom.cyclonedx.json
   |-- provenance.intoto.json
   |-- bundles/<CLAIM-ID>/
   |-- ci-summary.md
```

CI runs the same release gate with CI-specific additions:

- Upload unsigned bundles on pull requests.
- Sign bundle manifests on pushes to `main` or `v*` tags.
- Verify signatures with `cosign verify-blob`.
- Upload signed bundles and evidence as artifacts.
- Publish a GitHub Actions job summary.
- Use GitHub OIDC only in the signing job.

Local release scripts call the readiness gate before creating a release commit
and tag. They do not duplicate claim verification logic.

## Component Design

### 1. Release Readiness Command

Add a CLI surface under `refine release`:

- `refine release ready --version X.Y.Z`
- `refine release ready --version X.Y.Z --dry-run`
- `refine release ready --version X.Y.Z --evidence-dir release/evidence/<id>`
- `refine release ready --version X.Y.Z --skip-docker`
- `refine release ready --version X.Y.Z --skip-signature`

The command should produce a structured report even when a gate fails. The
process exits non-zero if any required gate fails.

Required gates:

- Workspace is a Git repository.
- Working tree is clean unless `--allow-dirty` is explicitly passed for
  diagnostic runs.
- Version is valid semver.
- Tag does not already exist locally.
- `Cargo.lock` is present and `cargo metadata --locked` succeeds.
- `cargo test -p refineforge-cli`.
- `cargo test -p refineforge-derive`.
- `cargo test -p example-counter`.
- `cargo test -p example-capability`.
- `refine lean check-all`.
- `refine scan check-all`.
- `refine lint check-all`.
- Bundle export and verify for every claim with release status, starting with
  EXAMPLE-001, EXAMPLE-002, and EXAMPLE-003.
- Release docs truth audit.
- Container smoke test unless Docker is absent or `--skip-docker` is supplied.

The command should report skipped gates as `skipped` with a reason, never as
`passed`.

### 2. Release Evidence Format

Create a stable evidence directory format:

```text
release/evidence/<run-id>/
├── release-report.json
├── release-report.md
├── sbom.cyclonedx.json
├── provenance.intoto.json
├── bundles/
│   └── <CLAIM-ID>/
└── logs/
    └── <gate-name>.log
```

The JSON report records:

- refineforge version requested.
- git commit.
- branch.
- dirty-tree status.
- host OS.
- UTC timestamp.
- gate names, commands, durations, statuses, and log paths.
- exported bundles and their manifest hashes.
- whether signatures were required, produced, verified, skipped, or unavailable.

The Markdown report is a human-readable rendering of the same data. It should
be suitable for pasting into a GitHub release or audit note.

### 3. SBOM

Generate a CycloneDX-compatible JSON SBOM from local Cargo metadata and the
checked-in lockfile.

Minimum useful fields:

- `bomFormat: "CycloneDX"`
- `specVersion`
- `metadata.component` for refineforge.
- one component per workspace package and dependency package.
- package name, version, package URL when derivable, license when available,
  and dependency edges.

This is not a substitute for a full audited commercial SBOM service. It is a
deterministic, source-controlled baseline that CI can upload and reviewers can
diff.

### 4. Provenance

Generate a SLSA/in-toto-shaped provenance predicate locally:

- subject: each exported bundle manifest and its SHA-256.
- builder: local refineforge command or GitHub Actions workflow.
- invocation: command arguments and selected environment variables that affect
  release behavior.
- materials: git commit, `Cargo.lock`, `lean/lean-toolchain`,
  `lean/lake-manifest.json` if present, and release scripts.

For local runs, the provenance is unsigned and must say so. For CI tag runs,
the signing job can attach GitHub artifact attestations or signed provenance
when the remote has permissions. Until that first real run exists, docs must
label this as CI-pending, not proven.

### 5. Signed Bundle Flow

Preserve the existing bundle behavior:

- `refine bundle export <CLAIM-ID>` creates the bundle.
- `refine bundle verify <dir>` verifies raw hashes.
- `refine bundle verify <dir> --verify-signature` delegates to `cosign`.

Tighten the release flow around it:

- Local readiness verifies hashes.
- Local readiness verifies signatures only if signature files are present and
  `cosign` is installed, unless the operator requires signatures.
- Tagged CI signs every exported bundle's `manifest.json`.
- Tagged CI verifies every produced signature before upload.
- Evidence records unsigned, signed, verified, and skipped states separately.

No release path should describe an unsigned bundle as signed.

### 6. CI Gates

Strengthen `.github/workflows/ci.yml` so the release lane cannot hide failures:

- Keep ordinary PR checks broad enough to catch Rust, Lean, scan, lint, bundle,
  Nix, and bit-exact regressions.
- Remove non-fatal scan behavior from release jobs. If scan fails, the release
  gate fails.
- Add `refine release ready --ci --evidence-dir ...` once the command exists.
- Upload release evidence artifacts from every relevant job.
- Build the verifier container and run a smoke verification against an exported
  bundle.
- Keep signing isolated to push/tag events with `id-token: write`.

### 7. Verifier Container

Keep `containers/Dockerfile.verifier` as the reviewer UX surface.

Required behavior:

- Builds from a clean checkout.
- Runs as a non-root user.
- Includes the `refine` binary.
- Includes the pinned Lean toolchain.
- Can verify an exported bundle mounted at runtime.
- CI smoke test proves at least `refine bundle verify /artifacts/EXAMPLE-003`
  works inside the image.

The container is a usability layer, not the reproducible-build root of trust.
Docs must continue to point to Nix for bit-identical rebuild goals.

### 8. Release Scripts

Refactor `release/release.sh` and `release/release.ps1` toward one behavior:

1. Validate version.
2. Confirm branch and tag policy.
3. Move changelog entries if needed.
4. Bump `workspace.package.version`.
5. Run `refine release ready --version X.Y.Z`.
6. Commit version/changelog/evidence manifest if the operator requested it.
7. Create annotated tag.
8. Print push instructions.

The scripts should not run a different set of verification gates than the CLI
release-readiness command. If `cargo nextest` is unavailable locally, the
release-readiness command should report that exact missing tool and either
fallback deliberately or fail according to configuration.

### 9. Docs Truth Audit

Add a lightweight docs audit that checks release-sensitive statements for
known truth boundaries.

At minimum, the audit should cover:

- `README.md`
- `SECURITY.md`
- `docs/security.md`
- `docs/reproducible-build.md`
- `ARCHITECTURE.md`
- `ROLES.md`
- `STRUCTURE.md`

The audit should fail on stale high-risk claims, such as:

- claiming real signed-bundle CI proof when no remote/OIDC run has produced it;
- claiming reproducible builds are proven when Nix first-build verification is
  still pending;
- describing three roles when the architecture is four roles;
- describing planned paths as missing when they are present.

The audit can start as pattern-based checks with explicit messages. It does
not need natural-language understanding.

### 10. Monitoring and Audit Reports

Because this phase has no hosted service, monitoring means CI/release
observability:

- GitHub job summaries for each release gate.
- Uploaded release evidence artifacts.
- Machine-readable report status per gate.
- Bundle manifest hash list.
- Signature status list.
- SBOM and provenance upload.

Later hosted infrastructure can consume this evidence format directly.

## Data Flow

### Local Release Candidate

1. Operator runs `refine release ready --version 0.2.2 --evidence-dir release/evidence/local-0.2.2`.
2. CLI runs gates and exports evidence.
3. Operator reviews `release-report.md`.
4. Operator runs `release/release.ps1 0.2.2` or `release/release.sh 0.2.2`.
5. Script delegates back to the readiness gate, commits, tags, and prints push
   instructions.

### Tagged CI Release

1. Operator pushes `main` and `vX.Y.Z`.
2. CI checks Rust, Lean, scan, lint, bundles, Nix, bit-exact, and container.
3. CI exports evidence.
4. CI signs bundle manifests with Sigstore keyless signing.
5. CI verifies signatures.
6. CI uploads signed bundles, SBOM, provenance, and release report.

## Error Handling

- Every gate records one of: `passed`, `failed`, `skipped`, `blocked`.
- `failed` means repository behavior did not satisfy the gate.
- `blocked` means an external prerequisite was unavailable, such as Docker,
  cosign, GitHub OIDC, or Nix.
- `skipped` requires an explicit CLI flag or CI condition and a reason.
- The final command exit is non-zero if any required gate is `failed` or
  `blocked`.
- Logs must not print secrets or token values.
- Missing optional tooling must not be silently treated as success.

## Security Model

This track strengthens artifact integrity, not proof semantics:

- Lean verification still belongs to the Lean track.
- Refinement correctness still depends on human review.
- Bundle signatures only prove which workflow signed a manifest.
- SBOM and provenance improve reviewability; they do not prove dependencies are
  vulnerability-free.
- The verifier container improves reviewer usability; it is not a hermetic
  proof of reproducibility.

## Interfaces

Section 3 consumes and wraps stable surfaces from Section 1:

- bundle directories;
- `manifest.json`;
- `report.json`;
- claim IDs;
- `refine lean check-all`;
- `refine scan check-all`;
- `refine lint check-all`;
- `refine bundle export`;
- `refine bundle verify`.

Section 3 produces stable surfaces for later sections:

- release evidence directory format;
- SBOM file;
- provenance file;
- signed bundle artifact layout;
- verifier container contract;
- release readiness CLI exit semantics.

## Verification Plan

Design/doc phase:

- Self-review this spec for placeholders, contradictions, scope creep, and
  ambiguous shipped-versus-planned claims.
- Commit the spec before writing implementation tasks.

Implementation phase:

- Unit-test release report serialization and gate status aggregation.
- Unit-test SBOM/provenance generation against fixed fixture metadata.
- CLI-test `refine release ready --dry-run` behavior.
- CLI-test failure reporting for a deliberately missing prerequisite where
  safe to simulate.
- Run `cargo test -p refineforge-cli`.
- Run `refine lean check-all`.
- Run `refine scan check-all`.
- Run `refine lint check-all`.
- Export and verify EXAMPLE-001, EXAMPLE-002, and EXAMPLE-003 bundles.
- Run the release readiness command locally.
- Build and smoke-test the verifier container if Docker is available.
- Run `git diff --check`.

If Docker, cosign, Nix, or GitHub OIDC are unavailable locally, report those
as blocked or CI-pending rather than passing them by assertion.

## Acceptance Criteria

Part 2 is complete when:

- `refine release ready` exists and produces JSON and Markdown evidence.
- Required gates fail closed.
- Release scripts delegate to the readiness command.
- CI runs the readiness gate or an equivalent exact command sequence.
- CI uploads release evidence artifacts.
- CI builds and smoke-tests the verifier container.
- SBOM and provenance artifacts are generated.
- Signed bundle states are represented honestly in evidence.
- Docs distinguish shipped, locally verified, stub-tested, CI-pending, and
  planned behavior.
- EXAMPLE-001, EXAMPLE-002, and EXAMPLE-003 can be exported and verified under
  the release gate.
- The local implementation is committed and verified with targeted gates.

## Risks

- **CI-only signing cannot be fully proven locally.** Mitigation: keep local
  signature verification testable with existing bundle signatures and label
  GitHub OIDC signing as pending until the first remote run.
- **Docker may be unavailable on the operator machine.** Mitigation: the gate
  records Docker as blocked unless `--skip-docker` is explicit.
- **SBOM scope can sprawl.** Mitigation: start with Cargo metadata and
  lockfile-derived CycloneDX JSON; do not add a separate SBOM ecosystem until
  the baseline is stable.
- **Release scripts can diverge between POSIX and Windows.** Mitigation: keep
  both thin and delegate gate semantics to Rust code.
- **Docs can overclaim.** Mitigation: encode high-risk claims in the docs audit
  and fail release readiness when they drift.

## Implementation Decisions

- Release evidence is generated locally and uploaded by CI. It is not committed
  by default because timestamps, host paths, and local tool availability make
  every run unique. The operator may explicitly commit selected evidence for a
  tagged release.
- `cargo test` is the portable baseline. `cargo nextest` may be used when
  available, but missing `cargo nextest` must not block the default local gate.
- The first SBOM includes Rust workspace packages, Cargo dependencies, the
  pinned Lean toolchain, `lean/lakefile.toml`, and `lean/lake-manifest.json`
  when present.
- CI signs every bundle selected by the release-readiness command. For this
  phase, the release-selected set is EXAMPLE-001, EXAMPLE-002, and EXAMPLE-003.
