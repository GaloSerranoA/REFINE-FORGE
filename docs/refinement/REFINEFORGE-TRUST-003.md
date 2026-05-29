# REFINEFORGE-TRUST-003 — Refinement argument: human-approval acceptance gate

> **Status: model+refined (human-reviewed 2026-05-29 by Galo Serrano Abad).**
> Dogfood claim: Lean proves a property of a model of Refine-Forge's *own*
> human-approval acceptance gate (`validate_human_approval`). It composes with
> REFINEFORGE-TRUST-002 to show an automated operator can never produce an
> accepted approval — the formal statement of "human review cannot be recorded
> by an AI". The §6 `[needs human]` items were reviewed and confirmed, so
> `review.human_operator` is populated and the Lean agent reports
> `human-reviewed`. (This claim depends on TRUST-002; invalidating that review,
> or changing the Rust acceptance conjunction or `human_ok`, requires
> re-certification.)

## 1. What the Lean model says

In `lean/Refineforge/ApprovalGate.lean` (`Refineforge.ApprovalGate`):

| Lean entity | Kind | Meaning |
|---|---|---|
| `humanOk` | function | `operatorNonEmpty && !OperatorGate.isAutomated tokens` |
| `accepts` | function | `schemaOk && roleOk && decisionOk && approvedAtOk && summaryOk && humanOk …` |
| `accepts_implies_human` | theorem | **T1:** `accepts … = true → humanOk … = true` |
| `automated_operator_rejected` | theorem | **T2:** `isAutomated tokens = true → accepts … = false` |
| `all_checks_accept` | theorem | **T3:** all six checks true ⇒ `accepts … = true` |
| `claude_cannot_approve` | theorem | **T4:** `accepts true true true true true true ["claude"] = false` |

T2 is the load-bearing composition: it reuses `OperatorGate.isAutomated`
(REFINEFORGE-TRUST-002), so the anti-spoofing guarantee propagates from the
operator gate to the whole approval decision. T4 is the concrete witness: even
with every other check satisfied, an approval naming "claude" is rejected.

## 2. What the Rust must implement

| Rust entity | Kind | Path | Lean counterpart |
|---|---|---|---|
| `fn validate_human_approval` | function | `crates/refineforge-cli/src/agent/common.rs` | `accepts` |
| `fn is_automated_operator` | function | `crates/refineforge-cli/src/agent/common.rs` | `isAutomated` (via TRUST-002) |

`refine scan check REFINEFORGE-TRUST-003` confirms both symbols exist at the path.

## 3. Mapping

### 3.1 `accepts` ↔ the Rust acceptance conjunction

Rust `validate_human_approval` ends with:

```rust
let human_ok = !operator.is_empty() && !is_automated_operator(operator);
validation.passed =
    schema_ok && role_ok && decision_ok && approved_at_ok && summary_ok && human_ok;
```

The Lean `accepts` is the same six-way conjunction in the same order.
**Idealisation:** the five non-operator flags (`schema_ok`, `role_ok`,
`decision_ok`, `approved_at_ok`, `summary_ok`) are modelled as opaque `Bool`s.
The Rust derives them from JSON field comparisons
(`schema_version == "refineforge-human-approval-v1"`, role match,
`decision == "approved"`, non-empty `approved_at`, non-empty `evidence_summary`);
those comparisons are NOT modelled here — the Lean takes the flags as given.

### 3.2 `humanOk` ↔ the Rust `human_ok`

`humanOk operatorNonEmpty tokens = operatorNonEmpty && !isAutomated tokens`
mirrors `!operator.is_empty() && !is_automated_operator(operator)`.
`operatorNonEmpty` models `!operator.is_empty()`; `isAutomated` is the
REFINEFORGE-TRUST-002 model and carries that claim's tokenisation idealisation.

## 4. Trusted code base

Conditional on: (1) Lean's kernel; (2) the Lean compiler v4.29.1; (3)
`rustc`/LLVM; (4) the Rust standard library; (5) the §3.1/§3.2 correspondences;
and (6) REFINEFORGE-TRUST-002's own trusted base (it is a dependency). We make
**no** claim that any of (1)–(4) is itself verified.

## 5. What this claim does NOT cover

- The JSON parsing and field extraction that produce the five opaque flags
  (schema/role/decision/approved_at/summary) — only the acceptance *logic* is
  modelled, not the string comparisons or file I/O.
- The completeness of `is_automated_operator` (a denylist, per
  REFINEFORGE-TRUST-002 §5) — a novel automated name not on the blocklist would
  pass `humanOk`.
- The callers of `validate_human_approval` (the agents/approval flow); this claim
  is about the function's decision, not where it is invoked.

## 6. Reviewer checklist

- [x] **[machine-checked]** `refine lean check REFINEFORGE-TRUST-003` →
      `Verified sorries=0 admits=0 axioms=0`.
- [x] **[machine-checked]** `refine scan check REFINEFORGE-TRUST-003` →
      `Verified` (`validate_human_approval`, `is_automated_operator` present).
- [x] **[machine-checked]** `refine bundle verify artifacts/REFINEFORGE-TRUST-003`
      succeeds.
- [x] **[needs human]** The Rust `validation.passed = …` conjunction matches the
      Lean `accepts` (same six flags, same order). *(Galo Serrano Abad, 2026-05-29.)*
- [x] **[needs human]** `human_ok` in Rust equals `humanOk` in Lean
      (`!is_empty && !is_automated_operator`). *(Galo Serrano Abad, 2026-05-29.)*
- [x] **[needs human]** Modelling the five non-operator checks as opaque Bools
      (§3.1) is acceptable — their JSON-derivation is out of scope by design.
      *(Galo Serrano Abad, 2026-05-29.)*

Reviewed and confirmed by **Galo Serrano Abad on 2026-05-29**;
`review.human_operator` is populated and the claim is `human-reviewed` / scope
`model+refined`. This review depends on REFINEFORGE-TRUST-002; invalidating that
review, or changing the Rust acceptance conjunction or `human_ok`, requires
re-certification.
