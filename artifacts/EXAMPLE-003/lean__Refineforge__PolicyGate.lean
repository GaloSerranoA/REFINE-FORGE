/-
REFINEFORGE-TRUST-007 — The no-sorry policy gate rejects any forbidden token under policy.

DOGFOOD model+refined claim about the FOUNDATIONAL gate of the whole framework:
the no-sorry policy gate that gives every `status: proven` claim its meaning. The
Lean model mirrors the `ok` computation in `sorry_gate::check`
(crates/refineforge-cli/src/sorry_gate.rs): the gate accepts iff NO enabled
policy flag has a positive forbidden-token count.

HONEST SCOPE. Lean proves the gate's boolean DECISION logic, taking the
post-comment-strip occurrence counts as given. The regex matching
(`\bsorry\b`, `\badmit\b`, `^\s*axiom\b`) and `strip_comments` that PRODUCE those
counts are the idealisation boundary — not modelled (refinement doc §5). So the
model proves "given correct counts, the verdict is correct", not that the lexer
counts correctly. NOT yet human-reviewed → agent trust = model-linked.

Policy: no `sorry`, no `admit`, no axioms beyond Lean core.
-/

namespace Refineforge.PolicyGate

/-- The three policy flags from a claim's `policy:` block (Rust `Policy`). -/
structure Policy where
  noSorry : Bool
  noAdmit : Bool
  noAxioms : Bool

/-- Forbidden-token occurrence counts after comment-stripping (the Rust
    `GateResult` counts). -/
structure Counts where
  sorryCount : Nat
  admitCount : Nat
  axiomCount : Nat

/-- The gate verdict: accept (`true`) iff no enabled policy flag has a positive
    count. Mirrors the `ok` computation in `sorry_gate::check`:
    `ok = !(no_sorry && sorry>0) && !(no_admit && admit>0) && !(no_axioms && axiom>0)`. -/
def ok (p : Policy) (c : Counts) : Bool :=
  !(p.noSorry && c.sorryCount != 0)
    && !(p.noAdmit && c.admitCount != 0)
    && !(p.noAxioms && c.axiomCount != 0)

/-- T1. A present `sorry` under an enabled `no_sorry` policy is rejected. -/
theorem present_sorry_rejected (p : Policy) (c : Counts)
    (hpol : p.noSorry = true) (hcnt : (c.sorryCount != 0) = true) : ok p c = false := by
  unfold ok; simp [hpol, hcnt]

/-- T2. A present `admit` under an enabled `no_admit` policy is rejected. -/
theorem present_admit_rejected (p : Policy) (c : Counts)
    (hpol : p.noAdmit = true) (hcnt : (c.admitCount != 0) = true) : ok p c = false := by
  unfold ok; simp [hpol, hcnt]

/-- T3. A present `axiom` declaration under an enabled `no_axioms_beyond_lean_core`
    policy is rejected. -/
theorem present_axiom_rejected (p : Policy) (c : Counts)
    (hpol : p.noAxioms = true) (hcnt : (c.axiomCount != 0) = true) : ok p c = false := by
  unfold ok; simp [hpol, hcnt]

/-- T4 (acceptance). A source with zero forbidden tokens passes the gate under
    ANY policy — so the gate is not vacuously rejecting. -/
theorem clean_source_accepted (p : Policy) (c : Counts)
    (hs : c.sorryCount = 0) (ha : c.admitCount = 0) (hx : c.axiomCount = 0) :
    ok p c = true := by
  unfold ok; simp [hs, ha, hx]

/-- T5 (concrete). Under the default policy (all three flags on), a single
    `sorry` is rejected — the headline guarantee of `status: proven`. -/
theorem default_policy_rejects_one_sorry :
    ok { noSorry := true, noAdmit := true, noAxioms := true }
       { sorryCount := 1, admitCount := 0, axiomCount := 0 } = false := by decide

end Refineforge.PolicyGate
