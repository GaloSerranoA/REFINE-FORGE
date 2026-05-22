# EXAMPLE-003 - Refinement argument

> **Status of this document:** DOGFOOD. This is a production-shaped
> repo-local claim used to validate the Lean verification track after
> scanner, linter, template, and bundle hardening.

## 1. What the Lean model says

In [`lean/Refineforge/CapabilityRevocation.lean`](../../lean/Refineforge/CapabilityRevocation.lean)
we define:

| Lean entity | Kind | Meaning |
|---|---|---|
| `Right` | inductive | Finite right set: `read`, `write`, `admin`. |
| `Capability` | structure | Boolean right flags plus a `revoked` flag. |
| `holds` | function | Checks whether a capability contains a right, ignoring revocation. |
| `authorizes` | predicate | Requires `revoked = false` and `holds = true`. |
| `revoke` | function | Sets `revoked := true`. |
| `revoked_authorizes_nothing` | theorem | `revoke c` authorizes no right. |
| `fresh_capability_authorizes_held_right` | theorem | An unrevoked capability authorizes a right it holds. |
| `revoke_is_idempotent` | theorem | Revoking twice equals revoking once. |

The model is intentionally finite and does not import Mathlib.

## 2. What the Rust must implement

| Rust entity | Kind | Path | Lean counterpart |
|---|---|---|---|
| `Capability` | struct | [`crates/example-capability/src/capability.rs`](../../crates/example-capability/src/capability.rs) | `Capability` |
| `authorizes` | function | [`crates/example-capability/src/capability.rs`](../../crates/example-capability/src/capability.rs) | `authorizes` |
| `revoke` | function | [`crates/example-capability/src/capability.rs`](../../crates/example-capability/src/capability.rs) | `revoke` |

`refine scan check EXAMPLE-003` must report `Verified`.

## 3. Mapping

### 3.1 `Right` mapping

Lean uses an inductive `Right` with exactly three constructors:
`read`, `write`, and `admin`. Rust uses the finite enum `Right` with
exactly the variants `Read`, `Write`, and `Admin`. The case mapping is
one-to-one and total.

### 3.2 `Capability` mapping

Lean `Capability` stores four booleans: `read`, `write`, `admin`, and
`revoked`. Rust `Capability` stores the same four booleans as private
fields. The fields are private, so callers construct values through
`Capability::fresh` and change revocation state through `revoke`.

`#[derive(LeanModel)]` generates the structural Lean skeleton for the
Rust struct. The hand-written Lean model is more precise because it
also defines the finite `Right` domain and the authorization semantics.

### 3.3 `authorizes` mapping

Lean `authorizes c r` is true exactly when `c.revoked = false` and
`holds c r = true`. Rust `authorizes(&capability, right)` returns
`!capability.revoked && capability.holds(right)`. These are the same
truth table over the same finite right set.

### 3.4 `revoke` mapping

Lean `revoke c` returns `{ c with revoked := true }`. Rust `revoke`
consumes a `Capability` by value and returns a copy with `revoked:
true` and the existing right flags preserved. There is no public
unrevoke operation.

## 4. Trusted code base

This claim depends on:

1. Lean's kernel.
2. Lean v4.29.1 pinned by `lean/lean-toolchain`.
3. `rustc`, LLVM, and the Rust standard library.
4. The `refineforge-derive` proc macro only as a documentation aid;
   the proof does not trust macro expansion for theorem validity.
5. The OS and hardware, which are outside this claim.

No cryptographic primitive or external runtime crate is involved.

## 5. What this claim does NOT cover

- Persistence of revocations across process restarts.
- Distributed revocation propagation.
- Concurrent in-flight operations that already copied an unrevoked
  capability before revocation occurred.
- Authorization domains beyond the three fixed rights.
- Side channels and resource exhaustion.

## 6. Reviewer checklist

- [x] **[machine-checked]** `refine lean check EXAMPLE-003` succeeds.
- [x] **[machine-checked]** `refine scan check EXAMPLE-003` succeeds.
- [x] **[machine-checked]** `refine lint check EXAMPLE-003` succeeds.
- [x] **[machine-checked]** `refine bundle verify artifacts/EXAMPLE-003` succeeds.
- [x] **[machine-checked]** `cargo test -p example-capability` succeeds.
- [ ] **[needs human]** The deployment context accepts that persistence
      and distributed revocation are out of scope.
- [ ] **[needs human]** The three-right domain is sufficient for the
      intended consumer.
