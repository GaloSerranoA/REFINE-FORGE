# DevOps Production Proof Checklist

The DevOps agent may emit `human-reviewed` only when the release evidence pack
includes:

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

The CI workflow writes the DevOps evidence artifact
`refineforge-devops-production-evidence` from the hosted signing job. That
artifact is the input for `REFINEFORGE_RELEASE_EVIDENCE_DIR`; a local
`release ready` report alone remains `release-ready-local`.
