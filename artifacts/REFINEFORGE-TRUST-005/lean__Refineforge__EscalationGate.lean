/-
REFINEFORGE-TRUST-006 — The escalation engine never auto-proceeds on trust-critical actions.

DOGFOOD model+refined claim about Refine-Forge's OWN autonomous-driver escalation
engine. The Lean model mirrors `Engine::decide` in
crates/refineforge-escalation/src/engine.rs: it collects per-category classifier
"hits" and returns `Proceed` ONLY when `hits.is_empty()`, else `Escalate`. Three
trust-critical action shapes always fire a classifier, so they can never
auto-proceed:

  - `WriteAxiom`  → `classify_custom_axiom` fires for ANY axiom.
  - `SetReviewOperator { from: None }` → `classify_status_upgrade` fires
    (flipping `review.human_operator` from null always escalates).
  - `Unknown`     → `classify_scope` fires (never silently auto-proceed).

T2 is the driver-level guard that keeps human review human: the autonomous
driver can NEVER auto-set `review.human_operator`; it must escalate to a person.
This complements REFINEFORGE-TRUST-002/003 (the operator + approval gates).

HONEST SCOPE. Lean proves a property of the *model*. The refinement doc
(docs/refinement/REFINEFORGE-TRUST-006.md) discloses the idealisation: we model
four action shapes (three always-fire + `benign`) and abstract the 9 classifiers
as `anyClassifierFires`; the criteria-version check and multi-category primary
selection are out of scope. NOT yet human-reviewed → agent trust = model-linked.

Policy: no `sorry`, no `admit`, no axioms beyond Lean core.
-/

namespace Refineforge.EscalationGate

inductive Decision where
  | proceed
  | escalate
  deriving DecidableEq, Repr

/-- Safety-critical proposed-action shapes — a faithful slice of the 30+ Rust
    `Action` variants. `benign` lumps the actions no classifier fires on
    (e.g. `Reformat`, `RenameLocalVar`, `AddCliHelpText`). -/
inductive Action where
  | writeAxiom
  | setReviewOperatorFromNull
  | unknown
  | benign
  deriving DecidableEq, Repr

/-- Does ANY of the 9 category classifiers fire? Mirrors the engine's `hits`
    collection (it proceeds only if `hits.is_empty()`). The three trust-critical
    shapes always fire a classifier; `benign` fires none. -/
def anyClassifierFires : Action → Bool
  | .writeAxiom => true
  | .setReviewOperatorFromNull => true
  | .unknown => true
  | .benign => false

/-- The engine decision: escalate iff a category fired, else proceed. Mirrors
    `if hits.is_empty() { Proceed } else { Escalate(..) }` in `Engine::decide`. -/
def decide (a : Action) : Decision :=
  match anyClassifierFires a with
  | true => Decision.escalate
  | false => Decision.proceed

/-- T1 (no silent custom axioms). The engine never auto-proceeds on a custom
    axiom declaration; it always escalates. -/
theorem axiom_always_escalates : decide Action.writeAxiom = Decision.escalate := by decide

/-- T2 (no silent operator set). The autonomous driver can never auto-set
    `review.human_operator` from null — it always escalates to the human. The
    driver-level guard that keeps human review human. -/
theorem set_operator_always_escalates :
    decide Action.setReviewOperatorFromNull = Decision.escalate := by decide

/-- T3 (no silent unknowns). Unrecognised action shapes always escalate; the
    engine never silently auto-proceeds on something it does not understand. -/
theorem unknown_always_escalates : decide Action.unknown = Decision.escalate := by decide

/-- T4 (proceed requires all-silent). The engine returns `Proceed` ONLY when no
    category fired — proceeding is never the default. -/
theorem proceed_implies_all_silent (a : Action) (h : decide a = Decision.proceed) :
    anyClassifierFires a = false := by
  cases a with
  | benign => rfl
  | writeAxiom => simp [decide, anyClassifierFires] at h
  | setReviewOperatorFromNull => simp [decide, anyClassifierFires] at h
  | unknown => simp [decide, anyClassifierFires] at h

end Refineforge.EscalationGate
