# HELYX case study

The HELYX trust-claim project is the worked reference implementation
that refineforge was extracted from. Read it to see the full
pattern in action — including all of the moving parts that the
hello-world tutorial omits.

## What HELYX demonstrates that the EXAMPLE-001 tutorial does not

| Pattern                                              | EXAMPLE-001 | HELYX-AUDIT-001 |
|------------------------------------------------------|:-----------:|:---------------:|
| Lean theorem with non-trivial proof                  |             |        ✅       |
| Inductive predicate refining a structural invariant  |             |        ✅       |
| `rust_source` block linking to a real crate          |             |        ✅       |
| Rust crate that compiles + passes refinement tests   |             |        ✅       |
| TDD red→green tests, one per Lean theorem            |             |        ✅       |
| Refinement-argument doc filled in                    |             |        ✅       |
| `refine scan` reporting `Verified` (not stub)        |             |        ✅       |

## Where to find it

The HELYX project lives separately as `helyx-proofforge` (originally
extracted to `C:\Users\GALO\Downloads\helyx-proofforge\` on the
machine that built refineforge). If you don't have it locally, ask
the project maintainer.

Key files to read, in order:

1. `lean/HELYX/AuditChain.lean` — model + 3 proven theorems
2. `claims/audit-chain.yaml` — the claim metadata, including
   `rust_source` pointing at `crates/helyx-audit/`
3. `crates/helyx-audit/src/chain.rs` — Rust implementation that
   refines the Lean model
4. `crates/helyx-audit/tests/chain.rs` — refinement tests, one per
   Lean theorem
5. `docs/refinement/HELYX-AUDIT-001.md` — the trust-critical bridge

## What HELYX is NOT

The HELYX claim set ships two claims (AUDIT-001 fully refined,
CAP-001 model-only). It is a **demonstration of the framework**,
not production-grade trust infrastructure for any real audit
system. Anyone reusing HELYX's Lean models or Rust crates for
production must subject them to their own review under their own
threat model.

## Relationship to refineforge

refineforge is the **framework** (CLI, schema, gate, bundle
exporter, scaffolder, scan). HELYX is the **first consumer**.
Improvements to either project should flow:

- **HELYX bug fix that is project-specific** → stays in HELYX.
- **HELYX bug fix that affects the framework** → port to refineforge
  (e.g. the Windows path-separator fix in `bundle.rs` was
  discovered in HELYX and ported here).
- **New framework feature** → add to refineforge first; HELYX
  picks it up by syncing.
