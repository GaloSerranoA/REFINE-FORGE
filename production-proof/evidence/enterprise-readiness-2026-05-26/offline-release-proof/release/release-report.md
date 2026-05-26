# refineforge release readiness report

- Version: `0.2.2`
- Generated at: `2026-05-26T11:38:00.812409500+00:00`
- Git commit: `b438ababfd46162dd4c594e686f1462356b3ede2`
- Git branch: `master`
- Dirty tree: `false`
- Host OS: `windows`
- Runner: `windows` / `x86_64`
- SBOM SHA-256: `5370d949bb279d92b97b16db290aeb8aa3c452498ecadda3892fdd2568d55f1d`
- Provenance SHA-256: `ad1c337a90056d9f787d82d88d40b0a347bdf990ece8bb3ba7384a209bf7aba9`

## Gates

| Gate | Status | Required | Message |
|---|---|---:|---|
| git-worktree | passed | yes | inside git worktree |
| git-clean | passed | yes | working tree is clean |
| version-semver | passed | yes | valid semver |
| tag-available | passed | yes | tag is available |
| cargo-metadata-locked | passed | yes | command exited 0 |
| cargo-test-refineforge-cli | passed | yes | command exited 0 |
| cargo-test-refineforge-derive | passed | yes | command exited 0 |
| cargo-test-example-counter | passed | yes | command exited 0 |
| cargo-test-example-capability | passed | yes | command exited 0 |
| lean-check-all | passed | yes | command exited 0 |
| scan-check-all | passed | yes | command exited 0 |
| lint-check-all | passed | yes | command exited 0 |
| bundle-export-EXAMPLE-001 | passed | yes | command exited 0 |
| bundle-verify-EXAMPLE-001 | passed | yes | command exited 0 |
| bundle-export-EXAMPLE-002 | passed | yes | command exited 0 |
| bundle-verify-EXAMPLE-002 | passed | yes | command exited 0 |
| bundle-export-EXAMPLE-003 | passed | yes | command exited 0 |
| bundle-verify-EXAMPLE-003 | passed | yes | command exited 0 |
| docs-truth-audit | passed | yes | docs truth audit passed |
| docker-verifier-smoke | skipped | yes | skipped by --skip-docker |
| signature-verification | skipped | yes | skipped by --skip-signature |

## Bundles

| Claim | Manifest SHA-256 | Signature |
|---|---|---|
| EXAMPLE-001 | `e267d5484782388fa66889d63288acd0c47e70ef928ecb13047c0ca9e9ab9030` | unsigned |
| EXAMPLE-002 | `fefe69ae4f139cedf2c34852cd1e227881bb382b9ff85f0e104b430e8799c284` | unsigned |
| EXAMPLE-003 | `6f40a1253958b45c35191fa7f2caf7a34b3e968c2fdfa1518a4c93e1372374ed` | unsigned |
