# Security

This doc is the starting point for refineforge's threat model,
supply-chain integrity, signing chain, and vulnerability-reporting
policy. It is owned by the **Infrastructure / DevOps** section
(see [ARCHITECTURE.md](../ARCHITECTURE.md) §3).

> **Status (0.1.0):** This is the *design* doc. The bundle exporter
> already produces SHA-256-sealed bundles, but they are not signed
> and the build is not yet hermetic. The work to close those gaps is
> Section 3 phase 2 in the architecture's sequencing.

## 1. Threat model

We name the adversary and what they're trying to do *before* we
talk about controls. This keeps us honest about which threats we
actually mitigate.

### 1.1 Adversaries refineforge defends against

| Adversary | Capability | Goal |
|---|---|---|
| **Mistaken contributor** | Commit access | Accidentally lands a `sorry`/`admit`/axiom that hides an unproven claim |
| **Bundle tamperer (in transit)** | Modifies bundle files between export and reviewer | Convinces reviewer a bundle verifies when it doesn't, or vice versa |
| **Toolchain drifter** | Reviewer uses a different Lean version than the bundle was built with | Theorems that hold under v4.29.1 fail under v4.x; reviewer cannot reproduce |
| **Refinement-doc bypasser** | Marks a claim's status `proven` (model+refined) without a written refinement argument | Marketing claim outruns the actual verification |

### 1.2 Adversaries refineforge does NOT defend against

