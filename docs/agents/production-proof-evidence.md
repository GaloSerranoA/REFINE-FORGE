# Production-Proof Evidence Pack

`refine production-proof verify` validates the evidence needed to move the
four Refine-Forge agents from local/readiness rungs to a production-proof
closure report.

The command does not create trust by itself. It only checks that the evidence
pack is self-contained, hashes every declared artifact, rejects placeholder
approval records, and writes `summary.json` / `summary.md` using the same
`agent-report-v1` envelope as `refine agent run-all`.

```bash
refine production-proof verify \
  --target helyx \
  --evidence-dir production-proof/evidence/helyx-2026-05-24 \
  --out production-proof/reports/helyx-2026-05-24 \
  --json
```

The evidence directory must contain `evidence.json` matching
`schemas/production-proof-evidence.schema.json`. All paths in that manifest are
relative to the evidence directory; absolute paths and `..` traversal are
rejected so CI artifacts can be re-verified on another machine.

## Required Evidence

DevOps:

- Hosted GitHub Actions run URL under `/actions/runs/`.
- GitHub OIDC issuer `https://token.actions.githubusercontent.com`.
- Sigstore verification output, SBOM, provenance, verifier container digest,
  `flake.lock`, `nix flake check` log, architecture matrix, and release
  approval file.

Training:

- Real checkpoint artifact.
- Evaluation report with `status: "passed"`.
- Regression report with `status: "passed"`.
- Compute ledger, promotion manifest with rollback data, and training approval
  file.

Kernel:

- Real kernel source with `source_kind` of `cuda`, `rust`, `external`, or `ptx`.
  `stub` is intentionally not accepted.
- Reference output, bit-exact report with `status: "passed"`, hardware matrix
  with GPU/driver/CUDA fields, compiler metadata, performance baseline, HELYX
  handoff, and kernel approval file.

Lean:

- Claim report with implementation-linked claims, refinement docs, Rust
  symbols, and Lean theorem names.
- Proof inventory, refinement-link report with `status: "passed"`, bundle
  hashes, and Lean approval file.

## Approval Files

Each role approval file uses:

```json
{
  "schema_version": "refineforge-human-approval-v1",
  "human_operator": "Galo Release Operator",
  "role": "release",
  "decision": "approved",
  "approved_at": "2026-05-24T00:00:00Z",
  "evidence_summary": "release production evidence reviewed"
}
```

The verifier rejects AI, bot, placeholder, empty, or malformed operator names.
Human approval is never inferred from a passing command, prompt text, or memory
record.
