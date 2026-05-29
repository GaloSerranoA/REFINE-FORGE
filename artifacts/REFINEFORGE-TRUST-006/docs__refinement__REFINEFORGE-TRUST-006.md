# REFINEFORGE-TRUST-006 — Refinement argument: escalation engine

> **Status: model+refined (human-reviewed 2026-05-29 by Galo Serrano Abad).**
> Dogfood claim: Lean proves that Refine-Forge's autonomous-driver escalation
> engine never auto-proceeds on trust-critical actions — including never
> auto-setting `review.human_operator`. The §6 items were reviewed and confirmed,
> so `review.human_operator` is populated and the agent reports `human-reviewed`.
> (Changing the classifiers or `decide` requires re-certification.)

## 1. What the Lean model says

In `lean/Refineforge/EscalationGate.lean`:

| Lean entity | Kind | Meaning |
|---|---|---|
| `Decision` | inductive | `proceed` \| `escalate` |
| `Action` | inductive | safety-critical shapes + `benign` |
| `anyClassifierFires` | function | does any of the 9 category classifiers fire? |
| `decide` | function | `escalate` iff a classifier fired, else `proceed` |
| `axiom_always_escalates` | theorem | **T1:** a custom axiom always escalates |
| `set_operator_always_escalates` | theorem | **T2:** null→value operator always escalates |
| `unknown_always_escalates` | theorem | **T3:** an unknown action always escalates |
| `proceed_implies_all_silent` | theorem | **T4:** `proceed` ⇒ no classifier fired |

T2 is the driver-level guard: the autonomous driver can never auto-record a human
operator; it must escalate to a person (complements TRUST-002/003).

## 2. What the Rust must implement

| Rust entity | Kind | Path | Lean counterpart |
|---|---|---|---|
| `fn decide` (Engine) | function | `crates/refineforge-escalation/src/engine.rs` | `decide` |
| `fn classify_custom_axiom` | function | `…/engine.rs` | the `writeAxiom` always-fire |
| `fn classify_status_upgrade` | function | `…/engine.rs` | the `setReviewOperatorFromNull` always-fire |
| `fn classify_scope` | function | `…/engine.rs` | the `unknown` always-fire |

`refine scan check REFINEFORGE-TRUST-006` confirms all four symbols exist.

## 3. Mapping

`Engine::decide` collects `hits` from the 9 classifiers and returns
`Decision::Proceed` iff `hits.is_empty()`, else `Decision::Escalate(..)`. `decide`
models exactly this (`escalate` iff `anyClassifierFires`). The three always-fire
shapes mirror the real classifiers:

- `classify_custom_axiom` returns `Some(..)` for **any** `WriteAxiom` (engine.rs).
- `classify_status_upgrade` returns `Some(..)` when `SetReviewOperator { from: None }`.
- `classify_scope` returns `Some(..)` for `Unknown` ("never silently auto-proceed").

**Idealisation:** we model four `Action` shapes (three always-fire + `benign`) of
the 30+ Rust variants, and abstract the 9 classifiers as `anyClassifierFires`.

## 4. Trusted code base

Lean kernel; Lean compiler v4.29.1; `rustc`/LLVM; the Rust standard library. No
claim that any is itself verified.

## 5. What this claim does NOT cover

- The criteria-version mismatch check (`decide` errors if the context's version
  differs) — modelled as out of scope; the model assumes a matching version.
- The multi-category **primary** selection (`pick_primary`) and packet rendering.
- The other 27 `Action` variants and the precise firing conditions of the other
  six classifiers (idealisation/external-fact/trust-base/bit-exact/etc.).
- That every real driver action is classified into one of the modelled shapes.

## 6. Reviewer checklist

- [x] **[machine-checked]** `refine lean check REFINEFORGE-TRUST-006` → `Verified`.
- [x] **[machine-checked]** `refine scan check REFINEFORGE-TRUST-006` → `Verified`.
- [x] **[machine-checked]** `refine bundle verify artifacts/REFINEFORGE-TRUST-006`.
- [x] **[needs human]** `Engine::decide` proceeds iff `hits.is_empty()`, matching
      `decide`. *(Galo Serrano Abad, 2026-05-29.)*
- [x] **[needs human]** `classify_custom_axiom`, `classify_status_upgrade`
      (null→value), and `classify_scope` (Unknown) fire as modelled (always).
      *(Galo Serrano Abad, 2026-05-29.)*
- [x] **[needs human]** Abstracting the 9 classifiers + four action shapes (§3)
      is acceptable for the guarantee being claimed. *(Galo Serrano Abad, 2026-05-29.)*

Reviewed and confirmed by **Galo Serrano Abad on 2026-05-29**; the claim is
`human-reviewed` / `model+refined`. Changing the classifiers, `decide`, or the
action model invalidates this review.