| Adversary | Why it's out of scope |
|---|---|
| **Lean kernel exploit** | The Lean kernel is in the trusted base. If you find a kernel bug, file it upstream. |
| **Compromised `rustc` / LLVM** | Same as Lean kernel; trusted by assumption. |
| **OS / hypervisor compromise** | Below the abstraction we model. |
| **Side-channel attack on `Capability::authorizes`** | Timing leaks are a separate claim family (`HELYX-SIDECHAN-*` in HELYX's case); each affected refinement doc must enumerate side-channel concerns in §5. |
| **Compromised LLM provider** (Section 2) | The repair loop runs every proposed patch through `lake build` + the no-sorry gate. A malicious LLM can waste compute, but cannot land a verified-but-wrong proof. (It can land a proof that satisfies the Lean spec but is semantically misleading — but that's also true of a human contributor; the refinement-doc review is the defence.) |

## 2. Supply chain

### 2.1 Inputs to a bundle

A `refine bundle export` produces an archive containing:

- Every `.lean` file under `lean/`
- The cited claim YAML
- `lean/lakefile.toml` and `lean/lean-toolchain` (the toolchain pin)
- `docs/refinement/<CLAIM-ID>.md` if present
- `manifest.json` (SHA-256 of every file above + the report)
- `report.json` (build status + policy-gate counts)

What's NOT in the bundle (and why it matters):

| Not bundled | Why | Trust required |
|---|---|---|
| `rustc` / `cargo` | Reviewers may rebuild the Rust impl from source | Trust your Rust toolchain |
| Lean compiler itself | Pinned by `lean-toolchain`; elan downloads from `leanprover/lean4` GitHub releases | Trust elan + leanprover/lean4 |
| Mathlib (when used) | Currently no claim uses Mathlib; when one does, the pin lives in `lean/lake-manifest.json` | Trust the pinned Mathlib commit |
| The `refine` binary itself | Reviewers run `cargo build --release` from source | Trust this repo's source + Cargo deps |

### 2.2 Cargo dependency pinning

`Cargo.lock` IS committed (despite the `.gitignore` listing — see
the gitignore-tracked-file note in [STRUCTURE.md](../STRUCTURE.md)),
so a reviewer who runs `cargo build --release --locked` gets the
exact dependency graph the maintainer used.

Each direct dependency in `crates/*/Cargo.toml` uses a caret
version range (`= "1.0"` means `>= 1.0, < 2.0`). For
reproducibility-critical work, the DevOps section's
[reproducible-build.md](reproducible-build.md) pins every input by
content hash via Nix (planned).

## 3. Signing chain (✅ shipped)

Bundles are SHA-256-self-attesting (the manifest hashes itself).
Pushed to `main` or tagged `v*`, they are ALSO Sigstore-signed in CI.

```
git commit ──▶ CI runs ──▶ refine bundle export ──▶ cosign sign-blob ──▶ Rekor log entry
                                                       │
                                                       ▼
                                                  bundle/manifest.json.sigbundle
                                                  bundle/manifest.json.sig
                                                  bundle/manifest.json.cert
```

`refine bundle verify` has the flag:

```
refine bundle verify <dir> --verify-signature
```

Which (via `cosign verify-blob` under the hood) checks:
- The `cosign` signature over `manifest.json` is valid
- The signing cert chain roots in Sigstore's Fulcio CA
- The Rekor transparency log contains an entry binding the
  signature to the git commit / workflow identity
- The signer identity (subject + issuer in the cert) matches an
  expected pattern. Default pattern: refineforge's canonical CI
  workflow identity. Overridable via `--identity-regex` /
  `--oidc-issuer` flags or `REFINEFORGE_EXPECTED_*` env vars.

This turns "we built this bundle" into "we built this bundle, here
is the cryptographic proof, and a public transparency log records it."

**Implementation choice:** the verifier delegates to the upstream
`cosign` binary rather than reimplementing signature / Fulcio / Rekor
verification in Rust. Same security guarantees (cosign does the real
cryptography), much less code, well-tested upstream. The seam is
the `REFINEFORGE_COSIGN_BIN` env var which lets tests substitute a
stub. A pure-Rust verification path using the `sigstore` crate is
documented as a future option but not implemented; it would let
refineforge ship a single binary without the cosign dependency.

**Future — hardware-backed release signing:** Optional YubiKey or TPM-
backed signature for releases tagged `v*`. The CI keyless signature
stays as the per-commit attestation; release-tag signatures would
be an additional layer for tagged versions. Not yet implemented.

## 4. Bundle verification policy

A reviewer evaluating a refineforge bundle should:

1. **Verify hashes** (mandatory). `refine bundle verify <dir>` re-
   hashes every file in `manifest.json` and confirms the report
   hash. Exit 0 means contents match the manifest.
2. **Verify signature** (mandatory once Phase 1 ships). Pass
   `--verify-signature` to additionally check the Sigstore proof.
3. **Re-run Lean** (recommended). Reconstruct the source layout
   from `manifest.json` (the `VERIFY.txt` in each bundle has the
   exact shell loop), then `cd lean && lake build`. This is the
   ONLY check that re-establishes the proof; everything else just
   confirms files haven't been tampered with.
4. **Read the refinement doc** (mandatory for any model+refined
   claim). Bundle hashes can't tell you whether the Rust impl
   matches the Lean model — only a human reader can.

## 5. Vulnerability reporting

If you find a security issue in refineforge — a way to make the
no-sorry gate miss a `sorry`, a way to make `refine bundle verify`
return OK on a tampered bundle, a way for the repair loop to land
a proof without `lake build` accepting it — please **do not** file
a public issue.

**Reporting channel (placeholder until repo has a remote):**
contact the maintainer named at the top of [README.md](../README.md).
A future revision of this doc will list a security@ email and a
PGP key fingerprint, plus a published response-time SLA.

**What we ask of you:**
- Allow 90 days for a fix before public disclosure.
- Describe the issue concretely: which check is bypassed, with a
  minimal reproducer.
- We will credit you in the [CHANGELOG.md](../CHANGELOG.md) entry
  for the fix unless you ask otherwise.

## 6. What's deliberately NOT in this doc

- A formal SBOM (planned for Section 3 phase 3; CycloneDX format,
  generated in CI alongside each bundle).
- A signed-release-tag policy (depends on the signing chain
  landing first).
- A bug bounty program (would require legal / financial
  infrastructure refineforge does not have at 0.1.0).
- Specific incident-response runbooks (premature with zero
  deployments).

These are noted here so a reader knows what to expect *later*
without confusing the current state of the world.
