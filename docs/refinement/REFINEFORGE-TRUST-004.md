# REFINEFORGE-TRUST-004 — Refinement argument: run-all aggregate trust

> **Status: model+refined (human-reviewed 2026-05-29 by Galo Serrano Abad).**
> Dogfood claim: Lean proves that Refine-Forge's `run_all` trust aggregation can
> never claim more trust than its weakest agent. The §6 items were reviewed and
> confirmed, so `review.human_operator` is populated and the agent reports
> `human-reviewed`. (Depends on TRUST-001; changing `lowest_trust` requires
> re-certification.)

## 1. What the Lean model says

In `lean/Refineforge/AggregateTrust.lean` (reuses `TrustLevel`/`rank` from
TRUST-001's `Refineforge.AgentTrust`):

| Lean entity | Kind | Meaning |
|---|---|---|
| `lowerOf` | function | the lower-rank of two levels |
| `lowest` | function | the lowest level across a list (empty ⇒ `humanReviewed`) |
| `lowest_le_member` | theorem | **T1:** `∀ t ∈ ts, rank (lowest ts) ≤ rank t` |
| `aggregate_picks_weakest` | theorem | **T2:** a measured-only member drags the aggregate to measured-only |

T1 is the safety property: the run-all summary is bounded above by every
member's rank, so the aggregate cannot over-trust. It would be false for an
aggregator returning the *highest* member — so it is not vacuous.

## 2. What the Rust must implement

| Rust entity | Kind | Path | Lean counterpart |
|---|---|---|---|
| `fn lowest_trust` | function | `crates/refineforge-cli/src/agent/mod.rs` | `lowest` |
| `fn lowest_trust_ceiling` | function | `crates/refineforge-cli/src/agent/mod.rs` | `lowest` (over ceilings) |

`refine scan check REFINEFORGE-TRUST-004` confirms both symbols exist.

## 3. Mapping

Rust `lowest_trust` is an if-else cascade from `Blocked` up to `HumanReviewed`
that returns the lowest-rank trust present among the reports; `lowest` models the
same "minimum by rank". `lowest_trust_ceiling` first copies each report's trust
to its ceiling, then calls `lowest_trust`; T1 applies to that composition.
**Idealisation:** the empty-list case — Lean `lowest [] = humanReviewed` —
matches the Rust `else` branch (empty reports ⇒ HumanReviewed); `run_all` always
has ≥1 report in practice. The model reuses TRUST-001's `rank`, so it inherits
that claim's correspondence.

## 4. Trusted code base

Lean kernel; Lean compiler v4.29.1; `rustc`/LLVM; the Rust standard library; and
TRUST-001's trusted base (the `rank`/`TrustLevel` correspondence). No claim that
any is itself verified.

## 5. What this claim does NOT cover

- The `run_all` **status** aggregation (the `AgentStatus` field) — only the
  trust-level minimum is modelled.
- The `lowest_trust_ceiling` synthetic-report copy step (it is argued, not
  separately modelled).
- That the four agents are actually run / their individual trust values are
  correct (those are the per-agent claims).

## 6. Reviewer checklist

- [x] **[machine-checked]** `refine lean check REFINEFORGE-TRUST-004` → `Verified`.
- [x] **[machine-checked]** `refine scan check REFINEFORGE-TRUST-004` → `Verified`.
- [x] **[machine-checked]** `refine bundle verify artifacts/REFINEFORGE-TRUST-004`.
- [x] **[needs human]** Rust `lowest_trust`'s cascade returns the lowest-rank
      member, matching Lean `lowest` (min by `rank`). *(Galo Serrano Abad, 2026-05-29.)*
- [x] **[needs human]** Reusing TRUST-001's `rank`/`TrustLevel` correspondence is
      acceptable (this claim depends on TRUST-001). *(Galo Serrano Abad, 2026-05-29.)*

Reviewed and confirmed by **Galo Serrano Abad on 2026-05-29**; the claim is
`human-reviewed` / `model+refined`. Depends on TRUST-001; invalidating that
review or changing `lowest_trust` requires re-certification.
