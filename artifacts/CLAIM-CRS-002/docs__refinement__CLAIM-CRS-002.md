# CLAIM-CRS-002 — Refinement argument: Workspace capacity bound

> **Status: model-only.** Lean proves a property of a *model* of consciousness-rs
> workspace capacity. No Rust is cited; `review.human_operator` is `null`.
>
> **2026-05-29:** the theorem was strengthened from a vacuous `P → P` form (it
> returned its own hypothesis) to a real preservation theorem about an admission
> operation — see `docs/verification/proof-audit.md` §Remediation.

## 1. What the Lean model says

In `lean/Refineforge/Consciousness/Claims.lean`:

| Lean entity | Kind | Meaning |
|---|---|---|
| `Workspace` | structure | `content`, `capacity` |
| `within_capacity` | def (Prop) | `content ≤ capacity` |
| `accept` | function | adds one item iff `content < capacity`; otherwise a no-op |
| `workspace_capacity_bound` | theorem | `within_capacity w → within_capacity (accept w)` |
| `accept_saturates_at_capacity` | theorem | at capacity, `accept w = w` (no overflow) |
| `over_capacity_exists` | theorem | an over-capacity state exists, so `within_capacity` is non-trivial |

Model assumption: `accept` is the only operation that adds content, and it
checks the bound first. The main theorem proves admission *preserves* the bound;
saturation proves the check is load-bearing. False for an `accept` that
incremented unconditionally — so the statement is not vacuous.

## 2. What the Rust must implement

**Model-only: no Rust entity is cited.** A future `model+refined` upgrade would
map `Workspace`/`accept` to the consciousness-rs workspace buffer and its
admission/eviction routine, then run `refine scan check CLAIM-CRS-002`.

## 3. Mapping

Deferred — model-only. Idealisation to record on upgrade: the Lean model adds at
most one item per `accept` and has no eviction; a real buffer with eviction or
batch admission would need a theorem about *those* operations, not just `accept`.

## 4. Trusted code base

Lean kernel; Lean compiler v4.29.1; and — once Rust is linked — `rustc`/LLVM, the
Rust standard library (collection capacity semantics), and the OS/hardware. No
claim that any is itself verified.

## 5. What this claim does NOT cover

- The Rust implementation (model-only).
- Eviction, replacement, or priority policies (not modelled).
- Concurrent admission from multiple threads.
- Memory exhaustion below the logical capacity bound.

## 6. Reviewer checklist

- [x] **[machine-checked]** `refine lean check CLAIM-CRS-002` →
      `Verified sorries=0 admits=0 axioms=0` (2026-05-29).
- [x] **[machine-checked]** `refine bundle verify artifacts/CLAIM-CRS-002`
      succeeds (re-exported 2026-05-29).
- [ ] **[needs human]** The model's single-item, no-eviction `accept` matches the
      consciousness-rs admission policy.
- [ ] **[needs human]** Upgrade to `model+refined` requires real Rust citations and
      second-engineer review.
- _N/A_ `refine scan` — model-only, no Rust cited.
