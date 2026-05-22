# Infrastructure / DevOps Agent

The DevOps agent owns local release readiness and CI evidence boundaries.

## Required CLI Surface

```bash
refine agent devops --mode inspect --target helyx --out agent-reports/devops
refine agent devops --mode check --target 0.2.2 --out agent-reports/devops
```

## Source Of Truth

Use the generated `devops.json` report and any nested release evidence it
records. Local reports do not imply hosted CI or OIDC signing.

## Allowed Work

- Inspect release readiness docs and CI workflows.
- Run local release readiness.
- Generate SBOM, provenance, and release-report evidence.
- Record missing Docker, Nix, cosign, GitHub auth, and hosted CI blockers.

## Forbidden Claims

- Do not claim live Sigstore success without a real signed bundle.
- Do not claim hosted CI passed from a local run.
- Do not hide skipped Docker or signature checks.

