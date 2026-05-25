# Infrastructure / DevOps Agent

The DevOps agent owns local release readiness and CI evidence boundaries.

## Required CLI Surface

```bash
refine agent devops --mode inspect --target helyx --out agent-reports/devops
refine agent devops --mode check --target 0.2.2 --out agent-reports/devops
refine agent devops --mode execute --target 0.2.2 --out agent-reports/devops
refine agent devops --mode execute --target 0.2.2 --allow-expensive --out agent-reports/devops-live
refine release offline-proof --version 0.2.2 --release-ready-dir release/evidence/local-0.2.2 --evidence-dir production-proof/evidence/devops-offline --signature-file path/to/local-release.sig --key-fingerprint <fingerprint> --verifier-log path/to/offline-verifier.log
REFINEFORGE_OFFLINE_RELEASE_EVIDENCE_DIR=production-proof/evidence/devops-offline \
  refine agent devops --mode inspect --target 0.2.2 --out agent-reports/devops-offline
```

## Source Of Truth

Use the generated `devops.json` report and any nested release evidence it
records. The runtime envelope caps the local command surface at
`release-ready-local` unless hosted CI/OIDC, Nix, artifact, architecture, and
named human release approval evidence all pass. Local reports do not imply
hosted CI or OIDC signing. Docker and signature gates are skipped unless
`--allow-expensive` is passed and the local tools actually run successfully.
Offline/local release evidence is reported as a separate assurance profile and
supports only `release-ready-local`.

## Allowed Work

- Inspect release readiness docs and CI workflows.
- Run local release readiness.
- Request Docker verifier and cosign/Sigstore gates with `--allow-expensive`.
- Generate SBOM, provenance, and release-report evidence.
- Ingest offline/local release proof through
  `REFINEFORGE_OFFLINE_RELEASE_EVIDENCE_DIR` and
  `approvals/release-offline.json`.
- Record missing Docker, Nix, cosign, GitHub auth, and hosted CI blockers.

Release production proof is governed by
`docs/release/devops-production-proof.md`.

## Forbidden Claims

- Do not claim live Sigstore success without a real signed bundle.
- Do not claim hosted CI passed from a local run.
- Do not treat `release-offline` approval as `release` approval.
- Do not hide skipped Docker or signature checks.
