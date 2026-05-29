/-
Refineforge.Consciousness — model-level invariants for the consciousness-rs
integration contract (claims CLAIM-CRS-001 … CLAIM-CRS-005).

HONEST SCOPE. These are theorems about small mathematical *models*. They do
NOT prove the consciousness-rs Rust implementation, phenomenal consciousness,
IIT Phi, or a complete ethics system. See docs/methodology.md and
docs/verification/proof-audit.md.

REMEDIATION (2026-05-29). An earlier version of this file proved theorems that
reduced to `x = x` / `P → P` — see docs/verification/proof-audit.md §Remediation
for the before/after. Every theorem below is now written to be NON-VACUOUS: it
quantifies over the model's objects and would be FALSE under an alternative
definition of the model's own operations. Where a property is only meaningful
relative to a *process* (an ignition that emits, an admission that bounds, a
gate that routes), the process is modelled explicitly and a companion theorem
exhibits the failure case, so the property provably has teeth.

Policy: no `sorry`, no `admit`, no axioms beyond Lean core.
-/

namespace Refineforge.Consciousness

/-! ## CLAIM-CRS-001 — Workspace broadcast completeness

A global-workspace "ignition" emits exactly one broadcast event. We model the
process that runs `n` ignitions (`run`) and prove the emitted trace is complete
for every `n`. Completeness is a real predicate: `broadcast_complete_falsifiable`
exhibits a trace that fails it. -/

structure BroadcastTrace where
  ignition_count : Nat
  event_count : Nat
  subscribers_ready : Bool
  deriving Repr, DecidableEq

def broadcast_complete (t : BroadcastTrace) : Prop :=
  t.subscribers_ready = true ∧ t.event_count = t.ignition_count

/-- Run `n` ignitions; each ignition emits exactly one broadcast event. -/
def run : Nat → BroadcastTrace
  | 0 => { ignition_count := 0, event_count := 0, subscribers_ready := true }
  | k + 1 =>
      { ignition_count := (run k).ignition_count + 1,
        event_count := (run k).event_count + 1,
        subscribers_ready := (run k).subscribers_ready }

theorem run_ready (n : Nat) : (run n).subscribers_ready = true := by
  induction n with
  | zero => rfl
  | succ k ih => simp only [run]; exact ih

theorem run_counts (n : Nat) : (run n).event_count = (run n).ignition_count := by
  induction n with
  | zero => rfl
  | succ k ih => simp only [run]; omega

/-- CRS-001. After running any number of ignitions, the broadcast trace is
    complete: subscribers are ready and the event count matches the ignition
    count. False for an `run` that dropped events — so it is not vacuous. -/
theorem workspace_broadcast_complete (n : Nat) : broadcast_complete (run n) :=
  ⟨run_ready n, run_counts n⟩

/-- Completeness is a genuine (falsifiable) predicate: a trace with one ignition
    and no event is not complete. -/
theorem broadcast_complete_falsifiable :
    ¬ broadcast_complete { ignition_count := 1, event_count := 0, subscribers_ready := true } := by
  intro h
  cases h with
  | intro _ hcount => exact absurd hcount (by decide)

/-! ## CLAIM-CRS-002 — Workspace capacity bound

Capacity is enforced by the `accept` operation, which adds content only when
strictly under capacity. We prove `accept` PRESERVES the bound (never overflows)
and SATURATES at capacity (the bound is load-bearing, not decorative). -/

structure Workspace where
  content : Nat
  capacity : Nat
  deriving Repr, DecidableEq

def within_capacity (w : Workspace) : Prop := w.content ≤ w.capacity

/-- Accept one content item iff strictly under capacity; otherwise a no-op. -/
def accept (w : Workspace) : Workspace :=
  if w.content < w.capacity then { w with content := w.content + 1 } else w

/-- CRS-002. `accept` preserves the capacity bound: from a within-capacity
    workspace it never produces an over-capacity one. False for an `accept`
    that incremented unconditionally — so it is not vacuous. -/
theorem workspace_capacity_bound (w : Workspace) (h : within_capacity w) :
    within_capacity (accept w) := by
  unfold within_capacity at h ⊢
  unfold accept
  split
  · show w.content + 1 ≤ w.capacity
    omega
  · exact h

/-- The bound is load-bearing: at capacity, `accept` is a no-op — it does not
    overflow by incrementing past the bound. -/
theorem accept_saturates_at_capacity (w : Workspace) (h : w.content = w.capacity) :
    accept w = w := by
  unfold accept
  split
  · exfalso; omega
  · rfl

