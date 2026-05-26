# Enterprise Readiness

`refine enterprise ready` is the final hardening gate for the boundary that
Refine-Forge is useful but not yet a finished enterprise product. It turns that
boundary into a deterministic report instead of a marketing claim.

The command writes:

- `enterprise-readiness.json`
- `enterprise-readiness.md`

The report status is `ready` only when every gate has real evidence. Otherwise
it stays `blocked` and names the missing proof.

## Command

```powershell
refine enterprise ready `
  --out enterprise-readiness/latest `
  --hosted-ci-evidence production-proof/evidence/devops/hosted-ci.json `
  --signed-release-evidence production-proof/evidence/devops/cosign-verify.json `
  --checkpoint-manifest training/runs/<run>/hrm-text-checkpoint-manifest.json `
  --helyx-integration-evidence production-proof/evidence/helyx/live-integration.json `
  --cleanup-report docs/release/cleanup-report.json
```

Use `--json` when another tool needs to consume the report from stdout.

## Gates

| Gate | Required evidence |
|---|---|
| Remote CI proof | JSON with `status` equal to `passed`, `success`, `ready`, `approved`, or `human-reviewed` from a hosted CI run |
| Signed release proof | JSON with accepted `status`, a signature marker, and a 64-hex `bundle_sha256` |
| Accepted real model checkpoint | JSON with accepted `status`, a 64-hex checkpoint SHA-256, and `helyx_handoff.requires_hash_verification: true` |
| Live HELYX integration | JSON with accepted `status` from a real HELYX integration run |
| Documentation polish | `README.md`, `STRUCTURE.md`, `CHANGELOG.md`, and this doc all mention enterprise readiness |
| Complexity cleanup report | JSON with accepted `status` from a cleanup or complexity review |

## Boundary

This gate does not create remote CI proof, sign releases, accept checkpoints, or
prove live HELYX integration by itself. It only validates evidence that those
steps happened. Missing evidence is a blocker, not a warning.

The public claim emitted by the report is intentionally narrow:

- `enterprise_readiness_blocked_until_external_evidence_present`
- `enterprise_readiness_evidence_complete_local_check`

Anything stronger requires external evidence and human release approval.
