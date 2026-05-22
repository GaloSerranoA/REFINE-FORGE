# CI Audit Report

> Snapshot date: 2026-05-22
> Remote run id: blocked - `git remote -v` produced no remote entries, and `gh auth status` reported "You are not logged into any GitHub hosts."

| Gate | Local evidence | CI evidence | Status | Notes |
|---|---|---|---|---|
| Rust tests | `cargo test -p refineforge-cli` | blocked | pending-local-run | Final verification records the current local result. |
| Lean gate | `refine lean check-all` | blocked | pending-local-run | Final verification records the current local result. |
| Scan gate | `refine scan check-all` | blocked | pending-local-run | Final verification records the current local result. |
| Lint gate | `refine lint check-all` | blocked | pending-local-run | Final verification records the current local result. |
| Bundle export/verify | EXAMPLE bundles via release readiness | blocked | pending-local-run | Manifest hashes appear in generated release evidence only. |
| Release evidence | `release-report.*`, SBOM, provenance | blocked | pending-local-run | Local generated evidence is removed after inspection unless the operator asks to commit it. |
| Verifier container | Docker local or skipped | blocked | blocked-local | `docker --version` failed because `docker` was not on PATH. |
| Nix | `nix flake check` | blocked | blocked-local | `nix --version` failed because `nix` was not on PATH; no `flake.lock` was generated. |
| Sigstore | local cosign verify | blocked | blocked-local + ci-pending | `cosign version --json` failed because `cosign` was not on PATH; no signed bundle exists without remote CI/OIDC. |
| Architecture | local host arch | blocked | authored | CI workflow records `runner.os`, `runner.arch`, `rustc -Vv`, and machine architecture on future runs. |

## Remote Blocker

This report is intentionally a local blocker report until a GitHub remote run is available. A future release operator should replace the `blocked` CI evidence cells with `gh run` URLs and downloaded artifact names after configuring a remote and authenticating `gh`.
