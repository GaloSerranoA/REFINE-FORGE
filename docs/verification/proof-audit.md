# Proof Audit — informational content of every Lean theorem

> Snapshot date: 2026-05-29
> Method: source inspection of all 12 claim theorems in `lean/Refineforge/**`.
> Each goal below is computed by hand-unfolding the `def`s shown in the same
> file.

> **STATUS — partially remediated 2026-05-29.** This audit was first written
> against a tree in which the five CLAIM-CRS-\* theorems were **vacuous**
> (Tier C: `x = x` / `P → P`). Those five were rewritten the same day to be
> substantive; the "as-found" analysis below is preserved as the historical
> record of *why* they were empty, and each affected entry now carries a
> **Remediated →** pointer to the [Remediation](#remediation-applied-2026-05-29)
> section. Verification of the rewrite: `lake build` green and
> `refine lean check-all` reports all nine claims
> `Verified  sorries=0 admits=0 axioms=0` (see [How to reproduce](#how-to-reproduce)).

## Why this doc exists

[`proof-inventory.md`](proof-inventory.md) records, for each claim, the *proof
shape* (`rfl`, `exact h`, `simp`, …) and the *scope* (`model-only`,
`tutorial`). It is honest that `status: proven` does not mean the Rust is
verified.

It did **not** originally grade the one thing a reader most wants to know: *does
the theorem assert anything?* A proof can build green, contain no `sorry`, and
still prove `x = x`. This document adds that missing axis — an
**informational-content grade** — for every theorem, and shows the unfolded goal
so each grade is checkable. (The grade column has since been back-ported into
`proof-inventory.md`.)

## Grading rubric

Each theorem is graded by the test: **could this statement be false under some
alternative definition of the model's own operations?**

| Tier | Name | Test | Information |
|---|---|---|---|
| **A** | Substantive (vs. its model) | Universally quantified over the model's objects; rules out a class of misbehavior, or relates two *independent* quantities. Could be false under a different `def`. | Real, even if elementary. |
| **B** | Definitional / elementary | Pins a definition or states an elementary one-step fact. Could be false under a different `def`, but the proof is a single unfold or one library lemma. | Low but non-zero — anchors the spec, catches a definition typo. |
| **C** | Vacuous / tautological | Goal reduces to a logical tautology (`x = x`, `P → P`) that holds under *every* model. Often rigged: the witness is constructed to satisfy the goal, or two distinct roles are collapsed into one variable. | None. |

This grade is **orthogonal to "proven"**. Every theorem is genuinely proven in
Lean. A Tier C theorem is proven *and* empty.

## Summary

The **Grade** column reads *as-found → now*. Entries with a single grade were
unchanged by the 2026-05-29 remediation.

| # | Theorem | File:line | Backs claim | Grade | Verdict |
|---|---|---|---|---|---|
| 1 | `add_comm_demo` | `Example.lean:27` | EXAMPLE-001 | **B** | Real fact (`a+b=b+a`), but a stdlib re-export. Honestly labeled hello-world. |
| 2 | `incr_monotone` | `Counter.lean:49` | EXAMPLE-002 | **B** | `n+1 ≥ n`. Elementary, library-discharged. |
| 3 | `incr_strictly_increases` | `Counter.lean:57` | EXAMPLE-002 | **B** | `n+1 > n`. Refinement doc honestly flags the `u64` saturation gap. |
| 4 | `append_increments_length` | `Helyx/Audit.lean:46` | HELYX-AUDIT-001 | **B** | Definitional restatement: unfolds to `len+1 = len+1`. Anchors the def, nothing more. |
| 5 | `workspace_broadcast_complete` | `Consciousness/Claims.lean` | CLAIM-CRS-001 | **C → A** | Was a rigged `n = n`. Now: completeness of the `run` ignition process, by induction; falsifiable. |
| 6 | `workspace_capacity_bound` | `Consciousness/Claims.lean` | CLAIM-CRS-002 | **C → A** | Was `P → P` (returned its hypothesis). Now: `accept` preserves the bound; saturates at capacity. |
| 7 | `narrative_append_only` | `Consciousness/Claims.lean` | CLAIM-CRS-003 | **C → A** | Was a rigged `x = x`. Now: append is history-preserving (`∃ rest, new = old ++ rest`). |
| 8 | `ethical_gate_non_bypass` | `Consciousness/Claims.lean` | CLAIM-CRS-004 | **C → A** | Was `P → P` (two roles collapsed). Now: `execute` routed through `gate`; rogue-executor companion exhibits a bypass. |
| 9 | `phi_proxy_deterministic` | `Consciousness/Claims.lean` | CLAIM-CRS-005 | **C → B** (+A) | Was `x = x`. Now: closed-form characterization (B) + two monotonicity theorems (A). |
| 10 | `revoked_authorizes_nothing` | `CapabilityRevocation.lean:46` | EXAMPLE-003 | **A** | Genuine ∀-safety property: a revoked cap authorizes no right. |
| 11 | `fresh_capability_authorizes_held_right` | `CapabilityRevocation.lean:52` | EXAMPLE-003 | **B** | Near-definitional: `false=false ∧ h`. |
| 12 | `revoke_is_idempotent` | `CapabilityRevocation.lean:59` | EXAMPLE-003 | **A** | Genuine algebraic property: `revoke∘revoke = revoke`. |

**Tally — as-found: A = 2, B = 5, C = 5. After remediation: A = 6, B = 6, C = 0.**
(The strengthened CRS file also adds 10 companion theorems — `run_ready`,
`run_counts`, `broadcast_complete_falsifiable`, `accept_saturates_at_capacity`,
`over_capacity_exists`, `narrative_append_increments_length`,
`reset_violates_append_only`, `rogue_executor_can_bypass`,
`phi_proxy_monotone_in_nodes`, `phi_proxy_monotone_in_edges` — not counted in the
12.)

### Headline finding (as-found) — RESOLVED 2026-05-29

The as-found audit found an inversion: the only **Tier A** theorems (#10, #12)
belonged to **EXAMPLE-003**, a *tutorial*, while every **Tier C** theorem
belonged to a grand-titled **CLAIM-CRS-\*** claim ("Ethical gate non-bypass,"
"Phi-proxy determinism") that proved `x = x` or `P → P`. The honest tutorial
proved more than the "consciousness" and "ethics" claims did.

The remediation removed the inversion: all five CRS theorems are now Tier A/B and
each ships a falsifiability companion. See [Remediation](#remediation-applied-2026-05-29).

---

## Per-theorem analysis (as-found)

The five CRS entries below describe the **pre-remediation** statements. They are
retained because they explain the failure pattern precisely; the current
statements are in the [Remediation](#remediation-applied-2026-05-29) section.

### 1. `add_comm_demo` — `Example.lean:27` — Tier B

```lean
theorem add_comm_demo (a b : Nat) : a + b = b + a := Nat.add_comm a b
```

- **Goal:** `a + b = b + a`. **Proof:** re-export of `Nat.add_comm`.
- **Verdict:** a genuine universal fact, but the proof contributes nothing new.
  The source calls it "hello-world," which is accurate.

### 2. `incr_monotone` — `Counter.lean:49` — Tier B

```lean
def incr (c : Counter) : Counter := { value := c.value + 1 }
theorem incr_monotone (c : Counter) : (incr c).value ≥ c.value := by simp [incr]
```

- **Goal (unfolded):** `c.value + 1 ≥ c.value`. Elementary, `simp`-discharged,
  universally quantified — not vacuous, but a single one-step fact.

### 3. `incr_strictly_increases` — `Counter.lean:57` — Tier B

```lean
theorem incr_strictly_increases (c : Counter) : (incr c).value > c.value := by simp [incr]
```

- **Goal (unfolded):** `c.value + 1 > c.value`. The EXAMPLE-002 refinement doc
  honestly discloses that the Rust `u64` does **not** preserve this at
  `u64::MAX` — the idealisation pattern the framework exists to teach.

### 4. `append_increments_length` — `Helyx/Audit.lean:46` — Tier B (definitional)

```lean
def append (chain : Chain) : Chain := { length := chain.length + 1 }
theorem append_increments_length (chain : Chain) :
    (append chain).length = chain.length + 1 := by cases chain; simp [append]
```

- **Goal (unfolded):** `chain.length + 1 = chain.length + 1`. A definitional
  restatement; it pins the `def` but asserts nothing beyond it. The substantive
  content HELYX-AUDIT-001 advertises (the real audit chain is append-only **and
  tamper-evident**) lives in the unverified refinement doc; the model even
  defines `tampered`/`replay` but proves **no theorem** about either.

### 5. `workspace_broadcast_complete` — CLAIM-CRS-001 — Tier C (as-found)

```lean
def broadcast_complete (trace : BroadcastTrace) : Prop := trace.event_count = trace.ignition_count
theorem workspace_broadcast_complete (n : Nat) :
    broadcast_complete { ignition_count := n, event_count := n, subscribers_ready := true } := by rfl
```

- **Goal (unfolded):** `n = n`. The statement hard-codes both fields to `n`, then
  proves they are equal. It never quantifies over a *trace*, so no incomplete
  trace is expressible and nothing is ruled out. **Remediated →**
  [CRS-001](#crs-001-workspace_broadcast_complete).

### 6. `workspace_capacity_bound` — CLAIM-CRS-002 — Tier C (as-found)

```lean
def within_capacity (content_items capacity : Nat) : Prop := content_items <= capacity
theorem workspace_capacity_bound (content_items capacity : Nat)
    (h : content_items <= capacity) : within_capacity content_items capacity := by exact h
```

- **Goal (unfolded):** `content_items ≤ capacity` — exactly the hypothesis. The
  theorem is the identity `h ↦ h`; it assumes its own conclusion. **Remediated →**
  [CRS-002](#crs-002-workspace_capacity_bound).

### 7. `narrative_append_only` — CLAIM-CRS-003 — Tier C (as-found)

```lean
def append_only_step (before after : Nat) : Prop := after = before + 1
theorem narrative_append_only (before : Nat) :
    append_only_step before (before + 1) := by rfl
```

- **Goal (unfolded):** `before + 1 = before + 1`. The `after` value is *supplied*
  as `before + 1`; nothing non-append-only is expressible. **Remediated →**
  [CRS-003](#crs-003-narrative_append_only).

### 8. `ethical_gate_non_bypass` — CLAIM-CRS-004 — Tier C (as-found) — most misleading

```lean
def gate_non_bypass (gate_result action_result : SafetyDecision) : Prop :=
  action_result = SafetyDecision.allow -> gate_result = SafetyDecision.allow
theorem ethical_gate_non_bypass (gate_result : SafetyDecision) :
    gate_non_bypass gate_result gate_result := by intro h; exact h
```

- **Goal (unfolded):** `gate_result = allow → gate_result = allow`. The property's
  only content is the relationship between two **independent** values (the gate's
  decision and the action's). The theorem instantiates both to the same variable,
  leaving `P → P`. The tell: the general `∀ g a, gate_non_bypass g a` is *false*
  (`g := deny, a := allow`), so only the diagonal was proved. **Remediated →**
  [CRS-004](#crs-004-ethical_gate_non_bypass).

### 9. `phi_proxy_deterministic` — CLAIM-CRS-005 — Tier C (as-found)

```lean
def phi_proxy (nodes edges : Nat) : Nat := nodes + edges
theorem phi_proxy_deterministic (nodes edges : Nat) :
    phi_proxy nodes edges = phi_proxy nodes edges := by rfl
```

- **Goal:** `phi_proxy nodes edges = phi_proxy nodes edges`. Determinism is a
  property **every** Lean function has by construction; this `rfl` is valid for
  *any* function and says nothing specific. **Remediated →**
  [CRS-005](#crs-005-phi_proxy_deterministic).

### 10. `revoked_authorizes_nothing` — `CapabilityRevocation.lean:46` — Tier A

```lean
def authorizes (c : Capability) (r : Right) : Prop := c.revoked = false ∧ holds c r = true
def revoke (c : Capability) : Capability := { c with revoked := true }
theorem revoked_authorizes_nothing (c : Capability) (r : Right) :
    ¬ authorizes (revoke c) r := by intro h; exact Bool.noConfusion h.left
```

- Universally quantified over all `c` and `r`; the proof uses the structure
  (`revoke` sets `revoked := true`, so the first conjunct becomes `true = false`).
  False for a `revoke` that left `revoked` unset → carries real information. Best
  theorem in the repo.

### 11. `fresh_capability_authorizes_held_right` — `CapabilityRevocation.lean:52` — Tier B

- **Goal (unfolded):** `(false = false) ∧ (holds {…} r = true)`. First conjunct
  `rfl`, second is the hypothesis. Near-definitional.

### 12. `revoke_is_idempotent` — `CapabilityRevocation.lean:59` — Tier A

```lean
theorem revoke_is_idempotent (c : Capability) : revoke (revoke c) = revoke c := by cases c; rfl
```

- A genuine algebraic property (idempotence), quantified over all `c`. False for a
  `revoke` that toggled or stacked. The `rfl` is *earned* by the definition, not
  rigged by the statement.

---

## Remediation applied (2026-05-29)

`lean/Refineforge/Consciousness/Claims.lean` was rewritten so the five CRS
theorems are non-vacuous. **All theorem names were kept identical**, so the claim
YAMLs and refinement docs that reference them stay valid. Each property that is
only meaningful relative to a process now models that process explicitly and
ships a companion theorem proving the predicate is falsifiable. Build + gate:
`lake build` → green; `refine lean check-all` → all nine claims `Verified`.

### CRS-001 `workspace_broadcast_complete`

```lean
def run : Nat → BroadcastTrace
  | 0 => { ignition_count := 0, event_count := 0, subscribers_ready := true }
  | k + 1 => { ignition_count := (run k).ignition_count + 1,
               event_count := (run k).event_count + 1,
               subscribers_ready := (run k).subscribers_ready }
theorem workspace_broadcast_complete (n : Nat) : broadcast_complete (run n) := ⟨run_ready n, run_counts n⟩
theorem broadcast_complete_falsifiable :
    ¬ broadcast_complete { ignition_count := 1, event_count := 0, subscribers_ready := true } := ...
```

- **Now Tier A.** Completeness is proved for the trace *produced by running `n`
  ignitions*, by induction — not by rigging the fields. `broadcast_complete` is
  shown falsifiable. Would fail for a `run` that dropped events.

### CRS-002 `workspace_capacity_bound`

```lean
def accept (w : Workspace) : Workspace :=
  if w.content < w.capacity then { w with content := w.content + 1 } else w
theorem workspace_capacity_bound (w : Workspace) (h : within_capacity w) : within_capacity (accept w) := ...
theorem accept_saturates_at_capacity (w : Workspace) (h : w.content = w.capacity) : accept w = w := ...
theorem over_capacity_exists : ¬ within_capacity { content := 3, capacity := 2 } := ...
```

- **Now Tier A.** Proves the *admission operation preserves* the bound (and
  saturates at capacity), instead of returning its own hypothesis. Would fail for
  an `accept` that incremented unconditionally. (`admit` was renamed to `accept`
  to avoid colliding with Lean's `admit` keyword / the no-sorry gate.)

### CRS-003 `narrative_append_only`

```lean
def append (l : Log) (e : String) : Log := { entries := l.entries ++ [e] }
theorem narrative_append_only (l : Log) (e : String) : ∃ rest, (append l e).entries = l.entries ++ rest := ⟨[e], by simp [append]⟩
theorem reset_violates_append_only : ¬ ∃ rest : List String, ([] : List String) = ["a"] ++ rest := ...
```

- **Now Tier A.** Proves *history preservation* — the new log is the old log with
  content appended, so nothing prior is removed or reordered — plus a length
  lemma. A history-dropping "reset" is shown to violate it.

### CRS-004 `ethical_gate_non_bypass`

```lean
def gate (r : Request) : Decision := if r.dangerous then Decision.deny else Decision.allow
def execute (r : Request) : Decision := gate r
theorem ethical_gate_non_bypass (r : Request) : execute r = Decision.allow → gate r = Decision.allow := ...
def rogue_execute (_ : Request) : Decision := Decision.allow
theorem rogue_executor_can_bypass : ∃ r : Request, rogue_execute r = Decision.allow ∧ gate r = Decision.deny := ⟨{ dangerous := true }, rfl, rfl⟩
```

- **Now Tier A.** The honest non-bypass property *was* provable — but only once
  `execute` is modelled as routing through `gate`. `rogue_executor_can_bypass`
  proves a gate-ignoring executor genuinely bypasses the gate, so the routing
  link is load-bearing. This directly answers the as-found §8 finding: the
  diagonal `g = a` collapse is replaced by a real definitional link between two
  functions of the same request.

### CRS-005 `phi_proxy_deterministic`

```lean
theorem phi_proxy_deterministic (nodes edges : Nat) : phi_proxy nodes edges = nodes + edges := rfl
theorem phi_proxy_monotone_in_nodes {n₁ n₂ : Nat} (edges : Nat) (h : n₁ ≤ n₂) : phi_proxy n₁ edges ≤ phi_proxy n₂ edges := ...
theorem phi_proxy_monotone_in_edges (nodes : Nat) {e₁ e₂ : Nat} (h : e₁ ≤ e₂) : phi_proxy nodes e₁ ≤ phi_proxy nodes e₂ := ...
```

- **Now Tier B (named) + Tier A (companions).** The `_deterministic` theorem is a
  *closed-form characterization* (no hidden state) rather than `x = x`; the real
  content is monotonicity in each argument, which a "fake" metric (e.g. one that
  subtracted) would violate.

## Recommendations status

**Cheap and honest:**

1. ~~Add a content grade (A/B/C) to `proof-inventory.md`.~~ **Done** (2026-05-29).
2. Per-claim `content_note:` in each YAML — **partially done**: the YAML
   `description`/`notes` were updated to describe the strengthened theorems. A
   dedicated machine-readable `content_grade:` field is still optional.
3. Rename misleading theorem titles — **N/A**: names were intentionally kept (to
   avoid breaking ~32 referencing files); the *statements* were strengthened to
   match the names instead.

**The real fix (make them Tier A/B):**

- ~~#5 broadcast_complete~~ — **Done** (modelled `run`).
- ~~#6 capacity_bound~~ — **Done** (modelled `accept`; preservation + saturation).
- ~~#7 narrative_append_only~~ — **Done** (history-preservation over a `Log`).
- ~~#8 ethical_gate_non_bypass~~ — **Done** (routed `execute`; rogue-executor counter-model).
- ~~#9 phi_proxy_deterministic~~ — **Done** (closed-form + monotonicity).

## What this audit did NOT verify

- The remediation was verified by `lake build` (green) and `refine lean check-all`
  (all nine `Verified`). It was **not** verified by an independent Lean reviewer.
- It did **not** audit the refinement documents (`docs/refinement/CLAIM-CRS-*.md`),
  the Rust crates, or whether any Rust implementation matches its Lean model. Per
  `docs/methodology.md`, the refinement argument is the trust-critical artifact;
  it is out of scope for this proof-only audit. **The CRS refinement docs predate
  the remediation and should be re-read against the new models.**
- The exported bundles under `artifacts/CLAIM-CRS-*/` still contain the
  pre-remediation `Claims.lean` and are **stale**. Refresh them with
  `refine bundle export <id>`; do not hand-edit sealed bundles.
- It did **not** verify the HELYX source-of-truth at `C:\HELYX`, nor that
  `Helyx/Audit.lean` is a faithful "verbatim slice."
- The Rust workspace's 574 tests were not run or assessed for substance here; this
  audit covers the Lean theorems only.

## How to reproduce

```bash
# List the theorems audited:
grep -rn "^theorem " lean/Refineforge/

# Build the Lean library (confirms green; no sorry/admit/axiom):
(cd lean && lake build)

# Run the project's own gate over every claim:
./target/release/refine lean check-all
# → CLAIM-CRS-001..005, EXAMPLE-001..003, HELYX-AUDIT-001 : Verified  sorries=0 admits=0 axioms=0
```
