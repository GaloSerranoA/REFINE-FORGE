# Reproducible builds

This doc describes how to make a refineforge bundle **bit-for-bit
reproducible** — meaning two independent rebuilds of the same git
commit produce byte-identical `manifest.json`, byte-identical
`report.json`, and byte-identical SHA-256 of the bundle directory.

Owned by the **Infrastructure / DevOps** section
([ARCHITECTURE.md](../ARCHITECTURE.md) §3).

> **Status (0.1.0):** This is the *design* doc. The current build
> is not hermetic. The Nix flake (or Bazel) work that delivers this
> is Section 3 phase 2 in the architecture's sequencing.

## 1. Why bit-identical matters

A bundle that "verifies" but cannot be re-built bit-for-bit forces
a reviewer to take the maintainer's word that the bundle came from
the cited git commit. That's a credibility gap any serious user
(auditor, regulator, security-conscious customer) will reject.

When the build is hermetic and reproducible:
- **Reviewer rebuilds → identical hash → no signature trust needed
  for source-to-bundle binding.** The bundle is its own attestation
  that it came from a particular git commit.
- **Maintainer rebuilds → identical hash → can prove an old bundle
  is still what it claims to be**, years later, with the original
  CI infrastructure gone.
- **Signing chain becomes meaningful.** A Sigstore signature over
  a bundle hash is only useful if the hash is reproducible from
  source; otherwise the signature attests to one specific build
  execution, not to the source code.

## 2. Sources of non-determinism (and what to do about them)

Every reproducibility problem comes from one of these:

| Source | Symptom in refineforge today | Fix |
|---|---|---|
| **Timestamps** | `manifest.json` includes `created_at` (ISO 8601 UTC) — changes every run | Either (a) accept that two timestamps differ and hash only the content fields, or (b) accept `SOURCE_DATE_EPOCH` env var and use it |
| **File ordering** | `BTreeMap<String, String>` already sorts — OK | Nothing to do; intentional design |
| **Lean compiler version** | Pinned in `lean-toolchain` — OK | Nothing to do |
| **Rust compiler version** | Not pinned today; CI uses whatever ships with the runner image | Pin via `rust-toolchain.toml` (planned) |
| **Cargo dependency graph** | `Cargo.lock` is committed — OK | Verify `cargo build --release --locked` is used (CI does this) |
| **Path separators in paths** | Fixed in 0.1.0 (manifest keys are forward-slash) | Nothing to do |
| **Path canonicalisation** | `path_to_uri` in LSP uses `canonicalize` — produces machine-specific absolute paths in non-bundle code paths | Bundle code already uses repo-relative paths; LSP paths are runtime-only, not in the bundle |
| **OS file metadata** | `.lake/` build artifacts contain file metadata; we don't bundle `.lake/` so this is OK | Nothing to do |
| **Locale** | Lean's elaborator can emit locale-dependent error messages in `stdout` | Set `LC_ALL=C.UTF-8` in CI; document in the Nix flake |
| **Cargo build caching** | sccache + `target/` may inject build-host info into binaries | Use `--locked` and a clean target dir for release builds |

## 3. The Nix flake approach (planned)

The intended end state is a `flake.nix` at repo root that pins:

- **The Lean toolchain** by content hash, not by version string.
  (`leanprover/lean4:v4.29.1` is pinned-by-version; the Nix
  derivation pins by SHA-256 of the binary distribution tarball.)
- **The Rust toolchain** (cargo, rustc, the standard library) by
  content hash via `rust-overlay`.
- **Every Cargo dependency** by content hash via `crane` or
  `cargo2nix`.
- **System tools** invoked by build scripts: `git`, `bash`, `coreutils`.

With the flake in place:

```bash
# Reproducer A (any machine with Nix):
nix build .#refine-bundle-EXAMPLE-002

# Reproducer B (different machine, hours/days later):
nix build .#refine-bundle-EXAMPLE-002

# Diff the two — should be empty:
diff -r result-A result-B
```

If the diff is non-empty, that's a reproducibility regression.
File a bug; treat it as a P1.

## 4. Alternatives considered

| Approach | Pros | Cons | Decision |
|---|---|---|---|
| **Nix flake** | Pinning by content hash is the standard reproducible-build technology; Lean & Rust both have good Nix support | Nix learning curve for contributors who don't already use it | **Chosen** |
| **Bazel** | First-class hermetic-build engine; used by Google, Stripe, others | Heavy buy-in; Lean integration is immature; would dominate the build system | Rejected — too much overhead for refineforge's size |
| **Docker-only** | Most contributors already have Docker | A Docker build is reproducible-ish but the base image isn't; Lean toolchain installs at build time leak host network state | Rejected as the *primary* mechanism; Dockerfile.verifier (Section 3 phase 1) is a usability layer, not a reproducibility layer |
| **`SOURCE_DATE_EPOCH` only** | Tiny diff; works with what we have | Doesn't address compiler-version, dependency-graph, or filesystem-layout sources | Rejected as insufficient |

## 5. Verification protocol (once the flake lands)

A reviewer who wants to audit a bundle's reproducibility:

1. `git clone <repo>` at the commit referenced in `manifest.json`.
2. `nix build .#refine-bundle-<CLAIM-ID>` — produces a bundle dir.
3. `sha256sum -c` of the new bundle's manifest against the
   reference bundle's manifest. They must match.
4. If they don't match: `diff -r` the two bundles to identify the
   non-determinism source. Report it as a bug.

This protocol is the same one [Reproducible Builds](https://reproducible-builds.org/)
uses for Debian, Tor, Bitcoin Core, and others. We're not
inventing anything novel here; we're applying the standard
discipline to a Lean+Rust artifact pipeline.

## 6. What this doc does NOT promise

- **Reproducibility across Lean *versions*.** A bundle built with
  `lean4:v4.29.1` is reproducible against itself. Switching the
  pin to `v4.30.0` will produce a different bundle hash even with
  no source changes. The bundle's `lean_toolchain` field documents
  which version was used; reviewers re-running must use the same.
- **Reproducibility across architectures.** `x86_64-linux` and
  `aarch64-darwin` will produce identical Lean-compiled outputs
  (Lean is bit-stable across architectures for its source-level
  build artifacts), but Rust binaries are arch-specific. The
  bundle contains source + Lean-source-derived artifacts; it does
  NOT contain compiled Rust binaries, so this is OK in practice.
- **Reproducibility without the flake.** Until the flake lands,
  treat reproducibility as best-effort. The discipline above
  documents the *intended* state, not the current state.

## 7. Sequencing

Per [ARCHITECTURE.md](../ARCHITECTURE.md):

| Step | Owner | When |
|---|---|---|
| Multi-arch CI matrix | DevOps | Section 3 phase 1 |
| Dockerfile.verifier (usability, not reproducibility) | DevOps | Section 3 phase 1 |
| Sigstore signing | DevOps | Section 3 phase 2 |
| **Nix flake (this doc)** | **DevOps** | **Section 3 phase 2** |
| Hardware-backed release signing | DevOps | Section 3 phase 3+ |

Until the Nix flake lands, bundles are reproducible to the extent
that the maintainer's local environment matches the next
reviewer's. The `lean-toolchain` pin + `Cargo.lock` + `--locked`
get you most of the way; the last mile is what this doc
eventually delivers.
