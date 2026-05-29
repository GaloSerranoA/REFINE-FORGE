# REFINEFORGE-TRUST-004 — Refinement argument: run-all aggregate trust

> **Status: model-linked (NOT human-reviewed).** Dogfood claim: Lean proves that
> Refine-Forge's `run_all` trust aggregation can never claim more trust than its
> weakest agent. `review.human_operator` is `null` → `model-linked`.

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
- [ ] **[needs human]** Rust `lowest_trust`'s cascade returns the lowest-rank
      member, matching Lean `lowest` (min by `rank`).
- [ ] **[needs human]** Reusing TRUST-001's `rank`/`TrustLevel` correspondence is
      acceptable (this claim depends on TRUST-001).

Once certified, populate `review.human_operator`; the claim moves to
`human-reviewed` / `model+refined`.
