# Security

This file is the entry point for security-relevant matters in refineforge.
For the full threat model and supply-chain details see
[docs/security.md](docs/security.md).

## Reporting a vulnerability

**Do not file a public issue.** If you've found a way to:

- bypass the no-sorry policy gate (a `sorry`/`admit`/non-core axiom
  that lake-builds clean but the gate misses),
- make `refine bundle verify` return OK on a tampered bundle (hash
  bypass, signature bypass, or schema-version downgrade attack),
- get the repair loop to land a `sorry` despite the no-sorry gate
  running after every patch,
- impersonate the CI signer identity in a way that
  `--verify-signature` accepts,

…please contact the maintainer listed at the top of [README.md](README.md).

A future revision of this doc will list a `security@` email and a PGP
key fingerprint. Until then, the maintainer's email is the channel.

We ask for **90 days** before public disclosure. You will be credited
in the [CHANGELOG.md](CHANGELOG.md) entry for the fix unless you ask
otherwise.

## Verifying a release

Reviewer-side Sigstore verification is implemented, but this checkout has not
yet produced a live GitHub OIDC signed-bundle artifact. Once the first real CI
signing run lands, release tags can be verified via Sigstore (Fulcio + Rekor),
keyless. To verify a signed bundle you downloaded:

```bash
# Install cosign once:
# https://github.com/sigstore/cosign/releases

# Verify a bundle's hashes + signature in one command:
refine bundle verify <bundle-dir> --verify-signature
```

`--verify-signature` invokes `cosign verify-blob` under the hood, which
checks:

1. The signature is valid over `manifest.json`.
2. The signing cert chains to the public Sigstore CA (Fulcio).
3. The cert's identity claim matches refineforge's CI workflow
   identity (overridable via `--identity-regex` for forks).
4. The signature was logged in Rekor, the public transparency log.

If any of those fail, the command exits non-zero and prints the failure
reason.

## Threat model summary

refineforge **defends against**:

- Mistaken contributor landing a `sorry`/`admit`/axiom (policy gate
  before `lake build`).
- Bundle tampering in transit (SHA-256 manifest + Sigstore signature).
- Toolchain drift (pinned `lean-toolchain`, manifest captures it).
- Refinement-doc bypass on a refined claim (status field requires
  human review per `docs/refinement-template.md` §6).

refineforge **does not defend against**:

- Lean kernel bugs (trusted base).
- Compromised `rustc`/LLVM (trusted base).
- OS/hypervisor compromise (below abstraction).
- Side-channel attacks on cited Rust code (each refinement doc must
  enumerate side-channel concerns in §5).
- A malicious LLM strategy proposing patches that compile but are
  semantically wrong (the no-sorry gate catches `sorry` injection,
  but cannot catch "this proof passes but proves the wrong theorem"
  — that's what refinement-doc review is for).

Full enumeration in [docs/security.md](docs/security.md) §1.
The current release-infrastructure truth table is maintained in
[docs/release/release-readiness-inventory.md](docs/release/release-readiness-inventory.md).

## Signing chain (current state)

| Layer | Status | Tool |
|---|---|---|
| SHA-256 manifest over bundle contents | ✅ shipped | `refine bundle export` (built-in) |
| Sigstore keyless signature over manifest.json | CI-pending for the first real GitHub OIDC run; reviewer-side verification shipped | `cosign sign-blob` is authored in `.github/workflows/ci.yml`; `refine bundle verify --verify-signature` calls `cosign verify-blob` |
| Rekor transparency-log inclusion proof | CI-pending for repo-produced signatures | cosign default behaviour once a real CI signature exists; check via `cosign verify-blob` |
| Hardware-backed release-tag signing (YubiKey/TPM) | not yet | future Section 3 phase 3 |
| Pure-Rust signature verification (no cosign dep) | not yet | future — `sigstore` crate exists in Rust ecosystem |
| Reproducible builds (Nix flake) | not yet | future Section 3 phase 2 — see [docs/reproducible-build.md](docs/reproducible-build.md) |

## Test discipline (honest disclosure)

The signature-verification code path was unit-tested with **stub
cosign binaries** that simulate success and failure modes. It has
**not been exercised against a real Fulcio cert in this session**
because that requires a real CI run from this repo, which has no
remote configured yet. The first push to a GitHub remote with the
shipped CI workflow will produce the first real signed bundle; any
contract mismatch between our `--verify-signature` flags and what
cosign actually emits will surface there.

The cosign command-line invocations + flag combinations follow the
documented `cosign sign-blob` / `cosign verify-blob` API for
cosign v2.4.x; any future-version drift would surface as a flag
mismatch on first real CI run.

`extract_signer_identity()` intentionally returns `None` today after cosign
validates the signature and identity regex. Reports therefore show the
fallback string `(identity matched but couldn't extract)` until a real X.509
SAN parser or stable cosign JSON output path is added. This is a reporting
gap, not a signature-validation bypass.
