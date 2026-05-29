/-
REFINEFORGE-TRUST-002 — The operator-identity gate rejects automated/placeholder names.

DOGFOOD model+refined claim about Refine-Forge's OWN anti-spoofing gate. The Lean
model mirrors the membership check in Rust `is_automated_operator`
(crates/refineforge-cli/src/agent/common.rs) — the function that prevents an AI,
bot, or placeholder from being recorded as a human approver. Concretely, it is
the check that would reject "claude" from signing a human review.

HONEST SCOPE. Lean proves a property of the *model*. The refinement doc
(docs/refinement/REFINEFORGE-TRUST-002.md) bridges it to the Rust and discloses
the tokenisation idealisation: the Rust lowercases the operator and splits it on
non-alphanumeric characters BEFORE this membership check; the Lean takes the
resulting tokens as given. NOT yet human-reviewed → agent trust = model-linked.

Policy: no `sorry`, no `admit`, no axioms beyond Lean core.
-/

namespace Refineforge.OperatorGate

/-- The blocklist of automated/placeholder operator tokens. Mirrors the `blocked`
    array in Rust `is_automated_operator` (the same 15 tokens). -/
def blocked : List String :=
  ["ai", "automated", "automation", "bot", "chatgpt", "claude", "codex",
   "gemini", "gpt", "llm", "none", "null", "placeholder", "tbd", "todo"]

/-- An operator is automated/placeholder iff any of its (already lowercased and
    split) tokens is in the blocklist. Mirrors the final `tokens.any(...)` check
    in `is_automated_operator`; the Rust lowercasing + split-on-non-alphanumeric
    is the idealisation boundary (refinement doc §3). -/
def isAutomated (tokens : List String) : Bool :=
  tokens.any (fun t => blocked.contains t)

/-- T1 (rejection soundness). If any blocked token appears among the operator's
    tokens, the operator is rejected. -/
theorem blocked_token_is_rejected (tokens : List String) (t : String)
    (hmem : t ∈ tokens) (hblocked : blocked.contains t = true) :
    isAutomated tokens = true := by
  unfold isAutomated
  rw [List.any_eq_true]
  exact ⟨t, hmem, hblocked⟩

/-- T2 (acceptance / no over-blocking). If no token is in the blocklist, the
    operator is accepted. Without this, an "always reject" gate would also
    satisfy T1; T2 shows the gate is not vacuously rejecting everyone. -/
theorem clean_operator_is_accepted (tokens : List String)
    (h : ∀ t ∈ tokens, blocked.contains t = false) :
    isAutomated tokens = false := by
  unfold isAutomated
  cases hany : tokens.any (fun t => blocked.contains t) with
  | false => rfl
  | true =>
      rw [List.any_eq_true] at hany
      obtain ⟨t, htmem, htp⟩ := hany
      have hb : blocked.contains t = true := htp
      rw [h t htmem] at hb
      exact Bool.noConfusion hb

/-- T3 (anti-spoofing, concrete). An AI operator name like "claude" is rejected:
    an AI cannot record itself as a human approver. This is the property that
    keeps `review.human_operator` honest. -/
theorem ai_name_is_rejected : isAutomated ["claude"] = true :=
  blocked_token_is_rejected ["claude"] "claude" (by decide) (by decide)

end Refineforge.OperatorGate