/-- Over-capacity states exist, so `within_capacity` is a genuine predicate. -/
theorem over_capacity_exists : ¬ within_capacity { content := 3, capacity := 2 } := by
  unfold within_capacity
  decide

/-! ## CLAIM-CRS-003 — Narrative log is append-only

Narrative identity is an append-only log. `append` adds one entry. We prove it
increments length by one AND preserves all prior history (the old log is a
prefix of the new one), so no entry is ever mutated or dropped. -/

structure Log where
  entries : List String
  deriving Repr

def append (l : Log) (e : String) : Log := { entries := l.entries ++ [e] }

theorem narrative_append_increments_length (l : Log) (e : String) :
    (append l e).entries.length = l.entries.length + 1 := by
  simp [append]

/-- CRS-003. Append is history-preserving: the new log is the old log with new
    content appended — nothing prior is removed or reordered. -/
theorem narrative_append_only (l : Log) (e : String) :
    ∃ rest, (append l e).entries = l.entries ++ rest :=
  ⟨[e], by simp [append]⟩

/-- History-preservation is a real constraint: emptying a non-empty log cannot
    be written as an append (no `rest` makes `[] = ["a"] ++ rest`). So a "reset"
    that drops history provably violates append-only. -/
theorem reset_violates_append_only :
    ¬ ∃ rest : List String, ([] : List String) = ["a"] ++ rest := by
  intro h
  cases h with
  | intro _ hr => simp at hr

/-! ## CLAIM-CRS-004 — Ethical gate cannot be bypassed

The gate decides allow/deny per request. The compliant executor is REQUIRED to
route its decision through the gate (`execute := gate`). Non-bypass then holds:
any action executed as `allow` was allowed by the gate. The companion theorem
shows a gate-ignoring "rogue" executor PROVABLY violates non-bypass — so the
routing link is load-bearing, not decorative. -/

inductive Decision where
  | allow
  | deny
  deriving DecidableEq, Repr

structure Request where
  dangerous : Bool
  deriving Repr, DecidableEq

/-- The gate denies dangerous requests. -/
def gate (r : Request) : Decision :=
  if r.dangerous then Decision.deny else Decision.allow

/-- The compliant executor routes its decision through the gate. -/
def execute (r : Request) : Decision := gate r

/-- CRS-004. Non-bypass: if the executed action is `allow`, the gate allowed it.
    Quantified over an arbitrary request; the two sides go through two different
    functions. False for an executor that ignores the gate — see
    `rogue_executor_can_bypass`. -/
theorem ethical_gate_non_bypass (r : Request) :
    execute r = Decision.allow → gate r = Decision.allow := by
  intro h
  unfold execute at h
  exact h

/-- A gate-ignoring executor that always allows. -/
def rogue_execute (_ : Request) : Decision := Decision.allow

/-- The non-bypass property has teeth: a rogue executor that ignores the gate
    allows a dangerous request the gate denies. -/
theorem rogue_executor_can_bypass :
    ∃ r : Request, rogue_execute r = Decision.allow ∧ gate r = Decision.deny :=
  ⟨{ dangerous := true }, rfl, rfl⟩

/-! ## CLAIM-CRS-005 — Phi-proxy is a deterministic, monotone metric

`phi_proxy` is a pure closed-form function of its inputs. Determinism of a pure
function is automatic, so we record the *closed form* (no hidden state) rather
than asserting `x = x`. The genuine content is the two monotonicity theorems:
the metric is order-preserving in each argument. This is NOT IIT Phi. -/

def phi_proxy (nodes edges : Nat) : Nat := nodes + edges

/-- CRS-005. Closed-form characterization: `phi_proxy` is exactly `nodes + edges`,
    with no hidden state or randomness. (Determinism of a pure function is
    automatic; this records the closed form rather than asserting `x = x`.) -/
theorem phi_proxy_deterministic (nodes edges : Nat) :
    phi_proxy nodes edges = nodes + edges := rfl

/-- The metric is monotone in the node count. False for a metric that subtracted
    nodes — so it is not vacuous. -/
theorem phi_proxy_monotone_in_nodes {n₁ n₂ : Nat} (edges : Nat) (h : n₁ ≤ n₂) :
    phi_proxy n₁ edges ≤ phi_proxy n₂ edges := by
  unfold phi_proxy; omega

/-- The metric is monotone in the edge count. -/
theorem phi_proxy_monotone_in_edges (nodes : Nat) {e₁ e₂ : Nat} (h : e₁ ≤ e₂) :
    phi_proxy nodes e₁ ≤ phi_proxy nodes e₂ := by
  unfold phi_proxy; omega

end Refineforge.Consciousness
