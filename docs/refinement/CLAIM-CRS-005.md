# CLAIM-CRS-005 — Refinement argument: Phi-proxy is a deterministic, monotone metric

> **Status: model-only.** Lean proves properties of a *model* of the
> consciousness-rs Phi-proxy metric. No Rust is cited; `review.human_operator` is
> `null`. This is **not** IIT Phi and **not** a consciousness measure.
>
> **2026-05-29:** the vacuous `x = x` "determinism" theorem was replaced with a
> closed-form characterization plus monotonicity theorems — see
> `docs/verification/proof-audit.md` §Remediation.

## 1. What the Lean model says

In `lean/Refineforge/Consciousness/Claims.lean`:

| Lean entity | Kind | Meaning |
|---|---|---|
| `phi_proxy` | function | `phi_proxy nodes edges = nodes + edges` |
| `phi_proxy_deterministic` | theorem | closed form: `phi_proxy nodes edges = nodes + edges` |
| `phi_proxy_monotone_in_nodes` | theorem | `n₁ ≤ n₂ → phi_proxy n₁ e ≤ phi_proxy n₂ e` |
| `phi_proxy_monotone_in_edges` | theorem | `e₁ ≤ e₂ → phi_proxy n e₁ ≤ phi_proxy n e₂` |

Model assumption: `phi_proxy` is a pure function of two counts. Determinism of a
pure function is automatic, so `phi_proxy_deterministic` records the *closed
form* (no hidden state or randomness) rather than asserting `x = x`. The genuine
content is monotonicity in each argument — false for a metric that subtracted a
count, so the theorems are not vacuous.

## 2. What the Rust must implement

**Model-only: no Rust entity is cited.** A future `model+refined` upgrade would
map `phi_proxy` to the consciousness-rs metric function and its input
normalisation, then run `refine scan check CLAIM-CRS-005`.

## 3. Mapping

Deferred — model-only. Idealisation to record on upgrade: the Lean metric is
`nodes + edges` over `Nat`. A real metric using floats, saturation, or weighting
would need its own monotonicity/closed-form theorems; the `Nat` additive model
must not be passed off as the production formula.

## 4. Trusted code base

Lean kernel; Lean compiler v4.29.1; and — once Rust is linked — `rustc`/LLVM, the
Rust standard library, and the numeric type used by the real metric (overflow /
float-rounding behaviour). No claim that any is itself verified.

## 5. What this claim does NOT cover

- IIT Phi, integrated information, or any consciousness measure.
- The Rust implementation (model-only).
- Numeric edge cases (overflow, float rounding) of a real metric.
- Whether `nodes + edges` is a *meaningful* proxy — that is a modelling judgement,
  not something the theorem establishes.

## 6. Reviewer checklist

- [x] **[machine-checked]** `refine lean check CLAIM-CRS-005` →
      `Verified sorries=0 admits=0 axioms=0` (2026-05-29).
- [x] **[machine-checked]** `refine bundle verify artifacts/CLAIM-CRS-005`
      succeeds (re-exported 2026-05-29).
- [ ] **[needs human]** The real metric's formula and numeric type match the `Nat`
      additive model (or the differences are documented in §3).
- [ ] **[needs human]** Upgrade to `model+refined` requires real Rust citations and
      second-engineer review.
- _N/A_ `refine scan` — model-only, no Rust cited.
