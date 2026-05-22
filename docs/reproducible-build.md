# Reproducible builds

This doc describes how to make a refineforge bundle **bit-for-bit
reproducible** — meaning two independent rebuilds of the same git
commit produce byte-identical `manifest.json`, byte-identical
`report.json`, and byte-identical SHA-256 of the bundle directory.

Owned by the **Infrastructure / DevOps** section
([ARCHITECTURE.md](../ARCHITECTURE.md) §3).

> **Status (Unreleased):** Nix flake **authored** (`flake.nix` at
> repo root). The structure follows the standard lean4-nix + crane
> + rust-overlay pattern. Targets shipped: `nix build .#refine`,
> `nix build .#refine-eval`, `nix build .#bundle-EXAMPLE-001`,
> `nix build .#bundle-EXAMPLE-002`, `nix develop`, `nix flake check`.
>
> **First-build verification PENDING.** The flake was authored on
> a Windows dev machine without a Nix install. First green run on a
> Linux/macOS machine (or via the `nix-flake-check` CI job that
> ships in the same commit) will surface any drift from
> `lean4-nix`'s documented API or `crane`'s evolving conventions.
> Until that first green run lands, treat this doc as describing
> the **intended state**, not the **proven state**.
>
> For the current shipped/stub-tested/CI-pending release
> infrastructure inventory, see
> [`docs/release/release-readiness-inventory.md`](release/release-readiness-inventory.md).

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

## 3. The Nix flake approach (skeleton shipped; first-build pending)

The flake at repo root pins:

- **The Lean toolchain** via `lean4-nix.readToolchainFile
  ./lean/lean-toolchain`. The toolchain file format is
  `leanprover/lean4:vX.Y.Z`; lean4-nix translates that into a
  content-addressed Nix derivation.
- **The Rust toolchain** via `rust-overlay` (`pkgs.rust-bin.stable.latest.default`).
- **Every Cargo dependency** via `crane` (`cargoArtifacts` cached
  separately from `buildPackage`).
- **System tools** in `devShells.default`: cargo-nextest, lean-all,
  cosign, git, jq, python3.

With the flake in place:

```bash
# Reproducer A (any machine with Nix):
nix build .#bundle-EXAMPLE-002

# Reproducer B (different machine, hours/days later):
nix build .#bundle-EXAMPLE-002

# Diff the two — should be empty (modulo manifest.created_at):
diff -r result-A result-B
```

If the diff is non-empty (excluding `created_at`), that's a
reproducibility regression. File a bug; treat it as a P1.

### Targets

| `nix build` invocation | What it produces | Status |
|---|---|---|
| `nix build .#refine` | the `refine` binary | high confidence — straight crane |
| `nix build .#refine-eval` | the `refine-eval` binary | high confidence — same pattern |
| `nix build .#bundle-EXAMPLE-001` | hermetic bundle for the Lean-only tutorial | medium confidence — lake build sandbox interaction is the unknown |
| `nix build .#bundle-EXAMPLE-002` | hermetic bundle for the refined tutorial | medium confidence — same as above |
| `nix develop` | shell with refine, lean-all, cosign, cargo, etc. | high confidence |
| `nix flake check` | `cargo test`, `cargo fmt --check`, `cargo clippy -D warnings` | high confidence |

### Known unknowns

1. **Lake's `.lake/` cache writes inside `src`.** Nix sandboxes
   typically deny writes to the source dir. The bundle derivation
   copies src to `$TMPDIR` via crane's standard pattern, so this
   *should* work — first build will confirm.
2. **`lake build` on first run may want network access** to fetch
   the Lean toolchain — but `pkgs.lean-all` provides it, so Lake
   should use the local copy. If we ever add a Mathlib-using claim,
   that claim's `bundleFor` derivation will need `__noChroot = true`
   (impure) or a pre-built Mathlib package.
3. **The `crane` API has drifted over versions.** Pinning a specific
   `crane` rev once `nix flake lock` runs will stabilise this.

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

## 7. Sequencing — current state

| Step | Owner | Status |
|---|---|---|
| Multi-arch CI matrix | DevOps | ✅ shipped (.github/workflows/ci.yml) |
| Dockerfile.verifier (usability, not reproducibility) | DevOps | ✅ shipped (containers/Dockerfile.verifier) |
| Sigstore signing | DevOps | ⚠️ CI workflow authored + verifier-side `--verify-signature` shipped; first real GitHub OIDC signed-bundle run pending |
| **Nix flake (this doc)** | **DevOps** | ⚠️ **authored; first build pending** |
| Hardware-backed release signing | DevOps | not yet (Section 3 phase 3) |
| Bit-identical bundle audit (verification protocol §5) | DevOps | not yet — gated on first green nix-flake-check CI run |

Until the first green `nix-flake-check` CI run lands, bundles are
reproducible to the extent that the maintainer's local environment
matches the next reviewer's. The `lean-toolchain` pin + `Cargo.lock`
+ `--locked` get you most of the way; the Nix flake's first green
build is the proof.

## 8. How to verify the flake yourself (if you have Nix)

The honest first action a Nix-on-Linux/macOS user can take:

```bash
git clone <this-repo> refineforge
cd refineforge
nix flake check --no-update-lock-file --print-build-logs
nix build .#refine --no-update-lock-file --print-build-logs
nix build .#bundle-EXAMPLE-002 --no-update-lock-file --print-build-logs
```

If any of those error: that's exactly the information the maintainer
needs. Open an issue with the full `--print-build-logs` output. The
flake was authored without a way to exercise it locally; the first
real user run is the verification.
