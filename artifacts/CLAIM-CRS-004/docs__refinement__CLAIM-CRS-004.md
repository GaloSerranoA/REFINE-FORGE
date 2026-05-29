# CLAIM-CRS-004 — Refinement argument: Ethical gate cannot be bypassed

> **Status: model-only.** Lean proves a property of a *model* of the
> consciousness-rs safety gate. No Rust is cited; `review.human_operator` is
> `null`. This is **not** a proof of a complete ethics system.
>
> **2026-05-29:** the theorem was strengthened from a vacuous `P → P` form (it
> collapsed the gate decision and the action decision into one variable) to a real
> routing theorem with a counter-model — see `docs/verification/proof-audit.md`
> §Remediation.

## 1. What the Lean model says

In `lean/Refineforge/Consciousness/Claims.lean`:

| Lean entity | Kind | Meaning |
|---|---|---|
| `Decision` | inductive | `allow` \| `deny` |
| `Request` | structure | `dangerous : Bool` |
| `gate` | function | denies dangerous requests, else allows |
| `execute` | function | the compliant executor, defined as `gate r` (it routes through the gate) |
| `ethical_gate_non_bypass` | theorem | `∀ r, execute r = allow → gate r = allow` |
| `rogue_execute` | function | a gate-ignoring executor that always allows |
| `rogue_executor_can_bypass` | theorem | `∃ r, rogue_execute r = allow ∧ gate r = deny` |

Model assumption — and the load-bearing point: non-bypass holds **because**
`execute` is modelled as routing through `gate`. The two sides of the implication
range over the same request through two *different* functions; the property is
false for an executor that ignores the gate, which `rogue_executor_can_bypass`
proves explicitly. (The earlier version instantiated both roles to one variable
and proved only `P → P`.)

## 2. What the Rust must implement

**Model-only: no Rust entity is cited.** A future `model+refined` upgrade would
have to show the consciousness-rs action path *actually routes* every execution
decision through the safety gate (the analogue of `execute := gate`), then run
`refine scan check CLAIM-CRS-004`. The hard part of refinement here is proving the
routing link exists in code — not the implication itself.

## 3. Mapping

Deferred — model-only. Idealisation to record on upgrade: the model has a single
binary gate and a single decision point. A real system with multiple gates,
capabilities, or async pre-authorisation would need the routing theorem restated
over its actual control flow, and would have to rule out paths that reach
execution without consulting the gate.

## 4. Trusted code base

Lean kernel; Lean compiler v4.29.1; and — once Rust is linked — `rustc`/LLVM, the
Rust standard library, and every code path that can trigger an action (to argue
none bypass the gate). No claim that any is itself verified.

## 5. What this claim does NOT cover

- A complete or correct ethics/policy system — only the routing invariant.
- The Rust implementation (model-only).
- Whether `gate`'s own policy (`dangerous → deny`) is *adequate*; that is a
  separate, human judgement.
- Adversarial inputs, side channels, or TOCTOU between gate and execution.

## 6. Reviewer checklist

- [x] **[machine-checked]** `refine lean check CLAIM-CRS-004` →
      `Verified sorries=0 admits=0 axioms=0` (2026-05-29).
- [x] **[machine-checked]** `refine bundle verify artifacts/CLAIM-CRS-004`
      succeeds (re-exported 2026-05-29).
- [ ] **[needs human]** In the real system, **every** execution path provably
      routes through the gate (the `execute := gate` assumption holds in code).
- [ ] **[needs human]** The gate's policy is adequate for the deployment context.
- [ ] **[needs human]** Upgrade to `model+refined` requires real Rust citations and
      second-engineer review.
- _N/A_ `refine scan` — model-only, no Rust cited.
