# CLAIM-CRS-003 — Refinement argument: Narrative log is append-only

> **Status: model-only.** Lean proves a property of a *model* of the
> consciousness-rs narrative log. No Rust is cited; `review.human_operator` is
> `null`. This is the strongest future-refinement candidate among the CRS claims.
>
> **2026-05-29:** the theorem was strengthened from a vacuous `x = x` form to a
> history-preservation theorem — see `docs/verification/proof-audit.md`
> §Remediation.

## 1. What the Lean model says

In `lean/Refineforge/Consciousness/Claims.lean`:

| Lean entity | Kind | Meaning |
|---|---|---|
| `Log` | structure | `entries : List String` |
| `append` | function | `{ entries := l.entries ++ [e] }` |
| `narrative_append_only` | theorem | `∃ rest, (append l e).entries = l.entries ++ rest` |
| `narrative_append_increments_length` | theorem | length grows by exactly one |
| `reset_violates_append_only` | theorem | emptying a non-empty log is **not** expressible as an append |

Model assumption: `append` is the only mutation, and it only concatenates. The
main theorem proves *history preservation* — nothing prior is removed or
reordered (the old log is a prefix of the new). `reset_violates_append_only`
shows a history-dropping operation provably fails the property, so it is not
vacuous.

## 2. What the Rust must implement

**Model-only: no Rust entity is cited.** A future `model+refined` upgrade would
map `Log`/`append` to the consciousness-rs narrative/event-log type and its push
routine, then run `refine scan check CLAIM-CRS-003`. This is the CRS claim most
worth refining first, because the property (append-only history) is exactly what
an audit-style narrative log should guarantee.

## 3. Mapping

Deferred — model-only. Idealisation to record on upgrade: the Lean `Log` is an
in-memory `List` with total, infallible append. A persistent log must argue that
a crash mid-append cannot truncate or reorder prior entries (durability/atomicity
is **not** in the model).

## 4. Trusted code base

Lean kernel; Lean compiler v4.29.1; and — once Rust is linked — `rustc`/LLVM, the
Rust standard library, and (critically for a real log) the storage backend and
its atomicity guarantees. No claim that any is itself verified.

## 5. What this claim does NOT cover

- The Rust implementation (model-only).
- Persistence durability or crash atomicity.
- Concurrent appends / ordering across threads.
- Compaction, truncation, or retention policies.

## 6. Reviewer checklist

- [x] **[machine-checked]** `refine lean check CLAIM-CRS-003` →
      `Verified sorries=0 admits=0 axioms=0` (2026-05-29).
- [x] **[machine-checked]** `refine bundle verify artifacts/CLAIM-CRS-003`
      succeeds (re-exported 2026-05-29).
- [ ] **[needs human]** The in-memory `List` model is an acceptable abstraction of
      the real narrative log (esp. durability assumptions in §5).
- [ ] **[needs human]** Upgrade to `model+refined` requires real Rust citations and
      second-engineer review.
- _N/A_ `refine scan` — model-only, no Rust cited.
