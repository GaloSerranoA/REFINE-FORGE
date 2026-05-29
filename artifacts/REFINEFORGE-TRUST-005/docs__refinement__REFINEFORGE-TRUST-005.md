# REFINEFORGE-TRUST-005 — Refinement argument: bundle verification

> **Status: model+refined (human-reviewed 2026-05-29 by Galo Serrano Abad).**
> Dogfood claim: Lean proves that Refine-Forge's bundle verifier accepts iff
> every file's recomputed hash matches the manifest. The §6 items were reviewed
> and confirmed — **including the conscious acceptance that SHA-256 collision
> resistance is out of scope (sha2's job)** — so `review.human_operator` is
> populated and the agent reports `human-reviewed`.

## 1. What the Lean model says

In `lean/Refineforge/BundleVerify.lean`:

| Lean entity | Kind | Meaning |
|---|---|---|
| `verifyEntries` | function | `entries.all (fun e => recompute e.1 == e.2)` |
| `verify_implies_all_match` | theorem | **T1:** verify passes ⇒ every recomputed hash matches |
| `mismatch_implies_reject` | theorem | **T2:** one mismatched file ⇒ verify fails |
| `tamper_is_detected` | theorem | **T3:** a concrete tampered file is rejected |

`recompute path` idealises "SHA-256 of the file at `path`". T2/T3 are the
tamper-detection direction; T1 is soundness.

## 2. What the Rust must implement

| Rust entity | Kind | Path | Lean counterpart |
|---|---|---|---|
| `fn verify` | function | `crates/refineforge-cli/src/bundle.rs` | `verifyEntries` (entry point) |
| `fn verify_with_options` | function | `crates/refineforge-cli/src/bundle.rs` | `verifyEntries` (mismatch loop) |

`refine scan check REFINEFORGE-TRUST-005` confirms both symbols exist.

## 3. Mapping

`verify_with_options` collects a `mismatches` list — pushing an entry whenever a
file is missing or `expected != got` — and accepts iff `mismatches.is_empty()`.
`verifyEntries` models "no mismatch" as `entries.all (recompute · == expected)`.
**Idealisation:** SHA-256 is an opaque `String → Nat` (`recompute`); the manifest
stores expected `Nat`s. The model captures the *comparison*, not SHA-256 itself.

## 4. Trusted code base

Lean kernel; Lean compiler v4.29.1; `rustc`/LLVM; the Rust standard library; and
**the `sha2` crate** (the real SHA-256). We make no claim that any is verified —
and critically, see §5 on collision resistance.

## 5. What this claim does NOT cover

- **SHA-256 collision resistance.** This is the load-bearing limitation: a forged
  file whose SHA-256 *collides* with the manifest entry would pass verification.
  Detecting that is SHA-256's (sha2's) job, not this gate's; the model treats the
  hash as opaque and only proves the comparison is correct.
- Missing-file handling, the `report.json` hash, schema-version, and signature
  verification (`--verify-signature`) — modelled only as "an entry that doesn't
  match fails"; the richer cases are out of scope.

## 6. Reviewer checklist

- [x] **[machine-checked]** `refine lean check REFINEFORGE-TRUST-005` → `Verified`.
- [x] **[machine-checked]** `refine scan check REFINEFORGE-TRUST-005` → `Verified`.
- [x] **[machine-checked]** `refine bundle verify artifacts/REFINEFORGE-TRUST-005`.
- [x] **[needs human]** `verify_with_options` accepts iff `mismatches.is_empty()`,
      matching `verifyEntries`'s all-match. *(Galo Serrano Abad, 2026-05-29.)*
- [x] **[needs human]** Treating SHA-256 as opaque (collision resistance is sha2's
      job, §5) is acceptable for what bundle verification guarantees.
      *(Galo Serrano Abad, 2026-05-29.)*

Reviewed and confirmed by **Galo Serrano Abad on 2026-05-29**; the claim is
`human-reviewed` / `model+refined`. Changing the verify logic — or relying on
this gate for collision resistance — invalidates this review.
