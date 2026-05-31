# refineforge release readiness report

- Version: `0.2.2`
- Generated at: `2026-05-31T01:50:59.826662079+00:00`
- Git commit: `c58336bf663415f70d6351c5ef8ae64a9595400d`
- Git branch: `main`
- Dirty tree: `true`
- Host OS: `linux`
- Runner: `Linux` / `X64`
- SBOM SHA-256: `d79cf3853699638f95b5e0ff0d6df255991da89f9c5821708dd812d63e4f84ed`
- Provenance SHA-256: `8a1443341691fabeaef428bad4347724b59ad2ba72fa5ccdca3347736426c650`

## Gates

| Gate | Status | Required | Message |
|---|---|---:|---|
| git-worktree | passed | yes | inside git worktree |
| git-clean | skipped | yes | allowed by --allow-dirty |
| version-semver | passed | yes | valid semver |
| tag-available | skipped | yes | dry-run |
| cargo-metadata-locked | skipped | yes | dry-run |
| cargo-test-refineforge-cli | skipped | yes | dry-run |
| cargo-test-refineforge-derive | skipped | yes | dry-run |
| cargo-test-example-counter | skipped | yes | dry-run |
| cargo-test-example-capability | skipped | yes | dry-run |
| lean-check-all | skipped | yes | dry-run |
| scan-check-all | skipped | yes | dry-run |
| lint-check-all | skipped | yes | dry-run |
| bundle-export-EXAMPLE-001 | skipped | yes | dry-run |
| bundle-verify-EXAMPLE-001 | skipped | yes | dry-run |
| bundle-export-EXAMPLE-002 | skipped | yes | dry-run |
| bundle-verify-EXAMPLE-002 | skipped | yes | dry-run |
| bundle-export-EXAMPLE-003 | skipped | yes | dry-run |
| bundle-verify-EXAMPLE-003 | skipped | yes | dry-run |
| docs-truth-audit | passed | yes | docs truth audit passed |
| docker-verifier-smoke | skipped | yes | skipped by --skip-docker |
| signature-verification | skipped | yes | skipped by --skip-signature |

## Bundles

| Claim | Manifest SHA-256 | Signature |
|---|---|---|
