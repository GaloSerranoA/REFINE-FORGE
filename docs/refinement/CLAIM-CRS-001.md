# CLAIM-CRS-001 — Refinement argument: Workspace broadcast completeness

> **Status: model-only.** The Lean model proves a property of a *model* of the
> consciousness-rs global-workspace broadcast. No Rust implementation is cited and
> `review.human_operator` is `null`. This document follows
> `docs/refinement-template.md` so the skeleton is ready if a `model+refined`
> upgrade ever links the model to real consciousness-rs Rust.
>
> **2026-05-29:** the theorem was strengthened from a vacuous `n = n` form to a
> substantive one — see `docs/verification/proof-audit.md` §Remediation.

## 1. What the Lean model says

In `lean/Refineforge/Consciousness/Claims.lean`:

| Lean entity | Kind | Meaning |
|---|---|---|
| `BroadcastTrace` | structure | `ignition_count`, `event_count`, `subscribers_ready` |
| `broadcast_complete` | def (Prop) | `subscribers_ready = true ∧ event_count = ignition_count` |
| `run` | function | runs `n` ignitions, each emitting exactly one broadcast event |
| `workspace_broadcast_complete` | theorem | `∀ n, broadcast_complete (run n)` (by induction) |
| `run_ready`, `run_counts` | theorem | per-field lemmas used by the main theorem |
| `broadcast_complete_falsifiable` | theorem | exhibits an *incomplete* trace, so the predicate is non-trivial |

Model assumption: each ignition emits exactly one broadcast event and never
clears `subscribers_ready`. The theorem proves the emitted trace is complete for
every run length; `broadcast_complete_falsifiable` shows completeness can fail,
so the statement carries information (it is not `n = n`).

## 2. What the Rust must implement

**Model-only: no Rust entity is cited for this claim.** A future `model+refined`
upgrade would map `BroadcastTrace`/`run` to the consciousness-rs global-workspace
broadcast type and its ignition loop, then run `refine scan check CLAIM-CRS-001`.
Until a real human links and reviews that code, this section intentionally cites
no Rust.

## 3. Mapping

Deferred — model-only; no Rust ↔ Lean correspondence is asserted. The
idealisation to record when this is filled in: the Lean `run` emits one event per
ignition synchronously with no loss. A real channel/back-pressure implementation
would have to argue that events are not dropped or coalesced under load before
this model applies.

## 4. Trusted code base

Even fully refined, this claim would depend on: (1) Lean's kernel; (2) the Lean
compiler v4.29.1 pinned in `lean-toolchain`; and — once Rust is linked — (3)
`rustc`/LLVM, (4) the Rust standard library, (5) the async runtime delivering
broadcasts, and (6) the OS/hardware. We make **no** claim that any of these is
itself verified.

## 5. What this claim does NOT cover

- Phenomenal or biological consciousness.
- The Rust implementation (model-only).
- Concurrency, ordering, or timing of real broadcast delivery.
- Persistence / crash recovery of in-flight ignitions.

## 6. Reviewer checklist

- [x] **[machine-checked]** `lake build` / `refine lean check CLAIM-CRS-001` →
      `Verified sorries=0 admits=0 axioms=0` (run 2026-05-29).
- [x] **[machine-checked]** `refine bundle verify artifacts/CLAIM-CRS-001`
      succeeds (re-exported 2026-05-29).
- [ ] **[needs human]** The Lean model captures what consciousness-rs
      documentation claims about workspace broadcast.
- [ ] **[needs human]** Upgrade to `model+refined` requires real Rust citations in
      §2/§3 and a second-engineer review before `review.human_operator` is set.
- _N/A_ `refine scan` — model-only, no Rust cited.
