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
