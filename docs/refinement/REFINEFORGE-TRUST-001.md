# REFINEFORGE-TRUST-001 — Refinement argument: agent trust-ceiling enforcement

> **Status: model-linked (NOT human-reviewed).** This is a dogfood claim: Lean
> proves a property of a model of Refine-Forge's *own* agent trust system, and
> this document bridges that model to the real Rust. `review.human_operator` is
> `null`, so the Lean agent's honest trust for this claim is `model-linked`. The
> only thing standing between this claim and `human-reviewed` is a real human
> reviewer signing off on §6 — which is exactly how the system is supposed to
> work.

## 1. What the Lean model says

In `lean/Refineforge/AgentTrust.lean` (`Refineforge.AgentTrust`):

| Lean entity | Kind | Meaning |
|---|---|---|
| `TrustLevel` | inductive | the 7-level trust lattice (`blocked` … `humanReviewed`) |
| `rank` | function | `TrustLevel → Nat`, assurance rank `0..=6` |
| `enforce` | function | `if rank reported ≤ rank ceiling then reported else ceiling` |
| `enforce_never_exceeds_ceiling` | theorem | **T1 (safety):** `rank (enforce reported ceiling) ≤ rank ceiling` |
| `enforce_keeps_when_within_ceiling` | theorem | **T2 (faithfulness):** `rank reported ≤ rank ceiling → enforce reported ceiling = reported` |
| `enforce_idempotent` | theorem | **T3:** `enforce (enforce reported ceiling) ceiling = enforce reported ceiling` |

T1 is the load-bearing one: it says no reported trust level can come out ranked
above the ceiling. It would be false for an `enforce` that ignored the ceiling,
so it is a genuine (non-vacuous) safety statement.

## 2. What the Rust must implement

| Rust entity | Kind | Path | Lean counterpart |
|---|---|---|---|
| `TrustLevel` | enum | `crates/refineforge-cli/src/agent/common.rs` | `TrustLevel` |
| `fn trust_rank` | function | `crates/refineforge-cli/src/agent/common.rs` | `rank` |
| `fn enforce_trust_ceiling` | function | `crates/refineforge-cli/src/agent/common.rs` | `enforce` (resulting level) |

`refine scan check REFINEFORGE-TRUST-001` confirms each entity exists at the
cited path (static name-presence check).

## 3. Mapping

### 3.1 `TrustLevel` ↔ Rust `TrustLevel`

The Rust enum has the same seven variants in the same order
(`Blocked, MeasuredOnly, ModelOnly, ModelLinked, ReleaseReadyLocal,
ReleaseReadyCi, HumanReviewed`). A reviewer must confirm the variant set and
order match the Lean lattice exactly (an added/removed/reordered variant would
break the correspondence).

### 3.2 `rank` ↔ `trust_rank`

Rust `trust_rank` maps each variant to `0..=6` with the same assignment as Lean
`rank`. **Idealisation:** the model assumes these two mappings agree
variant-for-variant; this is a [needs human] check in §6.

### 3.3 `enforce` ↔ `enforce_trust_ceiling`

Rust:

```rust
fn enforce_trust_ceiling(report: &mut AgentReport, trust_ceiling: TrustLevel) -> bool {
    if trust_rank(report.trust_level) <= trust_rank(trust_ceiling) { return false; }
    report.trust_level = trust_ceiling; // (+ warning, returns true)
    true
}
```

After the call, `report.trust_level` equals
`if trust_rank(reported) <= trust_rank(ceiling) { reported } else { ceiling }`,
which is exactly Lean `enforce reported ceiling`. **Idealisations (out of model
scope):**

- The Rust returns a `capped: bool` and pushes a human-readable warning. The
  Lean models only the *resulting trust level*, not those side effects.
- The Lean proves a property of the **function**. It does not prove that every
  agent call site actually routes its reported trust through
  `enforce_trust_ceiling` (each agent's `seal_runtime` does; verifying that for
  all four agents is a [needs human] check).

## 4. Trusted code base

Conditional on: (1) Lean's kernel; (2) the Lean compiler v4.29.1; (3)
`rustc`/LLVM; (4) the Rust standard library; and (5) the §3.2/§3.3 idealisations
being faithful, the cited Rust enforces the T1 safety invariant. We make **no**
claim that any of (1)–(4) is itself verified.

## 5. What this claim does NOT cover

- The Rust side effects of `enforce_trust_ceiling` (the `capped` return value
  and the warning string) — only the resulting trust level is modelled.
- That callers invoke `enforce_trust_ceiling` on every path that sets a report's
  trust level (the model is about the function, not the call graph).
- Concurrency, panics, or memory safety of the surrounding agent runtime.
- Whether the *ranks themselves* express the right policy — only that
  enforcement respects whatever ranks `trust_rank` assigns.

## 6. Reviewer checklist

- [x] **[machine-checked]** `refine lean check REFINEFORGE-TRUST-001` →
      `Verified sorries=0 admits=0 axioms=0` (2026-05-29).
- [x] **[machine-checked]** `refine scan check REFINEFORGE-TRUST-001` →
      `Verified` (TrustLevel, trust_rank, enforce_trust_ceiling all present).
- [x] **[machine-checked]** `refine bundle verify artifacts/REFINEFORGE-TRUST-001`
      succeeds (2026-05-29).
- [ ] **[needs human]** The seven `TrustLevel` variants and their `trust_rank`
      values match the Lean `TrustLevel`/`rank` exactly.
- [ ] **[needs human]** `enforce_trust_ceiling`'s resulting `report.trust_level`
      equals Lean `enforce` for all inputs (the §3.3 correspondence).
- [ ] **[needs human]** Every agent path that sets `report.trust_level` is
      bounded by `enforce_trust_ceiling` (or is otherwise within its ceiling).

Once a second engineer certifies the [needs human] items, populate
`review.human_operator` with their real name and the claim can move to
`human-reviewed`.
