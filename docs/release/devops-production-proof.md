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

## Offline / Local Release Proof

When GitHub, OIDC, Docker, Nix, or remote runners are unavailable, the DevOps
agent can ingest a separate local proof profile with:

- `release/release-report.json`
- `release/offline-release-proof.json`
- `release/offline-signature.json`
- `release/offline-verifier.json`
- `release/local-environment.json`
- `release/sbom.cyclonedx.json`
- `release/provenance.intoto.json`
- `approvals/release-offline.json`

Set `REFINEFORGE_OFFLINE_RELEASE_EVIDENCE_DIR` to that evidence directory. The
agent records this under `assurance_profiles[].id =
devops.offline_release_proof` with trust effect `supports
release-ready-local only`. It does not satisfy hosted CI, GitHub OIDC Sigstore,
Nix, verifier-container, or `approvals/release.json` production-proof gates.

Generate the local proof pack from an existing `refine release ready` output
and real local signature/verifier artifacts:

```bash
refine release offline-proof \
  --version 0.2.2 \
  --release-ready-dir release/evidence/local-0.2.2 \
  --evidence-dir production-proof/evidence/devops-offline \
  --signature-file path/to/local-release.sig \
  --key-fingerprint <local-key-fingerprint> \
  --verifier-log path/to/offline-verifier.log
```

The command copies the release-ready report/SBOM/provenance into
`release/`, records the signature file hash, records the verifier log hash,
and writes `release/local-environment.json`. It refuses missing files or a
release report whose version does not match `--version`.
