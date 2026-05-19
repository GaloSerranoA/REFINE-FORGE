# HELYX-AUDIT-001 — Refinement argument

> **The trust-critical artifact.** Lean proves the model;
> this document argues that the model represents the
> running HELYX audit chain. Refineforge cannot verify this
> document; a human operator must.

## What's claimed

Appending an entry to a HELYX audit chain produces a new
chain whose length is exactly one greater than the
original. This is the load-bearing invariant of HELYX's
audit substrate: the chain's length is the count of
events recorded, and that count must monotonically grow
by exactly one per append — never less (lost events),
never more (phantom events), never zero (silent failure).

The Lean theorem (machine-checked):

```lean
theorem append_increments_length (chain : Chain) :
    (append chain).length = chain.length + 1 := by
  cases chain
  simp [append]
```

Lives at: `lean/Refineforge/Helyx/Audit.lean` line 47.

## The four-link trust chain (per docs/methodology.md)

### Link 1 — Lean source

| | |
|---|---|
| File | `lean/Refineforge/Helyx/Audit.lean` |
| Toolchain pin | `leanprover/lean4:v4.29.1` |
| Lake-build | passes (`refine lean check HELYX-AUDIT-001`) |
| Policy gate | clean: 0 sorries, 0 admits, 0 non-core axioms |
| Theorem | `Refineforge.Helyx.Audit.append_increments_length` |

**Machine-checked.** The proof is `cases chain` + `simp [append]`.
By the structure of `Chain` (single field `length : Nat`) and
the definition of `append` (constructs a new chain with
`length := chain.length + 1`), `simp` closes the goal.

### Link 2 — Slice ↔ HELYX source-of-truth

| | |
|---|---|
| Refineforge slice | `lean/Refineforge/Helyx/Audit.lean` |
| HELYX source-of-truth | `C:\HELYX\verified\lean\HELYX\Audit\Chain.lean` + `Append.lean` |
| Relationship | **verbatim copy, modulo namespace rename** |

The slice differs from the HELYX original only in the
namespace: `HELYX.Audit` → `Refineforge.Helyx.Audit`. All
definitions (`Chain`, `empty`, `append`, `replay`,
`tampered`) and the theorem statement + proof are
byte-identical.

**Human assertion.** Refineforge cannot verify
"verbatim copy" automatically across repository
boundaries. A human reviewer compares the two files +
signs this section on update. The HELYX commit the slice
is current with:

> _To be filled by the operator on review. Cite the
> `C:\HELYX` HEAD commit SHA + the timestamp the slice
> was re-synced._

**Recovery condition.** If the HELYX source-of-truth
evolves (new theorems, refactored definitions), the
slice must be re-synced + this claim re-reviewed. Drift
between the two is a Category-8 (trust-base) escalation
per `docs/escalation-criteria.md` v0.3.

### Link 3 — HELYX verified-claim registry

| | |
|---|---|
| Crate | `C:\HELYX\verified\checked\helyx-audit-verified\` |
| Library | `src/lib.rs` |
| Generated from Lean | yes (per the crate's own docstring) |
| Exported claim | `audit_append_claim() -> VerifiedClaim` |
| Claim's Lean module reference | `LeanModule::new("HELYX.Audit.Append")` |

The verified-claim registry is a thin Rust scaffold that
names the verified claims by Lean module path. The exported
`audit_append_claim()` function returns a `VerifiedClaim`
whose `module` field references the Lean module that proves
the claim.

**Human assertion.** The string literal
`"HELYX.Audit.Append"` in `audit_append_claim()` matches
the actual Lean module path. Refineforge's scan can verify
this (regex check), but as of this claim the scan operates
on refineforge-local paths, not HELYX paths. Cross-repo
scan is Phase 2 work; for now the operator manually
inspected the helyx-audit-verified source on 2026-05-19
and confirmed the literal is `"HELYX.Audit.Append"`.

### Link 4 — HELYX working implementation

| | |
|---|---|
| Crate | `C:\HELYX\crates\helyx-audit\` |
| Audit operations exported | append, replay, tamper-detect (per HELYX structure.md) |
| Bridge to verified-core | via `helyx-audit-verified::audit_append_claim()` |

`crates/helyx-audit/` is the working Rust impl. It wraps
the verified-claim registry to constrain its public API:
operations that would violate the verified claim are not
expressible (or trip the registry's compile-time guards).

**Human assertion.** The working impl's `append` produces
a chain whose `length` is one greater than the input. Not
machine-checked by refineforge (the impl lives in a
different repo); machine-checked by HELYX's own 41-step
CI gate (`scripts/ci/fast.ps1` step "test: helyx-audit").
The HELYX verification bundle at `C:\Users\GALO\Desktop\helyx-verification\test-run-output.txt` records this test
suite at 4643/4643 passing as of 2026-05-15 HEAD `341c6263`.

The HELYX-side test that exercises the impl is the
operator's existing audit-chain test; this refinement
doc cites it but does not re-execute it from refineforge.

## What this claim does NOT cover

Honest carve-outs:

- **Cryptographic content of audit entries.** The Lean
  theorem proves only that the length increments; it
  doesn't say anything about hash chains, signatures,
  or replay-detection. HELYX has separate theorems for
  those (`Audit/TamperDetection.lean`, `Audit/Replay.lean`,
  `Audit/Causality.lean`) — each warrants its own
  HELYX-AUDIT-NNN refineforge claim.
- **Cross-repo Rust scan.** Refineforge's scan doesn't
  check that `helyx-audit::append` exists at the cited
  path; the operator manually inspected. Phase 2 cross-
  repo scan will close this gap.
- **Live integration with HELYX's lake project.** The slice
  in `lean/Refineforge/Helyx/Audit.lean` is verbatim of
  HELYX's source-of-truth as of the operator's manual
  inspection. Drift between the two is a Cat-8 escalation;
  there's no automation today watching for drift.
- **Bit-exact reproducibility of the audit chain's runtime.**
  Separate concern (Substrate H in HELYX's architecture);
  separate refineforge claim would gate the `refine-bitexact`
  primitive against an audit-chain replay scenario.

## Operator signature

| Field | Value |
|---|---|
| Reviewer | _(to be filled)_ |
| Reviewed on | _(to be filled — ISO 8601)_ |
| HELYX commit slice is current with | _(to be filled — `C:\HELYX` HEAD SHA + date)_ |
| Signature method | git commit message + cosign sign-blob on the bundle (per `release/release.sh`) |
| Notes | First HELYX-namespace refineforge claim. Slice is verbatim-modulo-namespace copy of HELYX's source-of-truth. Cross-repo automation deferred to Phase 2. |
