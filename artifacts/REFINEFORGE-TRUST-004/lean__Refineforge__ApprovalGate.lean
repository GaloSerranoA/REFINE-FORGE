/-
REFINEFORGE-TRUST-003 — The human-approval gate cannot accept an automated operator.

DOGFOOD model+refined claim about Refine-Forge's OWN human-approval acceptance
gate. The Lean model mirrors the final acceptance conjunction in Rust
`validate_human_approval` (crates/refineforge-cli/src/agent/common.rs):

    validation.passed = schema_ok && role_ok && decision_ok
                        && approved_at_ok && summary_ok && human_ok;
    // human_ok = !operator.is_empty() && !is_automated_operator(operator)

It COMPOSES with REFINEFORGE-TRUST-002: `humanOk` reuses
`OperatorGate.isAutomated`, so the theorem "an automated operator name forces
rejection" propagates the anti-spoofing guarantee from the operator gate to the
whole approval decision. This is the formal statement of "human review cannot be
recorded by an AI/bot/placeholder".

HONEST SCOPE. Lean proves a property of the *model*. The refinement doc
(docs/refinement/REFINEFORGE-TRUST-003.md) bridges it to the Rust and discloses
the idealisations: the five non-operator checks are modelled as opaque Bools
(the Rust computes them from JSON field comparisons), and `humanOk` shares
TRUST-002's tokenisation boundary. NOT yet human-reviewed → agent trust =
model-linked.

Policy: no `sorry`, no `admit`, no axioms beyond Lean core.
-/

import Refineforge.OperatorGate

namespace Refineforge.ApprovalGate

open Refineforge.OperatorGate (isAutomated)

/-- The human-operator check from Rust `validate_human_approval`:
    a non-empty operator that is NOT automated. `operatorNonEmpty` models
    `!operator.is_empty()`; `isAutomated` is the TRUST-002 model of
    `is_automated_operator`. -/
def humanOk (operatorNonEmpty : Bool) (operatorTokens : List String) : Bool :=
  operatorNonEmpty && !isAutomated operatorTokens

/-- The approval acceptance gate: the conjunction of all six checks. Mirrors
    `validation.passed = schema_ok && role_ok && decision_ok && approved_at_ok
    && summary_ok && human_ok`. The five non-operator flags are opaque Bools
    (idealisation boundary — see refinement doc §3). -/
def accepts (schemaOk roleOk decisionOk approvedAtOk summaryOk : Bool)
    (operatorNonEmpty : Bool) (operatorTokens : List String) : Bool :=
  schemaOk && roleOk && decisionOk && approvedAtOk && summaryOk
    && humanOk operatorNonEmpty operatorTokens

/-- T1 (acceptance soundness). A passing approval has a real human operator
    (non-empty and non-automated). -/
theorem accepts_implies_human
    (schemaOk roleOk decisionOk approvedAtOk summaryOk operatorNonEmpty : Bool)
    (tokens : List String)
    (h : accepts schemaOk roleOk decisionOk approvedAtOk summaryOk operatorNonEmpty tokens = true) :
    humanOk operatorNonEmpty tokens = true := by
  unfold accepts at h
  simp only [Bool.and_eq_true] at h
  exact h.2

/-- T2 (anti-spoofing composition). If the operator name is automated
    (`OperatorGate.isAutomated`), the approval is rejected regardless of the
    other checks. An AI/bot/placeholder operator can never produce an accepted
    human approval. -/
theorem automated_operator_rejected
    (schemaOk roleOk decisionOk approvedAtOk summaryOk operatorNonEmpty : Bool)
    (tokens : List String) (hauto : isAutomated tokens = true) :
    accepts schemaOk roleOk decisionOk approvedAtOk summaryOk operatorNonEmpty tokens = false := by
  unfold accepts humanOk
  simp [hauto]

/-- T3 (completeness). If all six checks pass, the approval is accepted. -/
theorem all_checks_accept
    (schemaOk roleOk decisionOk approvedAtOk summaryOk operatorNonEmpty : Bool)
    (tokens : List String)
    (hs : schemaOk = true) (hr : roleOk = true) (hd : decisionOk = true)
    (ha : approvedAtOk = true) (hsum : summaryOk = true)
    (hhuman : humanOk operatorNonEmpty tokens = true) :
    accepts schemaOk roleOk decisionOk approvedAtOk summaryOk operatorNonEmpty tokens = true := by
  unfold accepts
  simp [hs, hr, hd, ha, hsum, hhuman]

/-- T4 (concrete anti-spoofing). Even with every other check satisfied, an
    approval naming "claude" as the operator is rejected. -/
theorem claude_cannot_approve :
    accepts true true true true true true ["claude"] = false :=
  automated_operator_rejected true true true true true true ["claude"]
    Refineforge.OperatorGate.ai_name_is_rejected

end Refineforge.ApprovalGate
